//! IPC commands for setup window management and configuration.

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(feature = "wakeword-sherpa")]
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_autostart::ManagerExt;

// Phase D: voice_profile is now available for both wakeword-oww and wakeword-sherpa
use crate::voice_profile;
use crate::app_registry;

// ─── Pending sidebar content ───────────────────────────────────────
//
// When the sidebar window is created on-demand, the WebView2 needs time
// to load sidebar.html and mount the React app before it can receive
// Tauri events. If we emit `sidebar:show` / `sidebar:backdrop` immediately
// after creating the window, those events are lost because no listener
// exists yet.
//
// Instead, we store the content + backdrop here. The frontend calls
// `get_pending_sidebar_content` on mount, which returns and clears the
// pending data. This is race-free regardless of how long the WebView
// takes to load.

#[derive(Clone)]
struct PendingSidebar {
    query: String,
    text: String,
    backdrop: Option<String>, // data:image/png;base64,... URI
    analysis: Option<serde_json::Value>, // structured repo analysis data
    confirmation: Option<serde_json::Value>, // action confirmation payload
}

static PENDING_SIDEBAR: Mutex<Option<PendingSidebar>> = Mutex::new(None);

// ─── Pending PR list (race-free sidebar creation) ──────────────────────
// Same pattern as PENDING_SIDEBAR: the orchestrator stores the PR list
// result here before creating the sidebar window. The frontend calls
// `get_pending_pr_list` on mount to fetch (and clear) the pending data.
// This is race-free regardless of how long the WebView takes to load.
static PENDING_PR_LIST: Mutex<Option<serde_json::Value>> = Mutex::new(None);

/// Store a pending PR list result (called from the orchestrator before
/// creating the sidebar window). The frontend fetches this on mount.
pub fn set_pending_pr_list(pr_list_json: serde_json::Value) {
    let mut pending = PENDING_PR_LIST.lock().unwrap();
    *pending = Some(pr_list_json);
}

/// IPC: Fetch pending PR list data (called by the PR list sidebar on mount).
/// Returns the PR list JSON that was stored by the orchestrator, or null
/// if no data is pending. Clears the pending data after returning.
#[tauri::command]
pub fn get_pending_pr_list() -> Result<Option<serde_json::Value>, String> {
    let mut pending = PENDING_PR_LIST.lock().unwrap();
    let data = pending.take();
    if data.is_some() {
        tracing::info!("pr-list: pending data fetched by frontend");
    }
    Ok(data)
}

/// IPC: open the setup window (called from tray menu "Settings…" or first launch).
/// Creates the window on-demand if it doesn't exist (saves ~250 MB RAM at idle).
#[tauri::command]
pub fn open_setup_window<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let win = crate::dyn_windows::get_or_create_window(&app, crate::dyn_windows::WindowConfig::setup())?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// IPC: close/destroy the setup window and activate the main assistant orb.
/// Destroys the window (not just hide) to free ~250 MB of WebView2 processes.
/// If `first_run` is true, speaks the first-run greeting instead of waking.
#[tauri::command]
pub fn close_setup_window<R: Runtime>(
    app: tauri::AppHandle<R>,
    first_run: Option<bool>,
) -> Result<(), String> {
    let _ = crate::dyn_windows::destroy_window(&app, "setup");
    crate::window_manager::show_orb_interactive(&app);
    if first_run.unwrap_or(false) {
        let _ = tauri::Emitter::emit(&app, "orb:first_run_greeting", ());
    } else {
        let _ = tauri::Emitter::emit(&app, "orb:wake", ());
    }
    Ok(())
}

/// IPC: save the server URL config (marks setup as complete).
/// Writes a JSON file to the app data dir so the app knows setup is done.
#[tauri::command]
pub fn save_server_config<R: Runtime>(
    app: tauri::AppHandle<R>,
    server_url: String,
    user_id: String,
    device_id: String,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_path = dir.join("nexus-config.json");
    let config = serde_json::json!({
        "serverUrl": server_url,
        "userId": user_id,
        "deviceId": device_id,
    });
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(&config_path, config.to_string()).map_err(|e| e.to_string())?;
    tracing::info!("server config saved to {:?}", config_path);
    Ok(())
}

/// Serialized server config returned by `get_server_config`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub server_url: String,
    pub user_id: String,
    pub device_id: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            server_url: option_env!("NEXUS_SERVER_URL")
                .unwrap_or("https://nexus-worker.chitkullakshya.workers.dev")
                .to_string(),
            user_id: String::new(),
            device_id: String::new(),
        }
    }
}

/// IPC: Get the saved server config (or defaults if not yet configured).
/// The frontend calls this at startup to get the Worker URL, user ID,
/// and device ID — instead of relying on build-time env vars.
#[tauri::command]
pub fn get_server_config<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<ServerConfig, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let config_path = dir.join("nexus-config.json");
    if !config_path.exists() {
        return Ok(ServerConfig::default());
    }
    let content = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
    let json: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let default_url = option_env!("NEXUS_SERVER_URL")
        .unwrap_or("https://nexus-worker.chitkullakshya.workers.dev");
    // Use saved URL, but fall back to default if the saved URL is empty
    // (a previous save_settings call may have written an empty string).
    let saved_url = json["serverUrl"].as_str().unwrap_or(default_url);
    let server_url = if saved_url.is_empty() { default_url } else { saved_url };
    Ok(ServerConfig {
        server_url: server_url.to_string(),
        user_id: json["userId"].as_str().unwrap_or("").to_string(),
        device_id: json["deviceId"].as_str().unwrap_or("").to_string(),
    })
}

// ─── Feature 88: canonical laptop identity commands ──────────────

/// IPC: Claim the canonical profile from the Worker (setup Accounts step).
/// The Worker issues profile_id/device_id/device_token; the token goes to
/// the OS keyring. Idempotent — safe to call again on retry.
#[tauri::command]
pub async fn claim_profile<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::identity_state::IdentityStatus, String> {
    let status = crate::identity_state::claim(&app).await?;
    // Kick off the pending-approval poll loop (bounded).
    crate::identity_state::spawn_pending_poll(app);
    Ok(status)
}

/// IPC: Current identity state (provisional/pending/approved/suspended/revoked).
#[tauri::command]
pub fn get_identity_status<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::identity_state::IdentityStatus, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(crate::identity_state::identity_status(&dir))
}

/// IPC: Force-refresh identity state from the Worker (/v1/profiles/me).
#[tauri::command]
pub async fn refresh_identity_status<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<crate::identity_state::IdentityStatus, String> {
    crate::identity_state::refresh_status(&app).await
}

/// IPC: Self-revoke this device (settings Identity card "Disconnect").
#[tauri::command]
pub async fn disconnect_device<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    crate::identity_state::disconnect(&app).await
}

// ─── Voice profile commands ──────────────────────────────────────
// Phase D: Speaker verification is now available for both engines.
// - wakeword-sherpa: uses sherpa-onnx for embedding extraction (C++ deps)
// - wakeword-oww: uses the existing embedding_model.onnx (pure Rust)
//
// The OWW commands use the voice_profile module's SpeakerVerifier which
// extracts embeddings from the OWW embedding model via tract-onnx.
#[cfg(feature = "wakeword-sherpa")]
pub use voice_profile_commands::*;
#[cfg(feature = "wakeword-sherpa")]
mod voice_profile_commands {
    use super::*;

    /// IPC: Get the current voice profile status (enrolled or not, number of clips, threshold).
    #[tauri::command]
    pub fn get_voice_profile_status<R: Runtime>(
        app: tauri::AppHandle<R>,
    ) -> Result<voice_profile::VoiceProfileStatus, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let profile_path = voice_profile::resolve_profile_path(&dir);

        let sound_alikes: Vec<String> = voice_profile::SOUND_ALIKES
            .iter()
            .map(|s| s.to_string())
            .collect();

        if !profile_path.exists() {
            return Ok(voice_profile::VoiceProfileStatus {
                enrolled: false,
                num_clips: 0,
                threshold: voice_profile::DEFAULT_THRESHOLD,
                created_at: 0,
                updated_at: 0,
                wake_variants: vec!["nexus".to_string()],
                sound_alikes,
            });
        }

        let profile = voice_profile::VoiceProfile::load(&profile_path)
            .map_err(|e| e.to_string())?;
        Ok(voice_profile::VoiceProfileStatus {
            enrolled: true,
            num_clips: profile.num_clips,
            threshold: profile.threshold,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
            wake_variants: profile.wake_variants,
            sound_alikes,
        })
    }

    /// Resolve the sherpa resource directory (handles dev + production paths).
    pub fn resolve_sherpa_dir(resource_dir: &Path) -> Option<PathBuf> {
        // Production: resource_dir/resources/sherpa (Tauri v2 on Windows: resource_dir() = exe_dir)
        let sherpa = resource_dir.join("resources").join("sherpa");
        if sherpa.exists() {
            return Some(sherpa);
        }
        // Fallback: resource_dir/sherpa (some Tauri versions may return resources/ directly)
        let sherpa_alt = resource_dir.join("sherpa");
        if sherpa_alt.exists() {
            return Some(sherpa_alt);
        }
        if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
            let dev = PathBuf::from(manifest).join("resources").join("sherpa");
            if dev.exists() {
                return Some(dev);
            }
        }
        // Fallback: exe_dir/../resources/sherpa
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                let p = parent.join("../resources/sherpa");
                if p.exists() {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Run ASR on enrollment clips to capture wake-word variants.
    /// Returns the list of ASR transcripts (one per clip, may be empty/garbage).
    pub fn transcribe_enrollment_clips(
        sherpa_dir: &Path,
        clips: &[Vec<f32>],
    ) -> Result<Vec<String>, String> {
        use sherpa_onnx::{
            OnlineModelConfig, OnlineRecognizer, OnlineRecognizerConfig, OnlineTransducerModelConfig,
        };

        let kws_dir = sherpa_dir.join("kws");

        // Prefer int8 (quantized) models, fall back to fp32
        let encoder = kws_dir.join("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx");
        let encoder = if encoder.exists() { encoder } else { kws_dir.join("encoder-epoch-12-avg-2-chunk-16-left-64.onnx") };
        let decoder = kws_dir.join("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx");
        let decoder = if decoder.exists() { decoder } else { kws_dir.join("decoder-epoch-12-avg-2-chunk-16-left-64.onnx") };
        let joiner = kws_dir.join("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx");
        let joiner = if joiner.exists() { joiner } else { kws_dir.join("joiner-epoch-12-avg-2-chunk-16-left-64.onnx") };
        let tokens = kws_dir.join("tokens.txt");

        for (name, path) in [
            ("encoder", &encoder),
            ("decoder", &decoder),
            ("joiner", &joiner),
            ("tokens", &tokens),
        ] {
            if !path.exists() {
                return Err(format!("ASR model file '{}' not found at: {}", name, path.display()));
            }
        }

        let config = OnlineRecognizerConfig {
            model_config: OnlineModelConfig {
                transducer: OnlineTransducerModelConfig {
                    encoder: Some(encoder.to_string_lossy().to_string()),
                    decoder: Some(decoder.to_string_lossy().to_string()),
                    joiner: Some(joiner.to_string_lossy().to_string()),
                },
                tokens: Some(tokens.to_string_lossy().to_string()),
                num_threads: 1,
                provider: Some("cpu".to_string()),
                ..Default::default()
            },
            decoding_method: Some("greedy_search".to_string()),
            enable_endpoint: false,
            ..Default::default()
        };

        let recognizer = OnlineRecognizer::create(&config)
            .ok_or_else(|| "Failed to create OnlineRecognizer for enrollment".to_string())?;

        let mut variants = Vec::with_capacity(clips.len());

        for (i, clip) in clips.iter().enumerate() {
            if clip.is_empty() {
                variants.push(String::new());
                continue;
            }

            let stream = recognizer.create_stream();

            // Feed the clip + 0.5s tail padding
            stream.accept_waveform(16000, clip);
            let tail = vec![0.0f32; 8000];
            stream.accept_waveform(16000, &tail);
            stream.input_finished();

            while recognizer.is_ready(&stream) {
                recognizer.decode(&stream);
            }

            let text = if let Some(result) = recognizer.get_result(&stream) {
                result.text.trim().to_lowercase()
            } else {
                String::new()
            };

            tracing::info!("Enrollment clip {} ASR transcript: \"{}\"", i + 1, text);
            variants.push(text);

            recognizer.reset(&stream);
        }

        Ok(variants)
    }

    /// IPC: Enroll a voice profile from multiple audio clips.
    /// Each clip is a Vec<f32> of 16kHz mono audio samples.
    /// Also runs ASR on each clip to capture wake-word variants.
    /// Re-enrollment APPENDS new variants to existing ones (does not wipe).
    #[tauri::command]
    pub fn enroll_voice<R: Runtime>(
        app: tauri::AppHandle<R>,
        clips: Vec<Vec<f32>>,
        threshold: Option<f32>,
    ) -> Result<Vec<String>, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

        let profile_path = voice_profile::resolve_profile_path(&dir);

        let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
        let sherpa_dir = resolve_sherpa_dir(&resource_dir)
            .ok_or_else(|| "Sherpa resource directory not found".to_string())?;

        let speaker_model = sherpa_dir.join("speaker_model.onnx");

        let mut verifier = voice_profile::SpeakerVerifier::new(speaker_model, profile_path)
            .map_err(|e| e.to_string())?;

        // Run ASR on each clip to capture wake-word variants
        let asr_variants = transcribe_enrollment_clips(&sherpa_dir, &clips)
            .map_err(|e| {
                tracing::warn!("Enrollment ASR failed (continuing without variants): {e}");
                // Don't fail enrollment if ASR fails — just use empty variants
                vec![String::new(); clips.len()]
            })
            .unwrap_or_else(|_| vec![String::new(); clips.len()]);

        let threshold = threshold.unwrap_or(voice_profile::DEFAULT_THRESHOLD);
        verifier
            .enroll(&clips, threshold, asr_variants.clone())
            .map_err(|e| e.to_string())?;

        // Return the captured variants so the UI can show them
        let captured: Vec<String> = verifier
            .profile()
            .map(|p| p.wake_variants.clone())
            .unwrap_or_default();

        Ok(captured)
    }

    /// IPC: Delete the voice profile (disables speaker verification).
    #[tauri::command]
    pub fn delete_voice_profile<R: Runtime>(
        app: tauri::AppHandle<R>,
    ) -> Result<(), String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let profile_path = voice_profile::resolve_profile_path(&dir);

        if profile_path.exists() {
            std::fs::remove_file(&profile_path).map_err(|e| e.to_string())?;
            tracing::info!("Voice profile deleted");
        }
        Ok(())
    }
}

// ─── OWW Voice Profile Commands (Phase D) ──────────────────────────
// These commands work with the default wakeword-oww engine using the
// voice_profile module's SpeakerVerifier (pure Rust, no C++ deps).
// Enrollment extracts embeddings from the OWW embedding_model.onnx.
#[cfg(feature = "wakeword-oww")]
pub use oww_voice_profile_commands::*;
#[cfg(feature = "wakeword-oww")]
mod oww_voice_profile_commands {
    use super::*;

    /// IPC: Get the current voice profile status (enrolled or not, number of clips, threshold).
    #[tauri::command]
    pub fn get_voice_profile_status<R: Runtime>(
        app: tauri::AppHandle<R>,
    ) -> Result<voice_profile::VoiceProfileStatus, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let profile_path = voice_profile::resolve_profile_path(&dir);

        let sound_alikes: Vec<String> = voice_profile::SOUND_ALIKES
            .iter()
            .map(|s| s.to_string())
            .collect();

        if !profile_path.exists() {
            return Ok(voice_profile::VoiceProfileStatus {
                enrolled: false,
                num_clips: 0,
                threshold: voice_profile::DEFAULT_THRESHOLD,
                created_at: 0,
                updated_at: 0,
                wake_variants: vec!["nexus".to_string()],
                sound_alikes,
            });
        }

        let profile = voice_profile::VoiceProfile::load(&profile_path)
            .map_err(|e| e.to_string())?;
        Ok(voice_profile::VoiceProfileStatus {
            enrolled: true,
            num_clips: profile.num_clips,
            threshold: profile.threshold,
            created_at: profile.created_at,
            updated_at: profile.updated_at,
            wake_variants: profile.wake_variants,
            sound_alikes,
        })
    }

    /// IPC: Enroll the user's voice for speaker verification.
    /// Accepts 3+ audio clips (16kHz mono f32) and extracts embeddings
    /// using the OWW embedding model.
    #[tauri::command]
    pub fn enroll_voice<R: Runtime>(
        app: tauri::AppHandle<R>,
        clips: Vec<Vec<f32>>,
        threshold: Option<f32>,
    ) -> Result<Vec<String>, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let profile_path = voice_profile::resolve_profile_path(&dir);

        let resource_dir = app.path().resource_dir().map_err(|e| e.to_string())?;
        let oww_dir = resource_dir.join("oww");
        let embedding_model_path = oww_dir.join("embedding_model.onnx");
        let mel_model_path = oww_dir.join("melspectrogram.onnx");

        if !embedding_model_path.exists() {
            return Err("OWW embedding model not found".to_string());
        }

        // Load the OWW models for embedding extraction
        use tract_onnx::prelude::*;
        let mel_model = tract_onnx::onnx()
            .model_for_path(&mel_model_path)
            .map_err(|e| format!("Failed to load mel model: {e}"))?
            .into_optimized()
            .map_err(|e| format!("Failed to optimize mel model: {e}"))?
            .into_runnable()
            .map_err(|e| format!("Failed to make mel model runnable: {e}"))?;

        let embedding_model = tract_onnx::onnx()
            .model_for_path(&embedding_model_path)
            .map_err(|e| format!("Failed to load embedding model: {e}"))?
            .into_optimized()
            .map_err(|e| format!("Failed to optimize embedding model: {e}"))?
            .into_runnable()
            .map_err(|e| format!("Failed to make embedding model runnable: {e}"))?;

        // Extract embeddings from each clip
        let mut embeddings: Vec<Vec<f32>> = Vec::new();
        for (i, clip) in clips.iter().enumerate() {
            // Ensure 16kHz mono f32
            if clip.is_empty() {
                continue;
            }

            // Scale to int16 magnitude (OWW mel expects this)
            let scaled: Vec<f32> = clip.iter().map(|s| s * 32768.0).collect();

            // Run mel spectrogram
            let mel_input = Tensor::from_shape(&[1, scaled.len()], &scaled)
                .map_err(|e| format!("Mel input shape error: {e}"))?;
            let mel_output = mel_model.run(tvec!(mel_input.into()))
                .map_err(|e| format!("Mel model run error: {e}"))?;
            let mel_features = mel_output[0].clone().into_tensor();

            // Run embedding model
            let emb_output = embedding_model.run(tvec!(mel_features.into()))
                .map_err(|e| format!("Embedding model run error: {e}"))?;
            let emb_tensor = emb_output[0].clone().into_tensor();

            // Convert to plain array and average to get 96-dim embedding
            let emb_arr = emb_tensor.into_plain_array::<f32>()
                .map_err(|e| format!("Embedding array conversion error: {e}"))?;
            let emb_slice = emb_arr.as_slice().unwrap_or(&[]);

            // The embedding model outputs [1, 16, 96] — average over 16 frames
            let embedding: Vec<f32> = (0..96)
                .map(|j| {
                    (0..16)
                        .map(|i| {
                            let idx = i * 96 + j;
                            if idx < emb_slice.len() { emb_slice[idx] } else { 0.0 }
                        })
                        .sum::<f32>() / 16.0
                })
                .collect();

            embeddings.push(embedding.clone());
            tracing::debug!("Enrollment clip {} → embedding ({} dims)", i, embedding.len());
        }

        if embeddings.is_empty() {
            return Err("No valid embeddings extracted from clips".to_string());
        }

        // Create verifier and enroll
        let mut verifier = voice_profile::SpeakerVerifier::new(profile_path)
            .map_err(|e| e.to_string())?;
        let threshold = threshold.unwrap_or(voice_profile::DEFAULT_THRESHOLD);
        verifier
            .enroll(embeddings, threshold, vec!["nexus".to_string(), "hey nexus".to_string()])
            .map_err(|e| e.to_string())?;

        let captured: Vec<String> = verifier
            .profile()
            .map(|p| p.wake_variants.clone())
            .unwrap_or_default();

        tracing::info!("Voice profile enrolled ({} clips, threshold {:.2})", clips.len(), threshold);
        Ok(captured)
    }

    /// IPC: Delete the voice profile (disables speaker verification).
    #[tauri::command]
    pub fn delete_voice_profile<R: Runtime>(
        app: tauri::AppHandle<R>,
    ) -> Result<(), String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let profile_path = voice_profile::resolve_profile_path(&dir);

        if profile_path.exists() {
            std::fs::remove_file(&profile_path).map_err(|e| e.to_string())?;
            tracing::info!("Voice profile deleted");
        }
        Ok(())
    }
}

// ─── Meeting / privacy mode commands ─────────────────────────────────

/// IPC: Check whether TTS should be suppressed right now.
///
/// The frontend calls this before speaking to decide whether to
/// produce audible TTS or show a silent visual response instead.
///
/// Uses `should_suppress_tts()` (not `is_meeting_active()`) so that
/// disabling auto-detection in settings takes effect immediately.
/// `is_meeting_active()` only reports the raw detection flag, which the
/// polling loop clears up to 2s later — long enough for the user to
/// disable detection and still have their next response muted.
#[tauri::command]
pub fn meeting_active<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<bool, String> {
    let state = app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .ok_or_else(|| "meeting state not managed".to_string())?;
    Ok(state.should_suppress_tts())
}

/// IPC: Check if NEXUS is paused (manual pause via tray).
#[tauri::command]
pub fn is_nexus_paused<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<bool, String> {
    let state = app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .ok_or_else(|| "meeting state not managed".to_string())?;
    Ok(state.is_paused())
}

/// IPC: Get the full meeting/privacy mode status.
#[tauri::command]
pub fn meeting_status<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<MeetingStatus, String> {
    let state = app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .ok_or_else(|| "meeting state not managed".to_string())?;
    Ok(MeetingStatus {
        meeting_active: state.is_meeting_active(),
        paused: state.is_paused(),
        tts_playing: state.tts_playing.load(std::sync::atomic::Ordering::Relaxed),
        detection_enabled: state.detection_enabled.load(std::sync::atomic::Ordering::Relaxed),
    })
}

/// IPC: Enable or disable automatic meeting detection.
#[tauri::command]
pub fn set_meeting_detection<R: Runtime>(
    app: tauri::AppHandle<R>,
    enabled: bool,
) -> Result<(), String> {
    let state = app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .ok_or_else(|| "meeting state not managed".to_string())?;
    state.detection_enabled.store(enabled, std::sync::atomic::Ordering::Relaxed);
    tracing::info!("meeting detection: {}", if enabled { "enabled" } else { "disabled" });
    Ok(())
}

/// Serialized meeting status returned by `meeting_status`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MeetingStatus {
    pub meeting_active: bool,
    pub paused: bool,
    pub tts_playing: bool,
    pub detection_enabled: bool,
}

// ─── Response Sidebar window ─────────────────────────────────────────

/// Pending backdrop for the settings sidebar — stored when the window is
/// first shown, fetched by the frontend on mount (same pattern as
/// PENDING_SIDEBAR for the response sidebar). This handles the race
/// condition where the backdrop event is emitted before the React app
/// has mounted and registered its event listener.
static PENDING_SETTINGS_BACKDROP: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// IPC: Show the response sidebar window (positioned at bottom-right).
/// Called when a server response is incoming (n8n/Ollama/Hermes).
/// Creates the sidebar window on-demand if it doesn't exist (saves ~250 MB RAM at idle).
///
/// MUST be async — WebviewWindowBuilder::build() dispatches to the main thread,
/// and a synchronous command runs on a blocking thread that can't yield, causing
/// a deadlock. Async commands run on the tokio runtime which can properly yield.
#[tauri::command]
pub async fn show_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    // Unified window delegate — Assistant view, default right dock.
    let _ = unified_show_sidebar(&app, "assistant", None).await?;
    Ok(())
}

/// IPC: Show the sidebar AND set its content.
///
/// The sidebar window is created on-demand. Since the React app needs time
/// to load before it can receive Tauri events, we store the content + backdrop
/// in a static. The frontend calls `get_pending_sidebar_content` on mount,
/// which returns and clears the pending data. This is race-free.
///
/// If the window already exists (React already loaded), we also emit the
/// `sidebar:show` event as a fast path — the event listener will handle it
/// immediately without needing to poll the pending content.
#[tauri::command]
pub async fn show_sidebar_with_content<R: Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    text: String,
) -> Result<(), String> {
    let window_existed = app.get_webview_window("sidebar").is_some();

    // Unified delegate — Assistant view, right dock. prepare_sidebar
    // applies geometry WITHOUT showing, so the backdrop capture below
    // photographs the desktop, not the sidebar itself.
    let prep = prepare_sidebar(&app, "assistant", None).await?;
    let backdrop = capture_and_emit_backdrop(&app, &prep);

    // Store the pending content so the frontend can fetch it on mount.
    // This handles the fresh-window case where events would be missed.
    {
        let mut pending = PENDING_SIDEBAR.lock().unwrap();
        *pending = Some(PendingSidebar {
            query: query.clone(),
            text: text.clone(),
            backdrop: backdrop.clone(),
            analysis: None,
            confirmation: None,
        });
    }

    // Show + focus + emit sidebar:set_view (React switches view instantly).
    finish_sidebar(&app, &prep.win, "assistant", None)?;
    spawn_sidebar_live_blur(&app, &prep.win);

    // If the window already existed (React already loaded), also emit the
    // event as a fast path. The frontend listener will handle it immediately.
    if window_existed {
        let payload = serde_json::json!({
            "query": query,
            "text": text,
        });
        let _ = app.emit("sidebar:show", payload);
        // Also emit the backdrop if we captured one.
        if let Some(uri) = backdrop {
            let _ = app.emit("sidebar:backdrop", uri);
        }
    }

    tracing::info!("sidebar: shown with content (query={} chars, text={} chars, window_existed={})", query.len(), text.len(), window_existed);
    Ok(())
}

/// IPC: Show the sidebar with structured analysis data (rich dashboard).
/// Like `show_sidebar_with_content` but also stores the analysis JSON so
/// the frontend can render the AnalysisDashboard with pie charts.
#[tauri::command]
pub async fn show_sidebar_with_analysis<R: Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    text: String,
    analysis: serde_json::Value,
) -> Result<(), String> {
    let window_existed = app.get_webview_window("sidebar").is_some();

    // Unified delegate — Assistant view, right dock, geometry before capture.
    let prep = prepare_sidebar(&app, "assistant", None).await?;
    let backdrop = capture_and_emit_backdrop(&app, &prep);

    {
        let mut pending = PENDING_SIDEBAR.lock().unwrap();
        *pending = Some(PendingSidebar {
            query: query.clone(),
            text: text.clone(),
            backdrop: backdrop.clone(),
            analysis: Some(analysis.clone()),
            confirmation: None,
        });
    }

    finish_sidebar(&app, &prep.win, "assistant", None)?;
    spawn_sidebar_live_blur(&app, &prep.win);

    // Fast path: if the window already exists, also emit events
    if window_existed {
        let _ = app.emit("sidebar:show", serde_json::json!({
            "query": query,
            "text": text,
        }));
        let _ = app.emit("sidebar:analysis", serde_json::json!({
            "query": query,
            "text": text,
            "analysis": analysis,
        }));
        if let Some(uri) = backdrop {
            let _ = app.emit("sidebar:backdrop", uri);
        }
    }

    tracing::info!("sidebar: shown with analysis (query={} chars, text={} chars, window_existed={})", query.len(), text.len(), window_existed);
    Ok(())
}

/// IPC: Show the sidebar with an action confirmation dialog.
/// Stores the confirmation JSON so the frontend can render the ConfirmationPanel
/// with Confirm and Cancel buttons.
#[tauri::command]
pub async fn show_sidebar_with_confirmation<R: Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    prompt: String,
    confirmation: serde_json::Value,
) -> Result<(), String> {
    let window_existed = app.get_webview_window("sidebar").is_some();

    // Unified delegate — Assistant view, right dock, geometry before capture.
    let prep = prepare_sidebar(&app, "assistant", None).await?;
    let backdrop = capture_and_emit_backdrop(&app, &prep);

    {
        let mut pending = PENDING_SIDEBAR.lock().unwrap();
        *pending = Some(PendingSidebar {
            query: query.clone(),
            text: prompt.clone(),
            backdrop: backdrop.clone(),
            analysis: None,
            confirmation: Some(confirmation.clone()),
        });
    }

    finish_sidebar(&app, &prep.win, "assistant", None)?;
    spawn_sidebar_live_blur(&app, &prep.win);

    if window_existed {
        let _ = app.emit("sidebar:show", serde_json::json!({
            "query": query,
            "text": prompt,
        }));
        let _ = app.emit("sidebar:confirmation", serde_json::json!({
            "query": query,
            "prompt": prompt,
            "confirmation": confirmation,
        }));
        if let Some(uri) = backdrop {
            let _ = app.emit("sidebar:backdrop", uri);
        }
    }

    tracing::info!("sidebar: shown with confirmation (query={}, prompt={}, window_existed={})", query, prompt, window_existed);
    Ok(())
}

/// Store pending sidebar content from non-commands modules (the OCR
/// fallback in the orchestrator). Same race-free pattern; no backdrop.
pub fn set_pending_sidebar_text(query: String, text: String) {
    let mut pending = PENDING_SIDEBAR.lock().unwrap();
    *pending = Some(PendingSidebar {
        query,
        text,
        backdrop: None,
        analysis: None,
        confirmation: None,
    });
}

/// IPC: Fetch pending sidebar content (called by the frontend on mount).
/// Returns the content + backdrop + analysis + confirmation that was stored by
/// `show_sidebar_with_content`, `show_sidebar_with_analysis`, or
/// `show_sidebar_with_confirmation`, or null if no content is pending.
/// Clears the pending data after returning.
#[tauri::command]
pub fn get_pending_sidebar_content() -> Result<Option<serde_json::Value>, String> {    let mut pending = PENDING_SIDEBAR.lock().unwrap();
    let data = pending.take();
    match data {
        Some(p) => {
            tracing::info!("sidebar: pending content fetched (query={} chars, text={} chars, has_backdrop={}, has_analysis={}, has_confirmation={})", p.query.len(), p.text.len(), p.backdrop.is_some(), p.analysis.is_some(), p.confirmation.is_some());
            Ok(Some(serde_json::json!({
                "query": p.query,
                "text": p.text,
                "backdrop": p.backdrop,
                "analysis": p.analysis,
                "confirmation": p.confirmation,
            })))
        }
        None => Ok(None),
    }
}

/// Capture the desktop region behind the sidebar window (Windows only).
/// Must be called BEFORE `win.show()` so we don't capture the sidebar itself.
/// If `geom` is provided, uses those physical coords directly (avoids a race
/// where `win.outer_position()`/`inner_size()` haven't propagated yet).
/// Returns the blurred backdrop as a `data:image/jpeg;base64,...` URI, or None.
pub(crate) fn capture_backdrop<R: Runtime>(
    _app: &tauri::AppHandle<R>,
    win: &tauri::WebviewWindow<R>,
    geom: Option<(i32, i32, i32, i32)>,
) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let (x, y, phys_w, phys_h) = match geom {
            Some(g) => g,
            None => {
                let pos = win.outer_position().ok()?;
                let scale = win.scale_factor().ok()?;
                let logical = win.inner_size().ok()?.to_logical::<f64>(scale);
                (pos.x, pos.y, (logical.width * scale) as i32, (logical.height * scale) as i32)
            }
        };

        match crate::sidebar_backdrop::capture_and_blur_jpeg(x, y, phys_w, phys_h, 10.0) {
            Some(data_uri) => {
                tracing::info!("sidebar: backdrop captured ({} bytes)", data_uri.len());
                Some(data_uri)
            }
            None => {
                tracing::warn!("sidebar: backdrop capture failed (x={}, y={}, w={}, h={})", x, y, phys_w, phys_h);
                None
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (win, geom);
        None
    }
}

// ─── Unified dynamic sidebar (one window, four views) ────────────────
//
// ONE Tauri window ("sidebar") hosts every panel view — Assistant,
// Command Hub (settings), Architect, PR List — switched in React via
// `sidebar:set_view` without creating/destroying OS HWNDs. This kills
// the 4×WebView2 process cost (~250 MB each), the z-order fighting, and
// the spawn flicker, while the single compact HWND keeps native DWM
// hardware glass + rounded corners + capture exclusion fully active.
//
// Current view geometry (logical px):
//   assistant / pr-list : 520×min(1040, mh-40), right dock (x=mw-530, y=20)
//   settings            : 740×min(1040, mh-40), right dock (x=mw-750, y=20)
//   architect           : 960×min(1040, mh-40), centered (y=40)
// Explicit left/right/center docks override the default anchor per view.

/// The currently active sidebar view — kept so `set_sidebar_dock` can
/// re-anchor the window without the frontend re-sending the view name.
static ACTIVE_SIDEBAR_VIEW: Mutex<String> = Mutex::new(String::new());

/// Requested-but-possibly-lost view — the same race as pending sidebar
/// content: Rust emits `sidebar:set_view` the moment the window shows,
/// but the React router may not have mounted yet. Persist here; the
/// frontend fetches (and clears) it on mount so the FIRST show lands on
/// the right view.
static PENDING_SIDEBAR_VIEW: Mutex<Option<String>> = Mutex::new(None);

fn set_active_sidebar_view(view: &str) {
    *ACTIVE_SIDEBAR_VIEW.lock().unwrap() = view.to_string();
}

/// IPC: Fetch the pending sidebar view (router calls this on mount).
/// Returns the view name that was requested before the router mounted,
/// or null. Clears the pending value.
#[tauri::command]
pub fn get_pending_sidebar_view() -> Result<Option<String>, String> {
    let mut pending = PENDING_SIDEBAR_VIEW.lock().unwrap();
    Ok(pending.take())
}

// ─── Pending spatial analysis (Feature 86 race-free pattern) ─────────
// The orchestrator stores the full spatial payload BEFORE showing the
// unified sidebar; the Spatial view fetches it on mount (fresh-window
// race path) — same shape as PENDING_SIDEBAR.
static PENDING_SPATIAL: Mutex<Option<serde_json::Value>> = Mutex::new(None);

/// Store the spatial payload (called by the orchestrator).
pub fn set_pending_spatial(payload: &serde_json::Value) {
    *PENDING_SPATIAL.lock().unwrap() = Some(payload.clone());
}

/// IPC: Fetch pending spatial data (Spatial view calls this on mount).
/// Clears after returning.
#[tauri::command]
pub fn get_pending_spatial() -> Result<Option<serde_json::Value>, String> {
    let mut pending = PENDING_SPATIAL.lock().unwrap();
    Ok(pending.take())
}

// ─── Pending annotation seed (Feature 87 race-free pattern) ─────────
// Same shape as PENDING_SPATIAL: the orchestrator stores the seed canvas
// BEFORE showing the unified sidebar; the Annotate view fetches it on
// mount (fresh-window race path) and also listens for show_annotation
// (warm-window path).
static PENDING_ANNOTATION: Mutex<Option<serde_json::Value>> = Mutex::new(None);

/// Store the annotation seed (called by the orchestrator).
pub fn set_pending_annotation(payload: &serde_json::Value) {
    *PENDING_ANNOTATION.lock().unwrap() = Some(payload.clone());
}

/// IPC: Fetch pending annotation seed (Annotate view calls this on mount).
/// Clears after returning.
#[tauri::command]
pub fn get_pending_annotation() -> Result<Option<serde_json::Value>, String> {
    let mut pending = PENDING_ANNOTATION.lock().unwrap();
    Ok(pending.take())
}

/// Pure geometry resolver for the unified sidebar window.
/// Returns (x, y, width, height) in LOGICAL pixels for `view`/`dock` on a
/// monitor of logical size (monitor_w, monitor_h). Unit-tested — keep it
/// side-effect free.
pub fn sidebar_geometry(
    view: &str,
    dock: Option<&str>,
    monitor_w: f64,
    monitor_h: f64,
) -> (f64, f64, f64, f64) {
    let left = dock == Some("left");
    let right = dock == Some("right");
    let docked = left || right;
// Right dock: window sits inside the right screen edge with a 10px margin.
// x = monitor_w - w - 10. Left dock: 10px from left edge. y = 20px top
// margin for both. The right edge must never leave the screen.
    let dock_x = |w: f64, h: f64| -> (f64, f64, f64, f64) {
        let x = if left {
            10.0
        } else {
            (monitor_w - w - 10.0).max(0.0)
        };
        (x, 20.0, w, h)
    };
    let center_xy = |w: f64, h: f64| -> (f64, f64, f64, f64) {
        (
            ((monitor_w - w) / 2.0).max(0.0),
            ((monitor_h - h) / 2.0).max(0.0),
            w,
            h,
        )
    };
    match view {
        "settings" => {
            // Settings: full height (minus 40px top+bottom margin), 740px wide, right-docked
            let h = (monitor_h - 40.0).min(1080.0).max(400.0);
            if docked { dock_x(740.0, h) } else { dock_x(740.0, h) }
        }
        "architect" => {
            let h = (monitor_h - 40.0).min(1040.0).max(400.0);
            if docked {
                dock_x(960.0, h)
            } else {
                // Centered horizontally, pinned near the top (y=40, spec).
                let x = ((monitor_w - 960.0) / 2.0).max(0.0);
                (x, 40.0, 960.0, h)
            }
        }
        // "assistant" | "pr-list" | fallback → default right dock, full height.
        _ => {
            let h = (monitor_h - 40.0).min(1040.0).max(400.0);
            if dock == Some("center") { center_xy(520.0, h) } else { dock_x(520.0, h) }
        }
    }
}

/// One shared 1 FPS live-blur loop for the unified sidebar (replaces the
/// three per-view loops). Reads the window's ACTUAL position + size every
/// tick, so view switches and dock moves are tracked automatically with
/// zero extra wiring.
///
/// TEMPORARY: disabled. The post-show capture photographs the sidebar's own
/// transparent pixels — GDI BitBlt does not reliably honor
/// WDA_EXCLUDEFROMCAPTURE — producing black frames that paint the panel
/// pitch black (2026-10-01 live regression). Blur comes from the pre-show
/// `capture_backdrop` (ADR-05). A correct live loop needs DXGI desktop
/// duplication, which is separate future work.
pub(crate) fn spawn_sidebar_live_blur<R: Runtime>(app: &tauri::AppHandle<R>, win: &tauri::WebviewWindow<R>) {
    const DEV_LIVE_BLUR_DISABLED: bool = false; // 1 Hz live refresh enabled
    if DEV_LIVE_BLUR_DISABLED {
        return;
    }
    #[cfg(target_os = "windows")]
    {
        static LIVE_BLUR_ACTIVE: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        static LAST_FRAME_HASH: Mutex<Option<u64>> = Mutex::new(None);
        if LIVE_BLUR_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        LIVE_BLUR_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
        *LAST_FRAME_HASH.lock().unwrap() = None;

        let win_clone = win.clone();
        let app_clone = app.clone();
        tauri::async_runtime::spawn(async move {
            // Wait for window to fully appear
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            while win_clone.is_visible().unwrap_or(false) {
                // 4 FPS live blur — shared interval, see sidebar_backdrop.
                tokio::time::sleep(std::time::Duration::from_millis(
                    crate::sidebar_backdrop::LIVE_BLUR_INTERVAL_MS,
                ))
                .await;

                if !win_clone.is_visible().unwrap_or(false) {
                    break;
                }

                let scale = win_clone
                    .scale_factor()
                    .unwrap_or(1.0);
                let Ok(pos) = win_clone.outer_position() else { continue };
                let Ok(logical) = win_clone
                    .inner_size()
                    .map(|s| s.to_logical::<f64>(scale))
                else { continue };
                let phys_w = (logical.width * scale) as i32;
                let phys_h = (logical.height * scale) as i32;

                // Step 1: cheap raw capture for hashing (~1ms)
                let raw_bgra = match crate::sidebar_backdrop::capture_region_bgra_public(pos.x, pos.y, phys_w, phys_h) {
                    Some(bgra) => bgra,
                    None => continue,
                };

                // Step 2: hash and compare to previous frame
                let current_hash = crate::sidebar_backdrop::frame_hash(&raw_bgra);
                let mut prev_hash_guard = LAST_FRAME_HASH.lock().unwrap();
                let should_emit = match *prev_hash_guard {
                    Some(prev) => prev != current_hash,
                    None => true, // First frame after show — always emit
                };
                *prev_hash_guard = Some(current_hash);
                drop(prev_hash_guard);

                // Step 3: only run the pipeline if changed —
                // half-res fast blur keeps 4 FPS affordable.
                if should_emit {
                    if let Some(data_uri) = crate::sidebar_backdrop::blur_bgra_to_jpeg_fast(&raw_bgra, phys_w, phys_h, 32.0) {
                        let _ = app_clone.emit("sidebar:backdrop", data_uri);
                    }
                }
            }

            // Clean up: reset hash so next show captures fresh
            *LAST_FRAME_HASH.lock().unwrap() = None;
            LIVE_BLUR_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, win);
    }
}

/// Prepared unified-sidebar state: window + whether it was already on
/// screen (a visible window must NOT have its backdrop re-captured —
/// that would photograph the sidebar itself) + physical capture geometry.
pub(crate) struct SidebarPrepared<R: Runtime> {
    pub(crate) win: tauri::WebviewWindow<R>,
    pub(crate) already_visible: bool,
    pub(crate) capture_geom: Option<(i32, i32, i32, i32)>,
}

/// Get/create the unified sidebar window and apply the view's geometry
/// (logical size + position from `sidebar_geometry`). Does NOT show it.
/// MUST be awaited from an async context — `get_or_create_window` may
/// build a WebView, and `WebviewWindowBuilder::build()` dispatches to the
/// main thread (a sync command would deadlock there).
pub(crate) async fn prepare_sidebar<R: Runtime>(
    app: &tauri::AppHandle<R>,
    view: &str,
    dock: Option<&str>,
) -> Result<SidebarPrepared<R>, String> {
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::sidebar())?;
    let already_visible = win.is_visible().unwrap_or(false);

    if let Ok(Some(monitor)) = win.current_monitor().or_else(|_| win.primary_monitor()) {
        let scale = monitor.scale_factor();
        let mw = monitor.size().width as f64 / scale;
        let mh = monitor.size().height as f64 / scale;
        let (x, y, w, h) = sidebar_geometry(view, dock, mw, mh);
        let _ = win.set_size(tauri::LogicalSize::new(w, h));
        let _ = win.set_position(tauri::LogicalPosition::new(x, y));

        // ── Physical clamp (scale-mismatch drift fix) ──
        // Tauri converts a Logical position through the WINDOW's scale
        // factor, which can disagree with the monitor's (fresh windows
        // realize at 1.0 before DPI context attaches). That drift pushed
        // the right dock past the screen edge. Clamp the final PHYSICAL
        // rect into the monitor's physical bounds — correct on every
        // monitor/DPI combination regardless of which side drifted.
        let mon_w = monitor.size().width as i32;
        let mon_h = monitor.size().height as i32;
        let pw = (w * scale).round() as i32;
        let ph = (h * scale).round() as i32;
        if let Ok(cur) = win.outer_position() {
            let cx = cur.x.clamp(0, (mon_w - pw).max(0));
            let cy = cur.y.clamp(0, (mon_h - ph).max(0));
            if cx != cur.x || cy != cur.y {
                let _ = win.set_position(tauri::PhysicalPosition::new(cx, cy));
            }
        }
    }

    set_active_sidebar_view(view);
    // Persist for the router's fresh-window race path only after the window
    // has been prepared successfully.
    *PENDING_SIDEBAR_VIEW.lock().unwrap() = Some(view.to_string());

    // Compute physical capture geometry from the logical values we just set.
    // This avoids the race where win.outer_position()/inner_size() haven't
    // propagated yet when capture_backdrop runs immediately after.
    let capture_geom = if let Ok(Some(monitor)) = win.current_monitor().or_else(|_| win.primary_monitor()) {
        let scale = monitor.scale_factor();
        let mw = monitor.size().width as f64 / scale;
        let mh = monitor.size().height as f64 / scale;
        let (x, y, w, h) = sidebar_geometry(view, dock, mw, mh);
        let pw = (w * scale).round() as i32;
        let ph = (h * scale).round() as i32;
        let phys_x = ((x * scale).round() as i32).clamp(0, (monitor.size().width as i32 - pw).max(0));
        let phys_y = ((y * scale).round() as i32).clamp(0, (monitor.size().height as i32 - ph).max(0));
        Some((phys_x, phys_y, pw, ph))
    } else {
        None
    };

    Ok(SidebarPrepared { win, already_visible, capture_geom })
}

/// Show + focus + re-assert DWM corners (+ macOS vibrancy re-apply) +
/// emit `sidebar:set_view` so the React router switches instantly.
pub(crate) fn finish_sidebar<R: Runtime>(
    app: &tauri::AppHandle<R>,
    win: &tauri::WebviewWindow<R>,
    view: &str,
    dock: Option<&str>,
) -> Result<(), String> {
    win.show().map_err(|e| format!("sidebar show: {e}"))?;
    let _ = win.set_focus();
    crate::dwm_corners::round_corners(win);
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = apply_vibrancy(
            win,
            NSVisualEffectMaterial::Sidebar,
            Some(NSVisualEffectState::Active),
            Some(20.0),
        );
    }
    let _ = app.emit(
        "sidebar:set_view",
        serde_json::json!({ "view": view, "dock": dock }),
    );
    Ok(())
}

/// Capture the backdrop (if safe) and emit it as the fast-path
/// `sidebar:backdrop` event. Returns the captured URI so legacy commands
/// can also store it in their pending content (fresh-window race path).
/// When the window is already visible, returns None — capturing then
/// would photograph the sidebar itself.
pub(crate) fn capture_and_emit_backdrop<R: Runtime>(
    app: &tauri::AppHandle<R>,
    prep: &SidebarPrepared<R>,
) -> Option<String> {
    if prep.already_visible {
        return None;
    }
    let uri = capture_backdrop(app, &prep.win, prep.capture_geom);
    if let Some(u) = &uri {
        let _ = app.emit("sidebar:backdrop", u.clone());
    }
    uri
}

/// One-stop unified show: prepare → backdrop (currently disabled in
/// solid-black mode) → show/focus → set_view → live-blur loop (currently
/// disabled). Returns the captured backdrop for legacy pending
/// storage. Used by the new `show_sidebar_view` / `set_sidebar_dock`
/// commands and by every legacy show command below.
pub(crate) async fn unified_show_sidebar<R: Runtime>(
    app: &tauri::AppHandle<R>,
    view: &str,
    dock: Option<&str>,
) -> Result<Option<String>, String> {
    // `prepare_sidebar` persists the requested view for the router's
    // fresh-window race path and tracks the active view.
    let prep = prepare_sidebar(app, view, dock).await?;
    let backdrop = capture_and_emit_backdrop(app, &prep);
    finish_sidebar(app, &prep.win, view, dock)?;
    spawn_sidebar_live_blur(app, &prep.win);
    Ok(backdrop)
}

/// IPC: Show the unified sidebar in a specific view with an optional
/// dock anchor. `view`: "assistant" | "settings" | "architect" | "pr-list".
/// `dock`: Some("left"|"right"|"center") or None (view default).
#[tauri::command]
pub async fn show_sidebar_view<R: Runtime>(
    app: tauri::AppHandle<R>,
    view: String,
    dock: Option<String>,
) -> Result<(), String> {
    unified_show_sidebar(&app, &view, dock.as_deref())
        .await
        .map(|_| ())
}

/// IPC: Re-anchor the unified sidebar to a dock position ("left",
/// "right", or "center"/"float") keeping the currently active view.
#[tauri::command]
pub async fn set_sidebar_dock<R: Runtime>(
    app: tauri::AppHandle<R>,
    dock: String,
) -> Result<(), String> {
    let view = {
        let current = ACTIVE_SIDEBAR_VIEW.lock().unwrap();
        if current.is_empty() { "assistant".to_string() } else { current.clone() }
    };
    let dock_opt = match dock.as_str() {
        "center" | "float" | "" => None,
        other => Some(other.to_string()),
    };
    unified_show_sidebar(&app, &view, dock_opt.as_deref())
        .await
        .map(|_| ())
}

/// IPC: Hide the response sidebar window.
/// Called after the server response has been spoken.
#[tauri::command]
pub fn hide_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    // Destroy the unified sidebar window to free its ~250 MB WebView2 tree.
    let _ = crate::dyn_windows::destroy_window(&app, "sidebar");
    Ok(())
}

// ─── PR List view (unified sidebar) ──────────────────────────────────

/// IPC: Show the PR list in the unified sidebar window.
/// Right-dock geometry (520×980) comes from `prepare_sidebar`; the
/// backdrop capture + shared live-blur loop give the same liquid-glass
/// appearance as every other view.
#[tauri::command]
pub async fn show_pr_list_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let _ = unified_show_sidebar(&app, "pr-list", None).await?;
    Ok(())
}

/// IPC: Hide (destroy) the PR list sidebar window.
/// With the unified window this closes the whole sidebar panel.
#[tauri::command]
pub fn hide_pr_list_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let _ = crate::dyn_windows::destroy_window(&app, "sidebar");
    Ok(())
}

/// IPC: Show the Command Hub (settings view) in the unified sidebar.
/// Default geometry: center floating modal (740×min(900, mh-100)); users
/// can re-dock via the drag bar.
#[tauri::command]
pub async fn show_settings_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    // Unified delegate — Command Hub view, default center modal.
    let prep = prepare_sidebar(&app, "settings", None).await?;

    // Capture backdrop BEFORE showing (uses geometry we just set in prepare_sidebar).
    #[cfg(target_os = "windows")]
    {
        if let Some(data_uri) = capture_and_emit_backdrop(&app, &prep) {
            tracing::info!("settings-sidebar: backdrop captured ({} bytes)", data_uri.len());
            *PENDING_SETTINGS_BACKDROP.lock().unwrap() = Some(data_uri.clone());
        } else {
            tracing::warn!("settings-sidebar: backdrop capture failed or skipped (already_visible={})", prep.already_visible);
        }
    }

    finish_sidebar(&app, &prep.win, "settings", None)?;
    spawn_sidebar_live_blur(&app, &prep.win);

    Ok(())
}

/// IPC: Hide (destroy) the settings sidebar window.
/// With the unified window this closes the whole sidebar panel.
#[tauri::command]
pub fn hide_settings_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let _ = crate::dyn_windows::destroy_window(&app, "sidebar");
    Ok(())
}

/// IPC: Fetch the pending settings backdrop (called by the frontend on mount).
/// Returns the blurred desktop image as a data URI, or null if none pending.
/// Clears the pending data after returning. This handles the race condition
/// where the backdrop is captured before the React app has mounted.
#[tauri::command]
pub fn get_pending_settings_backdrop() -> Result<Option<String>, String> {
    let mut pending = PENDING_SETTINGS_BACKDROP.lock().unwrap();
    Ok(pending.take())
}

// ─── Loading indicator (stage-hosted, see orchestrator::show_loading/
// hide_loading and window_manager::emit_loading_rect) ──────────────────

// ─── Settings window + persistence ───────────────────────────────────

/// IPC: Open the settings window.
/// Creates the window on-demand if it doesn't exist (saves ~250 MB RAM at idle).
#[tauri::command]
pub fn open_settings_window<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let win = crate::dyn_windows::get_or_create_window(&app, crate::dyn_windows::WindowConfig::settings())?;
    win.show().map_err(|e| e.to_string())?;
    win.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// IPC: Close/hide the settings window.
#[tauri::command]
pub fn close_settings_window<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    // Destroy to free ~250 MB of WebView2 processes.
    let _ = crate::dyn_windows::destroy_window(&app, "settings");
    Ok(())
}

/// Serialized settings returned by `get_settings` and accepted by `save_settings`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NexusSettings {
    pub autostart: bool,
    pub hotkey: String,
    pub auto_hide_delay: u32,
    pub wake_word_enabled: bool,
    pub wake_phrase: String,
    pub wake_sensitivity: String,
    pub speaker_verification: bool,
    pub meeting_mode_auto: bool,
    pub suppress_tts_in_meetings: bool,
    pub local_stt_only: bool,
    pub server_url: String,
    pub user_id: String,
    pub device_id: String,
    pub tts_voice: String,
    pub speech_rate: f64,
    #[serde(default = "default_tts_provider")]
    pub tts_provider: String,
    /// TTS auto-volume: NEXUS sets system volume to this level (0-100)
    /// before speaking, then restores the original volume after.
    /// Default 75. Set to 0 to disable auto-volume.
    #[serde(default = "default_tts_volume")]
    pub tts_volume: u8,
    /// Groq API key for cloud STT (Whisper Large v3 Turbo).
    /// Free at console.groq.com — 2,000 requests/day per user.
    /// When empty, NEXUS uses local faster-whisper STT.
    #[serde(default)]
    pub groq_api_key: String,
    /// edge-tts voice name (Microsoft Neural, free, no API key).
    /// Default: en-US-AvaNeural. See full list at Microsoft Speech docs.
    #[serde(default = "default_edge_tts_voice")]
    pub edge_tts_voice: String,
    /// Iconic voice persona key (Feature 83 catalog, e.g. "jarvis").
    /// Default: "nexus". Written by set_voice_preference.
    #[serde(default = "default_selected_voice")]
    pub selected_voice: String,
    /// Local Piper twin model stem for the selected persona
    /// (e.g. "en_GB-alan-medium"). Default: bundled Amy twin.
    /// Written by set_voice_preference; resolved by the swap worker.
    #[serde(default = "default_offline_voice_model")]
    pub offline_voice_model: String,
    /// Orb horizontal position as percentage (0.0 = left, 0.5 = center, 1.0 = right).
    /// Default 0.5 (center). Saved to settings.json, persists across restarts.
    #[serde(default = "default_orb_horizontal_pct")]
    pub orb_horizontal_pct: f64,
    /// Orb vertical position as percentage (0.0 = top, 1.0 = bottom).
    /// Default 1.0 (bottom). Saved to settings.json, persists across restarts.
    #[serde(default = "default_orb_vertical_pct")]
    pub orb_vertical_pct: f64,
    /// Orb window size in pixels (100-300).
    /// Default 200. Saved to settings.json, persists across restarts.
    #[serde(default = "default_orb_size")]
    pub orb_size: u32,
    /// Gemini API key for Google Gemini models.
    /// When empty, Gemini features are unavailable.
    #[serde(default)]
    pub gemini_api_key: String,
    /// Cerebras API key for Cerebras-hosted Llama models.
    /// Free at cloud.cerebras.ai — 1M tokens/day. Fastest inference (~80ms).
    /// When empty, Cerebras is skipped in the 9Router cascade.
    #[serde(default)]
    pub cerebras_api_key: String,
    /// Moonshine STT model architecture.
    /// Options: "small_streaming" (123M, 7.84% WER, default for family),
    ///          "medium_streaming" (245M, 6.65% WER, recommended for admin),
    ///          "tiny_streaming" (34M, 12% WER, ultra-low RAM).
    /// Default: "medium_streaming" (better accuracy, admin has RAM).
    #[serde(default = "default_moonshine_model")]
    pub moonshine_model: String,
    /// Stage-2 wake-word verifier: on a stage-1 (acoustic) candidate, run the
    /// buffered audio through STT and wake only if the transcript contains
    /// "nexus". Kills TV/conversation false wakes at +~250ms latency.
    /// Default: true. Set false in settings.json to restore fire-on-detect.
    #[serde(default = "default_verify_wake")]
    pub verify_wake: bool,
    /// Mic keep-alive render (Meet parity): inaudible output stream beside
    /// capture holds the Intel DSP awake so it never power-gates the mic
    /// path mid-session. Default: true. Silent zeros, ~0 CPU.
    #[serde(default = "default_mic_keep_alive")]
    pub mic_keep_alive: bool,
    /// TTS voice emotion: "auto" (default, heuristics per reply),
    /// "neutral", "cheerful", "calm", "sad", "urgent", "whisper".
    /// Prosody mapped to edge-tts rate/pitch/volume.
    #[serde(default = "default_tts_emotion")]
    pub tts_emotion: String,
    /// Ghost vision provider order: "auto" (default, Gemini — sole vision
    /// engine since Groq decommissioned vision 2026-10-01), "groq"
    /// (kept for forward-compat; currently always fails), or "gemini".
    #[serde(default = "default_vision_provider")]
    pub vision_provider: String,
    /// Ghost vision strategy: "sequential" (default, quota-frugal) or
    /// "speed" (race both providers in parallel, first valid wins —
    /// halves worst-case latency, spends 2 quota units per miss).
    #[serde(default = "default_vision_race")]
    pub vision_race: String,
    /// Telegram owner chat id for the owner-only remote bridge (see
    /// telegram.rs). Empty = bridge off. The bot token itself lives in the
    /// vault ("telegram" service), never here. Set from the Connections tab.
    #[serde(default)]
    pub telegram_chat_id: String,
    /// Ghost FIFO queue: speak "Queued, sir." once per session when queue
    /// depth reaches 2+. Default: true. Plans D5/D6.
    #[serde(default = "default_ghost_depth_ack")]
    pub ghost_depth_ack: bool,
    /// Ghost FIFO queue: per-step watchdog timeout in ms. Default: 15000. D7.
    #[serde(default = "default_ghost_step_timeout_ms")]
    pub ghost_step_timeout_ms: u64,
    /// Ghost FIFO queue: inter-command gap τ in ms. Default: 1000. D4.
    #[serde(default = "default_ghost_turn_gap_ms")]
    pub ghost_turn_gap_ms: u64,
    /// Ghost waves placement (ghost sessions reposition the orb window to
    /// this rect — the waves own the visual then). Defaults = orb defaults.
    #[serde(default)]
    pub waves_horizontal_pct: f64,
    #[serde(default = "default_waves_vertical_pct")]
    pub waves_vertical_pct: f64,
    #[serde(default = "default_waves_size")]
    pub waves_size: u32,
    /// Loading indicator placement (center-anchored fractions + logical px).
    /// Defaults ≈ the historical top-right corner.
    #[serde(default = "default_loading_horizontal_pct")]
    pub loading_horizontal_pct: f64,
    #[serde(default = "default_loading_vertical_pct")]
    pub loading_vertical_pct: f64,
    #[serde(default = "default_loading_size")]
    pub loading_size: u32,
}

fn default_tts_provider() -> String {
    "kokoro".to_string()
}

fn default_tts_volume() -> u8 {
    75
}

fn default_edge_tts_voice() -> String {
    "en-US-AvaNeural".to_string()
}

fn default_orb_horizontal_pct() -> f64 {
    0.5
}

fn default_orb_vertical_pct() -> f64 {
    1.0
}

fn default_orb_size() -> u32 {
    200
}

fn default_moonshine_model() -> String {
    "medium_streaming".to_string()
}

fn default_verify_wake() -> bool {
    true
}

fn default_mic_keep_alive() -> bool {
    true
}

fn default_tts_emotion() -> String {
    "auto".to_string()
}

fn default_selected_voice() -> String {
    crate::voice_catalog::DEFAULT_VOICE_KEY.to_string()
}

fn default_offline_voice_model() -> String {
    "en_US-amy-medium".to_string()
}

fn default_vision_provider() -> String {
    "auto".to_string()
}

fn default_vision_race() -> String {
    "sequential".to_string()
}

fn default_ghost_depth_ack() -> bool {
    true
}

fn default_ghost_step_timeout_ms() -> u64 {
    15000
}

fn default_ghost_turn_gap_ms() -> u64 {
    1000
}

fn default_waves_vertical_pct() -> f64 {
    1.0
}

fn default_waves_size() -> u32 {
    200
}

fn default_loading_horizontal_pct() -> f64 {
    0.95
}

fn default_loading_vertical_pct() -> f64 {
    0.05
}

fn default_loading_size() -> u32 {
    80
}

impl Default for NexusSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            hotkey: "Ctrl+Space".to_string(),
            auto_hide_delay: 8,
            wake_word_enabled: true,
            wake_phrase: "NEXUS".to_string(),
            wake_sensitivity: "medium".to_string(),
            speaker_verification: false,
            meeting_mode_auto: true,
            suppress_tts_in_meetings: true,
            local_stt_only: true,
            server_url: option_env!("NEXUS_SERVER_URL")
                .unwrap_or("https://nexus-worker.chitkullakshya.workers.dev")
                .to_string(),
            user_id: String::new(),
            device_id: String::new(),
            tts_voice: "af_sky".to_string(),
            speech_rate: 1.15,
            tts_provider: "kokoro".to_string(),
            tts_volume: 75,
            groq_api_key: String::new(),
            edge_tts_voice: "en-US-AvaNeural".to_string(),
            selected_voice: default_selected_voice(),
            offline_voice_model: default_offline_voice_model(),
            orb_horizontal_pct: 0.5,
            orb_vertical_pct: 1.0,
            orb_size: 200,
            gemini_api_key: String::new(),
            cerebras_api_key: String::new(),
            moonshine_model: "medium_streaming".to_string(),
            tts_emotion: "auto".to_string(),
            vision_provider: "auto".to_string(),
            vision_race: "sequential".to_string(),
            verify_wake: true,
            mic_keep_alive: true,
            telegram_chat_id: String::new(),
            ghost_depth_ack: true,
            ghost_step_timeout_ms: 15000,
            ghost_turn_gap_ms: 1000,
            waves_horizontal_pct: 0.5,
            waves_vertical_pct: 1.0,
            waves_size: 200,
            loading_horizontal_pct: 0.95,
            loading_vertical_pct: 0.05,
            loading_size: 80,
        }
    }
}

/// TTS voice metadata returned by `list_tts_voices`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TtsVoiceInfo {
    pub id: String,
    pub name: String,
    pub gender: String,
    pub provider: String,
    pub language: String,
}

/// IPC: List all available TTS voices (Edge TTS cloud + Piper local).
/// Returns a curated list of en-US voices. The full Edge TTS catalog has
/// 400+ voices across 140+ locales — we surface the most useful en-US ones
/// to keep the settings UI manageable.
#[tauri::command]
pub fn list_tts_voices() -> Result<Vec<TtsVoiceInfo>, String> {
    let edge_voices = [
        ("en-US-AvaNeural",        "Ava",       "Female"),
        ("en-US-AndrewNeural",     "Andrew",    "Male"),
        ("en-US-AndrewMultilingualNeural", "Andrew ML", "Male"),
        ("en-US-EmmaNeural",       "Emma",      "Female"),
        ("en-US-BrianNeural",      "Brian",     "Male"),
        ("en-US-ChristopherNeural","Christopher","Male"),
        ("en-US-EricNeural",       "Eric",      "Male"),
        ("en-US-GuyNeural",        "Guy",       "Male"),
        ("en-US-JennyNeural",      "Jenny",     "Female"),
        ("en-US-MichelleNeural",   "Michelle",  "Female"),
        ("en-US-RogerNeural",      "Roger",     "Male"),
        ("en-US-SteffanNeural",    "Steffan",   "Male"),
        ("en-US-AriaNeural",       "Aria",      "Female"),
        ("en-US-DavisNeural",      "Davis",     "Male"),
        ("en-US-NancyNeural",      "Nancy",     "Female"),
        ("en-US-SaraNeural",       "Sara",      "Female"),
        ("en-US-NoraNeural",       "Nora",      "Female"),
        ("en-US-AmberNeural",      "Amber",     "Female"),
        ("en-US-AshleyNeural",     "Ashley",    "Female"),
        ("en-US-BrandonNeural",    "Brandon",   "Male"),
        ("enUS-CoraNeural",        "Cora",      "Female"),
        ("en-US-ElizabethNeural",  "Elizabeth", "Female"),
        ("en-US-MonicaNeural",     "Monica",    "Female"),
        ("en-US-SoniaNeural",      "Sonia",     "Female"),
    ];

    let mut voices: Vec<TtsVoiceInfo> = edge_voices
        .iter()
        .map(|(id, name, gender)| TtsVoiceInfo {
            id: id.to_string(),
            name: name.to_string(),
            gender: gender.to_string(),
            provider: "edge-tts".to_string(),
            language: "en-US".to_string(),
        })
        .collect();

    // Add local Piper voice (always available offline)
    voices.push(TtsVoiceInfo {
        id: "piper-amy".to_string(),
        name: "Amy (Offline)".to_string(),
        gender: "Female".to_string(),
        provider: "piper".to_string(),
        language: "en-US".to_string(),
    });

    Ok(voices)
}

/// IPC: Get the current settings (merged with defaults for missing fields).
#[tauri::command]
pub fn get_settings<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<NexusSettings, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    if !path.exists() {
        return Ok(NexusSettings::default());
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut settings: NexusSettings = serde_json::from_str(&content)
        .unwrap_or_default();
    // Merge server config if present
    let config_path = dir.join("nexus-config.json");
    if config_path.exists() {
        if let Ok(config) = std::fs::read_to_string(&config_path) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&config) {
                let default_url = option_env!("NEXUS_SERVER_URL")
                    .unwrap_or("https://nexus-worker.chitkullakshya.workers.dev");
                if let Some(url) = json.get("serverUrl").and_then(|v| v.as_str()) {
                    // Don't overwrite with empty string — keep default
                    settings.server_url = if url.is_empty() { default_url.to_string() } else { url.to_string() };
                }
                if let Some(uid) = json.get("userId").and_then(|v| v.as_str()) {
                    settings.user_id = uid.to_string();
                }
                if let Some(did) = json.get("deviceId").and_then(|v| v.as_str()) {
                    settings.device_id = did.to_string();
                }
            }
        }
    }
    Ok(settings)
}

/// IPC: Save settings to disk.
#[tauri::command]
pub fn save_settings<R: Runtime>(
    app: tauri::AppHandle<R>,
    settings: NexusSettings,
) -> Result<(), String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;

    // Also save server config separately — but NEVER overwrite an existing
    // identity (user_id / device_id) with empty values. This prevents
    // identity loss if settings.json is stale or nexus-config.json was
    // temporarily missing.
    let config_path = dir.join("nexus-config.json");
    let default_url = option_env!("NEXUS_SERVER_URL")
        .unwrap_or("https://nexus-worker.chitkullakshya.workers.dev");

    // Read existing config to preserve identity if needed
    let existing = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok());

    let (server_url, user_id, device_id) = if let Some(ref existing) = existing {
        let preserved_url = if settings.server_url.is_empty() {
            existing["serverUrl"].as_str().unwrap_or(default_url).to_string()
        } else {
            settings.server_url.clone()
        };
        let preserved_uid = if settings.user_id.is_empty() {
            existing["userId"].as_str().unwrap_or("").to_string()
        } else {
            settings.user_id.clone()
        };
        let preserved_did = if settings.device_id.is_empty() {
            existing["deviceId"].as_str().unwrap_or("").to_string()
        } else {
            settings.device_id.clone()
        };
        (preserved_url, preserved_uid, preserved_did)
    } else {
        // No existing config — use what we have, with URL fallback
        let url = if settings.server_url.is_empty() { default_url.to_string() } else { settings.server_url.clone() };
        (url, settings.user_id.clone(), settings.device_id.clone())
    };

    let config = serde_json::json!({
        "serverUrl": server_url,
        "userId": user_id,
        "deviceId": device_id,
    });
    std::fs::write(&config_path, config.to_string()).map_err(|e| e.to_string())?;
    tracing::info!("settings saved to {:?}", path);
    Ok(())
}

/// Read the Groq API key from settings.json (non-IPC helper for stt.rs).
/// Returns empty string if no key is set or settings file doesn't exist.
pub fn read_groq_api_key<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return String::new(); };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return String::new(); };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return String::new(); };
    // Read groqApiKey (frontend camelCase) with fallback to groq_api_key
    // (snake_case, used by manual settings.json edits or older versions).
    json.get("groqApiKey")
        .or_else(|| json.get("groq_api_key"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Read the localSttOnly flag from settings.json (non-IPC helper for stt.rs).
/// Returns true if the user has enabled "Local STT only" (privacy mode —
/// audio never leaves the device). Returns false if not set or file missing.
pub fn read_local_stt_only<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return false; };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return false; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return false; };
    json.get("localSttOnly")
        .or_else(|| json.get("local_stt_only"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

/// Read an API key by service name: keychain first, settings.json fallback.
/// Used by ghost.rs (vision keys) and the health panel. `service` is the
/// vault service id ("groq" | "gemini" | "cerebras").
pub fn read_api_key<R: tauri::Runtime>(app: &tauri::AppHandle<R>, service: &str) -> String {
    if let Some(k) = crate::auth_vault::get_api_key(service) {
        if !k.is_empty() {
            return k;
        }
    }
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return String::new(); };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else {
        return String::new();
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return String::new();
    };
    let camel = match service {
        "gemini" => "geminiApiKey",
        "cerebras" => "cerebrasApiKey",
        _ => "groqApiKey",
    };
    let snake = match service {
        "gemini" => "gemini_api_key",
        "cerebras" => "cerebras_api_key",
        _ => "groq_api_key",
    };
    json.get(camel)
        .or_else(|| json.get(snake))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Read the verifyWake flag from settings.json (non-IPC helper for wakeword_oww.rs).
/// Stage-2 verifier: cross-check stage-1 acoustic candidates with STT before
/// firing the wake. Defaults to TRUE (fail-open default: verification on).
/// Set `"verifyWake": false` in settings.json to restore fire-on-detect.
pub fn read_verify_wake<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return true; };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return true; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return true; };
    json.get("verifyWake")
        .or_else(|| json.get("verify_wake"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the micKeepAlive flag from settings.json (non-IPC helper for wakeword_oww.rs).
/// Keep-alive render: inaudible output stream holds the Intel DSP awake.
/// Defaults to TRUE. Set `"micKeepAlive": false` to disable.
pub fn read_mic_keep_alive<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return true; };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return true; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return true; };
    json.get("micKeepAlive")
        .or_else(|| json.get("mic_keep_alive"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the selected iconic voice persona key from settings.json
/// (Feature 83 catalog key, e.g. "jarvis"). Defaults to "nexus".
pub fn read_selected_voice<R: Runtime>(app: &tauri::AppHandle<R>) -> String {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return default_selected_voice() };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return default_selected_voice() };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return default_selected_voice() };
    let key = json.get("selectedVoice")
        .or_else(|| json.get("selected_voice"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if crate::voice_catalog::find_by_key(key).is_some() {
        key.to_string()
    } else {
        default_selected_voice()
    }
}

/// Read the edge-tts voice from settings.json (non-IPC helper for tts.rs).
/// Returns default voice if not set.
pub fn read_edge_tts_voice(app: &tauri::AppHandle) -> String {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return "en-US-AvaNeural".to_string(); };
    let path = dir.join("settings.json");
    let Ok(content) = std::fs::read_to_string(&path) else { return "en-US-AvaNeural".to_string(); };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return "en-US-AvaNeural".to_string(); };
    json.get("edgeTtsVoice")
        .or_else(|| json.get("edge_tts_voice"))
        .and_then(|v| v.as_str())
        .unwrap_or("en-US-AvaNeural")
        .to_string()
}

// ─── Autostart management ───────────────────────────────────────────
//
// Windows: uses a Scheduled Task named "NEXUS" with AtLogOn trigger.
// macOS/Linux: uses tauri-plugin-autostart (LaunchAgent / systemd).

/// IPC: Enable or disable auto-start at login.
/// On Windows, creates/removes the "NEXUS" Scheduled Task.
/// On macOS/Linux, calls the autostart plugin enable/disable.
#[tauri::command]
pub fn set_autostart<R: Runtime>(
    app: tauri::AppHandle<R>,
    enabled: bool,
) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = &app; // unused on Windows — autostart uses PowerShell directly
        let exe_path = std::env::current_exe()
            .map(|p| p.to_string_lossy().to_string())
            .map_err(|e| e.to_string())?;

        if enabled {
            // Create/update scheduled task with --background flag for silent start
            let ps_script = format!(
                r#"$exe = '{}';
                $user = [Security.Principal.WindowsIdentity]::GetCurrent().Name;
                $action = New-ScheduledTaskAction -Execute $exe -Argument '--background';
                $trigger = New-ScheduledTaskTrigger -AtLogOn -User $user;
                $settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Seconds 0);
                $result = Register-ScheduledTask -TaskName 'NEXUS' -Action $action -Trigger $trigger -Settings $settings -User $user -Force;
                if ($result) {{ Write-Output 'NEXUS_TASK_OK' }} else {{ Write-Output 'NEXUS_TASK_FAIL' }}"#,
                exe_path
            );
            let result = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output();
            match result {
                Ok(out) if out.status.success()
                    && String::from_utf8_lossy(&out.stdout).contains("NEXUS_TASK_OK") =>
                {
                    tracing::info!("autostart: scheduled task 'NEXUS' created (AtLogOn, --background)");
                    Ok(true)
                }
                Ok(out) => {
                    tracing::warn!(
                        "autostart: Register-ScheduledTask failed: stdout={} stderr={}",
                        String::from_utf8_lossy(&out.stdout).trim(),
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                    Err("Failed to create scheduled task".to_string())
                }
                Err(e) => Err(format!("PowerShell error: {e}")),
            }
        } else {
            // Remove the scheduled task
            let result = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command",
                    "Unregister-ScheduledTask -TaskName 'NEXUS' -Confirm:$false"])
                .creation_flags(0x08000000)
                .output();
            match result {
                Ok(out) if out.status.success() => {
                    tracing::info!("autostart: scheduled task 'NEXUS' removed");
                    Ok(false)
                }
                Ok(out) => {
                    // Task may not exist — that's fine, it's already disabled
                    tracing::info!(
                        "autostart: Unregister-ScheduledTask completed (stderr={})",
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                    Ok(false)
                }
                Err(e) => Err(format!("PowerShell error: {e}")),
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let autostart = app.autolaunch();
        if enabled {
            autostart.enable().map_err(|e| e.to_string())?;
            tracing::info!("autostart: enabled (LaunchAgent)");
        } else {
            autostart.disable().map_err(|e| e.to_string())?;
            tracing::info!("autostart: disabled");
        }
        Ok(enabled)
    }
}

/// IPC: Check if auto-start is currently enabled.
#[tauri::command]
pub fn is_autostart_enabled<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let _ = app; // unused on Windows
        let result = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "Get-ScheduledTask -TaskName 'NEXUS' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty State"])
            .creation_flags(0x08000000)
            .output();
        match result {
            Ok(out) => {
                let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let enabled = state == "Ready" || state == "Running";
                Ok(enabled)
            }
            Err(e) => {
                tracing::warn!("is_autostart_enabled: {e}");
                Ok(false)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let autostart = app.autolaunch();
        Ok(autostart.is_enabled().unwrap_or(false))
    }
}

// ─── Microphone permission ──────────────────────────────────────────
//
// On Windows, desktop (Win32) apps don't need per-app mic permission like UWP
// apps do. However, the GLOBAL mic privacy toggle (Settings → Privacy →
// Microphone → "Allow apps to access your microphone") can block all apps.
// When that's off, cpal returns an empty device list or fails to build a
// stream. We probe the default input device to detect this state.

/// IPC: Check if the microphone is accessible.
///
/// Probes cpal's default input device. Returns:
///   "granted"   — device found and stream can be built
///   "denied"    — no input devices or stream build failed (likely mic privacy off)
///   "no_device" — no input devices at all (no mic connected)
#[tauri::command]
pub fn check_mic_permission() -> String {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();

    // Check if there are any input devices at all
    let devices = match host.input_devices() {
        Ok(d) => d.collect::<Vec<_>>(),
        Err(e) => {
            tracing::warn!("check_mic_permission: input_devices() failed: {e}");
            return "denied".to_string();
        }
    };

    if devices.is_empty() {
        tracing::warn!("check_mic_permission: no input devices found");
        return "no_device".to_string();
    }

    // Try the default device
    match host.default_input_device() {
        Some(device) => {
            let dev_name = device.name().unwrap_or_else(|_| "unknown".into());
            tracing::info!("check_mic_permission: probing device '{}'", dev_name);

            // Try to build a minimal stream config to verify access
            let default_config = device.default_input_config();
            match default_config {
                Ok(_config) => {
                    // If we can get a default config, the device is accessible.
                    // Actually building a stream would require a callback and
                    // play() call, which is heavy for a permission check.
                    // Getting the default config is sufficient — if mic privacy
                    // is off, this returns an error on Windows.
                    tracing::info!("check_mic_permission: granted (device '{}')", dev_name);
                    "granted".to_string()
                }
                Err(e) => {
                    tracing::warn!("check_mic_permission: default_input_config failed: {e}");
                    "denied".to_string()
                }
            }
        }
        None => {
            tracing::warn!("check_mic_permission: no default input device");
            "no_device".to_string()
        }
    }
}

/// IPC: Open the OS microphone privacy settings.
///
/// On Windows, opens `ms-settings:privacy-microphone`.
/// On macOS, opens System Settings → Microphone.
/// On Linux, this is a no-op (PipeWire/PulseAudio handle permissions differently).
#[tauri::command]
pub fn open_mic_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "ms-settings:privacy-microphone"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .spawn()
            .map_err(|e| e.to_string())?;
        tracing::info!("opened Windows mic privacy settings");
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"])
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        // No standard mic privacy settings on most Linux distros
        tracing::info!("open_mic_settings: no-op on Linux");
    }

    Ok(())
}

/// IPC: Clear the conversation transcript (frontend store).
/// This is a no-op on the Rust side — the frontend handles it.
/// The command exists so the settings UI can call it via IPC.
#[tauri::command]
pub fn clear_transcript() -> Result<(), String> {
    tracing::info!("transcript cleared (frontend-side)");
    Ok(())
}

/// Force a manual app registry refresh (e.g. after installing a new app).
/// Scans the OS for installed apps and updates the cache immediately.
#[tauri::command]
pub fn refresh_app_registry() -> Result<String, String> {
    tracing::info!("manual app registry refresh requested");
    app_registry::force_refresh();
    Ok("App registry refreshed".to_string())
}

/// IPC: Pause the wake-word engine's cpal stream.
/// Called by the frontend before acquiring the microphone via getUserMedia()
/// to avoid OS-level mic lock contention (Intel Smart Sound Technology).
#[tauri::command]
pub fn pause_wakeword() -> Result<(), String> {
    crate::wakeword_oww::pause_stream();
    Ok(())
}

/// IPC: Resume the wake-word engine's cpal stream.
/// Called by the frontend after releasing the microphone so the wake-word
/// engine can resume listening for "NEXUS".
#[tauri::command]
pub fn resume_wakeword() -> Result<(), String> {
    crate::wakeword_oww::resume_stream();
    Ok(())
}

/// IPC: Mic self-test — is the mic healthy, quiet, or dead?
/// Analyzes the live 2.5s verify ring (no new stream opened, SST-safe).
/// Returns peak level, exact-zero ratio, and a verdict string:
/// "healthy" | "quiet-room" | "dead-silence".
#[tauri::command]
pub fn mic_self_test() -> Result<crate::wakeword_oww::MicSelfTestReport, String> {
    Ok(crate::wakeword_oww::mic_self_test_data())
}

/// Start Rust-side STT capture from the cpal stream.
/// Called by the frontend when the user wakes NEXUS or when retrying
/// after an empty transcript. The cpal stream captures audio directly —
/// no getUserMedia, no baton pass. This fixes the Intel SST driver issue
/// where getUserMedia returns silence but cpal is still working.
#[tauri::command]
pub fn start_stt_capture() -> Result<(), String> {
    crate::wakeword_oww::start_stt_capture();
    Ok(())
}

/// Abort an in-flight Rust-side STT capture (hotkey second-press cancel).
/// Returns whether voice was already underway + capture length so the
/// frontend can decide: hide the orb (no speech) vs let the turn finish.
#[tauri::command]
pub fn stop_stt_capture() -> Result<crate::wakeword_oww::SttAbortResult, String> {
    Ok(crate::wakeword_oww::abort_stt_capture())
}

/// Non-destructive voice check (no-input timeout): true once real voice
/// chunks landed. No state touched — slow-starter turns stay intact.
#[tauri::command]
pub fn stt_capture_had_speech() -> Result<bool, String> {
    Ok(crate::wakeword_oww::stt_capture_had_speech())
}

// ─── Ghost FIFO queue readers (loose settings.json parse) ───────────────

/// Read the ghostDepthAck flag (ghost FIFO queue depth-ACK, D5/D6).
pub fn read_ghost_depth_ack<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return true; };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else { return true; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return true; };
    json.get("ghostDepthAck")
        .or_else(|| json.get("ghost_depth_ack"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true)
}

/// Read the ghostStepTimeoutMs per-step watchdog value (D7). Floor 1000ms.
pub fn read_ghost_step_timeout_ms<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> u64 {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return 15000; };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else { return 15000; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return 15000; };
    json.get("ghostStepTimeoutMs")
        .or_else(|| json.get("ghost_step_timeout_ms"))
        .and_then(|v| v.as_u64())
        .filter(|v| *v >= 1000)
        .unwrap_or(15000)
}

/// Read the ghostTurnGapMs inter-step gap τ (D4). Ceiling 5000ms.
pub fn read_ghost_turn_gap_ms<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> u64 {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return 1000; };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else { return 1000; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return 1000; };
    json.get("ghostTurnGapMs")
        .or_else(|| json.get("ghost_turn_gap_ms"))
        .and_then(|v| v.as_u64())
        .filter(|v| *v <= 5000)
        .unwrap_or(1000)
}

/// Read the speakerVerification flag from settings.json (wakeword_oww.rs).
/// Defaults to FALSE. Set `"speakerVerification": true` to enable.
pub fn read_speaker_verification<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let dir = app.path().app_data_dir();
    let Ok(dir) = dir else { return false; };
    let Ok(content) = std::fs::read_to_string(dir.join("settings.json")) else { return false; };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else { return false; };
    json.get("speakerVerification")
        .or_else(|| json.get("speaker_verification"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

// ─── Restored IPC commands (clobbered by a stale commands.rs rewrite) ───

/// IPC: Temporary ghost-pipeline tracer (p0–p4 stamps from recorder.ts).
/// Prints to the unified console so a silent death downstream is provable.
#[tauri::command]
pub fn debug_trace(msg: String) -> Result<(), String> {
    println!("[TRACE] {msg}");
    tracing::info!("debug_trace: {msg}");
    Ok(())
}

// ─── Google multi-account commands ──────────────────────────────────────

/// IPC: Retrieve all connected Google account profiles.
#[tauri::command]
pub fn google_get_accounts() -> Vec<crate::google::types::GoogleAccountProfile> {
    crate::google::accounts::list_accounts()
}

/// IPC: Trigger direct RFC 8252 loopback OAuth sign-in to connect a Google account.
#[tauri::command]
pub async fn google_connect_account() -> Result<crate::google::types::GoogleAccountProfile, String> {
    crate::google::accounts::connect_account().await
}

/// IPC: Promote an account to primary (avatar badge + default token).
#[tauri::command]
pub fn google_set_primary_account(email: String) -> Result<(), String> {
    crate::google::accounts::set_primary_account(&email)
}

/// IPC: Disconnect a Google account (registry + keyring cleanup).
#[tauri::command]
pub fn google_disconnect_account(email: String) -> Result<(), String> {
    crate::google::accounts::disconnect_account(&email)
}

/// IPC: Save custom developer Google OAuth credentials (override built-ins).
#[tauri::command]
pub fn google_save_custom_credentials(
    client_id: String,
    client_secret: Option<String>,
) -> Result<(), String> {
    crate::google::accounts::save_custom_credentials(&client_id, client_secret.as_deref());
    Ok(())
}

// ─── Memory / diary / webhook / improvement / health / settings IO ──────

/// IPC: Recall stored personal memories ("what is my X" support).
#[tauri::command]
pub fn memory_recall<R: Runtime>(app: AppHandle<R>, query: String) -> Vec<(String, String)> {
    let Ok(dir) = app.path().app_data_dir() else { return vec![] };
    crate::memory::recall(&dir, &query)
}

/// IPC: Forget a stored memory key.
#[tauri::command]
pub fn memory_forget<R: Runtime>(app: AppHandle<R>, key: String) -> bool {
    let Ok(dir) = app.path().app_data_dir() else { return false };
    crate::memory::forget(&dir, &key)
}

/// IPC: Diary rollup for UI — recent events + a one-line summary label.
#[tauri::command]
pub fn diary_summary<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let events = crate::diary::recent_events(&dir, 50);
    let label = crate::diary::rollup_label(&events, std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64);
    Ok(serde_json::json!({ "label": label, "events": events }))
}

/// IPC: Current webhook bearer token (Connections tab display).
#[tauri::command]
pub fn webhook_token() -> String {
    crate::webhook::webhook_token()
}

/// IPC: Rotate the webhook bearer token; returns the new one.
#[tauri::command]
pub fn webhook_rotate_token() -> String {
    crate::webhook::rotate_webhook_token()
}

/// IPC: Edge-case miner report (miss/failure clusters → suggestions).
#[tauri::command]
pub fn improvement_report<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let clusters = crate::improve::run_miner(&dir);
    Ok(serde_json::json!({ "suggestions": crate::improve::top_suggestions(&clusters, 20) }))
}

/// IPC: Ghost vision provider daily-quota counters (Accounts tab UI).
#[tauri::command]
pub fn vision_quota_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(crate::vision::quota_status(&dir))
}

/// IPC: Vision key presence + Gemini quota in one call (Feature 83 P5 —
/// drives the Gemini card status pill; presence only, never key values).
#[tauri::command]
pub fn vision_key_status<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let quota = crate::vision::quota_status(&dir);
    Ok(serde_json::json!({
        "gemini_present": !read_api_key(&app, "gemini").is_empty(),
        "groq_present": !read_api_key(&app, "groq").is_empty(),
        "gemini_used": quota.get("gemini").and_then(|g| g.get("used")),
        "gemini_limit": quota.get("gemini").and_then(|g| g.get("limit")),
    }))
}

/// IPC: Validate the stored Gemini key with a quota-free models/list call
/// (Feature 83 P5 — the hub "Test" button; retrieval costs no quota).
#[tauri::command]
pub async fn vision_test_key<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let key = read_api_key(&app, "gemini");
    if key.is_empty() {
        return Ok(serde_json::json!({ "ok": false, "detail": "no-key" }));
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    match client
        .get("https://generativelanguage.googleapis.com/v1beta/models?pageSize=1")
        .header("x-goog-api-key", &key)
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            Ok(serde_json::json!({ "ok": true, "detail": "valid" }))
        }
        Ok(resp) if resp.status().as_u16() == 429 => {
            Ok(serde_json::json!({ "ok": false, "detail": "quota-exhausted" }))
        }
        Ok(_) => Ok(serde_json::json!({ "ok": false, "detail": "rejected" })),
        Err(_) => Ok(serde_json::json!({ "ok": false, "detail": "network-error" })),
    }
}

/// IPC: Runtime health snapshot (Connections tab System Status panel).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthStatus {
    pub memory_mb: u64,
    pub uptime_sec: u64,
    pub stt_port: bool,
    pub nlu_port: bool,
    pub worker_reachable: bool,
    pub groq_key: bool,
    pub gemini_key: bool,
}

#[tauri::command]
pub async fn get_health_status<R: Runtime>(app: AppHandle<R>) -> HealthStatus {
    let memory_mb = process_memory_mb();
    let uptime_sec = BOOT_INSTANT
        .get()
        .map(|b| b.elapsed().as_secs())
        .unwrap_or(0);
    let stt_port = crate::lazy_stt::is_stt_responsive();
    // In-process model now (nlu_local.rs) — "responsive" means "loaded",
    // not "sidecar port open". First call may load from disk, so run it
    // off the async executor thread like any other blocking I/O here.
    let nlu_port = tokio::task::spawn_blocking(crate::nlu_local::is_loaded)
        .await
        .unwrap_or(false);
    let worker_reachable = crate::tts_network::check_network().await;
    let groq_key = !read_groq_api_key(&app).is_empty() || !read_api_key(&app, "groq").is_empty();
    let gemini_key = !read_api_key(&app, "gemini").is_empty();
    HealthStatus {
        memory_mb,
        uptime_sec,
        stt_port,
        nlu_port,
        worker_reachable,
        groq_key,
        gemini_key,
    }
}

static BOOT_INSTANT: once_cell::sync::OnceCell<std::time::Instant> = once_cell::sync::OnceCell::new();

/// Record boot time (called from lib.rs setup) for the health panel.
pub fn note_boot() {
    let _ = BOOT_INSTANT.set(std::time::Instant::now());
}

/// Best-effort process working-set via sysinfo (matches app_registry usage).
/// All failures map to 0 and the UI just shows "—".
fn process_memory_mb() -> u64 {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
    let pid = std::process::id();
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]),
        true,
        ProcessRefreshKind::new().with_memory(),
    );
    sys.process(sysinfo::Pid::from_u32(pid))
        .map(|p| p.memory() / (1024 * 1024))
        .unwrap_or(0)
}

/// IPC: Export settings (secrets stay in the OS keychain — never in the file).
#[tauri::command]
pub fn export_settings<R: Runtime>(app: AppHandle<R>) -> Result<String, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    let content = std::fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_string());
    let mut json: serde_json::Value =
        serde_json::from_str(&content).unwrap_or_else(|_| serde_json::json!({}));
    // Secrets live in the keychain — strip any legacy disk copies.
    for key in ["groqApiKey", "geminiApiKey", "cerebrasApiKey"] {
        json[key] = serde_json::json!("");
    }
    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
}

/// IPC: Import settings from a JSON string (writes settings.json + keychain).
#[tauri::command]
pub fn import_settings<R: Runtime>(app: AppHandle<R>, json_str: String) -> Result<(), String> {
    let json: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|e| format!("invalid settings JSON: {e}"))?;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    std::fs::write(&path, serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    // Heal API keys into the OS keychain (best-effort).
    if let Some(k) = json.get("groqApiKey").and_then(|v| v.as_str()) {
        if !k.is_empty() {
            crate::auth_vault::set_api_key("groq", k);
        }
    }
    if let Some(k) = json.get("geminiApiKey").and_then(|v| v.as_str()) {
        if !k.is_empty() {
            crate::auth_vault::set_api_key("gemini", k);
        }
    }
    if let Some(k) = json.get("cerebrasApiKey").and_then(|v| v.as_str()) {
        if !k.is_empty() {
            crate::auth_vault::set_api_key("cerebras", k);
        }
    }
    Ok(())
}

// ─── Unified sidebar geometry tests ─────────────────────────────────

#[cfg(test)]
mod sidebar_geometry_tests {
    use super::sidebar_geometry;

    #[test]
    fn assistant_defaults_to_right_dock() {
        // 1920x1080: x = 1920 - 520 - 10 = 1390, y = 20,
        // h = min(1040, 1080-40) = 1040
        let (x, y, w, h) = sidebar_geometry("assistant", None, 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (1390.0, 20.0, 520.0, 1040.0));
    }

    #[test]
    fn assistant_left_dock_mirrors() {
        let (x, y, w, h) = sidebar_geometry("assistant", Some("left"), 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (10.0, 20.0, 520.0, 1040.0));
    }

    #[test]
    fn assistant_center_float() {
        // center: x = (1920-520)/2 = 700, y = (1080-1040)/2 = 20
        let (x, y, w, h) = sidebar_geometry("assistant", Some("center"), 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (700.0, 20.0, 520.0, 1040.0));
    }

    #[test]
    fn assistant_height_clamps_on_short_monitor() {
        // mh - 40 < 1040 → clamp; h never below 400
        let (_, _, _, h) = sidebar_geometry("assistant", None, 1920.0, 600.0);
        assert_eq!(h, 560.0);
        let (_, _, _, h) = sidebar_geometry("assistant", None, 1920.0, 300.0);
        assert_eq!(h, 400.0);
    }

    #[test]
    fn pr_list_uses_right_dock() {
        // x = 1920 - 520 - 10 = 1390, y = 20, h = 1040
        let (x, y, w, h) = sidebar_geometry("pr-list", None, 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (1390.0, 20.0, 520.0, 1040.0));
    }

    #[test]
    fn settings_right_docked() {
        // h = min(1080, 1080-40) = 1040; x = 1920 - 740 - 10 = 1170; y = 20
        let (x, y, w, h) = sidebar_geometry("settings", None, 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (1170.0, 20.0, 740.0, 1040.0));
    }

    #[test]
    fn settings_docked_goes_right() {
        let (x, y, w, h) = sidebar_geometry("settings", Some("right"), 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (1170.0, 20.0, 740.0, 1040.0));
    }

    #[test]
    fn settings_height_clamps_on_short_monitor() {
        let (_, _, _, h) = sidebar_geometry("settings", None, 1920.0, 800.0);
        assert_eq!(h, 760.0);
        let (_, _, _, h) = sidebar_geometry("settings", None, 1920.0, 300.0);
        assert_eq!(h, 400.0);
    }

    #[test]
    fn architect_centers_with_top_pin() {
        // h = min(1040, 1080-40) = 1040; x = (1920-960)/2 = 480; y = 40 (pinned)
        let (x, y, w, h) = sidebar_geometry("architect", None, 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (480.0, 40.0, 960.0, 1040.0));
    }

    #[test]
    fn architect_docked_goes_right() {
        // x = 1920 - 960 - 10 = 950; y = 20; h = 1040
        let (x, y, w, h) = sidebar_geometry("architect", Some("right"), 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (950.0, 20.0, 960.0, 1040.0));
    }

    #[test]
    fn architect_height_clamps_on_short_monitor() {
        let (_, _, _, h) = sidebar_geometry("architect", None, 1920.0, 500.0);
        assert_eq!(h, 460.0);
    }

    #[test]
    fn unknown_view_falls_back_to_assistant() {
        let (x, y, w, h) = sidebar_geometry("nonsense", None, 1920.0, 1080.0);
        assert_eq!((x, y, w, h), (1390.0, 20.0, 520.0, 1040.0));
    }

    #[test]
    fn small_monitor_x_never_negative() {
        let (x, _, _, _) = sidebar_geometry("architect", None, 500.0, 1080.0);
        assert_eq!(x, 0.0);
        let (x, _, _, _) = sidebar_geometry("assistant", None, 100.0, 1080.0);
        // 100 - 520 - 10 = -430 → clamp to 0
        assert_eq!(x, 0.0);
    }
}
