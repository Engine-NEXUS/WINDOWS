//! The one list of reminders the stage shows, and the one event every reminder uses.
//!
//! Any feature that sets a reminder (a trip, a calendar leave-by, a timetable slot, a timer, a spoken "remind me")
//! sends the same `ReminderEvent` here; this module validates it, keeps the list, and tells the stage:
//!
//! * `reminder:roll`   one event, a reminder was just set: the stage rolls the clock from now to its time.
//! * `reminder:update` the whole list, after every change: the stage draws one countdown chip per entry.
//! * `reminder:fire`   `{id, title, alert}`: the time has come: the stage shows the alert chip.
//!
//! Nothing here knows what a reminder is for. `kind` only picks a label. The producer keeps its own decision logic
//! (when to speak, how to re-measure) and tells this module the result (`announce`, `retime`, `fired`, `cancel`), so
//! the animation is identical for every producer and no producer contains animation code.
//!
//! The spoken line is NOT drawn by the reminder component: NEXUS says it through the normal speech path and the
//! response caption already shows it, letter by letter, in step with the voice. `spoken` is carried so a future
//! producer can use one string for both.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{AppHandle, Runtime};

/// The closed list of kinds (a label only; the animation is the same for all).
pub const KINDS: &[&str] = &["trip", "reminder", "calendar", "timer", "study"];
pub const MAX_TITLE_CHARS: usize = 60;
const MAX_LINE_CHARS: usize = 240;
/// A reminder is dropped from the list this long after its time (a producer that never calls `fired`).
pub const KEEP_AFTER_MS: i64 = 30 * 60 * 1000;
const MAX_ACTIVE: usize = 24;

/// What the producer asks of the display. The user's own setting (`reminderCountdown`) always wins: a reminder can
/// ask for no chip, but can never show one when the user has turned the countdown off (the stage applies that rule).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    #[serde(default = "yes")]
    pub countdown: bool,
}

fn yes() -> bool {
    true
}

impl Default for Display {
    fn default() -> Self {
        Display { countdown: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReminderEvent {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// The moment it fires, epoch milliseconds.
    pub at_ms: i64,
    /// The line NEXUS says when it is set.
    pub spoken: String,
    /// The line shown (and said) when it fires.
    pub alert: String,
    /// Which feature produced it ("directions", "calendar", "timetable", "voice", "timer"). Never used for behaviour.
    pub source: String,
    #[serde(default)]
    pub display: Display,
}

/// The fire notice sent to the stage.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fired {
    pub id: String,
    pub title: String,
    pub alert: String,
}

static ACTIVE: Mutex<Vec<ReminderEvent>> = Mutex::new(Vec::new());

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Whether an event can be shown. `future` is false only when restoring a reminder that was set earlier. Pure.
pub fn validate(ev: &ReminderEvent, now: i64, future: bool) -> Result<(), &'static str> {
    if ev.id.trim().is_empty() || ev.id.len() > 64 {
        return Err("a reminder needs an id of at most 64 characters");
    }
    if !KINDS.contains(&ev.kind.as_str()) {
        return Err("unknown reminder kind");
    }
    let t = ev.title.trim().chars().count();
    if t == 0 || t > MAX_TITLE_CHARS {
        return Err("a reminder title is 1 to 60 characters");
    }
    if ev.spoken.chars().count() > MAX_LINE_CHARS || ev.alert.chars().count() > MAX_LINE_CHARS {
        return Err("a reminder line is at most 240 characters");
    }
    if future && ev.at_ms <= now {
        return Err("a reminder must be in the future");
    }
    if !future && ev.at_ms + KEEP_AFTER_MS <= now {
        return Err("that reminder is long past");
    }
    Ok(())
}

/// Insert or replace by id; the oldest entry goes when the list is full. Pure.
pub fn upsert(list: &mut Vec<ReminderEvent>, ev: ReminderEvent) {
    match list.iter_mut().find(|e| e.id == ev.id) {
        Some(existing) => *existing = ev,
        None => {
            list.push(ev);
            if list.len() > MAX_ACTIVE {
                list.remove(0);
            }
        }
    }
}

/// Remove by id; the removed entry, if any. Pure.
pub fn remove(list: &mut Vec<ReminderEvent>, id: &str) -> Option<ReminderEvent> {
    let at = list.iter().position(|e| e.id == id)?;
    Some(list.remove(at))
}

/// Drop entries whose time is long past. Pure.
pub fn prune(list: &mut Vec<ReminderEvent>, now: i64) {
    list.retain(|e| e.at_ms + KEEP_AFTER_MS > now);
}

fn snapshot() -> Vec<ReminderEvent> {
    let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    prune(&mut g, now_ms());
    g.sort_by_key(|e| e.at_ms);
    g.clone()
}

fn publish<R: Runtime>(app: &AppHandle<R>) {
    crate::commands::emit_logged(app, "reminder:update", snapshot());
}

/// A reminder was just set: store it and tell the stage to roll the clock to its time.
pub fn announce<R: Runtime>(app: &AppHandle<R>, ev: ReminderEvent) -> Result<(), String> {
    validate(&ev, now_ms(), true).map_err(|e| e.to_string())?;
    {
        let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        upsert(&mut g, ev.clone());
    }
    tracing::info!("reminder: set {} ({}) from {}", ev.id, ev.kind, ev.source);
    crate::commands::emit_logged(app, "reminder:roll", ev);
    publish(app);
    Ok(())
}

/// A reminder that was set earlier (after a restart): show its chip, with no animation.
pub fn restore<R: Runtime>(app: &AppHandle<R>, ev: ReminderEvent) -> Result<(), String> {
    validate(&ev, now_ms(), false).map_err(|e| e.to_string())?;
    {
        let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        upsert(&mut g, ev);
    }
    publish(app);
    Ok(())
}

/// The producer re-measured and the time moved: update the chip, no animation. False when there is no such reminder.
pub fn retime<R: Runtime>(app: &AppHandle<R>, id: &str, at_ms: i64) -> bool {
    let changed = {
        let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        match g.iter_mut().find(|e| e.id == id) {
            Some(e) if at_ms > now_ms() && e.at_ms != at_ms => {
                e.at_ms = at_ms;
                true
            }
            _ => false,
        }
    };
    if changed {
        publish(app);
    }
    changed
}

/// The producer has spoken the reminder: the stage shows the alert chip, then the entry goes. `alert` is the line that
/// was actually spoken (a trip's wording depends on how late it is); `None` keeps the event's own alert line.
pub fn fired<R: Runtime>(app: &AppHandle<R>, id: &str, alert: Option<&str>) -> Option<ReminderEvent> {
    let ev = remove(&mut ACTIVE.lock().unwrap_or_else(|e| e.into_inner()), id)?;
    let line = alert.map(str::to_string).unwrap_or_else(|| ev.alert.clone());
    crate::commands::emit_logged(app, "reminder:fire", Fired { id: ev.id.clone(), title: ev.title.clone(), alert: line });
    publish(app);
    Some(ev)
}

/// The reminder is no longer wanted (replaced, cancelled, or its source vanished): remove the chip, no alert.
pub fn cancel<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    let removed = remove(&mut ACTIVE.lock().unwrap_or_else(|e| e.into_inner()), id).is_some();
    if removed {
        publish(app);
    }
    removed
}

/// A moved time is worth updating only when it moved by a minute or more. Pure.
pub fn moved_enough(old_ms: i64, new_ms: i64) -> bool {
    (new_ms - old_ms).abs() >= 60_000
}

/// Keep a chip for something that was not "just set" by the user (a timetable slot coming up, a calendar leave-by): show
/// it with no roll animation, and move it only when its time really moved. Called every tick by the producer, so it
/// publishes nothing when nothing changed.
pub fn track<R: Runtime>(app: &AppHandle<R>, ev: ReminderEvent) {
    let existing = ACTIVE.lock().unwrap_or_else(|e| e.into_inner()).iter().find(|e| e.id == ev.id).map(|e| e.at_ms);
    match existing {
        None => {
            if let Err(e) = restore(app, ev) {
                tracing::debug!("reminder: not tracked: {e}");
            }
        }
        Some(old) if moved_enough(old, ev.at_ms) => {
            retime(app, &ev.id, ev.at_ms);
        }
        Some(_) => {}
    }
}

/// The time has come for something that may or may not be showing as a chip: show the alert chip with the line that was
/// actually spoken, then drop the entry.
pub fn fire_now<R: Runtime>(app: &AppHandle<R>, ev: ReminderEvent, line: &str) {
    if let Err(e) = validate(&ev, now_ms(), false) {
        tracing::debug!("reminder: no alert chip: {e}");
        return;
    }
    {
        let mut g = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
        upsert(&mut g, ev.clone());
    }
    fired(app, &ev.id, Some(line));
}

/// IPC: the current list, for a stage that mounted after a reminder was set.
#[tauri::command]
pub fn reminder_list() -> Vec<ReminderEvent> {
    snapshot()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn ev(id: &str, at: i64) -> ReminderEvent {
        ReminderEvent {
            id: id.into(),
            kind: "trip".into(),
            title: "Leave for the airport".into(),
            at_ms: at,
            spoken: "Sir, I will remind you at 5:00 PM.".into(),
            alert: "Time to leave for the airport, sir.".into(),
            source: "directions".into(),
            display: Display::default(),
        }
    }

    #[test]
    fn a_good_event_is_accepted_and_every_bad_one_is_named() {
        assert_eq!(validate(&ev("a", NOW + 1000), NOW, true), Ok(()));
        assert!(validate(&ev("", NOW + 1000), NOW, true).is_err());
        assert!(validate(&ev(&"x".repeat(65), NOW + 1000), NOW, true).is_err());
        let mut bad = ev("a", NOW + 1000);
        bad.kind = "birthday".into();
        assert_eq!(validate(&bad, NOW, true), Err("unknown reminder kind"));
        bad = ev("a", NOW + 1000);
        bad.title = "   ".into();
        assert!(validate(&bad, NOW, true).is_err());
        bad.title = "x".repeat(61);
        assert!(validate(&bad, NOW, true).is_err());
        bad = ev("a", NOW + 1000);
        bad.alert = "x".repeat(241);
        assert!(validate(&bad, NOW, true).is_err());
        // a new reminder must be in the future; one being restored may be a little late but not long past
        assert!(validate(&ev("a", NOW - 1), NOW, true).is_err());
        assert!(validate(&ev("a", NOW), NOW, true).is_err());
        assert_eq!(validate(&ev("a", NOW - 60_000), NOW, false), Ok(()));
        assert!(validate(&ev("a", NOW - KEEP_AFTER_MS), NOW, false).is_err());
    }

    #[test]
    fn the_kind_is_only_a_label_every_listed_kind_is_valid() {
        for k in KINDS {
            let mut e = ev("a", NOW + 1000);
            e.kind = (*k).into();
            assert_eq!(validate(&e, NOW, true), Ok(()), "{k}");
        }
    }

    #[test]
    fn replacing_by_id_keeps_one_entry_and_the_list_is_capped() {
        let mut l = vec![];
        upsert(&mut l, ev("a", NOW + 1));
        upsert(&mut l, ev("a", NOW + 2));
        assert_eq!((l.len(), l[0].at_ms), (1, NOW + 2));
        for n in 0..40 {
            upsert(&mut l, ev(&format!("r{n}"), NOW + 10 + n));
        }
        assert_eq!(l.len(), MAX_ACTIVE);
        assert!(l.iter().all(|e| e.id != "a"), "the oldest entry is the one that goes");
    }

    #[test]
    fn a_time_that_moved_less_than_a_minute_is_not_worth_an_update() {
        assert!(!moved_enough(NOW, NOW));
        assert!(!moved_enough(NOW, NOW + 59_999));
        assert!(!moved_enough(NOW + 59_999, NOW));
        assert!(moved_enough(NOW, NOW + 60_000));
        assert!(moved_enough(NOW + 120_000, NOW));
    }

    #[test]
    fn remove_and_prune() {
        let mut l = vec![ev("a", NOW + 1), ev("b", NOW + 2)];
        assert_eq!(remove(&mut l, "a").map(|e| e.id), Some("a".to_string()));
        assert_eq!(remove(&mut l, "a"), None);
        l.push(ev("old", NOW - KEEP_AFTER_MS - 1));
        l.push(ev("late", NOW - 60_000));
        prune(&mut l, NOW);
        let ids: Vec<&str> = l.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["b", "late"], "long-past entries go, a slightly late one stays");
    }

    #[test]
    fn the_event_has_the_camel_case_shape_the_stage_reads() {
        let v = serde_json::to_value(ev("rem_1", NOW + 5)).unwrap();
        for k in ["id", "kind", "title", "atMs", "spoken", "alert", "source", "display"] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
        assert_eq!(v["display"]["countdown"], true);
        assert!(v.get("at_ms").is_none());
    }

    #[test]
    fn a_producer_that_sends_no_display_gets_the_countdown() {
        let raw = r#"{"id":"x","kind":"timer","title":"Tea","atMs":5,"spoken":"s","alert":"a","source":"timer"}"#;
        let e: ReminderEvent = serde_json::from_str(raw).unwrap();
        assert!(e.display.countdown);
        let off: ReminderEvent = serde_json::from_str(r#"{"id":"x","kind":"timer","title":"Tea","atMs":5,"spoken":"s","alert":"a","source":"timer","display":{"countdown":false}}"#).unwrap();
        assert!(!off.display.countdown);
        let empty: ReminderEvent = serde_json::from_str(r#"{"id":"x","kind":"timer","title":"Tea","atMs":5,"spoken":"s","alert":"a","source":"timer","display":{}}"#).unwrap();
        assert!(empty.display.countdown, "an empty display object still means the default");
    }
}
