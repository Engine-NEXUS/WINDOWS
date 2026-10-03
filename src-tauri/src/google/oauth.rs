//! Direct RFC 8252 Native Loopback OAuth for Google Accounts.
//!
//! Handles PKCE flow, loopback TCP listener on `127.0.0.1:49152`, token exchange,
//! profile retrieval, and automatic token refreshes via refresh_token.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::types::GoogleAccountProfile;

pub const DEFAULT_CLIENT_ID: &str =
    "1065171708892-v7k7b2f6k526b74499n8k00000000000.apps.googleusercontent.com";
pub const LOOPBACK_PORT: u16 = 49152;
pub const REDIRECT_URI: &str = "http://127.0.0.1:49152/callback";

/// Retrieve Developer Client ID and Client Secret (custom or fallback).
pub fn get_client_credentials() -> (String, Option<String>) {
    let client_id = crate::auth_vault::get_api_key("google_client_id")
        .or_else(|| std::env::var("GOOGLE_CLIENT_ID").ok())
        .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());

    let client_secret = crate::auth_vault::get_api_key("google_client_secret")
        .or_else(|| std::env::var("GOOGLE_CLIENT_SECRET").ok());

    (client_id, client_secret)
}

fn get_random_bytes(dest: &mut [u8]) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut rng = seed;
    for (i, byte) in dest.iter_mut().enumerate() {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1);
        *byte = ((rng >> 32) ^ (i as u128)) as u8;
    }
}

/// Generate PKCE verifier (43 chars) and challenge (S256).
pub fn generate_pkce() -> (String, String) {
    let mut random_bytes = [0u8; 32];
    get_random_bytes(&mut random_bytes);
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes);

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(hash);

    (verifier, challenge)
}

/// Start direct native OAuth flow via Tokio TCP listener on 127.0.0.1:49152.
pub async fn start_loopback_oauth_flow() -> Result<GoogleAccountProfile, String> {
    let (client_id, client_secret) = get_client_credentials();
    let (verifier, challenge) = generate_pkce();

    let scopes = [
        "https://www.googleapis.com/auth/gmail.readonly",
        "https://www.googleapis.com/auth/gmail.send",
        "https://www.googleapis.com/auth/gmail.modify",
        "https://www.googleapis.com/auth/calendar",
        "https://www.googleapis.com/auth/photoslibrary.readonly",
        "https://www.googleapis.com/auth/userinfo.profile",
        "https://www.googleapis.com/auth/userinfo.email",
    ]
    .join(" ");

    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent",
        urlencoding::encode(&client_id),
        urlencoding::encode(REDIRECT_URI),
        urlencoding::encode(&scopes),
        urlencoding::encode(&challenge)
    );

    // Bind loopback TCP listener
    let addr = format!("127.0.0.1:{LOOPBACK_PORT}");
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind OAuth loopback listener on {addr}: {e}"))?;

    tracing::info!("OAuth loopback listener active on {addr}, opening browser...");
    let _ = open::that(&auth_url);

    // Wait up to 120s for OAuth authorization callback
    let mut code: Option<String> = None;
    let timeout = tokio::time::sleep(Duration::from_secs(120));
    tokio::pin!(timeout);

    tokio::select! {
        res = listener.accept() => {
            match res {
                Ok((mut stream, _)) => {
                    let mut buf = [0u8; 2048];
                    let read_bytes = stream.read(&mut buf).await.unwrap_or(0);
                    let req_str = String::from_utf8_lossy(&buf[..read_bytes]);

                    // Parse request line e.g. GET /callback?code=4/0A... HTTP/1.1
                    if let Some(first_line) = req_str.lines().next() {
                        if let Some(path) = first_line.split_whitespace().nth(1) {
                            if let Some(query) = path.split_once('?') {
                                for pair in query.1.split('&') {
                                    if let Some((k, v)) = pair.split_once('=') {
                                        if k == "code" {
                                            code = Some(urlencoding::decode(v).unwrap_or_default().to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Respond to browser with a clean success page
                    let html = "<html><body style='font-family:sans-serif;text-align:center;padding-top:50px;'><h2>Authentication successful!</h2><p>You can close this tab and return to NEXUS.</p></body></html>";
                    let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", html.len(), html);
                    let _ = stream.write_all(resp.as_bytes()).await;
                }
                Err(e) => return Err(format!("TCP accept error: {e}")),
            }
        }
        _ = &mut timeout => {
            return Err("OAuth login timed out after 120 seconds.".to_string());
        }
    }

    let auth_code = code.ok_or_else(|| "No authorization code received in callback".to_string())?;

    // Exchange auth code for tokens
    exchange_code_and_save(&client_id, client_secret.as_deref(), &auth_code, &verifier).await
}

/// Exchange OAuth authorization code for tokens and store account profile.
async fn exchange_code_and_save(
    client_id: &str,
    client_secret: Option<&str>,
    code: &str,
    verifier: &str,
) -> Result<GoogleAccountProfile, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let mut params = std::collections::HashMap::new();
    params.insert("client_id", client_id);
    if let Some(sec) = client_secret {
        params.insert("client_secret", sec);
    }
    params.insert("code", code);
    params.insert("code_verifier", verifier);
    params.insert("grant_type", "authorization_code");
    params.insert("redirect_uri", REDIRECT_URI);

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token request failed: {e}"))?;

    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("Token exchange failed: {err_body}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid token response JSON: {e}"))?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or_else(|| "Missing access_token in token response".to_string())?;

    let refresh_token = json["refresh_token"].as_str();
    let expires_in = json["expires_in"].as_f64().unwrap_or(3600.0);

    // Fetch user profile info
    let profile_resp = client
        .get("https://www.googleapis.com/oauth2/v2/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Profile request failed: {e}"))?;

    if !profile_resp.status().is_success() {
        return Err(format!("Profile request error {}", profile_resp.status()));
    }

    let userinfo: serde_json::Value = profile_resp
        .json()
        .await
        .map_err(|e| format!("Invalid profile JSON: {e}"))?;

    let email = userinfo["email"]
        .as_str()
        .ok_or_else(|| "Missing email in userinfo response".to_string())?
        .to_string();

    let name = userinfo["name"]
        .as_str()
        .unwrap_or(&email)
        .to_string();

    let picture = userinfo["picture"].as_str().map(str::to_string);

    let profile = GoogleAccountProfile {
        email: email.clone(),
        name,
        picture,
        is_primary: false,
        added_at_ms: chrono::Utc::now().timestamp_millis() as u64,
        scopes: vec![
            "gmail.readonly".into(),
            "gmail.send".into(),
            "gmail.modify".into(),
            "calendar".into(),
            "photos".into(),
        ],
    };

    crate::auth_vault::save_google_account(profile.clone(), refresh_token);
    crate::auth_vault::set_google_access_token(&email, access_token, expires_in);

    // Also update legacy single-account vault token for seamless backward-compatibility
    crate::auth_vault::set_token("google", access_token, expires_in);

    tracing::info!("Google account successfully authenticated & saved: {email}");
    Ok(profile)
}

/// Refresh an access token for a specific email using its stored refresh_token.
pub async fn refresh_access_token_for(email: &str) -> Result<String, String> {
    let refresh_token = crate::auth_vault::get_google_refresh_token(email)
        .ok_or_else(|| format!("No refresh token stored for {email}"))?;

    let (client_id, client_secret) = get_client_credentials();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let mut params = std::collections::HashMap::new();
    params.insert("client_id", client_id.as_str());
    if let Some(ref sec) = client_secret {
        params.insert("client_secret", sec.as_str());
    }
    params.insert("refresh_token", refresh_token.as_str());
    params.insert("grant_type", "refresh_token");

    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .map_err(|e| format!("Token refresh request failed: {e}"))?;

    if !resp.status().is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed for {email}: {err_body}"));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Invalid token refresh response JSON: {e}"))?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or_else(|| "Missing access_token in refresh response".to_string())?;

    let expires_in = json["expires_in"].as_f64().unwrap_or(3600.0);

    crate::auth_vault::set_google_access_token(email, access_token, expires_in);

    // If this is the primary account, update legacy single-token vault as well
    let accounts = crate::auth_vault::get_google_accounts();
    if accounts.iter().any(|a| a.email.eq_ignore_ascii_case(email) && a.is_primary) {
        crate::auth_vault::set_token("google", access_token, expires_in);
    }

    Ok(access_token.to_string())
}

// ─── Simple Helper Module URL Encoding ───────────────────────────────────────
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut encoded = String::new();
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    encoded.push(b as char);
                }
                _ => {
                    encoded.push_str(&format!("%{:02X}", b));
                }
            }
        }
        encoded
    }

    pub fn decode(s: &str) -> Option<String> {
        let mut bytes = Vec::new();
        let mut chars = s.bytes();
        while let Some(b) = chars.next() {
            if b == b'%' {
                let h1 = chars.next()?;
                let h2 = chars.next()?;
                let hex_arr = [h1, h2];
                let hex_str = std::str::from_utf8(&hex_arr).ok()?;
                let byte = u8::from_str_radix(hex_str, 16).ok()?;
                bytes.push(byte);
            } else if b == b'+' {
                bytes.push(b' ');
            } else {
                bytes.push(b);
            }
        }
        String::from_utf8(bytes).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let (verifier, challenge) = generate_pkce();
        assert_eq!(verifier.len(), 43);
        assert!(!challenge.is_empty());
        assert_ne!(verifier, challenge);
    }

    #[test]
    fn test_urlencoding() {
        let raw = "user@gmail.com & scopes";
        let enc = urlencoding::encode(raw);
        assert!(enc.contains("%20"));
        assert!(enc.contains("%40"));
        assert_eq!(urlencoding::decode(&enc).as_deref(), Some(raw));
    }
}
