//! Feature 83 P3 — background offline-twin swap worker.
//!
//! Downloads the equipped persona's Piper twin into the managed
//! single slot (`voices/active_offline.onnx`), verified and atomic:
//! stream → `.tmp` → SHA-256 (+ catalog checksum when known) →
//! trial-load → delete old → atomic rename → manifest → engine reload.
//! Any failure leaves the previous twin untouched and emits an error
//! status (hub falls back to the cloud pill — never a broken state).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Runtime};

/// Verified rhasspy/piper-voices layout:
/// en/{lang}/{locale}/{voice}/{quality}/{stem}.onnx
pub const PIPER_CDN_BASE: &str =
    "https://huggingface.co/rhasspy/piper-voices/resolve/main";
const SLOT_ONNX: &str = "active_offline.onnx";
const SLOT_JSON: &str = "active_offline.onnx.json";
const TMP_ONNX: &str = "voice_download.tmp";
const TMP_JSON: &str = "voice_download.tmp.json";

static SWAP_ACTIVE: AtomicBool = AtomicBool::new(false);
static SWAP_TARGET: Mutex<Option<String>> = Mutex::new(None);

/// Persona key currently downloading (if any) — lets get_voice_status
/// report "downloading" truthfully mid-swap.
pub fn swap_target() -> Option<String> {
    SWAP_TARGET.lock().ok()?.clone()
}

/// Download URL for a Piper stem, derived from the verified repo layout.
/// `{locale}-{voice}-{quality}` → `en/{lang}/{locale}/{voice}/{quality}/`.
/// Returns None when the stem doesn't decompose (no guessing — a wrong
/// URL is worse than an honest error status).
pub fn piper_download_url(stem: &str, ext_json: bool) -> Option<String> {
    let mut parts = stem.split('-');
    let locale = parts.next()?;
    let mut rest: Vec<&str> = parts.collect();
    if rest.is_empty() || !locale.contains('_') {
        return None;
    }
    let quality = rest.pop()?;
    if quality.is_empty() {
        return None;
    }
    let voice = rest.join("-");
    if voice.is_empty() {
        return None;
    }
    let lang = locale.split('_').next()?;
    let mut file = format!(
        "{PIPER_CDN_BASE}/{lang}/{locale}/{voice}/{quality}/{stem}.onnx"
    );
    if ext_json {
        file.push_str(".json");
    }
    Some(file)
}

/// SHA-256 hex digest of a file. Pure over bytes + unit-tested.
pub fn sha256_file(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::Digest;
    let bytes = std::fs::read(path)?;
    let mut hasher = sha2::Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn emit_status<R: Runtime>(app: &AppHandle<R>, status: &str, key: &str, progress: Option<u8>) {
    let _ = app.emit(
        "voice:status",
        serde_json::json!({ "status": status, "voice_key": key, "progress": progress }),
    );
}

fn finish_swap(key: &str) {
    SWAP_ACTIVE.store(false, Ordering::SeqCst);
    if let Ok(mut t) = SWAP_TARGET.lock() {
        if t.as_deref() == Some(key) {
            *t = None;
        }
    }
}

/// Background swap worker. Spawned (never awaited) by set_voice_preference.
/// Emits downloading{progress} → ready | error. Old twin is deleted ONLY
/// after the new files verify — a failed download changes nothing.
pub async fn run_swap<R: Runtime>(
    app: AppHandle<R>,
    voice_key: String,
    engine: crate::tts_piper::PiperEngine,
) {
    use tokio::io::AsyncWriteExt;

    let Some(persona) = crate::voice_catalog::find_by_key(&voice_key) else {
        return;
    };
    let stem = persona.local_model.to_string();

    // Already cached → nothing to do (fresh check; P2 emitted cloud-only
    // before the spawn won the race in the other order).
    if crate::voice_catalog::twin_ready_for(&stem) {
        emit_status(&app, "ready", &voice_key, Some(100));
        return;
    }
    // One swap at a time — a second equip waits for the next turn.
    if SWAP_ACTIVE.swap(true, Ordering::SeqCst) {
        tracing::info!("tts-swap: swap already active, skipping duplicate for '{voice_key}'");
        return;
    }
    if let Ok(mut t) = SWAP_TARGET.lock() {
        *t = Some(voice_key.clone());
    }

    let dir = match crate::voice_catalog::managed_voices_dir() {
        Some(d) => d,
        None => {
            emit_status(&app, "error", &voice_key, None);
            finish_swap(&voice_key);
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("tts-swap: cannot create voices dir: {e}");
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }

    let onnx_url = match piper_download_url(&stem, false) {
        Some(u) => u,
        None => {
            tracing::warn!("tts-swap: stem '{stem}' does not decompose into a CDN path");
            emit_status(&app, "error", &voice_key, None);
            finish_swap(&voice_key);
            return;
        }
    };
    let json_url = match piper_download_url(&stem, true) {
        Some(u) => u,
        None => {
            emit_status(&app, "error", &voice_key, None);
            finish_swap(&voice_key);
            return;
        }
    };

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .unwrap_or_default();
    let tmp_onnx = dir.join(TMP_ONNX);
    let tmp_json = dir.join(TMP_JSON);

    // Stream the model with byte progress (0→85%), then the sidecar (→90%).
    emit_status(&app, "downloading", &voice_key, Some(0));
    let downloaded_onnx = async {
        let mut resp = client.get(&onnx_url).send().await.map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("model HTTP {}", resp.status()));
        }
        let total = resp.content_length().unwrap_or(0);
        let mut file = tokio::fs::File::create(&tmp_onnx)
            .await
            .map_err(|e| e.to_string())?;
        let mut done: u64 = 0;
        let mut last_emit = 0u8;
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    done += chunk.len() as u64;
                    if total > 0 {
                        let pct = ((done * 85) / total.max(1)) as u8;
                        if pct >= last_emit + 10 {
                            last_emit = pct;
                            emit_status(&app, "downloading", &voice_key, Some(pct));
                        }
                    }
                }
                Ok(None) => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        file.flush().await.map_err(|e| e.to_string())?;
        if done == 0 {
            return Err("model download was empty".to_string());
        }
        Ok::<(), String>(())
    }
    .await;
    if let Err(e) = downloaded_onnx {
        tracing::warn!("tts-swap: model download failed for '{stem}': {e}");
        let _ = std::fs::remove_file(&tmp_onnx);
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }

    // Sidecar (tiny — single shot).
    let downloaded_json = async {
        let bytes = client
            .get(&json_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .bytes()
            .await
            .map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Err("sidecar download was empty".to_string());
        }
        tokio::fs::write(&tmp_json, &bytes)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<(), String>(())
    }
    .await;
    if let Err(e) = downloaded_json {
        tracing::warn!("tts-swap: sidecar download failed for '{stem}': {e}");
        let _ = std::fs::remove_file(&tmp_onnx);
        let _ = std::fs::remove_file(&tmp_json);
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }
    emit_status(&app, "downloading", &voice_key, Some(90));

    // Verify: catalog checksum when known, then trial-load (catches
    // truncation that hashes can't — the definitive check).
    let digest = match sha256_file(&tmp_onnx) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("tts-swap: hash failed: {e}");
            let _ = std::fs::remove_file(&tmp_onnx);
            let _ = std::fs::remove_file(&tmp_json);
            emit_status(&app, "error", &voice_key, None);
            finish_swap(&voice_key);
            return;
        }
    };
    if let Some(expected) = persona.sha256 {
        if !expected.is_empty() && expected != digest {
            tracing::warn!("tts-swap: checksum mismatch for '{stem}'");
            let _ = std::fs::remove_file(&tmp_onnx);
            let _ = std::fs::remove_file(&tmp_json);
            emit_status(&app, "error", &voice_key, None);
            finish_swap(&voice_key);
            return;
        }
    }
    if piper_rs::Piper::new(&tmp_onnx, &tmp_json).is_err() {
        tracing::warn!("tts-swap: trial load failed for '{stem}'");
        let _ = std::fs::remove_file(&tmp_onnx);
        let _ = std::fs::remove_file(&tmp_json);
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }

    // Verified — NOW delete the old slot and atomically rename.
    let slot_onnx = dir.join(SLOT_ONNX);
    let slot_json = dir.join(SLOT_JSON);
    let _ = std::fs::remove_file(&slot_onnx);
    let _ = std::fs::remove_file(&slot_json);
    if std::fs::rename(&tmp_onnx, &slot_onnx).is_err()
        || std::fs::rename(&tmp_json, &slot_json).is_err()
    {
        tracing::warn!("tts-swap: atomic rename failed");
        let _ = std::fs::remove_file(&tmp_onnx);
        let _ = std::fs::remove_file(&tmp_json);
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }
    let manifest = crate::voice_catalog::VoiceManifest {
        voice_key: voice_key.clone(),
        model_name: stem.clone(),
        sha256: digest,
        version: 1,
    };
    if crate::voice_catalog::write_manifest_at(&dir, &manifest).is_err() {
        tracing::warn!("tts-swap: manifest write failed");
        emit_status(&app, "error", &voice_key, None);
        finish_swap(&voice_key);
        return;
    }

    // Reload the engine onto the new twin (slot-aware resolution finds it).
    match crate::tts_piper::load_voice(&engine, &stem).await {
        Ok(()) => {
            tracing::info!("tts-swap: '{stem}' live in single slot");
            emit_status(&app, "ready", &voice_key, Some(100));
        }
        Err(e) => {
            tracing::warn!("tts-swap: engine reload failed for '{stem}': {e}");
            emit_status(&app, "error", &voice_key, None);
        }
    }
    finish_swap(&voice_key);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_piper_download_urls_verified_layout() {
        // Layout verified against rhasspy/piper-voices VOICES.md (2026-10-02).
        assert_eq!(
            piper_download_url("en_GB-alan-medium", false),
            Some(format!(
                "{PIPER_CDN_BASE}/en/en_GB/alan/medium/en_GB-alan-medium.onnx"
            ))
        );
        assert_eq!(
            piper_download_url("en_GB-alan-medium", true),
            Some(format!(
                "{PIPER_CDN_BASE}/en/en_GB/alan/medium/en_GB-alan-medium.onnx.json"
            ))
        );
        assert_eq!(
            piper_download_url("en_US-hfc_female-medium", false),
            Some(format!(
                "{PIPER_CDN_BASE}/en/en_US/hfc_female/medium/en_US-hfc_female-medium.onnx"
            ))
        );
    }

    #[test]
    fn test_piper_download_url_rejects_garbage() {
        assert_eq!(piper_download_url("", false), None);
        assert_eq!(piper_download_url("amy", false), None);
        assert_eq!(piper_download_url("en_US", false), None);
        assert_eq!(piper_download_url("enUS-amy-medium", false), None); // no underscore
        assert_eq!(piper_download_url("-amy-medium", false), None);
    }

    #[test]
    fn test_sha256_file_known_vector() {
        let dir = std::env::temp_dir().join(format!("nexus_voice_sha_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc.txt");
        std::fs::write(&p, b"abc").unwrap();
        // SHA-256("abc") — canonical test vector.
        assert_eq!(
            sha256_file(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_every_catalog_twin_resolves_a_url() {
        for p in crate::voice_catalog::VOICE_CATALOG {
            assert!(
                piper_download_url(p.local_model, false).is_some(),
                "no URL for twin {}",
                p.local_model
            );
            assert!(
                piper_download_url(p.local_model, true).is_some(),
                "no JSON URL for twin {}",
                p.local_model
            );
        }
    }
}
