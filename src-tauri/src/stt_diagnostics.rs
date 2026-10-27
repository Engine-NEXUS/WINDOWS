//! Optional local speech replay evidence. No plaintext clip reaches disk.
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};
use tauri::Manager;

const RETAIN_SECONDS: i64 = 24 * 60 * 60;
const MAX_CLIPS: usize = 100;
static PROCESS_EPOCH_MS: once_cell::sync::Lazy<i64> =
    once_cell::sync::Lazy::new(|| chrono::Utc::now().timestamp_millis());

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    session: u64,
    captured_at: i64,
    raw_text: String,
    corrected_text: String,
    intent: String,
    decision: String,
    stats: serde_json::Value,
    audio_pcm16_b64: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub session: u64,
    pub captured_at: i64,
    pub raw_text: String,
    pub corrected_text: String,
    pub intent: String,
    pub decision: String,
    pub stats: serde_json::Value,
}

fn dir(data: &Path) -> PathBuf { data.join("stt_diagnostics") }
fn path(data: &Path, session: u64) -> PathBuf {
    dir(data).join(format!("{}-{session}.nxm", *PROCESS_EPOCH_MS))
}
fn read_record(path: &Path, key: &crate::memcore::crypto::Key) -> Option<Record> {
    let blob = fs::read(path).ok()?;
    serde_json::from_slice(&crate::memcore::crypto::open(key, &blob)?).ok()
}
fn write_record(path: &Path, key: &crate::memcore::crypto::Key, record: &Record) -> Result<(), String> {
    let plain = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    let sealed = crate::memcore::crypto::seal(key, &plain).ok_or("encryption unavailable")?;
    let temp = path.with_extension("tmp");
    fs::write(&temp, sealed).map_err(|e| e.to_string())?;
    // Windows rename does not replace an existing destination. Both the old
    // file and the temporary replacement are sealed before this small gap.
    if path.exists() { fs::remove_file(path).map_err(|e| e.to_string())?; }
    fs::rename(&temp, path).map_err(|e| e.to_string())
}
fn purge(data: &Path, now: i64) {
    let Ok(entries) = fs::read_dir(dir(data)) else { return; };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if extension != "nxm" && extension != "tmp" { continue; }
        let age = entry.metadata().ok().and_then(|m| m.modified().ok())
            .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| now.saturating_sub(d.as_secs() as i64));
        let limit = if extension == "tmp" { 3600 } else { RETAIN_SECONDS };
        if age.is_none_or(|a| a >= limit || a < -300) {
            crate::memcore::crypto::shred_file(&path);
        } else if extension == "nxm" { live.push(path); }
    }
    live.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
    for old in live.iter().take(live.len().saturating_sub(MAX_CLIPS)) {
        crate::memcore::crypto::shred_file(old);
    }
}

pub fn start_retention_sweep<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(data) = app.path().app_data_dir() else { return; };
    std::thread::spawn(move || loop {
        if crate::focus_mode::paused() {
            crate::focus_mode::defer_log("stt_diagnostics_sweep");
            std::thread::sleep(std::time::Duration::from_secs(60));
            continue;
        }
        purge(&data, chrono::Utc::now().timestamp());
        std::thread::sleep(std::time::Duration::from_secs(60));
    });
}

pub fn enabled<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let Ok(data) = app.path().app_data_dir() else { return false; };
    let Ok(settings) = fs::read_to_string(data.join("settings.json")) else { return false; };
    serde_json::from_str::<serde_json::Value>(&settings).ok()
        .and_then(|v| v.get("sttDiagnostics").and_then(|x| x.as_bool()))
        .unwrap_or(false)
}

pub fn save_capture<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: u64,
    pcm: &[i16], raw_text: &str, stats: &crate::wakeword_oww::TurnStats) {
    if !enabled(app) { return; }
    let Some(key) = crate::memcore::crypto::process_key() else {
        tracing::warn!("stt diagnostics: OS encryption key unavailable; clip discarded"); return;
    };
    let Ok(data) = app.path().app_data_dir() else { return; };
    let folder = dir(&data);
    if fs::create_dir_all(&folder).is_err() { return; }
    purge(&data, chrono::Utc::now().timestamp());
    let bytes: Vec<u8> = pcm.iter().flat_map(|s| s.to_le_bytes()).collect();
    let record = Record { session, captured_at: chrono::Utc::now().timestamp(),
        raw_text: raw_text.into(), corrected_text: String::new(), intent: String::new(),
        decision: "pending".into(), stats: serde_json::to_value(stats).unwrap_or_default(),
        audio_pcm16_b64: base64::engine::general_purpose::STANDARD.encode(bytes) };
    if let Err(e) = write_record(&path(&data, session), &key, &record) {
        tracing::warn!("stt diagnostics: failed to store encrypted clip: {e}");
    } else {
        purge(&data, chrono::Utc::now().timestamp());
    }
}

pub fn update_decision<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: u64,
    corrected: &str, intent: &str, decision: &str) {
    if session == 0 || !enabled(app) { return; }
    let (Some(key), Ok(data)) = (crate::memcore::crypto::process_key(), app.path().app_data_dir()) else { return; };
    let file = path(&data, session);
    let Some(mut record) = read_record(&file, &key) else { return; };
    record.corrected_text = corrected.into();
    record.intent = intent.into();
    record.decision = decision.into();
    let _ = write_record(&file, &key, &record);
}

#[tauri::command]
pub fn stt_diagnostics_list<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Vec<Summary> {
    let (Some(key), Ok(data)) = (crate::memcore::crypto::process_key(), app.path().app_data_dir()) else { return Vec::new(); };
    purge(&data, chrono::Utc::now().timestamp());
    let Ok(entries) = fs::read_dir(dir(&data)) else { return Vec::new(); };
    let mut summaries: Vec<_> = entries.flatten().filter_map(|e| read_record(&e.path(), &key)).map(|r| Summary {
        session: r.session, captured_at: r.captured_at, raw_text: r.raw_text,
        corrected_text: r.corrected_text, intent: r.intent, decision: r.decision, stats: r.stats,
    }).collect();
    summaries.sort_by_key(|r| r.captured_at);
    summaries
}

#[tauri::command]
pub fn stt_diagnostics_delete<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<u32, String> {
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let Ok(entries) = fs::read_dir(dir(&data)) else { return Ok(0); };
    let mut count = 0;
    for entry in entries.flatten() {
        if entry.path().extension().is_some_and(|e| e == "nxm" || e == "tmp") {
            crate::memcore::crypto::shred_file(&entry.path()); count += 1;
        }
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_record_has_no_plaintext_and_is_readable() {
        let folder = std::env::temp_dir().join(format!("nexus-stt-diag-{}", std::process::id()));
        let _ = fs::create_dir_all(&folder);
        let file = folder.join("1.nxm");
        let key = [9u8; 32];
        let r = Record { session: 1, captured_at: 1, raw_text: "open secret app".into(),
            corrected_text: String::new(), intent: String::new(), decision: "pending".into(),
            stats: serde_json::json!({"voiced_chunks": 4}), audio_pcm16_b64: "AQI=".into() };
        write_record(&file, &key, &r).unwrap();
        assert!(!fs::read(&file).unwrap().windows(6).any(|w| w == b"secret"));
        assert_eq!(read_record(&file, &key).unwrap().raw_text, r.raw_text);
        crate::memcore::crypto::shred_file(&file);
        let _ = fs::remove_dir(&folder);
    }
    #[test]
    fn expired_clip_is_removed_by_retention_sweep() {
        let data = std::env::temp_dir().join(format!("nexus-stt-retention-{}", std::process::id()));
        let folder = dir(&data);
        fs::create_dir_all(&folder).unwrap();
        let old = folder.join("old.nxm");
        fs::write(&old, b"sealed fixture").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&old).unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1)).unwrap();
        purge(&data, RETAIN_SECONDS + 2);
        assert!(!old.exists());
        let _ = fs::remove_dir_all(&data);
    }
}
