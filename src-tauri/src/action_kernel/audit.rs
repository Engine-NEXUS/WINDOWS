//! Persistent audit log (docs/features/140, P0): one JSON line per action decision, local only.
//! Phrases stay on the device; nothing here is ever sent to a cloud service.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const AUDIT_FILE: &str = "action_audit.jsonl";
/// The file is rewritten without old lines when it grows past this, whatever the age.
const MAX_BYTES: u64 = 2_000_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    /// Unix seconds.
    pub ts: i64,
    pub phrase: String,
    pub tool: String,
    pub risk: String,
    pub origin: String,
    /// allow / ask / deny / confirmed / declined / expired
    pub decision: String,
    /// verified / unconfirmed / blocked / stopped / dispatched
    pub outcome: String,
    pub detail: String,
}

impl AuditEntry {
    pub fn new(ts: i64, phrase: &str, tool: &str, risk: &str, origin: &str, decision: &str, outcome: &str, detail: &str) -> Self {
        AuditEntry {
            ts,
            phrase: phrase.chars().take(200).collect(),
            tool: tool.into(),
            risk: risk.into(),
            origin: origin.into(),
            decision: decision.into(),
            outcome: outcome.into(),
            detail: detail.chars().take(600).collect(),
        }
    }
}

fn path_of(dir: &Path) -> PathBuf {
    dir.join(AUDIT_FILE)
}

/// Append one line. Failure to write the audit is logged, never fatal to the action.
pub fn record(dir: &Path, entry: &AuditEntry) {
    let Ok(line) = serde_json::to_string(entry) else { return };
    let p = path_of(dir);
    let res = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&p)
        .and_then(|mut f| writeln!(f, "{line}"));
    if let Err(e) = res {
        tracing::warn!("action audit: could not write {}: {e}", p.display());
        return;
    }
    if std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        let keep = tail(dir, 2000);
        let _ = rewrite(&p, &keep);
    }
}

fn rewrite(p: &Path, entries: &[AuditEntry]) -> std::io::Result<()> {
    let tmp = p.with_extension("jsonl.tmp");
    let mut text = String::new();
    for e in entries {
        if let Ok(l) = serde_json::to_string(e) {
            text.push_str(&l);
            text.push('\n');
        }
    }
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, p)
}

/// The newest `n` entries, oldest first. Corrupt lines are skipped.
pub fn tail(dir: &Path, n: usize) -> Vec<AuditEntry> {
    let text = std::fs::read_to_string(path_of(dir)).unwrap_or_default();
    let all: Vec<AuditEntry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    let skip = all.len().saturating_sub(n);
    all.into_iter().skip(skip).collect()
}

/// Drop lines older than `days`. Returns how many were removed.
pub fn prune(dir: &Path, now: i64, days: i64) -> usize {
    let all = tail(dir, usize::MAX);
    let cutoff = now - days * 86_400;
    let keep: Vec<AuditEntry> = all.iter().filter(|e| e.ts >= cutoff).cloned().collect();
    let removed = all.len() - keep.len();
    if removed > 0 {
        let _ = rewrite(&path_of(dir), &keep);
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::paths::test_dir;

    fn e(ts: i64, tool: &str) -> AuditEntry {
        AuditEntry::new(ts, "delete my pdfs", tool, "destructive", "user_voice", "ask", "blocked", "x")
    }

    #[test]
    fn lines_are_appended_and_read_back_in_order() {
        let d = test_dir("audit");
        record(&d, &e(1, "delete"));
        record(&d, &e(2, "move"));
        record(&d, &e(3, "copy"));
        let t = tail(&d, 2);
        assert_eq!(t.len(), 2);
        assert_eq!((t[0].ts, t[1].ts), (2, 3));
    }

    #[test]
    fn corrupt_lines_are_skipped_and_prune_removes_only_old_ones() {
        let d = test_dir("audit_prune");
        record(&d, &e(100, "a"));
        std::fs::OpenOptions::new().append(true).open(d.join(AUDIT_FILE)).unwrap().write_all(b"not json\n").unwrap();
        record(&d, &e(100 + 200 * 86_400, "b"));
        let removed = prune(&d, 100 + 200 * 86_400, 90);
        assert_eq!(removed, 1);
        let t = tail(&d, 10);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].tool, "b");
    }

    #[test]
    fn long_phrases_are_truncated() {
        let long = "x".repeat(900);
        let en = AuditEntry::new(1, &long, "t", "r", "o", "d", "o", &long);
        assert_eq!(en.phrase.len(), 200);
        assert_eq!(en.detail.len(), 600);
    }
}
