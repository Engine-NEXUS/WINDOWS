//! Self-learning from the user's own behaviour — local, deterministic, no LLM.
//!
//! Every successful local command (open an app, open a site, media control …)
//! bumps a counter in `memory/habits.json`: how often, when (hour-of-day
//! histogram) and last seen. From those counters NEXUS derives *suggestions*
//! ("you open VS Code most mornings — make coding a priority?") that the user
//! explicitly confirms or dismisses in the Memory panel. Nothing here writes
//! to the profile on its own, and nothing from this file is ever put in a
//! cloud prompt — only confirmed profile entries are.
//!
//! What is deliberately NOT recorded: dictated text, search queries, message
//! bodies, contact names (see `orchestrator::local_turn_summary` for the
//! whitelist of intents that are counted).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Timelike;
use serde::{Deserialize, Serialize};

const MAX_KEYS: usize = 200;
/// A habit needs this many observations before it is worth suggesting.
pub const MIN_COUNT_FOR_SUGGESTION: u32 = 3;
const MAX_SUGGESTIONS: usize = 3;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HabitEntry {
    pub count: u32,
    pub last_ts: i64,
    /// 24-bucket hour-of-day histogram (local time).
    #[serde(default)]
    pub hours: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HabitsFile {
    #[serde(default)]
    pub entries: BTreeMap<String, HabitEntry>,
    /// Suggestion keys the user dismissed — never suggested again.
    #[serde(default)]
    pub dismissed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HabitSuggestion {
    /// Habit key it came from ("app:vs code").
    pub key: String,
    /// Ready-to-save priority text.
    pub text: String,
    /// Why NEXUS is suggesting it (shown in the Memory panel).
    pub reason: String,
}

fn habits_path(app_data_dir: &Path) -> PathBuf {
    crate::memory::resolve_memory_dir(app_data_dir).join("habits.json")
}

pub fn load(app_data_dir: &Path) -> HabitsFile {
    serde_json::from_value(crate::memory::read_json(&habits_path(app_data_dir))).unwrap_or_default()
}

fn save(app_data_dir: &Path, file: &HabitsFile) -> bool {
    match serde_json::to_value(file) {
        Ok(v) => crate::memory::write_json(&habits_path(app_data_dir), &v),
        Err(_) => false,
    }
}

/// Record one observation at an explicit time (pure → testable).
pub fn record_at(app_data_dir: &Path, key: &str, hour: u32, now_ts: i64) {
    let key = key.trim().to_lowercase();
    if key.is_empty() || key.chars().count() > 80 {
        return;
    }
    let mut file = {
        let v = crate::memory::read_json_for_update(&habits_path(app_data_dir));
        serde_json::from_value::<HabitsFile>(v).unwrap_or_default()
    };
    let entry = file.entries.entry(key).or_default();
    entry.count = entry.count.saturating_add(1);
    entry.last_ts = now_ts;
    if entry.hours.len() != 24 {
        entry.hours = vec![0; 24];
    }
    entry.hours[(hour % 24) as usize] += 1;

    // Bound the file: drop the least-used, least-recent keys.
    if file.entries.len() > MAX_KEYS {
        let mut ranked: Vec<(String, u32, i64)> = file
            .entries
            .iter()
            .map(|(k, e)| (k.clone(), e.count, e.last_ts))
            .collect();
        ranked.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
        let excess = file.entries.len() - MAX_KEYS;
        for (k, _, _) in ranked.into_iter().take(excess) {
            file.entries.remove(&k);
        }
    }
    save(app_data_dir, &file);
}

/// Record one observation now (local time).
pub fn record_now(app_data_dir: &Path, key: &str) {
    let now = chrono::Local::now();
    record_at(app_data_dir, key, now.hour(), now.timestamp());
}

/// Never suggest this habit again.
pub fn dismiss(app_data_dir: &Path, key: &str) -> bool {
    let mut file = load(app_data_dir);
    let key = key.trim().to_lowercase();
    if !file.dismissed.contains(&key) {
        file.dismissed.push(key);
    }
    save(app_data_dir, &file)
}

/// Delete everything learned (privacy "clear everything").
pub fn clear_all(app_data_dir: &Path) -> bool {
    let path = habits_path(app_data_dir);
    !path.exists() || std::fs::remove_file(path).is_ok()
}

/// "9 AM", "12 PM", "11 PM".
pub fn hour_label(hour: u32) -> String {
    let h = hour % 24;
    let (n, suffix) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{n} {suffix}")
}

fn peak_hour(entry: &HabitEntry) -> Option<u32> {
    if entry.hours.len() != 24 {
        return None;
    }
    let (idx, max) = entry
        .hours
        .iter()
        .enumerate()
        .max_by_key(|(i, c)| (**c, std::cmp::Reverse(*i)))?;
    if *max == 0 {
        None
    } else {
        Some(idx as u32)
    }
}

fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Derive up to 3 suggestions from the most-used apps/sites. `existing` is the
/// current priority text — a habit already mentioned there isn't suggested.
pub fn suggestions_from(file: &HabitsFile, existing_priorities: &[String]) -> Vec<HabitSuggestion> {
    let existing: Vec<String> = existing_priorities.iter().map(|p| p.to_lowercase()).collect();
    let mut ranked: Vec<(&String, &HabitEntry)> = file
        .entries
        .iter()
        .filter(|(k, e)| {
            (k.starts_with("app:") || k.starts_with("site:"))
                && e.count >= MIN_COUNT_FOR_SUGGESTION
                && !file.dismissed.contains(*k)
        })
        .collect();
    ranked.sort_by(|a, b| b.1.count.cmp(&a.1.count).then(b.1.last_ts.cmp(&a.1.last_ts)));

    let mut out = Vec::new();
    for (key, entry) in ranked {
        let name = key.split_once(':').map(|(_, n)| n).unwrap_or(key);
        if existing.iter().any(|p| p.contains(&name.to_lowercase())) {
            continue;
        }
        let shown = title_case(name);
        let when = peak_hour(entry)
            .map(|h| format!(", usually around {}", hour_label(h)))
            .unwrap_or_default();
        out.push(HabitSuggestion {
            key: key.clone(),
            text: format!("Make progress in {shown}"),
            reason: format!("You've opened {shown} {} times{when}.", entry.count),
        });
        if out.len() >= MAX_SUGGESTIONS {
            break;
        }
    }
    out
}

/// Suggestions for the Memory panel (reads habits + current priorities).
pub fn suggestions(app_data_dir: &Path) -> Vec<HabitSuggestion> {
    let existing: Vec<String> = crate::profile::load(app_data_dir)
        .priorities
        .into_iter()
        .map(|e| e.text)
        .collect();
    suggestions_from(&load(app_data_dir), &existing)
}

/// "Most used" list for the panel: (display name, count), top `n`.
pub fn top_used(file: &HabitsFile, n: usize) -> Vec<(String, u32)> {
    let mut v: Vec<(String, u32)> = file
        .entries
        .iter()
        .filter(|(k, _)| k.starts_with("app:") || k.starts_with("site:"))
        .map(|(k, e)| (title_case(k.split_once(':').map(|(_, n)| n).unwrap_or(k)), e.count))
        .collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.truncate(n);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_habits_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn test_hour_label() {
        assert_eq!(hour_label(0), "12 AM");
        assert_eq!(hour_label(9), "9 AM");
        assert_eq!(hour_label(12), "12 PM");
        assert_eq!(hour_label(23), "11 PM");
    }

    #[test]
    fn test_record_counts_and_hour_histogram() {
        let d = tmpdir("rec");
        for _ in 0..3 {
            record_at(&d, "App:VS Code", 9, 1000);
        }
        record_at(&d, "app:vs code", 21, 2000);
        let f = load(&d);
        let e = &f.entries["app:vs code"];
        assert_eq!(e.count, 4);
        assert_eq!(e.last_ts, 2000);
        assert_eq!(e.hours.len(), 24);
        assert_eq!(e.hours[9], 3);
        assert_eq!(e.hours[21], 1);
        assert_eq!(peak_hour(e), Some(9));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_suggestions_threshold_dismissal_and_dedupe() {
        let d = tmpdir("sug");
        for _ in 0..5 {
            record_at(&d, "app:vs code", 9, 10);
        }
        for _ in 0..2 {
            record_at(&d, "app:notepad", 9, 10); // below threshold
        }
        for _ in 0..4 {
            record_at(&d, "site:youtube", 20, 11);
        }
        record_at(&d, "media", 8, 12); // not an app/site habit
        let s = suggestions_from(&load(&d), &[]);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(s[0].key, "app:vs code");
        assert!(s[0].reason.contains("5 times") && s[0].reason.contains("9 AM"), "{:?}", s[0]);
        assert_eq!(s[1].key, "site:youtube");

        // Already a priority → not suggested again.
        let s = suggestions_from(&load(&d), &["finish the vs code extension".to_string()]);
        assert!(!s.iter().any(|x| x.key == "app:vs code"));

        // Dismissed → gone for good.
        assert!(dismiss(&d, "site:youtube"));
        let s = suggestions_from(&load(&d), &[]);
        assert!(!s.iter().any(|x| x.key == "site:youtube"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_file_is_bounded() {
        let d = tmpdir("cap");
        for i in 0..(MAX_KEYS + 25) {
            record_at(&d, &format!("app:app{i}"), 1, i as i64);
        }
        // Make one clearly the most used.
        for _ in 0..5 {
            record_at(&d, "app:favorite", 1, 99_999);
        }
        let f = load(&d);
        assert!(f.entries.len() <= MAX_KEYS);
        assert!(f.entries.contains_key("app:favorite"), "high-count habits survive pruning");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_top_used_and_clear() {
        let d = tmpdir("top");
        for _ in 0..3 {
            record_at(&d, "app:chrome", 10, 1);
        }
        record_at(&d, "app:spotify", 10, 1);
        let top = top_used(&load(&d), 5);
        assert_eq!(top[0], ("Chrome".to_string(), 3));
        assert!(clear_all(&d));
        assert!(load(&d).entries.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_blank_or_huge_keys_ignored() {
        let d = tmpdir("bad");
        record_at(&d, "  ", 1, 1);
        record_at(&d, &"x".repeat(200), 1, 1);
        assert!(load(&d).entries.is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }
}
