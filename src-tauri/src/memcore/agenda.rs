//! Calendar helpers (plan P5): spoken agenda, and turning "add dentist to my
//! calendar tomorrow at 5pm" into an event. Pure and unit-tested; the HTTP
//! calls live in `google_io.rs`.
//!
//! Creating an event is never silent: the draft is read back and only saved
//! after a yes (the same offer mechanism the timetable uses).

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime, TimeZone, Timelike};

use crate::google::client::GoogleClient;
use crate::google::types::{CalendarEvent, NewEvent};

use super::timetable;

#[derive(Debug, Clone, PartialEq)]
pub struct AgendaItem {
    pub summary: String,
    pub all_day: bool,
    pub start_epoch: i64,
    pub end_epoch: i64,
    /// "3 PM", "5:30 PM" or "all day".
    pub when_label: String,
}

fn fmt_local_time(dt: &DateTime<Local>) -> String {
    timetable::fmt_time((dt.hour() * 60 + dt.minute()) as u16)
}

/// Events (already sorted by Google) → display items in local time.
pub fn items_from_events(events: &[CalendarEvent]) -> Vec<AgendaItem> {
    events
        .iter()
        .filter(|e| e.status != "cancelled")
        .filter_map(|e| {
            let all_day = e.start_iso.len() == 10;
            let start = crate::google::calendar::CalendarService::parse_iso_to_epoch(&e.start_iso)?;
            let end = crate::google::calendar::CalendarService::parse_iso_to_epoch(&e.end_iso).unwrap_or(start);
            let when_label = if all_day {
                "all day".to_string()
            } else {
                fmt_local_time(&Local.timestamp_opt(start, 0).single()?)
            };
            Some(AgendaItem {
                summary: e.summary.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect(),
                all_day,
                start_epoch: start,
                end_epoch: end,
                when_label,
            })
        })
        .collect()
}

/// RFC 3339 bounds (local midnight → next local midnight) for today + offset.
pub fn day_bounds(now: DateTime<Local>, day_offset: i64) -> (String, String) {
    let date = now.date_naive() + Duration::days(day_offset);
    let start = local_midnight(date);
    let end = local_midnight(date + Duration::days(1));
    (start.to_rfc3339(), end.to_rfc3339())
}

fn local_midnight(date: NaiveDate) -> DateTime<Local> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0).unwrap_or_default())
        .earliest()
        .unwrap_or_else(Local::now)
}

/// "You have 2 events today: Dentist at 3 PM; and Team sync at 5:30 PM."
pub fn agenda_speech(items: &[AgendaItem], day_word: &str, address: Option<&str>) -> String {
    let tail = address.map(|a| format!(", {a}")).unwrap_or_default();
    if items.is_empty() {
        return format!("Nothing on your calendar {day_word}{tail}.");
    }
    let parts: Vec<String> = items
        .iter()
        .take(4)
        .map(|i| if i.all_day { format!("{}, all day", i.summary) } else { format!("{} at {}", i.summary, i.when_label) })
        .collect();
    let more = items.len().saturating_sub(4);
    let list = match parts.len() {
        1 => parts[0].clone(),
        n => format!("{}; and {}", parts[..n - 1].join("; "), parts[n - 1]),
    };
    format!(
        "You have {} event{} {day_word}{tail}: {list}{}.",
        items.len(),
        if items.len() == 1 { "" } else { "s" },
        if more > 0 { format!("; plus {more} more") } else { String::new() }
    )
}

fn md_escape(s: &str) -> String {
    s.chars()
        .flat_map(|c| if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|') { vec!['\\', c] } else { vec![c] })
        .collect()
}

pub fn card_markdown(title: &str, items: &[AgendaItem]) -> String {
    let mut md = format!("## {}\n\n", md_escape(title));
    if items.is_empty() {
        md.push_str("Nothing scheduled.\n");
    }
    for i in items {
        md.push_str(&format!("- **{}** — {}\n", md_escape(&i.summary), i.when_label));
    }
    md
}

// ─── "add X to my calendar tomorrow at 5pm" ──────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct EventDraft {
    pub summary: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    NoTitle,
    NoTime,
    InThePast,
}

impl DraftError {
    pub fn message(self) -> &'static str {
        match self {
            DraftError::NoTitle => "What should I call it, sir? Say add, then the event, to my calendar, with the day and time.",
            DraftError::NoTime => "I need the day and time, sir. For example, add dentist to my calendar tomorrow at 5 PM.",
            DraftError::InThePast => "That time has already passed today, sir.",
        }
    }
}

const WEEKDAYS: [(&str, u32); 14] = [
    ("monday", 0), ("mon", 0), ("tuesday", 1), ("tue", 1), ("tues", 1), ("wednesday", 2), ("wed", 2),
    ("thursday", 3), ("thu", 3), ("thurs", 3), ("friday", 4), ("fri", 4), ("saturday", 5), ("sunday", 6),
];

/// Day words → how many days from `today` (0 = today). Returns the offset and
/// the indexes of the words that were consumed.
fn find_day(words: &[String], today_weekday0: u32) -> Option<(i64, Vec<usize>)> {
    for (i, w) in words.iter().enumerate() {
        match w.as_str() {
            "today" | "tonight" => return Some((0, vec![i])),
            "tomorrow" => return Some((1, vec![i])),
            _ => {}
        }
        if w == "day" && words.get(i + 1).map(|x| x == "after").unwrap_or(false) && words.get(i + 2).map(|x| x == "tomorrow").unwrap_or(false) {
            return Some((2, vec![i, i + 1, i + 2]));
        }
        if let Some((_, target)) = WEEKDAYS.iter().find(|(n, _)| n == w) {
            let mut ahead = ((*target + 7 - today_weekday0) % 7) as i64;
            if ahead == 0 {
                ahead = 7;
            }
            let mut used = vec![i];
            if i > 0 && (words[i - 1] == "next" || words[i - 1] == "this") {
                used.push(i - 1);
            }
            return Some((ahead, used));
        }
    }
    None
}

/// "at 5pm", "at 5:30 pm", "at 17:00", "noon", "at 5:30 in the evening", or without "at".
/// Returns minutes and consumed word indexes.
fn find_time(words: &[String]) -> Option<(u16, Vec<usize>)> {
    for (i, w) in words.iter().enumerate() {
        if w == "noon" {
            return Some((720, vec![i]));
        }
        if w == "midnight" {
            return Some((0, vec![i]));
        }
        if w == "at" {
            // "at 5", "at 5pm", "at 5 pm", "at 5:30 pm", "at 5:30 in the evening"
            let first = words.get(i + 1)?;
            if let Some(m) = timetable::parse_time(first) {
                // Bare hour with a following am/pm word.
                if let Some(ap) = words.get(i + 2).filter(|x| matches!(x.as_str(), "am" | "pm")) {
                    if let Some(m2) = timetable::parse_time(&format!("{first}{ap}")) {
                        return Some((m2, vec![i, i + 1, i + 2]));
                    }
                }
                // "at 5:30 in the evening" / "at 5:30 in the afternoon" / "at 5:30 in the morning"
                if words.get(i + 2).map(|s| s.as_str()) == Some("in")
                    && words.get(i + 3).map(|s| s.as_str()) == Some("the")
                {
                    match words.get(i + 4).map(|s| s.as_str()) {
                        Some("evening" | "afternoon" | "night") => {
                            let pm_m = if m < 12 * 60 { m + 12 * 60 } else { m };
                            return Some((pm_m, vec![i, i + 1, i + 2, i + 3, i + 4]));
                        }
                        Some("morning") => {
                            let am_m = if m >= 12 * 60 { m - 12 * 60 } else { m };
                            return Some((am_m, vec![i, i + 1, i + 2, i + 3, i + 4]));
                        }
                        _ => {}
                    }
                }
                return Some((m, vec![i, i + 1]));
            }
        }
    }

    // Fallback: times spoken without a leading "at" (e.g. "tomorrow 5pm", "tomorrow 5:30 in the evening")
    for (i, w) in words.iter().enumerate() {
        if (w.ends_with("pm") || w.ends_with("am")) && w != "pm" && w != "am" {
            if let Some(m) = timetable::parse_time(w) {
                return Some((m, vec![i]));
            }
        }
        if let Some(ap) = words.get(i + 1).filter(|x| matches!(x.as_str(), "am" | "pm")) {
            if let Some(m) = timetable::parse_time(&format!("{w}{ap}")) {
                return Some((m, vec![i, i + 1]));
            }
        }
        if words.get(i + 1).map(|s| s.as_str()) == Some("in")
            && words.get(i + 2).map(|s| s.as_str()) == Some("the")
        {
            if let Some(m) = timetable::parse_time(w) {
                match words.get(i + 3).map(|s| s.as_str()) {
                    Some("evening" | "afternoon" | "night") => {
                        let pm_m = if m < 12 * 60 { m + 12 * 60 } else { m };
                        return Some((pm_m, vec![i, i + 1, i + 2, i + 3]));
                    }
                    Some("morning") => {
                        let am_m = if m >= 12 * 60 { m - 12 * 60 } else { m };
                        return Some((am_m, vec![i, i + 1, i + 2, i + 3]));
                    }
                    _ => {}
                }
            }
        }
    }

    None
}

fn find_duration(words: &[String]) -> (i64, Vec<usize>) {
    for (i, w) in words.iter().enumerate() {
        if w == "for" {
            if let (Some(n), Some(unit)) = (words.get(i + 1).and_then(|x| x.parse::<i64>().ok()), words.get(i + 2)) {
                if unit.starts_with("hour") {
                    return (n.clamp(1, 12) * 60, vec![i, i + 1, i + 2]);
                }
                if unit.starts_with("min") {
                    return (n.clamp(5, 720), vec![i, i + 1, i + 2]);
                }
            }
        }
    }
    (60, vec![])
}

const CALENDAR_MARKERS: &[&str] = &[
    "to my calendar", "on my calendar", "in my calendar", "to the calendar", "on the calendar",
    "to my google calendar", "on my google calendar", "in my google calendar",
];
const ADD_VERBS: &[&str] = &["add", "put", "schedule", "create", "book", "block", "set up"];

/// True for "add dentist to my calendar tomorrow at 5pm" style requests.
pub fn is_calendar_add(text: &str) -> bool {
    let t = text.to_lowercase();
    CALENDAR_MARKERS.iter().any(|m| t.contains(m)) && ADD_VERBS.iter().any(|v| t.starts_with(v) || t.contains(&format!(" {v} ")))
}

pub fn parse_event_request(text: &str, now: DateTime<Local>) -> Result<EventDraft, DraftError> {
    let lower = text.trim().to_lowercase();
    let lower = lower.trim_end_matches(['.', '!', '?']).to_string();
    // Remove the calendar marker and the leading verb, keep everything else.
    let mut rest = lower.clone();
    for m in CALENDAR_MARKERS {
        rest = rest.replace(m, " ");
    }
    let words: Vec<String> = rest
        .split(|c: char| !(c.is_alphanumeric() || c == ':' || c == '.'))
        .filter(|w| !w.is_empty())
        .map(|w| w.trim_matches('.').to_string())
        .filter(|w| !w.is_empty())
        .collect();

    let mut consumed = vec![false; words.len()];
    if let Some(first) = words.first() {
        if ADD_VERBS.iter().any(|v| v.split(' ').next() == Some(first.as_str())) {
            consumed[0] = true;
            if first == "set" && words.get(1).map(|w| w == "up").unwrap_or(false) {
                consumed[1] = true;
            }
        }
    }
    let day = find_day(&words, now.weekday().num_days_from_monday());
    if let Some((_, used)) = &day {
        for i in used {
            consumed[*i] = true;
        }
    }
    let time = find_time(&words);
    if let Some((_, used)) = &time {
        for i in used {
            consumed[*i] = true;
        }
    }
    let (minutes, dur_used) = find_duration(&words);
    for i in dur_used {
        consumed[i] = true;
    }

    const FILLER: &[&str] = &["a", "an", "the", "my", "event", "meeting", "called", "named", "titled", "please", "on", "at", "for", "next", "this", "me"];
    let title_words: Vec<&str> = words
        .iter()
        .enumerate()
        .filter(|(i, w)| !consumed[*i] && !FILLER.contains(&w.as_str()))
        .map(|(_, w)| w.as_str())
        .collect();
    // Keep "meeting"/"event" when they are the only content ("add a meeting ...").
    let summary = if title_words.is_empty() {
        let only_meeting = words.iter().enumerate().any(|(i, w)| !consumed[i] && (w == "meeting" || w == "event"));
        if only_meeting { "Meeting".to_string() } else { return Err(DraftError::NoTitle) }
    } else {
        let t = title_words.join(" ");
        let mut c = t.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or(t)
    };

    let (offset, minute) = match (day, time) {
        (Some((d, _)), Some((m, _))) => (d, m),
        (None, Some((m, _))) => (0, m),
        _ => return Err(DraftError::NoTime),
    };
    let date = now.date_naive() + Duration::days(offset);
    let start_naive = date.and_time(NaiveTime::from_hms_opt((minute / 60) as u32, (minute % 60) as u32, 0).ok_or(DraftError::NoTime)?);
    let start = Local.from_local_datetime(&start_naive).earliest().ok_or(DraftError::NoTime)?;
    if start <= now {
        return Err(DraftError::InThePast);
    }
    Ok(EventDraft { summary: summary.chars().take(80).collect(), start, end: start + Duration::minutes(minutes) })
}

pub fn to_new_event(d: &EventDraft) -> NewEvent {
    NewEvent {
        summary: d.summary.clone(),
        description: Some("Added by NEXUS".to_string()),
        start_iso: d.start.to_rfc3339(),
        end_iso: d.end.to_rfc3339(),
        location: None,
    }
}

/// "tomorrow at 5 PM" / "Friday at 5 PM" — for reading a draft back.
pub fn when_phrase(start: DateTime<Local>, now: DateTime<Local>) -> String {
    let days = (start.date_naive() - now.date_naive()).num_days();
    let day = match days {
        0 => "today".to_string(),
        1 => "tomorrow".to_string(),
        2..=6 => start.format("%A").to_string(),
        _ => start.format("%A %e %B").to_string().split_whitespace().collect::<Vec<_>>().join(" "),
    };
    format!("{day} at {}", fmt_local_time(&start))
}

/// What overlaps a draft, read back before confirming.
pub fn conflict_speech(items: &[AgendaItem], draft: &EventDraft) -> Option<String> {
    let (s, e) = (draft.start.timestamp(), draft.end.timestamp());
    let hits: Vec<&AgendaItem> = items
        .iter()
        .filter(|i| !i.all_day && s < i.end_epoch && e > i.start_epoch)
        .collect();
    match hits.as_slice() {
        [] => None,
        [one] => Some(format!("Note, that overlaps with {} at {}.", one.summary, one.when_label)),
        many => Some(format!("Note, that overlaps with {} other events.", many.len())),
    }
}

/// Generate the confirmation prompt and Offer for a proposed event draft, checking for schedule conflicts.
pub fn prompt_add_event(draft: &EventDraft, now: DateTime<Local>, existing: &[AgendaItem]) -> (String, super::offer::Offer) {
    let when = when_phrase(draft.start, now);
    let ev = to_new_event(draft);
    let conflict = conflict_speech(existing, draft).map(|c| format!(" {c}")).unwrap_or_default();
    let prompt = format!("Add {} {when} to your calendar?{conflict}", draft.summary);
    let offer = super::offer::Offer::AddEvent {
        summary: draft.summary.clone(),
        start_iso: ev.start_iso,
        end_iso: ev.end_iso,
        when,
    };
    (prompt, offer)
}

pub const CLIENT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Insert an event that the user already confirmed. Builds the `NewEvent`,
/// calls Google Calendar under a timeout, and formats the speech for success or error.
pub async fn insert_confirmed_event(
    client: &GoogleClient,
    summary: String,
    start_iso: String,
    end_iso: String,
    when: String,
    timeout: std::time::Duration,
) -> (String, bool) {
    let ev = NewEvent {
        summary: summary.clone(),
        description: Some("Added by NEXUS".to_string()),
        start_iso,
        end_iso,
        location: None,
    };
    let insert_fut = super::google_io::calendar_insert(client, &ev);
    match tokio::time::timeout(timeout, insert_fut).await {
        Ok(Ok(_)) => (format!("Done, sir. {summary} is on your calendar {when}."), true),
        Ok(Err(e)) => (insert_error_speech(&e), false),
        Err(_) => (
            insert_error_speech(&crate::google::types::GoogleError::NetworkError("connection timed out".into())),
            false,
        ),
    }
}

/// Confirm or decline the pending calendar offer.
/// If `reply` is `Reply::Yes`, executes the event insert via `client`.
/// If `reply` is `Reply::No` or anything else, drops the offer and does not call the network.
/// If no pending `AddEvent` offer exists, returns a message indicating there is nothing to confirm.
#[cfg(test)]
pub async fn handle_calendar_confirm(
    client: &GoogleClient,
    reply: super::offer::Reply,
) -> (String, bool) {
    handle_calendar_confirm_with_timeout(client, reply, CLIENT_TIMEOUT).await
}

/// Confirm or decline the pending calendar offer with a specific timeout for the HTTP insert.
#[cfg(test)]
pub async fn handle_calendar_confirm_with_timeout(
    client: &GoogleClient,
    reply: super::offer::Reply,
    timeout: std::time::Duration,
) -> (String, bool) {
    let Some(offer) = super::offer::take() else {
        return ("There is nothing to confirm, sir.".to_string(), false);
    };
    match (offer, reply) {
        (super::offer::Offer::AddEvent { summary, start_iso, end_iso, when }, super::offer::Reply::Yes) => {
            insert_confirmed_event(client, summary, start_iso, end_iso, when, timeout).await
        }
        (super::offer::Offer::AddEvent { .. }, super::offer::Reply::Later) => {
            ("Very well, sir. Ask me again whenever you're ready.".to_string(), false)
        }
        (super::offer::Offer::AddEvent { .. }, _) => {
            ("Understood, sir. I won't add it.".to_string(), false)
        }
        _ => ("There is nothing to confirm, sir.".to_string(), false),
    }
}

/// Spoken error message for a calendar insert failure.
pub fn insert_error_speech(err: &crate::google::types::GoogleError) -> String {
    match err {
        crate::google::types::GoogleError::AuthRequired | crate::google::types::GoogleError::TokenExpired => {
            "Google authentication required. Please sign in again via Settings.".to_string()
        }
        crate::google::types::GoogleError::RateLimited => {
            "Google API rate limit exceeded, sir. Please try again in a moment.".to_string()
        }
        crate::google::types::GoogleError::ApiError { code, .. } if *code == 403 => {
            "Google API quota exceeded, sir. Please check your account limits.".to_string()
        }
        crate::google::types::GoogleError::ApiError { code, .. } => {
            format!("Google calendar error {code}, sir. I couldn't save the event.")
        }
        crate::google::types::GoogleError::NetworkError(_) => {
            "Network timeout, sir. I couldn't reach Google Calendar.".to_string()
        }
        crate::google::types::GoogleError::SerializationError(_) => {
            "I received an invalid response from Google Calendar, sir.".to_string()
        }
        _ => "I couldn't save it to your calendar, sir. Please check that Google is connected.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Wednesday 2026-10-07 10:00 local.
    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 7, 10, 0, 0).earliest().unwrap()
    }

    fn event(summary: &str, start: &str, end: &str) -> CalendarEvent {
        CalendarEvent {
            id: "e".into(),
            summary: summary.into(),
            description: None,
            start_iso: start.into(),
            end_iso: end.into(),
            location: None,
            status: "confirmed".into(),
        }
    }

    #[test]
    fn parses_natural_requests() {
        let d = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        assert_eq!(d.summary, "Dentist");
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:00");
        assert_eq!((d.end - d.start).num_minutes(), 60);

        let d = parse_event_request("Schedule exam revision on my calendar on friday at 4 pm for 2 hours", now()).unwrap();
        assert_eq!(d.summary, "Exam revision");
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-09 16:00");
        assert_eq!((d.end - d.start).num_minutes(), 120);

        let d = parse_event_request("put team sync today at 5:30 pm on my calendar", now()).unwrap();
        assert_eq!((d.summary.as_str(), d.start.format("%H:%M").to_string().as_str()), ("Team sync", "17:30"));

        let d = parse_event_request("add hackathon deadline to my calendar next monday at noon", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-12 12:00");
        assert_eq!(d.summary, "Hackathon deadline");
    }

    #[test]
    fn weekday_equal_to_today_means_next_week() {
        let d = parse_event_request("add gym to my calendar on wednesday at 6pm", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d").to_string(), "2026-10-14");
    }

    #[test]
    fn reports_what_is_missing() {
        assert_eq!(parse_event_request("add dentist to my calendar", now()).unwrap_err(), DraftError::NoTime);
        assert_eq!(parse_event_request("add to my calendar tomorrow at 5pm", now()).unwrap_err(), DraftError::NoTitle);
        assert_eq!(parse_event_request("add dentist to my calendar today at 9am", now()).unwrap_err(), DraftError::InThePast);
        assert!(parse_event_request("add a meeting to my calendar tomorrow at 3pm", now()).unwrap().summary == "Meeting");
    }

    #[test]
    fn detects_calendar_add_requests_only() {
        assert!(is_calendar_add("add dentist to my calendar tomorrow at 5pm"));
        assert!(is_calendar_add("schedule a call on my calendar friday at 3"));
        assert!(!is_calendar_add("open google calendar"));
        assert!(!is_calendar_add("what's on my calendar today"));
        assert!(!is_calendar_add("add milk to my shopping list"));
    }

    #[test]
    fn event_payload_has_an_offset() {
        let d = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        let e = to_new_event(&d);
        assert!(e.start_iso.contains('T') && (e.start_iso.contains('+') || e.start_iso.contains('-') || e.start_iso.ends_with('Z')), "{}", e.start_iso);
        assert!(crate::google::calendar::CalendarService::parse_iso_to_epoch(&e.start_iso).is_some());
    }

    #[test]
    fn agenda_wording() {
        let evs = vec![
            event("Dentist", "2026-10-07T15:00:00+00:00", "2026-10-07T16:00:00+00:00"),
            event("Holiday", "2026-10-07", "2026-10-08"),
        ];
        let items = items_from_events(&evs);
        assert_eq!(items.len(), 2);
        assert!(items[1].all_day && items[1].when_label == "all day");
        let s = agenda_speech(&items, "today", Some("sir"));
        assert!(s.starts_with("You have 2 events today, sir: Dentist at ") && s.contains("Holiday, all day"), "{s}");
        assert_eq!(agenda_speech(&[], "tomorrow", Some("sir")), "Nothing on your calendar tomorrow, sir.");
        let one = agenda_speech(&items[..1], "today", None);
        assert!(one.starts_with("You have 1 event today: Dentist at "), "{one}");
        let cancelled = CalendarEvent { status: "cancelled".into(), ..evs[0].clone() };
        assert!(items_from_events(&[cancelled]).is_empty());
    }

    #[test]
    fn conflicts_and_phrases() {
        let d = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        let busy = AgendaItem {
            summary: "Team sync".into(),
            all_day: false,
            start_epoch: d.start.timestamp() - 1800,
            end_epoch: d.start.timestamp() + 1800,
            when_label: "4:30 PM".into(),
        };
        assert!(conflict_speech(&[busy.clone()], &d).unwrap().contains("Team sync"));
        let free = AgendaItem { start_epoch: d.end.timestamp() + 10, end_epoch: d.end.timestamp() + 3000, ..busy };
        assert!(conflict_speech(&[free], &d).is_none());
        assert_eq!(when_phrase(d.start, now()), "tomorrow at 5 PM");
    }

    #[test]
    fn day_bounds_span_a_local_day() {
        let (a, b) = day_bounds(now(), 0);
        let (sa, sb) = (
            crate::google::calendar::CalendarService::parse_iso_to_epoch(&a).unwrap(),
            crate::google::calendar::CalendarService::parse_iso_to_epoch(&b).unwrap(),
        );
        let span = sb - sa;
        assert!((82_800..=90_000).contains(&span), "{span}"); // 23-25 h around DST changes
    }

    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use super::super::google_io::TEST_GUARD as MOCK_GUARD;

    enum MockReply {
        Json(u16, String),
        Raw(u16, &'static str, String),
        Delay(std::time::Duration),
    }

    struct MockCalendar {
        log: Arc<Mutex<Vec<String>>>,
        stop: Arc<AtomicBool>,
    }

    impl Drop for MockCalendar {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            *super::super::google_io::TEST_BASE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    fn mock_calendar(routes: Vec<(&'static str, Vec<MockReply>)>) -> MockCalendar {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log2, stop2) = (log.clone(), stop.clone());
        let mut state: Vec<(&'static str, Vec<MockReply>, usize)> =
            routes.into_iter().map(|(k, v)| (k, v, 0)).collect();
        std::thread::spawn(move || {
            while !stop2.load(Ordering::Relaxed) {
                let Ok((mut sock, _)) = listener.accept() else {
                    std::thread::sleep(std::time::Duration::from_millis(3));
                    continue;
                };
                let _ = sock.set_nonblocking(false);
                let _ = sock.set_read_timeout(Some(std::time::Duration::from_secs(3)));
                let mut buf = [0u8; 4096];
                let n = sock.read(&mut buf).unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let method_target = req.lines().next().unwrap_or("").to_string();
                log2.lock().unwrap().push(method_target.clone());
                let reply_spec = match state.iter_mut().find(|(k, _, _)| method_target.contains(*k)) {
                    Some((_, resp, i)) => {
                        let r = match &resp[(*i).min(resp.len() - 1)] {
                            MockReply::Json(c, b) => MockReply::Json(*c, b.clone()),
                            MockReply::Raw(c, ct, b) => MockReply::Raw(*c, ct, b.clone()),
                            MockReply::Delay(d) => MockReply::Delay(*d),
                        };
                        *i += 1;
                        r
                    }
                    None => MockReply::Json(404, "{}".to_string()),
                };
                match reply_spec {
                    MockReply::Delay(d) => {
                        std::thread::sleep(d);
                    }
                    MockReply::Json(code, body) => {
                        let reply = format!(
                            "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = sock.write_all(reply.as_bytes());
                    }
                    MockReply::Raw(code, content_type, body) => {
                        let reply = format!(
                            "HTTP/1.1 {code} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = sock.write_all(reply.as_bytes());
                    }
                }
            }
        });
        *super::super::google_io::TEST_BASE.lock().unwrap_or_else(|e| e.into_inner()) = Some(format!("http://{addr}"));
        MockCalendar { log, stop }
    }

    fn test_client() -> crate::google::client::GoogleClient {
        crate::google::client::GoogleClient::new("test-token".into()).unwrap()
    }

    #[test]
    fn test_spoken_time_understanding_variants() {
        // "5pm" with and without "at"
        let d = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:00");
        let d = parse_event_request("add dentist to my calendar tomorrow 5pm", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:00");

        // "5 pm" with and without "at"
        let d = parse_event_request("add dentist to my calendar tomorrow at 5 pm", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:00");
        let d = parse_event_request("add dentist to my calendar tomorrow 5 pm", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:00");

        // "5:30 in the evening" with and without "at"
        let d = parse_event_request("add dinner to my calendar tomorrow at 5:30 in the evening", now()).unwrap();
        assert_eq!(d.summary, "Dinner");
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:30");
        let d = parse_event_request("add dinner to my calendar tomorrow 5:30 in the evening", now()).unwrap();
        assert_eq!(d.summary, "Dinner");
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 17:30");

        // "noon"
        let d = parse_event_request("add lunch to my calendar tomorrow at noon", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 12:00");
        let d = parse_event_request("add lunch to my calendar tomorrow noon", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 12:00");

        // "midnight"
        let d = parse_event_request("add sleep to my calendar tomorrow at midnight", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 00:00");

        // "tomorrow at 9"
        let d = parse_event_request("add sync to my calendar tomorrow at 9", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-08 09:00");

        // "today at 17:00"
        let d = parse_event_request("put team sync today at 17:00 on my calendar", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-07 17:00");

        // "next monday at 10am"
        let d = parse_event_request("add planning to my calendar next monday at 10am", now()).unwrap();
        assert_eq!(d.start.format("%Y-%m-%d %H:%M").to_string(), "2026-10-12 10:00");

        // Missing time: must return DraftError::NoTime and ask for day and time
        let err = parse_event_request("add dentist to my calendar tomorrow", now()).unwrap_err();
        assert_eq!(err, DraftError::NoTime);
        assert!(err.message().contains("I need the day and time, sir"));

        // Impossible time: "25 o'clock" must return DraftError::NoTime and create nothing
        let err = parse_event_request("add flight to my calendar tomorrow at 25 o'clock", now()).unwrap_err();
        assert_eq!(err, DraftError::NoTime);

        // Past time today: 8am on a 10am day must yield DraftError::InThePast
        let err = parse_event_request("add breakfast to my calendar today at 8am", now()).unwrap_err();
        assert_eq!(err, DraftError::InThePast);
        assert_eq!(err.message(), "That time has already passed today, sir.");
    }

    #[test]
    fn test_title_handling_limits_and_escaping() {
        // Very long title is clipped to 80 characters
        let long_title_req = "add comprehensive quarterly infrastructure security evaluation and cloud architecture migration review to my calendar tomorrow at 5pm";
        let d = parse_event_request(long_title_req, now()).unwrap();
        assert_eq!(d.summary.chars().count(), 80);

        // Empty title is refused with a spoken question
        let err = parse_event_request("add to my calendar tomorrow at 5pm", now()).unwrap_err();
        assert_eq!(err, DraftError::NoTitle);
        assert!(err.message().starts_with("What should I call it, sir?"));

        // Title containing quotes, ampersand, emoji and non-ASCII is carried unchanged into payload
        let special_summary = "Sprint \"Retro\" & Planning \u{1f680} café";
        let special_draft = EventDraft {
            summary: special_summary.to_string(),
            start: now() + Duration::days(1),
            end: now() + Duration::days(1) + Duration::hours(1),
        };
        let ev = to_new_event(&special_draft);
        assert_eq!(ev.summary, special_summary);
        let payload = crate::google::calendar::CalendarService::create_event_payload(&ev);
        assert_eq!(payload["summary"], special_summary);
        let json_str = serde_json::to_string(&payload).unwrap();
        assert!(json_str.contains(r#"Sprint \"Retro\" & Planning"#));
        assert!(json_str.contains("café") || json_str.contains(r#"caf\u00e9"#));
    }

    #[test]
    fn test_conflict_handling_voice_flow_and_decline() {
        let _g = MOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        super::super::offer::clear();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mock = mock_calendar(vec![(
            "/calendar/v3/calendars/primary/events",
            vec![MockReply::Json(
                200,
                r#"{"id":"ev_123","summary":"Dentist","start":{"dateTime":"2026-10-08T17:00:00+05:30"}}"#.into(),
            )],
        )]);

        let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        // Existing event "Team sync" from 16:30 to 17:30
        let existing = vec![AgendaItem {
            summary: "Team sync".into(),
            all_day: false,
            start_epoch: draft.start.timestamp() - 1800,
            end_epoch: draft.start.timestamp() + 1800,
            when_label: "4:30 PM".into(),
        }];

        // 1. Spoken warning generated naming the existing event
        let (prompt, offer) = prompt_add_event(&draft, now(), &existing);
        assert!(
            prompt.contains("Note, that overlaps with Team sync at 4:30 PM."),
            "Prompt must warn about the overlapping event: {prompt}"
        );
        super::super::offer::set(offer, true);

        // 2. Before user responds, no insert request has reached the server
        let post_count = mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count();
        assert_eq!(post_count, 0, "No POST should be sent before user confirmation");

        // 3. User declines (Reply::No) -> nothing is inserted
        rt.block_on(async {
            let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::No).await;
            assert!(!added, "Event must not be added when user declines");
            assert_eq!(speech, "Understood, sir. I won't add it.");
        });

        // 4. Assert mock server saw zero POST requests
        let post_count = mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count();
        assert_eq!(post_count, 0, "Decline must never send a POST to the calendar API");
        super::super::offer::clear();
    }

    #[test]
    fn test_confirm_gate_no_insert_without_confirm_and_empty_offer() {
        let _g = MOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        super::super::offer::clear();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mock = mock_calendar(vec![(
            "/calendar/v3/calendars/primary/events",
            vec![MockReply::Json(
                200,
                r#"{"id":"ev_new","summary":"Dentist","start":{"dateTime":"2026-10-08T17:00:00+05:30"}}"#.into(),
            )],
        )]);

        // 1. Confirming when there is no pending event says there is nothing to confirm
        rt.block_on(async {
            let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
            assert!(!added);
            assert_eq!(speech, "There is nothing to confirm, sir.");
        });
        assert_eq!(mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count(), 0);

        // 2. Propose an event
        let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
        let (_, offer) = prompt_add_event(&draft, now(), &[]);
        super::super::offer::set(offer, true);

        // Still no POST before confirm
        assert_eq!(mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count(), 0);

        // 3. Confirm with Yes -> event is inserted
        rt.block_on(async {
            let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
            assert!(added, "Event must be reported as added");
            assert!(speech.starts_with("Done, sir. Dentist is on your calendar"));
        });
        assert_eq!(mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count(), 1);

        // 4. Repeated confirmation attempt after offer consumed -> nothing to confirm, does not insert twice
        rt.block_on(async {
            let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
            assert!(!added);
            assert_eq!(speech, "There is nothing to confirm, sir.");
        });
        assert_eq!(
            mock.log.lock().unwrap().iter().filter(|t| t.starts_with("POST ")).count(),
            1,
            "Cannot insert twice on a consumed offer"
        );
        super::super::offer::clear();
    }

    #[test]
    fn test_mock_server_failure_modes_401_403_500_timeout_invalid_json() {
        let _g = MOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        super::super::offer::clear();
        let rt = tokio::runtime::Runtime::new().unwrap();

        // 401 Unauthorized -> spoken "sign in again" message, does not loop
        {
            let _mock = mock_calendar(vec![(
                "/calendar/v3/calendars/primary/events",
                vec![MockReply::Json(401, r#"{"error":{"code":401,"message":"Invalid Credentials"}}"#.into())],
            )]);
            let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
            let (_, offer) = prompt_add_event(&draft, now(), &[]);
            super::super::offer::set(offer, true);
            rt.block_on(async {
                let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
                assert!(!added);
                assert!(
                    speech.contains("sign in again") || speech.contains("authentication required"),
                    "401 must produce sign-in-again message: {speech}"
                );
            });
        }

        // 403 Forbidden / Quota exceeded -> spoken quota message
        {
            let _mock = mock_calendar(vec![(
                "/calendar/v3/calendars/primary/events",
                vec![MockReply::Json(403, r#"{"error":{"code":403,"message":"Quota exceeded"}}"#.into())],
            )]);
            let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
            let (_, offer) = prompt_add_event(&draft, now(), &[]);
            super::super::offer::set(offer, true);
            rt.block_on(async {
                let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
                assert!(!added);
                assert!(
                    speech.contains("quota") || speech.contains("authentication"),
                    "403 must yield quota or auth guidance: {speech}"
                );
            });
        }

        // 500 Internal Server Error -> spoken error message
        {
            let _mock = mock_calendar(vec![(
                "/calendar/v3/calendars/primary/events",
                vec![MockReply::Json(500, r#"{"error":{"code":500,"message":"Internal Server Error"}}"#.into())],
            )]);
            let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
            let (_, offer) = prompt_add_event(&draft, now(), &[]);
            super::super::offer::set(offer, true);
            rt.block_on(async {
                let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
                assert!(!added);
                assert!(speech.contains("500") || speech.contains("couldn't save"), "{speech}");
            });
        }

        // Timeout: server delays for 3s, client timeout configured to 400ms -> finishes under 1s, never panics
        {
            let _mock = mock_calendar(vec![(
                "/calendar/v3/calendars/primary/events",
                vec![MockReply::Delay(std::time::Duration::from_millis(3000))],
            )]);
            let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
            let (_, offer) = prompt_add_event(&draft, now(), &[]);
            super::super::offer::set(offer, true);
            let t0 = std::time::Instant::now();
            rt.block_on(async {
                let (speech, added) = handle_calendar_confirm_with_timeout(
                    &test_client(),
                    super::super::offer::Reply::Yes,
                    std::time::Duration::from_millis(400),
                )
                .await;
                assert!(!added);
                assert!(speech.contains("timeout") || speech.contains("reach"), "{speech}");
            });
            let elapsed = t0.elapsed();
            assert!(elapsed < std::time::Duration::from_millis(1500), "Timeout must complete under 1.5s: {elapsed:?}");
        }

        // Invalid JSON body -> does not panic, returns short spoken error
        {
            let _mock = mock_calendar(vec![(
                "/calendar/v3/calendars/primary/events",
                vec![MockReply::Raw(200, "application/json", "{not valid json at all".into())],
            )]);
            let draft = parse_event_request("add dentist to my calendar tomorrow at 5pm", now()).unwrap();
            let (_, offer) = prompt_add_event(&draft, now(), &[]);
            super::super::offer::set(offer, true);
            rt.block_on(async {
                let (speech, added) = handle_calendar_confirm(&test_client(), super::super::offer::Reply::Yes).await;
                assert!(!added);
                assert!(speech.contains("invalid") || speech.contains("couldn't save"), "{speech}");
            });
        }

        super::super::offer::clear();
    }

    #[test]
    fn test_insert_confirmed_event_direct_success_and_timeout() {
        let _g = MOCK_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _mock = mock_calendar(vec![(
            "/calendar/v3/calendars/primary/events",
            vec![
                MockReply::Json(
                    200,
                    r#"{"id":"ev_direct","summary":"Dentist","start":{"dateTime":"2026-10-08T17:00:00+05:30"}}"#.into(),
                ),
                MockReply::Delay(std::time::Duration::from_millis(500)),
            ],
        )]);

        rt.block_on(async {
            // Direct call: success returns formatted speech and true
            let (speech, added) = insert_confirmed_event(
                &test_client(),
                "Dentist".to_string(),
                "2026-10-08T17:00:00Z".to_string(),
                "2026-10-08T18:00:00Z".to_string(),
                "tomorrow at 5 PM".to_string(),
                CLIENT_TIMEOUT,
            )
            .await;
            assert!(added);
            assert_eq!(speech, "Done, sir. Dentist is on your calendar tomorrow at 5 PM.");

            // Direct call: timeout returns network error speech and false
            let (speech, added) = insert_confirmed_event(
                &test_client(),
                "Dentist".to_string(),
                "2026-10-08T17:00:00Z".to_string(),
                "2026-10-08T18:00:00Z".to_string(),
                "tomorrow at 5 PM".to_string(),
                std::time::Duration::from_millis(50),
            )
            .await;
            assert!(!added);
            assert!(speech.contains("Network timeout") || speech.contains("reach Google Calendar"), "{speech}");
        });
    }
}
