//! The "watch this, then power off" plan the user is holding with NEXUS (doc 142, phase W1).
//!
//! One plan at a time, kept in memory only (it dies with the process on purpose: a shutdown plan must
//! never survive a restart). The user can ask about it, cancel it, turn it into "just tell me", change
//! the power action, or relax "only if perfect". The parser is pure and strict: a sentence is only
//! claimed when every word belongs to the small vocabulary of that command, so "restart instead" or
//! "cancel the plan for the weekend" never reach this module.
//!
//! NOTHING IS ARMED YET: the observer and the unattended shutdown are later phases. Every answer says
//! so while `ARMED_SUPPORTED` is false, so NEXUS never implies it will power off by itself.

use crate::power_cmd::PowerKind;
use crate::watch_power::{read_back, WatchPlan};
use std::sync::Mutex;

/// Flipped to true by the phase that really watches a session and powers off. Until then the plan is
/// only a note and every spoken answer says so.
pub const ARMED_SUPPORTED: bool = false;

const NOT_ARMED: &str = "This is only noted for now, sir: I can't watch or power off by myself yet, so your laptop will stay on.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPlan {
    pub plan: WatchPlan,
    /// "Never mind the shutdown, just tell me": watch, report, never power off.
    pub notify_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanCommand {
    Cancel,
    Status,
    NotifyOnly,
    /// "shut down even if it fails" (the only way the clean-ending requirement is relaxed)
    EvenIfFails,
    SetKind(PowerKind),
}

impl PlanCommand {
    /// Wire form carried by `ParsedIntent::WatchPlanCommand`.
    pub fn as_str(&self) -> String {
        match self {
            PlanCommand::Cancel => "cancel".to_string(),
            PlanCommand::Status => "status".to_string(),
            PlanCommand::NotifyOnly => "notify_only".to_string(),
            PlanCommand::EvenIfFails => "even_if_fails".to_string(),
            PlanCommand::SetKind(k) => format!("kind:{}", k.as_str()),
        }
    }

    pub fn from_str(s: &str) -> Option<PlanCommand> {
        Some(match s {
            "cancel" => PlanCommand::Cancel,
            "status" => PlanCommand::Status,
            "notify_only" => PlanCommand::NotifyOnly,
            "even_if_fails" => PlanCommand::EvenIfFails,
            other => PlanCommand::SetKind(PowerKind::from_str(other.strip_prefix("kind:")?)?),
        })
    }
}

static PLAN: Mutex<Option<StoredPlan>> = Mutex::new(None);

/// Remember a new plan (replaces any older one).
pub fn set(plan: WatchPlan) {
    if let Ok(mut g) = PLAN.lock() {
        *g = Some(StoredPlan { plan, notify_only: false });
    }
}

#[cfg(test)]
pub fn current() -> Option<StoredPlan> {
    PLAN.lock().ok().and_then(|g| g.clone())
}

/// Run a command against the held plan and return what NEXUS says.
pub fn run(cmd: &PlanCommand) -> String {
    match PLAN.lock() {
        Ok(mut g) => apply(cmd, &mut g),
        Err(_) => "I couldn't reach my notes on that, sir.".to_string(),
    }
}

fn who(plan: &WatchPlan) -> String {
    plan.agent.clone().unwrap_or_else(|| "this session".to_string())
}

fn action_words(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::Shutdown => "shut down your laptop",
        PowerKind::Restart => "restart your laptop",
        PowerKind::Sleep => "put your laptop to sleep",
        PowerKind::Lock => "lock your screen",
        PowerKind::SignOut => "sign you out",
    }
}

fn describe(s: &StoredPlan) -> String {
    if s.notify_only {
        format!("I'm set to watch {} and only tell you when it finishes, with no power-off", who(&s.plan))
    } else {
        read_back(&s.plan)
    }
}

fn tail() -> String {
    if ARMED_SUPPORTED {
        String::new()
    } else {
        format!(" {NOT_ARMED}")
    }
}

/// Pure state change + the sentence to speak.
pub fn apply(cmd: &PlanCommand, slot: &mut Option<StoredPlan>) -> String {
    match (cmd, slot.as_mut()) {
        (PlanCommand::Status, None) => "I'm not holding any shutdown plan, sir.".to_string(),
        (PlanCommand::Status, Some(s)) => format!("{}.{}", describe(s), tail()),
        (PlanCommand::Cancel, None) => "There's no shutdown plan to cancel, sir. Nothing will be turned off.".to_string(),
        (PlanCommand::Cancel, Some(_)) => {
            *slot = None;
            "Done, sir. I've dropped the shutdown plan. Nothing will be turned off.".to_string()
        }
        (_, None) => "There's no shutdown plan to change, sir. Say watch over this and shut down when it's perfect to make one.".to_string(),
        (PlanCommand::NotifyOnly, Some(s)) => {
            s.notify_only = true;
            format!("Understood, sir. I'll only tell you when {} finishes, and I won't power anything off.{}", who(&s.plan), tail())
        }
        (PlanCommand::EvenIfFails, Some(s)) => {
            s.notify_only = false;
            s.plan.require_clean = false;
            format!("Understood, sir. I'll {} when {} finishes, even if it does not end cleanly.{}", action_words(s.plan.kind), who(&s.plan), tail())
        }
        (PlanCommand::SetKind(k), Some(s)) => {
            s.plan.kind = *k;
            s.notify_only = false;
            format!("Changed, sir. When {} finishes I'll {}.{}", who(&s.plan), action_words(*k), tail())
        }
    }
}

// ─── Parsing ─────────────────────────────────────────────────────────

fn norm(text: &str) -> String {
    let mut t = text
        .to_lowercase()
        .replace('\u{2019}', "'")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    loop {
        let before = t.clone();
        for p in ["hey nexus ", "nexus ", "okay ", "ok ", "so ", "please ", "can you ", "could you ", "alright ", "well "] {
            if let Some(r) = t.strip_prefix(p) {
                t = r.to_string();
            }
        }
        for sfx in [" nexus", " please", " sir", " now"] {
            if let Some(r) = t.strip_suffix(sfx) {
                t = r.to_string();
            }
        }
        if t == before {
            return t;
        }
    }
}

fn has(padded: &str, phrase: &str) -> bool {
    padded.contains(&format!(" {phrase} "))
}

fn has_any(padded: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|p| has(padded, p))
}

/// Every word of the sentence must come from `vocab`: a sentence that says anything else is not ours.
fn only_vocab(tokens: &[&str], vocab: &[&str]) -> bool {
    tokens.iter().all(|t| vocab.contains(t))
}

fn kind_in(padded: &str) -> Option<PowerKind> {
    let table: &[(&str, PowerKind)] = &[
        ("shut down", PowerKind::Shutdown),
        ("shutdown", PowerKind::Shutdown),
        ("power off", PowerKind::Shutdown),
        ("turn off", PowerKind::Shutdown),
        ("restart", PowerKind::Restart),
        ("reboot", PowerKind::Restart),
        ("sleep", PowerKind::Sleep),
        ("lock", PowerKind::Lock),
        ("sign out", PowerKind::SignOut),
        ("log out", PowerKind::SignOut),
    ];
    table.iter().find(|(p, _)| has(padded, p)).map(|(_, k)| *k)
}

const POWER_WORDS: &[&str] = &["shutdown", "shut down", "power off", "turn off", "power"];

const NOUNS_OF_THE_PLAN: &[&str] = &[
    "shutdown plan", "shut down plan", "power off plan", "power plan", "the plan", "the watch", "this watch", "the shutdown watch",
];

/// Parse a command about the held plan. `None` = not about the plan.
pub fn parse_command(text: &str) -> Option<PlanCommand> {
    let t = norm(text);
    if t.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = t.split(' ').collect();
    let p = format!(" {t} ");

    // "never mind the shutdown, just tell me (when it's done)"
    let tell_me = has_any(&p, &["just tell me", "just notify me", "just let me know", "only tell me", "only notify me", "only let me know", "just alert me", "just message me"]);
    let negates = has_any(&p, &["never mind", "nevermind", "forget", "skip", "no", "don't", "dont", "do not", "without", "instead of", "cancel", "drop"]);
    if tell_me && negates && has_any(&p, POWER_WORDS) {
        return Some(PlanCommand::NotifyOnly);
    }

    // cancel
    let cancel_verb = has_any(&p, &["cancel", "forget", "drop", "scrap", "abort", "delete", "remove", "never mind", "nevermind", "stop", "end", "clear"]);
    const CANCEL_VOCAB: &[&str] = &[
        "cancel", "forget", "drop", "scrap", "abort", "delete", "remove", "never", "mind", "nevermind", "stop", "end", "clear", "the",
        "that", "this", "shutdown", "shut", "down", "power", "off", "plan", "watch", "my", "and", "it", "can", "you",
    ];
    if cancel_verb && has_any(&p, NOUNS_OF_THE_PLAN) && only_vocab(&tokens, CANCEL_VOCAB) {
        return Some(PlanCommand::Cancel);
    }
    if has_any(
        &p,
        &[
            "don't shut down after", "do not shut down after", "dont shut down after", "don't shut down when", "do not shut down when",
            "dont shut down when", "don't power off after", "do not power off after", "don't turn off after", "do not turn off after",
        ],
    ) && (has(&p, "it") || has(&p, "it's") || has(&p, "this") || has(&p, "claude") || has(&p, "the session") || has(&p, "that"))
    {
        return Some(PlanCommand::Cancel);
    }

    // status
    if has_any(
        &p,
        &[
            "what are you watching", "what are you watching for", "are you still watching", "are you watching anything",
            "is the shutdown plan still on", "is the plan still on", "what is the shutdown plan", "what's the shutdown plan",
            "status of the shutdown plan", "shutdown plan status", "is the shutdown still planned", "are you still going to shut down",
            "is the shutdown plan still active", "what shutdown plan do you have",
        ],
    ) {
        return Some(PlanCommand::Status);
    }

    // shut down even if it fails
    const EVEN_VOCAB: &[&str] = &[
        "shut", "down", "shutdown", "power", "off", "turn", "even", "if", "it", "fails", "fail", "failed", "errors", "error", "something",
        "breaks", "goes", "wrong", "regardless", "no", "matter", "what", "whatever", "happens", "do", "go", "ahead", "and", "the", "laptop",
        "computer", "pc", "my", "still", "anyway", "plan", "either", "way", "that", "when", "claude", "finishes", "is", "done", "a", "problem", "problems",
        "there", "are",
    ];
    if (has(&p, "even if") || has(&p, "no matter what") || has(&p, "whatever happens") || has(&p, "regardless"))
        && has_any(&p, &["shut down", "shutdown", "power off", "turn off"])
        // (the word list has no "not" / "don't", so a negated sentence never gets here)
        && only_vocab(&tokens, EVEN_VOCAB)
    {
        return Some(PlanCommand::EvenIfFails);
    }

    // "make it sleep instead", "change that to restart"
    const KIND_VOCAB: &[&str] = &[
        "change", "make", "switch", "set", "that", "it", "to", "the", "plan", "shutdown", "shut", "down", "instead", "put", "sleep",
        "restart", "reboot", "lock", "sign", "out", "log", "off", "power", "my", "laptop", "computer", "pc", "of", "shutting", "rather",
        "than", "turn", "go", "i", "want", "would", "like", "actually", "no", "on", "standby", "a",
    ];
    if only_vocab(&tokens, KIND_VOCAB) {
        let verb_to = has_any(&p, &["change", "make", "switch", "set"]) && has(&p, "to");
        if has(&p, "instead") || verb_to {
            // the new kind is the one named LAST ("change shutdown to sleep" -> sleep); with "instead of" the
            // old action comes after it, so the kind named BEFORE "instead of" wins ("restart instead of shut down").
            let mut found: Vec<(usize, PowerKind)> = Vec::new();
            for (word, kind) in [
                ("shut down", PowerKind::Shutdown), ("shutdown", PowerKind::Shutdown), ("power off", PowerKind::Shutdown),
                ("restart", PowerKind::Restart), ("reboot", PowerKind::Restart), ("sleep", PowerKind::Sleep), ("lock", PowerKind::Lock),
                ("sign out", PowerKind::SignOut), ("log out", PowerKind::SignOut),
            ] {
                if let Some(i) = p.rfind(&format!(" {word} ")) {
                    found.push((i, kind));
                }
            }
            found.sort_by_key(|(i, _)| *i);
            if let Some(cut) = p.find(" instead of ") {
                found.retain(|(i, _)| *i < cut);
            }
            let kind = found.last().map(|(_, k)| *k).or_else(|| kind_in(&p));
            if let Some(k) = kind {
                return Some(PlanCommand::SetKind(k));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(kind: PowerKind) -> WatchPlan {
        WatchPlan { agent: Some("claude".into()), kind, require_clean: true, deadline_mins: None }
    }

    fn held(kind: PowerKind) -> Option<StoredPlan> {
        Some(StoredPlan { plan: plan(kind), notify_only: false })
    }

    #[test]
    fn cancel_status_and_the_others_are_recognised_in_many_words() {
        let cancel = [
            "cancel the shutdown plan", "cancel the power off plan", "forget the shutdown plan", "drop the shutdown plan",
            "scrap the shutdown plan", "never mind the shutdown plan", "nevermind the plan", "stop the watch", "cancel the watch",
            "abort the shutdown plan", "delete the shutdown plan", "remove the shutdown plan", "clear the shutdown plan",
            "nexus cancel the shutdown plan", "please cancel the shutdown plan", "cancel the plan", "end the watch",
            "don't shut down after it finishes", "do not shut down after this", "don't shut down when it's done",
            "Nexus, cancel the shutdown plan.", "cancel that shutdown plan", "can you cancel the shutdown plan",
        ];
        for t in cancel {
            assert_eq!(parse_command(t), Some(PlanCommand::Cancel), "{t}");
        }
        for t in [
            "what are you watching", "what are you watching for", "are you still watching", "is the shutdown plan still on",
            "what's the shutdown plan", "status of the shutdown plan", "is the plan still on", "are you still going to shut down",
            "nexus what are you watching?",
        ] {
            assert_eq!(parse_command(t), Some(PlanCommand::Status), "{t}");
        }
        for t in [
            "never mind the shutdown just tell me", "never mind the shutdown, just tell me when it's done", "don't shut down, just let me know",
            "skip the shutdown and just tell me", "forget the power off, only notify me", "no shutdown just tell me when it finishes",
            "cancel the shutdown and just let me know",
        ] {
            assert_eq!(parse_command(t), Some(PlanCommand::NotifyOnly), "{t}");
        }
        for t in [
            "shut down even if it fails", "shut down even if something goes wrong", "power off no matter what",
            "shut down my laptop regardless", "shut down whatever happens", "turn off even if there are errors", "shut down even if it breaks",
            "go ahead and shut down even if it fails", "nexus shut down even if it fails",
        ] {
            assert_eq!(parse_command(t), Some(PlanCommand::EvenIfFails), "{t}");
        }
        for (t, k) in [
            ("make it sleep instead", PowerKind::Sleep), ("change that to sleep", PowerKind::Sleep), ("restart instead", PowerKind::Restart),
            ("switch it to restart", PowerKind::Restart), ("lock instead", PowerKind::Lock), ("put it to sleep instead", PowerKind::Sleep),
            ("change the plan to restart", PowerKind::Restart), ("sleep instead of shutting down", PowerKind::Sleep),
            ("make it a shutdown instead", PowerKind::Shutdown), ("change that to shutdown", PowerKind::Shutdown),
            ("actually sign out instead", PowerKind::SignOut), ("nexus make it lock instead", PowerKind::Lock),
            ("restart instead of shut down", PowerKind::Restart), ("change shutdown to sleep", PowerKind::Sleep),
        ] {
            assert_eq!(parse_command(t), Some(PlanCommand::SetKind(k)), "{t}");
        }
    }

    #[test]
    fn other_sentences_are_left_alone() {
        for t in [
            "", "cancel", "stop", "cancel the meeting", "cancel the plan for the weekend", "drop the database", "stop the music",
            "what are you doing", "what is the weather", "are you listening", "restart the app instead", "restart nexus instead",
            "play music instead", "make it louder", "change that to red", "change the volume to ten", "lock the door instead",
            "shut down notepad even if it fails", "run the tests even if it fails", "don't shut down even if it fails", "do not power off even if it fails", "open chrome no matter what", "do it even if it fails",
            "don't shut down my laptop", "never mind", "just tell me", "just tell me when it's done", "tell me when claude finishes",
            "sleep", "make it sleep", "i'll sleep instead of working", "cancel the shutdown", "shut down my laptop",
        ] {
            assert_eq!(parse_command(t), None, "{t:?}");
        }
    }

    #[test]
    fn the_wire_form_round_trips() {
        for c in [
            PlanCommand::Cancel, PlanCommand::Status, PlanCommand::NotifyOnly, PlanCommand::EvenIfFails,
            PlanCommand::SetKind(PowerKind::Sleep), PlanCommand::SetKind(PowerKind::SignOut),
        ] {
            assert_eq!(PlanCommand::from_str(&c.as_str()), Some(c));
        }
        assert_eq!(PlanCommand::from_str("kind:bogus"), None);
        assert_eq!(PlanCommand::from_str("nonsense"), None);
    }

    #[test]
    fn state_changes_and_answers() {
        // nothing held
        let mut none: Option<StoredPlan> = None;
        for c in [PlanCommand::Cancel, PlanCommand::Status, PlanCommand::NotifyOnly, PlanCommand::EvenIfFails, PlanCommand::SetKind(PowerKind::Sleep)] {
            let said = apply(&c, &mut none);
            assert!(none.is_none(), "{c:?} must not create a plan");
            assert!(said.contains("no shutdown plan") || said.contains("not holding"), "{said}");
        }

        // status tells the whole plan and that nothing is armed
        let mut s = held(PowerKind::Shutdown);
        let said = apply(&PlanCommand::Status, &mut s);
        assert!(said.contains("claude") && said.contains("shut down your laptop") && said.contains("only if it looks perfect"), "{said}");
        assert!(said.contains("laptop will stay on"), "{said}");

        // each change applies and is spoken
        let said = apply(&PlanCommand::SetKind(PowerKind::Sleep), &mut s);
        assert_eq!(s.as_ref().unwrap().plan.kind, PowerKind::Sleep);
        assert!(said.contains("put your laptop to sleep"), "{said}");
        let said = apply(&PlanCommand::EvenIfFails, &mut s);
        assert!(!s.as_ref().unwrap().plan.require_clean);
        assert!(said.contains("even if it does not end cleanly"), "{said}");
        let said = apply(&PlanCommand::NotifyOnly, &mut s);
        assert!(s.as_ref().unwrap().notify_only);
        assert!(said.contains("won't power anything off"), "{said}");
        let said = apply(&PlanCommand::Status, &mut s);
        assert!(said.contains("only tell you") && !said.contains("shut down your laptop"), "{said}");
        // a new power action turns "just tell me" back off
        apply(&PlanCommand::SetKind(PowerKind::Shutdown), &mut s);
        assert!(!s.as_ref().unwrap().notify_only);

        // cancel removes it for good
        let said = apply(&PlanCommand::Cancel, &mut s);
        assert!(s.is_none());
        assert!(said.contains("Nothing will be turned off"), "{said}");
        assert!(apply(&PlanCommand::Status, &mut s).contains("not holding"));
    }

    #[test]
    fn nothing_claims_to_be_armed_while_the_observer_does_not_exist() {
        assert!(!ARMED_SUPPORTED, "flip this only together with the phase that really powers off");
        let mut s = held(PowerKind::Shutdown);
        for c in [PlanCommand::Status, PlanCommand::NotifyOnly, PlanCommand::EvenIfFails, PlanCommand::SetKind(PowerKind::Lock)] {
            assert!(apply(&c, &mut s).contains("stay on"), "{c:?}");
        }
    }

    #[test]
    fn the_whole_parser_hands_these_sentences_to_the_plan_and_nothing_else() {
        use crate::intent_parser::{parse_deterministic, ParsedIntent};
        for (t, want) in [
            ("cancel the shutdown plan", "cancel"), ("what are you watching", "status"), ("never mind the shutdown, just tell me", "notify_only"),
            ("shut down even if it fails", "even_if_fails"), ("make it sleep instead", "kind:sleep"),
        ] {
            match parse_deterministic(t).map(|r| r.intent) {
                Some(ParsedIntent::WatchPlanCommand { command }) => assert_eq!(command, want, "{t}"),
                other => panic!("{t:?} -> {other:?}"),
            }
        }
        // the sentence that MAKES a plan still makes one, and an explicit power command is still a power command
        assert!(matches!(
            parse_deterministic("watch this session until it's done and if it's perfect shut down my laptop").map(|r| r.intent),
            Some(ParsedIntent::WatchThenPower { .. })
        ));
        assert!(matches!(parse_deterministic("shut down my laptop").map(|r| r.intent), Some(ParsedIntent::PowerAction { .. })));
        assert!(!matches!(parse_deterministic("cancel the meeting").map(|r| r.intent), Some(ParsedIntent::WatchPlanCommand { .. })));
    }

    #[test]
    fn the_global_plan_is_replaced_by_a_new_one() {
        set(plan(PowerKind::Shutdown));
        assert_eq!(current().unwrap().plan.kind, PowerKind::Shutdown);
        set(WatchPlan { agent: None, kind: PowerKind::Sleep, require_clean: false, deadline_mins: Some(30) });
        let c = current().unwrap();
        assert_eq!((c.plan.kind, c.plan.agent), (PowerKind::Sleep, None));
        assert!(run(&PlanCommand::Cancel).contains("dropped"));
        assert!(current().is_none());
    }
}
