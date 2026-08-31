//! IPC commands for setup window management and configuration.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{Emitter, Manager, Runtime};
#[cfg(feature = "wakeword-sherpa")]
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
}

static PENDING_SIDEBAR: Mutex<Option<PendingSidebar>> = Mutex::new(None);

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
#[tauri::command]
pub fn close_setup_window<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let _ = crate::dyn_windows::destroy_window(&app, "setup");
    if let Some(main_win) = app.get_webview_window("main") {
        let _ = main_win.show();
        let _ = crate::window_manager::configure_non_activating_overlay(&main_win);
        let _ = main_win.set_ignore_cursor_events(false);
        let _ = main_win.eval("window.__NEXUS_WAKE__ && window.__NEXUS_WAKE__()");
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
    Ok(ServerConfig {
        server_url: json["serverUrl"].as_str()
            .unwrap_or(option_env!("NEXUS_SERVER_URL").unwrap_or("ws://127.0.0.1:41098/ws"))
            .to_string(),
        user_id: json["userId"].as_str().unwrap_or("").to_string(),
        device_id: json["deviceId"].as_str().unwrap_or("").to_string(),
    })
}

// ─── Voice profile commands — only available when wakeword-sherpa is enabled ─
// Speaker enrollment uses sherpa-onnx for embedding extraction. When using the
// default wakeword-oww engine, verification is not yet wired (see AGENTS.md),
// so these commands are compiled out to avoid pulling in sherpa-onnx C++ deps.
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
        let sherpa = resource_dir.join("sherpa");
        if sherpa.exists() {
            return Some(sherpa);
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

// ─── First-of-day greeting ─────────────────────────────────────────────────
//
// Instead of greeting on every boot or every sleep/wake, NEXUS greets only
// on the first user interaction (wake) of each calendar day. The last
// greeting date is persisted to `greeting-state.json` in the app data dir,
// so it survives restarts, shutdowns, and crashes.
//
// Flow:
//   1. User wakes NEXUS (hotkey or spoken "nexus")
//   2. Frontend calls `should_greet_today` → Rust compares today's date
//      with the stored `last_greeting_date`
//   3. If different (first time today) → frontend speaks the greeting,
//      then calls `mark_greeted_today` to persist today's date
//   4. If same (already greeted today) → frontend skips greeting, goes
//      straight to listening

/// Resolve the greeting state file path in the app data directory.
fn greeting_state_path<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("greeting-state.json"))
}

/// Read the last greeting date from disk. Returns None if the file
/// doesn't exist or is corrupted (safe default: greet).
fn read_last_greeting_date(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    json["last_greeting_date"]
        .as_str()
        .map(|s| s.to_string())
}

/// Write today's date to the greeting state file.
fn write_last_greeting_date(path: &Path, date: &str) -> Result<(), String> {
    let json = serde_json::json!({ "last_greeting_date": date });
    std::fs::write(path, json.to_string())
        .map_err(|e| format!("failed to write greeting state: {e}"))
}

/// Get today's date as YYYY-MM-DD in the local timezone.
fn today_local() -> String {
    use chrono::Local;
    Local::now().format("%Y-%m-%d").to_string()
}

/// IPC: Check if NEXUS should greet the user on this wake.
///
/// Returns true if today's date differs from the stored `last_greeting_date`
/// (i.e., this is the first interaction of the day). Also checks meeting/pause
/// state — if a meeting is active or NEXUS is paused, the greeting is
/// suppressed but the date is NOT saved, so the next wake after the meeting
/// will still greet.
#[tauri::command]
pub fn should_greet_today<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<bool, String> {
    let path = greeting_state_path(&app)?;
    let last = read_last_greeting_date(&path);
    let today = today_local();

    let is_new_day = last.as_deref() != Some(&today);

    // Check meeting/pause state — suppress greeting but don't save date
    let (meeting, paused) = match app
        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
    {
        Some(state) => (state.is_meeting_active(), state.is_paused()),
        None => (false, false),
    };

    let should_greet = is_new_day && !meeting && !paused;
    tracing::info!(
        "greeting check: today={} last={:?} new_day={} meeting={} paused={} → greet={}",
        today, last, is_new_day, meeting, paused, should_greet
    );
    Ok(should_greet)
}

/// IPC: Mark that NEXUS has greeted the user today.
///
/// Called by the frontend AFTER the greeting TTS finishes (or starts —
/// either way, the date is saved so subsequent wakes don't re-greet).
#[tauri::command]
pub fn mark_greeted_today<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    let path = greeting_state_path(&app)?;
    let today = today_local();
    write_last_greeting_date(&path, &today)?;
    tracing::info!("greeting: marked today ({}) as greeted", today);
    Ok(())
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
    let win = crate::dyn_windows::get_or_create_window(&app, crate::dyn_windows::WindowConfig::sidebar())?;
    // Check if window is already visible to avoid capturing ourselves
    let already_visible = win.is_visible().unwrap_or(false);
    show_sidebar_inner(&app, &win, already_visible)?;
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
    let win = crate::dyn_windows::get_or_create_window(&app, crate::dyn_windows::WindowConfig::sidebar())?;

    // ── Key fix: pre-position the window BEFORE capturing the backdrop ──
    // A freshly-created window sits at physical (0, 0). On Windows, a window
    // at (0, 0) may be partially off-screen or on the wrong monitor, which
    // causes `win.current_monitor()` inside `capture_backdrop` to return None,
    // silently aborting the capture. We run the same positioning math used by
    // `show_sidebar_inner` here first, so the window is in its final position
    // on the correct monitor before we call `BitBlt`. This is safe to do even
    // before `win.show()` — `set_position` works on hidden windows.
    if let Ok(Some(monitor)) = win.current_monitor().or_else(|_| {
        // Fallback: if not yet on a monitor, try primary monitor
        win.primary_monitor()
    }) {
        let scale = monitor.scale_factor();
        let screen = monitor.size();
        let sidebar_w = 600i32;
        let sidebar_h = 1000i32;
        let phys_w = (sidebar_w as f64 * scale) as i32;
        let phys_h = (sidebar_h as f64 * scale) as i32;
        #[cfg(target_os = "windows")]
        let taskbar = (48.0 * scale) as i32;
        #[cfg(not(target_os = "windows"))]
        let taskbar = (48.0 * scale) as i32;
        let gap = (12.0 * scale) as i32;
        let x = screen.width as i32 - phys_w - gap;
        let y = (screen.height as i32 - phys_h - taskbar - gap).max(0);
        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    }

    // Capture the backdrop before showing (only for fresh windows or hidden ones).
    // If the window is already visible, skip capture to avoid capturing ourselves.
    let backdrop = if window_existed && win.is_visible().unwrap_or(false) {
        None
    } else {
        capture_backdrop(&app, &win)
    };

    // Store the pending content so the frontend can fetch it on mount.
    // This handles the fresh-window case where events would be missed.
    {
        let mut pending = PENDING_SIDEBAR.lock().unwrap();
        *pending = Some(PendingSidebar {
            query: query.clone(),
            text: text.clone(),
            backdrop: backdrop.clone(),
            analysis: None,
        });
    }

    // Show the window (no-op if already visible).
    show_sidebar_inner(&app, &win, backdrop.is_some())?;

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
    let win = crate::dyn_windows::get_or_create_window(&app, crate::dyn_windows::WindowConfig::sidebar())?;

    // Pre-position the window before capturing backdrop
    if !window_existed {
        let _ = win.set_position(tauri::LogicalPosition::new(0.0, 0.0));
    }

    let backdrop = if window_existed && win.is_visible().unwrap_or(false) {
        None
    } else {
        capture_backdrop(&app, &win)
    };

    {
        let mut pending = PENDING_SIDEBAR.lock().unwrap();
        *pending = Some(PendingSidebar {
            query: query.clone(),
            text: text.clone(),
            backdrop: backdrop.clone(),
            analysis: Some(analysis.clone()),
        });
    }

    show_sidebar_inner(&app, &win, backdrop.is_some())?;

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

/// IPC: Fetch pending sidebar content (called by the frontend on mount).
/// Returns the content + backdrop + analysis that was stored by
/// `show_sidebar_with_content` or `show_sidebar_with_analysis`, or null
/// if no content is pending. Clears the pending data after returning.
#[tauri::command]
pub fn get_pending_sidebar_content() -> Result<Option<serde_json::Value>, String> {
    let mut pending = PENDING_SIDEBAR.lock().unwrap();
    let data = pending.take();
    match data {
        Some(p) => {
            tracing::info!("sidebar: pending content fetched (query={} chars, text={} chars, has_backdrop={}, has_analysis={})", p.query.len(), p.text.len(), p.backdrop.is_some(), p.analysis.is_some());
            Ok(Some(serde_json::json!({
                "query": p.query,
                "text": p.text,
                "backdrop": p.backdrop,
                "analysis": p.analysis,
            })))
        }
        None => Ok(None),
    }
}

/// Capture the desktop region behind the sidebar window (Windows only).
/// Must be called BEFORE `win.show()` so we don't capture the sidebar itself.
/// Returns the blurred backdrop as a `data:image/png;base64,...` URI, or None.
fn capture_backdrop<R: Runtime>(
    _app: &tauri::AppHandle<R>,
    win: &tauri::WebviewWindow<R>,
) -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        let monitor = win.current_monitor().ok()??;
        let scale = monitor.scale_factor();
        let screen = monitor.size();
        let sidebar_w = 600i32;
        let sidebar_h = 1000i32;
        let phys_w = (sidebar_w as f64 * scale) as i32;
        let phys_h = (sidebar_h as f64 * scale) as i32;
        let taskbar = (48.0 * scale) as i32;
        let gap = (12.0 * scale) as i32;
        let x = screen.width as i32 - phys_w - gap;
        let y = (screen.height as i32 - phys_h - taskbar - gap).max(0);

        match crate::sidebar_backdrop::capture_and_blur(x, y, phys_w, phys_h, 32.0) {
            Some(data_uri) => {
                tracing::info!("sidebar: backdrop captured ({} bytes)", data_uri.len());
                Some(data_uri)
            }
            None => {
                tracing::warn!("sidebar: backdrop capture failed (x={x}, y={y}, w={phys_w}, h={phys_h})");
                None
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Shared inner logic for showing the sidebar window.
/// `backdrop_already_captured`: if true, skip the backdrop capture (it was
/// already done by the caller and stored in the pending content). This prevents
/// re-capturing the window itself when the window is already visible.
fn show_sidebar_inner<R: Runtime>(
    _app: &tauri::AppHandle<R>,
    win: &tauri::WebviewWindow<R>,
    backdrop_already_captured: bool,
) -> Result<(), String> {

    // Position at bottom-right of the screen, above the taskbar.
    use tauri::PhysicalPosition;
    if let Ok(Some(monitor)) = win.current_monitor() {
        let scale = monitor.scale_factor();
        let screen = monitor.size();
        let sidebar_w = 600i32;
        let sidebar_h = 1000i32;
        let phys_w = (sidebar_w as f64 * scale) as i32;
        let phys_h = (sidebar_h as f64 * scale) as i32;

        #[cfg(target_os = "macos")]
        let taskbar = (70.0 * scale) as i32;
        #[cfg(target_os = "windows")]
        let taskbar = (48.0 * scale) as i32;
        #[cfg(target_os = "linux")]
        let taskbar = (36.0 * scale) as i32;
        let gap = (12.0 * scale) as i32;

        let x = screen.width as i32 - phys_w - gap;
        // Clamp Y so the window doesn't go off-screen if taller than the monitor
        let y = (screen.height as i32 - phys_h - taskbar - gap).max(0);
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    // ─── "Fake blur" backdrop capture (Windows only) ───────────────
    // Only capture if the caller hasn't already done so. This prevents
    // capturing the sidebar itself when the window is already visible
    // (e.g. when show_sidebar is called from the frontend's useEffect).
    if !backdrop_already_captured {
        if let Some(data_uri) = capture_backdrop(_app, win) {
            let _ = _app.emit("sidebar:backdrop", data_uri);
        }
    }

    win.show().map_err(|e| e.to_string())?;

    #[cfg(target_os = "linux")]
    {
        use tauri::PhysicalPosition;
        if let Ok(Some(monitor)) = win.current_monitor() {
            let scale = monitor.scale_factor();
            let screen = monitor.size();
            let sidebar_w = 600i32;
            let sidebar_h = 1000i32;
            let phys_w = (sidebar_w as f64 * scale) as i32;
            let phys_h = (sidebar_h as f64 * scale) as i32;
            let taskbar = (36.0 * scale) as i32;
            let gap = (12.0 * scale) as i32;
            let x = screen.width as i32 - phys_w - gap;
            let y = (screen.height as i32 - phys_h - taskbar - gap).max(0);
            let _ = win.set_position(PhysicalPosition::new(x, y));
        }
    }

    #[cfg(target_os = "windows")]
    {
        // No window-vibrancy re-apply here — see the detailed comment in
        // lib.rs's setup hook for why this window intentionally never calls
        // apply_blur/apply_acrylic/apply_mica. The window's transparency
        // comes from tao's own material-free DWM registration (done once at
        // window creation) and does not need re-applying on every show.
        //
        // Corner rounding is a plain window-shape attribute (not a material)
        // so it's safe/cheap to re-assert on every show in case it was lost.
        crate::dwm_corners::round_corners(&win);

        // ── Live blur: 1 FPS + change detection ──────────────────────
        // The previous 200ms (5 FPS) loop created a "buffering video"
        // effect — each frame required the full capture→blur→JPEG→base64
        //→event→repaint pipeline (~50-100ms), making 5 FPS look stuttery.
        //
        // Now: capture every 1s, hash the raw BGRA, and only run the
        // expensive blur→encode→emit pipeline when the background actually
        // changed (hash differs). When nothing moves behind the sidebar,
        // CPU stays at ~0. When something moves, the blur updates within
        // ~1s with a gentle CSS crossfade (see sidebar.css ::before).
        static LIVE_BLUR_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        static LAST_FRAME_HASH: std::sync::Mutex<Option<u64>> = std::sync::Mutex::new(None);
        if !LIVE_BLUR_ACTIVE.load(std::sync::atomic::Ordering::SeqCst) {
            LIVE_BLUR_ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
            // Reset hash so the first frame after show always emits
            *LAST_FRAME_HASH.lock().unwrap() = None;

            let win_clone = win.clone();
            let app_clone = _app.clone();
            tauri::async_runtime::spawn(async move {
                // Wait for window to fully appear
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;

                while win_clone.is_visible().unwrap_or(false) {
                    // 1 FPS — 1000ms between captures
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

                    if !win_clone.is_visible().unwrap_or(false) {
                        break;
                    }

                    if let Ok(Some(monitor)) = win_clone.current_monitor() {
                        let scale = monitor.scale_factor();
                        let screen = monitor.size();
                        let sidebar_w = 600i32;
                        let sidebar_h = 1000i32;
                        let phys_w = (sidebar_w as f64 * scale) as i32;
                        let phys_h = (sidebar_h as f64 * scale) as i32;
                        let taskbar = (48.0 * scale) as i32;
                        let gap = (12.0 * scale) as i32;
                        let x = screen.width as i32 - phys_w - gap;
                        let y = (screen.height as i32 - phys_h - taskbar - gap).max(0);

                        // Step 1: cheap raw capture for hashing (~1ms)
                        let raw_bgra = match crate::sidebar_backdrop::capture_region_bgra_public(x, y, phys_w, phys_h) {
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

                        // Step 3: only run the expensive pipeline if changed
                        if should_emit {
                            // We already have the raw BGRA — blur + encode it
                            // without re-capturing (reuse the bytes we have).
                            if let Some(data_uri) = crate::sidebar_backdrop::blur_bgra_to_jpeg(&raw_bgra, phys_w, phys_h, 32.0) {
                                let _ = app_clone.emit("sidebar:backdrop", data_uri);
                            }
                        }
                    }
                }

                // Clean up: reset hash so next show captures fresh
                *LAST_FRAME_HASH.lock().unwrap() = None;
                LIVE_BLUR_ACTIVE.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        // Re-apply vibrancy after show — the effect can be lost if the
        // window was hidden for a long time or the app was backgrounded.
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = apply_vibrancy(
            &win,
            NSVisualEffectMaterial::Sidebar,
            Some(NSVisualEffectState::Active),
            Some(20.0),
        );
    }

    Ok(())
}

/// IPC: Hide the response sidebar window.
/// Called after the server response has been spoken.
#[tauri::command]
pub fn hide_sidebar<R: Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<(), String> {
    // Destroy the sidebar window to free ~250 MB of WebView2 processes.
    // It will be recreated on-demand next time show_sidebar is called.
    let _ = crate::dyn_windows::destroy_window(&app, "sidebar");
    Ok(())
}

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
}

fn default_tts_provider() -> String {
    "kokoro".to_string()
}

impl Default for NexusSettings {
    fn default() -> Self {
        Self {
            autostart: true,
            hotkey: "Ctrl+Shift+Space".to_string(),
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
            speech_rate: 1.0,
            tts_provider: "kokoro".to_string(),
        }
    }
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
                if let Some(url) = json.get("serverUrl").and_then(|v| v.as_str()) {
                    settings.server_url = url.to_string();
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
    // Also save server config separately (so existing code can read it)
    let config = serde_json::json!({
        "serverUrl": settings.server_url,
        "userId": settings.user_id,
        "deviceId": settings.device_id,
    });
    let config_path = dir.join("nexus-config.json");
    std::fs::write(&config_path, config.to_string()).map_err(|e| e.to_string())?;
    tracing::info!("settings saved to {:?}", path);
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
