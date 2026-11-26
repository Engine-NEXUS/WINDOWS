//! Listening session: press Ctrl+Alt+L and give commands one after another without the wake word.
//!
//! The microphone re-opens by itself after each turn. Because it is open, every capture is treated like Ghost's open
//! mic: it ends after the short silence wait, and only speech the directed-speech gate accepts is acted on. Silence,
//! noise and ignored speech never reach the command pipeline and are never answered. It controls nothing by itself.
//!
//! Ends with: Ctrl+Alt+L again, Ctrl+Space, saying "stop listening", entering Ghost mode, or two minutes without an
//! accepted command. It waits while NEXUS speaks or works, in a meeting and while hand mode mutes voice.
//! Off by default (`listenSession`; takes effect at the next start).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) const HOTKEY: &str = "CommandOrControl+Alt+L";
/// Two minutes with no accepted command closes the microphone again.
const IDLE_END: Duration = Duration::from_secs(120);
/// NEXUS must have been quiet and idle this long before the microphone re-opens (the tail of its own voice).
const SETTLE: Duration = Duration::from_millis(800);
const TICK: Duration = Duration::from_millis(200);

static ACTIVE: AtomicBool = AtomicBool::new(false);
/// Bumped at every start and end, so an old pump thread stops.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static LAST_HEARD: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);

pub(crate) fn active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// What the app is doing right now, as the pump sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Busy {
    pub capturing: bool,
    pub speaking: bool,
    pub working: bool,
    pub meeting: bool,
    pub muted: bool,
}

impl Busy {
    fn any(self) -> bool {
        self.capturing || self.speaking || self.working || self.meeting || self.muted
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Wait,
    Listen,
    End,
}

/// One pump decision. Pure.
pub(crate) fn decide(busy: Busy, ghost: bool, idle_for: Duration, quiet_for: Duration) -> Step {
    if ghost || (idle_for >= IDLE_END && !busy.speaking && !busy.working) {
        Step::End
    } else if busy.any() || quiet_for < SETTLE {
        Step::Wait
    } else {
        Step::Listen
    }
}

/// "stop listening" and close variants end the session. The whole utterance only: "stop listening to music" does not. Pure.
pub(crate) fn is_end_phrase(text: &str) -> bool {
    let cleaned: String =
        text.to_lowercase().chars().filter(|c| c.is_alphanumeric() || c.is_whitespace()).collect();
    let mut words: Vec<&str> = cleaned.split_whitespace().collect();
    while words.first().is_some_and(|w| matches!(*w, "nexus" | "okay" | "ok" | "please")) {
        words.remove(0);
    }
    while words.last().is_some_and(|w| matches!(*w, "nexus" | "please" | "now" | "sir")) {
        words.pop();
    }
    matches!(words.join(" ").as_str(), "stop listening" | "end listening" | "thats all" | "that is all")
}

/// End the session, if one is running. `why` goes to the log.
pub(crate) fn end<R: Runtime>(app: &AppHandle<R>, why: &str) {
    if !ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
    crate::wakeword_oww::abort_stt_capture();
    let _ = tauri::Emitter::emit(app, "orb:hold_end", ());
    tracing::info!("[LISTEN] session ended: {why}");
}

/// A transcript from a session capture: `true` = drop it (never emitted, never answered).
pub(crate) fn drops<R: Runtime>(app: &AppHandle<R>, text: &str) -> bool {
    if text.trim().is_empty() {
        return true;
    }
    if is_end_phrase(text) {
        end(app, "asked to stop");
        return true;
    }
    if !crate::directed::directed_gate(text.to_string(), app.clone()).accept {
        return true;
    }
    *LAST_HEARD.lock() = Some(Instant::now());
    false
}

fn busy_now<R: Runtime>(app: &AppHandle<R>) -> Busy {
    let meeting_state = app.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>();
    Busy {
        capturing: crate::wakeword_oww::stt_capturing(),
        speaking: meeting_state.as_ref().is_some_and(|m| m.tts_playing.load(Ordering::Relaxed)),
        working: crate::orchestrator::has_active_request() || crate::turn_pending::is_pending(),
        meeting: meeting_state.as_ref().is_some_and(|m| m.is_meeting_active()),
        muted: crate::hand::suppresses_voice(),
    }
}

fn start<R: Runtime>(app: &AppHandle<R>) {
    ACTIVE.store(true, Ordering::SeqCst);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    *LAST_HEARD.lock() = Some(Instant::now());
    tracing::info!("[LISTEN] session started");
    crate::wake_cue::play_for_wake(app);
    let app = app.clone();
    std::thread::spawn(move || {
        let mut quiet_since: Option<Instant> = Some(Instant::now() - SETTLE);
        while ACTIVE.load(Ordering::SeqCst) && GENERATION.load(Ordering::SeqCst) == generation {
            let busy = busy_now(&app);
            if busy.any() {
                quiet_since = None;
            } else if quiet_since.is_none() {
                quiet_since = Some(Instant::now());
            }
            let idle_for = LAST_HEARD.lock().map_or(Duration::ZERO, |t| t.elapsed());
            let quiet_for = quiet_since.map_or(Duration::ZERO, |t| t.elapsed());
            match decide(busy, crate::ghost::session_active(), idle_for, quiet_for) {
                Step::End => {
                    end(&app, "idle or Ghost mode");
                    break;
                }
                Step::Listen => {
                    crate::wakeword_oww::start_listen_capture();
                    if crate::wakeword_oww::stt_capturing() {
                        crate::window_manager::show_orb_interactive(&app);
                        let _ = tauri::Emitter::emit(&app, "orb:hold", ());
                    }
                    quiet_since = None;
                }
                Step::Wait => {}
            }
            std::thread::sleep(TICK);
        }
    });
}

/// Register the key, only when the switch is on.
pub(crate) fn init<R: Runtime>(app: &AppHandle<R>) {
    if !crate::commands::read_flag(app, "listenSession") {
        return;
    }
    #[cfg(not(target_os = "linux"))]
    {
        use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
        let Ok(shortcut) = HOTKEY.parse::<Shortcut>() else {
            tracing::warn!("[LISTEN] could not read the key {HOTKEY}");
            return;
        };
        let handle = app.clone();
        match app.global_shortcut().on_shortcut(shortcut, move |_a, _s, event| {
            if event.state() != ShortcutState::Pressed {
                return;
            }
            if active() {
                end(&handle, "key pressed");
            } else if !crate::ghost::session_active() {
                crate::orchestrator::request_barge_in("listen-session");
                let _ = crate::tts::stop_tts();
                let _ = tauri::Emitter::emit(&handle, "tts:stop", ());
                start(&handle);
            }
        }) {
            Ok(()) => tracing::info!("[LISTEN] listening session registered on {HOTKEY}"),
            Err(e) => tracing::warn!("[LISTEN] could not register {HOTKEY} (another app may own it): {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    #[test]
    fn the_microphone_reopens_only_when_nexus_is_idle_and_has_settled() {
        let idle = Busy::default();
        assert_eq!(decide(idle, false, S, S), Step::Listen);
        assert_eq!(decide(idle, false, S, Duration::from_millis(500)), Step::Wait, "its own voice may still sound");
        for busy in [
            Busy { capturing: true, ..idle },
            Busy { speaking: true, ..idle },
            Busy { working: true, ..idle },
            Busy { meeting: true, ..idle },
            Busy { muted: true, ..idle },
        ] {
            assert_eq!(decide(busy, false, S, S), Step::Wait, "{busy:?}");
        }
    }

    #[test]
    fn it_ends_on_ghost_mode_or_two_quiet_minutes_but_not_mid_answer() {
        let idle = Busy::default();
        assert_eq!(decide(idle, true, S, S), Step::End);
        assert_eq!(decide(idle, false, IDLE_END, S), Step::End);
        assert_eq!(decide(Busy { capturing: true, ..idle }, false, IDLE_END, S), Step::End, "an open mic still times out");
        assert_eq!(decide(Busy { speaking: true, ..idle }, false, IDLE_END, S), Step::Wait);
        assert_eq!(decide(Busy { working: true, ..idle }, false, IDLE_END, S), Step::Wait);
    }

    #[test]
    fn only_a_whole_stop_sentence_ends_the_session() {
        for yes in ["Stop listening.", "Nexus, stop listening", "okay that's all", "stop listening now please"] {
            assert!(is_end_phrase(yes), "{yes}");
        }
        for no in ["stop listening to music", "stop", "why did you stop listening", "open notepad", ""] {
            assert!(!is_end_phrase(no), "{no}");
        }
    }

    #[test]
    fn the_key_does_not_collide_with_the_other_hotkeys() {
        assert!(!include_str!("hotkey.rs").contains(HOTKEY));
        assert_ne!(HOTKEY, crate::hold_to_talk::HOTKEY);
        assert_ne!(HOTKEY, crate::hold_to_talk::COMMAND_HOTKEY);
    }
}
