//! About-me profile: who the user is, what they can do, what they need, and
//! what they are prioritising right now.
//!
//! Stored in `memory/profile.json` (same atomic file-backed pattern as
//! `memory.rs`; no native deps, no embeddings, no LLM). A small capped block is
//! ALWAYS injected into cloud prompts (see `memory::get_memory_context`) — the
//! old flat `core.json` recall only injected a fact when a query token happened
//! to match it, so the assistant knew nothing about the user exactly when a
//! question like "what should I work on today?" needed it.
//!
//! Voice grammar is deliberately narrow so in-room speech never writes memory
//! by accident:
//!   * priorities: "my priority is …", "my top priority is …", "set a priority: …",
//!     "my goal is …"
//!   * everything else needs an explicit memory verb: "remember that I am …",
//!     "remember I can …", "note that I need …"
//!   * recall:  "what do you know about me", "who am I", "what are my priorities"
//!   * forget:  "forget my priority …", "clear my priorities", "forget that I …",
//!     "forget my <fact>"
//!
//! Entries containing PII (phone, e-mail, card, …) are flagged `sensitive`:
//! kept locally, never put in a cloud prompt, never read aloud (TTS may be a
//! cloud engine).

use std::path::{Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

pub const IDENTITY: &str = "identity";
pub const CAN_DO: &str = "can_do";
pub const NEEDS: &str = "needs";
pub const PRIORITIES: &str = "priorities";
pub const SECTIONS: [&str; 4] = [IDENTITY, CAN_DO, NEEDS, PRIORITIES];

const MAX_ENTRY_CHARS: usize = 160;
/// Prompt budget for the profile block (characters). Always-on, so it stays small.
const PROMPT_MAX_CHARS: usize = 450;

fn cap_for(section: &str) -> usize {
    match section {
        IDENTITY => 6,
        CAN_DO | NEEDS => 8,
        PRIORITIES => 5,
        _ => 0,
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub text: String,
    #[serde(default)]
    pub ts: i64,
    /// Contains PII: local-only (not injected into prompts, not spoken).
    #[serde(default)]
    pub sensitive: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub identity: Vec<Entry>,
    #[serde(default)]
    pub can_do: Vec<Entry>,
    #[serde(default)]
    pub needs: Vec<Entry>,
    #[serde(default)]
    pub priorities: Vec<Entry>,
}

impl Profile {
    pub fn section(&self, name: &str) -> &[Entry] {
        match name {
            IDENTITY => &self.identity,
            CAN_DO => &self.can_do,
            NEEDS => &self.needs,
            PRIORITIES => &self.priorities,
            _ => &[],
        }
    }

    fn section_mut(&mut self, name: &str) -> Option<&mut Vec<Entry>> {
        match name {
            IDENTITY => Some(&mut self.identity),
            CAN_DO => Some(&mut self.can_do),
            NEEDS => Some(&mut self.needs),
            PRIORITIES => Some(&mut self.priorities),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        SECTIONS.iter().all(|s| self.section(s).is_empty())
    }
}

fn profile_path(app_data_dir: &Path) -> PathBuf {
    crate::memory::resolve_memory_dir(app_data_dir).join("profile.json")
}

/// Load the profile (missing / unreadable file → empty profile).
pub fn load(app_data_dir: &Path) -> Profile {
    let v = crate::memory::read_json(&profile_path(app_data_dir));
    serde_json::from_value(v).unwrap_or_default()
}

/// Atomic save.
pub fn save(app_data_dir: &Path, profile: &Profile) -> bool {
    match serde_json::to_value(profile) {
        Ok(v) => crate::memory::write_json(&profile_path(app_data_dir), &v),
        Err(_) => false,
    }
}

/// Outcome of adding an entry.
#[derive(Debug, Clone, PartialEq)]
pub enum AddOutcome {
    Added { sensitive: bool },
    Duplicate,
    Invalid,
}

fn clean_text(raw: &str) -> String {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed
        .trim_matches(|c: char| matches!(c, '.' | ',' | ';' | '!' | '?' | ':' | '"' | '\'' | ' '))
        .to_string();
    let mut out = trimmed;
    crate::memory::truncate_chars(&mut out, MAX_ENTRY_CHARS);
    out
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Add an entry to a section. Case-insensitive de-dupe (refreshes the
/// timestamp); the section is capped by dropping the OLDEST entries.
pub fn add(app_data_dir: &Path, section: &str, text: &str) -> AddOutcome {
    let text = clean_text(text);
    if text.chars().count() < 2 || cap_for(section) == 0 {
        return AddOutcome::Invalid;
    }
    let mut profile = {
        // Corrupt file → back it up (read_json_for_update) instead of wiping.
        let v = crate::memory::read_json_for_update(&profile_path(app_data_dir));
        serde_json::from_value::<Profile>(v).unwrap_or_default()
    };
    let sensitive = crate::pii_filter::sanitize(&text) != text;
    let Some(list) = profile.section_mut(section) else {
        return AddOutcome::Invalid;
    };
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.text.to_lowercase() == text.to_lowercase())
    {
        existing.ts = now_ts();
        save(app_data_dir, &profile);
        return AddOutcome::Duplicate;
    }
    list.push(Entry {
        text,
        ts: now_ts(),
        sensitive,
    });
    let cap = cap_for(section);
    while list.len() > cap {
        list.remove(0);
    }
    if save(app_data_dir, &profile) {
        AddOutcome::Added { sensitive }
    } else {
        AddOutcome::Invalid
    }
}

/// Remove one entry by index within a section (UI delete). Returns true if removed.
pub fn remove_at(app_data_dir: &Path, section: &str, index: usize) -> bool {
    let mut profile = load(app_data_dir);
    let Some(list) = profile.section_mut(section) else {
        return false;
    };
    if index >= list.len() {
        return false;
    }
    list.remove(index);
    save(app_data_dir, &profile)
}

/// Remove every entry whose text contains `needle` (case-insensitive, min 3
/// chars so "a" can't wipe the profile). Returns how many were removed.
pub fn remove_matching(app_data_dir: &Path, needle: &str) -> usize {
    let needle = needle.trim().to_lowercase();
    if needle.chars().count() < 3 {
        return 0;
    }
    let mut profile = load(app_data_dir);
    let mut removed = 0;
    for section in SECTIONS {
        if let Some(list) = profile.section_mut(section) {
            let before = list.len();
            list.retain(|e| !e.text.to_lowercase().contains(&needle));
            removed += before - list.len();
        }
    }
    if removed > 0 {
        save(app_data_dir, &profile);
    }
    removed
}

/// Empty one section. Returns how many entries were removed.
pub fn clear_section(app_data_dir: &Path, section: &str) -> usize {
    let mut profile = load(app_data_dir);
    let Some(list) = profile.section_mut(section) else {
        return 0;
    };
    let n = list.len();
    list.clear();
    if n > 0 {
        save(app_data_dir, &profile);
    }
    n
}

/// Delete the whole profile file (privacy "clear everything").
pub fn clear_all(app_data_dir: &Path) -> bool {
    let path = profile_path(app_data_dir);
    !path.exists() || std::fs::remove_file(path).is_ok()
}

/// The always-on prompt block. Sensitive entries are excluded. `None` when
/// there is nothing shareable.
pub fn prompt_block(app_data_dir: &Path) -> Option<String> {
    block_for(&load(app_data_dir))
}

fn block_for(profile: &Profile) -> Option<String> {
    let line = |label: &str, section: &str| -> Option<String> {
        let items: Vec<&str> = profile
            .section(section)
            .iter()
            .filter(|e| !e.sensitive)
            .map(|e| e.text.as_str())
            .collect();
        if items.is_empty() {
            None
        } else {
            Some(format!("- {label}: {}", items.join("; ")))
        }
    };
    let lines: Vec<String> = [
        line("Who they are", IDENTITY),
        line("Can do", CAN_DO),
        line("Needs", NEEDS),
        // Priorities last and most recent last: the freshest is what matters.
        line("Current priorities", PRIORITIES),
    ]
    .into_iter()
    .flatten()
    .collect();
    if lines.is_empty() {
        return None;
    }
    let mut out = format!("About the user:\n{}", lines.join("\n"));
    crate::memory::truncate_chars(&mut out, PROMPT_MAX_CHARS);
    Some(out)
}

/// Spoken summary for "what do you know about me" / "what are my priorities".
/// Sensitive entries are never read out.
pub fn spoken_summary(profile: &Profile, section: Option<&str>) -> String {
    let visible = |name: &str| -> Vec<String> {
        profile
            .section(name)
            .iter()
            .filter(|e| !e.sensitive)
            .map(|e| e.text.clone())
            .collect()
    };
    let hidden: usize = SECTIONS
        .iter()
        .map(|s| profile.section(s).iter().filter(|e| e.sensitive).count())
        .sum();

    if let Some(name) = section {
        let items = visible(name);
        if items.is_empty() {
            return match name {
                PRIORITIES => "You haven't told me your priorities yet, sir. Say, my priority is, and then what matters.".to_string(),
                _ => "I don't have anything on that yet, sir.".to_string(),
            };
        }
        return match name {
            PRIORITIES => format!("Your priorities, sir: {}.", items.join(", then ")),
            _ => format!("{}.", items.join(". ")),
        };
    }

    let mut parts: Vec<String> = Vec::new();
    let who = visible(IDENTITY);
    if !who.is_empty() {
        parts.push(who.join(". "));
    }
    let can = visible(CAN_DO);
    if !can.is_empty() {
        parts.push(format!("You can do this: {}", can.join(", ")));
    }
    let needs = visible(NEEDS);
    if !needs.is_empty() {
        parts.push(format!("You need this: {}", needs.join(", ")));
    }
    let pri = visible(PRIORITIES);
    if !pri.is_empty() {
        parts.push(format!("Your priorities: {}", pri.join(", then ")));
    }
    if parts.is_empty() {
        return "I don't know much about you yet, sir. Try, remember that I am, and tell me about yourself.".to_string();
    }
    let mut out = format!("{}.", parts.join(". "));
    if hidden > 0 {
        out.push_str(&format!(" I'm also keeping {hidden} private note{} locally.", if hidden == 1 { "" } else { "s" }));
    }
    out
}

// ─── Voice grammar ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ProfileCommand {
    Add { section: &'static str, text: String },
    ForgetMatching { needle: String },
    ForgetFact { key: String },
    ClearPriorities,
    Show { section: Option<&'static str> },
}

static RE_PRIORITY_IS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:my|our)\s+(?:(?:top|main|current|first|biggest)\s+)?priorit(?:y|ies)(?:\s+(?:right now|today|this week|these days))?\s*(?:is|are|:)\s*(.+)$",
    )
    .unwrap()
});
static RE_SET_PRIORITY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:set|add|make)\s+(?:a\s+)?(?:new\s+)?priority(?:\s+(?:to|as|of))?\s*:?\s*(.+)$").unwrap()
});
static RE_GOAL_IS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^my\s+(?:(?:main|top|current)\s+)?goal(?:\s+for\s+(?:today|this\s+week|this\s+month))?\s*(?:is|:)\s*(.+)$",
    )
    .unwrap()
});
static RE_REMEMBER_I: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:please\s+)?(?:remember|note|save|keep\s+in\s+mind)(?:\s+that)?\s+(i(?:'m|\s+am|\s+work|\s+study|\s+live|\s+can|\s+know\s+how\s+to|\s+need|\s+have\s+to|\s+must|\s+want\s+to)\b.+)$",
    )
    .unwrap()
});
static RE_FORGET_PRIORITY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^(?:forget|remove|delete|drop|clear)\s+(?:my\s+|the\s+)?priority\s+(?:about\s+|of\s+)?(.+)$").unwrap()
});
static RE_CLEAR_PRIORITIES: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(?:clear|forget|reset)\s+(?:all\s+)?(?:of\s+)?my\s+priorities$").unwrap());
static RE_FORGET_THAT_I: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^forget\s+that\s+(i(?:'m|\s+am|\s+can|\s+need|\s+have\s+to|\s+want\s+to)\b.+)$").unwrap()
});
static RE_FORGET_MY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^forget\s+my\s+([a-z][a-z' ]{1,40})$").unwrap());
static RE_SHOW_ALL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:who\s+am\s+i|what\s+do\s+you\s+(?:know|remember)\s+about\s+me|tell\s+me\s+(?:what\s+you\s+know\s+)?about\s+(?:me|myself))$",
    )
    .unwrap()
});
static RE_SHOW_PRIORITIES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^(?:what(?:'s|\s+is|\s+are)(?:\s+all)?\s+my\s+(?:(?:top|main|current)\s+)?(?:priority|priorities|goals?)(?:\s+(?:right now|today|this week))?|what\s+should\s+i\s+(?:focus|work)\s+on(?:\s+(?:today|now|next))?)$",
    )
    .unwrap()
});

/// Normalise a transcript for matching: curly apostrophes → straight, leading
/// wake word and trailing punctuation removed.
fn normalize(transcript: &str) -> String {
    let mut s = transcript.trim().replace(['\u{2019}', '\u{2018}'], "'");
    let lower = s.to_ascii_lowercase();
    for w in ["nexus, ", "nexus. ", "nexus "] {
        if lower.starts_with(w) {
            s = s[w.len()..].trim().to_string();
            break;
        }
    }
    s.trim_end_matches(['.', '!', '?', ',']).trim().to_string()
}

/// Classify a first-person statement ("I can …", "I need …", else identity).
fn section_for_statement(statement: &str) -> &'static str {
    let l = statement.to_ascii_lowercase();
    if l.starts_with("i can") || l.starts_with("i know how to") {
        CAN_DO
    } else if l.starts_with("i need") || l.starts_with("i have to") || l.starts_with("i must") || l.starts_with("i want to") {
        NEEDS
    } else {
        IDENTITY
    }
}

/// Parse a profile voice command. Pure; `None` = not a profile command (the
/// transcript then falls through to the normal pipeline untouched).
pub fn parse_profile_command(transcript: &str) -> Option<ProfileCommand> {
    let t = normalize(transcript);
    if t.is_empty() {
        return None;
    }
    for re in [&*RE_PRIORITY_IS, &*RE_SET_PRIORITY, &*RE_GOAL_IS] {
        if let Some(c) = re.captures(&t) {
            return Some(ProfileCommand::Add {
                section: PRIORITIES,
                text: c[1].to_string(),
            });
        }
    }
    if let Some(c) = RE_REMEMBER_I.captures(&t) {
        let statement = c[1].to_string();
        return Some(ProfileCommand::Add {
            section: section_for_statement(&statement),
            text: statement,
        });
    }
    if RE_CLEAR_PRIORITIES.is_match(&t) {
        return Some(ProfileCommand::ClearPriorities);
    }
    if let Some(c) = RE_FORGET_PRIORITY.captures(&t) {
        return Some(ProfileCommand::ForgetMatching { needle: c[1].to_string() });
    }
    if let Some(c) = RE_FORGET_THAT_I.captures(&t) {
        return Some(ProfileCommand::ForgetMatching { needle: c[1].to_string() });
    }
    if RE_SHOW_ALL.is_match(&t) {
        return Some(ProfileCommand::Show { section: None });
    }
    if RE_SHOW_PRIORITIES.is_match(&t) {
        return Some(ProfileCommand::Show { section: Some(PRIORITIES) });
    }
    if let Some(c) = RE_FORGET_MY.captures(&t) {
        let key = crate::memory::normalize_key(&c[1]);
        if !key.is_empty() {
            return Some(ProfileCommand::ForgetFact { key });
        }
    }
    None
}

/// Execute a parsed command and return the line to speak.
pub fn apply(app_data_dir: &Path, cmd: &ProfileCommand) -> String {
    match cmd {
        ProfileCommand::Add { section, text } => match add(app_data_dir, section, text) {
            AddOutcome::Added { sensitive: true } => {
                "Saved, sir — but it looks private, so I'm keeping it on this machine only and won't send it anywhere."
                    .to_string()
            }
            AddOutcome::Added { sensitive: false } => match *section {
                PRIORITIES => format!("Noted, sir. Priority: {}.", clean_text(text)),
                _ => "Noted, sir. I'll remember that.".to_string(),
            },
            AddOutcome::Duplicate => "Already on record, sir.".to_string(),
            AddOutcome::Invalid => "Sorry sir, I couldn't save that.".to_string(),
        },
        ProfileCommand::ClearPriorities => {
            let n = clear_section(app_data_dir, PRIORITIES);
            if n == 0 {
                "You don't have any priorities saved, sir.".to_string()
            } else {
                "Cleared your priorities, sir.".to_string()
            }
        }
        ProfileCommand::ForgetMatching { needle } => {
            if remove_matching(app_data_dir, needle) > 0 {
                "Forgotten, sir.".to_string()
            } else {
                "I don't have anything like that, sir.".to_string()
            }
        }
        ProfileCommand::ForgetFact { key } => {
            // Facts are stored as "name", or "my_dog" for explicit remembers.
            if crate::memory::forget(app_data_dir, key) || crate::memory::forget(app_data_dir, &format!("my_{key}")) {
                "Forgotten, sir.".to_string()
            } else {
                "I don't have anything like that, sir.".to_string()
            }
        }
        ProfileCommand::Show { section } => spoken_summary(&load(app_data_dir), *section),
    }
}

/// Parse + apply in one step: `Some(reply)` when the transcript was a profile
/// command, `None` to continue through the normal pipeline.
pub fn handle(app_data_dir: &Path, transcript: &str) -> Option<String> {
    let cmd = parse_profile_command(transcript)?;
    Some(apply(app_data_dir, &cmd))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_profile_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn test_parse_priority_forms() {
        for (t, want) in [
            ("my priority is finishing the report", "finishing the report"),
            ("My top priority is shipping NEXUS.", "shipping NEXUS"),
            ("my priorities are exams and the demo", "exams and the demo"),
            ("Nexus, my current priority is the Ghost Mode fixes", "the Ghost Mode fixes"),
            ("set a priority: call the bank", "call the bank"),
            ("my goal is to learn Rust", "to learn Rust"),
            ("my main goal for this week is the launch", "the launch"),
        ] {
            assert_eq!(
                parse_profile_command(t),
                Some(ProfileCommand::Add { section: PRIORITIES, text: want.to_string() }),
                "{t}"
            );
        }
    }

    #[test]
    fn test_parse_first_person_needs_explicit_memory_verb() {
        assert_eq!(
            parse_profile_command("remember that I am a final year CS student"),
            Some(ProfileCommand::Add { section: IDENTITY, text: "I am a final year CS student".into() })
        );
        assert_eq!(
            parse_profile_command("remember I can code in Rust and Python"),
            Some(ProfileCommand::Add { section: CAN_DO, text: "I can code in Rust and Python".into() })
        );
        assert_eq!(
            parse_profile_command("note that I need to renew my passport"),
            Some(ProfileCommand::Add { section: NEEDS, text: "I need to renew my passport".into() })
        );
        // Curly apostrophe from STT.
        assert_eq!(
            parse_profile_command("remember that I\u{2019}m a night owl"),
            Some(ProfileCommand::Add { section: IDENTITY, text: "I'm a night owl".into() })
        );
    }

    /// In-room / ordinary speech must never write memory.
    #[test]
    fn test_parse_ignores_ordinary_speech() {
        for t in [
            "I need help with this",
            "I am hungry",
            "I can't hear you",
            "open chrome",
            "remember that we need milk",
            "remember to call mom",
            "what is the capital of France",
            "forget it",
            "priority mail tracking",
            "my mic is not working",
            "",
        ] {
            assert_eq!(parse_profile_command(t), None, "{t}");
        }
    }

    #[test]
    fn test_parse_show_and_forget() {
        assert_eq!(parse_profile_command("what do you know about me?"), Some(ProfileCommand::Show { section: None }));
        assert_eq!(parse_profile_command("who am I"), Some(ProfileCommand::Show { section: None }));
        assert_eq!(
            parse_profile_command("what are my priorities"),
            Some(ProfileCommand::Show { section: Some(PRIORITIES) })
        );
        assert_eq!(
            parse_profile_command("what should I work on today"),
            Some(ProfileCommand::Show { section: Some(PRIORITIES) })
        );
        assert_eq!(parse_profile_command("clear my priorities"), Some(ProfileCommand::ClearPriorities));
        assert_eq!(
            parse_profile_command("forget my priority the demo"),
            Some(ProfileCommand::ForgetMatching { needle: "the demo".into() })
        );
        assert_eq!(
            parse_profile_command("forget that I am a night owl"),
            Some(ProfileCommand::ForgetMatching { needle: "I am a night owl".into() })
        );
        assert_eq!(
            parse_profile_command("forget my favorite color"),
            Some(ProfileCommand::ForgetFact { key: "favorite_color".into() })
        );
    }

    #[test]
    fn test_add_dedupes_caps_and_persists() {
        let d = tmpdir("add");
        assert_eq!(add(&d, PRIORITIES, "finish the report"), AddOutcome::Added { sensitive: false });
        assert_eq!(add(&d, PRIORITIES, "Finish the Report."), AddOutcome::Duplicate);
        assert_eq!(load(&d).priorities.len(), 1);
        for i in 0..8 {
            add(&d, PRIORITIES, &format!("priority number {i}"));
        }
        let p = load(&d);
        assert_eq!(p.priorities.len(), 5, "capped");
        assert_eq!(p.priorities.last().unwrap().text, "priority number 7", "newest kept");
        assert!(!p.priorities.iter().any(|e| e.text == "finish the report"), "oldest dropped");
        assert_eq!(add(&d, "bogus", "x y z"), AddOutcome::Invalid);
        assert_eq!(add(&d, NEEDS, " "), AddOutcome::Invalid);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_sensitive_entries_stay_local() {
        let d = tmpdir("sens");
        assert_eq!(
            add(&d, IDENTITY, "my number is 9876543210 and mail me at a@b.com"),
            AddOutcome::Added { sensitive: true }
        );
        add(&d, IDENTITY, "I am a student");
        let block = prompt_block(&d).unwrap();
        assert!(block.contains("student"));
        assert!(!block.contains("9876543210") && !block.contains("a@b.com"), "{block}");
        let spoken = spoken_summary(&load(&d), None);
        assert!(!spoken.contains("9876543210") && !spoken.contains("a@b.com"), "{spoken}");
        assert!(spoken.contains("private note"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_prompt_block_always_on_and_capped() {
        let d = tmpdir("block");
        assert_eq!(prompt_block(&d), None);
        for i in 0..6 {
            add(&d, IDENTITY, &format!("identity fact number {i} {}", "x".repeat(60)));
            add(&d, NEEDS, &format!("need number {i} {}", "y".repeat(60)));
        }
        add(&d, PRIORITIES, "ship the fixes");
        let block = prompt_block(&d).unwrap();
        assert!(block.starts_with("About the user:"));
        assert!(block.chars().count() <= PROMPT_MAX_CHARS);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_prompt_block_multibyte_safe() {
        let d = tmpdir("mb");
        for i in 0..6 {
            add(&d, IDENTITY, &format!("मैं दिल्ली में रहता हूँ और पढ़ाई करता हूँ {i} {}", "दिल्ली ".repeat(20)));
        }
        let block = prompt_block(&d).unwrap();
        assert!(block.chars().count() <= PROMPT_MAX_CHARS);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_handle_end_to_end_replies() {
        let d = tmpdir("e2e");
        let r = handle(&d, "my priority is the Ghost Mode fixes").unwrap();
        assert!(r.contains("Noted") && r.contains("Ghost Mode fixes"), "{r}");
        let r = handle(&d, "what are my priorities").unwrap();
        assert!(r.contains("Ghost Mode fixes"), "{r}");
        let r = handle(&d, "remember that I am a student").unwrap();
        assert!(r.contains("Noted"), "{r}");
        let r = handle(&d, "what do you know about me").unwrap();
        assert!(r.contains("student") && r.contains("Ghost Mode fixes"), "{r}");
        let r = handle(&d, "forget my priority ghost mode").unwrap();
        assert_eq!(r, "Forgotten, sir.");
        let r = handle(&d, "forget my priority ghost mode").unwrap();
        assert!(r.contains("don't have"), "{r}");
        assert!(load(&d).priorities.is_empty());
        let r = handle(&d, "clear my priorities").unwrap();
        assert!(r.contains("don't have any priorities"), "{r}");
        assert_eq!(handle(&d, "open chrome"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_forget_fact_handles_explicit_remember_keys() {
        let d = tmpdir("fact");
        crate::memory::remember(&d, "favorite_color", "blue");
        crate::memory::remember(&d, "my_dog", "Bruno");
        assert_eq!(handle(&d, "forget my favorite color").unwrap(), "Forgotten, sir.");
        assert_eq!(handle(&d, "forget my dog").unwrap(), "Forgotten, sir.");
        assert!(handle(&d, "forget my dog").unwrap().contains("don't have"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_remove_at_and_clear_all() {
        let d = tmpdir("rm");
        add(&d, NEEDS, "a coffee break");
        add(&d, NEEDS, "a new laptop");
        assert!(remove_at(&d, NEEDS, 0));
        assert_eq!(load(&d).needs.len(), 1);
        assert!(!remove_at(&d, NEEDS, 5));
        assert!(clear_all(&d));
        assert!(load(&d).is_empty());
        assert!(clear_all(&d), "idempotent");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_remove_matching_needs_three_chars() {
        let d = tmpdir("min");
        add(&d, PRIORITIES, "an important thing");
        assert_eq!(remove_matching(&d, "a"), 0);
        assert_eq!(load(&d).priorities.len(), 1);
        assert_eq!(remove_matching(&d, "important"), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
