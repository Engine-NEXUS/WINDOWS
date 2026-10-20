//! Global hotkeys and action dispatch.
//!
//! Hotkeys:
//!   - `CommandOrControl+Space`: Symmetric agent call / cancel.
//!     If the agent is active (capturing audio, speaking TTS, processing a
//!     request, or running a ghost session), cancels the active agent and
//!     hides the orb. If the agent is idle, calls the agent (plays the wake
//!     cue, starts STT capture, and wakes the orb). Windows (settings, sidebar,
//!     setup) are closed by Escape, never by this hotkey.
//!   - `CommandOrControl+Shift+S`: Opens the Command Hub settings sidebar directly.
//!   - `CommandOrControl+Alt+X`: Stage kill switch: destroys and disables the
//!     fullscreen overlay for the session as an escape hatch even if the renderer
//!     is wedged.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentState {
    pub capturing: bool,
    pub speaking: bool,
    pub has_request: bool,
    pub ghost: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Press {
    OpenSettings,
    KillStage,
    Cancel { abort_ghost: bool },
    Call,
}

/// Pure hotkey action decision based on hotkey chord and live agent activity.
pub(crate) fn decide(hotkey: &str, state: AgentState) -> Press {
    match hotkey {
        "CommandOrControl+Shift+S" => Press::OpenSettings,
        "CommandOrControl+Alt+X" => Press::KillStage,
        _ => {
            if state.capturing || state.speaking || state.has_request || state.ghost {
                Press::Cancel {
                    abort_ghost: state.ghost,
                }
            } else {
                Press::Call
            }
        }
    }
}

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
                let state = AgentState {
                    capturing: crate::wakeword_oww::stt_capturing(),
                    speaking: handle
                        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
                        .map(|ms| ms.tts_playing.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(false),
                    has_request: crate::orchestrator::has_active_request(),
                    ghost: crate::ghost::session_active(),
                };

                match decide(hk, state) {
                    Press::OpenSettings => {
                        // Ctrl+Shift+S → open settings sidebar directly
                        tracing::info!("hotkey ({}) → opening settings sidebar", hk);
                        let app_clone = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::commands::show_settings_sidebar(app_clone).await;
                        });
                    }
                    Press::KillStage => {
                        // Ctrl+Alt+X → stage kill-switch: destroy + disable the
                        // fullscreen overlay for the session (blackout escape hatch
                        // even if the renderer is wedged and can't hear IPC).
                        tracing::warn!("hotkey ({}) → stage kill-switch", hk);
                        let app_clone = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            crate::orchestrator::request_barge_in("hotkey-stage-kill");
                            let _ = crate::stage::stage_hide_kill(app_clone).await;
                        });
                    }
                    Press::Cancel { abort_ghost } => {
                        // Ctrl+Space → agent is active: cancel calling agent.
                        tracing::info!(
                            "hotkey ({}) → agent is active (capturing={}, speaking={}, thinking={}, ghost={}), CANCELING calling agent",
                            hk, state.capturing, state.speaking, state.has_request, state.ghost
                        );

                        // 1. Stop TTS audio immediately
                        let _ = crate::tts::stop_tts();
                        let _ = tauri::Emitter::emit(&handle, "tts:stop", ());
                        if let Some(ms) = handle.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>() {
                            ms.set_tts_playing(false);
                        }

                        // 2. Cancel in-flight orchestrator requests
                        crate::orchestrator::cancel_active();

                        // 3. Abort STT capture
                        crate::wakeword_oww::abort_stt_capture();

                        // 4. If ghost session was active, abort it
                        if abort_ghost {
                            let app_clone = handle.clone();
                            tauri::async_runtime::spawn(async move {
                                let _ = crate::ghost::ghost_abort(app_clone).await;
                            });
                        }

                        // 5. Emit cancellation event to stage overlay to immediately hide orb & reset to idle
                        let _ = tauri::Emitter::emit(&handle, "orb:cancel", ());
                    }
                    Press::Call => {
                        // Agent is idle → CALL the agent
                        tracing::info!("hotkey ({}) → agent is idle, CALLING agent", hk);
                        println!("[ORB-HOTKEY] Hotkey ({hk}) → agent idle, calling agent & waking orb to listening");

                        // Cancel any stale request slot just in case
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

                        // "I heard you" sound (skipped in meetings / when switched off), then capture.
                        crate::wake_cue::play_for_wake(&handle);

                        // Start Rust-side STT capture (same as wake word path).
                        crate::wakeword_oww::start_stt_capture();
                        crate::window_manager::wake_orb_src(&handle, "hotkey");
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_space_decision_matrix_all_16_states() {
        for capturing in [false, true] {
            for speaking in [false, true] {
                for has_request in [false, true] {
                    for ghost in [false, true] {
                        let state = AgentState {
                            capturing,
                            speaking,
                            has_request,
                            ghost,
                        };
                        let press = decide("CommandOrControl+Space", state);
                        if !capturing && !speaking && !has_request && !ghost {
                            assert_eq!(press, Press::Call);
                        } else {
                            assert_eq!(press, Press::Cancel { abort_ghost: ghost });
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn settings_and_kill_stage_hotkeys_for_all_16_states() {
        for capturing in [false, true] {
            for speaking in [false, true] {
                for has_request in [false, true] {
                    for ghost in [false, true] {
                        let state = AgentState {
                            capturing,
                            speaking,
                            has_request,
                            ghost,
                        };
                        assert_eq!(
                            decide("CommandOrControl+Shift+S", state),
                            Press::OpenSettings
                        );
                        assert_eq!(
                            decide("CommandOrControl+Alt+X", state),
                            Press::KillStage
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_hotkey_behaves_like_ctrl_space() {
        for capturing in [false, true] {
            for speaking in [false, true] {
                for has_request in [false, true] {
                    for ghost in [false, true] {
                        let state = AgentState {
                            capturing,
                            speaking,
                            has_request,
                            ghost,
                        };
                        let unknown_decision = decide("CommandOrControl+Shift+Alt+Z", state);
                        let ctrl_space_decision = decide("CommandOrControl+Space", state);
                        assert_eq!(unknown_decision, ctrl_space_decision);
                    }
                }
            }
        }
    }

    #[test]
    fn hotkeys_entries_parse_and_are_distinct() {
        let mut parsed = Vec::new();
        for &hk in HOTKEYS {
            let sc = str::parse::<Shortcut>(hk)
                .unwrap_or_else(|e| panic!("failed to parse hotkey '{hk}': {e}"));
            parsed.push(sc);
        }

        assert_eq!(parsed.len(), 3);
        assert_ne!(parsed[0], parsed[1]);
        assert_ne!(parsed[0], parsed[2]);
        assert_ne!(parsed[1], parsed[2]);

        assert_ne!(HOTKEYS[0], HOTKEYS[1]);
        assert_ne!(HOTKEYS[0], HOTKEYS[2]);
        assert_ne!(HOTKEYS[1], HOTKEYS[2]);
    }

    #[test]
    fn hotkeys_has_exactly_three_documented_entries() {
        assert_eq!(HOTKEYS.len(), 3);
        assert_eq!(
            HOTKEYS,
            &[
                "CommandOrControl+Space",
                "CommandOrControl+Shift+S",
                "CommandOrControl+Alt+X",
            ]
        );
    }
}
