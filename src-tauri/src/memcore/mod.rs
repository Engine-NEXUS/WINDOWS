//! Memory Core — the single owner of what NEXUS remembers (plan P1).
//!
//! `store` is the SQLite layer. This module adds the process-wide handle
//! cache, the one-time import of the legacy JSON files, the `context_pack`
//! that is the ONLY thing allowed to leave the device for a cloud model, and
//! thin mirror/forget/wipe entry points used by `memory.rs`.
//!
//! Rollout is dual-write / dual-read: the legacy files stay authoritative
//! for one release, memcore mirrors every write and answers recall first,
//! and anything memcore cannot answer falls back to the legacy path. The
//! `memcore` setting (default on) turns the whole thing off.

pub mod agenda;
pub mod account_seed;
pub mod briefing;
pub mod corrections;
pub mod curator;
pub mod crypto;
pub mod google_io;
pub mod health;
pub mod hygiene;
pub mod mailtriage;
pub mod mailwatch;
pub mod github_watch;
pub mod deploy_watch;
#[cfg(test)]
pub(crate) mod test_http;
pub mod mailwording;
pub mod selfnames;
pub mod names;
pub mod offer;
pub mod leavewatch;
pub mod leases;
pub mod people;
pub mod profile;
pub mod restore;
pub mod resume;
pub mod review;
pub mod routines;
pub mod scheduler;
pub mod session;
pub mod store;
pub mod synonyms;
pub mod timetable;
pub mod timetable_io;
pub mod wa;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use once_cell::sync::Lazy;
use parking_lot::Mutex;

use store::{Observation, Store, Tier, Trust};

/// Context-pack budget in chars: the always-on profile (<= `profile::PROFILE_CAP`) plus about 1,200 for
/// the question-specific facts and recent episodes (plan 95: 800 -> 2000; Memory v2 M1: 2000 -> 2600).
const MAX_PACK_FACTS: usize = 8;
const MAX_PACK_EPISODES: usize = 3;

static STORES: Lazy<Mutex<HashMap<PathBuf, Arc<Mutex<Store>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static EPISODE_SEQ: AtomicU64 = AtomicU64::new(0);

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Boolean flag from settings.json (camelCase key); `default` when the file
/// or key is missing/unreadable.
pub fn flag(app_data_dir: &Path, key: &str, default: bool) -> bool {
    std::fs::read_to_string(app_data_dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|j| j.get(key).and_then(|v| v.as_bool()))
        .unwrap_or(default)
}

/// `memcore` setting from settings.json (default true; unreadable = true).
pub fn enabled(app_data_dir: &Path) -> bool {
    flag(app_data_dir, "memcore", true)
}

/// Run `f` against this app-data dir's store (opened + migrated on first
/// use). `None` when disabled or the database cannot be opened — callers
/// then use the legacy path.
pub fn with_store<T>(app_data_dir: &Path, f: impl FnOnce(&Store) -> T) -> Option<T> {
    let handle = store_handle(app_data_dir)?;
    let guard = handle.lock();
    Some(f(&guard))
}

/// The shared handle to this dir's store (opened + migrated on first use), for
/// callers that must not hold the lock across an `.await` (take it briefly,
/// twice, around the network work). `None` when disabled or unopenable.
pub fn store_handle(app_data_dir: &Path) -> Option<Arc<Mutex<Store>>> {
    if !enabled(app_data_dir) {
        return None;
    }
    let mut map = STORES.lock();
    if let Some(h) = map.get(app_data_dir) {
        return Some(h.clone());
    }
    let store = match Store::open(
        &crate::memory::resolve_memory_dir(app_data_dir),
        crypto::process_key(),
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("memcore: cannot open database ({e}); using legacy files");
            return None;
        }
    };
    store.batch(|s| import_legacy(s, app_data_dir));
    store.prune(now_ts());
    let h = Arc::new(Mutex::new(store));
    map.insert(app_data_dir.to_path_buf(), h.clone());
    Some(h)
}

/// One-time import of core.json / facts.json / episodes.jsonl. Idempotent
/// via a `meta` flag; the legacy files are left untouched.
fn import_legacy(store: &Store, app_data_dir: &Path) {
    if store.meta_get("legacy_imported").is_some() {
        return;
    }
    let dir = crate::memory::resolve_memory_dir(app_data_dir);
    let now = now_ts();
    let mut n = 0usize;

    if let Some(obj) = crate::memory::read_json(&dir.join("core.json")).as_object() {
        for (k, v) in obj {
            if let Some(v) = v.as_str() {
                let o = Observation {
                    tier: Tier::Fact,
                    key: k.clone(),
                    value: v.to_string(),
                    source: "legacy:core".into(),
                    trust: Trust::UserSaid,
                    pinned: true,
                };
                if store.observe(&o, now).is_ok() {
                    n += 1;
                }
            }
        }
    }
    if let serde_json::Value::Array(arr) = crate::memory::read_json(&dir.join("facts.json")) {
        for item in arr {
            let k = item.get("key").and_then(|v| v.as_str()).unwrap_or("");
            let v = item.get("value").and_then(|v| v.as_str()).unwrap_or("");
            let ts = item.get("updated_at").and_then(|v| v.as_i64()).unwrap_or(now);
            let o = Observation {
                tier: Tier::Fact,
                key: k.into(),
                value: v.into(),
                source: "legacy:facts".into(),
                trust: Trust::Derived,
                pinned: false,
            };
            // Keep the original timestamp so old facts still decay on schedule.
            if store.observe(&o, ts).is_ok() {
                n += 1;
            }
        }
    }
    if let Ok(content) = std::fs::read_to_string(dir.join("episodes.jsonl")) {
        for (i, line) in content.lines().enumerate() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
            let ts = v.get("ts").and_then(|t| t.as_i64()).unwrap_or(now);
            let text = v.get("transcript").and_then(|t| t.as_str()).unwrap_or("");
            let o = Observation {
                tier: Tier::Episode,
                key: format!("ep:{ts}:i{i}"),
                value: text.into(),
                source: "legacy:episodes".into(),
                trust: Trust::Derived,
                pinned: false,
            };
            if store.observe(&o, ts).is_ok() {
                n += 1;
            }
        }
    }
    store.meta_set("legacy_imported", "1");
    tracing::info!("memcore: imported {n} legacy records");
}

/// Mirror an explicit "remember" (user said it → pinned).
pub fn mirror_core_fact(app_data_dir: &Path, key: &str, value: &str) {
    with_store(app_data_dir, |s| {
        let _ = s.observe(
            &Observation {
                tier: Tier::Fact,
                key: key.into(),
                value: value.into(),
                source: "user:remember".into(),
                trust: Trust::UserSaid,
                pinned: true,
            },
            now_ts(),
        );
    });
}

/// Mirror a mined fact (derived, decays).
pub fn mirror_learned_fact(app_data_dir: &Path, key: &str, value: &str) {
    with_store(app_data_dir, |s| {
        let _ = s.observe(
            &Observation {
                tier: Tier::Fact,
                key: key.into(),
                value: value.into(),
                source: "miner:episode".into(),
                trust: Trust::Derived,
                pinned: false,
            },
            now_ts(),
        );
    });
}

pub fn mirror_episode(app_data_dir: &Path, transcript: &str) {
    let seq = EPISODE_SEQ.fetch_add(1, Ordering::Relaxed);
    let now = now_ts();
    with_store(app_data_dir, |s| {
        let _ = s.observe(
            &Observation {
                tier: Tier::Episode,
                key: format!("ep:{now}:{seq}"),
                value: transcript.into(),
                source: "turn".into(),
                trust: Trust::Derived,
                pinned: false,
            },
            now,
        );
        s.prune(now);
    });
}

pub fn forget_key(app_data_dir: &Path, key: &str) -> usize {
    with_store(app_data_dir, |s| s.forget_key(key, "user", now_ts())).unwrap_or(0)
}

/// Erase every record. When encrypted, also rotate the data key so any old
/// ciphertext left in free disk blocks becomes permanently unreadable.
pub fn wipe(app_data_dir: &Path) -> usize {
    with_store(app_data_dir, |s| {
        let n = s.wipe("user", now_ts());
        if s.is_encrypted() {
            #[cfg(not(test))]
            if let Some(k) = crypto::rotate_key() {
                s.rekey(k);
            }
            #[cfg(test)]
            if let Some(k) = crypto::generate_key() {
                s.rekey(k);
            }
        }
        n
    })
    .unwrap_or(0)
}

/// Record what is leaving the device for a cloud model on this turn.
pub fn log_egress(app_data_dir: &Path, route: &str, request: &str, memory: &str) {
    with_store(app_data_dir, |s| s.log_egress(now_ts(), route, request, memory));
}

pub fn egress_log(app_data_dir: &Path, limit: usize) -> Vec<store::EgressEntry> {
    with_store(app_data_dir, |s| s.egress_recent(limit)).unwrap_or_default()
}

/// Every mishearing NEXUS has noticed, applied or not (Memory page, "Words I learned to fix").
pub fn list_corrections(app_data_dir: &Path) -> Vec<store::CorrectionRow> {
    with_store(app_data_dir, |s| s.corrections_all()).unwrap_or_default()
}

/// Stop applying and forget what was learned about one heard phrase. Returns rows removed.
pub fn forget_correction(app_data_dir: &Path, heard: &str) -> usize {
    let n = with_store(app_data_dir, |s| s.correction_forget(heard.trim())).unwrap_or(0);
    corrections::refresh(app_data_dir);
    n
}

/// Everything remembered, for the "what do you remember" views.
pub fn list_rows(app_data_dir: &Path, limit: usize) -> Vec<store::Row> {
    with_store(app_data_dir, |s| s.list_rows(limit)).unwrap_or_default()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Status {
    pub enabled: bool,
    /// At-rest encryption active (false = plaintext fallback or disabled).
    pub encrypted: bool,
    pub facts: usize,
    pub episodes: usize,
    pub egress_7d: usize,
    pub redact_names: bool,
    /// Foreground-window recording (resume points) is on.
    pub activity: bool,
    /// Resume points currently stored.
    pub resume_points: usize,
    /// Once-a-day boot briefing is on.
    pub briefing: bool,
    /// Timetable reminders are on.
    pub timetable: bool,
    /// Saved timetable slots.
    pub slots: usize,
    /// Inbox watching + calendar are on.
    pub mail: bool,
    /// Say important emails in plain words (GitHub on device; unusual via cloud).
    pub mail_plain: bool,
    /// Watch GitHub Actions runs of recent repos and announce failure/recovery.
    pub github_watch: bool,
    /// Watch latest production deploys (Vercel, Render, Netlify) and announce failure/recovery.
    pub deploy_watch: bool,
    /// A Google account with mail access is connected.
    pub google_connected: bool,
    /// Important emails currently filed (7 days).
    pub mail_items: usize,
    /// Google rejected the stored sign-in (typically the 7-day Testing-mode
    /// expiry): the inbox watcher is paused until the user signs in again.
    pub google_needs_signin: bool,
    /// WhatsApp priority watcher is switched on (default off).
    pub whatsapp: bool,
    /// Message text may be spoken aloud (off: sidebar only).
    pub whatsapp_speak: bool,
    /// Leave-by reminders (calendar + traffic + remembered home) are on.
    pub leave_by: bool,
    /// Learned WhatsApp people.
    pub people: usize,
    /// "What was open" snapshots (reopen offer after a restart) are on.
    pub session: bool,
    /// Startup connection check in the greeting is on.
    pub health: bool,
    /// The always-on profile (what you told NEXUS, minus sensitive topics) is sent with cloud requests.
    pub profile: bool,
    /// The owner's name is learned from the connected Google account (older UIs omit this: on).
    pub account_seed: bool,
    /// NEXUS may notice command sequences you repeat and offer to turn them into one phrase.
    pub routines: bool,
    /// Optional background review of cloud conversations is on (off unless the user switched it on).
    pub review: bool,
    /// Suggested memory changes waiting for the user's approval.
    pub pending: usize,
    /// Recent housekeeping notice (e.g. an unused routine was removed); empty when there is none.
    pub curator_note: String,
}

/// Erase recorded resume points (activity history) and nothing else.
pub fn clear_activity(app_data_dir: &Path) -> usize {
    // The "what was open" snapshots are activity history too.
    with_store(app_data_dir, |s| {
        s.meta_set("session_last", "");
        s.meta_set("session_pending", "");
        s.clear_tier(Tier::Resume, "user", now_ts())
    })
    .unwrap_or(0)
}

pub fn status(app_data_dir: &Path) -> Status {
    let redact = names::enabled(app_data_dir);
    let activity = flag(app_data_dir, "memcoreActivity", true);
    let briefing = flag(app_data_dir, "memcoreBriefing", true);
    let timetable = flag(app_data_dir, "memcoreTimetable", true);
    let mail = flag(app_data_dir, "memcoreMail", true);
    let mail_plain = flag(app_data_dir, "memcoreMailPlain", false);
    let github_watch = flag(app_data_dir, "memcoreGithubWatch", true);
    let deploy_watch = flag(app_data_dir, "memcoreDeployWatch", true);
    let whatsapp = flag(app_data_dir, "memcoreWhatsapp", false);
    let whatsapp_speak = flag(app_data_dir, "memcoreWhatsappSpeak", false);
    let leave_by = flag(app_data_dir, "memcoreLeaveBy", true);
    let session = flag(app_data_dir, "memcoreSession", true);
    let health = flag(app_data_dir, "memcoreHealth", true);
    let profile = flag(app_data_dir, "memcoreProfile", true);
    let account_seed = flag(app_data_dir, "memcoreAccountSeed", true);
    let routines = flag(app_data_dir, "memcoreRoutines", true);
    let review = flag(app_data_dir, "memcoreReview", false);
    let pending = review::list(app_data_dir).len();
    let curator_note = curator::note(app_data_dir, now_ts()).unwrap_or_default();
    let google_connected = !google_io::list_accounts(google_io::can_read_mail).is_empty();
    let google_needs_signin = with_store(app_data_dir, |s| s.meta_get("mail_needs_signin").map(|v| !v.is_empty()).unwrap_or(false))
        .unwrap_or(false);
    match with_store(app_data_dir, |s| {
        (
            s.is_encrypted(),
            s.count(Tier::Fact),
            s.count(Tier::Episode),
            s.egress_recent(10_000).len(),
            s.count(Tier::Resume),
            s.count(Tier::Slot),
            s.count(Tier::Mail),
            s.count(Tier::Person),
        )
    }) {
        Some((encrypted, facts, episodes, egress_7d, resume_points, slots, mail_items, people)) => Status {
            enabled: true,
            encrypted,
            facts,
            episodes,
            egress_7d,
            redact_names: redact,
            activity,
            resume_points,
            briefing,
            timetable,
            slots,
            mail,
            mail_plain,
            github_watch,
            deploy_watch,
            google_connected,
            mail_items,
            google_needs_signin,
            whatsapp,
            whatsapp_speak,
            leave_by,
            people,
            session,
            health,
            profile,
            account_seed,
            routines,
            review,
            pending,
            curator_note,
        },
        None => Status {
            enabled: false,
            encrypted: false,
            facts: 0,
            episodes: 0,
            egress_7d: 0,
            redact_names: redact,
            activity,
            resume_points: 0,
            briefing,
            timetable,
            slots: 0,
            mail,
            mail_plain,
            github_watch,
            deploy_watch,
            google_connected,
            mail_items: 0,
            google_needs_signin,
            whatsapp,
            whatsapp_speak,
            leave_by,
            people: 0,
            session,
            health,
            profile,
            account_seed,
            routines,
            review,
            pending,
            curator_note,
        },
    }
}

/// Ranked fact recall. `None` = memcore unavailable; `Some(vec![])` = it
/// answered and found nothing (caller may still try the legacy substring path).
pub fn recall(app_data_dir: &Path, query: &str, limit: usize) -> Option<Vec<(String, String)>> {
    with_store(app_data_dir, |s| {
        s.search(Tier::Fact, query, limit, now_ts())
            .into_iter()
            .map(|h| (h.key, h.value))
            .collect()
    })
}

fn md_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn age_label(now: i64, ts: i64) -> String {
    let days = ((now - ts).max(0)) / 86_400;
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        n => format!("{n}d ago"),
    }
}

/// Sidebar card for "what do you remember": pinned (you told me), learned,
/// recent conversations — each with where it came from and how old it is.
pub fn card_markdown(rows: &[store::Row], st: &Status, now: i64) -> String {
    let mut md = String::from("## What I remember\n\n");
    let said: Vec<_> = rows.iter().filter(|r| r.tier == "fact" && r.trust == "user_said").collect();
    let learned: Vec<_> = rows.iter().filter(|r| r.tier == "fact" && r.trust != "user_said").collect();
    let eps: Vec<_> = rows.iter().filter(|r| r.tier == "episode").collect();
    if said.is_empty() && learned.is_empty() && eps.is_empty() {
        md.push_str("Nothing yet. Tell me *remember that my dog is Bruno* and I will.\n\n");
    }
    if !said.is_empty() {
        md.push_str("### You told me\n");
        for r in &said {
            md.push_str(&format!(
                "- **{}**: {} — {}\n",
                md_escape(&r.key.replace('_', " ")),
                md_escape(&r.value),
                age_label(now, r.created)
            ));
        }
        md.push('\n');
    }
    if !learned.is_empty() {
        md.push_str("### I picked up (fades if unused)\n");
        for r in &learned {
            md.push_str(&format!(
                "- **{}**: {} — {}, last used {}\n",
                md_escape(&r.key.replace('_', " ")),
                md_escape(&r.value),
                r.source.replace(':', " "),
                age_label(now, r.last_seen)
            ));
        }
        md.push('\n');
    }
    if !eps.is_empty() {
        md.push_str(&format!("### Recent conversations ({})\n", eps.len()));
        for r in eps.iter().take(3) {
            let t: String = r.value.chars().take(60).collect();
            md.push_str(&format!("- {} — {}\n", md_escape(&t), age_label(now, r.last_seen)));
        }
        md.push('\n');
    }
    md.push_str("---\n");
    md.push_str(&format!(
        "Stored only on this PC, encrypted: **{}**. Sent to cloud models in the last 7 days: **{}** turns (Command Hub → Memory shows exactly what).\n\n",
        if st.encrypted { "yes" } else { "no" },
        st.egress_7d
    ));
    if st.activity {
        md.push_str(&format!(
            "Where-you-left-off history: **{}** points kept for 7 days, on this PC only (Command Hub → Memory can switch it off or clear it).

",
            st.resume_points
        ));
    } else {
        md.push_str("Where-you-left-off recording is **off**.

");
    }
    md.push_str("Say *forget* followed by the name to remove one thing, or *forget everything* to erase it all.\n");
    md
}

/// Truncate to `budget` chars, preferring to cut at a line boundary.
fn fit_budget(text: &str, budget: usize) -> String {
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let cut: String = text.chars().take(budget).collect();
    match cut.rfind('\n') {
        Some(i) if i > budget / 2 => cut[..i].to_string(),
        _ => cut,
    }
}

/// The only memory text allowed to leave the device. Ranked facts + the most
/// recent episodes, PII-redacted, hard-capped to `budget` chars. Used facts
/// get their frequency signal bumped. `None` = memcore unavailable or empty
/// (caller falls back to the legacy context).
pub fn context_pack(app_data_dir: &Path, query: &str, budget: usize) -> Option<String> {
    context_pack_with(app_data_dir, query, budget, flag(app_data_dir, "memcoreProfile", true))
}

/// `context_pack` with the always-on profile switched explicitly (tests compare both).
fn context_pack_with(app_data_dir: &Path, query: &str, budget: usize, use_profile: bool) -> Option<String> {
    let now = now_ts();
    let (profile_text, facts, episodes) = with_store(app_data_dir, |s| {
        // Profile first and stable: the same text every turn until the user's facts change, so the
        // prompt prefix stays cacheable. Its facts are not repeated in the recall list below.
        let (profile_text, profile_ids) = if use_profile {
            profile::render(&curator::profile_entries(s), profile::PROFILE_CAP)
        } else {
            (String::new(), Vec::new())
        };
        let hits = s.search(Tier::Fact, query, MAX_PACK_FACTS + profile_ids.len(), now);
        // A profile fact the question touched was useful: count it, so the curator can keep the facts
        // that get used when the profile is full. (It is not repeated in the recall list below.)
        let touched: Vec<i64> = hits.iter().filter(|h| profile_ids.contains(&h.id)).map(|h| h.id).collect();
        s.mark_used(&touched, now);
        let facts: Vec<_> = hits
            .into_iter()
            .filter(|h| !profile_ids.contains(&h.id))
            .take(MAX_PACK_FACTS)
            .collect();
        let ids: Vec<i64> = facts.iter().map(|h| h.id).collect();
        s.mark_used(&ids, now);
        // Over-fetch, then keep only real speech: junk that predates the admit gate (or slipped
        // past it) must never be sent to the cloud as "Recent:" context.
        let recent: Vec<_> = s
            .recent(Tier::Episode, MAX_PACK_EPISODES * 4)
            .into_iter()
            .filter(|h| hygiene::transcript_quality(&h.value).is_ok())
            .take(MAX_PACK_EPISODES)
            .collect();
        (profile_text, facts, recent)
    })?;
    let mut parts: Vec<String> = vec![];
    if !profile_text.is_empty() {
        parts.push(profile_text);
    }
    if !facts.is_empty() {
        let lines: Vec<String> = facts.iter().map(|h| format!("- {}: {}", h.key, h.value)).collect();
        parts.push(format!("Known facts:\n{}", lines.join("\n")));
    }
    if !episodes.is_empty() {
        let lines: Vec<String> = episodes
            .iter()
            .map(|h| format!("- past: {}", h.value.chars().take(120).collect::<String>()))
            .collect();
        parts.push(format!("Recent:\n{}", lines.join("\n")));
    }
    if parts.is_empty() {
        return None;
    }
    let mut redacted = crate::pii_filter::sanitize(&parts.join("\n"));
    if names::enabled(app_data_dir) {
        redacted = names::redact(&redacted, &names::roster(app_data_dir));
    }
    Some(fit_budget(&redacted, budget))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_memcore_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn imports_legacy_files_once_and_leaves_them_in_place() {
        let d = tmpdir("import");
        let mem = crate::memory::resolve_memory_dir(&d);
        std::fs::create_dir_all(&mem).unwrap();
        std::fs::write(mem.join("core.json"), r#"{"dog_name":"Bruno"}"#).unwrap();
        let recent = now_ts() - 86_400;
        std::fs::write(
            mem.join("facts.json"),
            format!(
                r#"[{{"key":"employer","value":"Acme","source":"episode","updated_at":{recent}}}]"#
            ),
        )
        .unwrap();
        std::fs::write(
            mem.join("episodes.jsonl"),
            "{\"ts\":4102444800,\"transcript\":\"open chrome\",\"response\":\"ok\"}\n",
        )
        .unwrap();
        let hits = recall(&d, "dog", 5).unwrap();
        assert_eq!(hits, vec![("dog_name".to_string(), "Bruno".to_string())]);
        // Second open must not re-import (count stays put).
        let (facts, eps) = with_store(&d, |s| (s.count(Tier::Fact), s.count(Tier::Episode))).unwrap();
        assert_eq!((facts, eps), (2, 1));
        assert!(mem.join("core.json").exists() && mem.join("facts.json").exists());
    }

    #[test]
    fn disabled_flag_bypasses_memcore() {
        let d = tmpdir("disabled");
        std::fs::write(d.join("settings.json"), r#"{"memcore": false}"#).unwrap();
        assert!(!enabled(&d));
        assert!(recall(&d, "anything", 5).is_none());
        assert!(context_pack(&d, "anything", 500).is_none());
        let mem = crate::memory::resolve_memory_dir(&d);
        assert!(!mem.join(store::DB_FILE).exists() && !mem.join(store::ENC_FILE).exists());
    }

    #[test]
    fn context_pack_is_ranked_redacted_and_budgeted() {
        let d = tmpdir("pack");
        mirror_core_fact(&d, "favorite_pizza_topping", "pepperoni");
        mirror_core_fact(&d, "work_email", "lakshya@example.com");
        mirror_core_fact(&d, "dog_name", "Bruno");
        mirror_episode(&d, "open chrome please");
        let pack = context_pack(&d, "what pizza do I like", 2600).unwrap();
        assert!(pack.contains("favorite_pizza_topping: pepperoni"), "{pack}");
        assert!(pack.contains("past: open chrome please"), "{pack}");
        // Email is redacted before it can leave the device.
        let all = context_pack(&d, "email", 2600).unwrap();
        assert!(!all.contains("lakshya@example.com"), "{all}");
        // Budget is honoured and never splits a multi-byte char.
        let tiny = context_pack(&d, "pizza", 20).unwrap();
        assert!(tiny.chars().count() <= 20);
    }

    #[test]
    fn the_profile_leads_the_pack_is_stable_and_is_not_repeated_in_recall() {
        let d = tmpdir("profile");
        mirror_core_fact(&d, "dog_name", "Bruno");
        mirror_core_fact(&d, "employer", "Acme");
        mirror_core_fact(&d, "allergy", "peanuts");
        mirror_learned_fact(&d, "favorite_snack", "chips");
        let prefix = "About the user (they told you this):\n- dog_name: Bruno\n- employer: Acme";

        let a = context_pack_with(&d, "what is my dog name", 2600, true).unwrap();
        let b = context_pack_with(&d, "weather tomorrow", 2600, true).unwrap();
        // Same prefix whatever is asked: that is what lets providers cache it.
        assert!(a.starts_with(prefix), "{a}");
        assert!(b.starts_with(prefix), "{b}");
        // A profile fact is not repeated in the question-specific list.
        assert_eq!(a.matches("dog_name: Bruno").count(), 1, "{a}");
        // Sensitive and mined facts are never always-on...
        assert!(!b.contains("peanuts") && !b.contains("chips"), "{b}");
        // ...but a question that asks for them still finds them.
        let c = context_pack_with(&d, "am I allergic to anything", 2600, true).unwrap();
        assert!(c.contains("allergy: peanuts"), "{c}");
    }

    #[test]
    fn the_profile_can_be_switched_off() {
        let d = tmpdir("profile_off");
        mirror_core_fact(&d, "dog_name", "Bruno");
        let off = context_pack_with(&d, "weather tomorrow", 2600, false);
        assert!(off.map_or(true, |p| !p.contains("About the user")));
        std::fs::write(d.join("settings.json"), r#"{"memcoreProfile": false}"#).unwrap();
        assert!(!status(&d).profile);
        let via_setting = context_pack(&d, "weather tomorrow", 2600);
        assert!(via_setting.map_or(true, |p| !p.contains("About the user")));
    }

    #[test]
    fn synonyms_fill_in_after_exact_words_and_rank_below_them() {
        let d = tmpdir("syn");
        mirror_core_fact(&d, "work_hours", "9 to 5");
        mirror_core_fact(&d, "employer", "Acme");
        let hits = recall(&d, "where do I work", 3).unwrap();
        assert_eq!(hits.first().map(|h| h.0.as_str()), Some("work_hours"), "{hits:?}");
        assert!(hits.iter().any(|h| h.0 == "employer"), "{hits:?}");
    }

    #[test]
    fn fit_budget_cuts_on_line_boundary() {
        let text = "Known facts:\n- aaaa: 1111\n- bbbb: 2222\n- cccc: 3333";
        let cut = fit_budget(text, 40);
        assert!(cut.chars().count() <= 40);
        assert!(!cut.ends_with(' '));
        assert!(text.starts_with(&cut));
        assert_eq!(fit_budget("héllo wörld ünï", 5).chars().count(), 5);
    }

    #[test]
    fn status_reports_encryption_counts_and_egress() {
        let d = tmpdir("status");
        mirror_core_fact(&d, "dog_name", "Bruno");
        mirror_episode(&d, "open chrome");
        log_egress(&d, "cloud", "hello", "Known facts: dog_name Bruno");
        let st = status(&d);
        assert!(st.enabled && st.encrypted, "{st:?}");
        assert_eq!((st.facts, st.episodes, st.egress_7d), (1, 1, 1));
        let log = egress_log(&d, 10);
        assert_eq!(log[0].memory, "Known facts: dog_name Bruno");
        // On disk: sealed snapshot only, and no plaintext of the fact.
        let mem = crate::memory::resolve_memory_dir(&d);
        let raw = std::fs::read(mem.join(store::ENC_FILE)).unwrap();
        assert!(!raw.windows(5).any(|w| w == b"Bruno"));
        assert!(!mem.join(store::DB_FILE).exists());
    }

    #[test]
    fn wipe_keeps_the_store_usable_after_key_rotation() {
        let d = tmpdir("wipe_rekey");
        mirror_core_fact(&d, "dog_name", "Bruno");
        assert_eq!(wipe(&d), 1);
        assert_eq!(status(&d).facts, 0);
        mirror_core_fact(&d, "city", "Pune");
        assert_eq!(recall(&d, "city", 5).unwrap()[0].1, "Pune");
    }

    #[test]
    fn name_redaction_applies_to_the_context_pack_when_enabled() {
        let d = tmpdir("redact_pack");
        std::fs::write(d.join("contacts.json"), r#"{"Asha": "+9111"}"#).unwrap();
        mirror_core_fact(&d, "sister_name", "Asha");
        let plain = context_pack(&d, "sister", 2600).unwrap();
        assert!(plain.contains("Asha"), "{plain}");
        std::fs::write(d.join("settings.json"), r#"{"memcoreRedactNames": true}"#).unwrap();
        let masked = context_pack(&d, "sister", 2600).unwrap();
        assert!(!masked.contains("Asha") && masked.contains("Person A"), "{masked}");
    }

    #[test]
    fn card_markdown_groups_by_provenance_escapes_and_dates() {
        let now = 100 * 86_400;
        let row = |tier: &str, key: &str, value: &str, trust: &str, created: i64| store::Row {
            id: 1,
            tier: tier.into(),
            key: key.into(),
            value: value.into(),
            source: "miner:episode".into(),
            trust: trust.into(),
            created,
            last_seen: created,
            pinned: trust == "user_said",
            uses: 0,
        };
        let rows = vec![
            row("fact", "dog_name", "Bruno [x](http://evil)", "user_said", now),
            row("fact", "employer", "Acme", "derived", now - 3 * 86_400),
            row("episode", "ep:1", "open chrome", "derived", now - 86_400),
        ];
        let st = Status { enabled: true, encrypted: true, facts: 2, episodes: 1, egress_7d: 4, redact_names: false, activity: true, resume_points: 12, briefing: true, timetable: true, slots: 0, mail: true, mail_plain: true, github_watch: true, deploy_watch: true, google_connected: false, mail_items: 0, google_needs_signin: false, whatsapp: false, whatsapp_speak: false, leave_by: true, people: 0, session: true, health: true, profile: true, account_seed: true, routines: true, review: false, pending: 0, curator_note: String::new() };
        let md = card_markdown(&rows, &st, now);
        assert!(md.contains("### You told me") && md.contains("**dog name**"), "{md}");
        assert!(md.contains("\\[x\\]\\(http") || md.contains("\\[x\\]("), "markdown must be escaped: {md}");
        assert!(md.contains("### I picked up") && md.contains("last used 3d ago"), "{md}");
        assert!(md.contains("Recent conversations (1)") && md.contains("yesterday"), "{md}");
        assert!(md.contains("encrypted: **yes**") && md.contains("**4** turns"), "{md}");
        let empty = card_markdown(&[], &Status { enabled: true, encrypted: false, facts: 0, episodes: 0, egress_7d: 0, redact_names: false, activity: true, resume_points: 0, briefing: true, timetable: true, slots: 0, mail: true, mail_plain: true, github_watch: true, deploy_watch: true, google_connected: false, mail_items: 0, google_needs_signin: false, whatsapp: false, whatsapp_speak: false, leave_by: true, people: 0, session: true, health: true, profile: true, account_seed: true, routines: true, review: false, pending: 0, curator_note: String::new() }, now);
        assert!(empty.contains("Nothing yet"), "{empty}");
    }

    #[test]
    fn forget_and_wipe_reach_the_store() {
        let d = tmpdir("forget");
        mirror_core_fact(&d, "dog_name", "Bruno");
        mirror_learned_fact(&d, "employer", "Acme");
        assert_eq!(forget_key(&d, "dog_name"), 1);
        assert!(recall(&d, "dog", 5).unwrap().is_empty());
        assert_eq!(wipe(&d), 1);
        assert!(recall(&d, "employer", 5).unwrap().is_empty());
    }

    /// Recall eval (plan 95 `recall_fixtures`): a fixed fact set and queries
    /// with the expected top hit. Lexical retrieval cannot bridge vocabulary
    /// ("work" vs "employer"); that gap is pinned as an explicit known miss
    /// so nobody mistakes the ranking for semantic search.
    #[test]
    fn recall_fixtures_hit_at_1() {
        let d = tmpdir("fixtures");
        let facts = [
            ("favorite_pizza_topping", "pepperoni"),
            ("dog_name", "Bruno"),
            ("employer", "Acme Robotics"),
            ("city", "Pune"),
            ("birthday", "14 March"),
            ("favorite_color", "blue"),
            ("sister_name", "Asha"),
            ("wifi_router_location", "hallway shelf"),
            ("gym_days", "Monday Wednesday Friday"),
            ("allergic_to", "peanuts"),
            ("car_model", "Honda City"),
            ("college_name", "VIT"),
            ("favorite_music", "jazz"),
            ("coffee_order", "oat latte"),
        ];
        for (k, v) in facts {
            mirror_core_fact(&d, k, v);
        }
        let cases: &[(&str, Option<&str>)] = &[
            ("what pizza do I like", Some("favorite_pizza_topping")),
            ("what is my dog's name", Some("dog_name")),
            ("which city do I live in", Some("city")),
            ("when is my birthday", Some("birthday")),
            ("what is my favorite color", Some("favorite_color")),
            ("what is my sister called", Some("sister_name")),
            ("which days do I go to the gym", Some("gym_days")),
            ("what am I allergic to", Some("allergic_to")),
            ("which car do I drive", Some("car_model")),
            ("which college am I in", Some("college_name")),
            ("what music do I like", Some("favorite_music")),
            ("what is my coffee order", Some("coffee_order")),
            ("who is Bruno", Some("dog_name")),
            ("where is the router", Some("wifi_router_location")),
            ("peanut", Some("allergic_to")),
            // Was a known lexical gap (no shared word with "employer"); the synonym families close it (Memory v2, M1).
            ("where do I work", Some("employer")),
        ];
        let mut misses = vec![];
        for (q, want) in cases {
            let got = recall(&d, q, 3).unwrap();
            let top = got.first().map(|(k, _)| k.as_str());
            if top != *want {
                misses.push(format!("{q:?}: wanted {want:?}, got {top:?}"));
            }
        }
        assert!(misses.is_empty(), "recall misses:
{}", misses.join("
"));
    }
}

#[cfg(test)]
mod recall_baseline {
    //! Memory v2 plan, phases M0/M1: a fixed measurement of how well recall works, so every change
    //! (profile, synonyms, later aliases or embeddings) is judged against numbers instead of feel.
    //!
    //! 20 target facts hidden among 20 distractors, each asked two ways: `direct` (shares a word with the
    //! key or value, which lexical search can find) and `paraphrase` (shares none, which it cannot).
    //! M0 baseline (40-fact store): direct 20/21, paraphrase 0/19.
    use super::*;
    use std::time::Instant;

    const FACTS: &[(&str, &str)] = &[
        // targets
        ("dog_name", "Bruno"),
        ("employer", "Acme Corp"),
        ("favorite_pizza_topping", "pepperoni"),
        ("home_city", "Hyderabad"),
        ("sister_name", "Asha"),
        ("birthday", "12 March"),
        ("college", "IIT Delhi"),
        ("favorite_music_genre", "lo-fi"),
        ("wake_up_time", "6 am"),
        ("car", "Honda City"),
        ("allergy", "peanuts"),
        ("gym_days", "Monday Wednesday Friday"),
        ("manager_name", "Priya"),
        ("phone_model", "Pixel 8"),
        ("coffee_order", "oat milk latte"),
        ("programming_language", "Rust"),
        ("nickname", "Laks"),
        ("hometown", "Vijayawada"),
        ("favorite_movie", "Interstellar"),
        ("diet", "vegetarian"),
        // distractors
        ("shoe_size", "9"),
        ("wifi_name", "HomeNet"),
        ("laptop_brand", "HP"),
        ("favorite_color", "blue"),
        ("bank_branch", "Madhapur"),
        ("blood_group", "B positive"),
        ("tv_show", "Severance"),
        ("newspaper", "The Hindu"),
        ("favorite_sport", "cricket"),
        ("cousin_name", "Ravi"),
        ("favorite_fruit", "mango"),
        ("pet_peeve", "loud chewing"),
        ("commute_mode", "metro"),
        ("favorite_book", "Dune"),
        ("tea_preference", "no sugar"),
        ("language_spoken", "Telugu"),
        ("keyboard", "mechanical"),
        ("watch_brand", "Casio"),
        ("gaming_console", "PlayStation"),
        ("favorite_season", "monsoon"),
    ];

    /// (question, expected key, is_paraphrase)
    const QUESTIONS: &[(&str, &str, bool)] = &[
        ("what is my dog's name", "dog_name", false),
        ("what do I call my pet", "dog_name", true),
        ("who is my employer", "employer", false),
        ("where do I work", "employer", true),
        ("what pizza topping do I like", "favorite_pizza_topping", false),
        ("what do I put on my pizza", "favorite_pizza_topping", false),
        ("which city is my home", "home_city", false),
        ("where do I live", "home_city", true),
        ("what is my sister called", "sister_name", false),
        ("who is my sibling", "sister_name", true),
        ("when is my birthday", "birthday", false),
        ("when was I born", "birthday", true),
        ("which college am I in", "college", false),
        ("where do I study", "college", true),
        ("what music genre do I like", "favorite_music_genre", false),
        ("what do I listen to while studying", "favorite_music_genre", true),
        ("what time do I wake up", "wake_up_time", false),
        ("when do I get up in the morning", "wake_up_time", true),
        ("which car do I have", "car", false),
        ("what do I drive", "car", true),
        ("am I allergic to anything", "allergy", false),
        ("what can't I eat", "allergy", true),
        ("which days do I go to the gym", "gym_days", false),
        ("when do I work out", "gym_days", true),
        ("who is my manager", "manager_name", false),
        ("who is my boss", "manager_name", true),
        ("which phone do I use", "phone_model", false),
        ("what handset do I own", "phone_model", true),
        ("how do I take my coffee", "coffee_order", false),
        ("what do I drink in the morning", "coffee_order", true),
        ("which programming language do I use", "programming_language", false),
        ("what do I code in", "programming_language", true),
        ("what is my nickname", "nickname", false),
        ("what should you call me", "nickname", true),
        ("what is my hometown", "hometown", false),
        ("where am I from", "hometown", true),
        ("what is my favorite movie", "favorite_movie", false),
        ("what film do I love", "favorite_movie", true),
        ("what is my diet", "diet", false),
        ("do I eat meat", "diet", true),
    ];

    /// Written BEFORE the synonym families were built and not used to tune them: different wording from
    /// `QUESTIONS`. Some are expected to stay unanswered - that is the honest part of the measurement.
    /// (Same author as the families, so this is a smaller bias than tuning on the dev set, not none.)
    const HELD_OUT: &[(&str, &str)] = &[
        ("what is the name of my puppy", "dog_name"),
        ("which firm pays me", "employer"),
        ("what company am I employed by", "employer"),
        ("which town do I reside in", "home_city"),
        ("what is my brother or sister's name", "sister_name"),
        ("what is my date of birth", "birthday"),
        ("which university do I attend", "college"),
        ("what kind of songs do I enjoy", "favorite_music_genre"),
        ("when does my alarm go off", "wake_up_time"),
        ("which vehicle do I own", "car"),
        ("any food I should stay away from", "allergy"),
        ("how often do I exercise", "gym_days"),
        ("who do I report to at work", "manager_name"),
        ("which mobile do I carry", "phone_model"),
        ("what do I normally drink to wake up", "coffee_order"),
        ("which language do I write programs in", "programming_language"),
        ("what name do you use for me", "nickname"),
        ("where did I grow up", "hometown"),
        ("which movie do I like most", "favorite_movie"),
        ("do I avoid any kind of food", "diet"),
    ];

    fn tmpdir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("nexus_recall_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::create_dir_all(&p);
        p
    }

    fn present(pack: &str, key: &str) -> bool {
        pack.lines().any(|l| l.starts_with(&format!("- {key}: ")))
    }

    #[test]
    fn retrieval_hit_rates_against_the_m0_baseline() {
        let d = tmpdir("retrieval");
        for (k, v) in FACTS {
            mirror_core_fact(&d, k, v);
        }
        assert_eq!(QUESTIONS.len(), 40);
        let (mut direct, mut direct_n, mut para, mut para_n) = (0usize, 0usize, 0usize, 0usize);
        let mut misses: Vec<String> = vec![];
        for (q, key, is_para) in QUESTIONS {
            let hit = recall(&d, q, 3).expect("memcore").iter().any(|(k, _)| k == key);
            if *is_para {
                para_n += 1;
                para += hit as usize;
            } else {
                direct_n += 1;
                direct += hit as usize;
            }
            if !hit {
                misses.push(format!("{q} [{}] -> {key}", if *is_para { "paraphrase" } else { "direct" }));
            }
        }
        let mut held = 0usize;
        for (q, key) in HELD_OUT {
            let hit = recall(&d, q, 3).expect("memcore").iter().any(|(k, _)| k == key);
            held += hit as usize;
            if !hit {
                misses.push(format!("{q} [held-out] -> {key}"));
            }
        }
        println!("RETRIEVAL ONLY (40-fact store, hit@3)   M0 baseline -> now");
        println!("  direct            20/21 -> {direct}/{direct_n}");
        println!("  paraphrase (dev)   0/19 -> {para}/{para_n}");
        println!("  held-out           n/a  -> {held}/{}", HELD_OUT.len());
        for m in &misses {
            println!("  MISS {m}");
        }
        // Regression guards (never let the exact-word path get worse) plus the M1 goal on the dev set.
        assert!(direct >= 19, "direct recall regressed: {direct}/{direct_n}");
        assert!(para >= 1, "synonyms found nothing at all");
    }

    #[test]
    fn what_the_cloud_model_would_see_with_and_without_the_profile() {
        // 100 facts (targets, distractors, 60 fillers), interleaved deterministically, so the 1,400-char
        // profile cannot hold them all and selection matters.
        let d = tmpdir("pack100");
        let mut all: Vec<(String, String)> = FACTS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        for i in 0..60 {
            all.push((format!("note_{i:02}"), format!("saved detail number {i}")));
        }
        all.sort_by_key(|(k, _)| k.bytes().fold(7u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64)));
        for (k, v) in &all {
            mirror_core_fact(&d, k, v);
        }
        let questions: Vec<(&str, &str)> = QUESTIONS
            .iter()
            .map(|(q, k, _)| (*q, *k))
            .chain(HELD_OUT.iter().map(|(q, k)| (*q, *k)))
            .collect();

        let (mut without, mut with) = (0usize, 0usize);
        let mut micros: Vec<u128> = vec![];
        let mut max_chars = 0usize;
        for (q, key) in &questions {
            let off = context_pack_with(&d, q, 2600, false).unwrap_or_default();
            without += present(&off, key) as usize;
            let t = Instant::now();
            let on = context_pack_with(&d, q, 2600, true).unwrap_or_default();
            micros.push(t.elapsed().as_micros());
            with += present(&on, key) as usize;
            max_chars = max_chars.max(on.chars().count());
        }
        micros.sort_unstable();
        let p50 = micros[micros.len() / 2];
        let p95 = micros[(micros.len() * 95) / 100];
        let profile_chars = {
            let off = context_pack_with(&d, "zzz nothing matches", 2600, true).unwrap_or_default();
            off.chars().count()
        };
        println!("PACK PRESENCE (100-fact store, {} questions: dev + held-out)", questions.len());
        println!("  fact in pack, profile OFF: {without}/{}", questions.len());
        println!("  fact in pack, profile ON : {with}/{}", questions.len());
        println!("  context_pack (profile on): p50 {p50} us, p95 {p95} us, max {max_chars} chars (~{} tokens)", max_chars / 4);
        println!("  profile alone is {profile_chars} chars (cap {})", profile::PROFILE_CAP);

        assert!(with >= without, "the profile must never hide a fact");
        assert!(max_chars <= 2600, "pack over budget: {max_chars}");
        assert!(p95 < 100_000, "context_pack p95 {p95} us is far slower than expected (debug build)");
    }
}
