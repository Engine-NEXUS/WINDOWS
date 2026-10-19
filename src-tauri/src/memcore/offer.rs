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

/// How long a spoken question stays answerable after the mic closed, by what a wrong "yes" would cost
/// (doc 124 rule 3): high-risk (shutdown, erase) 2 min, medium (calendar, timetable) 5 min, low 10 min.
pub const REPLY_WINDOW_HIGH: Duration = Duration::from_secs(2 * 60);
pub const REPLY_WINDOW_MEDIUM: Duration = Duration::from_secs(5 * 60);
pub const REPLY_WINDOW_LOW: Duration = Duration::from_secs(10 * 60);
/// How long an offer may wait to be spoken (the proactive policy can defer it
/// through a meeting) before it is dropped.
pub const SPEAK_WAIT: Duration = Duration::from_secs(15 * 60);
/// Mic window the frontend opens after the question is spoken: up to this long for the answer to START
/// (once speech has started the capture is never cut, see `openPendingOfferWindow`). People need 2-4 s
/// to think, so 8 s was easy to miss.
pub const LISTEN_WINDOW_MS: u64 = 10_000;
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
    /// "Shall I reopen everything you had open when you left?" (boot greeting)
    ReopenSession,
    /// "Shut down your laptop?" (`kind` = power_cmd::PowerKind::as_str)
    Power { kind: String },
    /// "Move these 3 PDFs to the trash, 14 MB? Say yes." The held plan lives in
    /// `action_kernel::pending`; this marker only opens the answer window.
    FileConfirm,
    /// "I couldn't find 'mummy chat' exactly - did you mean Google Chat?"
    /// A guess from the app registry: nothing launches until the user says yes.
    OpenApp { display_name: String, spoken: String },
    /// "Add a Stop hook to Claude Code's settings so I know when it stops?" (`agent` = agent_hooks::Agent::name).
    /// Nothing is written until the user says yes.
    HookSetup { agent: String },
}

impl Offer {
    /// How long this question stays answerable after it was spoken.
    pub fn reply_window(&self) -> Duration {
        match self {
            Offer::Power { .. } | Offer::ClearTimetable | Offer::FileConfirm | Offer::HookSetup { .. } => REPLY_WINDOW_HIGH,
            Offer::AddEvent { .. } | Offer::AddSlots { .. } => REPLY_WINDOW_MEDIUM,
            Offer::Start { .. } | Offer::Choose { .. } | Offer::ReopenSession | Offer::OpenApp { .. } => REPLY_WINDOW_LOW,
        }
    }
}

struct State {
    offer: Offer,
    armed: bool,
    expires: Instant,
    /// The question was already asked a second time after an unclear answer (never more than once).
    reasked: bool,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn lock() -> std::sync::MutexGuard<'static, Option<State>> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// Register an offer. `armed` = its question is being spoken right now;
/// otherwise it arms when `arm()` is called (the reminder path).
pub fn set(offer: Offer, armed: bool) {
    let ttl = if armed { offer.reply_window() } else { SPEAK_WAIT };
    *lock() = Some(State { offer, armed, expires: Instant::now() + ttl, reasked: false });
}

/// An armed question got an answer that was not one (a mumble, "dirt." for "yes"): allow ONE more try. Returns true
/// the first time per question (and gives it a fresh reply window), false afterwards or when nothing is waiting.
pub fn reask_once() -> bool {
    let mut g = lock();
    match g.as_mut() {
        Some(s) if s.armed && s.expires > Instant::now() && !s.reasked => {
            s.reasked = true;
            s.expires = Instant::now() + s.offer.reply_window();
            true
        }
        _ => false,
    }
}

/// The question was spoken: start the reply window.
pub fn arm() {
    if let Some(s) = lock().as_mut() {
        s.armed = true;
        s.expires = Instant::now() + s.offer.reply_window();
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
#[allow(dead_code)]
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

/// Every word an answer to a pending question may contain. One word outside this set (a content word
/// such as "chrome", "weather", "music") makes the sentence an ordinary command, not an answer.
const ANSWER_VOCAB: &[&str] = &[
    // yes
    "yes", "yeah", "yep", "yup", "sure", "ok", "okay", "proceed", "confirm", "confirmed", "affirmative",
    "go", "ahead", "do", "it", "for", "let's", "lets", "start", "begin", "add", "save", "them", "those", "reopen",
    "everything", "all",
    // more ways to say yes ("absolutely", "sounds good", "by all means", "that would be great", "make it so")
    "absolutely", "definitely", "certainly", "of", "course", "sounds", "good", "alright", "correct", "agreed", "thing",
    "would", "be", "great", "by", "means", "make", "so", "why", "on", "right", "that's", "thats", "i", "do", "fine",
    // no
    "no", "nope", "nah", "skip", "cancel", "stop", "don't", "dont", "never", "mind", "not", "today", "leave",
    // later
    "later", "remind", "me", "snooze", "few", "minutes", "minute", "in", "a", "bit", "now",
    // polite filler
    "please", "sir", "thanks", "thank", "you", "then", "the", "that", "this", "with", "kindly", "and",
];

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
    // The WHOLE sentence must be an answer. "okay open chrome", "sure what time is it" and "stop the
    // music" used to be read as yes / no and would have approved a pending shutdown or swallowed a
    // command (the old test looked for an answer word anywhere in up to nine words).
    if !w.iter().all(|x| ANSWER_VOCAB.contains(&x.as_str())) {
        return Reply::Other;
    }
    // "why not" is a yes whose "not" is not a negation.
    let joined = w.join(" ").replace("why not", "whynot");
    let w: Vec<String> = joined.split_whitespace().map(String::from).collect();
    let has = |needle: &str| joined.contains(needle);
    let has_word = |cands: &[&str]| w.iter().any(|x| cands.contains(&x.as_str()));

    if has("not now") || has("later") || has("remind me") || has("snooze") || has("few minutes") || has("in a bit") {
        return Reply::Later;
    }
    // Any negation wins over a yes word: "absolutely not", "not sure", "of course not", "definitely not".
    if has_word(&["no", "nope", "nah", "skip", "cancel", "stop", "not", "never"])
        || w.iter().any(|x| x.ends_with("n't"))
        || has("don't")
        || has("do not")
        || has("never mind")
        || has("not today")
        || has("leave it")
    {
        return Reply::No;
    }
    if has_word(&[
        "yes", "yeah", "yep", "yup", "sure", "ok", "okay", "proceed", "confirm", "confirmed", "affirmative",
        "absolutely", "definitely", "certainly", "alright", "agreed", "correct",
    ])
        || has("go ahead")
        || has("go on")
        || has("of course")
        || has("sounds good")
        || has("by all means")
        || has("whynot")
        || has("make it so")
        || has("please do")
        || has("do that")
        || has("that would be great")
        || has("that would be good")
        || has("that's right")
        || has("thats right")
        || has("i do")
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

/// Extra words a spoken answer to a POWER question may contain ("shut it down", "yes power off",
/// "good night"). Only used for `Offer::Power`, so "shut down" cannot approve any other question.
const POWER_VOCAB: &[&str] = &[
    "shut", "down", "off", "power", "turn", "switch", "restart", "reboot", "sleep", "lock", "sign", "out", "log",
    "me", "laptop", "computer", "pc", "system", "good", "night", "goodnight", "tonight", "put", "to", "my", "up",
];

fn power_action_phrases(kind: &str) -> &'static [&'static str] {
    match kind {
        "shutdown" => &[
            "shut down", "shutdown", "shut it down", "shut off", "power off", "power it off", "power down",
            "turn it off", "turn off", "switch it off", "switch off", "good night", "goodnight", "night night",
        ],
        "restart" => &["restart", "reboot"],
        "sleep" => &["sleep", "put it to sleep", "suspend"],
        "lock" => &["lock", "lock it"],
        "signout" => &["sign out", "sign me out", "log out", "log me out", "log off"],
        _ => &[],
    }
}

/// Answers to a power question: the generic yes/no/later words PLUS the action itself, so "shut it down",
/// "yes, power off" and "good night" all say yes to "Shut down your laptop?". A negation anywhere wins
/// ("no don't shut down"). A different action ("restart it" to a shutdown question) is not an answer.
fn classify_power(kind: &str, text: &str) -> Reply {
    let mut w = words(text);
    if w.first().map(|x| x == "nexus").unwrap_or(false) {
        w.remove(0);
    }
    if w.is_empty() || w.len() > MAX_REPLY_WORDS {
        return Reply::Other;
    }
    if !w.iter().all(|x| ANSWER_VOCAB.contains(&x.as_str()) || POWER_VOCAB.contains(&x.as_str())) {
        return Reply::Other;
    }
    let joined = w.join(" ");
    if w.iter().any(|x| matches!(x.as_str(), "no" | "nope" | "nah" | "don't" | "dont" | "not" | "never" | "cancel" | "stop" | "skip"))
        || joined.contains("do not")
    {
        return Reply::No;
    }
    if power_action_phrases(kind).iter().any(|p| joined.contains(p)) {
        return Reply::Yes;
    }
    Reply::Other
}

/// Classify an answer to the question that is waiting. Power questions also accept the action itself.
pub fn classify_for(offer: &Offer, text: &str) -> Reply {
    let base = classify(text);
    if base != Reply::Other {
        return base;
    }
    match offer {
        Offer::Power { kind } => classify_power(kind, text),
        _ => Reply::Other,
    }
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
        for t in ["yes", "Yes please", "yeah", "sure", "okay", "go ahead", "do it", "start", "let's go", "Nexus yes", "add them", "yes reopen them"] {
            assert_eq!(classify(t), Reply::Yes, "{t}");
        }
    }

    #[test]
    fn many_more_ways_to_say_yes() {
        for t in [
            "yup", "yep", "yeah sure", "absolutely", "definitely", "certainly", "of course", "sounds good", "alright",
            "agreed", "correct", "that's right", "please do", "do that", "go on", "by all means", "sure thing",
            "why not", "make it so", "that would be great", "yes sir", "yes please do", "okay do it", "sure go ahead",
            "yeah go ahead", "affirmative", "confirmed", "i do", "Nexus, absolutely", "yes, of course",
        ] {
            assert_eq!(classify(t), Reply::Yes, "{t}");
        }
        // and the refusals still win
        for t in ["no way", "absolutely not", "of course not", "not sure", "no thanks sir", "definitely not"] {
            assert_ne!(classify(t), Reply::Yes, "{t}");
        }
    }

    #[test]
    fn a_power_question_also_accepts_the_action_itself() {
        let off = Offer::Power { kind: "shutdown".into() };
        for t in [
            "shut it down", "shut down", "yes shut down", "yes, shut it down", "power off", "turn it off", "switch it off",
            "go ahead and shut down", "do it shut down", "good night", "yes good night", "shut down the laptop", "Nexus shut it down",
            "yup", "sure", "absolutely",
        ] {
            assert_eq!(classify_for(&off, t), Reply::Yes, "{t}");
        }
        for t in ["don't shut down", "no don't shut it down", "cancel", "not tonight", "stop", "never mind"] {
            assert_eq!(classify_for(&off, t), Reply::No, "{t}");
        }
        // a different action or an unrelated sentence is not an answer
        for t in ["restart it", "sleep", "shut down notepad", "open chrome", "turn off the lights", "good morning"] {
            assert_eq!(classify_for(&off, t), Reply::Other, "{t}");
        }
        // "shut down" must not approve any other question
        assert_eq!(classify_for(&Offer::ClearTimetable, "shut down"), Reply::Other);
        assert_eq!(classify_for(&Offer::ReopenSession, "good night"), Reply::Other);
        // each kind takes its own action words
        let restart = Offer::Power { kind: "restart".into() };
        assert_eq!(classify_for(&restart, "restart it"), Reply::Yes);
        assert_eq!(classify_for(&restart, "shut down"), Reply::Other);
    }

    #[test]
    fn a_sentence_that_merely_contains_an_answer_word_is_not_an_answer() {
        for t in [
            "okay open chrome",
            "ok so what is the weather",
            "sure what time is it",
            "stop the music",
            "yes open chrome",
            "no I want the other one",
            "please open notepad",
            "start spotify",
            "later tell me a joke",
        ] {
            assert_eq!(classify(t), Reply::Other, "{t}");
        }
        // "yes, in the browser" answers a Choose offer through parse_choice, not through yes/no.
        assert_eq!(classify("yes, in the browser"), Reply::Other);
        assert!(super::super::timetable::parse_choice("yes, in the browser").is_some());
    }

    #[test]
    fn an_unclear_answer_gets_exactly_one_more_try() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear();
        assert!(!reask_once(), "nothing waiting");
        set(Offer::HookSetup { agent: "antigravity".into() }, false);
        assert!(!reask_once(), "an offer whose question was not spoken yet is not re-asked");
        arm();
        assert!(reask_once(), "first unclear answer: ask again");
        assert!(!reask_once(), "second unclear answer: give up");
        assert!(peek().is_some(), "the question is still waiting after the re-ask");
        // a new question starts fresh
        set(Offer::HookSetup { agent: "opencode".into() }, true);
        assert!(reask_once());
        clear();
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
    fn the_answer_window_is_longer_the_cheaper_a_wrong_yes_is() {
        assert_eq!(Offer::Power { kind: "shutdown".into() }.reply_window(), REPLY_WINDOW_HIGH);
        assert_eq!(Offer::ClearTimetable.reply_window(), REPLY_WINDOW_HIGH);
        assert_eq!(Offer::ReopenSession.reply_window(), REPLY_WINDOW_LOW);
        assert_eq!(Offer::Start { title: "DSA".into(), activity: "dsa".into() }.reply_window(), REPLY_WINDOW_LOW);
        assert!(REPLY_WINDOW_HIGH < REPLY_WINDOW_MEDIUM && REPLY_WINDOW_MEDIUM < REPLY_WINDOW_LOW);
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
        *lock() = Some(State { offer: Offer::ClearTimetable, armed: true, expires: Instant::now() - Duration::from_secs(1), reasked: false });
        assert!(peek().is_none() && !is_armed());
        clear();
    }
}
