//! Tauri commands behind the Memory panel (settings sidebar): view, edit,
//! delete and clear everything NEXUS remembers about the user.
//!
//! All data is local files under `<app data>/memory/` (+ the conversation
//! files); nothing here talks to the network.

use serde_json::json;
use tauri::{AppHandle, Manager, Runtime};

fn data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

fn entry_json(e: &crate::profile::Entry) -> serde_json::Value {
    json!({ "text": e.text, "ts": e.ts, "sensitive": e.sensitive })
}

/// Everything the panel shows, in one call.
#[tauri::command]
pub fn memory_overview<R: Runtime>(app: AppHandle<R>) -> Result<serde_json::Value, String> {
    let dir = data_dir(&app)?;
    let profile = crate::profile::load(&dir);
    let habits = crate::habits::load(&dir);
    let facts: Vec<serde_json::Value> = crate::memory::all_facts(&dir)
        .into_iter()
        .map(|(k, v)| {
            let sensitive = crate::pii_filter::sanitize(&v) != v;
            json!({ "key": k, "value": v, "sensitive": sensitive })
        })
        .collect();
    let section = |name: &str| -> Vec<serde_json::Value> {
        profile.section(name).iter().map(entry_json).collect()
    };
    Ok(json!({
        "profile": {
            "identity": section(crate::profile::IDENTITY),
            "can_do": section(crate::profile::CAN_DO),
            "needs": section(crate::profile::NEEDS),
            "priorities": section(crate::profile::PRIORITIES),
        },
        "facts": facts,
        "habits": {
            "top": crate::habits::top_used(&habits, 5)
                .into_iter()
                .map(|(name, count)| json!({ "name": name, "count": count }))
                .collect::<Vec<_>>(),
            "suggestions": crate::habits::suggestions(&dir),
        },
        "conversation_turns": crate::conversation::turn_count(&dir),
    }))
}

/// Add a profile entry. Returns "added" | "added_private" | "duplicate".
#[tauri::command]
pub fn memory_profile_add<R: Runtime>(
    app: AppHandle<R>,
    section: String,
    text: String,
) -> Result<String, String> {
    let dir = data_dir(&app)?;
    match crate::profile::add(&dir, &section, &text) {
        crate::profile::AddOutcome::Added { sensitive: false } => Ok("added".to_string()),
        crate::profile::AddOutcome::Added { sensitive: true } => Ok("added_private".to_string()),
        crate::profile::AddOutcome::Duplicate => Ok("duplicate".to_string()),
        crate::profile::AddOutcome::Invalid => Err("couldn't save that entry".to_string()),
    }
}

/// Remove one profile entry by its index inside a section.
#[tauri::command]
pub fn memory_profile_remove<R: Runtime>(
    app: AppHandle<R>,
    section: String,
    index: usize,
) -> Result<bool, String> {
    Ok(crate::profile::remove_at(&data_dir(&app)?, &section, index))
}

/// Save (or overwrite) a core fact. The key is normalised like the voice hook.
#[tauri::command]
pub fn memory_fact_set<R: Runtime>(
    app: AppHandle<R>,
    key: String,
    value: String,
) -> Result<bool, String> {
    let key = crate::memory::normalize_key(&key);
    let value = value.trim().to_string();
    if key.is_empty() || value.is_empty() {
        return Err("a fact needs a name and a value".to_string());
    }
    Ok(crate::memory::remember(&data_dir(&app)?, &key, &value))
}

/// Never suggest this habit again.
#[tauri::command]
pub fn memory_habit_dismiss<R: Runtime>(app: AppHandle<R>, key: String) -> Result<bool, String> {
    Ok(crate::habits::dismiss(&data_dir(&app)?, &key))
}

/// Clear stored memory. `scope`: conversation | profile | facts | habits | all.
#[tauri::command]
pub fn memory_clear<R: Runtime>(app: AppHandle<R>, scope: String) -> Result<bool, String> {
    let dir = data_dir(&app)?;
    let ok = match scope.as_str() {
        "conversation" => clear_conversation(&dir),
        "profile" => crate::profile::clear_all(&dir),
        "facts" => crate::memory::clear_facts(&dir),
        "habits" => crate::habits::clear_all(&dir),
        "all" => {
            // Evaluate every store (no short-circuit) so one failure doesn't
            // leave the others behind.
            let results = [
                clear_conversation(&dir),
                crate::profile::clear_all(&dir),
                crate::memory::clear_facts(&dir),
                crate::habits::clear_all(&dir),
            ];
            results.iter().all(|r| *r)
        }
        other => return Err(format!("unknown memory scope: {other}")),
    };
    Ok(ok)
}

/// Raw conversation turns + brief + the legacy episodic log.
pub(crate) fn clear_conversation(dir: &std::path::Path) -> bool {
    let a = crate::conversation::clear_all(dir);
    let episodes = crate::memory::resolve_memory_dir(dir).join("episodes.jsonl");
    let b = !episodes.exists() || std::fs::remove_file(episodes).is_ok();
    a && b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_memipc_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    /// "Clear conversation history" used to be a no-op that left every
    /// transcript on disk; the clear must really remove the files.
    #[test]
    fn test_clear_conversation_removes_all_transcript_files() {
        let d = tmpdir("clear");
        crate::conversation::record_conversation_turn(
            &d,
            crate::conversation::ConversationTurnInput {
                agent: "worker".into(),
                intent: "unknown".into(),
                transcript: "what is the weather".into(),
                response: "sunny".into(),
                outcome: crate::conversation::ConversationOutcome::Completed,
                unresolved: vec![],
                owner: crate::voice_profile::TurnOwnership::Verified,
            },
        );
        crate::memory::log_episode(&d, "old episode", "old reply");
        assert_eq!(crate::conversation::turn_count(&d), 1);
        assert!(clear_conversation(&d));
        assert_eq!(crate::conversation::turn_count(&d), 0);
        assert!(!crate::memory::resolve_memory_dir(&d).join("episodes.jsonl").exists());
        // Facts/profile are untouched by a conversation-only clear.
        crate::memory::remember(&d, "city", "Delhi");
        assert!(clear_conversation(&d));
        assert_eq!(crate::memory::all_facts(&d), vec![("city".to_string(), "Delhi".to_string())]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn test_all_facts_sorted_and_clear_facts() {
        let d = tmpdir("facts");
        crate::memory::remember(&d, "zeta", "1");
        crate::memory::remember(&d, "alpha", "2");
        assert_eq!(
            crate::memory::all_facts(&d).iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );
        assert!(crate::memory::clear_facts(&d));
        assert!(crate::memory::all_facts(&d).is_empty());
        assert!(crate::memory::clear_facts(&d), "idempotent");
        let _ = std::fs::remove_dir_all(&d);
    }
}
