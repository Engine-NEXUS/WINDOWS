//! Undo registry and quarantine manifest (docs/features/140, P2). One small JSON file in the
//! NEXUS data directory, rewritten atomically. Delete never deletes: items move to
//! `<allowed root>\.nexus-trash\<batch>\` (same volume, so the move is instant) and the manifest
//! remembers where they came from, so "undo that" always works until the age purge.

use std::path::Path;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

pub const JOURNAL_FILE: &str = "action_journal.json";
pub const TRASH_DIR: &str = ".nexus-trash";
const MAX_BATCHES: usize = 50;

/// Serialises load-modify-save so two actions never overwrite each other's journal entries.
pub static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QEntry {
    pub id: String,
    pub batch: String,
    pub original: String,
    pub stored: String,
    pub is_dir: bool,
    pub bytes: u64,
    pub items: u64,
    pub at: i64,
    pub restored: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Moved { from: String, to: String },
    Created { path: String },
    Quarantined { entry: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Batch {
    pub id: String,
    pub tool: String,
    pub at: i64,
    pub phrase: String,
    pub ops: Vec<Op>,
    pub undone: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Journal {
    pub quarantine: Vec<QEntry>,
    pub batches: Vec<Batch>,
}

pub fn load(dir: &Path) -> Journal {
    std::fs::read_to_string(dir.join(JOURNAL_FILE))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, j: &Journal) -> Result<(), String> {
    let tmp = dir.join(format!("{JOURNAL_FILE}.tmp"));
    std::fs::write(&tmp, serde_json::to_string_pretty(j).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(JOURNAL_FILE)).map_err(|e| e.to_string())
}

pub fn new_id(now: i64) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    format!("{now}-{}", N.fetch_add(1, Ordering::SeqCst))
}

impl Journal {
    pub fn push_batch(&mut self, b: Batch) {
        self.batches.push(b);
        let extra = self.batches.len().saturating_sub(MAX_BATCHES);
        if extra > 0 {
            self.batches.drain(0..extra);
        }
    }

    /// The newest batch that has not been undone yet.
    pub fn last_undoable(&self) -> Option<&Batch> {
        self.batches.iter().rev().find(|b| !b.undone)
    }

    pub fn entry(&self, id: &str) -> Option<&QEntry> {
        self.quarantine.iter().find(|e| e.id == id)
    }

    /// Quarantined items still waiting (not restored), newest first, for the Hub list.
    pub fn waiting(&self) -> Vec<&QEntry> {
        let mut v: Vec<&QEntry> = self.quarantine.iter().filter(|e| !e.restored).collect();
        v.sort_by(|a, b| b.at.cmp(&a.at));
        v
    }

    /// Entries older than `days` that were never restored.
    pub fn purge_due(&self, now: i64, days: i64) -> Vec<QEntry> {
        let cutoff = now - days * 86_400;
        self.quarantine.iter().filter(|e| !e.restored && e.at < cutoff).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::paths::test_dir;

    fn batch(id: &str) -> Batch {
        Batch { id: id.into(), tool: "move".into(), at: 1, phrase: "p".into(), ops: vec![], undone: false }
    }

    #[test]
    fn round_trip_and_corrupt_fallback() {
        let d = test_dir("journal");
        let mut j = Journal::default();
        j.push_batch(batch("a"));
        j.quarantine.push(QEntry {
            id: "q1".into(),
            batch: "a".into(),
            original: "D:\\x\\a.pdf".into(),
            stored: "D:\\x\\.nexus-trash\\a\\a.pdf".into(),
            is_dir: false,
            bytes: 5,
            items: 1,
            at: 10,
            restored: false,
        });
        save(&d, &j).unwrap();
        let back = load(&d);
        assert_eq!(back.batches.len(), 1);
        assert_eq!(back.entry("q1").unwrap().bytes, 5);
        std::fs::write(d.join(JOURNAL_FILE), "garbage").unwrap();
        assert!(load(&d).batches.is_empty());
    }

    #[test]
    fn only_the_newest_fifty_batches_are_kept_and_undone_ones_are_skipped() {
        let mut j = Journal::default();
        for i in 0..60 {
            j.push_batch(batch(&format!("b{i}")));
        }
        assert_eq!(j.batches.len(), MAX_BATCHES);
        assert_eq!(j.batches[0].id, "b10");
        assert_eq!(j.last_undoable().unwrap().id, "b59");
        j.batches.last_mut().unwrap().undone = true;
        assert_eq!(j.last_undoable().unwrap().id, "b58");
    }

    #[test]
    fn purge_selects_only_old_unrestored_entries() {
        let mut j = Journal::default();
        let mk = |id: &str, at: i64, restored: bool| QEntry {
            id: id.into(),
            batch: "b".into(),
            original: String::new(),
            stored: String::new(),
            is_dir: false,
            bytes: 0,
            items: 1,
            at,
            restored,
        };
        j.quarantine.push(mk("old", 0, false));
        j.quarantine.push(mk("old_restored", 0, true));
        j.quarantine.push(mk("new", 40 * 86_400, false));
        let due = j.purge_due(41 * 86_400, 30);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].id, "old");
        assert_eq!(j.waiting().len(), 2);
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(new_id(5), new_id(5));
    }
}
