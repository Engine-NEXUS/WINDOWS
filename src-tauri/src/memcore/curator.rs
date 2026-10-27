//! Idle upkeep of what NEXUS knows (Memory v2, phase M5; the "sleep-time" idea from Letta, the
//! bounded-memory discipline from Hermes Agent). Deterministic, no model, no network, never on the
//! hot path: it runs at most once a day, only while the user is away from the PC and NEXUS is idle.
//!
//! * **Profile choice when full.** The always-on profile has a fixed size. While everything fits,
//!   nothing changes (oldest first, the text stays identical so provider prompt caches keep hitting).
//!   Only when facts no longer fit does the curator rank them by how often a question actually
//!   touched them (`uses`) and store that order, so the facts you ask about stay in and the ones
//!   nothing ever touches drop out. The order is recomputed here only, so the prompt prefix changes
//!   at most once a day.
//! * **Unused routines are retired.** A routine neither run nor created for 60 days is removed; the
//!   Memory page says so for two weeks ("Removed the routine ..."). They are cheap to recreate and a
//!   stale one only adds noise.
//! * **Housekeeping.** Old "refused suggestion" markers and old routine-offer counters are dropped,
//!   the usual retention/decay pass runs.
//!
//! It never deletes a fact the user told NEXUS, and never changes a fact's text.

use std::path::Path;

use super::profile::{self, Entry};
use super::store::Store;

pub const RETIRE_AFTER_SECS: i64 = 60 * 24 * 3600;
const MIN_GAP_SECS: i64 = 20 * 3600;
const IDLE_SECS: u64 = 5 * 60;
const NOTE_TTL_SECS: i64 = 14 * 24 * 3600;
const META_LAST: &str = "curator_last";
const META_NOTE: &str = "curator_note";
const META_NOTE_TS: &str = "curator_note_ts";
const META_ORDER: &str = "profile_order";

// ---- Pure rules ----

/// Should the curator run now? Once a day, only while the user is away and NEXUS is idle.
pub fn should_run(now: i64, last_run: i64, idle_secs: Option<u64>, busy: bool) -> bool {
    !busy && now - last_run >= MIN_GAP_SECS && idle_secs.map_or(false, |s| s >= IDLE_SECS)
}

/// A routine nobody used (or made) for 60 days.
pub fn is_stale_routine(created: i64, last_run: i64, now: i64) -> bool {
    let touched = created.max(last_run);
    now - touched >= RETIRE_AFTER_SECS
}

/// Put `entries` in the stored usage order (ids not mentioned keep their creation order, after).
pub fn apply_order(entries: Vec<Entry>, order: &[i64]) -> Vec<Entry> {
    if order.is_empty() {
        return entries;
    }
    let mut keyed: Vec<(usize, usize, Entry)> = entries
        .into_iter()
        .enumerate()
        .map(|(i, e)| (order.iter().position(|id| *id == e.id).unwrap_or(usize::MAX), i, e))
        .collect();
    keyed.sort_by_key(|(pos, i, _)| (*pos, *i));
    keyed.into_iter().map(|(_, _, e)| e).collect()
}

/// The usage order worth storing, or `None` when creation order already keeps the same facts
/// (the common case: then the profile text must not change at all).
pub fn choose_order(entries: &[Entry], ids_by_use: &[i64], cap: usize) -> Option<Vec<i64>> {
    let (_, by_age) = profile::render(entries, cap);
    let ranked = apply_order(entries.to_vec(), ids_by_use);
    let (_, by_usage) = profile::render(&ranked, cap);
    let mut a = by_age;
    let mut b = by_usage;
    a.sort_unstable();
    b.sort_unstable();
    if a == b {
        None
    } else {
        Some(ranked.iter().map(|e| e.id).collect())
    }
}

fn parse_order(s: &str) -> Vec<i64> {
    s.split(',').filter_map(|v| v.trim().parse().ok()).collect()
}

/// The profile candidates in the order the profile block uses (single place, shared by the cloud
/// context and the background review so both see the same thing).
pub fn profile_entries(s: &Store) -> Vec<Entry> {
    let order = s.meta_get(META_ORDER).map(|v| parse_order(&v)).unwrap_or_default();
    apply_order(s.pinned_user_facts(), &order)
}

// ---- The pass ----

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub retired: Vec<String>,
    /// The stored profile order was changed (or cleared).
    pub order_changed: bool,
    pub housekeeping: usize,
}

/// One upkeep pass at time `now`.
pub fn run_pass(dir: &Path, now: i64) -> Option<Report> {
    super::with_store(dir, |s| {
        let mut report = Report::default();

        for r in s.routines_all() {
            if is_stale_routine(r.created, r.last_run, now) && s.routine_delete(&r.name, now) {
                report.retired.push(r.name);
            }
        }
        if !report.retired.is_empty() {
            let names = report.retired.iter().map(|n| format!("\"{n}\"")).collect::<Vec<_>>().join(", ");
            let noun = if report.retired.len() == 1 { "routine" } else { "routines" };
            s.meta_set(META_NOTE, &format!("Removed the {noun} {names}: not used for 60 days."));
            s.meta_set(META_NOTE_TS, &now.to_string());
        }

        let entries = s.pinned_user_facts();
        let wanted = choose_order(&entries, &s.pinned_fact_ids_by_use(), profile::PROFILE_CAP);
        let current = s.meta_get(META_ORDER).unwrap_or_default();
        let new_value = wanted
            .as_ref()
            .map(|o| o.iter().map(i64::to_string).collect::<Vec<_>>().join(","))
            .unwrap_or_default();
        if new_value != current {
            s.meta_set(META_ORDER, &new_value);
            report.order_changed = true;
        }

        report.housekeeping = s.housekeeping(now);
        s.prune(now);
        s.meta_set(META_LAST, &now.to_string());
        report
    })
}

/// The "removed a routine" notice, while it is recent.
pub fn note(dir: &Path, now: i64) -> Option<String> {
    super::with_store(dir, |s| {
        let ts: i64 = s.meta_get(META_NOTE_TS).and_then(|v| v.parse().ok())?;
        if now - ts > NOTE_TTL_SECS {
            return None;
        }
        s.meta_get(META_NOTE).filter(|n| !n.is_empty())
    })
    .flatten()
}

/// Run the upkeep daily while the user is away.
pub fn start_curator<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(600)).await;
        loop {
            if crate::focus_mode::paused() {
                crate::focus_mode::defer_log("curator");
                tokio::time::sleep(std::time::Duration::from_secs(15 * 60)).await;
                continue;
            }
            if let Ok(dir) = app.path().app_data_dir() {
                if super::enabled(&dir) {
                    let now = chrono::Utc::now().timestamp();
                    let last: i64 = super::with_store(&dir, |s| s.meta_get(META_LAST).and_then(|v| v.parse().ok()))
                        .flatten()
                        .unwrap_or(0);
                    if should_run(now, last, super::resume::user_idle_secs(), crate::orchestrator::has_active_request()) {
                        if let Some(r) = run_pass(&dir, now) {
                            println!(
                                "[CURATOR] retired {} routine(s), profile order {}, housekeeping {} row(s)",
                                r.retired.len(),
                                if r.order_changed { "updated" } else { "unchanged" },
                                r.housekeeping
                            );
                        }
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(15 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 24 * 3600;

    fn e(id: i64, key: &str, value: &str) -> Entry {
        Entry { id, key: key.into(), value: value.into() }
    }

    fn d(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_curator_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn it_runs_only_when_idle_not_busy_and_at_most_daily() {
        let now = 1_800_000_000;
        assert!(should_run(now, 0, Some(600), false));
        assert!(!should_run(now, 0, Some(60), false), "user is at the PC");
        assert!(!should_run(now, 0, None, false), "idle time unknown");
        assert!(!should_run(now, 0, Some(600), true), "NEXUS is working");
        assert!(!should_run(now, now - 3_600, Some(600), false), "ran an hour ago");
        assert!(should_run(now, now - 21 * 3_600, Some(600), false));
    }

    #[test]
    fn a_routine_is_stale_after_sixty_days_of_no_use() {
        let now = 1_800_000_000;
        assert!(is_stale_routine(now - 61 * DAY, 0, now), "never run, made 61 days ago");
        assert!(!is_stale_routine(now - 59 * DAY, 0, now));
        assert!(!is_stale_routine(now - 200 * DAY, now - 5 * DAY, now), "run recently");
        assert!(is_stale_routine(now - 200 * DAY, now - 70 * DAY, now), "last run 70 days ago");
    }

    #[test]
    fn apply_order_is_stable_and_keeps_unknown_ids_after() {
        let es = vec![e(1, "a", "x"), e(2, "b", "x"), e(3, "c", "x"), e(4, "d", "x")];
        let got: Vec<i64> = apply_order(es.clone(), &[3, 1]).iter().map(|x| x.id).collect();
        assert_eq!(got, vec![3, 1, 2, 4]);
        let same: Vec<i64> = apply_order(es, &[]).iter().map(|x| x.id).collect();
        assert_eq!(same, vec![1, 2, 3, 4]);
    }

    #[test]
    fn nothing_is_reordered_while_everything_fits() {
        let es = vec![e(1, "pet", "dog"), e(2, "employer", "Acme")];
        assert_eq!(choose_order(&es, &[2, 1], profile::PROFILE_CAP), None);
    }

    #[test]
    fn when_full_the_facts_that_get_used_replace_the_ones_that_do_not() {
        let value = "v".repeat(100);
        let es: Vec<Entry> = (1..=20).map(|i| e(i, &format!("fact_{i:02}"), &value)).collect();
        let (_, by_age) = profile::render(&es, profile::PROFILE_CAP);
        assert!(by_age.len() < 20 && !by_age.contains(&20), "premise: the newest fact does not fit");
        // fact 20 is the one people keep asking about
        let order = choose_order(&es, &[20, 19, 1, 2, 3], profile::PROFILE_CAP).expect("reorder needed");
        let ranked = apply_order(es, &order);
        let (_, kept) = profile::render(&ranked, profile::PROFILE_CAP);
        assert!(kept.contains(&20) && kept.contains(&19));
        assert_eq!(kept.len(), by_age.len(), "same size, different facts");
    }

    #[test]
    fn asking_about_a_fact_already_in_the_profile_moves_it_to_the_front_when_full() {
        let dir = d("touched");
        let now = 1_800_000_000;
        let value = "word ".repeat(20);
        for i in 1..=20 {
            let lead = match i {
                5 => "yak".to_string(),
                20 => "zebra".to_string(),
                _ => format!("n{i:02}"),
            };
            super::super::mirror_core_fact(&dir, &format!("topic_{i:02}"), &format!("{lead} {value}"));
        }
        // topic_05 is already in the profile; topic_20 is not. Both get asked about, 05 more often.
        for _ in 0..5 {
            super::super::context_pack(&dir, "yak", super::super::DEFAULT_BUDGET);
        }
        for _ in 0..3 {
            super::super::context_pack(&dir, "zebra", super::super::DEFAULT_BUDGET);
        }
        assert!(run_pass(&dir, now).unwrap().order_changed);
        let first: Vec<String> = super::super::with_store(&dir, |s| {
            profile_entries(s).into_iter().take(2).map(|e| e.key).collect()
        })
        .unwrap();
        assert_eq!(first, vec!["topic_05".to_string(), "topic_20".to_string()], "most used first");
    }

    #[test]
    fn the_pass_retires_only_unused_routines_and_leaves_a_note() {
        let dir = d("routines");
        let now = 1_800_000_000;
        let steps = vec!["open vs code".to_string(), "open chrome".to_string()];
        super::super::with_store(&dir, |s| {
            s.routine_add("old one", &steps, now - 100 * DAY).unwrap();
            s.routine_add("busy one", &steps, now - 100 * DAY).unwrap();
            s.routine_record_run("busy one", now - 3 * DAY);
            s.routine_add("new one", &steps, now - 2 * DAY).unwrap();
        })
        .unwrap();
        let r = run_pass(&dir, now).unwrap();
        assert_eq!(r.retired, vec!["old one".to_string()]);
        let left: Vec<String> =
            super::super::with_store(&dir, |s| s.routines_all().into_iter().map(|r| r.name).collect()).unwrap();
        assert_eq!(left, vec!["busy one".to_string(), "new one".to_string()]);
        let n = note(&dir, now).unwrap();
        assert!(n.contains("\"old one\"") && n.contains("60 days"), "{n}");
        assert!(note(&dir, now + 15 * DAY).is_none(), "the notice expires after two weeks");
        // a second pass has nothing left to retire and keeps the note text
        assert!(run_pass(&dir, now + 60).unwrap().retired.is_empty());
    }

    #[test]
    fn the_pass_never_touches_facts_and_a_wipe_leaves_no_curator_state_in_effect() {
        let dir = d("facts");
        let now = 1_800_000_000;
        super::super::mirror_core_fact(&dir, "employer", "Acme");
        run_pass(&dir, now).unwrap();
        let facts = super::super::recall(&dir, "employer", 5).unwrap();
        assert!(facts.iter().any(|(k, v)| k == "employer" && v == "Acme"));
    }

    #[test]
    fn a_full_profile_ends_up_showing_the_facts_that_were_asked_about() {
        let dir = d("usage");
        let now = 1_800_000_000;
        let value = "word ".repeat(20); // ~100 chars
        for i in 1..=20 {
            let lead = if i == 20 { "zebra".to_string() } else { format!("n{i:02}") };
            super::super::mirror_core_fact(&dir, &format!("topic_{i:02}"), &format!("{lead} {value}"));
        }
        let before = super::super::context_pack(&dir, "weather", super::super::DEFAULT_BUDGET).unwrap();
        assert!(!before.contains("topic_20"), "premise: the newest fact does not fit: {before}");
        // The user keeps asking about the zebra fact: relevant questions bump its usage.
        for _ in 0..3 {
            super::super::context_pack(&dir, "zebra", super::super::DEFAULT_BUDGET);
        }
        let r = run_pass(&dir, now).unwrap();
        assert!(r.order_changed);
        let after = super::super::context_pack(&dir, "weather", super::super::DEFAULT_BUDGET).unwrap();
        assert!(after.contains("topic_20"), "{after}");
        // Nothing asked about changes again -> the second pass leaves the order (and the prefix) alone.
        assert!(!run_pass(&dir, now + DAY).unwrap().order_changed);
    }
}
