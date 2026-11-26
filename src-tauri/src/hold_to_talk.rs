//! Hold-to-talk dictation: hold Ctrl+Alt+D, speak, let go, and the words are typed into the window in front.
//!
//! Why it exists: a normal turn waits for 400 ms of silence before it knows you finished. Letting go of a key says that
//! at once. Everything else is the ordinary path on purpose: the same capture, the same owner check (Voice Lock), the
//! same speech-to-text choice (cloud, or local only in privacy mode), the same cleaning and the same typing refusals
//! (protected windows, NEXUS itself, elevated windows, password fields). Nothing said this way is a command.
//!
//! A second key, Ctrl+Alt+Enter (`holdToCommand`), uses the same held capture but runs the words as a command.
//!
//! Both are off by default (they take effect at the next start, because a registered key is taken from every app).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime};

pub(crate) const HOTKEY: &str = "CommandOrControl+Alt+D";
pub(crate) const COMMAND_HOTKEY: &str = "CommandOrControl+Alt+Enter";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// The words are typed into the window in front.
    Dictate,
    /// The words are run as a command.
    Command,
}
/// A press shorter than this is a slip of the finger, not a sentence.
const MIN_HOLD_MS: u64 = 300;
/// How long to wait for Ctrl, Alt, Shift and the Windows key to come up before typing.
const MODIFIER_WAIT: Duration = Duration::from_millis(3000);
/// A released capture that has not ended by now has no microphone audio arriving: drop it.
const STUCK_AFTER: Duration = Duration::from_millis(1500);

static PRESSED_AT_MS: AtomicU64 = AtomicU64::new(0);
/// The window in front when the key went down: text is typed only if it is still in front.
static TARGET_WINDOW: AtomicU64 = AtomicU64::new(0);
/// Which key is held; the other one is ignored until it is up.
static HELD_COMMAND: AtomicBool = AtomicBool::new(false);

fn now_ms() -> u64 {
    static EPOCH: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    // never 0: 0 means "not held"
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64 + 1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PressAction {
    /// Start a held capture.
    Start,
    /// Key repeat while already held.
    Ignore,
    /// Ghost mode owns the microphone.
    Refuse,
}

/// What a key-down does. Pure.
pub(crate) fn decide_press(already_held: bool, ghost: bool) -> PressAction {
    if already_held {
        PressAction::Ignore
    } else if ghost {
        PressAction::Refuse
    } else {
        PressAction::Start
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReleaseAction {
    /// End the capture and transcribe it.
    Finish,
    /// Too short to be speech: throw the capture away.
    Cancel,
    /// No press is open.
    Ignore,
}

/// What a key-up does after being held `held_ms` (`None` = no open press). Pure.
pub(crate) fn decide_release(held_ms: Option<u64>) -> ReleaseAction {
    match held_ms {
        None => ReleaseAction::Ignore,
        Some(ms) if ms < MIN_HOLD_MS => ReleaseAction::Cancel,
        Some(_) => ReleaseAction::Finish,
    }
}

/// Why text must not be typed now, as a log reason; `None` = type it. Pure.
pub(crate) fn typing_block(modifiers_down: bool, window_at_press: u64, window_now: u64) -> Option<&'static str> {
    if modifiers_down {
        // typed letters would arrive as Ctrl or Alt shortcuts
        Some("Ctrl, Alt, Shift or the Windows key is still held")
    } else if window_at_press != 0 && window_now != window_at_press {
        Some("another window came to the front")
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn foreground_window() -> u64 {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow().0 as u64 }
}
#[cfg(not(target_os = "windows"))]
fn foreground_window() -> u64 {
    0
}

#[cfg(target_os = "windows")]
fn modifiers_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
    [VK_CONTROL, VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN].iter().any(|k| unsafe { GetAsyncKeyState(k.0 as i32) } as u16 & 0x8000 != 0)
}
#[cfg(not(target_os = "windows"))]
fn modifiers_down() -> bool {
    false
}

fn press<R: Runtime>(app: &AppHandle<R>, kind: Kind) {
    let held = PRESSED_AT_MS.load(Ordering::SeqCst) != 0;
    match decide_press(held, crate::ghost::session_active()) {
        PressAction::Ignore => {}
        PressAction::Refuse => tracing::info!("[HOLD] ignored: Ghost mode owns the microphone"),
        PressAction::Start => {
            PRESSED_AT_MS.store(now_ms(), Ordering::SeqCst);
            HELD_COMMAND.store(kind == Kind::Command, Ordering::SeqCst);
            TARGET_WINDOW.store(foreground_window(), Ordering::SeqCst);
            // whatever NEXUS is saying or doing stops: the owner is talking
            crate::orchestrator::request_barge_in("hold-to-talk");
            let _ = crate::tts::stop_tts();
            let _ = tauri::Emitter::emit(app, "tts:stop", ());
            crate::wakeword_oww::abort_stt_capture();
            crate::wakeword_oww::start_hold_capture(kind == Kind::Dictate);
            if kind == Kind::Command && crate::wakeword_oww::stt_capturing() {
                // show the listening orb without the wake handler, which would abort a capture that is already open
                crate::window_manager::show_orb_interactive(app);
                let _ = tauri::Emitter::emit(app, "orb:hold", ());
                let _ = tauri::Emitter::emit(app, "orb:key_held", true);
            }
            tracing::info!("[HOLD] listening ({kind:?})");
        }
    }
}

fn release<R: Runtime>(app: &AppHandle<R>, kind: Kind) {
    if HELD_COMMAND.load(Ordering::SeqCst) != (kind == Kind::Command) {
        return;
    }
    if kind == Kind::Command {
        let _ = tauri::Emitter::emit(app, "orb:key_held", false);
    }
    let at = PRESSED_AT_MS.swap(0, Ordering::SeqCst);
    let held = (at != 0).then(|| now_ms().saturating_sub(at));
    match decide_release(held) {
        ReleaseAction::Ignore => {}
        ReleaseAction::Cancel => {
            crate::wakeword_oww::abort_stt_capture();
            if kind == Kind::Command {
                let _ = tauri::Emitter::emit(app, "orb:hold_end", ());
            }
            tracing::info!("[HOLD] too short, dropped");
        }
        ReleaseAction::Finish => {
            let session = crate::wakeword_oww::release_hold_capture();
            tracing::info!("[HOLD] released after {} ms", held.unwrap_or(0));
            if let Some(session) = session {
                let app = app.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(STUCK_AFTER);
                    if crate::wakeword_oww::drop_stuck_hold(session) {
                        if kind == Kind::Command {
                            let _ = tauri::Emitter::emit(&app, "orb:hold_end", ());
                        }
                        tracing::warn!("[HOLD] no microphone audio arrived: capture dropped");
                    }
                });
            }
        }
    }
}

/// Type the transcript of a held capture. Blocking (waits for the modifier keys); returns whether text was typed.
pub(crate) fn deliver<R: Runtime>(app: &AppHandle<R>, text: &str, released_at: Instant) -> bool {
    let transcribed_ms = released_at.elapsed().as_millis();
    if text.trim().is_empty() {
        tracing::info!("[HOLD] nothing heard ({transcribed_ms} ms after release)");
        return false;
    }
    let deadline = Instant::now() + MODIFIER_WAIT;
    while modifiers_down() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if let Some(why) = typing_block(modifiers_down(), TARGET_WINDOW.load(Ordering::SeqCst), foreground_window()) {
        tracing::warn!("[HOLD] not typed: {why}");
        return false;
    }
    let dictionary = app.path().app_data_dir().ok().map(|d| crate::dictation::load_dictionary(&d)).unwrap_or_default();
    let outcome = crate::dictation::dictate_oneshot(text, dictionary);
    let total_ms = released_at.elapsed().as_millis();
    use crate::dictation::Outcome;
    let (typed, note) = match &outcome {
        Outcome::Typed(_) => (true, "typed".to_string()),
        Outcome::Unconfirmed => (true, "sent once, not confirmed in the field".to_string()),
        Outcome::Undone => (true, "undid the previous text".to_string()),
        Outcome::Nothing => (false, "nothing to type".to_string()),
        Outcome::Refused(why) => (false, format!("refused: {why}")),
    };
    // the measurement: key up -> words in the window (the text itself is never logged)
    tracing::info!("[HOLD] release->transcript {transcribed_ms} ms, release->done {total_ms} ms: {note}");
    typed
}

/// Register each key whose switch is on.
pub(crate) fn init<R: Runtime>(app: &AppHandle<R>) {
    for (setting, key, kind) in
        [("holdToTalk", HOTKEY, Kind::Dictate), ("holdToCommand", COMMAND_HOTKEY, Kind::Command)]
    {
        if !crate::commands::read_flag(app, setting) {
            continue;
        }
        #[cfg(not(target_os = "linux"))]
        {
            use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
            let Ok(shortcut) = key.parse::<Shortcut>() else {
                tracing::warn!("[HOLD] could not read the key {key}");
                continue;
            };
            let handle = app.clone();
            match app.global_shortcut().on_shortcut(shortcut, move |_a, _s, event| match event.state() {
                ShortcutState::Pressed => press(&handle, kind),
                ShortcutState::Released => release(&handle, kind),
            }) {
                Ok(()) => tracing::info!("[HOLD] {kind:?} registered on {key}"),
                Err(e) => tracing::warn!("[HOLD] could not register {key} (another app may own it): {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_down_starts_once_and_never_during_ghost_mode() {
        assert_eq!(decide_press(false, false), PressAction::Start);
        assert_eq!(decide_press(true, false), PressAction::Ignore, "key repeat");
        assert_eq!(decide_press(true, true), PressAction::Ignore);
        assert_eq!(decide_press(false, true), PressAction::Refuse);
    }

    #[test]
    fn a_key_up_finishes_a_real_hold_and_drops_a_slip() {
        assert_eq!(decide_release(None), ReleaseAction::Ignore);
        assert_eq!(decide_release(Some(MIN_HOLD_MS - 1)), ReleaseAction::Cancel);
        assert_eq!(decide_release(Some(MIN_HOLD_MS)), ReleaseAction::Finish);
    }

    #[test]
    fn text_is_never_typed_with_modifiers_held_or_into_another_window() {
        assert!(typing_block(true, 5, 5).is_some(), "letters would become shortcuts");
        assert!(typing_block(false, 5, 6).is_some(), "the window changed");
        assert_eq!(typing_block(false, 5, 5), None);
        assert_eq!(typing_block(false, 0, 7), None, "unknown at press: the typing refusals still apply");
    }

    #[test]
    fn the_key_does_not_collide_with_the_other_hotkeys() {
        assert_ne!(HOTKEY, "CommandOrControl+Space");
        assert!(!include_str!("hotkey.rs").contains(&format!("{HOTKEY:?}")));
        assert_ne!(COMMAND_HOTKEY, HOTKEY);
        assert!(!include_str!("hotkey.rs").contains(&format!("{COMMAND_HOTKEY:?}")));
    }

    #[test]
    fn the_transcript_text_is_never_logged() {
        let src = include_str!("hold_to_talk.rs");
        let prod = &src[..src.find("#[cfg(test)]\nmod tests").or_else(|| src.find("#[cfg(test)]")).unwrap_or(src.len())];
        for line in prod.lines().filter(|l| l.contains("tracing::")) {
            assert!(!line.contains("{text") && !line.contains(", text"), "{line}");
        }
    }
}
