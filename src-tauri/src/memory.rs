//! Persistent memory: core facts (+ the about-me profile in `profile.rs` and
//! learned habits in `habits.rs`).
//!
//! File-based (no native deps): memory/ under app data dir.
//!   core.json      — persistent key-value facts ("name" -> "Lakshya")
//!   profile.json   — who the user is / what they can do / need / prioritise
//!   habits.json    — local usage counters (self-learning, no LLM)
//!   episodes.jsonl — optional rolling 30-day log (`log_episode`; not wired to
//!                    any caller today — conversation history lives in
//!                    `conversation.rs`)
//!
//! Recall is substring-based (no embeddings) — fast, local, deterministic.

use std::path::{Path, PathBuf};

const MAX_EPISODES: usize = 500;
const EPISODE_RETENTION_SECS: i64 = 30 * 24 * 3600;
const MAX_CONTEXT_CHARS: usize = 800;

pub fn resolve_memory_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("memory")
}

pub(crate) fn read_json(path: &Path) -> serde_json::Value {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// Read a JSON file for a read-modify-write. A file that EXISTS but does not
/// parse is first copied to `<name>.corrupt` so the next write can't silently
/// replace every stored fact with `{}` (the old behavior after a crash
/// mid-write).
pub(crate) fn read_json_for_update(path: &Path) -> serde_json::Value {
    match std::fs::read_to_string(path) {
        Ok(c) => match serde_json::from_str(&c) {
            Ok(v) => v,
            Err(_) => {
                if !c.trim().is_empty() {
                    let mut backup = path.as_os_str().to_owned();
                    backup.push(".corrupt");
                    let _ = std::fs::write(PathBuf::from(backup), c);
                }
                serde_json::Value::Null
            }
        },
        Err(_) => serde_json::Value::Null,
    }
}

/// Atomic write: temp file + rename, so a crash can't leave a half-written
/// (unparseable) file behind.
pub(crate) fn write_json(path: &Path, v: &serde_json::Value) -> bool {
    match serde_json::to_string_pretty(v) {
        Ok(text) => write_text_atomic(path, &text),
        Err(_) => false,
    }
}

/// Atomic text write (temp file + rename) — shared by every memory-side file.
pub(crate) fn write_text_atomic(path: &Path, text: &str) -> bool {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    if std::fs::write(&tmp, text).is_err() {
        return false;
    }
    if std::fs::rename(&tmp, path).is_ok() {
        return true;
    }
    // Windows can refuse to rename over an existing file that another process
    // holds open; fall back to remove + rename, then to a plain overwrite.
    let _ = std::fs::remove_file(path);
    if std::fs::rename(&tmp, path).is_ok() {
        return true;
    }
    let ok = std::fs::copy(&tmp, path).is_ok();
    let _ = std::fs::remove_file(&tmp);
    ok
}

/// Truncate to at most `max` CHARACTERS (never bytes — `String::truncate`
/// panics when the byte index lands inside a multibyte character, e.g. Hindi
/// or accented text in a stored fact).
pub(crate) fn truncate_chars(s: &mut String, max: usize) {
    if let Some((idx, _)) = s.char_indices().nth(max) {
        s.truncate(idx);
    }
}

/// Personal attributes the bare "my X is Y" shorthand may store. Without this
/// gate any complaint ("my mic is not working", "my screen is dim") became a
/// permanent fact. Explicit "remember …" commands accept any key.
fn is_personal_attribute(key: &str) -> bool {
    const ATTRS: &[&str] = &[
        "name", "nickname", "age", "birthday", "birthdate", "birth_date", "email",
        "phone", "number", "address", "city", "location", "hometown", "country",
        "job", "profession", "role", "title", "company", "employer", "college",
        "university", "school", "major", "course", "degree", "branch", "year",
        "wife", "husband", "partner", "girlfriend", "boyfriend", "mom", "mother",
        "dad", "father", "brother", "sister", "son", "daughter", "friend", "boss",
        "dog", "cat", "pet", "hobby", "goal", "language", "timezone", "diet",
        "birth_city", "pronouns",
    ];
    if ATTRS.contains(&key) {
        return true;
    }
    key.starts_with("favorite_")
        || key.starts_with("favourite_")
        || key.ends_with("_name")
        || key.ends_with("_birthday")
        || key.ends_with("_email")
        || key.ends_with("_number")
}

/// Parse "remember that X is Y" / "remember X is Y" / "my X is Y".
/// Returns (key, value). All lowercase keys, original-case values.
pub fn parse_remember(transcript: &str) -> Option<(String, String)> {
    let t = transcript.trim();
    // ASCII-lowercase keeps byte offsets identical to `t` (Unicode lowercasing
    // can change byte lengths and made the slicing below land mid-character).
    let lower = t.to_ascii_lowercase();
    // Explicit remember commands
    for prefix in ["remember that ", "remember ", "note that "] {
        if lower.starts_with(prefix) {
            let orig_rest = t.get(prefix.len()..).unwrap_or("");
            if let Some((k, v)) = split_is(orig_rest) {
                let key = normalize_key(k);
                if !key.is_empty() && !v.trim().is_empty() {
                    return Some((key, v.trim().to_string()));
                }
            }
        }
    }
    // "my X is Y" shorthand — personal attributes only
    if lower.starts_with("my ") {
        let orig_rest = t.get(3..).unwrap_or("");
        if let Some((k, v)) = split_is(orig_rest) {
            let key = normalize_key(k);
            if is_personal_attribute(&key) && !v.trim().is_empty() {
                return Some((key, v.trim().to_string()));
            }
        }
    }
    None
}

fn split_is(s: &str) -> Option<(&str, &str)> {
    // ASCII-lowercase: same byte indices as `s`, so slicing is always safe.
    let lower = s.to_ascii_lowercase();
    let idx = lower.find(" is ")?;
    Some((s[..idx].trim(), s[idx + 4..].trim()))
}

pub(crate) fn normalize_key(k: &str) -> String {
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
    let mut map = read_json_for_update(&path);
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
    let mut map = read_json_for_update(&path);
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

/// Every stored core fact, sorted by key (Memory panel — `recall` caps at 10).
pub fn all_facts(app_data_dir: &Path) -> Vec<(String, String)> {
    let map = read_json(&resolve_memory_dir(app_data_dir).join("core.json"));
    let mut out: Vec<(String, String)> = map
        .as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Delete every core fact (privacy "clear everything").
pub fn clear_facts(app_data_dir: &Path) -> bool {
    let path = resolve_memory_dir(app_data_dir).join("core.json");
    !path.exists() || std::fs::remove_file(path).is_ok()
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
/// The about-me profile (always) + core facts matching the transcript + up to
/// 3 recent episodes.
pub fn get_memory_context(app_data_dir: &Path, transcript: &str) -> Option<String> {
    let mut parts: Vec<String> = vec![];

    // The profile is injected on EVERY turn (not query-gated): a question like
    // "what should I work on today?" shares no token with "priority: …".
    if let Some(block) = crate::profile::prompt_block(app_data_dir) {
        parts.push(block);
    }

    // Facts containing PII (phone, e-mail, card, …) stay local: injecting them
    // only for the router to redact (and the Worker path to leak) helped nobody.
    let facts: Vec<(String, String)> = recall(app_data_dir, transcript)
        .into_iter()
        .filter(|(_, v)| crate::pii_filter::sanitize(v) == *v)
        .collect();
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
    truncate_chars(&mut ctx, MAX_CONTEXT_CHARS);
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

    /// Regression: any "my X is Y" complaint used to become a permanent fact.
    #[test]
    fn test_parse_remember_my_shorthand_ignores_complaints() {
        for t in [
            "my mic is not working",
            "my screen is too dim",
            "my laptop is slow today",
            "my internet is down",
        ] {
            assert_eq!(parse_remember(t), None, "{t}");
        }
        assert_eq!(
            parse_remember("my name is Lakshya"),
            Some(("name".to_string(), "Lakshya".to_string()))
        );
        assert_eq!(
            parse_remember("my best friend name is Asha"),
            Some(("best_friend_name".to_string(), "Asha".to_string()))
        );
        // Explicit remember still accepts anything.
        assert_eq!(
            parse_remember("remember that my laptop is a Dell"),
            Some(("my_laptop".to_string(), "a Dell".to_string()))
        );
    }

    /// Regression: byte-offset slicing of a Unicode-lowercased copy could land
    /// mid-character and panic on non-ASCII input.
    #[test]
    fn test_parse_remember_non_ascii_never_panics() {
        for t in [
            "remember İstanbul is my city",
            "my İsim is Ahmet",
            "remember that मेरा शहर is दिल्ली",
            "remember that my café is Blue Tokai",
        ] {
            let _ = parse_remember(t);
        }
        assert_eq!(
            parse_remember("remember that my café is Blue Tokai"),
            Some(("my_café".to_string(), "Blue Tokai".to_string()))
        );
    }

    #[test]
    fn test_truncate_chars_is_utf8_safe() {
        let mut s = "é".repeat(500); // 2 bytes per char; byte 800 is mid-char
        truncate_chars(&mut s, 400);
        assert_eq!(s.chars().count(), 400);
        let mut short = "abc".to_string();
        truncate_chars(&mut short, 400);
        assert_eq!(short, "abc");
    }

    #[test]
    fn test_context_with_multibyte_facts_does_not_panic() {
        let d = tmpdir("utf8ctx");
        for i in 0..8 {
            remember(&d, &format!("fact_{i}"), &"दिल्ली".repeat(40));
        }
        let ctx = get_memory_context(&d, "").expect("context");
        assert!(ctx.chars().count() <= MAX_CONTEXT_CHARS);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_corrupt_core_is_backed_up_not_silently_wiped() {
        let d = tmpdir("corrupt");
        let dir = resolve_memory_dir(&d);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("core.json"), "{\"name\": \"Lakshya\", ").unwrap();
        assert!(remember(&d, "city", "Delhi"));
        let backup = std::fs::read_to_string(dir.join("core.json.corrupt")).unwrap();
        assert!(backup.contains("Lakshya"), "corrupt original must be preserved");
        assert!(!dir.join("core.json.tmp").exists(), "no temp file left behind");
        let _ = std::fs::remove_dir_all(&d);
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
    fn test_context_always_includes_profile_and_skips_pii_facts() {
        let d = tmpdir("ctxprof");
        crate::profile::add(&d, crate::profile::PRIORITIES, "ship the ghost mode fixes");
        remember(&d, "phone", "9876543210");
        remember(&d, "city", "Delhi");
        // Query shares no token with the profile or the facts.
        let ctx = get_memory_context(&d, "what should we tackle next").unwrap();
        assert!(ctx.contains("About the user"), "{ctx}");
        assert!(ctx.contains("ship the ghost mode fixes"), "{ctx}");
        assert!(!ctx.contains("9876543210"), "PII fact must stay local: {ctx}");
        let ctx = get_memory_context(&d, "which city").unwrap();
        assert!(ctx.contains("Delhi"), "{ctx}");
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
