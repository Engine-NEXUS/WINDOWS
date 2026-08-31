use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant};
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::State;

pub struct SttState {
    pub transcriber: Arc<Mutex<Option<MoonshineModel>>>,
}

/// Resolve the Moonshine model directory.
/// Looks in (1) NEXUS_STT_MODEL_DIR env var, (2) app data dir /models/moonshine,
/// (3) dev path <repo>/src-tauri/models/moonshine.
fn resolve_model_dir() -> Result<PathBuf, String> {
    if let Ok(dir) = std::env::var("NEXUS_STT_MODEL_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Ok(p);
        }
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(&appdata)
            .join("com.nexus.assistant")
            .join("models")
            .join("moonshine");
        if p.exists() {
            return Ok(p);
        }
    }

    // Dev path relative to the executable
    let dev_path = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .and_then(|p| p.parent()) // target/release -> target
        .and_then(|p| p.parent()) // target -> src-tauri
        .map(|p| p.join("models").join("moonshine"));
    if let Some(p) = dev_path {
        if p.exists() {
            return Ok(p);
        }
    }

    Err("Moonshine model directory not found. Place ONNX model files in %APPDATA%/com.nexus.assistant/models/moonshine or set NEXUS_STT_MODEL_DIR.".to_string())
}

#[tauri::command]
pub async fn transcribe_audio(
    samples: Vec<i16>,
    state: State<'_, SttState>,
) -> Result<String, String> {
    tracing::info!("stt: received {} samples for local Moonshine transcription", samples.len());

    let transcriber_arc = state.transcriber.clone();
    let mut lock = transcriber_arc.lock().await;

    if lock.is_none() {
        tracing::info!("stt: initializing Moonshine Tiny model...");
        let model_dir = resolve_model_dir()?;
        tracing::info!("stt: using model dir: {:?}", model_dir);
        let model = MoonshineModel::load(
            &model_dir,
            MoonshineVariant::Tiny,
            &Quantization::default(),
        )
            .map_err(|e| format!("Failed to init Moonshine: {}", e))?;
        *lock = Some(model);
        tracing::info!("stt: Moonshine initialized.");
    }

    let model = lock.as_mut().unwrap();

    // Convert i16 to f32 for transcribe-rs
    let f32_samples: Vec<f32> = samples.iter().map(|&s| s as f32 / 32768.0).collect();

    let result = model.transcribe(&f32_samples, &TranscribeOptions::default())
        .map_err(|e| format!("Transcription error: {}", e))?;

    let text = result.text.trim().to_string();

    // Apply hallucination filter (same as before)
    let filtered = apply_hallucination_filter(&text);

    if filtered != text {
        tracing::info!("stt: filtered hallucination '{}' -> '{}'", text, filtered);
    } else {
        tracing::info!("stt: transcript: '{}'", text);
    }

    Ok(filtered)
}

fn apply_hallucination_filter(text: &str) -> String {
    let lower = text.to_lowercase();
    let hallucinations = [
        "thank you for watching", "thanks for watching", "thank you.", "you.", "bye.",
        "please subscribe", "subscribe to my channel",
    ];

    for h in hallucinations.iter() {
        if lower.contains(h) {
            return "".to_string();
        }
    }

    let alpha_count = text.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 2 {
        return "".to_string();
    }

    text.to_string()
}

#[tauri::command]
pub async fn stt_status() -> Result<bool, String> {
    // We are running in-process, so if this is called, we are up.
    Ok(true)
}
