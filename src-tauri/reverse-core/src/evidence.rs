//! Evidence IDs: every fact the analysis produces gets a stable ID (`E-IMP-0003`, `E-STR-0017` ...).
//! Reports cite IDs, and the LLM claim validator (`explain`) drops any claim whose cited IDs do not
//! exist or do not contain the names the claim mentions.

use serde::Serialize;
use std::collections::HashMap;

/// Hard cap so a hostile file cannot make the evidence table (and the report) unbounded.
pub const MAX_EVIDENCE: usize = 6000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvKind {
    Import,
    Export,
    Section,
    Str,
    Meta,
    Rule,
    Packer,
    Dotnet,
    Asar,
}

impl EvKind {
    pub fn prefix(self) -> &'static str {
        match self {
            EvKind::Import => "IMP",
            EvKind::Export => "EXP",
            EvKind::Section => "SEC",
            EvKind::Str => "STR",
            EvKind::Meta => "META",
            EvKind::Rule => "RULE",
            EvKind::Packer => "PACK",
            EvKind::Dotnet => "NET",
            EvKind::Asar => "ASAR",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Evidence {
    pub id: String,
    pub kind: EvKind,
    /// The literal thing observed (an import name, a string, a section name, a rule name ...).
    pub value: String,
    /// Where/how it was observed (DLL, offset, entropy ...). Searched by the validator too.
    pub detail: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct EvidenceSet {
    items: Vec<Evidence>,
    #[serde(skip)]
    counters: HashMap<EvKind, usize>,
    #[serde(skip)]
    by_id: HashMap<String, usize>,
    pub truncated: bool,
}

impl EvidenceSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a fact; returns its ID, or `None` when the table is full (`truncated` is then set).
    pub fn add(&mut self, kind: EvKind, value: impl Into<String>, detail: impl Into<String>) -> Option<String> {
        if self.items.len() >= MAX_EVIDENCE {
            self.truncated = true;
            return None;
        }
        let n = self.counters.entry(kind).or_insert(0);
        *n += 1;
        let id = format!("E-{}-{:04}", kind.prefix(), *n);
        self.by_id.insert(id.clone(), self.items.len());
        self.items.push(Evidence { id: id.clone(), kind, value: value.into(), detail: detail.into() });
        Some(id)
    }

    pub fn get(&self, id: &str) -> Option<&Evidence> {
        self.by_id.get(id).map(|&i| &self.items[i])
    }

    pub fn items(&self) -> &[Evidence] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// First evidence ID of `kind` whose value equals `value` (case-insensitive).
    pub fn find(&self, kind: EvKind, value: &str) -> Option<&Evidence> {
        self.items.iter().find(|e| e.kind == kind && e.value.eq_ignore_ascii_case(value))
    }

    /// Does `token` occur (case-insensitive substring) in the value or detail of ANY of the cited IDs?
    /// Unknown IDs never match.
    pub fn cites_token(&self, ids: &[String], token: &str) -> bool {
        let t = token.to_ascii_lowercase();
        ids.iter().filter_map(|id| self.get(id)).any(|e| {
            e.value.to_ascii_lowercase().contains(&t) || e.detail.to_ascii_lowercase().contains(&t)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_per_kind_and_resolvable() {
        let mut s = EvidenceSet::new();
        let a = s.add(EvKind::Import, "CreateProcessW", "kernel32.dll").unwrap();
        let b = s.add(EvKind::Import, "VirtualAlloc", "kernel32.dll").unwrap();
        let c = s.add(EvKind::Str, "http://example.test/x", "offset 0x40").unwrap();
        assert_eq!((a.as_str(), b.as_str(), c.as_str()), ("E-IMP-0001", "E-IMP-0002", "E-STR-0001"));
        assert_eq!(s.get("E-IMP-0002").unwrap().value, "VirtualAlloc");
        assert!(s.get("E-IMP-0099").is_none());
        assert!(s.find(EvKind::Import, "virtualalloc").is_some());
        assert!(s.find(EvKind::Str, "virtualalloc").is_none());
    }

    #[test]
    fn cites_token_checks_only_the_cited_ids_and_ignores_unknown_ids() {
        let mut s = EvidenceSet::new();
        let a = s.add(EvKind::Import, "CreateProcessW", "kernel32.dll").unwrap();
        let b = s.add(EvKind::Import, "connect", "ws2_32.dll").unwrap();
        assert!(s.cites_token(&[a.clone()], "createprocess"));
        assert!(s.cites_token(&[a.clone()], "KERNEL32"));
        assert!(!s.cites_token(&[a.clone()], "connect"));
        assert!(s.cites_token(&[a.clone(), b], "connect"));
        assert!(!s.cites_token(&["E-IMP-0777".to_string()], "createprocess"));
    }

    #[test]
    fn table_is_capped() {
        let mut s = EvidenceSet::new();
        for i in 0..MAX_EVIDENCE + 10 {
            let _ = s.add(EvKind::Str, format!("s{i}"), "");
        }
        assert_eq!(s.len(), MAX_EVIDENCE);
        assert!(s.truncated);
        assert!(s.add(EvKind::Str, "x", "").is_none());
    }
}
