//! Pending offers (plan P4): the one-question-at-a-time state behind
//! "Shall I start?", "In the app or the browser?", "Add these slots?".
//!
//! At most one offer exists. It only starts listening for a reply once the
//! line that asks the question has actually been spoken (`arm`), and it
//! expires on its own, so a stray "yes" minutes later can never trigger
//! something. Classification of the reply is pure and strict: long or
//! unrelated sentences are `Other` and fall through to normal handling.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::timetable::Slot;

/// How long an armed offer waits for an answer after its question was spoken.
pub const REPLY_WINDOW: Duration = Duration::from_secs(90);
/// How long an offer may wait to be spoken (the proactive policy can defer it
/// through a meeting) before it is dropped.
pub const SPEAK_WAIT: Duration = Duration::from_secs(15 * 60);
/// Mic window the frontend opens after the question is spoken.
pub const LISTEN_WINDOW_MS: u64 = 8_000;
/// Replies longer than this are never treated as a bare yes/no.
const MAX_REPLY_WORDS: usize = 9;

#[derive(Debug, Clone, PartialEq)]
pub enum Offer {
    /// "Time for DSA. Shall I start?"
    Start { title: String, activity: String },
    /// "In the app or the browser?" (first time for this activity)
    Choose { title: String, activity: String },
    /// "I found 6 slots. Add them?"
    AddSlots { slots: Vec<Slot>, label: String },
    /// "Erase your whole timetable?"
    ClearTimetable,
    /// "Add dentist tomorrow at 5 PM to your calendar?"
    AddEvent { summary: String, start_iso: String, end_iso: String, when: String },
}

struct State {
    offer: Offer,
    armed: bool,
    expires: Instant,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Register an offer. `armed` = its question is being spoken right now;
/// otherwise it arms when `arm()` is called (the reminder path).
pub fn set(offer: Offer, armed: bool) {
    let ttl = if armed { REPLY_WINDOW } else { SPEAK_WAIT };
    *lock() = Some(State { offer, armed, expires: Instant::now() + ttl });
}

/// The question was spoken: start the reply window.
pub fn arm() {
    if let Some(s) = lock().as_mut() {
        s.armed = true;
        s.expires = Instant::now() + REPLY_WINDOW;
    }
}

pub fn clear() {
    *lock() = None;
}

/// The live (unexpired) offer, if any. Expired offers are dropped.
pub fn peek() -> Option<Offer> {
    let mut g = lock();
    match g.as_ref() {
        Some(s) if s.expires > Instant::now() => Some(s.offer.clone()),
        Some(_) => {
            *g = None;
            None
        }
        None => None,
    }
}

/// Take the live offer (consuming it).
pub fn take() -> Option<Offer> {
    let o = peek();
    if o.is_some() {
        clear();
    }
    o
}

/// True while a spoken question is waiting for its answer.
pub fn is_armed() -> bool {
    let mut g = lock();
    match g.as_ref() {
        Some(s) if s.expires > Instant::now() => s.armed,
        Some(_) => {
            *g = None;
            false
        }
        None => false,
    }
}

/// For the frontend: how long to keep the mic open, if a question is waiting.
pub fn listen_window_ms() -> Option<u64> {
    is_armed().then_some(LISTEN_WINDOW_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Yes,
    No,
    /// "Not now", "later", "remind me in a bit".
    Later,
    Other,
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|w| !w.is_empty())
        .map(String::from)
        .collect()
}

/// Strict short-reply classifier. Order matters: "not now" must not read as
/// "no", and "no thanks" must not read as yes.
pub fn classify(text: &str) -> Reply {
    let mut w = words(text);
    if w.first().map(|x| x == "nexus").unwrap_or(false) {
        w.remove(0);
    }
    if w.is_empty() || w.len() > MAX_REPLY_WORDS {
        return Reply::Other;
    }
    let joined = w.join(" ");
    let has = |needle: &str| joined.contains(needle);
    let has_word = |cands: &[&str]| w.iter().any(|x| cands.contains(&x.as_str()));

    if has("not now") || has("later") || has("remind me") || has("snooze") || has("few minutes") || has("in a bit") {
        return Reply::Later;
    }
    if has_word(&["no", "nope", "nah", "skip", "cancel", "stop"])
        || has("don't")
        || has("do not")
        || has("never mind")
        || has("not today")
        || has("leave it")
    {
        return Reply::No;
    }
    if has_word(&["yes", "yeah", "yep", "yup", "sure", "ok", "okay", "proceed", "confirm", "confirmed", "affirmative"])
        || has("go ahead")
        || has("do it")
        || has("go for it")
        || has("let's go")
        || has("let's start")
        || has("start it")
        || has("add them")
        || has("add it")
        || has("save them")
        || has("save it")
        || (w.len() <= 3 && has_word(&["start", "begin", "please"]))
    {
        return Reply::Yes;
    }
    Reply::Other
}

/// Slots read from an image but not yet confirmed. Kept for 10 minutes so
/// "add those" still works after the short reply window has closed.
const DRAFT_TTL: Duration = Duration::from_secs(10 * 60);
static DRAFT: Mutex<Option<(Vec<Slot>, String, Instant)>> = Mutex::new(None);

pub fn set_draft(slots: Vec<Slot>, label: String) {
    *DRAFT.lock().unwrap_or_else(|e| e.into_inner()) = Some((slots, label, Instant::now() + DRAFT_TTL));
}

/// Take the draft if it is still fresh.
pub fn take_draft() -> Option<(Vec<Slot>, String)> {
    let mut g = DRAFT.lock().unwrap_or_else(|e| e.into_inner());
    match g.take() {
        Some((slots, label, until)) if until > Instant::now() => Some((slots, label)),
        _ => None,
    }
}

/// Test-only serialisation: the offer is process-global state.
#[cfg(test)]
pub static TEST_GUARD: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yes_family() {
        for t in ["yes", "Yes please", "yeah", "sure", "okay", "go ahead", "do it", "start", "let's go", "Nexus yes", "yes, in the browser", "add them"] {
            assert_eq!(classify(t), Reply::Yes, "{t}");
        }
    }

    #[test]
    fn no_and_later_families_do_not_cross() {
        for t in ["no", "nope", "no thanks", "skip it", "cancel", "don't", "never mind", "not today"] {
            assert_eq!(classify(t), Reply::No, "{t}");
        }
        for t in ["not now", "later", "remind me later", "in a few minutes", "snooze"] {
            assert_eq!(classify(t), Reply::Later, "{t}");
        }
    }

    #[test]
    fn long_or_unrelated_speech_is_other() {
        for t in [
            "",
            "what is the weather like in pune tomorrow evening",
            "open chrome",
            "yes I think so but only if the weather is good tonight okay",
            "tell me a joke",
        ] {
            assert_eq!(classify(t), Reply::Other, "{t}");
        }
    }

    #[test]
    fn drafts_are_taken_once_and_expire() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let slot = super::super::timetable::make_slot("DSA", vec![0], 1080, None, None).unwrap();
        set_draft(vec![slot.clone()], "Section 2".into());
        assert_eq!(take_draft(), Some((vec![slot.clone()], "Section 2".to_string())));
        assert!(take_draft().is_none());
        *DRAFT.lock().unwrap() = Some((vec![slot], "x".into(), Instant::now() - Duration::from_secs(1)));
        assert!(take_draft().is_none(), "stale drafts are dropped");
    }

    #[test]
    fn offers_arm_expire_and_are_consumed_once() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(peek().is_none() && !is_armed() && listen_window_ms().is_none());

        set(Offer::ClearTimetable, false);
        assert!(peek().is_some());
        assert!(!is_armed(), "a reminder that has not been spoken yet must not open the mic");
        arm();
        assert!(is_armed());
        assert_eq!(listen_window_ms(), Some(LISTEN_WINDOW_MS));
        assert_eq!(take(), Some(Offer::ClearTimetable));
        assert!(take().is_none(), "consumed once");
        assert!(!is_armed());

        // A newer offer replaces an older one.
        set(Offer::ClearTimetable, true);
        set(Offer::Choose { title: "DSA".into(), activity: "dsa".into() }, true);
        assert!(matches!(peek(), Some(Offer::Choose { .. })));

        // Expiry drops it.
        *lock() = Some(State { offer: Offer::ClearTimetable, armed: true, expires: Instant::now() - Duration::from_secs(1) });
        assert!(peek().is_none() && !is_armed());
        clear();
    }
}
