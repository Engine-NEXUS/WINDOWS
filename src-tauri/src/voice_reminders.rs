//! Spoken reminders and timers: "remind me to call mom at 5 pm", "remind me in 45 seconds to check the oven",
//! "set a timer for 10 minutes for the tea", "cancel the timer", "what reminders do I have".
//!
//! This is a producer of the universal reminder (`crate::reminder`): it decides WHEN something fires and WHAT is said, and
//! hands the event to the shared module, which draws the roll, the countdown chip and the alert. No animation code here.
//! Items are kept in `voice_reminders.json` so they survive a restart; a one-second check fires what is due through
//! `proactive_policy` (so meeting mode, an open microphone and a question waiting are respected). One that came due while
//! NEXUS was off is still said, marked as late.
//!
//! The words are built by pure functions (`parse`, `parse_duration`, `resolve_when`, `*_phrase`, `pick_to_cancel`) so they
//! are tested without a window, a clock or a network.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use crate::google::types::AlertUrgency;
use crate::orchestrator::{install_new_request, say_and_finish, ProcessResult, Subsystem};
use crate::reminder::{Display, ReminderEvent, MAX_TITLE_CHARS};

/// The longest a spoken reminder or timer may be set ahead.
pub const MAX_AHEAD_SECS: u32 = 7 * 24 * 3600;
/// Timers are short by nature.
pub const MAX_TIMER_SECS: u32 = 24 * 3600;
/// A reminder this long past its time is announced as late.
const LATE_AFTER_MS: i64 = 2 * 60 * 1000;
const MAX_ITEMS: usize = 24;

// ── the command ─────────────────────────────────────────────────────────────

/// What was asked. Time is carried as spoken (relative seconds, or a clock time) and turned into an instant by `resolve_when`
/// at run time, because "5 pm" means different instants depending on when it is said.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cmd {
    /// `in_secs` or `at_min` set; neither set means the time was not said and NEXUS asks.
    Remind {
        task: String,
        in_secs: Option<u32>,
        at_min: Option<u16>,
        /// A bare "at 5": 5 am or 5 pm, whichever comes next.
        ambiguous: bool,
        tomorrow: bool,
    },
    /// `seconds == 0` means the length was not said and NEXUS asks.
    Timer { seconds: u32, label: String },
    /// `kind` is "any", "timer" or "reminder"; `text` narrows to one by its words.
    Cancel { kind: String, all: bool, text: String },
    List { timers_only: bool },
}

// ── parsing words ───────────────────────────────────────────────────────────

fn clean(text: &str) -> String {
    let t = text.trim().to_lowercase();
    let mut s = t.trim_end_matches(['.', ',', '!', '?']).trim().to_string();
    loop {
        let before = s.len();
        for lead in ["please ", "nexus, ", "nexus ", "hey nexus ", "can you ", "could you ", "can u ", "would you "] {
            if let Some(r) = s.strip_prefix(lead) {
                s = r.trim_start().to_string();
            }
        }
        if s.len() == before {
            break;
        }
    }
    for tail in [" please", " for me"] {
        if let Some(r) = s.strip_suffix(tail) {
            s = r.trim_end().to_string();
        }
    }
    s
}

fn number_word(w: &str) -> Option<f64> {
    Some(match w {
        "a" | "an" | "one" => 1.0,
        "two" => 2.0,
        "three" => 3.0,
        "four" => 4.0,
        "five" => 5.0,
        "six" => 6.0,
        "seven" => 7.0,
        "eight" => 8.0,
        "nine" => 9.0,
        "ten" => 10.0,
        "eleven" => 11.0,
        "twelve" => 12.0,
        "thirteen" => 13.0,
        "fourteen" => 14.0,
        "fifteen" => 15.0,
        "sixteen" => 16.0,
        "seventeen" => 17.0,
        "eighteen" => 18.0,
        "nineteen" => 19.0,
        "twenty" => 20.0,
        "thirty" => 30.0,
        "forty" => 40.0,
        "fifty" => 50.0,
        "sixty" => 60.0,
        "ninety" => 90.0,
        _ => return None,
    })
}

/// A number at the start of `w`: digits ("10", "1.5"), a word ("ten"), or "twenty five". Returns the value and words used.
fn number_at(w: &[&str]) -> Option<(f64, usize)> {
    let first = *w.first()?;
    if first.chars().next()?.is_ascii_digit() {
        return first.parse::<f64>().ok().map(|v| (v, 1));
    }
    let v = number_word(first)?;
    if matches!(first, "twenty" | "thirty" | "forty" | "fifty") {
        if let Some(u) = w.get(1).and_then(|n| number_word(n)) {
            if (1.0..=9.0).contains(&u) && *w.get(1)? != "a" && *w.get(1)? != "an" {
                return Some((v + u, 2));
            }
        }
    }
    Some((v, 1))
}

fn unit_secs(w: &str) -> Option<f64> {
    Some(match w {
        "second" | "seconds" | "sec" | "secs" => 1.0,
        "minute" | "minutes" | "min" | "mins" => 60.0,
        "hour" | "hours" | "hr" | "hrs" => 3600.0,
        _ => return None,
    })
}

/// A spoken length at the start of `w`: "10 minutes", "an hour and a half", "half an hour", "1 hour 30 minutes", "90 seconds".
/// Returns whole seconds and the words used. Pure.
pub fn parse_duration(w: &[&str]) -> Option<(u32, usize)> {
    let mut i = 0;
    let mut total = 0.0f64;
    let mut last_unit = 0.0f64;
    let mut any = false;
    loop {
        if any && w.get(i) == Some(&"and") {
            // "and" only joins if something duration-like follows
            let rest = &w[i + 1..];
            let follows = rest.first().is_some_and(|x| *x == "a" && rest.get(1) == Some(&"half"))
                || number_at(rest).is_some_and(|(_, n)| rest.get(n).is_some_and(|u| unit_secs(u).is_some()))
                || (rest.first() == Some(&"half"));
            if !follows {
                break;
            }
            i += 1;
        }
        // "half an hour"
        if w.get(i) == Some(&"half") && matches!(w.get(i + 1), Some(&"an") | Some(&"a")) {
            if let Some(u) = w.get(i + 2).and_then(|x| unit_secs(x)) {
                total += 0.5 * u;
                last_unit = u;
                i += 3;
                any = true;
                continue;
            }
        }
        // "... and a half" after a unit
        if any && w.get(i) == Some(&"a") && w.get(i + 1) == Some(&"half") && last_unit > 0.0 {
            total += 0.5 * last_unit;
            i += 2;
            last_unit = 0.0;
            continue;
        }
        if let Some((n, used)) = number_at(&w[i.min(w.len())..]) {
            if let Some(u) = w.get(i + used).and_then(|x| unit_secs(x)) {
                total += n * u;
                last_unit = u;
                i += used + 1;
                any = true;
                continue;
            }
        }
        break;
    }
    if !any || total < 1.0 || total > u32::MAX as f64 {
        return None;
    }
    Some((total.round() as u32, i))
}

/// A clock time at the start of `w`: "5 pm", "5:30", "17:30", "noon", "9 o'clock". Returns minutes since midnight, whether
/// the hour is ambiguous (a bare 1 to 12), and the words used. Pure.
fn parse_clock(w: &[&str]) -> Option<(u16, bool, usize)> {
    let first = *w.first()?;
    match first {
        "noon" | "midday" => return Some((12 * 60, false, 1)),
        "midnight" => return Some((0, false, 1)),
        _ => {}
    }
    let digit = first.chars().next()?.is_ascii_digit();
    if !digit {
        return None;
    }
    let meridiem = |s: &str| matches!(s, "am" | "pm" | "a.m." | "p.m." | "a.m" | "p.m");
    if let Some(m) = w.get(1).filter(|s| meridiem(s)) {
        let min = crate::memcore::timetable::parse_time(&format!("{first}{}", m.replace('.', "")))?;
        return Some((min, false, 2));
    }
    if first.len() > 5 {
        return None;
    }
    let has_suffix = first.ends_with("am") || first.ends_with("pm");
    let min = crate::memcore::timetable::parse_time(first)?;
    let four_digit = first.len() == 4 && first.chars().all(|c| c.is_ascii_digit());
    let h = min / 60;
    let ambiguous = !has_suffix && !four_digit && (1..=12).contains(&h) && !first.starts_with('0');
    let mut used = 1;
    if matches!(w.get(1), Some(&"o'clock") | Some(&"oclock")) {
        used = 2;
    }
    Some((min, ambiguous, used))
}

/// "in the evening" / "at night" after a clock time makes a bare hour pm; "in the morning" makes it am. Returns the new
/// minutes and the words used. Pure.
fn day_part(w: &[&str], min: u16, ambiguous: bool) -> (u16, bool, usize) {
    if !ambiguous {
        return (min, ambiguous, 0);
    }
    let pm = |m: u16| if m / 60 == 12 { m } else { m + 12 * 60 };
    let am = |m: u16| if m / 60 == 12 { m - 12 * 60 } else { m };
    match (w.first(), w.get(1), w.get(2)) {
        (Some(&"in"), Some(&"the"), Some(&"evening")) | (Some(&"in"), Some(&"the"), Some(&"afternoon")) => (pm(min), false, 3),
        (Some(&"in"), Some(&"the"), Some(&"morning")) => (am(min), false, 3),
        (Some(&"at"), Some(&"night"), _) | (Some(&"tonight"), _, _) => (pm(min), false, if w[0] == "at" { 2 } else { 1 }),
        _ => (min, ambiguous, 0),
    }
}

/// The time part found by `split_when`.
#[derive(Debug, Clone, PartialEq, Default)]
struct When {
    in_secs: Option<u32>,
    at_min: Option<u16>,
    ambiguous: bool,
    tomorrow: bool,
}

fn day_word(w: &str) -> Option<bool> {
    // true = tomorrow, false = today / tonight
    match w {
        "tomorrow" => Some(true),
        "today" | "tonight" => Some(false),
        _ => None,
    }
}

/// Take a time off the END of "call mom at 5 pm" / "call mom tomorrow at 9" / "check the oven in 45 seconds".
/// Returns the text before it and the time. Pure.
fn split_when_tail(rest: &str) -> (String, When) {
    let words: Vec<&str> = rest.split_whitespace().collect();
    for i in (0..words.len()).rev() {
        if words[i] == "in" {
            if let Some((secs, used)) = parse_duration(&words[i + 1..]) {
                if i + 1 + used == words.len() {
                    return (words[..i].join(" "), When { in_secs: Some(secs), ..When::default() });
                }
            }
        }
        if words[i] == "at" {
            if let Some((min, amb, used)) = parse_clock(&words[i + 1..]) {
                let (min, amb, extra) = day_part(&words[i + 1 + used..], min, amb);
                if i + 1 + used + extra == words.len() {
                    let mut head = words[..i].to_vec();
                    let mut tomorrow = false;
                    let mut tonight = false;
                    if let Some(last) = head.last() {
                        if let Some(t) = day_word(last) {
                            tomorrow = t;
                            tonight = *last == "tonight";
                            head.pop();
                        }
                    }
                    let (min, amb) = if tonight && amb { (if min / 60 == 12 { min } else { min + 12 * 60 }, false) } else { (min, amb) };
                    return (head.join(" "), When { at_min: Some(min), ambiguous: amb, tomorrow, ..When::default() });
                }
            }
        }
    }
    // "... tomorrow" with no clock: no time said
    (rest.trim().to_string(), When::default())
}

/// The time part at the START of "at 5 pm to call mom" / "tomorrow at 9 to ..." / "in 10 minutes to ...": the time and the
/// text after "to". Pure.
fn split_when_head(rest: &str) -> Option<(When, String)> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    let mut tomorrow = false;
    let mut tonight = false;
    if let Some(t) = words.first().and_then(|w| day_word(w)) {
        tomorrow = t;
        tonight = words[0] == "tonight";
        i = 1;
    }
    let when = match words.get(i)? {
        &"in" => {
            let (secs, used) = parse_duration(&words[i + 1..])?;
            i += 1 + used;
            When { in_secs: Some(secs), ..When::default() }
        }
        &"at" | &"for" | &"by" => {
            let (min, amb, used) = parse_clock(&words[i + 1..])?;
            let (min, amb, extra) = day_part(&words[i + 1 + used..], min, amb);
            i += 1 + used + extra;
            let (min, amb) = if tonight && amb { (if min / 60 == 12 { min } else { min + 12 * 60 }, false) } else { (min, amb) };
            When { at_min: Some(min), ambiguous: amb, tomorrow, ..When::default() }
        }
        _ => return None,
    };
    if words.get(i) != Some(&"to") {
        return None;
    }
    let task = words[i + 1..].join(" ");
    (!task.trim().is_empty()).then_some((when, task))
}

fn tidy_task(task: &str) -> Option<String> {
    let t = task.trim().trim_matches(|c: char| c == ',' || c == '.').trim();
    let t = t.strip_prefix("that ").unwrap_or(t).trim();
    (!t.is_empty()).then(|| t.to_string())
}

fn parse_remind(s: &str) -> Option<Cmd> {
    const WITH_TO: &[&str] = &["remind me to ", "set a reminder to ", "set reminder to ", "create a reminder to ", "add a reminder to ", "make a reminder to "];
    const NO_TO: &[&str] = &["remind me ", "set a reminder ", "set reminder ", "create a reminder ", "add a reminder ", "make a reminder "];
    for head in WITH_TO {
        if let Some(rest) = s.strip_prefix(head) {
            let (task, when) = split_when_tail(rest);
            let task = tidy_task(&task)?;
            return Some(Cmd::Remind { task, in_secs: when.in_secs, at_min: when.at_min, ambiguous: when.ambiguous, tomorrow: when.tomorrow });
        }
    }
    for head in NO_TO {
        if let Some(rest) = s.strip_prefix(head) {
            let (when, task) = split_when_head(rest)?;
            let task = tidy_task(&task)?;
            return Some(Cmd::Remind { task, in_secs: when.in_secs, at_min: when.at_min, ambiguous: when.ambiguous, tomorrow: when.tomorrow });
        }
    }
    None
}

/// What follows a timer length: "for the tea", "called pasta", "to check the oven". Pure.
fn timer_label(rest: &[&str]) -> String {
    let mut r = rest;
    if matches!(r.first(), Some(&"for") | Some(&"called") | Some(&"named") | Some(&"to") | Some(&"labelled") | Some(&"labeled")) {
        r = &r[1..];
    } else {
        return String::new();
    }
    while matches!(r.first(), Some(&"the") | Some(&"my") | Some(&"a") | Some(&"an")) {
        r = &r[1..];
    }
    let label = r.join(" ");
    label.chars().take(40).collect::<String>().trim().to_string()
}

fn parse_timer(s: &str) -> Option<Cmd> {
    let words: Vec<&str> = s.split_whitespace().collect();
    if !words.iter().any(|w| matches!(*w, "timer" | "stopwatch")) {
        return None;
    }
    // "set a timer for 10 minutes [for the tea]"
    for head in [
        "set a timer for ", "set timer for ", "start a timer for ", "start timer for ", "timer for ", "set up a timer for ", "create a timer for ",
        "put a timer for ", "put on a timer for ", "set a timer of ", "make a timer for ", "run a timer for ", "set another timer for ",
    ] {
        if let Some(rest) = s.strip_prefix(head) {
            let w: Vec<&str> = rest.split_whitespace().collect();
            let (secs, used) = parse_duration(&w)?;
            return Some(Cmd::Timer { seconds: secs, label: timer_label(&w[used..]) });
        }
    }
    // "set a 10 minute timer [for the tea]"
    for head in ["set a ", "set an ", "start a ", "start an ", "create a ", "make a ", "set ", "start "] {
        if let Some(rest) = s.strip_prefix(head) {
            let w: Vec<&str> = rest.split_whitespace().collect();
            // the duration words run up to "timer"
            let Some(at) = w.iter().position(|x| *x == "timer") else { continue };
            if let Some((secs, used)) = parse_duration(&w[..at]) {
                if used == at {
                    return Some(Cmd::Timer { seconds: secs, label: timer_label(&w[at + 1..]) });
                }
            }
        }
    }
    // "set a timer" / "start a timer": the length is missing
    if matches!(s, "set a timer" | "set timer" | "start a timer" | "start timer" | "set up a timer" | "create a timer" | "make a timer" | "i need a timer" | "i want a timer") {
        return Some(Cmd::Timer { seconds: 0, label: String::new() });
    }
    None
}

fn parse_cancel(s: &str) -> Option<Cmd> {
    let mut rest = None;
    for head in ["cancel ", "delete ", "remove ", "clear ", "stop ", "turn off ", "forget ", "dismiss "] {
        if let Some(r) = s.strip_prefix(head) {
            rest = Some(r);
            break;
        }
    }
    let rest = rest?;
    let mut w: Vec<&str> = rest.split_whitespace().collect();
    let all = w.iter().any(|x| matches!(*x, "all" | "every" | "everything")) || w.last().is_some_and(|x| *x == "reminders" || *x == "timers") && !w.iter().any(|x| matches!(*x, "the" | "my" | "that" | "this" | "last"));
    w.retain(|x| !matches!(*x, "the" | "my" | "that" | "this" | "last" | "all" | "of" | "every" | "everything" | "current" | "a" | "an"));
    let kind = if w.iter().any(|x| matches!(*x, "timer" | "timers")) {
        "timer"
    } else if w.iter().any(|x| matches!(*x, "reminder" | "reminders")) {
        "reminder"
    } else {
        return None;
    };
    // "reminder to call mom", "reminder about the exam", "call mom reminder"
    let at = w.iter().position(|x| matches!(*x, "reminder" | "reminders" | "timer" | "timers"))?;
    let mut text_words: Vec<&str> = Vec::new();
    if let Some(after) = w.get(at + 1..) {
        let after: Vec<&str> = after.iter().copied().filter(|x| !matches!(*x, "to" | "about" | "for" | "named" | "called")).collect();
        text_words.extend(after);
    }
    text_words.extend(w[..at].iter().copied());
    Some(Cmd::Cancel { kind: kind.to_string(), all, text: text_words.join(" ") })
}

fn parse_list(s: &str) -> Option<Cmd> {
    const LISTS: &[&str] = &[
        "what reminders do i have", "what reminders have i set", "what are my reminders", "what's my reminders", "what is my reminders",
        "list my reminders", "list reminders", "list all reminders", "show my reminders", "show me my reminders", "read my reminders",
        "read out my reminders", "do i have any reminders", "any reminders", "any reminders set", "check my reminders", "my reminders",
        "what reminders are set", "what reminders are there", "tell me my reminders", "what timers do i have", "what timers are running",
        "list my timers", "show my timers", "do i have any timers", "any timers running", "any timers",
    ];
    if LISTS.contains(&s) {
        return Some(Cmd::List { timers_only: s.contains("timer") });
    }
    for q in [
        "how long is left on the timer", "how long is left on my timer", "how much time is left on the timer", "how much time is left on my timer",
        "how much time is left", "how long is left", "how long left on the timer", "how long on the timer", "how long is the timer",
        "how long until the timer", "how much time is left on the timer", "check the timer", "check my timer", "time left on the timer",
        "how much longer on the timer", "how long does the timer have", "what's left on the timer", "what is left on the timer",
    ] {
        if s == q {
            return Some(Cmd::List { timers_only: true });
        }
    }
    None
}

/// Whether this sentence is a spoken reminder, timer, cancel or list request. Pure.
pub fn parse(text: &str) -> Option<Cmd> {
    let s = clean(text);
    if s.is_empty() {
        return None;
    }
    parse_timer(&s).or_else(|| parse_remind(&s)).or_else(|| parse_cancel(&s)).or_else(|| parse_list(&s))
}

// ── time ────────────────────────────────────────────────────────────────────

/// When a reminder fires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Resolved {
    pub epoch_ms: i64,
    /// The clock time falls on another calendar day than `now`.
    pub next_day: bool,
}

/// Turn the spoken time into an instant. A bare hour takes whichever of am or pm comes next; a time that has passed today
/// means tomorrow. Pure.
pub fn resolve_when(
    now: chrono::DateTime<chrono::Local>,
    in_secs: Option<u32>,
    at_min: Option<u16>,
    ambiguous: bool,
    tomorrow: bool,
) -> Result<Resolved, &'static str> {
    use chrono::{Datelike, TimeZone};
    let now_ms = now.timestamp_millis();
    if let Some(secs) = in_secs {
        if secs == 0 {
            return Err("that is no time at all");
        }
        if secs > MAX_AHEAD_SECS {
            return Err("that is more than a week away");
        }
        return Ok(Resolved { epoch_ms: now_ms + secs as i64 * 1000, next_day: false });
    }
    let min = at_min.ok_or("no time was said")?;
    let at_on = |day_offset: i64, m: u16| -> Option<i64> {
        let date = now.date_naive() + chrono::Duration::days(day_offset);
        let (h, mm) = ((m / 60) as u32, (m % 60) as u32);
        chrono::Local.with_ymd_and_hms(date.year(), date.month(), date.day(), h, mm, 0).earliest().map(|d| d.timestamp_millis())
    };
    // a bare hour: both readings, earliest first
    let readings: Vec<u16> = if ambiguous {
        let base = min % 720;
        vec![base, base + 720]
    } else {
        vec![min]
    };
    if tomorrow {
        // "tomorrow at 5" is almost always the afternoon; 7 to 11 is the morning
        let pick = if ambiguous {
            let h = readings[0] / 60;
            if (1..=6).contains(&h) { readings[1] } else { min }
        } else {
            min
        };
        let ms = at_on(1, pick).ok_or("that time does not exist tomorrow")?;
        return Ok(Resolved { epoch_ms: ms, next_day: true });
    }
    let mut today: Vec<i64> = readings.iter().filter_map(|m| at_on(0, *m)).filter(|ms| *ms > now_ms).collect();
    today.sort_unstable();
    if let Some(ms) = today.first() {
        return Ok(Resolved { epoch_ms: *ms, next_day: false });
    }
    let mut next: Vec<i64> = readings.iter().filter_map(|m| at_on(1, *m)).collect();
    next.sort_unstable();
    next.first().map(|ms| Resolved { epoch_ms: *ms, next_day: true }).ok_or("that time does not exist")
}

// ── words NEXUS says ────────────────────────────────────────────────────────

/// "45 seconds" / "10 minutes" / "1 hour 30 minutes" / "2 hours". Pure.
pub fn duration_phrase(secs: u32) -> String {
    let secs = secs.max(1);
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    let plural = |n: u32, w: &str| format!("{n} {w}{}", if n == 1 { "" } else { "s" });
    if h > 0 {
        let mut out = plural(h, "hour");
        if m > 0 {
            out.push(' ');
            out.push_str(&plural(m, "minute"));
        }
        out
    } else if m > 0 {
        let mut out = plural(m, "minute");
        if s > 0 && m < 5 {
            out.push(' ');
            out.push_str(&plural(s, "second"));
        }
        out
    } else {
        plural(s, "second")
    }
}

/// "5:00 PM" for an epoch in milliseconds. Pure.
pub fn clock_of_ms(ms: i64) -> String {
    use chrono::{TimeZone, Timelike};
    match chrono::Local.timestamp_millis_opt(ms).single() {
        Some(d) => crate::memcore::timetable::fmt_time((d.hour() * 60 + d.minute()) as u16),
        None => String::new(),
    }
}

/// "call my mom" -> "call your mom": the task is said back to the person who set it. Whole words only. Pure.
pub fn you_form(task: &str) -> String {
    task.split_whitespace()
        .map(|w| {
            let (core, tail) = match w.find(|c: char| !c.is_alphanumeric() && c != '\'') {
                Some(i) => (&w[..i], &w[i..]),
                None => (w, ""),
            };
            let swapped = match core {
                "my" => "your",
                "me" => "you",
                "myself" => "yourself",
                "mine" => "yours",
                "i'm" => "you're",
                "i'll" => "you'll",
                "i've" => "you've",
                other => other,
            };
            format!("{swapped}{tail}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The line NEXUS says when a reminder is set. Pure.
pub fn reminder_set_line(task: &str, in_secs: Option<u32>, r: Resolved) -> String {
    let when = match in_secs {
        Some(s) => format!("in {}", duration_phrase(s)),
        None if r.next_day => format!("tomorrow at {}", clock_of_ms(r.epoch_ms)),
        None => format!("at {}", clock_of_ms(r.epoch_ms)),
    };
    format!("Okay, sir. I'll remind you to {} {when}.", you_form(task))
}

/// The line NEXUS says when a timer is set. Pure.
pub fn timer_set_line(secs: u32, label: &str) -> String {
    if label.is_empty() {
        format!("Timer set for {}, sir.", duration_phrase(secs))
    } else {
        format!("Timer set for {} for the {label}, sir.", duration_phrase(secs))
    }
}

fn unique_id(prefix: &str, now_ms: i64) -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    format!("{prefix}_{now_ms}_{}", N.fetch_add(1, Ordering::Relaxed) % 1000)
}

/// The reminder as the generic event the stage draws. Pure.
pub fn reminder_event(task: &str, at_ms: i64, spoken: &str, now_ms: i64) -> ReminderEvent {
    let title: String = crate::directions::cap_first(task).chars().take(MAX_TITLE_CHARS).collect();
    ReminderEvent {
        id: unique_id("voice", now_ms),
        kind: "reminder".to_string(),
        title,
        at_ms,
        spoken: spoken.chars().take(240).collect(),
        alert: format!("Sir, a reminder: {}.", you_form(task)).chars().take(240).collect(),
        source: "voice".to_string(),
        display: Display::default(),
    }
}

/// The timer as the generic event the stage draws. Pure.
pub fn timer_event(secs: u32, label: &str, spoken: &str, now_ms: i64) -> ReminderEvent {
    let title: String = if label.is_empty() { duration_phrase(secs) } else { crate::directions::cap_first(label) }.chars().take(MAX_TITLE_CHARS).collect();
    let alert = if label.is_empty() { "Sir, your timer is done.".to_string() } else { format!("Sir, your {label} timer is done.") };
    ReminderEvent {
        id: unique_id("timer", now_ms),
        kind: "timer".to_string(),
        title,
        at_ms: now_ms + secs as i64 * 1000,
        spoken: spoken.chars().take(240).collect(),
        alert: alert.chars().take(240).collect(),
        source: "timer".to_string(),
        display: Display::default(),
    }
}

/// What is said when one fires; a reminder that came due while NEXUS was off says so. Pure.
pub fn fire_line(ev: &ReminderEvent, now_ms: i64) -> String {
    if now_ms - ev.at_ms > LATE_AFTER_MS {
        format!("Sorry sir, this one is late. {}", ev.alert.strip_prefix("Sir, ").map(crate::directions::cap_first).unwrap_or_else(|| ev.alert.clone()))
    } else {
        ev.alert.clone()
    }
}

/// "3 minutes 20 seconds" left, or "now". Pure.
fn left_phrase(ms: i64) -> String {
    if ms <= 0 {
        "now".to_string()
    } else {
        duration_phrase(((ms + 999) / 1000) as u32)
    }
}

/// The spoken list of what is waiting, soonest first, at most three by name. Pure.
pub fn list_line(events: &[ReminderEvent], now_ms: i64, timers_only: bool) -> String {
    let mut v: Vec<&ReminderEvent> = events.iter().filter(|e| !timers_only || e.kind == "timer").collect();
    v.sort_by_key(|e| e.at_ms);
    let noun = if timers_only { "timer" } else { "reminder" };
    if v.is_empty() {
        return format!("You have no {noun}s set, sir.");
    }
    let item = |e: &ReminderEvent| {
        if e.kind == "timer" {
            let name = if e.title.chars().next().is_some_and(|c| c.is_ascii_digit()) { "The timer".to_string() } else { format!("The {} timer", e.title.to_lowercase()) };
            format!("{name} has {} left", left_phrase(e.at_ms - now_ms))
        } else {
            format!("{} at {}", e.title, clock_of_ms(e.at_ms))
        }
    };
    let head = if v.len() == 1 { format!("You have one {noun}, sir.") } else { format!("You have {} {noun}s, sir.", v.len()) };
    let shown: Vec<String> = v.iter().take(3).map(|e| item(e)).collect();
    let more = if v.len() > 3 { format!(" And {} more.", v.len() - 3) } else { String::new() };
    format!("{head} {}.{more}", shown.join(". "))
}

// ── cancelling ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pick {
    Nothing,
    One(String),
    Several(Vec<String>),
    Ask(Vec<String>),
}

/// Which items a cancel request means. Only voice reminders and timers can be cancelled this way. Pure.
pub fn pick_to_cancel(items: &[ReminderEvent], kind: &str, all: bool, text: &str) -> Pick {
    let words: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
    let mut hits: Vec<&ReminderEvent> = items
        .iter()
        .filter(|e| (e.source == "voice" || e.source == "timer") && (kind == "any" || e.kind == kind))
        .filter(|e| {
            let hay = e.title.to_lowercase();
            words.iter().all(|w| hay.contains(w.as_str()))
        })
        .collect();
    hits.sort_by_key(|e| e.at_ms);
    match (hits.len(), all) {
        (0, _) => Pick::Nothing,
        (1, _) => Pick::One(hits[0].id.clone()),
        (_, true) => Pick::Several(hits.iter().map(|e| e.id.clone()).collect()),
        _ => Pick::Ask(hits.iter().take(3).map(|e| e.title.clone()).collect()),
    }
}

// ── state, files, firing ────────────────────────────────────────────────────

static ITEMS: Mutex<Vec<ReminderEvent>> = Mutex::new(Vec::new());

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn items_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("voice_reminders.json"))
}

pub fn load_items(path: &std::path::Path) -> Vec<ReminderEvent> {
    std::fs::read_to_string(path).ok().and_then(|c| serde_json::from_str(&c).ok()).unwrap_or_default()
}

pub fn save_items(path: &std::path::Path, items: &[ReminderEvent]) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(items).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, body).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Remove and return what is due. Pure apart from the list it is given.
pub fn take_due(items: &mut Vec<ReminderEvent>, now: i64) -> Vec<ReminderEvent> {
    let (due, rest): (Vec<_>, Vec<_>) = std::mem::take(items).into_iter().partition(|e| e.at_ms <= now);
    *items = rest;
    due
}

fn persist<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let path = items_path(app).ok_or("no data folder")?;
    let g = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
    save_items(&path, &g)
}

fn store<R: Runtime>(app: &AppHandle<R>, ev: ReminderEvent) -> Result<(), String> {
    {
        let mut g = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() >= MAX_ITEMS {
            return Err("full".to_string());
        }
        g.push(ev.clone());
    }
    if let Err(e) = persist(app) {
        // Never promise a reminder that was not saved.
        ITEMS.lock().unwrap_or_else(|e| e.into_inner()).retain(|x| x.id != ev.id);
        return Err(e);
    }
    Ok(())
}

/// Do what was asked and return the line to say. The only function with side effects besides the ticker.
fn handle<R: Runtime>(app: &AppHandle<R>, cmd: Cmd) -> String {
    let now = chrono::Local::now();
    let now_ms = now.timestamp_millis();
    match cmd {
        Cmd::Remind { task, in_secs, at_min, ambiguous, tomorrow } => {
            if in_secs.is_none() && at_min.is_none() {
                return format!("When should I remind you, sir? Say: remind me to {task} at 5 pm, or in 20 minutes.");
            }
            let r = match resolve_when(now, in_secs, at_min, ambiguous, tomorrow) {
                Ok(r) => r,
                Err("that is more than a week away") => return "I can only set reminders up to a week ahead, sir.".to_string(),
                Err(_) => return "I couldn't work out that time, sir. Nothing is set.".to_string(),
            };
            let spoken = reminder_set_line(&task, in_secs, r);
            let ev = reminder_event(&task, r.epoch_ms, &spoken, now_ms);
            if let Err(e) = store(app, ev.clone()) {
                tracing::warn!("voice_reminders: could not save: {e}");
                return if e == "full" { "You have too many reminders waiting, sir. Cancel one first.".to_string() } else { "I couldn't save that reminder, sir. Nothing is set.".to_string() };
            }
            if let Err(e) = crate::reminder::announce(app, ev) {
                tracing::warn!("voice_reminders: saved but could not be shown: {e}");
            }
            spoken
        }
        Cmd::Timer { seconds, label } => {
            if seconds == 0 {
                return "For how long, sir? Say: set a timer for 10 minutes.".to_string();
            }
            if seconds > MAX_TIMER_SECS {
                return "I can set timers up to 24 hours, sir. For longer, ask me to remind you.".to_string();
            }
            let spoken = timer_set_line(seconds, &label);
            let ev = timer_event(seconds, &label, &spoken, now_ms);
            if let Err(e) = store(app, ev.clone()) {
                tracing::warn!("voice_reminders: could not save: {e}");
                return if e == "full" { "You have too many timers and reminders waiting, sir. Cancel one first.".to_string() } else { "I couldn't save that timer, sir. Nothing is set.".to_string() };
            }
            if let Err(e) = crate::reminder::announce(app, ev) {
                tracing::warn!("voice_reminders: saved but could not be shown: {e}");
            }
            spoken
        }
        Cmd::Cancel { kind, all, text } => {
            let items = ITEMS.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let ids = match pick_to_cancel(&items, &kind, all, &text) {
                Pick::Nothing => {
                    let noun = if kind == "timer" { "timer" } else { "reminder" };
                    return format!("There's no {noun} to cancel, sir.");
                }
                Pick::One(id) => vec![id],
                Pick::Several(ids) => ids,
                Pick::Ask(titles) => {
                    return format!("Which one, sir? You have: {}. Say cancel all, or name it.", titles.join(", "));
                }
            };
            let n = ids.len();
            ITEMS.lock().unwrap_or_else(|e| e.into_inner()).retain(|e| !ids.contains(&e.id));
            for id in &ids {
                crate::reminder::cancel(app, id);
            }
            if persist(app).is_err() {
                tracing::warn!("voice_reminders: cancelled but could not rewrite the file");
            }
            let noun = if kind == "timer" { "timer" } else { "reminder" };
            if n == 1 { format!("Cancelled the {noun}, sir.") } else { format!("Cancelled {n} {noun}s, sir.") }
        }
        Cmd::List { timers_only } => {
            let events = crate::reminder::reminder_list();
            list_line(&events, now_ms, timers_only)
        }
    }
}

/// The spoken entry point: install a request, do the thing, say the result.
pub async fn run<R: Runtime>(app: AppHandle<R>, cmd: Cmd) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let line = handle(&app, cmd);
    say_and_finish(&app, request_id, line)
}

/// Bring saved items back after a start (chips, no animation) and fire what is due. One cheap lock a second.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        if let Some(path) = items_path(&app) {
            let loaded = load_items(&path);
            for ev in &loaded {
                let _ = crate::reminder::restore(&app, ev.clone());
            }
            *ITEMS.lock().unwrap_or_else(|e| e.into_inner()) = loaded;
        }
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let now = now_ms();
            let due = {
                let mut g = ITEMS.lock().unwrap_or_else(|e| e.into_inner());
                if g.iter().all(|e| e.at_ms > now) {
                    continue;
                }
                take_due(&mut g, now)
            };
            if let Err(e) = persist(&app) {
                tracing::warn!("voice_reminders: could not rewrite the file: {e}");
            }
            for ev in due {
                let line = fire_line(&ev, now);
                crate::reminder::fired(&app, &ev.id, Some(&line));
                crate::proactive_policy::submit(&app, format!("reminder_{}", ev.id), line, AlertUrgency::High);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> chrono::DateTime<chrono::Local> {
        chrono::Local.with_ymd_and_hms(2026, 11, 14, h, m, 0).unwrap()
    }

    fn remind(task: &str, in_secs: Option<u32>, at_min: Option<u16>, ambiguous: bool, tomorrow: bool) -> Option<Cmd> {
        Some(Cmd::Remind { task: task.into(), in_secs, at_min, ambiguous, tomorrow })
    }

    #[test]
    fn lengths_are_understood_in_the_ways_people_say_them() {
        let d = |s: &str| {
            let w: Vec<&str> = s.split_whitespace().collect();
            parse_duration(&w)
        };
        assert_eq!(d("10 minutes"), Some((600, 2)));
        assert_eq!(d("ten minutes"), Some((600, 2)));
        assert_eq!(d("90 seconds"), Some((90, 2)));
        assert_eq!(d("a minute"), Some((60, 2)));
        assert_eq!(d("an hour"), Some((3600, 2)));
        assert_eq!(d("half an hour"), Some((1800, 3)));
        assert_eq!(d("an hour and a half"), Some((5400, 5)));
        assert_eq!(d("1 hour 30 minutes"), Some((5400, 4)));
        assert_eq!(d("2 hours and 15 minutes"), Some((8100, 5)));
        assert_eq!(d("twenty five minutes"), Some((1500, 3)));
        assert_eq!(d("1.5 hours"), Some((5400, 2)));
        assert_eq!(d("5 mins for the tea"), Some((300, 2)), "stops where the length ends");
        assert_eq!(d("10"), None);
        assert_eq!(d("soon"), None);
        assert_eq!(d("0 minutes"), None);
    }

    #[test]
    fn a_reminder_at_a_clock_time() {
        assert_eq!(parse("remind me to call mom at 5 pm"), remind("call mom", None, Some(17 * 60), false, false));
        assert_eq!(parse("Remind me to call my mom at 5:30 PM."), remind("call my mom", None, Some(17 * 60 + 30), false, false));
        assert_eq!(parse("nexus remind me to take the tablet at 9 am"), remind("take the tablet", None, Some(9 * 60), false, false));
        assert_eq!(parse("remind me to call the dentist at 5"), remind("call the dentist", None, Some(5 * 60), true, false));
        assert_eq!(parse("remind me to submit the form at 17:30"), remind("submit the form", None, Some(17 * 60 + 30), false, false));
        assert_eq!(parse("remind me to pay rent at noon"), remind("pay rent", None, Some(12 * 60), false, false));
        assert_eq!(parse("set a reminder to water the plants at 7 in the evening"), remind("water the plants", None, Some(19 * 60), false, false));
        assert_eq!(parse("remind me to wake the kids at 7 in the morning"), remind("wake the kids", None, Some(7 * 60), false, false));
        assert_eq!(parse("remind me to take out the bins tomorrow at 8 am"), remind("take out the bins", None, Some(8 * 60), false, true));
        assert_eq!(parse("remind me to stretch tonight at 9"), remind("stretch", None, Some(21 * 60), false, false));
    }

    #[test]
    fn a_reminder_after_a_wait() {
        assert_eq!(parse("remind me in 45 seconds to check the oven"), remind("check the oven", Some(45), None, false, false));
        assert_eq!(parse("remind me to check the oven in 45 seconds"), remind("check the oven", Some(45), None, false, false));
        assert_eq!(parse("remind me in 2 hours and 15 minutes to leave"), remind("leave", Some(8100), None, false, false));
        assert_eq!(parse("remind me in an hour and a half to stand up"), remind("stand up", Some(5400), None, false, false));
        assert_eq!(parse("set a reminder to submit the assignment in half an hour"), remind("submit the assignment", Some(1800), None, false, false));
    }

    #[test]
    fn the_time_may_come_first() {
        assert_eq!(parse("remind me at 5 pm to call mom"), remind("call mom", None, Some(17 * 60), false, false));
        assert_eq!(parse("remind me tomorrow at 9 am to send the report"), remind("send the report", None, Some(9 * 60), false, true));
        assert_eq!(parse("set a reminder for 6:30 pm to start dinner"), remind("start dinner", None, Some(18 * 60 + 30), false, false));
    }

    #[test]
    fn a_reminder_without_a_time_is_still_a_reminder_so_nexus_can_ask() {
        assert_eq!(parse("remind me to call mom"), remind("call mom", None, None, false, false));
        assert_eq!(parse("remind me to call mom tomorrow"), remind("call mom tomorrow", None, None, false, false));
    }

    #[test]
    fn a_task_that_mentions_at_or_in_is_not_cut_wrongly() {
        assert_eq!(parse("remind me to look at the report at 3 pm"), remind("look at the report", None, Some(15 * 60), false, false));
        assert_eq!(parse("remind me to meet him in the lobby at 4 pm"), remind("meet him in the lobby", None, Some(16 * 60), false, false));
        assert_eq!(parse("remind me to buy 2 eggs"), remind("buy 2 eggs", None, None, false, false));
        assert_eq!(parse("remind me to buy milk in the shop"), remind("buy milk in the shop", None, None, false, false));
    }

    #[test]
    fn timers() {
        let t = |secs: u32, label: &str| Some(Cmd::Timer { seconds: secs, label: label.into() });
        assert_eq!(parse("set a timer for 10 minutes"), t(600, ""));
        assert_eq!(parse("start a timer for 90 seconds"), t(90, ""));
        assert_eq!(parse("timer for 5 minutes"), t(300, ""));
        assert_eq!(parse("set a 10 minute timer"), t(600, ""));
        assert_eq!(parse("set a 25 minute timer for the pomodoro"), t(1500, "pomodoro"));
        assert_eq!(parse("set a timer for 12 minutes for the tea"), t(720, "tea"));
        assert_eq!(parse("set a timer for an hour and a half called laundry"), t(5400, "laundry"));
        assert_eq!(parse("can you set a timer for half an hour"), t(1800, ""));
        assert_eq!(parse("set a timer"), t(0, ""));
    }

    #[test]
    fn cancelling() {
        let c = |kind: &str, all: bool, text: &str| Some(Cmd::Cancel { kind: kind.into(), all, text: text.into() });
        assert_eq!(parse("cancel the timer"), c("timer", false, ""));
        assert_eq!(parse("cancel my reminder"), c("reminder", false, ""));
        assert_eq!(parse("cancel all reminders"), c("reminder", true, ""));
        assert_eq!(parse("cancel all timers"), c("timer", true, ""));
        assert_eq!(parse("cancel reminders"), c("reminder", true, ""));
        assert_eq!(parse("cancel the reminder to call mom"), c("reminder", false, "call mom"));
        assert_eq!(parse("delete the call mom reminder"), c("reminder", false, "call mom"));
        assert_eq!(parse("stop the timer"), c("timer", false, ""));
        assert_eq!(parse("turn off the pasta timer"), c("timer", false, "pasta"));
    }

    #[test]
    fn listing() {
        assert_eq!(parse("what reminders do I have"), Some(Cmd::List { timers_only: false }));
        assert_eq!(parse("Do I have any reminders?"), Some(Cmd::List { timers_only: false }));
        assert_eq!(parse("how much time is left on the timer"), Some(Cmd::List { timers_only: true }));
        assert_eq!(parse("what timers do I have"), Some(Cmd::List { timers_only: true }));
    }

    #[test]
    fn ordinary_sentences_are_left_alone() {
        for p in [
            "remind",
            "reminder",
            "open the timer app",
            "the timer is broken",
            "stop",
            "cancel",
            "cancel the order",
            "cancel my subscription",
            "stop the music",
            "delete the file",
            "what time is it",
            "set a alarm for 7",
            "I need to remember to call mom",
            "remind me",
            "what is a reminder",
            "how long is the flight",
            "list my files",
        ] {
            assert_eq!(parse(p), None, "'{p}' was wrongly taken");
        }
    }

    #[test]
    fn a_wait_becomes_an_instant() {
        let now = at(10, 0);
        let r = resolve_when(now, Some(90), None, false, false).unwrap();
        assert_eq!((r.epoch_ms - now.timestamp_millis(), r.next_day), (90_000, false));
        assert!(resolve_when(now, Some(0), None, false, false).is_err());
        assert!(resolve_when(now, Some(MAX_AHEAD_SECS + 1), None, false, false).is_err());
        assert!(resolve_when(now, None, None, false, false).is_err());
    }

    #[test]
    fn a_clock_time_is_the_next_one_that_comes() {
        let min = |h: u32, m: u32| (h * 60 + m) as u16;
        let ms = |r: Resolved| chrono::Local.timestamp_millis_opt(r.epoch_ms).unwrap();
        // 5 pm said at 10 am is today
        let r = resolve_when(at(10, 0), None, Some(min(17, 0)), false, false).unwrap();
        assert_eq!((ms(r).format("%H:%M").to_string(), r.next_day), ("17:00".to_string(), false));
        // 9 am said at 11 pm is tomorrow
        let r = resolve_when(at(23, 0), None, Some(min(9, 0)), false, false).unwrap();
        assert_eq!((ms(r).format("%d %H:%M").to_string(), r.next_day), ("15 09:00".to_string(), true));
        // a bare 5 said at 3 pm is 5 pm; said at 6 pm it is 5 am tomorrow; said at 4 am it is 5 am
        let r = resolve_when(at(15, 0), None, Some(min(5, 0)), true, false).unwrap();
        assert_eq!(ms(r).format("%H:%M").to_string(), "17:00");
        let r = resolve_when(at(18, 0), None, Some(min(5, 0)), true, false).unwrap();
        assert_eq!((ms(r).format("%d %H:%M").to_string(), r.next_day), ("15 05:00".to_string(), true));
        let r = resolve_when(at(4, 0), None, Some(min(5, 0)), true, false).unwrap();
        assert_eq!((ms(r).format("%H:%M").to_string(), r.next_day), ("05:00".to_string(), false));
        // a bare 12 is noon or midnight, whichever is next
        let r = resolve_when(at(9, 0), None, Some(min(12, 0)), true, false).unwrap();
        assert_eq!(ms(r).format("%H:%M").to_string(), "12:00");
        // the exact minute is already past
        let r = resolve_when(at(17, 0), None, Some(min(17, 0)), false, false).unwrap();
        assert!(r.next_day);
        // "tomorrow at 5" means the afternoon, "tomorrow at 9" the morning
        let r = resolve_when(at(10, 0), None, Some(min(5, 0)), true, true).unwrap();
        assert_eq!((ms(r).format("%d %H:%M").to_string(), r.next_day), ("15 17:00".to_string(), true));
        let r = resolve_when(at(10, 0), None, Some(min(9, 0)), true, true).unwrap();
        assert_eq!(ms(r).format("%d %H:%M").to_string(), "15 09:00");
    }

    #[test]
    fn what_nexus_says() {
        assert_eq!(duration_phrase(45), "45 seconds");
        assert_eq!(duration_phrase(1), "1 second");
        assert_eq!(duration_phrase(600), "10 minutes");
        assert_eq!(duration_phrase(150), "2 minutes 30 seconds");
        assert_eq!(duration_phrase(1500), "25 minutes");
        assert_eq!(duration_phrase(5400), "1 hour 30 minutes");
        assert_eq!(duration_phrase(7200), "2 hours");
        assert_eq!(you_form("call my mom"), "call your mom");
        assert_eq!(you_form("send it to me, please"), "send it to you, please");
        assert_eq!(you_form("tell her I'm late"), "tell her I'm late", "only whole words that mean the speaker change");
        assert_eq!(you_form("water my plants and myself"), "water your plants and yourself");
        assert_eq!(timer_set_line(600, ""), "Timer set for 10 minutes, sir.");
        assert_eq!(timer_set_line(720, "tea"), "Timer set for 12 minutes for the tea, sir.");
        let rel = reminder_set_line("call my mom", Some(2700), Resolved { epoch_ms: 0, next_day: false });
        assert_eq!(rel, "Okay, sir. I'll remind you to call your mom in 45 minutes.");
    }

    #[test]
    fn the_events_are_valid_and_labelled() {
        let now = 1_800_000_000_000i64;
        let r = reminder_event("call my mom", now + 60_000, "said", now);
        assert_eq!((r.kind.as_str(), r.title.as_str(), r.source.as_str()), ("reminder", "Call my mom", "voice"));
        assert_eq!(r.alert, "Sir, a reminder: call your mom.");
        assert_eq!(crate::reminder::validate(&r, now, true), Ok(()));
        let t = timer_event(600, "", "said", now);
        assert_eq!((t.kind.as_str(), t.title.as_str(), t.at_ms), ("timer", "10 minutes", now + 600_000));
        assert_eq!(t.alert, "Sir, your timer is done.");
        assert_eq!(crate::reminder::validate(&t, now, true), Ok(()));
        assert_eq!(timer_event(300, "tea", "s", now).alert, "Sir, your tea timer is done.");
        assert_ne!(reminder_event("a", now + 1, "", now).id, reminder_event("a", now + 1, "", now).id, "two in one millisecond differ");
        let long = reminder_event(&"x".repeat(200), now + 1000, "", now);
        assert_eq!(crate::reminder::validate(&long, now, true), Ok(()), "a long task is cut for the title, never refused");
    }

    #[test]
    fn a_late_one_says_so() {
        let now = 1_800_000_000_000i64;
        let ev = reminder_event("call mom", now, "s", now - 10_000);
        assert_eq!(fire_line(&ev, now + 5_000), ev.alert);
        let late = fire_line(&ev, now + LATE_AFTER_MS + 1);
        assert!(late.starts_with("Sorry sir, this one is late. A reminder: call mom."), "{late}");
    }

    #[test]
    fn the_spoken_list() {
        let now = 1_800_000_000_000i64;
        let a = reminder_event("call mom", now + 3_600_000, "", now);
        let mut b = timer_event(300, "tea", "", now);
        b.at_ms = now + 200_000;
        assert_eq!(list_line(&[], now, false), "You have no reminders set, sir.");
        assert_eq!(list_line(&[a.clone()], now, true), "You have no timers set, sir.");
        let both = list_line(&[a.clone(), b.clone()], now, false);
        assert!(both.starts_with("You have 2 reminders, sir. The tea timer has 3 minutes 20 seconds left. Call mom at "), "{both}");
        assert_eq!(list_line(&[a, b], now, true), "You have one timer, sir. The tea timer has 3 minutes 20 seconds left.");
    }

    #[test]
    fn cancelling_picks_what_was_meant_and_asks_when_it_cannot_tell() {
        let now = 1_800_000_000_000i64;
        let mut a = reminder_event("call mom", now + 100, "", now);
        a.id = "a".into();
        let mut b = reminder_event("pay rent", now + 200, "", now);
        b.id = "b".into();
        let mut t = timer_event(60, "tea", "", now);
        t.id = "t".into();
        let mut trip = reminder_event("leave", now + 300, "", now);
        trip.id = "trip".into();
        trip.source = "directions".into();
        trip.kind = "trip".into();
        let all = vec![a, b, t, trip];
        assert_eq!(pick_to_cancel(&all, "timer", false, ""), Pick::One("t".into()));
        assert_eq!(pick_to_cancel(&all, "reminder", false, "call mom"), Pick::One("a".into()));
        assert_eq!(pick_to_cancel(&all, "reminder", false, "rent"), Pick::One("b".into()));
        assert_eq!(pick_to_cancel(&all, "reminder", true, ""), Pick::Several(vec!["a".into(), "b".into()]));
        assert_eq!(pick_to_cancel(&all, "reminder", false, ""), Pick::Ask(vec!["Call mom".into(), "Pay rent".into()]));
        assert_eq!(pick_to_cancel(&all, "reminder", false, "dentist"), Pick::Nothing);
        assert_eq!(pick_to_cancel(&all, "any", true, "leave"), Pick::Nothing, "a trip reminder is not cancelled by voice here");
    }

    #[test]
    fn what_is_due_is_taken_once_and_the_rest_stays() {
        let now = 1_800_000_000_000i64;
        let mut a = reminder_event("a", now - 5, "", now - 10_000);
        a.at_ms = now - 5;
        let b = reminder_event("b", now + 5_000, "", now);
        let mut list = vec![a, b];
        let due = take_due(&mut list, now);
        assert_eq!((due.len(), list.len()), (1, 1));
        assert!(take_due(&mut list, now).is_empty(), "it does not fire twice");
    }

    #[test]
    fn the_file_round_trips_and_a_damaged_one_is_empty() {
        let dir = std::env::temp_dir().join(format!("nexus_vr_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("voice_reminders.json");
        let now = 1_800_000_000_000i64;
        let items = vec![reminder_event("call mom", now + 60_000, "s", now), timer_event(60, "tea", "s", now)];
        save_items(&p, &items).unwrap();
        assert_eq!(load_items(&p), items);
        std::fs::write(&p, "{ not json").unwrap();
        assert!(load_items(&p).is_empty());
        assert!(load_items(&dir.join("missing.json")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
