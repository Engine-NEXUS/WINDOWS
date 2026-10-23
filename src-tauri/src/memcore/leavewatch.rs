//! Leave-by reminders: "Time to leave for your dentist appointment" - computed
//! from the calendar event's location, live traffic (Routes API) and your
//! remembered home address.
//!
//! What it deliberately does NOT do: it does not know where you are right now
//! (NEXUS has no device location), so the trip is always measured FROM HOME and
//! the spoken line says so. Events without a physical location (no location,
//! video-call links, all-day events) are ignored.
//!
//! Cost control: the calendar is read every 10 minutes (one cheap call); the
//! Routes API is called only for events starting within 90 minutes, at most
//! once per 10 minutes per event and 3 calls per tick.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::{Local, TimeZone};
use tauri::{AppHandle, Manager, Runtime};

use crate::google::maps::MapsService;
use crate::google::types::{AlertUrgency, CalendarEvent, TravelMode};

use super::google_io;

pub const TICK_SECS: u64 = 60;
const CAL_REFRESH_SECS: u64 = 10 * 60;
const ETA_REFRESH_SECS: u64 = 10 * 60;
/// Calendar look-ahead.
const HORIZON_SECS: i64 = 3 * 3600;
/// Only events starting this soon get a (billable) route lookup.
const ETA_WINDOW_SECS: i64 = 90 * 60;
/// Extra minutes to be on time (parking, walking in).
const BUFFER_SECS: i64 = 5 * 60;
/// Speak this long BEFORE the exact leave time.
const LEAD_SECS: i64 = 5 * 60;
/// A longer "trip" than this is a wrong geocode, not a commute.
const MAX_ETA_SECS: i64 = 3 * 3600;
const MAX_ETA_CALLS_PER_TICK: usize = 3;
/// After a failed route lookup, leave that event alone this long (a bad key or a quota error must not be retried every minute).
const ETA_RETRY_AFTER_FAILURE_SECS: u64 = 2 * 60;

#[derive(Debug, Clone, PartialEq)]
pub struct LeaveEvent {
    pub id: String,
    pub summary: String,
    pub location: String,
    pub start_epoch: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Reminder {
    /// Stable per event+start, so a moved event can remind again.
    pub key: String,
    pub urgency: AlertUrgency,
    pub text: String,
}

/// True for an address-like location; false for video-call links and "online".
pub fn is_physical_location(loc: &str) -> bool {
    let l = loc.trim().to_lowercase();
    if l.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
        return false;
    }
    if l.contains("http://") || l.contains("https://") || l.contains("www.") || l.contains("meet.google") {
        return false;
    }
    const VIRTUAL: &[&str] = &[
        "zoom", "meet", "teams", "webex", "skype", "online", "virtual", "remote", "phone", "call", "video",
        "hangouts", "slack", "tbd", "tba",
    ];
    !l.split(|c: char| !c.is_alphanumeric()).any(|w| VIRTUAL.contains(&w))
}

/// Timed, non-cancelled events with a physical location that start within
/// `horizon_secs` from `now`. Pure.
pub fn candidate_events(events: &[CalendarEvent], now: i64, horizon_secs: i64) -> Vec<LeaveEvent> {
    events
        .iter()
        .filter(|e| e.status != "cancelled" && e.start_iso.len() > 10)
        .filter_map(|e| {
            let loc = e.location.as_deref()?.trim();
            if !is_physical_location(loc) {
                return None;
            }
            let start = crate::google::calendar::CalendarService::parse_iso_to_epoch(&e.start_iso)?;
            if start <= now || start > now + horizon_secs {
                return None;
            }
            Some(LeaveEvent {
                id: e.id.clone(),
                summary: e.summary.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(60).collect(),
                location: loc.chars().take(160).collect(),
                start_epoch: start,
            })
        })
        .collect()
}

fn clock_label(epoch: i64) -> String {
    match Local.timestamp_opt(epoch, 0).single() {
        Some(dt) => {
            use chrono::Timelike;
            super::timetable::fmt_time((dt.hour() * 60 + dt.minute()) as u16)
        }
        None => String::new(),
    }
}

/// Decide whether to speak now. Pure. `address` = how to address the user ("sir").
pub fn decide(ev: &LeaveEvent, eta_secs: i64, now: i64, address: Option<&str>) -> Option<Reminder> {
    if eta_secs <= 0 || eta_secs > MAX_ETA_SECS || now >= ev.start_epoch {
        return None;
    }
    let leave_at = ev.start_epoch - eta_secs - BUFFER_SECS;
    if now < leave_at - LEAD_SECS {
        return None;
    }
    let eta_min = ((eta_secs + 59) / 60).max(1);
    let tail = address.map(|a| format!(", {a}")).unwrap_or_default();
    let at = clock_label(ev.start_epoch);
    let trip = format!("It's about {eta_min} minute{} from home and starts at {at}.", if eta_min == 1 { "" } else { "s" });
    let (urgency, text) = if now >= ev.start_epoch - eta_secs {
        (AlertUrgency::High, format!("You're running late for {}{tail}. {trip}", ev.summary))
    } else if now >= leave_at {
        (AlertUrgency::High, format!("Time to leave for {}{tail}. {trip}", ev.summary))
    } else {
        let mins = (((leave_at - now) + 59) / 60).max(1);
        (
            AlertUrgency::Medium,
            format!("Leave in about {mins} minute{} for {}{tail}. {trip}", if mins == 1 { "" } else { "s" }, ev.summary),
        )
    };
    Some(Reminder { key: format!("{}:{}", ev.id, ev.start_epoch), urgency, text })
}

/// Stable per event and start time: a moved event reminds again, a re-read of the same event does not.
pub fn event_key(ev: &LeaveEvent) -> String {
    format!("{}:{}", ev.id, ev.start_epoch)
}

/// Why the watcher does nothing this tick (`None` = it may run). Pure.
pub fn skip_reason(memcore_on: bool, leaveby_on: bool, maps_key_set: bool, has_home: bool, has_account: bool) -> Option<&'static str> {
    if !memcore_on {
        Some("memory is off")
    } else if !leaveby_on {
        Some("leave-by reminders are off")
    } else if !maps_key_set {
        Some("no Routes API key")
    } else if !has_home {
        Some("no remembered home address")
    } else if !has_account {
        Some("no connected Google account with calendar access")
    } else {
        None
    }
}

/// Events that need a (billable) route lookup now: starting within the lookup window, not yet reminded, with
/// no fresh answer, at most `max` of them. Pure.
pub fn needs_lookup<'a>(
    events: &'a [LeaveEvent],
    now: i64,
    has_fresh_answer: impl Fn(&str) -> bool,
    already_reminded: impl Fn(&str) -> bool,
    max: usize,
) -> Vec<&'a LeaveEvent> {
    events
        .iter()
        .filter(|ev| ev.start_epoch > now && ev.start_epoch - now <= ETA_WINDOW_SECS)
        .filter(|ev| !already_reminded(&event_key(ev)))
        .filter(|ev| !has_fresh_answer(&ev.id))
        .take(max)
        .collect()
}

/// Reminders due now: events in the window, not yet reminded, whose trip time is known. Pure.
pub fn due_reminders(
    events: &[LeaveEvent],
    now: i64,
    eta_secs_of: impl Fn(&str) -> Option<i64>,
    already_reminded: impl Fn(&str) -> bool,
    address: Option<&str>,
) -> Vec<(String, Reminder)> {
    events
        .iter()
        .filter(|ev| ev.start_epoch > now && ev.start_epoch - now <= ETA_WINDOW_SECS)
        .filter(|ev| !already_reminded(&event_key(ev)))
        .filter_map(|ev| {
            let eta = eta_secs_of(&ev.id)?;
            decide(ev, eta, now, address).map(|r| (event_key(ev), r))
        })
        .collect()
}

fn already_fired(dir: &std::path::Path, key: &str) -> bool {
    super::with_store(dir, |s| s.meta_get(&format!("leaveby:{key}")).is_some()).unwrap_or(false)
}

fn mark_fired(dir: &std::path::Path, key: &str) {
    let _ = super::with_store(dir, |s| s.meta_set(&format!("leaveby:{key}"), "1"));
}

pub fn home_address(dir: &std::path::Path) -> Option<String> {
    crate::orchestrator::pick_saved_place(&crate::memory::recall(dir, "home"), "home")
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_default();
        let mut events: Vec<LeaveEvent> = vec![];
        let mut next_cal = Instant::now();
        let mut etas: HashMap<String, (i64, Instant)> = HashMap::new();
        let mut failed: HashMap<String, Instant> = HashMap::new();
        let mut fired: HashSet<String> = HashSet::new();
        loop {
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
            let Ok(dir) = app.path().app_data_dir() else { continue };
            let key = crate::commands::read_api_key(&app, "google_maps");
            let home = home_address(&dir);
            let acct = google_io::primary_account(google_io::can_use_calendar);
            if skip_reason(
                super::enabled(&dir),
                super::flag(&dir, "memcoreLeaveBy", true),
                !key.is_empty(),
                home.is_some(),
                acct.is_some(),
            )
            .is_some()
            {
                continue;
            }
            let (Some(home), Some(acct)) = (home, acct) else { continue };
            let now = chrono::Utc::now().timestamp();

            if Instant::now() >= next_cal {
                next_cal = Instant::now() + Duration::from_secs(CAL_REFRESH_SECS);
                let from = chrono::Local::now().to_rfc3339();
                let to = (chrono::Local::now() + chrono::Duration::seconds(HORIZON_SECS)).to_rfc3339();
                let got = google_io::with_client(acct.email.as_deref(), |c| {
                    let (from, to) = (from.clone(), to.clone());
                    async move { google_io::calendar_events(&c, &from, &to).await }
                })
                .await;
                match got {
                    Ok(list) => events = candidate_events(&list, now, HORIZON_SECS),
                    Err(e) => tracing::debug!("leavewatch: calendar: {e}"),
                }
                etas.retain(|id, _| events.iter().any(|e| &e.id == id));
                failed.retain(|id, _| events.iter().any(|e| &e.id == id));
            }

            let address = super::wa::speak_address(&app);
            let reminded = |k: &str| fired.contains(k) || already_fired(&dir, k);
            let fresh = |id: &str| {
                etas.get(id).is_some_and(|(_, at)| at.elapsed() < Duration::from_secs(ETA_REFRESH_SECS))
                    || failed.get(id).is_some_and(|at| at.elapsed() < Duration::from_secs(ETA_RETRY_AFTER_FAILURE_SECS))
            };
            let todo: Vec<LeaveEvent> = needs_lookup(&events, now, fresh, reminded, MAX_ETA_CALLS_PER_TICK)
                .into_iter()
                .cloned()
                .collect();
            for ev in &todo {
                match MapsService::compute_route(&client, &key, &home, &ev.location, TravelMode::Driving).await {
                    Ok(eta) => {
                        failed.remove(&ev.id);
                        etas.insert(ev.id.clone(), (eta.minutes as i64 * 60, Instant::now()));
                    }
                    Err(e) => {
                        tracing::debug!("leavewatch: routes: {e}");
                        failed.insert(ev.id.clone(), Instant::now());
                    }
                }
            }
            let due = {
                let reminded = |k: &str| fired.contains(k) || already_fired(&dir, k);
                due_reminders(&events, now, |id| etas.get(id).map(|(s, _)| *s), reminded, address.as_deref())
            };
            for (k, r) in due {
                fired.insert(k.clone());
                mark_fired(&dir, &k);
                tracing::info!("leavewatch: reminder {k}");
                crate::proactive_policy::submit(&app, format!("leaveby_{}", r.key), r.text, r.urgency);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn cal(id: &str, start_offset: i64, loc: Option<&str>) -> CalendarEvent {
        let start = chrono::DateTime::from_timestamp(NOW + start_offset, 0).unwrap().to_rfc3339();
        CalendarEvent {
            id: id.into(),
            summary: "Dentist  appointment".into(),
            description: None,
            start_iso: start.clone(),
            end_iso: start,
            location: loc.map(String::from),
            status: "confirmed".into(),
        }
    }

    fn ev(start_offset: i64) -> LeaveEvent {
        LeaveEvent { id: "e1".into(), summary: "Dentist".into(), location: "5 High St".into(), start_epoch: NOW + start_offset }
    }

    #[test]
    fn physical_vs_virtual_locations() {
        for ok in ["5 High Street, London", "Dr Smith's Clinic", "Starbucks Oxford Circus", "Callaway Street 4"] {
            assert!(is_physical_location(ok), "{ok}");
        }
        for bad in ["", "  ", "ab", "https://zoom.us/j/123", "Zoom", "Google Meet", "meet.google.com/abc-defg", "Microsoft Teams Meeting", "Online", "Phone call", "TBD"] {
            assert!(!is_physical_location(bad), "{bad}");
        }
    }

    #[test]
    fn only_timed_future_physical_events_within_horizon_qualify() {
        let mut all_day = cal("allday", 3600, Some("5 High St"));
        all_day.start_iso = "2026-10-09".into();
        let mut cancelled = cal("c", 3600, Some("5 High St"));
        cancelled.status = "cancelled".into();
        let list = vec![
            cal("ok", 3600, Some(" 5 High St ")),
            cal("noloc", 3600, None),
            cal("virtual", 3600, Some("Zoom")),
            cal("past", -600, Some("5 High St")),
            cal("far", 5 * 3600, Some("5 High St")),
            all_day,
            cancelled,
        ];
        let got = candidate_events(&list, NOW, HORIZON_SECS);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].id.as_str(), got[0].location.as_str(), got[0].summary.as_str()), ("ok", "5 High St", "Dentist appointment"));
    }

    #[test]
    fn silent_until_five_minutes_before_leave_time() {
        // Event in 60 min, trip 20 min, buffer 5: leave at +35 min; speak from +30 min.
        let e = ev(60 * 60);
        assert_eq!(decide(&e, 20 * 60, NOW + 29 * 60, None), None);
        let r = decide(&e, 20 * 60, NOW + 30 * 60, Some("sir")).unwrap();
        assert_eq!(r.urgency, AlertUrgency::Medium);
        assert!(r.text.starts_with("Leave in about 5 minutes for Dentist, sir."), "{}", r.text);
        assert!(r.text.contains("It's about 20 minutes from home and starts at"), "{}", r.text);
        assert_eq!(r.key, format!("e1:{}", NOW + 3600));
    }

    #[test]
    fn leave_now_then_late_then_expired() {
        let e = ev(60 * 60);
        let r = decide(&e, 20 * 60, NOW + 36 * 60, None).unwrap();
        assert_eq!(r.urgency, AlertUrgency::High);
        assert!(r.text.starts_with("Time to leave for Dentist."), "{}", r.text);
        // 45 min in: only 15 min left for a 20 min trip.
        let r = decide(&e, 20 * 60, NOW + 45 * 60, None).unwrap();
        assert!(r.text.starts_with("You're running late for Dentist."), "{}", r.text);
        // Event already started: never speak.
        assert_eq!(decide(&e, 20 * 60, NOW + 61 * 60, None), None);
    }

    fn lev(id: &str, start_offset: i64) -> LeaveEvent {
        LeaveEvent { id: id.into(), summary: id.into(), location: "5 High St".into(), start_epoch: NOW + start_offset }
    }

    #[test]
    fn the_watcher_names_the_first_missing_ingredient() {
        assert_eq!(skip_reason(true, true, true, true, true), None);
        assert_eq!(skip_reason(false, true, true, true, true), Some("memory is off"));
        assert_eq!(skip_reason(true, false, true, true, true), Some("leave-by reminders are off"));
        assert_eq!(skip_reason(true, true, false, true, true), Some("no Routes API key"));
        assert_eq!(skip_reason(true, true, true, false, true), Some("no remembered home address"));
        assert_eq!(skip_reason(true, true, true, true, false), Some("no connected Google account with calendar access"));
        // several missing: the first in the fixed order wins
        assert_eq!(skip_reason(true, true, false, false, false), Some("no Routes API key"));
    }

    #[test]
    fn only_events_inside_the_ninety_minute_window_are_looked_up() {
        let evs = vec![lev("soon", 30 * 60), lev("edge", 90 * 60), lev("far", 90 * 60 + 1), lev("started", -1), lev("now", 0)];
        let got: Vec<_> = needs_lookup(&evs, NOW, |_| false, |_| false, 10).iter().map(|e| e.id.as_str()).collect();
        assert_eq!(got, ["soon", "edge"], "exactly 90 min is in, one second later is out, started events are out");
    }

    #[test]
    fn a_fresh_answer_a_finished_reminder_or_the_per_tick_cap_each_stop_a_lookup() {
        let evs = vec![lev("a", 600), lev("b", 700), lev("c", 800), lev("d", 900), lev("e", 1000)];
        assert_eq!(needs_lookup(&evs, NOW, |_| false, |_| false, MAX_ETA_CALLS_PER_TICK).len(), 3, "billable calls are capped per tick");
        let got: Vec<_> = needs_lookup(&evs, NOW, |id| id == "a", |_| false, 10).iter().map(|e| e.id.clone()).collect();
        assert_eq!(got, ["b", "c", "d", "e"], "a fresh answer is not asked again");
        let done = format!("b:{}", NOW + 700);
        let got: Vec<_> = needs_lookup(&evs, NOW, |_| false, |k| k == done, 10).iter().map(|e| e.id.clone()).collect();
        assert!(!got.contains(&"b".to_string()), "an event already reminded is never looked up again");
    }

    #[test]
    fn a_reminder_needs_a_known_trip_time_and_is_given_once_per_event_start() {
        let evs = vec![lev("a", 60 * 60)];
        // no trip time yet: nothing
        assert!(due_reminders(&evs, NOW + 31 * 60, |_| None, |_| false, None).is_empty());
        // trip known and it is time
        let due = due_reminders(&evs, NOW + 31 * 60, |_| Some(20 * 60), |_| false, Some("sir"));
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].0, format!("a:{}", NOW + 3600));
        assert!(due[0].1.text.contains(", sir."), "{}", due[0].1.text);
        // the same event again after it was recorded: nothing
        let key = due[0].0.clone();
        assert!(due_reminders(&evs, NOW + 31 * 60, |_| Some(20 * 60), |k| k == key, None).is_empty());
        // the event is moved 30 minutes later: a new key, so it can remind again
        let moved = vec![lev("a", 90 * 60)];
        assert_ne!(event_key(&moved[0]), key);
        assert_eq!(due_reminders(&moved, NOW + 61 * 60, |_| Some(20 * 60), |k| k == key, None).len(), 1);
    }

    #[test]
    fn two_events_in_one_tick_give_two_reminders_in_calendar_order() {
        let evs = vec![lev("first", 40 * 60), lev("second", 45 * 60)];
        let due = due_reminders(&evs, NOW + 25 * 60, |_| Some(10 * 60), |_| false, None);
        assert_eq!(due.iter().map(|(k, _)| k.split(':').next().unwrap()).collect::<Vec<_>>(), ["first", "second"]);
    }

    #[test]
    fn calendar_time_zones_are_read_correctly() {
        use crate::google::calendar::CalendarService as C;
        let utc = C::parse_iso_to_epoch("2026-10-20T09:30:00Z").unwrap();
        assert_eq!(C::parse_iso_to_epoch("2026-10-20T15:00:00+05:30"), Some(utc));
        assert_eq!(C::parse_iso_to_epoch("2026-10-20T04:30:00-05:00"), Some(utc));
        assert_eq!(C::parse_iso_to_epoch("not a time"), None);
    }

    #[test]
    fn the_given_reminder_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("nexus_leaveby_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!already_fired(&dir, "e1:100"));
        mark_fired(&dir, "e1:100");
        assert!(already_fired(&dir, "e1:100"));
        assert!(!already_fired(&dir, "e1:101"), "another start time is another reminder");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nonsense_trip_times_are_ignored() {
        let e = ev(3600);
        assert_eq!(decide(&e, 0, NOW, None), None);
        assert_eq!(decide(&e, -5, NOW, None), None);
        assert_eq!(decide(&e, 4 * 3600, NOW, None), None, "a 4 h trip is a wrong geocode");
        // 1 minute trip is spoken in the singular.
        let r = decide(&ev(7 * 60), 30, NOW + 60, None).unwrap();
        assert!(r.text.contains("about 1 minute from home"), "{}", r.text);
    }
}
