//! Disposable, local REA browser observation for public website reviews.
//! REA is called only with its passive CDP commands; Chrome starts on one
//! prevalidated URL in a new profile and the user's browser is untouched.

use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

pub struct ReaObservation {
    pub image_data_url: Option<String>,
    pub summary: Option<String>,
    pub technologies: Vec<crate::foreground_review::TechnologyClaim>,
    pub warning: Option<String>,
}

fn chrome_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("NEXUS_CHROME_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() { return Some(path); }
    }
    #[cfg(windows)]
    {
        let candidates = [
            PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
        ];
        for path in candidates {
            if path.is_file() { return Some(path); }
        }
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let path = PathBuf::from(local).join(r"Google\Chrome\Application\chrome.exe");
            if path.is_file() { return Some(path); }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let path = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
        if path.is_file() { return Some(path); }
    }
    None
}

fn rea_script() -> Option<PathBuf> {
    let path = dirs_next::home_dir()?.join(".nexus-tools/rea/node_modules/rea-agents/scripts/rea.mjs");
    let manifest = path.parent()?.parent()?.join("package.json");
    let json: Value = serde_json::from_slice(&std::fs::read(manifest).ok()?).ok()?;
    if json["version"].as_str()? != "6.7.0" { return None; }
    if !json["repository"]["url"].as_str()?.contains("github.com/morluto/rea") { return None; }
    path.is_file().then_some(path)
}

fn result_value(json: &Value) -> &Value {
    json.get("result").unwrap_or(json)
}

async fn rea_command(script: &Path, args: &[&str]) -> Result<Value, String> {
    let mut cmd = Command::new("node");
    cmd.arg(script).args(args).arg("--format").arg("json");
    cmd.kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(18), cmd.output())
        .await.map_err(|_| "REA timed out".to_string())?
        .map_err(|e| format!("REA could not start: {e}"))?;
    if !output.status.success() {
        return Err(format!("REA {} failed", args.first().copied().unwrap_or("command")));
    }
    if output.stdout.len() > 7_000_000 {
        return Err("REA output exceeded the review limit".into());
    }
    serde_json::from_slice(&output.stdout).map_err(|_| "REA returned invalid JSON".into())
}

fn screenshot_data(json: &Value) -> Option<String> {
    let artifact = &result_value(json)["artifact"];
    if artifact["media_type"].as_str()? != "image/png" { return None; }
    let encoded = artifact["data_base64"].as_str()?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).ok()?;
    if bytes.len() > 4_000_000 || bytes.len() != artifact["bytes"].as_u64()? as usize
        || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return None; }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash != artifact["sha256"].as_str()? { return None; }
    Some(format!("data:image/png;base64,{encoded}"))
}

fn bundle_claims(url: &str, json: &Value) -> Vec<crate::foreground_review::TechnologyClaim> {
    let Some(rows) = result_value(json)["inferences"].as_array() else { return Vec::new() };
    rows.iter().take(20).filter_map(|row| {
        let value = row["value"].as_str()?.chars().take(90).collect::<String>();
        let kind = row["kind"].as_str()?;
        if !["vendor_fingerprint", "route_framework", "bundle_runtime"].contains(&kind) { return None; }
        let basis = row["basis"].as_array()?.first()?.to_string();
        Some(crate::foreground_review::TechnologyClaim {
            name: value, status: format!("REA inferred ({})", row["confidence"].as_str().unwrap_or("unknown")),
            evidence: basis.chars().take(300).collect(), source_url: url.to_string(),
        })
    }).collect()
}

fn remove_profile(profile: &Path, temp_root: &Path) {
    if !profile.file_name().and_then(|name| name.to_str())
        .map(|name| name.starts_with("nexus-review-")).unwrap_or(false) { return; }
    if let Ok(resolved) = profile.canonicalize() {
        if resolved.parent() == Some(temp_root) {
            let _ = std::fs::remove_dir_all(resolved);
        }
    }
}

/// One page, one new browser profile. All subprocesses have time bounds and
/// kill-on-drop; the profile is removed after Chrome exits.
pub async fn observe(url: &reqwest::Url, full_inspection: bool, cancelled: &Arc<AtomicBool>) -> ReaObservation {
    let unavailable = |message: String| ReaObservation {
        image_data_url: None, summary: None, technologies: Vec::new(), warning: Some(message),
    };
    if cancelled.load(Ordering::Relaxed) { return unavailable("Review cancelled".into()); }
    let Some(chrome) = chrome_path() else { return unavailable("Chrome is not installed for page screenshots".into()) };
    let Some(script) = rea_script() else { return unavailable("Pinned REA 6.7.0 is not installed on this laptop".into()) };
    let addr = match crate::foreground_review::public_address(url) {
        Ok(addr) => addr,
        Err(e) => return unavailable(e),
    };
    let raw_temp_root = std::env::temp_dir();
    let temp_root = match raw_temp_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return unavailable("Could not verify the temporary browser directory".into()),
    };
    let profile = raw_temp_root.join(format!("nexus-review-{}", crate::network::uuid_v4()));
    if std::fs::create_dir(&profile).is_err() { return unavailable("Could not create an isolated browser profile".into()) }
    let mut cmd = Command::new(chrome);
    cmd.arg("--headless=new")
        .arg("--no-first-run")
        .arg("--no-default-browser-check")
        .arg("--disable-extensions")
        .arg("--disable-background-networking")
        .arg("--no-proxy-server")
        .arg("--remote-debugging-address=127.0.0.1")
        .arg("--remote-debugging-port=0")
        .arg("--window-size=1280,800")
        .arg(format!("--host-resolver-rules=MAP {} {}", url.host_str().unwrap_or(""), addr.ip()))
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(url.as_str())
        .kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            remove_profile(&profile, &temp_root);
            return unavailable(format!("Could not start isolated Chrome: {e}"));
        }
    };
    let outcome = observe_started(url, &script, &profile, full_inspection, cancelled).await;
    let _ = child.kill().await;
    let _ = child.wait().await;
    remove_profile(&profile, &temp_root);
    outcome.unwrap_or_else(unavailable)
}

async fn observe_started(
    url: &reqwest::Url, script: &Path, profile: &Path,
    full_inspection: bool, cancelled: &Arc<AtomicBool>,
) -> Result<ReaObservation, String> {
    let port_file = profile.join("DevToolsActivePort");
    let mut port = None;
    for _ in 0..80 {
        if cancelled.load(Ordering::Relaxed) { return Err("Review cancelled".into()); }
        if let Ok(contents) = std::fs::read_to_string(&port_file) {
            port = contents.lines().next().and_then(|s| s.parse::<u16>().ok());
            if port.is_some() { break }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let endpoint = format!("http://127.0.0.1:{}", port.ok_or("Chrome did not expose local CDP")?);
    let mut target_id = None;
    for _ in 0..4 {
        if cancelled.load(Ordering::Relaxed) { return Err("Review cancelled".into()); }
        let target_list = rea_command(script, &["list-browser-targets", &endpoint]).await?;
        target_id = result_value(&target_list)["targets"].as_array()
            .and_then(|targets| targets.iter().find(|t| {
                t["url"].as_str().and_then(|u| reqwest::Url::parse(u).ok())
                    .map(|u| u.origin() == url.origin() && u.path() == url.path())
                    .unwrap_or(false)
            }))
            .and_then(|target| target["target_id"].as_str().map(str::to_owned));
        if target_id.is_some() { break; }
        tokio::time::sleep(Duration::from_millis(350)).await;
    }
    let target_id = target_id.ok_or("REA did not find the requested public page")?;
    if cancelled.load(Ordering::Relaxed) { return Err("Review cancelled".into()); }
    let shot = rea_command(script, &["capture-web-screenshot", &endpoint, &target_id]).await?;
    let image_data_url = screenshot_data(&shot).ok_or("REA screenshot failed integrity validation")?;
    let mut technologies = Vec::new();
    let mut summary = None;
    if full_inspection && !cancelled.load(Ordering::Relaxed) {
        if let Ok(inspection) = rea_command(script, &["inspect-web-page", &endpoint, &target_id]).await {
            let value = result_value(&inspection);
            let json = serde_json::to_string(value).unwrap_or_default();
            summary = Some(format!("REA page inspection: {}", json.chars().take(3500).collect::<String>()));
        }
        if let Ok(bundle) = rea_command(script, &["analyze-web-bundle", &endpoint, &target_id]).await {
            technologies = bundle_claims(url.as_str(), &bundle);
            let r = result_value(&bundle);
            let scripts = r["capture"]["scripts_observed"].as_u64().unwrap_or(0);
            let analyzed = r["capture"]["scripts_analyzed"].as_u64().unwrap_or(0);
            let tail = format!("Bundle scripts observed: {scripts}, analysed: {analyzed}.");
            summary = Some(format!("{} {tail}", summary.unwrap_or_default()));
        }
    }
    Ok(ReaObservation { image_data_url: Some(image_data_url), summary,
        technologies, warning: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unverified_screenshot() {
        assert!(screenshot_data(&serde_json::json!({"artifact":{"media_type":"image/png","bytes":4,"data_base64":"AAAA","sha256":"bad"}})).is_none());
    }
    #[test]
    fn extracts_only_bounded_bundle_inferences() {
        let json = serde_json::json!({"result":{"inferences":[
            {"kind":"route_framework","value":"Next.js","confidence":"medium","basis":[{"marker":"/_next/"}]},
            {"kind":"endpoint_candidate","value":"/api/pay","confidence":"low","basis":[{}]}
        ]}});
        let claims = bundle_claims("https://example.com/", &json);
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].name, "Next.js");
    }
}
