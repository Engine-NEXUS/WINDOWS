//! 3-tier persistent memory: core facts + episodic log + semantic recall.
//!
//! File-based (no native deps): memory/ under app data dir.
//!   core.json      — persistent key-value facts ("name" -> "Lakshya")
//!   episodes.jsonl — rolling 30-day conversation log (transcript + response)
//!   facts.json     — extracted semantic facts [{key, value, updated_at}]
//!
//! Recall is substring-based (no embeddings) — fast, local, deterministic.

use std::path::{Path, PathBuf};

const MAX_EPISODES: usize = 500;
const EPISODE_RETENTION_SECS: i64 = 30 * 24 * 3600;
const MAX_CONTEXT_CHARS: usize = 800;

pub fn resolve_memory_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("memory")
}

fn read_json(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(serde_json::Value::Null)
}

fn write_json(path: &Path, v: &serde_json::Value) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    serde_json::to_string_pretty(v)
        .map(|s| std::fs::write(path, s).is_ok())
        .unwrap_or(false)
}

/// Parse "remember that X is Y" / "remember X is Y" / "my X is Y".
/// Returns (key, value). All lowercase keys, original-case values.
pub fn parse_remember(transcript: &str) -> Option<(String, String)> {
    let t = transcript.trim();
    let lower = t.to_lowercase();
    // Explicit remember commands
    for prefix in ["remember that ", "remember ", "note that "] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            // rest is lowercase; recover original-case value by slicing original
            let orig_rest = &t[prefix.len().min(t.len())..];
            if let Some((k, v)) = split_is(orig_rest) {
                return Some((normalize_key(k), v.trim().to_string()));
            }
            let _ = rest;
        }
    }
    // "my X is Y" shorthand
    if let Some(rest) = lower.strip_prefix("my ") {
        let orig_rest = &t[3.min(t.len())..];
        if let Some((k, v)) = split_is(orig_rest) {
            let _ = rest;
            return Some((normalize_key(k), v.trim().to_string()));
        }
    }
    None
}

fn split_is(s: &str) -> Option<(&str, &str)> {
    let lower = s.to_lowercase();
    // find " is " (case-insensitive via lower, same byte indices for ASCII " is ")
    let idx = lower.find(" is ")?;
    Some((s[..idx].trim(), s[idx + 4..].trim()))
}

pub fn normalize_key(k: &str) -> String {
    k.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Keys/values that must NEVER be learned or stored (secrets, OTPs).
/// Checked in `remember()` and in episode mining — memory is for facts
/// about the user, never credentials.
const SECRET_MARKERS: &[&str] = &[
    "password", "passwd", "otp", "cvv", "cvc", "pin", "token", "secret", "api_key", "apikey",
    "private_key", "privatekey", "auth", "credential",
];

fn is_secret(key: &str, value: &str) -> bool {
    // Word-exact (not substring): "shopping_list" and "favorite_author"
    // must stay learnable while "my_pin" and "api_key" are refused.
    // Owned Strings first — the segments borrow them.
    let key_lower = key.to_lowercase();
    let value_lower = value.to_lowercase();
    let mut segs: Vec<&str> = key_lower.split('_').filter(|s| !s.is_empty()).collect();
    segs.extend(
        value_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty()),
    );
    SECRET_MARKERS.iter().any(|m| segs.iter().any(|s| s == m))
}

/// Store a core fact. Returns true on success. Refuses secrets.
pub fn remember(app_data_dir: &Path, key: &str, value: &str) -> bool {
    if is_secret(key, value) {
        return false;
    }
    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("core.json");
    let mut map = read_json(&path);
    if !map.is_object() {
        map = serde_json::json!({});
    }
    map[key] = serde_json::Value::String(value.to_string());
    write_json(&path, &map)
}

/// Delete a core fact. Returns true if it existed.
pub fn forget(app_data_dir: &Path, key: &str) -> bool {
    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("core.json");
    let mut map = read_json(&path);
    let obj = match map.as_object_mut() {
        Some(o) => o,
        None => return false,
    };
    let removed = obj.remove(key).is_some();
    if removed {
        write_json(&path, &map);
    }
    removed
}

/// Recall facts matching query tokens (substring, case-insensitive).
pub fn recall(app_data_dir: &Path, query: &str) -> Vec<(String, String)> {
    let dir = resolve_memory_dir(app_data_dir);
    let map = read_json(&dir.join("core.json"));
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect();
    let mut hits = vec![];
    // Core facts (missing file = empty, not an early return — learned
    // facts below must still be reachable).
    if let Some(obj) = map.as_object() {
        for (k, v) in obj {
            let v_str = v.as_str().unwrap_or("").to_string();
            let hay = format!("{} {}", k, v_str).to_lowercase();
            if tokens.is_empty() || tokens.iter().any(|tok| hay.contains(tok)) {
                hits.push((k.clone(), v_str));
            }
            if hits.len() >= 10 {
                break;
            }
        }
    }
    // Learned facts (facts.json) for keys not already hit — same rule.
    if hits.len() < 10 {
        if let serde_json::Value::Array(arr) = read_json(&dir.join("facts.json")) {
            for item in arr {
                let k = item.get("key").and_then(|v| v.as_str()).unwrap_or("");
                let v = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
                if k.is_empty() || hits.iter().any(|(hk, _)| hk == k) {
                    continue;
                }
                let hay = format!("{} {}", k, v).to_lowercase();
                if tokens.is_empty() || tokens.iter().any(|tok| hay.contains(tok)) {
                    hits.push((k.to_string(), v.to_string()));
                }
                if hits.len() >= 10 {
                    break;
                }
            }
        }
    }
    hits
}

/// Deterministic auto-learning patterns (M2): (phrase, fact key).
/// Local-only, no cloud. Values are original-case remainders.
const LEARN_PATTERNS: &[(&str, &str)] = &[
    ("i work at ", "employer"),
    ("i work for ", "employer"),
    ("i like ", "likes"),
    ("i love ", "likes"),
    ("call me ", "nickname"),
    ("my birthday is ", "birthday"),
    ("i live in ", "city"),
];

/// Values too generic to be worth learning ("i like it", "call me Ish" ok).
const GENERIC_VALUES: &[&str] = &[
    "it", "that", "this", "here", "there", "them", "you", "me", "us", "him", "her", "ok", "okay",
    "fine", "good", "tired", "hungry", "busy", "late", "ready", "sure",
];

/// Mine learnable facts from one transcript. Pure + unit-tested.
/// Returns (key, value). Secrets and generic values are skipped.
pub fn extract_learned_facts(transcript: &str) -> Vec<(String, String)> {
    let lower = transcript.trim().to_lowercase();
    let mut out = vec![];
    for (phrase, key) in LEARN_PATTERNS {
        if let Some(pos) = lower.find(phrase) {
            let raw = transcript.trim()[pos + phrase.len()..].trim();
            let value: String = raw
                .trim_end_matches(['.', ',', '!', '?'])
                .trim()
                .chars()
                .take(80)
                .collect::<String>()
                .trim()
                .to_string();
            if value.len() < 2 || GENERIC_VALUES.contains(&value.to_lowercase().as_str()) {
                continue;
            }
            if is_secret(key, &value) {
                continue;
            }
            // Nicknames must look like names (1-2 alpha words).
            if *key == "nickname" {
                let words: Vec<&str> = value.split_whitespace().collect();
                if words.len() > 2 || !words.iter().all(|w| w.chars().all(|c| c.is_alphabetic())) {
                    continue;
                }
            }
            out.push((key.to_string(), value));
        }
    }
    out
}

const MAX_LEARNED_FACTS: usize = 200;

/// Persist mined facts into facts.json (upsert by key). This makes the
/// long-documented facts.json real: learned semantic facts with source
/// and timestamp, recalled alongside core.json.
pub fn save_learned_facts(app_data_dir: &Path, facts: &[(String, String)]) -> bool {
    if facts.is_empty() {
        return true;
    }
    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("facts.json");
    let now = chrono::Utc::now().timestamp();
    let mut arr = match read_json(&path) {
        serde_json::Value::Array(a) => a,
        _ => vec![],
    };
    for (key, value) in facts {
        if let Some(pos) = arr.iter().position(|item| {
            item.get("key").and_then(|v| v.as_str()).unwrap_or("") == key
        }) {
            arr[pos] = serde_json::json!({"key": key, "value": value, "source": "episode", "updated_at": now});
        } else {
            arr.push(serde_json::json!({"key": key, "value": value, "source": "episode", "updated_at": now}));
        }
    }
    if arr.len() > MAX_LEARNED_FACTS {
        let drop = arr.len() - MAX_LEARNED_FACTS;
        arr.drain(..drop);
    }
    write_json(&path, &serde_json::Value::Array(arr))
}

/// Append an episode (transcript + optional response). Prunes to cap + 30-day window.
/// Also mines deterministic learnable facts (M2, local-only, silent).
pub fn log_episode(app_data_dir: &Path, transcript: &str, response: &str) {
    let dir = resolve_memory_dir(app_data_dir);
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("episodes.jsonl");
    let now = chrono::Utc::now().timestamp();
    let line = serde_json::json!({
        "ts": now,
        "transcript": transcript.chars().take(300).collect::<String>(),
        "response": response.chars().take(300).collect::<String>(),
    });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        use std::io::Write;
        let _ = writeln!(f, "{}", line);
    }
    prune_episodes(&path, now);
    // M2 auto-learning: mine + persist (bounded, silent, local-only).
    let mined = extract_learned_facts(transcript);
    if !mined.is_empty() {
        save_learned_facts(app_data_dir, &mined);
    }
}

fn prune_episodes(path: &Path, now: i64) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };
    let lines: Vec<&str> = content.lines().collect();
    let mut kept: Vec<String> = vec![];
    for line in lines.iter().rev() {
        if kept.len() >= MAX_EPISODES {
            break;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ts = v.get("ts").and_then(|t| t.as_i64()).unwrap_or(0);
        if now - ts > EPISODE_RETENTION_SECS {
            continue;
        }
        kept.push(line.to_string());
    }
    kept.reverse();
    let _ = std::fs::write(path, kept.join("\n") + if kept.is_empty() { "" } else { "\n" });
}

/// Build a compact memory context block for prompt injection.
/// Core facts matching the transcript + up to 3 recent episodes.
pub fn get_memory_context(app_data_dir: &Path, transcript: &str) -> Option<String> {
    let mut parts: Vec<String> = vec![];

    let facts = recall(app_data_dir, transcript);
    if !facts.is_empty() {
        let lines: Vec<String> = facts
            .iter()
            .take(8)
            .map(|(k, v)| format!("- {}: {}", k, v))
            .collect();
        parts.push(format!("Known facts:\n{}", lines.join("\n")));
    }

    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("episodes.jsonl");
    if let Ok(content) = std::fs::read_to_string(&path) {
        let lines: Vec<&str> = content.lines().collect();
        let recent: Vec<String> = lines
            .iter()
            .rev()
            .take(3)
            .filter_map(|l| {
                serde_json::from_str::<serde_json::Value>(l).ok().map(|v| {
                    let t = v.get("transcript").and_then(|x| x.as_str()).unwrap_or("");
                    format!("- past: {}", t.chars().take(120).collect::<String>())
                })
            })
            .collect();
        if !recent.is_empty() {
            parts.push(format!("Recent:\n{}", recent.join("\n")));
        }
    }

    if parts.is_empty() {
        return None;
    }
    let mut ctx = parts.join("\n");
    if ctx.len() > MAX_CONTEXT_CHARS {
        ctx.truncate(MAX_CONTEXT_CHARS);
    }
    Some(ctx)
}

// ─── Proactive Mail Watches Store ─────────────────────────────────────

use crate::google::types::{ThreadWatchTarget, WatchStatus};

/// Load all saved thread watches from memory.
pub fn load_mail_watches(app_data_dir: &Path) -> Vec<ThreadWatchTarget> {
    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("mail_watches.json");
    let val = read_json(&path);
    serde_json::from_value(val).unwrap_or_default()
}

/// Save the complete list of thread watches to memory.
pub fn save_mail_watches(app_data_dir: &Path, watches: &[ThreadWatchTarget]) -> bool {
    let dir = resolve_memory_dir(app_data_dir);
    let path = dir.join("mail_watches.json");
    if let Ok(val) = serde_json::to_value(watches) {
        write_json(&path, &val)
    } else {
        false
    }
}

/// Add or update a thread watch in memory.
pub fn add_mail_watch(app_data_dir: &Path, watch: ThreadWatchTarget) -> bool {
    let mut watches = load_mail_watches(app_data_dir);
    if let Some(pos) = watches.iter().position(|w| w.watch_id == watch.watch_id || w.thread_id == watch.thread_id) {
        watches[pos] = watch;
    } else {
        watches.push(watch);
    }
    save_mail_watches(app_data_dir, &watches)
}

/// Update the status of a specific thread watch.
pub fn update_mail_watch_status(app_data_dir: &Path, watch_id: &str, status: WatchStatus) -> bool {
    let mut watches = load_mail_watches(app_data_dir);
    if let Some(w) = watches.iter_mut().find(|w| w.watch_id == watch_id) {
        w.status = status;
        save_mail_watches(app_data_dir, &watches)
    } else {
        false
    }
}

// ─── Unified user profile (M1) ──────────────────────────────────────
// One `memory/user.json` merging every credential island: manual facts,
// primary Google identity, GitHub username, voice enrollment, contact
// names (read-only), frequent apps. Rebuilt from sources on demand, so a
// wipe is always recoverable and nothing here is authoritative alone.

/// Known-person cap (contacts can be hundreds; profile stays small).
const MAX_PROFILE_PEOPLE: usize = 20;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UserProfile {
    pub name: Option<String>,
    pub primary_email: Option<String>,
    pub github_user: Option<String>,
    pub voice_enrolled: bool,
    pub people: Vec<String>,
    pub updated_at: i64,
}

/// Read contact names from the local contacts.json (`{"Name": number}`).
/// Read-only. Missing/corrupt file → empty (never an error).
pub fn read_contact_names(app_data_dir: &Path) -> Vec<String> {
    let content = std::fs::read_to_string(app_data_dir.join("contacts.json")).unwrap_or_default();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::Value::Null);
    match v.as_object() {
        Some(obj) => obj
            .keys()
            .take(MAX_PROFILE_PEOPLE)
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty())
            .collect(),
        None => vec![],
    }
}

/// Pure builder: core facts + contact names + credential hints → profile.
pub fn build_user_profile(
    core: &serde_json::Value,
    contact_names: &[String],
    primary_email: Option<&str>,
    github_user: Option<&str>,
    voice_enrolled: bool,
) -> UserProfile {
    let name = core
        .get("name")
        .or_else(|| core.get("nickname"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut people: Vec<String> = contact_names.iter().take(MAX_PROFILE_PEOPLE).cloned().collect();
    if let Some(n) = &name {
        people.retain(|p| p.to_lowercase() != n.to_lowercase());
    }
    UserProfile {
        name,
        primary_email: primary_email.map(|s| s.to_string()),
        github_user: github_user.map(|s| s.to_string()),
        voice_enrolled,
        people,
        updated_at: chrono::Utc::now().timestamp(),
    }
}

/// Rebuild user.json from current sources. Returns true on success.
pub fn refresh_user_profile(
    app_data_dir: &Path,
    primary_email: Option<&str>,
    github_user: Option<&str>,
    voice_enrolled: bool,
) -> bool {
    let core = read_json(&resolve_memory_dir(app_data_dir).join("core.json"));
    let contacts = read_contact_names(app_data_dir);
    let profile = build_user_profile(&core, &contacts, primary_email, github_user, voice_enrolled);
    match serde_json::to_value(&profile) {
        Ok(v) => write_json(&resolve_memory_dir(app_data_dir).join("user.json"), &v),
        Err(_) => false,
    }
}

pub fn read_user_profile(app_data_dir: &Path) -> Option<UserProfile> {
    let v = read_json(&resolve_memory_dir(app_data_dir).join("user.json"));
    serde_json::from_value(v).ok()
}

fn count_array_file(dir: &Path, name: &str) -> usize {
    match read_json(&dir.join(name)) {
        serde_json::Value::Array(a) => a.len(),
        _ => 0,
    }
}

fn count_lines(path: &Path) -> usize {
    std::fs::read_to_string(path).map(|c| c.lines().count()).unwrap_or(0)
}

/// Spoken memory audit (M0): "I remember N facts. ...". Short by design
/// (spoken aloud) — the sidebar/JSON surface can show the full profile.
pub fn memory_audit_summary(app_data_dir: &Path) -> String {
    let dir = resolve_memory_dir(app_data_dir);
    let core = read_json(&dir.join("core.json"));
    let core_count = core.as_object().map(|o| o.len()).unwrap_or(0);
    let learned = count_array_file(&dir, "facts.json");
    let episodes = count_lines(&dir.join("episodes.jsonl"));
    let profile = read_user_profile(app_data_dir);
    let mut parts: Vec<String> = vec![];
    if let Some(name) = profile.as_ref().and_then(|p| p.name.clone()) {
        parts.push(format!("You are {}", name));
    }
    let total = core_count + learned;
    if total == 0 {
        parts.push("I don't remember any facts yet".to_string());
    } else {
        parts.push(format!("I remember {} {}", total, if total == 1 { "fact" } else { "facts" }));
    }
    if let Some(obj) = core.as_object() {
        for (k, v) in obj.iter().take(3) {
            if let Some(s) = v.as_str() {
                parts.push(format!("{} is {}", k.replace('_', " "), s));
            }
        }
    }
    if let Some(p) = profile {
        if !p.people.is_empty() {
            let show: Vec<&str> = p.people.iter().take(3).map(|s| s.as_str()).collect();
            parts.push(format!(
                "I know {} {} ({})",
                p.people.len(),
                if p.people.len() == 1 { "person" } else { "people" },
                show.join(", ")
            ));
        }
        if episodes > 0 {
            parts.push(format!(
                "across {} recent {}",
                episodes,
                if episodes == 1 { "conversation" } else { "conversations" }
            ));
        }
    }
    parts.join(". ") + "."
}

/// Wipe all learned memory (core + learned facts + episodes + profile).
/// Credential islands (Google/GitHub/voice) are untouched — the profile
/// rebuilds from them on next refresh. Returns files removed.
pub fn wipe_memory(app_data_dir: &Path) -> usize {
    let dir = resolve_memory_dir(app_data_dir);
    let mut n = 0;
    for name in ["core.json", "facts.json", "episodes.jsonl", "user.json"] {
        if std::fs::remove_file(dir.join(name)).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_mem_test_{}_{}", name, std::process::id()));
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn test_parse_remember_explicit() {
        assert_eq!(
            parse_remember("remember that my dog is Bruno"),
            Some(("my_dog".to_string(), "Bruno".to_string()))
        );
        assert_eq!(
            parse_remember("Remember pizza topping is pepperoni"),
            Some(("pizza_topping".to_string(), "pepperoni".to_string()))
        );
    }

    #[test]
    fn test_parse_remember_my_shorthand() {
        assert_eq!(
            parse_remember("my favorite color is blue"),
            Some(("favorite_color".to_string(), "blue".to_string()))
        );
    }

    #[test]
    fn test_parse_remember_miss() {
        assert_eq!(parse_remember("open chrome"), None);
        assert_eq!(parse_remember("remember"), None);
    }

    #[test]
    fn test_remember_recall_forget() {
        let d = tmpdir("rrf");
        assert!(remember(&d, "name", "Lakshya"));
        let hits = recall(&d, "what is my name");
        assert!(hits.iter().any(|(k, v)| k == "name" && v == "Lakshya"));
        assert!(forget(&d, "name"));
        assert!(recall(&d, "name").is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_recall_empty_query_returns_all() {
        let d = tmpdir("all");
        remember(&d, "a", "1");
        assert!(!recall(&d, "").is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_log_and_context() {
        let d = tmpdir("ctx");
        remember(&d, "dog", "Bruno");
        log_episode(&d, "what is my dog name", "Bruno");
        let ctx = get_memory_context(&d, "dog name");
        assert!(ctx.is_some());
        assert!(ctx.unwrap().contains("Bruno"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_no_memory_returns_none() {
        let d = tmpdir("none");
        assert_eq!(get_memory_context(&d, "hello world xyz"), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_episode_prune_cap() {
        let d = tmpdir("prune");
        for i in 0..5 {
            log_episode(&d, &format!("q{i}"), &format!("a{i}"));
        }
        let path = resolve_memory_dir(&d).join("episodes.jsonl");
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 5);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_mail_watches_persistence() {
        let d = tmpdir("watches");
        let w = ThreadWatchTarget {
            watch_id: "w_123".into(),
            thread_id: "th_456".into(),
            account_email: None,
            initial_history_id: Some("999".into()),
            sender: "prof@university.edu".into(),
            subject: "CS50 Project".into(),
            initial_deadline_raw: Some("Friday 5 PM".into()),
            message_count: 2,
            created_at_ms: 1000,
            last_checked_ms: 1000,
            status: WatchStatus::Active,
        };

        assert!(add_mail_watch(&d, w.clone()));
        let loaded = load_mail_watches(&d);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].watch_id, "w_123");
        assert_eq!(loaded[0].status, WatchStatus::Active);

        // Update status to Triggered
        assert!(update_mail_watch_status(&d, "w_123", WatchStatus::Triggered));
        let updated = load_mail_watches(&d);
        assert_eq!(updated[0].status, WatchStatus::Triggered);

        let _ = std::fs::remove_dir_all(&d);
    }

    // ─── M0/M1/M2 memory tests ─────────────────────────────────────

    #[test]
    fn test_extract_learned_facts_patterns() {
        let f = extract_learned_facts("I work at ServX");
        assert!(f.contains(&("employer".to_string(), "ServX".to_string())), "{:?}", f);
        let f = extract_learned_facts("call me Ish");
        assert!(f.contains(&("nickname".to_string(), "Ish".to_string())), "{:?}", f);
        let f = extract_learned_facts("my birthday is June 1");
        assert!(f.contains(&("birthday".to_string(), "June 1".to_string())), "{:?}", f);
        let f = extract_learned_facts("I live in Hyderabad");
        assert!(f.contains(&("city".to_string(), "Hyderabad".to_string())), "{:?}", f);
        let f = extract_learned_facts("I like biryani");
        assert!(f.contains(&("likes".to_string(), "biryani".to_string())), "{:?}", f);
    }

    #[test]
    fn test_extract_skips_generic_and_secrets() {
        assert!(extract_learned_facts("I like it").is_empty());
        assert!(extract_learned_facts("call me when you are free").is_empty());
        assert!(extract_learned_facts("my password is hunter2").is_empty());
        assert!(extract_learned_facts("what is the weather").is_empty());
    }

    #[test]
    fn test_remember_refuses_secrets_but_keeps_normal() {
        let d = tmpdir("secrets");
        assert!(!remember(&d, "my_password", "hunter2"));
        assert!(!remember(&d, "otp", "4821"));
        assert!(remember(&d, "employer", "ServX"));
        // Word-exact: shopping/author stay learnable.
        assert!(remember(&d, "shopping_list", "milk and eggs"));
        assert!(remember(&d, "favorite_author", "Asimov"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_log_episode_mines_and_recall_finds_learned() {
        let d = tmpdir("mine");
        log_episode(&d, "I work at ServX", "noted");
        let hits = recall(&d, "employer servx");
        assert!(hits.iter().any(|(k, v)| k == "employer" && v == "ServX"), "{:?}", hits);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_profile_builder_merge_and_audit() {
        let d = tmpdir("profile");
        assert!(remember(&d, "name", "Lakshya"));
        std::fs::write(d.join("contacts.json"), r#"{"Mom": "+9111", "Asha": "+9122"}"#).unwrap();
        let core = read_json(&resolve_memory_dir(&d).join("core.json"));
        let p = build_user_profile(&core, &["Mom".to_string(), "Asha".to_string()], Some("me@gmail.com"), None, true);
        assert_eq!(p.name.as_deref(), Some("Lakshya"));
        assert_eq!(p.primary_email.as_deref(), Some("me@gmail.com"));
        assert!(p.voice_enrolled);
        assert_eq!(p.people.len(), 2);
        assert!(refresh_user_profile(&d, Some("me@gmail.com"), None, true));
        let summary = memory_audit_summary(&d);
        assert!(summary.contains("Lakshya"), "{summary}");
        assert!(summary.contains("1 fact"), "{summary}");
        assert!(summary.contains("Mom"), "{summary}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_read_contact_names_and_wipe() {
        let d = tmpdir("contacts");
        std::fs::write(d.join("contacts.json"), r#"{"Mom": "+9111", "Asha": "+9122"}"#).unwrap();
        let names = read_contact_names(&d);
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Mom".to_string()));
        assert!(remember(&d, "name", "Lakshya"));
        assert!(wipe_memory(&d) >= 1);
        assert!(read_user_profile(&d).is_none() || memory_audit_summary(&d).contains("don't remember"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
