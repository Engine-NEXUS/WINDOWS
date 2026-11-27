//! Glue between the pure `nexus_reverse` crate and the app: resolve the target, analyse it on a blocking
//! thread, optionally ask the user's own LLM key for an explanation (evidence summary only, never the file),
//! validate that explanation, and render the report.
//!
//! Hard rules: the target is only READ (`nexus_reverse::analyze::analyze_path`, `std::fs::read`); nothing in
//! this module launches a process, loads a library or opens a socket to the target.

use super::target::{self, TargetError, TargetSpec};
use nexus_reverse::{analyze, explain, har, report};
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

pub struct ReverseSettings {
    pub enabled: bool,
    pub explain: bool,
    pub max_mb: u64,
}

impl Default for ReverseSettings {
    fn default() -> Self {
        Self { enabled: true, explain: true, max_mb: 512 }
    }
}

/// Lenient reader (one bad field never resets the others) - same convention as the other `read_*` helpers.
pub fn read_settings<R: Runtime>(app: &AppHandle<R>) -> ReverseSettings {
    let mut s = ReverseSettings::default();
    let Ok(dir) = app.path().app_data_dir() else { return s };
    let Ok(text) = std::fs::read_to_string(dir.join("settings.json")) else { return s };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return s };
    if let Some(b) = v.get("reverseLabEnabled").and_then(|x| x.as_bool()) {
        s.enabled = b;
    }
    if let Some(b) = v.get("reverseLabExplain").and_then(|x| x.as_bool()) {
        s.explain = b;
    }
    if let Some(n) = v.get("reverseLabMaxFileMb").and_then(|x| x.as_u64()) {
        s.max_mb = n.clamp(1, 4096);
    }
    s
}

pub struct Outcome {
    pub markdown: String,
    pub spoken: String,
    pub title: String,
}

/// Map the intent's target string onto a `TargetSpec`.
pub fn spec_for(target_str: &str, har_mode: bool) -> TargetSpec {
    match target_str {
        "app" => TargetSpec::ForegroundApp,
        "download" => TargetSpec::JustDownloaded,
        "clipboard" => TargetSpec::Clipboard,
        "latest" => TargetSpec::LatestHar,
        t if t.starts_with("path:") => {
            let p = &t[5..];
            // a bare file name ("setup.exe") is looked up in Downloads / Desktop
            if !p.contains('\\') && !p.contains('/') {
                let (dl, extra) = target::user_dirs();
                for d in dl.iter().chain(extra.iter()) {
                    let c = d.join(p);
                    if c.is_file() {
                        return TargetSpec::Path(c.display().to_string());
                    }
                }
            }
            TargetSpec::Path(p.to_string())
        }
        _ => {
            if har_mode {
                TargetSpec::LatestHar
            } else {
                TargetSpec::ForegroundApp
            }
        }
    }
}

fn spoken_error(e: &TargetError) -> String {
    e.spoken()
}

/// Ask the user's own LLM key for an explanation. Only the capped, redacted evidence summary is sent.
async fn ask_model<R: Runtime>(app: &AppHandle<R>, a: &analyze::Analysis) -> Option<String> {
    let keys = crate::router::read_provider_keys(app);
    if keys.groq.is_empty() && keys.gemini.is_empty() && keys.cerebras.is_empty() {
        return None;
    }
    let summary = explain::evidence_summary(a, explain::SUMMARY_CAP);
    let client = reqwest::Client::builder().timeout(Duration::from_secs(25)).build().ok()?;
    let user = explain::user_prompt(&summary);
    let fut = crate::router::route_with_system(explain::system_prompt(), &user, 900, true, &keys, &client);
    tokio::time::timeout(Duration::from_secs(30), fut).await.ok().flatten().map(|r| r.text)
}

fn resolve(target_str: &str, har_mode: bool, max_mb: u64) -> Result<target::ResolvedTarget, TargetError> {
    target::resolve(&spec_for(target_str, har_mode), max_mb.saturating_mul(1 << 20))
}

pub async fn run_program<R: Runtime>(app: &AppHandle<R>, target_str: &str) -> Result<Outcome, String> {
    let st = read_settings(app);
    if !st.enabled {
        return Err("Reverse Lab is switched off in settings, sir.".to_string());
    }
    let resolved = resolve(target_str, false, st.max_mb).map_err(|e| spoken_error(&e))?;
    analyse_resolved(app, resolved.path, resolved.how, st.explain).await
}

/// Same pipeline for an explicit path (IPC / CLI).
pub async fn analyse_resolved<R: Runtime>(app: &AppHandle<R>, path: PathBuf, how: &'static str, explain_on: bool) -> Result<Outcome, String> {
    let p2 = path.clone();
    let a = tokio::task::spawn_blocking(move || analyze::analyze_path(&p2))
        .await
        .map_err(|_| "The analysis crashed unexpectedly, sir. Nothing was run.".to_string())?
        .map_err(|e| format!("I couldn't read that file: {e}"))?;
    if a.kind == analyze::FileKind::Unknown {
        return Err("That file isn't a Windows program or an Electron archive I can analyse, sir.".to_string());
    }
    let raw = if explain_on { ask_model(app, &a).await } else { None };
    let (e, md) = report::explain_and_report(&a, how, raw.as_deref());
    let spoken = report::spoken_summary(&a, &e);
    Ok(Outcome { markdown: md, spoken, title: a.file_name.clone() })
}

pub async fn run_har<R: Runtime>(app: &AppHandle<R>, target_str: &str) -> Result<Outcome, String> {
    let st = read_settings(app);
    if !st.enabled {
        return Err("Reverse Lab is switched off in settings, sir.".to_string());
    }
    let resolved = resolve(target_str, true, st.max_mb).map_err(|e| spoken_error(&e))?;
    let path = resolved.path.clone();
    let result = tokio::task::spawn_blocking(move || std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| har::analyze_har(&b)))
        .await
        .map_err(|_| "The HAR analysis crashed unexpectedly, sir.".to_string())?
        .map_err(|e| format!("I couldn't read that HAR file: {e}"))?;
    if result.endpoints.is_empty() {
        return Err("That capture has no API requests I can use, sir.".to_string());
    }
    // Save the OpenAPI document in the app's own data folder (never next to the user's capture).
    let mut saved: Option<PathBuf> = None;
    if let Ok(dir) = app.path().app_data_dir() {
        let out_dir = dir.join("reverse");
        if std::fs::create_dir_all(&out_dir).is_ok() {
            let stem = resolved.path.file_stem().and_then(|s| s.to_str()).unwrap_or("capture");
            let out = out_dir.join(format!("{}.openapi.yaml", stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect::<String>()));
            if std::fs::write(&out, &result.openapi_yaml).is_ok() {
                saved = Some(out);
            }
        }
    }
    let mut md = report::har_report(&result, resolved.how);
    if let Some(p) = &saved {
        md.push_str(&format!("\nOpenAPI document saved to `{}`.\n", p.display()));
    }
    let spoken = format!(
        "I found {} endpoint{} in that capture and wrote an OpenAPI document. Credentials were dropped, and it's a reconstruction, not a spec.",
        result.endpoints.len(),
        if result.endpoints.len() == 1 { "" } else { "s" }
    );
    Ok(Outcome { markdown: md, spoken, title: "API from capture".to_string() })
}
