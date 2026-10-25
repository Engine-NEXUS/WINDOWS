//! What the agents had done when NEXUS powered the laptop off (doc 146, phase P4).
//!
//! "Shut down after Antigravity" means "I'm done for today": the laptop powers off however the agents ended. So that
//! nothing is lost with it, the result is kept (in the sealed memory store, not in a file) and the next morning's
//! briefing says it before anything else: "Before the shutdown, Antigravity stopped, but the output mentions a failure
//! (3 failing). Claude Code finished clean." Told once, and only while it is fresh.

use crate::agent_tracker::{outcome_line, Outcome};
use crate::session_verdict::Verdict;
use serde::{Deserialize, Serialize};

const KEY: &str = "agent_overnight";
/// An older record is not "last night" any more.
pub const MAX_AGE_SECS: i64 = 36 * 3600;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Unix seconds when the power-off was started.
    pub at: i64,
    /// "shut down", "restarted", ...
    pub what: String,
    /// One sentence fragment per agent ("Claude Code finished clean").
    pub lines: Vec<String>,
    /// Every agent finished clean.
    pub clean: bool,
    pub told: bool,
}

/// The record for a power-off now. `None` when no agent was being followed (nothing to remember).
pub fn record_from(outcomes: &[Outcome], what: &str, now: i64) -> Option<Record> {
    if outcomes.is_empty() {
        return None;
    }
    Some(Record {
        at: now,
        what: what.to_string(),
        lines: outcomes.iter().map(outcome_line).collect(),
        clean: outcomes.iter().all(|o| o.verdict == Verdict::Go),
        told: false,
    })
}

/// A record that is still worth telling: not told yet, and from the last day and a half.
pub fn fresh(rec: &Record, now: i64) -> bool {
    !rec.told && now - rec.at >= 0 && now - rec.at <= MAX_AGE_SECS
}

/// The sentence(s) the briefing speaks first.
pub fn spoken(rec: &Record) -> String {
    format!("Before the laptop went off, {}.", rec.lines.join(". "))
}

pub fn parse(json: &str) -> Option<Record> {
    serde_json::from_str(json).ok()
}

pub fn save(dir: &std::path::Path, rec: &Record) {
    if let Ok(json) = serde_json::to_string(rec) {
        crate::memcore::with_store(dir, |s| s.meta_set(KEY, &json));
    }
}

/// The untold, fresh record, if any (it stays untold until `mark_told`).
pub fn peek(dir: &std::path::Path, now: i64) -> Option<Record> {
    crate::memcore::with_store(dir, |s| s.meta_get(KEY)).flatten().and_then(|j| parse(&j)).filter(|r| fresh(r, now))
}

pub fn mark_told(dir: &std::path::Path) {
    crate::memcore::with_store(dir, |s| {
        if let Some(mut rec) = s.meta_get(KEY).and_then(|j| parse(&j)) {
            rec.told = true;
            if let Ok(json) = serde_json::to_string(&rec) {
                s.meta_set(KEY, &json);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_verdict::Reason;

    fn fail() -> Verdict {
        Verdict::Hold(vec![Reason::FailureMarker("3 failing".into(), "Fix 3 failing tests".into())])
    }

    #[test]
    fn the_record_keeps_how_each_agent_ended_and_nothing_is_kept_for_nobody() {
        assert!(record_from(&[], "shut down", 100).is_none());
        let r = record_from(
            &[Outcome { label: "Claude Code".into(), verdict: Verdict::Go }, Outcome { label: "Antigravity".into(), verdict: fail() }],
            "shut down",
            1_000,
        )
        .unwrap();
        assert!(!r.clean && !r.told && r.at == 1_000);
        assert_eq!(r.lines.len(), 2);
        let s = spoken(&r);
        assert!(s.starts_with("Before the laptop went off, Claude Code finished clean. Antigravity stopped, but the output mentions a failure (3 failing)."), "{s}");
        let clean = record_from(&[Outcome { label: "Codex".into(), verdict: Verdict::Go }], "shut down", 5).unwrap();
        assert!(clean.clean);
        assert_eq!(spoken(&clean), "Before the laptop went off, Codex finished clean.");
    }

    #[test]
    fn it_is_told_while_fresh_and_once_and_survives_a_round_trip() {
        let r = record_from(&[Outcome { label: "Codex".into(), verdict: Verdict::Go }], "shut down", 10_000).unwrap();
        assert!(fresh(&r, 10_000 + 8 * 3600));
        assert!(fresh(&r, 10_000 + MAX_AGE_SECS));
        assert!(!fresh(&r, 10_000 + MAX_AGE_SECS + 1), "older than a day and a half");
        assert!(!fresh(&r, 9_999), "from the future: a clock error is not 'last night'");
        let mut told = r.clone();
        told.told = true;
        assert!(!fresh(&told, 10_100));
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(parse(&json), Some(r));
        assert_eq!(parse("not json"), None);
        assert_eq!(parse(r#"{"at":1}"#), None, "a partial record is ignored");
    }

    static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct DirGuard(std::path::PathBuf);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tmpdir(name: &str) -> (std::path::PathBuf, DirGuard) {
        let p = std::env::temp_dir().join(format!("nexus_outcomes_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        let guard = DirGuard(p.clone());
        (p, guard)
    }

    #[test]
    fn the_record_survives_a_restart_and_is_told_exactly_once() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (dir, _guard) = tmpdir("restart");
        let now = 1_700_000_000;
        let outcomes = vec![
            Outcome { label: "Claude Code".into(), verdict: Verdict::Go },
            Outcome { label: "Antigravity".into(), verdict: fail() },
            Outcome { label: "Cursor".into(), verdict: Verdict::Unsure(vec![Reason::NothingToJudge]) },
        ];
        let rec = record_from(&outcomes, "shut down", now).expect("record from outcomes");
        save(&dir, &rec);

        // Drop in-memory handles before peek, simulating restart boot flow
        drop(rec);
        drop(outcomes);

        let loaded = peek(&dir, now + 60).expect("record returned by peek");
        assert_eq!(loaded.lines.len(), 3);
        assert_eq!(loaded.lines[0], "Claude Code finished clean");
        assert_eq!(loaded.lines[1], "Antigravity stopped, but the output mentions a failure (3 failing)");
        assert_eq!(loaded.lines[2], "Cursor may be done, but I could not read its final output");
        assert_eq!(loaded.what, "shut down");
        assert_eq!(loaded.at, now);
        assert!(!loaded.clean);
        assert!(!loaded.told);

        let spoken_text = spoken(&loaded);
        assert!(spoken_text.contains("Claude Code"));
        assert!(spoken_text.contains("Antigravity"));
        assert!(spoken_text.contains("Cursor"));
        assert!(spoken_text.contains("3 failing"));
        assert_eq!(
            spoken_text,
            "Before the laptop went off, Claude Code finished clean. Antigravity stopped, but the output mentions a failure (3 failing). Cursor may be done, but I could not read its final output."
        );

        mark_told(&dir);
        assert!(peek(&dir, now + 120).is_none(), "told record must not be peeked again");

        // Save a new record later and verify told state resets for the new record
        let later_now = now + 3600;
        let new_rec = record_from(
            &[Outcome { label: "Codex".into(), verdict: Verdict::Go }],
            "restarted",
            later_now,
        )
        .expect("new record");
        save(&dir, &new_rec);
        drop(new_rec);

        let loaded_new = peek(&dir, later_now + 30).expect("new record returned by peek");
        assert_eq!(loaded_new.what, "restarted");
        assert_eq!(loaded_new.lines, vec!["Codex finished clean"]);
        assert!(loaded_new.clean);
        assert!(!loaded_new.told);
        assert_eq!(loaded_new.at, later_now);
    }

    #[test]
    fn an_old_record_is_not_told() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (dir, _guard) = tmpdir("old");
        let now = 1_000_000;
        let rec = record_from(
            &[Outcome { label: "Claude Code".into(), verdict: Verdict::Go }],
            "shut down",
            now,
        )
        .expect("record");
        save(&dir, &rec);

        // One second inside the maximum age limit: record is returned
        assert!(peek(&dir, now + MAX_AGE_SECS - 1).is_some(), "one second inside max age must be returned");
        assert!(peek(&dir, now + MAX_AGE_SECS).is_some(), "exactly at max age must be returned");

        // Older than the maximum age limit: record is not returned
        assert!(peek(&dir, now + MAX_AGE_SECS + 1).is_none(), "older than max age must not be returned");
    }

    #[test]
    fn a_corrupt_record_is_ignored_and_does_not_block_the_next_one() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (dir, _guard) = tmpdir("corrupt");
        let now = 1_000_000;

        // Write corrupt garbage where the record is stored using module's storage key
        crate::memcore::with_store(&dir, |s| s.meta_set(KEY, "{ not valid json @@@"));

        // peek returns None without panicking
        assert!(peek(&dir, now).is_none());

        // A following save of a good record is readable
        let good = record_from(
            &[Outcome { label: "Claude Code".into(), verdict: Verdict::Go }],
            "shut down",
            now,
        )
        .expect("good record");
        save(&dir, &good);

        let read = peek(&dir, now + 10).expect("saved record should be readable");
        assert_eq!(read, good);
    }

    #[test]
    fn no_outcomes_means_no_record() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let (dir, _guard) = tmpdir("none");
        let now = 1_000_000;

        let rec = record_from(&[], "shut down", now);
        assert!(rec.is_none(), "no outcomes must produce None record");

        // Saving nothing leaves peek None
        assert!(peek(&dir, now).is_none(), "peek must return None when nothing was saved");
    }
}
