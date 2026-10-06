//! Global hotkey (Ctrl/Cmd+Space) → state-dependent action.
//!
//! On press:
//!   - If any window (the unified sidebar hosting Assistant/Command
//!     Hub/Architect/PR-List, settings, setup) is visible → close it
//!     only (do NOT wake).
//!   - If no window is visible → wake the assistant (do NOT touch windows).
//!   - If the assistant is speaking → barge-in: the frontend wake handler
//!     stops TTS and starts listening (handled in main.tsx startListening).
//!
//! This means:
//!   - Pressing the hotkey twice (with sidebar visible) first closes the
//!     sidebar, then wakes NEXUS on the second press.
//!   - Wake-word activation does NOT close the sidebar (handled separately
//!     in `wakeword_oww.rs`, which never emits `sidebar:hide`).
//!   - The hotkey never does both at once — it's one or the other based on
//!     the current sidebar visibility state.
//!
//! NOTE: The global-shortcut plugin is not available on Linux.
//! This entire module is compiled only on Windows and macOS.

#![cfg(not(target_os = "linux"))]

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const HOTKEYS: &[&str] = &[
    "CommandOrControl+Space",
    "CommandOrControl+Shift+S",
    "CommandOrControl+Alt+X",
];

pub fn init<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    for &hk in HOTKEYS {
        let sc: Shortcut = match hk.parse() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to parse hotkey '{hk}': {e}");
                continue;
            }
        };

        let handle = app.clone();
        if let Err(e) = app.global_shortcut().on_shortcut(sc, move |_app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                // Ctrl+Shift+S → open settings sidebar directly
                if hk == "CommandOrControl+Shift+S" {
                    tracing::info!("hotkey ({}) → opening settings sidebar", hk);
                    let app_clone = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::commands::show_settings_sidebar(app_clone).await;
                    });
                    return;
                }

                // Ctrl+Alt+X → stage kill-switch: destroy + disable the
                // fullscreen overlay for the session (blackout escape hatch
                // even if the renderer is wedged and can't hear IPC).
                if hk == "CommandOrControl+Alt+X" {
                    tracing::warn!("hotkey ({}) → stage kill-switch", hk);
                    let app_clone = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::stage::stage_hide_kill(app_clone).await;
                    });
                    return;
                }

                // Ctrl+Space → ghost session live: end it (same path as
                // Esc / voice exit) + spoken confirm. Checked FIRST so a
                // ghost session never falls through to wake/close-window
                // behavior below. TTS is stopped first so the exit line
                // isn't talked over.
                if crate::ghost::session_active() {
                    tracing::info!("hotkey ({}) → ghost session live, ending it", hk);
                    let _ = crate::tts::stop_tts();
                    crate::orchestrator::cancel_active();
                    if let Some(ms) = handle.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>() {
                        ms.set_tts_playing(false);
                    }
                    // The exit line is spoken by abort_session itself, so all
                    // abort paths (Esc, here, API) say the identical line.
                    let app_clone = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::ghost::ghost_abort(app_clone).await;
                    });
                    return;
                }

                // Ctrl+Space → if TTS is playing, stop speech immediately AND
                // start listening (barge-listen): cut the audio, flush the
                // 150ms DAC/room tail so it can't leak into the new capture,
                // then run the identical wake sequence as the idle branch.
                let is_speaking = handle.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
                    .map(|ms| ms.tts_playing.load(std::sync::atomic::Ordering::Relaxed))
                    .unwrap_or(false);
                if is_speaking {
                    tracing::info!("hotkey ({}) → TTS playing, stopping speech and starting barge-listen", hk);
                    // Single choke point (order: audio → turn → flag → queue).
                    crate::orchestrator::request_barge_in("hotkey-tts");
                    let _ = tauri::Emitter::emit(&handle, "tts:stop", ());
                    let handle_clone = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        // DAC drain: the sound card + room reverb tail
                        // outlives stop_tts() and would poison the capture.
                        tokio::time::sleep(tokio::time::Duration::from_millis(
                            crate::orchestrator::BARGE_DAC_DRAIN_MS,
                        ))
                        .await;
                        crate::wakeword_oww::start_stt_capture();
                        crate::window_manager::wake_orb(&handle_clone);
                    });
                    return;
                }

                // Ctrl+Space → close any visible window, or wake NEXUS
                // Check if any sidebar/window is currently visible.
                // If so, close it and do NOT wake NEXUS.
                // (The four panel views — Assistant, Command Hub, Architect,
                // PR List — all live inside the ONE unified "sidebar" window.)
                // NOTE: `stage` is no longer part of this check — since it
                // hosts the always-on orb now (single-stage migration), its
                // own visibility is permanent infrastructure, not a
                // closeable "window" state. A live ghost session is handled
                // separately above (session_active() check, before this
                // point) and never reaches here.
                let sidebar_visible = handle
                    .get_webview_window("sidebar")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                let settings_visible = handle
                    .get_webview_window("settings")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);
                let setup_visible = handle
                    .get_webview_window("setup")
                    .and_then(|w| w.is_visible().ok())
                    .unwrap_or(false);

                // Ctrl+Space → close any visible window AND wake NEXUS (D3).
                // Window closing moved to Escape at the frontend layer, so
                // the hotkey is uniformly "talk to NEXUS" in every state.
                if sidebar_visible || settings_visible || setup_visible {
                    // A window is visible → destroy it to free ~250 MB each.
                    tracing::info!("hotkey ({}) → window visible, closing window(s) then waking", hk);
                    let _ = crate::dyn_windows::destroy_window(&handle, "sidebar");
                    let _ = crate::dyn_windows::destroy_window(&handle, "settings");
                    let _ = crate::dyn_windows::destroy_window(&handle, "setup");
                }
                // Wake NEXUS in all cases (windows were closed above, if any).
                {
                    tracing::info!("hotkey ({}) → waking NEXUS", hk);
                    println!("[ORB-HOTKEY] Hotkey ({hk}) → cancelling active request, flushing DAC, waking orb to listening");
                    // Cancel any in-flight request (especially while thinking) so late completions
                    // cannot stomp on the new turn and hide the orb.
                    crate::orchestrator::request_barge_in("hotkey-wake");
                    let _ = tauri::Emitter::emit(&handle, "tts:stop", ());
                    let _ = crate::tts::stop_tts();

                    // Only pre-start local STT sidecar if cloud STT won't be used
                    // (saves ~340 MB RAM when Groq cloud STT is active).
                    let groq_key = crate::commands::read_groq_api_key(&handle);
                    let local_only = crate::commands::read_local_stt_only(&handle);
                    if groq_key.is_empty() || local_only {
                        std::thread::spawn(|| {
                            crate::lazy_stt::ensure_stt_running();
                        });
                    } else {
                        tracing::info!("hotkey: Groq cloud STT configured, skipping local sidecar pre-start (saves RAM)");
                    }

                    // Start Rust-side STT capture (same as wake word path).
                    // Captures audio from the cpal stream — no getUserMedia needed.
                    crate::wakeword_oww::start_stt_capture();
                    crate::window_manager::wake_orb(&handle);
                }
            }
        }) {
            tracing::warn!("Failed to register handler for hotkey '{hk}': {e}");
        } else {
            tracing::info!("Registered global hotkey handler: {hk}");
        }
    }

    Ok(())
}
