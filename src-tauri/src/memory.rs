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

fn normalize_key(k: &str) -> String {
    k.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Store a core fact. Returns true on success.
pub fn remember(app_data_dir: &Path, key: &str, value: &str) -> bool {
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
    let obj = match map.as_object() {
        Some(o) => o,
        None => return vec![],
    };
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .map(|s| s.to_string())
        .collect();
    let mut hits = vec![];
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
    hits
}

/// Append an episode (transcript + optional response). Prunes to cap + 30-day window.
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
}
