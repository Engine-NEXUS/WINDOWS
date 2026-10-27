//! Device credentials for the Cloudflare Worker's authenticated routes.
//!
//! The Worker no longer trusts a caller-supplied `user_id` on `/oauth/*` and `/apikeys/*`
//! (server/worker/src/route_auth.ts). Every request must carry
//!
//! ```text
//! Authorization:      Bearer <device token>     (OS keyring, service "device_token")
//! X-Nexus-Profile-Id: <profile id>              (nexus-config.json "profileId")
//! X-Nexus-Device-Id:  <device id>               (the id the WORKER issued at claim time)
//! ```
//!
//! and the Worker derives the owner from the profile, so clients stop sending `user_id`.
//!
//! The device token never reaches the web view: the frontend asks Rust to perform the
//! request through [`worker_request`], which only talks to the configured Worker and only
//! under `/oauth/`.

use crate::identity_state::{
    self, IdentityConfig, DEVICE_TOKEN_SERVICE, IDENTITY_CANONICAL,
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

pub const PROFILE_HEADER: &str = "x-nexus-profile-id";
pub const DEVICE_HEADER: &str = "x-nexus-device-id";

/// Deliberately no `Debug`/`Serialize`: the token must never end up in a log line.
#[derive(Clone)]
pub struct WorkerCredentials {
    profile_id: String,
    device_id: String,
    token: String,
}

impl WorkerCredentials {
    /// The three request headers, or `None` if a value cannot be a header.
    pub fn headers(&self) -> Option<HeaderMap> {
        build_headers(&self.profile_id, &self.device_id, &self.token)
    }
}

/// Pure header builder.
pub fn build_headers(profile_id: &str, device_id: &str, token: &str) -> Option<HeaderMap> {
    if profile_id.is_empty() || device_id.is_empty() || token.is_empty() {
        return None;
    }
    let mut map = HeaderMap::new();
    let mut auth = HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    auth.set_sensitive(true); // keeps it out of reqwest/hyper debug output
    map.insert(AUTHORIZATION, auth);
    map.insert(HeaderName::from_static(PROFILE_HEADER), HeaderValue::from_str(profile_id).ok()?);
    map.insert(HeaderName::from_static(DEVICE_HEADER), HeaderValue::from_str(device_id).ok()?);
    Some(map)
}

/// Pure: decide whether this device can authenticate to the Worker at all.
/// Provisional (never claimed) devices have no profile and no Worker-issued device id.
pub fn credentials_from(
    cfg: &IdentityConfig,
    device_id: &str,
    token: Option<String>,
) -> Option<WorkerCredentials> {
    if cfg.identity != IDENTITY_CANONICAL || cfg.profile_id.is_empty() {
        return None;
    }
    let token = token.filter(|t| !t.is_empty())?;
    if device_id.is_empty() {
        return None;
    }
    Some(WorkerCredentials {
        profile_id: cfg.profile_id.clone(),
        device_id: device_id.to_string(),
        token,
    })
}

/// Load this device's credentials from the config directory and the OS keyring.
pub fn load_credentials(dir: &Path) -> Option<WorkerCredentials> {
    let cfg = identity_state::read_identity_config(dir);
    let device_id = identity_state::network_device_id(dir);
    let token = crate::auth_vault::get_api_key(DEVICE_TOKEN_SERVICE);
    credentials_from(&cfg, &device_id, token)
}

/// The app data dir without an `AppHandle`, derived from the environment like
/// `nlu_update::downloaded_model_dir_envless` (used by callers that have no handle).
pub fn app_data_dir_envless() -> Option<PathBuf> {
    crate::app_dirs::nexus_data_dir()
}

/// Credentials without an `AppHandle`.
pub fn credentials_envless() -> Option<WorkerCredentials> {
    load_credentials(&app_data_dir_envless()?)
}

/// What to tell the user when the Worker is not reachable with credentials.
pub const NOT_LINKED: &str =
    "This device isn't linked to the NEXUS cloud service yet. Ask the NEXUS admin to approve it, then try again.";

/// GET an authenticated Worker URL (no `AppHandle` needed). Certificate validation stays on.
pub async fn authed_get(url: &str, timeout: Duration) -> Result<reqwest::Response, String> {
    let headers = credentials_envless()
        .and_then(|c| c.headers())
        .ok_or_else(|| NOT_LINKED.to_string())?;
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    client
        .get(url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| format!("worker request: {e}"))
}

/// Turn a 401/403 from the Worker into a sentence a person can act on.
pub fn describe_denial(status: u16, body: &str) -> String {
    let code = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"].as_str().map(str::to_owned))
        .unwrap_or_default();
    match (status, code.as_str()) {
        (401, "auth_required") => NOT_LINKED.to_string(),
        (401, _) => "The NEXUS cloud service doesn't recognise this device. Reconnect it from the Command Hub."
            .to_string(),
        (403, "forbidden_user") => "That account doesn't belong to this device.".to_string(),
        (403, "pending") => "This device is still waiting for admin approval.".to_string(),
        (403, "suspended") | (403, "revoked") => {
            "This device's access to the NEXUS cloud service has been turned off.".to_string()
        }
        (403, "expired") => "This device's access to the NEXUS cloud service has expired.".to_string(),
        (403, "not_entitled") => "This device isn't allowed to use the NEXUS cloud service.".to_string(),
        (503, _) => "The NEXUS cloud service is temporarily unavailable.".to_string(),
        _ => format!("The NEXUS cloud service refused the request ({status})."),
    }
}

// ---- Frontend bridge ----

/// Only these Worker paths may be reached from the web view. The SMS gateway
/// and signaling upgrade have separate, non-WebView authentication paths.
const LINK_ROUTES: &[&str] = &[
    "/v1/links/phone", "/v1/links/phone/start", "/v1/links/phone/confirm",
    "/v1/links/request", "/v1/links/pending", "/v1/links/accept",
    "/v1/links/reject",
    "/v1/links/list", "/v1/links/revoke",
];

/// Validate and normalise a path the web view asks for. Strips `user_id` (the Worker
/// derives the owner from the device credentials) and refuses anything that could
/// escape `/oauth/` or point at another host.
pub fn sanitize_proxy_path(path: &str) -> Result<String, String> {
    if path.contains("..")
        || path.contains("//")
        || path.contains('\\')
        || path.contains('#')
        || path.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("invalid worker path".into());
    }
    let (route, query) = match path.split_once('?') {
        Some((r, q)) => (r, Some(q)),
        None => (path, None),
    };
    if !((route.starts_with("/oauth/") && route != "/oauth/callback")
        || LINK_ROUTES.contains(&route))
    {
        return Err("worker_request only reaches approved OAuth and Device Links routes".into());
    }
    let kept: Vec<&str> = query
        .map(|q| {
            q.split('&')
                .filter(|kv| !kv.is_empty() && kv.split('=').next() != Some("user_id"))
                .collect()
        })
        .unwrap_or_default();
    Ok(if kept.is_empty() { route.to_string() } else { format!("{route}?{}", kept.join("&")) })
}

#[derive(Serialize)]
pub struct WorkerResponse {
    pub status: u16,
    pub body: serde_json::Value,
    /// Set for 401/403: a plain-language reason to show or speak.
    pub message: Option<String>,
}

fn configured_server_url(dir: &Path) -> Result<String, String> {
    let content = std::fs::read_to_string(dir.join("nexus-config.json")).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    crate::network::normalize_worker_url(json["serverUrl"].as_str().unwrap_or(""))
}

/// IPC: perform an authenticated request to the Worker's `/oauth/*` routes on behalf of
/// the web view. The device token stays in Rust.
#[tauri::command]
pub async fn worker_request<R: Runtime>(
    app: AppHandle<R>,
    method: String,
    path: String,
    body: Option<serde_json::Value>,
) -> Result<WorkerResponse, String> {
    let path = sanitize_proxy_path(&path)?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let server = configured_server_url(&dir)?;
    let Some(headers) = load_credentials(&dir).and_then(|c| c.headers()) else {
        return Ok(WorkerResponse {
            status: 401,
            body: serde_json::json!({ "error": "auth_required" }),
            message: Some(NOT_LINKED.to_string()),
        });
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let url = format!("{}{}", server.trim_end_matches('/'), path);
    let mut req = match method.to_ascii_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "DELETE" => client.delete(&url),
        other => return Err(format!("unsupported method {other}")),
    }
    .headers(headers);
    if let Some(mut b) = body {
        if let Some(obj) = b.as_object_mut() {
            obj.remove("user_id");
        }
        req = req.json(&b);
    }

    let resp = req.send().await.map_err(|e| format!("worker request: {e}"))?;
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let message = (status == 401 || status == 403 || status == 503).then(|| describe_denial(status, &text));
    let body = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    Ok(WorkerResponse { status, body, message })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canonical() -> IdentityConfig {
        IdentityConfig {
            identity: IDENTITY_CANONICAL.into(),
            profile_id: "prof_A".into(),
            ..IdentityConfig::default()
        }
    }

    #[test]
    fn headers_carry_bearer_profile_and_device() {
        let h = build_headers("prof_A", "dev_B", "tok123").unwrap();
        assert_eq!(h.get(AUTHORIZATION).unwrap().to_str().unwrap(), "Bearer tok123");
        assert_eq!(h.get(PROFILE_HEADER).unwrap().to_str().unwrap(), "prof_A");
        assert_eq!(h.get(DEVICE_HEADER).unwrap().to_str().unwrap(), "dev_B");
        assert!(h.get(AUTHORIZATION).unwrap().is_sensitive());
    }

    #[test]
    fn headers_refuse_missing_or_invalid_values() {
        assert!(build_headers("", "dev", "tok").is_none());
        assert!(build_headers("prof", "", "tok").is_none());
        assert!(build_headers("prof", "dev", "").is_none());
        assert!(build_headers("prof", "dev", "bad\ntoken").is_none());
    }

    #[test]
    fn provisional_or_tokenless_devices_have_no_credentials() {
        assert!(credentials_from(&IdentityConfig::default(), "dev", Some("t".into())).is_none());
        assert!(credentials_from(&canonical(), "dev", None).is_none());
        assert!(credentials_from(&canonical(), "dev", Some(String::new())).is_none());
        assert!(credentials_from(&canonical(), "", Some("t".into())).is_none());
        assert!(credentials_from(&canonical(), "dev", Some("t".into())).is_some());
    }

    #[test]
    fn proxy_path_is_confined_to_oauth_and_drops_user_id() {
        assert_eq!(sanitize_proxy_path("/oauth/status").unwrap(), "/oauth/status");
        assert_eq!(
            sanitize_proxy_path("/oauth/auth-url?provider=google&user_id=victim&code_challenge=abc").unwrap(),
            "/oauth/auth-url?provider=google&code_challenge=abc"
        );
        assert_eq!(sanitize_proxy_path("/oauth/status?user_id=x").unwrap(), "/oauth/status");
    }

    #[test]
    fn proxy_path_rejects_escapes() {
        for bad in [
            "/v1/admin/profiles/list",
            "/apikeys/list",
            "https://evil.example/oauth/status",
            "//evil.example/oauth/x",
            "/oauth/../v1/admin/profiles/list",
            "/oauth/callback?code=1",
            "/v1/links/sms/next",
            "/v1/links/signal/abc",
            "/v1/links/phone/start/extra",
            "/oauth/status#x",
            "/oauth/sta tus",
            "/oauth/status\r\nX: y",
            "oauth/status",
            "",
        ] {
            assert!(sanitize_proxy_path(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn proxy_allows_only_device_link_control_routes() {
        for path in LINK_ROUTES {
            assert_eq!(sanitize_proxy_path(path).unwrap(), *path);
        }
    }

    #[test]
    fn denials_become_plain_sentences() {
        assert_eq!(describe_denial(401, r#"{"error":"auth_required"}"#), NOT_LINKED);
        assert!(describe_denial(401, r#"{"error":"bad_token"}"#).contains("doesn't recognise"));
        assert!(describe_denial(403, r#"{"error":"forbidden_user"}"#).contains("doesn't belong"));
        assert!(describe_denial(403, r#"{"error":"pending"}"#).contains("approval"));
        assert!(describe_denial(403, r#"{"error":"revoked"}"#).contains("turned off"));
        assert!(describe_denial(418, "not json").contains("418"));
    }
}
