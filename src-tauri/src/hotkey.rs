//! Global hotkeys and action dispatch.
//!
//! Hotkeys:
//!   - `CommandOrControl+Space`: Symmetric agent call / cancel.
//!     If the agent is active (capturing audio, speaking TTS, processing a
//!     request, or running a ghost session), cancels the active agent and
//!     hides the orb. If the agent is idle, calls the agent (plays the wake
//!     cue, starts STT capture, and wakes the orb). Windows (settings, sidebar,
//!     setup) are closed by Escape, never by this hotkey.
//!     Tap: silence ends the turn. Hold and talk: it listens until the key
//!     comes up (60 s at most), then ends at once.
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
    // Sidebar depth, from any window: the sidebar cannot hear the plain arrow keys while another window has focus
    // (or while it is behind everything), so these work without focusing it.
    "CommandOrControl+Alt+Down",
    "CommandOrControl+Alt+Up",
];

/// Ctrl+Space held at least this long is "listen while I hold it"; anything shorter is a tap.
const HELD_AFTER_MS: u128 = 400;
/// A released capture that is still open this much later has no microphone audio arriving.
const STUCK_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

/// When the Ctrl+Space press that called NEXUS went down.
static CALL_DOWN: parking_lot::Mutex<Option<std::time::Instant>> = parking_lot::Mutex::new(None);

/// Was this a hold (end the turn now) rather than a tap (let silence end it)? Pure.
pub(crate) fn was_held(held_ms: u128) -> bool {
    held_ms >= HELD_AFTER_MS
}

fn call_key_released<R: Runtime>(app: &AppHandle<R>) {
    let Some(down) = CALL_DOWN.lock().take() else { return };
    let _ = tauri::Emitter::emit(app, "orb:key_held", false);
    let held = down.elapsed().as_millis();
    if !was_held(held) {
        crate::wakeword_oww::unhold_capture();
        return;
    }
    tracing::info!("hotkey: Ctrl+Space released after {held} ms, ending the turn");
    if let Some(session) = crate::wakeword_oww::release_hold_capture() {
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(STUCK_AFTER);
            if crate::wakeword_oww::drop_stuck_hold(session) {
                let _ = tauri::Emitter::emit(&app, "orb:hold_end", ());
                tracing::warn!("hotkey: no microphone audio arrived, capture dropped");
            }
        });
    }
}

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
    /// Sidebar behind every window (`true`) or back in front (`false`).
    SidebarDepth { back: bool },
    Cancel { abort_ghost: bool },
    Call,
}

/// Pure hotkey action decision based on hotkey chord and live agent activity.
pub(crate) fn decide(hotkey: &str, state: AgentState) -> Press {
    match hotkey {
        "CommandOrControl+Shift+S" => Press::OpenSettings,
        "CommandOrControl+Alt+X" => Press::KillStage,
        "CommandOrControl+Alt+Down" => Press::SidebarDepth { back: true },
        "CommandOrControl+Alt+Up" => Press::SidebarDepth { back: false },
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
            if event.state() == ShortcutState::Released && hk == "CommandOrControl+Space" {
                call_key_released(&handle);
            }
            if event.state() == ShortcutState::Pressed {
                let state = AgentState {
                    capturing: crate::wakeword_oww::stt_capturing(),
                    speaking: handle
                        .try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
                        .map(|ms| ms.tts_playing.load(std::sync::atomic::Ordering::Relaxed))
                        .unwrap_or(false),
                    // A heard turn still being transcribed counts as "thinking": no request exists yet.
                    has_request: crate::orchestrator::has_active_request() || crate::turn_pending::is_pending(),
                    ghost: crate::ghost::session_active(),
                };
                // between two captures a listening session looks idle: Ctrl+Space must still end it, not call NEXUS
                if hk == "CommandOrControl+Space" && crate::listen_session::active() {
                    crate::listen_session::end(&handle, "Ctrl+Space");
                    if !(state.speaking || state.has_request) {
                        return;
                    }
                }

                match decide(hk, state) {
                    Press::OpenSettings => {
                        // Ctrl+Shift+S → open settings sidebar directly
                        tracing::info!("hotkey ({}) → opening settings sidebar", hk);
                        let app_clone = handle.clone();
                        tauri::async_runtime::spawn(async move {
                            let _ = crate::commands::show_settings_sidebar(app_clone).await;
                        });
                    }
                    Press::SidebarDepth { back } => {
                        tracing::info!("hotkey ({}) → sidebar {}", hk, if back { "behind every window" } else { "to the front" });
                        crate::commands::apply_sidebar_depth(&handle, back);
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
                        // ... and the turn that is still on its way from speech-to-text, if any
                        crate::turn_pending::cancel();

                        // ... and a listening session, which would otherwise re-open the microphone
                        crate::listen_session::end(&handle, "Ctrl+Space");

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

                        // Start Rust-side STT capture. While the key stays down silence does not end it; a tap
                        // turns it back into an ordinary capture on release.
                        *CALL_DOWN.lock() = Some(std::time::Instant::now());
                        crate::wakeword_oww::start_hold_capture(false);
                        let _ = tauri::Emitter::emit(&handle, "orb:key_held", true);
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
    fn a_short_press_is_a_tap_and_a_long_one_is_a_hold() {
        assert!(!was_held(0));
        assert!(!was_held(HELD_AFTER_MS - 1));
        assert!(was_held(HELD_AFTER_MS));
        assert!(was_held(30_000));
    }

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

        assert_eq!(parsed.len(), HOTKEYS.len());
        for i in 0..parsed.len() {
            for j in (i + 1)..parsed.len() {
                assert_ne!(parsed[i], parsed[j], "{} and {} must be different chords", HOTKEYS[i], HOTKEYS[j]);
                assert_ne!(HOTKEYS[i], HOTKEYS[j]);
            }
        }
    }

    #[test]
    fn hotkeys_has_exactly_the_documented_entries() {
        assert_eq!(
            HOTKEYS,
            &[
                "CommandOrControl+Space",
                "CommandOrControl+Shift+S",
                "CommandOrControl+Alt+X",
                "CommandOrControl+Alt+Down",
                "CommandOrControl+Alt+Up",
            ]
        );
    }

    #[test]
    fn depth_chords_move_the_sidebar_without_touching_the_agent() {
        let idle = AgentState { capturing: false, speaking: false, has_request: false, ghost: false };
        let busy = AgentState { capturing: true, speaking: true, has_request: true, ghost: true };
        for st in [idle, busy] {
            assert_eq!(decide("CommandOrControl+Alt+Down", st), Press::SidebarDepth { back: true });
            assert_eq!(decide("CommandOrControl+Alt+Up", st), Press::SidebarDepth { back: false });
        }
    }
}
