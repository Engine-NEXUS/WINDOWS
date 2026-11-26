//! Typing by voice, with one key and one name.
//!
//! * **Hold Ctrl+Space** with the cursor in a text box: what you say is typed there; letting go means done. With no
//!   text box under the cursor the same hold is a command (see hotkey.rs). A tap is always a command.
//! * **"Nexus, type for me"**: the microphone stays open, sentence after sentence, and each is typed. **"done"** said
//!   on its own ends it and leaves the text; **"Nexus, send"** presses the send key once, only in the window the
//!   typing went into. Ctrl+Space also ends it, and so do 30 quiet seconds.
//!
//! The owner check (Voice Lock), the speech-to-text choice and the typing refusals (protected, elevated and password
//! targets, NEXUS itself) are the ordinary ones.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, Runtime};

/// How long to wait for Ctrl, Alt, Shift and the Windows key to come up before typing.
const MODIFIER_WAIT: Duration = Duration::from_millis(3000);
/// Hands-free typing ends after this long without a sentence.
const IDLE_END: Duration = Duration::from_secs(30);
/// NEXUS must have been quiet this long after speaking before the microphone opens (the tail of its own voice).
const SETTLE: Duration = Duration::from_millis(800);
/// How long to wait for NEXUS to start announcing the typing mode before opening the microphone anyway.
const ANNOUNCE_WAIT: Duration = Duration::from_millis(2500);
/// Short, so the gap between one sentence and the next recording stays under one audio chunk.
const TICK: Duration = Duration::from_millis(20);

// ─── is the cursor in a text box? ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    Edit,
    Document,
    /// Combo boxes, groups, panes and custom controls: text boxes only when they say their value can be changed.
    Container,
    Other,
}

/// Can text be typed into the focused control? `read_only` is what its value pattern says, if it has one. Pure.
pub(crate) fn is_text_box(control: Control, read_only: Option<bool>, editor_app: bool) -> bool {
    match control {
        Control::Edit => read_only != Some(true),
        // a web page is a read-only document; a mail body or an editor's page is not
        Control::Document => read_only == Some(false) || (read_only.is_none() && editor_app),
        Control::Container => read_only == Some(false),
        Control::Other => false,
    }
}

/// Programs whose main page is a document you type into but which expose no editable value.
fn is_editor_app(process: &str) -> bool {
    ["notepad.exe", "winword.exe", "wordpad.exe", "onenote.exe", "soffice.bin"]
        .iter()
        .any(|p| process.eq_ignore_ascii_case(p))
}

#[cfg(target_os = "windows")]
fn focused_is_text_box() -> bool {
    use uiautomation::patterns::UIValuePattern;
    use uiautomation::types::ControlType;
    let Ok(automation) = uiautomation::UIAutomation::new() else { return false };
    let Ok(element) = automation.get_focused_element() else { return false };
    let control = match element.get_control_type() {
        Ok(ControlType::Edit) => Control::Edit,
        Ok(ControlType::Document) => Control::Document,
        Ok(ControlType::ComboBox | ControlType::Group | ControlType::Pane | ControlType::Custom) => Control::Container,
        _ => Control::Other,
    };
    let read_only = element.get_pattern::<UIValuePattern>().ok().and_then(|p| p.is_readonly().ok());
    let process = crate::browser_url::foreground_process_name().unwrap_or_default();
    is_text_box(control, read_only, is_editor_app(&process))
}
#[cfg(not(target_os = "windows"))]
fn focused_is_text_box() -> bool {
    false
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

/// Why text must not be typed (or send pressed) now; `None` = go on. Pure.
pub(crate) fn typing_block(modifiers_down: bool, window_before: u64, window_now: u64) -> Option<&'static str> {
    if modifiers_down {
        // typed letters would arrive as Ctrl or Alt shortcuts
        Some("Ctrl, Alt, Shift or the Windows key is still held")
    } else if window_before != 0 && window_now != window_before {
        Some("another window came to the front")
    } else {
        None
    }
}

fn wait_for_modifiers() {
    let deadline = Instant::now() + MODIFIER_WAIT;
    while modifiers_down() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn dictionary<R: Runtime>(app: &AppHandle<R>) -> Vec<(String, String)> {
    app.path().app_data_dir().ok().map(|d| crate::dictation::load_dictionary(&d)).unwrap_or_default()
}

// ─── hold Ctrl+Space ────────────────────────────────────────────────────────

const BOX_UNKNOWN: u8 = 0;
const BOX_YES: u8 = 1;
const BOX_NO: u8 = 2;
static BOX_AT_PRESS: AtomicU8 = AtomicU8::new(BOX_UNKNOWN);
/// The window in front when the key went down: held text is typed only if it is still in front.
static WINDOW_AT_PRESS: AtomicU64 = AtomicU64::new(0);

/// The talk key went down: note the window, and look (off this thread) whether the cursor is in a text box.
pub(crate) fn note_key_down() {
    BOX_AT_PRESS.store(BOX_UNKNOWN, Ordering::SeqCst);
    WINDOW_AT_PRESS.store(foreground_window(), Ordering::SeqCst);
    std::thread::spawn(|| {
        let found = if focused_is_text_box() { BOX_YES } else { BOX_NO };
        BOX_AT_PRESS.store(found, Ordering::SeqCst);
    });
}

/// Was the cursor in a text box when the key went down? Waits briefly for the look; unknown counts as no.
pub(crate) fn box_at_press() -> bool {
    let deadline = Instant::now() + Duration::from_millis(500);
    while BOX_AT_PRESS.load(Ordering::SeqCst) == BOX_UNKNOWN && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    BOX_AT_PRESS.load(Ordering::SeqCst) == BOX_YES
}

/// Type the transcript of a held capture. Blocking (waits for the modifier keys).
pub(crate) fn deliver_held<R: Runtime>(app: &AppHandle<R>, text: &str, released_at: Instant) {
    if text.trim().is_empty() {
        tracing::info!("[TYPE] held: nothing heard");
        return;
    }
    wait_for_modifiers();
    if let Some(why) = typing_block(modifiers_down(), WINDOW_AT_PRESS.load(Ordering::SeqCst), foreground_window()) {
        tracing::warn!("[TYPE] held: not typed, {why}");
        return;
    }
    let outcome = crate::dictation::dictate_oneshot(text, dictionary(app));
    // key up -> words in the window; the words themselves are never logged
    tracing::info!("[TYPE] held: release->done {} ms, {}", released_at.elapsed().as_millis(), outcome_note(&outcome));
}

fn outcome_note(outcome: &crate::dictation::Outcome) -> &'static str {
    use crate::dictation::Outcome;
    match outcome {
        Outcome::Typed(_) => "typed",
        Outcome::Unconfirmed => "sent once, not confirmed in the field",
        Outcome::Undone => "undid the previous text",
        Outcome::Nothing => "nothing to type",
        Outcome::Refused(_) => "refused",
    }
}

// ─── hands-free typing ──────────────────────────────────────────────────────

/// What one sentence of hands-free typing means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Sentence {
    Text,
    Done,
    Send,
}

/// Orders count only as the whole sentence. "done" ends it; sending needs the name ("Nexus, send"), because a sent
/// message cannot be taken back. Everything else, including "send me the file", is text. Pure.
pub(crate) fn classify(text: &str) -> Sentence {
    let lower = text.to_lowercase();
    let mut words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    while words.first().is_some_and(|w| matches!(*w, "okay" | "ok" | "hey")) {
        words.remove(0);
    }
    let named = words.first().is_some_and(|w| matches!(*w, "nexus" | "nexas" | "nexis" | "nexes"));
    if named {
        words.remove(0);
    }
    match words.join(" ").as_str() {
        "done" | "done typing" | "stop typing" | "stop dictation" => Sentence::Done,
        "stop" if named => Sentence::Done,
        "send" | "send it" | "send the message" | "send message" if named => Sentence::Send,
        _ => Sentence::Text,
    }
}

static STREAMING: AtomicBool = AtomicBool::new(false);
/// Captures numbered from here on belong to the running stream (0 = none opened yet).
static FIRST_CAPTURE: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
/// Counts notices NEXUS is about to speak during a stream: the pump waits for each before listening again.
static NOTICES: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct Stream {
    last_sentence: Option<Instant>,
    /// The window the text went into (0 = nothing typed yet).
    window: u64,
    told_no_box: bool,
}
static STREAM: parking_lot::Mutex<Option<Stream>> = parking_lot::Mutex::new(None);

pub(crate) fn active() -> bool {
    STREAMING.load(Ordering::SeqCst)
}

/// Does capture `session` belong to the running stream? Its audio stays valid although later captures have started.
pub(crate) fn owns(session: u64) -> bool {
    let first = FIRST_CAPTURE.load(Ordering::SeqCst);
    active() && first != 0 && session >= first
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step {
    Wait,
    Listen,
    End,
}

/// One pump decision. `quiet_for` is `None` while NEXUS has not yet spoken in this stream. Pure.
pub(crate) fn decide(
    wanted: bool,
    blocked: bool,
    capturing: bool,
    speaking: bool,
    since_start: Duration,
    quiet_for: Option<Duration>,
    idle_for: Duration,
) -> Step {
    if !wanted || blocked || idle_for >= IDLE_END {
        return Step::End;
    }
    let settled = match quiet_for {
        Some(quiet) => quiet >= SETTLE,
        None => since_start >= ANNOUNCE_WAIT,
    };
    if capturing || speaking || !settled {
        Step::Wait
    } else {
        Step::Listen
    }
}

/// Start hands-free typing (the dictation flag is already set by the caller).
pub(crate) fn start_stream<R: Runtime>(app: &AppHandle<R>) {
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    FIRST_CAPTURE.store(0, Ordering::SeqCst);
    *STREAM.lock() = Some(Stream { last_sentence: Some(Instant::now()), ..Default::default() });
    STREAMING.store(true, Ordering::SeqCst);
    tracing::info!("[TYPE] hands-free typing started");
    let app = app.clone();
    std::thread::spawn(move || {
        let mut started = Instant::now();
        let mut notices = NOTICES.load(Ordering::SeqCst);
        let mut spoke = false;
        let mut quiet_since: Option<Instant> = None;
        while active() && GENERATION.load(Ordering::SeqCst) == generation {
            let meeting_state = app.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>();
            let speaking = meeting_state.as_ref().is_some_and(|m| m.tts_playing.load(Ordering::Relaxed));
            let latest = NOTICES.load(Ordering::SeqCst);
            if latest != notices {
                // a notice is on its way to the speaker: wait for it as for the first announcement
                notices = latest;
                started = Instant::now();
                spoke = false;
                quiet_since = None;
            }
            if speaking {
                // never record NEXUS's own voice, whoever asked it to speak
                if crate::wakeword_oww::stt_capturing() {
                    crate::wakeword_oww::abort_stt_capture();
                }
                spoke = true;
                quiet_since = None;
            } else if spoke && quiet_since.is_none() {
                quiet_since = Some(Instant::now());
            }
            let blocked = crate::ghost::session_active() || meeting_state.as_ref().is_some_and(|m| m.is_meeting_active());
            let idle_for = STREAM.lock().as_ref().and_then(|s| s.last_sentence).map_or(Duration::ZERO, |t| t.elapsed());
            let quiet_for = spoke.then(|| quiet_since.map_or(Duration::ZERO, |t| t.elapsed()));
            let wanted = crate::orchestrator::is_dictation_active();
            let capturing = crate::wakeword_oww::stt_capturing();
            match decide(wanted, blocked, capturing, speaking, started.elapsed(), quiet_for, idle_for) {
                Step::End => {
                    end(&app, "stopped, idle, a meeting or Ghost mode");
                    if idle_for >= IDLE_END {
                        say(&app, "I stopped typing, sir. I heard nothing for thirty seconds.");
                    }
                    break;
                }
                Step::Listen => {
                    if let Some(session) = crate::wakeword_oww::start_typing_capture() {
                        let _ = FIRST_CAPTURE.compare_exchange(0, session, Ordering::SeqCst, Ordering::SeqCst);
                        crate::window_manager::show_orb_interactive(&app);
                        let _ = tauri::Emitter::emit(&app, "orb:key_held", true);
                        let _ = tauri::Emitter::emit(&app, "orb:hold", ());
                    }
                }
                Step::Wait => {}
            }
            std::thread::sleep(TICK);
        }
    });
}

/// End hands-free typing, if it is running. The text stays where it is.
pub(crate) fn end<R: Runtime>(app: &AppHandle<R>, why: &str) {
    if !STREAMING.swap(false, Ordering::SeqCst) {
        return;
    }
    GENERATION.fetch_add(1, Ordering::SeqCst);
    *STREAM.lock() = None;
    crate::orchestrator::set_dictation_active(false);
    crate::wakeword_oww::abort_stt_capture();
    let _ = tauri::Emitter::emit(app, "orb:key_held", false);
    let _ = tauri::Emitter::emit(app, "orb:hold_end", ());
    tracing::info!("[TYPE] hands-free typing ended: {why}");
}

fn say<R: Runtime>(app: &AppHandle<R>, line: &str) {
    if active() {
        NOTICES.fetch_add(1, Ordering::SeqCst);
        crate::wakeword_oww::abort_stt_capture();
    }
    crate::orchestrator::announce(app, line);
}

/// One transcribed sentence of the stream, in the order spoken. Blocking.
pub(crate) fn sentence<R: Runtime>(app: &AppHandle<R>, text: &str) {
    if !active() || text.trim().is_empty() {
        return;
    }
    let (window, told_no_box) = match STREAM.lock().as_mut() {
        Some(s) => {
            s.last_sentence = Some(Instant::now());
            (s.window, s.told_no_box)
        }
        None => return,
    };
    match classify(text) {
        Sentence::Done => {
            end(app, "done");
            say(app, "Done, sir.");
        }
        Sentence::Send => {
            let blocked = typing_block(modifiers_down(), window, foreground_window());
            end(app, "send");
            if window == 0 {
                say(app, "I haven't typed anything, sir, so I didn't send.");
            } else if blocked.is_some() {
                say(app, "You've moved to another window, sir, so I didn't send.");
            } else if let Some(why) = crate::dictation::refusal_now() {
                say(app, &why);
            } else {
                match press_send() {
                    Ok(()) => say(app, "I pressed send, sir."),
                    Err(e) => {
                        tracing::warn!("[TYPE] send key failed: {e}");
                        say(app, "I couldn't press send, sir.");
                    }
                }
            }
        }
        Sentence::Text => {
            wait_for_modifiers();
            let now = foreground_window();
            if let Some(why) = typing_block(modifiers_down(), window, now) {
                tracing::warn!("[TYPE] not typed: {why}");
                end(app, why);
                say(app, "I stopped typing, sir. The window changed.");
                return;
            }
            if !focused_is_text_box() {
                tracing::warn!("[TYPE] not typed: the cursor is not in a text box");
                if !told_no_box {
                    if let Some(s) = STREAM.lock().as_mut() {
                        s.told_no_box = true;
                    }
                    say(app, "I can't see a text box, sir. Click where you want the text.");
                }
                return;
            }
            let outcome = crate::dictation::dictate(text, dictionary(app));
            tracing::info!("[TYPE] sentence {}", outcome_note(&outcome));
            match outcome {
                crate::dictation::Outcome::Refused(why) => say(app, &why),
                crate::dictation::Outcome::Typed(_) | crate::dictation::Outcome::Unconfirmed => {
                    if let Some(s) = STREAM.lock().as_mut() {
                        s.window = now;
                    }
                }
                _ => {}
            }
        }
    }
}

/// Mail apps send with Ctrl+Enter; chats send with Enter. Pure.
pub(crate) fn sends_with_ctrl_enter(title: &str, process: &str) -> bool {
    let title = title.to_lowercase();
    process.eq_ignore_ascii_case("outlook.exe") || title.contains("gmail") || title.contains("outlook")
}

fn press_send() -> Result<(), String> {
    let title = crate::browser_url::foreground_window_title().unwrap_or_default();
    let process = crate::browser_url::foreground_process_name().unwrap_or_default();
    if sends_with_ctrl_enter(&title, &process) {
        crate::live::commands::keyboard::press_hotkey(&["ctrl", "enter"])
    } else {
        crate::live::commands::keyboard::press_key("enter")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    #[test]
    fn only_a_place_you_can_write_counts_as_a_text_box() {
        assert!(is_text_box(Control::Edit, None, false));
        assert!(is_text_box(Control::Edit, Some(false), false));
        assert!(!is_text_box(Control::Edit, Some(true), false), "a read-only field");
        assert!(!is_text_box(Control::Document, Some(true), false), "a web page");
        assert!(is_text_box(Control::Document, Some(false), false), "a mail body");
        assert!(is_text_box(Control::Document, None, true), "an editor's page");
        assert!(!is_text_box(Control::Document, None, false));
        assert!(is_text_box(Control::Container, Some(false), false));
        assert!(!is_text_box(Control::Container, None, true));
        assert!(!is_text_box(Control::Other, Some(false), true), "a list item or a button");
    }

    #[test]
    fn done_ends_and_sending_needs_the_name() {
        for t in ["done", "Done.", "Nexus, done", "stop typing", "okay done typing", "Nexus stop"] {
            assert_eq!(classify(t), Sentence::Done, "{t}");
        }
        for t in ["Nexus, send.", "nexus send it", "Hey Nexus, send the message"] {
            assert_eq!(classify(t), Sentence::Send, "{t}");
        }
    }

    #[test]
    fn everything_else_is_text() {
        for t in [
            "send",
            "send it",
            "stop",
            "cancel",
            "send me the file tomorrow",
            "I am done with the report",
            "Nexus, send me the file",
            "see you tomorrow, Nexus send",
            "Nexus is the name of the app",
            "next send",
        ] {
            assert_eq!(classify(t), Sentence::Text, "{t}");
        }
    }

    #[test]
    fn the_microphone_opens_after_the_announcement_and_reopens_at_once() {
        let none = Duration::ZERO;
        // not announced yet: wait, but not for ever
        assert_eq!(decide(true, false, false, false, S, None, none), Step::Wait);
        assert_eq!(decide(true, false, false, false, ANNOUNCE_WAIT, None, none), Step::Listen);
        assert_eq!(decide(true, false, false, true, S, Some(none), none), Step::Wait, "NEXUS is speaking");
        assert_eq!(decide(true, false, false, false, S, Some(Duration::from_millis(300)), none), Step::Wait, "its echo");
        assert_eq!(decide(true, false, false, false, S, Some(SETTLE), none), Step::Listen);
        assert_eq!(decide(true, false, true, false, S, Some(S), none), Step::Wait, "already recording");
    }

    #[test]
    fn typing_ends_when_stopped_blocked_or_quiet() {
        assert_eq!(decide(false, false, false, false, S, Some(S), Duration::ZERO), Step::End);
        assert_eq!(decide(true, true, false, false, S, Some(S), Duration::ZERO), Step::End);
        assert_eq!(decide(true, false, true, false, S, Some(S), IDLE_END), Step::End);
    }

    #[test]
    fn nothing_is_typed_or_sent_with_modifiers_held_or_in_another_window() {
        assert!(typing_block(true, 5, 5).is_some(), "letters would become shortcuts");
        assert!(typing_block(false, 5, 6).is_some(), "the window changed");
        assert_eq!(typing_block(false, 5, 5), None);
        assert_eq!(typing_block(false, 0, 7), None, "nothing typed yet");
    }

    #[test]
    fn mail_sends_with_ctrl_enter() {
        assert!(sends_with_ctrl_enter("Inbox - Gmail - Brave", "brave.exe"));
        assert!(sends_with_ctrl_enter("Re: hello", "OUTLOOK.EXE"));
        assert!(!sends_with_ctrl_enter("WhatsApp", "whatsapp.exe"));
    }

    #[test]
    fn the_words_are_never_logged() {
        let src = include_str!("typing.rs");
        let prod = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];
        for line in prod.lines().filter(|l| l.contains("tracing::")) {
            assert!(!line.contains("{text") && !line.contains(", text"), "{line}");
        }
    }
}
