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
}
