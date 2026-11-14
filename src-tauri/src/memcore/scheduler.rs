//! Slot reminders (plan P4): every 30 s, fire the reminder for any timetable
//! slot whose start time has arrived and that has not fired today.
//!
//! Delivery goes through `proactive_policy::submit`, so a reminder waits out
//! meetings and speech instead of cutting in. "Fired today" is stored in the
//! sealed store, so a restart inside the 10-minute grace window does not
//! repeat it. Activities with something to open ask "Shall I start?" and
//! register a pending offer; the rest ("Gym", "Lunch") are only announced.

use tauri::{AppHandle, Manager, Runtime};

use super::offer::{self, Offer};
use super::timetable::{self, Slot};

pub const TICK_SECS: u64 = 30;
/// "Later" re-asks after this long.
pub const SNOOZE_SECS: u64 = 10 * 60;

/// What the reminder says. Pure.
pub fn reminder_text(title: &str, asks_to_start: bool, address: Option<&str>) -> String {
    let who = address.map(|a| format!(", {a}")).unwrap_or_default();
    if asks_to_start {
        format!("It's time for {title}{who}. Shall I start?")
    } else {
        format!("It's time for {title}{who}.")
    }
}

/// Slots start this soon get a countdown chip beside the orb; earlier than that a daily slot would be a chip all day.
pub const LOOKAHEAD_MIN: u16 = 30;

fn chip_id(slot_id: &str) -> String {
    format!("slot_{slot_id}").chars().take(64).collect()
}

/// The reminder the stage shows for a slot: kind "study", at the slot's start. Pure.
pub fn slot_event(slot: &Slot, at_ms: i64) -> crate::reminder::ReminderEvent {
    let title: String = slot.title.chars().take(crate::reminder::MAX_TITLE_CHARS).collect();
    crate::reminder::ReminderEvent {
        id: chip_id(&slot.id),
        kind: "study".to_string(),
        title: title.clone(),
        at_ms,
        spoken: String::new(),
        alert: reminder_text(&title, false, None),
        source: "timetable".to_string(),
        display: crate::reminder::Display::default(),
    }
}

/// Slots starting within `LOOKAHEAD_MIN` minutes today that have not fired: the chips to show. `midnight_ms` is today's
/// local midnight in epoch milliseconds. Pure.
pub fn lookahead_events(
    slots: &[Slot],
    weekday0: u8,
    minute_now: u16,
    midnight_ms: i64,
    fired_today: &dyn Fn(&str) -> bool,
) -> Vec<crate::reminder::ReminderEvent> {
    slots
        .iter()
        .filter(|s| s.days.contains(&weekday0) && s.start_min > minute_now && s.start_min - minute_now <= LOOKAHEAD_MIN && !fired_today(&s.id))
        .map(|s| slot_event(s, midnight_ms + s.start_min as i64 * 60_000))
        .collect()
}

/// Fire one slot's reminder now.
pub fn fire<R: Runtime>(app: &AppHandle<R>, slot: &Slot) {
    let activity = timetable::activity_key(&slot.title);
    let asks = timetable::plan_sites(&activity).is_some();
    let friend = crate::persona::is_friend(&crate::commands::read_persona_mode(app));
    let name = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| crate::memory::read_user_profile(&d))
        .and_then(|p| p.name);
    let address = crate::persona::address(name.as_deref(), friend);
    let text = reminder_text(&slot.title, asks, address.as_deref());
    // The alert chip on the stage (the countdown chip may or may not have been showing already).
    crate::reminder::fire_now(app, slot_event(slot, chrono::Utc::now().timestamp_millis()), &text);
    if asks {
        // Not armed until the question is actually spoken (see speak_line).
        offer::set(Offer::Start { title: slot.title.clone(), activity }, false);
        crate::proactive_policy::submit(
            app,
            format!("offer_slot_{}", slot.id),
            text,
            crate::google::types::AlertUrgency::Medium,
        );
    } else {
        crate::proactive_policy::submit(
            app,
            format!("slot_{}", slot.id),
            text,
            crate::google::types::AlertUrgency::Medium,
        );
    }
}

/// "Later": ask again in ten minutes (the slot stays fired for today).
pub fn snooze_start<R: Runtime>(app: &AppHandle<R>, title: String, activity: String) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(SNOOZE_SECS)).await;
        let friend = crate::persona::is_friend(&crate::commands::read_persona_mode(&app));
        let address = crate::persona::address(None, friend).filter(|_| !friend);
        let text = reminder_text(&title, true, address.as_deref());
        offer::set(Offer::Start { title: title.clone(), activity }, false);
        crate::proactive_policy::submit(
            &app,
            format!("offer_snooze_{}", timetable::activity_key(&title)),
            text,
            crate::google::types::AlertUrgency::Medium,
        );
    });
}

/// Start the 30 s scheduler. Cheap no-op without slots.
pub fn start<R: Runtime>(app: AppHandle<R>) {
    use chrono::{Datelike, Timelike};
    tauri::async_runtime::spawn(async move {
        let mut chips: Vec<String> = vec![];
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
            let Ok(dir) = app.path().app_data_dir() else { continue };
            if !super::enabled(&dir) || !super::flag(&dir, "memcoreTimetable", true) {
                for id in std::mem::take(&mut chips) {
                    crate::reminder::cancel(&app, &id);
                }
                continue;
            }
            let now = chrono::Local::now();
            let weekday0 = now.weekday().num_days_from_monday() as u8;
            let minute = (now.hour() * 60 + now.minute()) as u16;
            let today = now.format("%Y-%m-%d").to_string();
            let midnight_ms = now
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .and_then(|d| d.and_local_timezone(chrono::Local).earliest())
                .map(|d| d.timestamp_millis())
                .unwrap_or(0);
            let (due, coming): (Vec<Slot>, Vec<crate::reminder::ReminderEvent>) = super::with_store(&dir, |s| {
                let slots = timetable::load_slots(s);
                if slots.is_empty() {
                    return (vec![], vec![]);
                }
                let coming = lookahead_events(&slots, weekday0, minute, midnight_ms, &|id| {
                    s.meta_get(&timetable::fired_key(id)).as_deref() == Some(today.as_str())
                });
                let due: Vec<Slot> = timetable::due(&slots, weekday0, minute, &|id| {
                    s.meta_get(&timetable::fired_key(id)).as_deref() == Some(today.as_str())
                })
                .into_iter()
                .cloned()
                .collect();
                for d in &due {
                    s.meta_set(&timetable::fired_key(&d.id), &today);
                }
                (due, coming)
            })
            .unwrap_or_default();
            // chips for what starts soon; one that is no longer coming (fired, deleted) loses its chip
            for ev in &coming {
                crate::reminder::track(&app, ev.clone());
            }
            for id in std::mem::take(&mut chips) {
                if !coming.iter().any(|e| e.id == id) {
                    crate::reminder::cancel(&app, &id);
                }
            }
            chips = coming.iter().map(|e| e.id.clone()).collect();
            for slot in due {
                tracing::info!("timetable: reminder for '{}'", slot.title);
                fire(&app, &slot);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: &str, title: &str, days: &[u8], start: u16) -> Slot {
        Slot { id: id.into(), title: title.into(), days: days.to_vec(), start_min: start, end_min: None, section: None }
    }

    #[test]
    fn a_slot_gets_a_chip_only_in_the_half_hour_before_it_starts() {
        let slots = vec![slot("a", "DSA", &[0, 2], 18 * 60), slot("b", "Gym", &[0], 6 * 60), slot("c", "Maths", &[1], 18 * 60)];
        let none = |_: &str| false;
        let mid = 1_800_000_000_000i64;
        assert!(lookahead_events(&slots, 0, 17 * 60 + 29, mid, &none).is_empty(), "31 minutes before is too early");
        let c = lookahead_events(&slots, 0, 17 * 60 + 30, mid, &none);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].id.as_str(), c[0].kind.as_str(), c[0].title.as_str(), c[0].at_ms), ("slot_a", "study", "DSA", mid + 18 * 3_600_000));
        assert_eq!(c[0].alert, "It's time for DSA.");
        assert_eq!(c[0].source, "timetable");
        assert_eq!(crate::reminder::validate(&c[0], mid, true), Ok(()));
        assert!(lookahead_events(&slots, 0, 18 * 60, mid, &none).is_empty(), "the minute it starts the reminder speaks instead");
        assert!(lookahead_events(&slots, 3, 17 * 60 + 40, mid, &none).is_empty(), "another weekday");
        assert!(lookahead_events(&slots, 0, 17 * 60 + 40, mid, &|id| id == "a").is_empty(), "already fired today");
    }

    #[test]
    fn a_long_slot_title_is_cut_for_the_chip_never_refused() {
        let s = slot(&"x".repeat(80), &"y".repeat(100), &[0], 600);
        let e = slot_event(&s, 1_800_000_000_000);
        assert_eq!((e.id.chars().count(), e.title.chars().count()), (64, 60));
    }

    #[test]
    fn reminder_wording() {
        assert_eq!(reminder_text("DSA", true, Some("sir")), "It's time for DSA, sir. Shall I start?");
        assert_eq!(reminder_text("Gym", false, Some("sir")), "It's time for Gym, sir.");
        assert_eq!(reminder_text("DSA", true, None), "It's time for DSA. Shall I start?");
        assert_eq!(reminder_text("DSA", true, Some("Lakshya")), "It's time for DSA, Lakshya. Shall I start?");
    }
}
