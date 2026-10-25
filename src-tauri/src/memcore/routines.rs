//! Routines you repeat (Memory v2, phase M3).
//!
//! NEXUS notices command sequences you really do again and again ("open VS Code, open Chrome, open
//! Spotify" most evenings) and offers to turn them into one phrase. It never creates one on its own.
//!
//! * **What is watched.** Only a short allow-list of safe, repeatable commands (open an app, open a
//!   site, open or start music). Messages, files, power, purchases and anything with content are never
//!   logged. Each logged command is re-checked: its canonical text must parse back to the same intent,
//!   so a saved routine can always be replayed.
//! * **What counts as a routine.** Two to four different commands, run within three minutes of each
//!   other, seen in at least three sessions on at least three different days.
//! * **How it is offered.** At most once a day, each suggestion at most twice, spoken only when the
//!   proactive-speech policy allows it. You answer "save that as study mode" (a one-to-three word name
//!   that is not already a command). Memory page shows the same suggestion and lets you name it.
//! * **How it runs.** Saying the name rewrites the utterance into its steps joined by "then", so the
//!   normal compound-task path executes it, with every confirmation gate it already has.
//! * **Control.** The Memory page lists routines and removes them; "delete the study mode routine"
//!   works by voice; the whole feature has a switch (`memcoreRoutines`).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, RwLock};

use super::corrections::tokens;
use super::store::{RoutineRow, MAX_ROUTINES};

/// The only intents that may become routine steps.
pub const ALLOWED_LABELS: &[&str] = &["open_app", "open_url", "open_music_player", "play_music"];

/// Commands further apart than this belong to different sessions.
const SESSION_GAP_SECS: i64 = 180;
const MIN_SUPPORT: usize = 3;
const MIN_DAYS: usize = 3;
const MAX_STEPS: usize = 4;
/// Look back this far when mining.
const MINE_WINDOW_SECS: i64 = 60 * 24 * 3600;
/// A suggestion waits this long for a "save that as ..." before it lapses.
const PENDING_TTL_SECS: i64 = 24 * 3600;
const OFFER_EVERY_SECS: i64 = 24 * 3600;
const MAX_SHOWS: i64 = 2;
const OFFER_META_KEY: &str = "routine_last_offer";

/// Words a routine name may not start with: they begin real commands or the phrases around routines.
const RESERVED_FIRST_WORDS: &[&str] = &[
    "open", "launch", "start", "run", "close", "quit", "play", "pause", "call", "message", "text", "send",
    "whatsapp", "search", "find", "go", "switch", "show", "read", "type", "save", "delete", "remove", "forget",
    "do", "yes", "no", "the", "my", "stop", "cancel", "nexus", "hey", "ok", "okay",
];

// ---- Which commands may be logged ----

/// The canonical text of a command that may become a routine step, or `None`. It is lowercase, free of
/// wake words and punctuation, and parses back to the same intent.
pub fn canonical_step(transcript: &str, label: &str) -> Option<String> {
    if !ALLOWED_LABELS.contains(&label) {
        return None;
    }
    let normalized = crate::intent_parser::normalize_phonetic_mishearings(transcript);
    let canon = tokens(&normalized).join(" ");
    if canon.len() < 4 || canon.len() > 80 {
        return None;
    }
    let parsed = crate::intent_parser::parse_deterministic(&canon)?;
    (crate::intent_parser::intent_to_label(&parsed.intent) == label).then_some(canon)
}

/// Hook for every recorded local turn that completed.
pub fn observe_command(dir: &Path, transcript: &str, label: &str, ts: i64) {
    if !super::flag(dir, "memcoreRoutines", true) {
        return;
    }
    if let Some(canon) = canonical_step(transcript, label) {
        super::with_store(dir, |s| s.command_log_add(ts, label, &canon));
    }
}

// ---- Mining (pure) ----

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub steps: Vec<String>,
    /// Sessions in which the sequence appeared.
    pub support: usize,
    /// Different calendar days it appeared on.
    pub days: usize,
    /// "morning", "afternoon", "evening" or "night" when most runs share one.
    pub bucket: Option<&'static str>,
}

impl Candidate {
    pub fn key(&self) -> String {
        self.steps.join(" | ")
    }
}

fn bucket_of(ts: i64, tz_offset_secs: i64) -> &'static str {
    match (ts + tz_offset_secs).rem_euclid(86_400) / 3600 {
        5..=11 => "morning",
        12..=16 => "afternoon",
        17..=21 => "evening",
        _ => "night",
    }
}

/// Repeated command sequences in `log` (time, canonical command), best first. Sequences already saved
/// as routines (`saved`) are left out.
pub fn mine(log: &[(i64, String)], saved: &[Vec<String>], tz_offset_secs: i64) -> Vec<Candidate> {
    let mut entries = log.to_vec();
    entries.sort_by_key(|e| e.0);

    // Sessions: commands each within SESSION_GAP_SECS of the one before; an immediate repeat is dropped.
    let mut sessions: Vec<Vec<(i64, String)>> = Vec::new();
    for e in entries {
        let joins_last = sessions.last().map_or(false, |s| e.0 - s.last().map_or(0, |l| l.0) <= SESSION_GAP_SECS);
        if joins_last {
            if let Some(s) = sessions.last_mut() {
                if s.last().map_or(true, |l| l.1 != e.1) {
                    s.push(e);
                }
            }
        } else {
            sessions.push(vec![e]);
        }
    }

    #[derive(Default)]
    struct Acc {
        support: usize,
        days: HashSet<i64>,
        buckets: HashMap<&'static str, usize>,
    }
    let mut acc: HashMap<Vec<String>, Acc> = HashMap::new();
    for s in sessions.iter().filter(|s| s.len() >= 2) {
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        for i in 0..s.len() {
            for len in 2..=MAX_STEPS.min(s.len() - i) {
                let seq: Vec<String> = s[i..i + len].iter().map(|e| e.1.clone()).collect();
                if seq.iter().collect::<HashSet<_>>().len() != seq.len() {
                    continue; // a command twice in one sequence is not a routine
                }
                if !seen.insert(seq.clone()) {
                    continue; // once per session
                }
                let a = acc.entry(seq).or_default();
                a.support += 1;
                a.days.insert((s[i].0 + tz_offset_secs).div_euclid(86_400));
                *a.buckets.entry(bucket_of(s[i].0, tz_offset_secs)).or_default() += 1;
            }
        }
    }

    let mut found: Vec<Candidate> = acc
        .into_iter()
        .filter(|(seq, a)| a.support >= MIN_SUPPORT && a.days.len() >= MIN_DAYS && !saved.contains(seq))
        .map(|(steps, a)| {
            let (top_bucket, top_count) = a
                .buckets
                .iter()
                .map(|(b, c)| (*b, *c))
                .max_by_key(|(b, c)| (*c, *b))
                .unwrap_or(("night", 0));
            // Only claim a time of day when most runs share it.
            let bucket = (top_count * 10 >= a.support * 6).then_some(top_bucket);
            Candidate { steps, support: a.support, days: a.days.len(), bucket }
        })
        .collect();

    // A shorter sequence that only ever appears inside a longer one adds nothing.
    let all = found.clone();
    found.retain(|c| {
        !all.iter().any(|o| {
            o.steps.len() > c.steps.len()
                && o.support == c.support
                && o.steps.windows(c.steps.len()).any(|w| w == c.steps.as_slice())
        })
    });
    found.sort_by(|a, b| {
        (b.support * b.steps.len()).cmp(&(a.support * a.steps.len())).then_with(|| a.steps.cmp(&b.steps))
    });
    found
}

fn join_and(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => (*one).to_string(),
        [rest @ .., last] => format!("{} and {}", rest.join(", "), last),
    }
}

/// "open vs code, chrome and spotify" for ["open vs code", "open chrome", "open spotify"].
pub fn describe(steps: &[String]) -> String {
    if steps.iter().all(|s| s.starts_with("open ")) {
        let names: Vec<&str> = steps.iter().map(|s| &s["open ".len()..]).collect();
        format!("open {}", join_and(&names))
    } else {
        let all: Vec<&str> = steps.iter().map(String::as_str).collect();
        join_and(&all)
    }
}

/// What NEXUS says when it offers a routine.
pub fn offer_line(c: &Candidate) -> String {
    let when = c.bucket.map(|b| format!(" most {b}s")).unwrap_or_default();
    format!(
        "I've noticed you {} together{when}, sir. Say save that as, and a name, and I'll make it one phrase.",
        describe(&c.steps)
    )
}

// ---- Names ----

/// Why a name cannot be used, or `Ok(normalized name)`.
pub fn validate_name(raw: &str, existing: &[String]) -> Result<String, String> {
    let words = tokens(raw);
    if words.is_empty() || words.len() > 3 {
        return Err("Use one to three words for the name, sir.".into());
    }
    if words.iter().any(|w| w.len() < 2 || w.len() > 15 || !w.chars().all(|c| c.is_ascii_alphabetic())) {
        return Err("A name can only use letters, sir.".into());
    }
    let name = words.join(" ");
    if name.len() < 3 {
        return Err("That name is too short, sir.".into());
    }
    if RESERVED_FIRST_WORDS.contains(&words[0].as_str()) || crate::intent_parser::parse_deterministic(&name).is_some() {
        return Err("That already means something else, sir. Try another name.".into());
    }
    if existing.iter().any(|e| *e == name) {
        return Err("You already have a routine with that name, sir.".into());
    }
    Ok(name)
}

// ---- What the user said (pure) ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Utterance {
    Save { name: String },
    Delete { name: String },
    Run { name: String },
}

const SAVE_OPENINGS: &[&[&str]] = &[
    &["save", "that", "routine", "as"],
    &["save", "that", "as"],
    &["save", "this", "as"],
    &["save", "it", "as"],
    &["make", "that", "a", "routine", "called"],
    &["call", "that", "routine"],
    &["name", "that", "routine"],
];

/// Interpret an utterance against the known routine names. Save is recognised only when `suggestion` is
/// waiting, Delete and Run only for names that exist, so ordinary talk is never captured.
pub fn parse_utterance(text: &str, names: &[String], suggestion_waiting: bool) -> Option<Utterance> {
    let words = tokens(text);
    if words.is_empty() || words.len() > 9 {
        return None;
    }
    if suggestion_waiting {
        for opening in SAVE_OPENINGS {
            if words.len() > opening.len() && opening.iter().zip(&words).all(|(o, w)| *o == w.as_str()) {
                return Some(Utterance::Save { name: words[opening.len()..].join(" ") });
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    // delete / remove / forget [the|my] <name> [routine]
    if matches!(words[0].as_str(), "delete" | "remove" | "forget") {
        let mut rest = &words[1..];
        if matches!(rest.first().map(String::as_str), Some("the" | "my")) {
            rest = &rest[1..];
        }
        if rest.first().map(String::as_str) == Some("routine") {
            rest = &rest[1..];
        }
        if rest.last().map(String::as_str) == Some("routine") {
            rest = &rest[..rest.len() - 1];
        }
        let name = rest.join(" ");
        return names.contains(&name).then_some(Utterance::Delete { name });
    }
    // [run|start|do|activate] [my|the] <name> [routine]
    let mut rest = &words[..];
    if matches!(rest.first().map(String::as_str), Some("run" | "start" | "do" | "activate")) {
        rest = &rest[1..];
    }
    if matches!(rest.first().map(String::as_str), Some("my" | "the")) {
        rest = &rest[1..];
    }
    if rest.last().map(String::as_str) == Some("routine") {
        rest = &rest[..rest.len() - 1];
    }
    let name = rest.join(" ");
    names.contains(&name).then_some(Utterance::Run { name })
}

// ---- Process-wide state (read while parsing; no I/O on the hot path) ----

static NAMES: RwLock<Vec<String>> = RwLock::new(Vec::new());
/// The suggestion currently waiting for a name, and when it lapses.
static PENDING: Mutex<Option<(Vec<String>, i64)>> = Mutex::new(None);

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn pending_steps(now: i64) -> Option<Vec<String>> {
    let g = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    g.as_ref().filter(|(_, until)| *until > now).map(|(steps, _)| steps.clone())
}

fn set_pending(steps: Vec<String>, now: i64) {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = Some((steps, now + PENDING_TTL_SECS));
}

fn clear_pending() {
    *PENDING.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub fn refresh(dir: &Path) {
    if let Some(rows) = super::with_store(dir, |s| s.routines_all()) {
        if let Ok(mut g) = NAMES.write() {
            *g = rows.into_iter().map(|r| r.name).collect();
        }
    }
}

pub fn init(dir: &Path) {
    refresh(dir);
}

// ---- Saving, deleting, running ----

/// Save the waiting suggestion under `raw_name`. `Err` is a sentence to say to the user.
pub fn save_pending(dir: &Path, raw_name: &str) -> Result<RoutineRow, String> {
    let now = now_ts();
    let steps = pending_steps(now).ok_or_else(|| "There is nothing waiting to be saved, sir.".to_string())?;
    let existing: Vec<String> = super::with_store(dir, |s| s.routines_all().into_iter().map(|r| r.name).collect())
        .unwrap_or_default();
    let name = validate_name(raw_name, &existing)?;
    super::with_store(dir, |s| s.routine_add(&name, &steps, now))
        .ok_or_else(|| "I can't reach my memory right now, sir.".to_string())?
        .map_err(|e| format!("I couldn't save it: {e}, sir."))?;
    clear_pending();
    refresh(dir);
    Ok(RoutineRow { name, steps, created: now, last_run: 0, runs: 0 })
}

pub fn delete(dir: &Path, name: &str) -> bool {
    let gone = super::with_store(dir, |s| s.routine_delete(name, now_ts())).unwrap_or(false);
    refresh(dir);
    gone
}

/// What the orchestrator should do with an utterance.
pub enum Handled {
    /// Say this and stop.
    Say(String),
    /// Continue as if the user had said this (the routine's steps joined by "then").
    Run(String),
}

pub fn interpret_and_apply(dir: &Path, text: &str) -> Option<Handled> {
    let names = NAMES.read().map(|g| g.clone()).unwrap_or_default();
    let now = now_ts();
    match parse_utterance(text, &names, pending_steps(now).is_some())? {
        Utterance::Save { name } => Some(Handled::Say(match save_pending(dir, &name) {
            Ok(r) => format!("Done, sir. Say {} and I'll do it all.", r.name),
            Err(why) => why,
        })),
        Utterance::Delete { name } => {
            delete(dir, &name).then(|| Handled::Say(format!("Removed the {name} routine, sir.")))
        }
        Utterance::Run { name } => {
            let row = super::with_store(dir, |s| {
                s.routine_record_run(&name, now);
                s.routines_all().into_iter().find(|r| r.name == name)
            })??;
            Some(Handled::Run(row.steps.join(" then ")))
        }
    }
}

// ---- Suggesting ----

/// The best routine not yet offered twice, if one may be offered now (once a day at most).
pub fn suggest(dir: &Path, now: i64, tz_offset_secs: i64) -> Option<Candidate> {
    super::with_store(dir, |s| {
        let last: i64 = s.meta_get(OFFER_META_KEY).and_then(|v| v.parse().ok()).unwrap_or(0);
        if now - last < OFFER_EVERY_SECS {
            return None;
        }
        let log = s.command_log_since(now - MINE_WINDOW_SECS);
        let saved: Vec<Vec<String>> = s.routines_all().into_iter().map(|r| r.steps).collect();
        let pick = mine(&log, &saved, tz_offset_secs).into_iter().find(|c| s.offer_shown(&c.key()) < MAX_SHOWS)?;
        s.offer_mark_shown(&pick.key(), now);
        s.meta_set(OFFER_META_KEY, &now.to_string());
        Some(pick)
    })?
}

/// Look for a routine to offer soon after start-up and every few hours after.
pub fn start_suggester<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(150)).await;
        loop {
            if let Ok(dir) = app.path().app_data_dir() {
                if super::enabled(&dir) && super::flag(&dir, "memcoreRoutines", true) {
                    let tz = chrono::Local::now().offset().local_minus_utc() as i64;
                    let now = now_ts();
                    if let Some(c) = suggest(&dir, now, tz) {
                        println!("[ROUTINE] suggesting: {}", c.key());
                        set_pending(c.steps.clone(), now);
                        crate::proactive_policy::submit(
                            &app,
                            format!("routine-offer-{now}"),
                            offer_line(&c),
                            crate::google::types::AlertUrgency::Medium,
                        );
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(3 * 3600)).await;
        }
    });
}

// ---- Memory page ----

#[derive(Debug, Clone, serde::Serialize)]
pub struct Suggestion {
    pub steps: Vec<String>,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct View {
    pub routines: Vec<RoutineRow>,
    pub suggestion: Option<Suggestion>,
    pub limit: usize,
}

pub fn view(dir: &Path) -> View {
    let routines = super::with_store(dir, |s| s.routines_all()).unwrap_or_default();
    let suggestion = pending_steps(now_ts()).map(|steps| Suggestion { text: describe(&steps), steps });
    View { routines, suggestion, limit: MAX_ROUTINES }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    /// An evening (18:00 UTC); the tz offset is 0 in all tests.
    const T0: i64 = 1_799_776_800;

    fn session(day: i64, hour_offset_secs: i64, cmds: &[&str]) -> Vec<(i64, String)> {
        cmds.iter()
            .enumerate()
            .map(|(i, c)| (T0 + day * DAY + hour_offset_secs + (i as i64) * 20, (*c).to_string()))
            .collect()
    }

    fn log(parts: Vec<Vec<(i64, String)>>) -> Vec<(i64, String)> {
        parts.into_iter().flatten().collect()
    }

    fn steps(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn three_evenings_make_a_suggestion() {
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome", "open spotify"]),
            session(1, 0, &["open vs code", "open chrome", "open spotify"]),
            session(2, 600, &["open vs code", "open chrome", "open spotify"]),
        ]);
        let found = mine(&l, &[], 0);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].steps, steps(&["open vs code", "open chrome", "open spotify"]));
        assert_eq!((found[0].support, found[0].days, found[0].bucket), (3, 3, Some("evening")));
    }

    #[test]
    fn one_busy_day_is_not_a_habit() {
        let mut parts = vec![];
        for k in 0..6 {
            parts.push(session(0, k * 1_000, &["open vs code", "open chrome"]));
        }
        assert!(mine(&log(parts), &[], 0).is_empty(), "six times on one day");
    }

    #[test]
    fn two_days_are_not_enough() {
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome"]),
            session(1, 0, &["open vs code", "open chrome"]),
        ]);
        assert!(mine(&l, &[], 0).is_empty());
    }

    #[test]
    fn commands_minutes_apart_are_separate_sessions() {
        let mut l = vec![];
        for day in 0..4 {
            l.push((T0 + day * DAY, "open vs code".to_string()));
            l.push((T0 + day * DAY + 900, "open chrome".to_string())); // 15 minutes later
        }
        assert!(mine(&l, &[], 0).is_empty());
    }

    #[test]
    fn the_longest_sequence_wins_over_its_own_pieces() {
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome", "open spotify"]),
            session(1, 0, &["open vs code", "open chrome", "open spotify"]),
            session(2, 0, &["open vs code", "open chrome", "open spotify"]),
        ]);
        let found = mine(&l, &[], 0);
        assert_eq!(found.len(), 1, "sub-sequences with the same support are folded in: {found:?}");
        assert_eq!(found[0].steps.len(), 3);
    }

    #[test]
    fn a_more_common_shorter_sequence_is_kept_beside_the_longer_one() {
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome", "open spotify"]),
            session(1, 0, &["open vs code", "open chrome", "open spotify"]),
            session(2, 0, &["open vs code", "open chrome", "open spotify"]),
            session(3, 0, &["open vs code", "open chrome"]),
            session(4, 0, &["open vs code", "open chrome"]),
        ]);
        let found = mine(&l, &[], 0);
        let two = steps(&["open vs code", "open chrome"]);
        assert!(found.iter().any(|c| c.steps == two && c.support == 5), "{found:?}");
        assert!(found.iter().any(|c| c.steps.len() == 3 && c.support == 3), "{found:?}");
    }

    #[test]
    fn a_repeated_command_inside_a_sequence_is_not_a_routine() {
        let l = log(vec![
            session(0, 0, &["open chrome", "open notepad", "open chrome"]),
            session(1, 0, &["open chrome", "open notepad", "open chrome"]),
            session(2, 0, &["open chrome", "open notepad", "open chrome"]),
        ]);
        let found = mine(&l, &[], 0);
        assert!(found.iter().all(|c| c.steps.iter().collect::<HashSet<_>>().len() == c.steps.len()), "{found:?}");
    }

    #[test]
    fn an_already_saved_routine_is_not_suggested_again() {
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome"]),
            session(1, 0, &["open vs code", "open chrome"]),
            session(2, 0, &["open vs code", "open chrome"]),
        ]);
        let saved = vec![steps(&["open vs code", "open chrome"])];
        assert!(mine(&l, &saved, 0).is_empty());
    }

    #[test]
    fn describing_a_routine_reads_naturally() {
        assert_eq!(describe(&steps(&["open vs code", "open chrome", "open spotify"])), "open vs code, chrome and spotify");
        assert_eq!(describe(&steps(&["open chrome", "play lo fi beats"])), "open chrome and play lo fi beats");
        let c = Candidate { steps: steps(&["open vs code", "open chrome"]), support: 3, days: 3, bucket: Some("evening") };
        assert!(offer_line(&c).contains("open vs code and chrome together most evenings"));
    }

    #[test]
    fn only_safe_repeatable_commands_become_steps() {
        assert_eq!(canonical_step("Nexus, open VS Code.", "open_app"), Some("open vs code".into()));
        // message content, files, power and purchases are never logged
        assert_eq!(canonical_step("send mom a whatsapp message saying hi", "send_whatsapp_message"), None);
        assert_eq!(canonical_step("shut down my laptop", "power_action"), None);
        assert_eq!(canonical_step("delete the pdfs in downloads", "file_action"), None);
        // a transcript that no longer parses to the claimed intent is not logged either
        assert_eq!(canonical_step("what is the weather today", "open_app"), None);
    }

    #[test]
    fn names_must_be_short_plain_and_not_already_commands() {
        let none: Vec<String> = vec![];
        assert_eq!(validate_name("Study Mode", &none), Ok("study mode".into()));
        assert!(validate_name("", &none).is_err());
        assert!(validate_name("one two three four", &none).is_err());
        assert!(validate_name("mode 2", &none).is_err(), "digits");
        assert!(validate_name("open chrome", &none).is_err(), "a real command");
        assert!(validate_name("play something nice", &none).is_err(), "starts like a command");
        assert!(validate_name("study mode", &["study mode".to_string()]).is_err(), "taken");
    }

    #[test]
    fn utterances_are_recognised_only_when_they_mean_a_routine() {
        let names = vec!["study mode".to_string()];
        assert_eq!(
            parse_utterance("Nexus, save that as study mode", &names, true),
            Some(Utterance::Save { name: "study mode".into() })
        );
        // with no suggestion waiting, "save that as ..." belongs to someone else
        assert_eq!(parse_utterance("save that as report", &[], false), None);
        for run in ["study mode", "run study mode", "start my study mode routine", "do the study mode"] {
            assert_eq!(parse_utterance(run, &names, false), Some(Utterance::Run { name: "study mode".into() }), "{run}");
        }
        assert_eq!(
            parse_utterance("delete the study mode routine", &names, false),
            Some(Utterance::Delete { name: "study mode".into() })
        );
        // ordinary talk and unknown names are left alone
        assert_eq!(parse_utterance("what is the weather", &names, false), None);
        assert_eq!(parse_utterance("run exam mode", &names, false), None);
        assert_eq!(parse_utterance("delete the exam routine", &names, false), None);
    }

    fn d(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_routines_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn store_round_trip_save_run_delete_and_wipe() {
        let dir = d("store");
        let now = 1_800_000_000;
        let s = steps(&["open vs code", "open chrome"]);
        super::super::with_store(&dir, |st| {
            assert!(st.routine_add("study mode", &s, now).is_ok());
            assert!(st.routine_add("study mode", &s, now).is_err(), "duplicate name refused");
            st.routine_record_run("study mode", now + 5);
            let all = st.routines_all();
            assert_eq!((all.len(), all[0].runs, all[0].last_run), (1, 1, now + 5));
            assert_eq!(all[0].steps, s);
            assert!(st.routine_delete("study mode", now));
            assert!(!st.routine_delete("study mode", now));
            st.routine_add("again", &s, now).unwrap();
            st.wipe("test", now);
            assert!(st.routines_all().is_empty());
        })
        .unwrap();
    }

    #[test]
    fn the_routine_limit_is_enforced() {
        let dir = d("limit");
        let s = steps(&["open vs code", "open chrome"]);
        super::super::with_store(&dir, |st| {
            for i in 0..MAX_ROUTINES {
                st.routine_add(&format!("routine{i}"), &s, 1).unwrap();
            }
            assert!(st.routine_add("one too many", &s, 1).is_err());
        })
        .unwrap();
    }

    #[test]
    fn a_suggestion_is_offered_once_a_day_and_at_most_twice_in_total() {
        let dir = d("offers");
        let l = log(vec![
            session(0, 0, &["open vs code", "open chrome"]),
            session(1, 0, &["open vs code", "open chrome"]),
            session(2, 0, &["open vs code", "open chrome"]),
        ]);
        super::super::with_store(&dir, |st| {
            for (ts, c) in &l {
                st.command_log_add(*ts, "open_app", c);
            }
        })
        .unwrap();
        let now = T0 + 3 * DAY;
        let first = suggest(&dir, now, 0).expect("a routine to offer");
        assert_eq!(first.steps, steps(&["open vs code", "open chrome"]));
        assert!(suggest(&dir, now + 3_600, 0).is_none(), "not twice in one day");
        assert!(suggest(&dir, now + DAY + 10, 0).is_some(), "second showing the next day");
        assert!(suggest(&dir, now + 3 * DAY, 0).is_none(), "never a third time");
    }

    #[test]
    fn saving_needs_a_waiting_suggestion_and_a_valid_name() {
        let dir = d("save");
        clear_pending();
        assert!(save_pending(&dir, "study mode").is_err(), "nothing waiting");
        set_pending(steps(&["open vs code", "open chrome"]), now_ts());
        assert!(save_pending(&dir, "open chrome").is_err(), "name that is a command");
        let row = save_pending(&dir, "study mode").unwrap();
        assert_eq!(row.name, "study mode");
        assert!(pending_steps(now_ts()).is_none(), "consumed");
        let all = super::super::with_store(&dir, |s| s.routines_all()).unwrap();
        assert_eq!(all.len(), 1);

        // The entry point the orchestrator calls (kept in this one test: it reads the process-wide name list).
        match interpret_and_apply(&dir, "Nexus, run my study mode routine") {
            Some(Handled::Run(text)) => assert_eq!(text, "open vs code then open chrome"),
            _ => panic!("the routine should run as its steps joined by 'then'"),
        }
        assert_eq!(super::super::with_store(&dir, |s| s.routines_all()[0].runs).unwrap(), 1, "the run is counted");
        assert!(interpret_and_apply(&dir, "save that as something").is_none(), "nothing is waiting any more");
        assert!(interpret_and_apply(&dir, "what is the weather").is_none(), "ordinary talk is untouched");
        match interpret_and_apply(&dir, "delete the study mode routine") {
            Some(Handled::Say(line)) => assert!(line.contains("Removed"), "{line}"),
            _ => panic!("deleting by voice should answer"),
        }
        assert!(interpret_and_apply(&dir, "study mode").is_none(), "gone");
        assert!(super::super::with_store(&dir, |s| s.routines_all()).unwrap().is_empty());
    }

    #[test]
    fn a_routine_containing_a_gated_step_still_goes_through_the_confirmation_gate() {
        // A routine runs as "step then step then step" through the normal compound path, so a step that
        // needs confirmation is held there exactly as if the user had typed the compound command.
        let text = steps(&["open chrome", "send mom a whatsapp message saying hi"]).join(" then ");
        let plan = crate::command_center::build_plan(&text, "t1").expect("a two-step plan");
        assert_eq!(plan.steps.len(), 2);
        let gated = &plan.steps[1];
        assert!(
            matches!(gated.subsystem, crate::orchestrator::Subsystem::Mcp),
            "the WhatsApp step must be routed to the confirmation-gated MCP path, got {:?}",
            gated.subsystem
        );
    }
}
