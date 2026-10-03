//! Piper TTS — local VITS ONNX fallback (no internet required).
//!
//! Uses piper-rs to load ONNX voice models. Lazy-loaded only when edge-tts
//! is unavailable (network down).
//!
//! Latency: ~40ms (warm, CPU inference)
//! RAM: 80 MB (model loaded in memory)
//! Model size: ~60 MB (en_US-amy-medium.onnx)
//! Cost: $0 (free, MIT license)
//! Quality: Good (clear but less natural than edge-tts/Kokoro)

use std::sync::Arc;
use tokio::sync::Mutex;

/// Lazy-initialized Piper TTS engine.
/// None = not loaded yet. Some = loaded and ready.
pub type PiperEngine = Arc<Mutex<Option<piper_rs::Piper>>>;

/// Stem of the currently loaded voice (mirrors the engine above).
/// Needed because the engine holds a single model — switching voices
/// unloads + reloads (~1.7s one-time per voice, then cached in RAM).
static LOADED_STEM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Legacy bundled voice id → model stem.
pub const BUNDLED_STEM: &str = "en_US-amy-medium";

/// Map a voice id to a Piper model stem.
/// `piper-amy` (legacy) → bundled amy; `piper-<stem>` → `<stem>`;
/// anything else → None. Pure + unit-tested.
pub fn voice_stem(voice_id: &str) -> Option<String> {
    if voice_id == "piper-amy" {
        return Some(BUNDLED_STEM.to_string());
    }
    if let Some(stem) = voice_id.strip_prefix("piper-") {
        let stem = stem.trim();
        if stem.is_empty() || stem.contains('/') || stem.contains('\\') || stem.contains("..") {
            return None;
        }
        return Some(stem.to_string());
    }
    None
}

/// Candidate directories (in order) for `<stem>.onnx` + `<stem>.onnx.json`.
fn voice_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            dirs.push(exe_dir.join("resources").join("piper"));
        }
    }
    if let Some(cache_dir) = dirs_next::cache_dir() {
        dirs.push(cache_dir.join("com.nexus.assistant").join("piper"));
    }
    dirs
}

/// User drop-in directory: %APPDATA%/com.nexus.assistant/piper_voices/.
fn user_voices_dir() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("com.nexus.assistant").join("piper_voices"))
}

/// Find `<stem>.onnx` + sidecar in a directory list. Pure over paths.
fn find_in_dirs(dirs: &[std::path::PathBuf], stem: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    for dir in dirs {
        let onnx = dir.join(format!("{stem}.onnx"));
        let json = dir.join(format!("{stem}.onnx.json"));
        if onnx.exists() && json.exists() {
            return Some((onnx, json));
        }
    }
    None
}

/// Scan for user-added voices (stems with .onnx + .json sidecar).
/// Always includes the bundled voice first. Pure-ish (fs reads).
pub fn scan_custom_voices() -> Vec<String> {
    let mut out = vec![BUNDLED_STEM.to_string()];
    let Some(dir) = user_voices_dir() else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("onnx") {
            continue;
        }
        let stem = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        if stem == BUNDLED_STEM || out.contains(&stem) {
            continue;
        }
        // Require the .json sidecar or piper-rs load fails.
        if p.with_extension("onnx.json").exists() || dir.join(format!("{stem}.onnx.json")).exists() {
            out.push(stem);
        }
    }
    out.sort();
    out
}

/// Create a new lazy Piper engine state (engine not loaded).
pub fn new_engine() -> PiperEngine {
    Arc::new(Mutex::new(None))
}

/// Find the Piper model path.
/// Checks bundled resources first, then user cache directory.
fn find_model_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    find_in_dirs(&voice_dirs(), BUNDLED_STEM)
}

/// Find paths for a specific voice stem.
/// Order: managed single slot (Feature 83 — the user's explicit equipped
/// twin wins) → bundled → cache → user drop-in.
fn find_voice_paths(stem: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    if crate::voice_catalog::twin_ready_for(stem) {
        if let Some(paths) = crate::voice_catalog::managed_model_paths() {
            tracing::info!("tts-piper: resolving '{stem}' from managed single slot");
            return Some(paths);
        }
    }
    let mut dirs = voice_dirs();
    if let Some(user) = user_voices_dir() {
        dirs.push(user);
    }
    find_in_dirs(&dirs, stem)
}

/// Load a specific voice, replacing whatever is loaded.
/// Records the stem so repeat calls are no-ops.
pub async fn load_voice(engine: &PiperEngine, stem: &str) -> Result<(), String> {
    if LOADED_STEM.lock().map(|g| g.as_deref() == Some(stem)).unwrap_or(false)
        && engine.lock().await.is_some()
    {
        return Ok(());
    }
    let (onnx_path, json_path) = find_voice_paths(stem).ok_or_else(|| {
        format!("Piper voice '{stem}' not found. Drop {stem}.onnx + {stem}.onnx.json into %APPDATA%/com.nexus.assistant/piper_voices/")
    })?;

    if std::env::var("PIPER_ESPEAKNG_DATA_DIRECTORY").is_err() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let espeak_parent = exe_dir.join("resources");
                if espeak_parent.join("espeak-ng-data").exists() {
                    std::env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", &espeak_parent);
                }
            }
        }
    }

    tracing::info!("tts-piper: loading voice '{stem}'...");
    let start = std::time::Instant::now();
    let piper = piper_rs::Piper::new(&onnx_path, &json_path)
        .map_err(|e| format!("Piper voice load failed: {}", e))?;
    *engine.lock().await = Some(piper);
    if let Ok(mut g) = LOADED_STEM.lock() {
        *g = Some(stem.to_string());
    }
    tracing::info!(
        "tts-piper: voice '{stem}' loaded in {:.2}s",
        start.elapsed().as_secs_f32()
    );
    Ok(())
}

/// Synthesize with an explicit voice stem (switches models if needed).
pub async fn synthesize_with_voice(
    engine: &PiperEngine,
    text: &str,
    stem: &str,
) -> Result<(Vec<f32>, u32), String> {
    if text.is_empty() {
        return Err("Empty text".to_string());
    }
    load_voice(engine, stem).await?;
    synthesize(engine, text).await
}

/// Lazily load the Piper engine on first use.
pub async fn ensure_engine_loaded(engine: &PiperEngine) -> Result<(), String> {
    if engine.lock().await.is_some() {
        return Ok(());
    }

    tracing::info!("tts-piper: lazy-loading Piper engine...");
    let start = std::time::Instant::now();

    let (onnx_path, json_path) = find_model_paths().ok_or_else(|| {
        "Piper model not found. Place en_US-amy-medium.onnx + .json in resources/piper/".to_string()
    })?;

    // Piper requires espeak-ng data path.
    // This is also set at startup in lib.rs::setup_espeak_data_path(),
    // but we keep it here as a fallback in case the startup check missed
    // the resources directory (e.g. different working directory).
    if std::env::var("PIPER_ESPEAKNG_DATA_DIRECTORY").is_err() {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let espeak_parent = exe_dir.join("resources");
                if espeak_parent.join("espeak-ng-data").exists() {
                    std::env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", &espeak_parent);
                    tracing::info!("tts-piper: espeak-ng data path set to {}", espeak_parent.display());
                }
            }
        }
    }

    let piper = piper_rs::Piper::new(&onnx_path, &json_path)
        .map_err(|e| format!("Piper model load failed: {}", e))?;

    *engine.lock().await = Some(piper);

    tracing::info!(
        "tts-piper: engine loaded in {:.2}s",
        start.elapsed().as_secs_f32()
    );

    Ok(())
}

/// Synthesize text to f32 PCM samples using Piper.
///
/// Returns (samples, sample_rate) for rodio playback.
/// Piper outputs 22050 Hz mono by default.
pub async fn synthesize(
    engine: &PiperEngine,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    if text.is_empty() {
        return Err("Empty text".to_string());
    }

    ensure_engine_loaded(engine).await?;

    let engine_clone = engine.clone();
    let text_clone = text.to_string();

    let (samples, sample_rate) = tokio::task::spawn_blocking(move || {
        let mut lock = engine_clone.blocking_lock();
        let piper = lock.as_mut().ok_or("Piper engine not loaded")?;

        // Piper::create returns (samples, sample_rate)
        let (samples, sample_rate) = piper
            .create(&text_clone, false, None, None, None, None)
            .map_err(|e| format!("Piper synthesis failed: {}", e))?;

        Ok::<(Vec<f32>, u32), String>((samples, sample_rate))
    })
    .await
    .map_err(|e| format!("Piper task panicked: {}", e))??;

    tracing::info!(
        "tts-piper: synthesized '{}' ({} samples, {}Hz)",
        crate::tts::truncate_for_log(text, 50),
        samples.len(),
        sample_rate
    );

    Ok((samples, sample_rate))
}

/// Unload the Piper engine to free ~80 MB RAM.
///
/// Called by the network monitor after 10 minutes of stable network.
/// If the network drops again, Piper will reload on the next fallback.
pub async fn unload_engine(engine: &PiperEngine) {
    let mut lock = engine.lock().await;
    if lock.is_some() {
        *lock = None;
        if let Ok(mut g) = LOADED_STEM.lock() {
            *g = None;
        }
        tracing::info!("tts-piper: engine unloaded (network stable for 10+ minutes, ~80 MB freed)");
    }
}

/// Check if the Piper engine is currently loaded.
pub async fn is_engine_loaded(engine: &PiperEngine) -> bool {
    engine.lock().await.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_engine_starts_empty() {
        let engine = new_engine();
        let _ = std::hint::black_box(engine);
    }

    #[test]
    fn test_find_model_paths_doesnt_panic() {
        let _ = find_model_paths();
    }

    #[test]
    fn test_voice_stem_legacy() {
        assert_eq!(voice_stem("piper-amy"), Some(BUNDLED_STEM.to_string()));
    }

    #[test]
    fn test_voice_stem_custom() {
        assert_eq!(
            voice_stem("piper-en_IN-rohan-medium"),
            Some("en_IN-rohan-medium".to_string())
        );
    }

    #[test]
    fn test_voice_stem_rejects() {
        assert_eq!(voice_stem("en-US-AvaNeural"), None);
        assert_eq!(voice_stem("piper-"), None);
        assert_eq!(voice_stem("piper-../evil"), None);
        assert_eq!(voice_stem("piper-a/b"), None);
        assert_eq!(voice_stem(""), None);
    }

    #[test]
    fn test_find_in_dirs() {
        let d = std::env::temp_dir().join(format!("nexus_piper_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        std::fs::write(d.join("q.onnx"), b"x").unwrap();
        std::fs::write(d.join("q.onnx.json"), b"{}").unwrap();
        let found = find_in_dirs(&[d.clone()], "q");
        assert!(found.is_some());
        assert!(find_in_dirs(&[d.clone()], "missing").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_scan_custom_voices_no_panic() {
        let voices = scan_custom_voices();
        assert!(voices.contains(&BUNDLED_STEM.to_string()));
    }
}
