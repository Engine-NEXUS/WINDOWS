//! "Codex needs your input, sir": telling the user that a coding agent is waiting for them, and what happened while they
//! were away (doc: docs/research/capabilities-audit/01-agent-watch-keep-awake-sleep-wake.md, fixes 2 and 3).
//!
//! Before this, a waiting agent only counted as "still working": nothing was said and the screen stayed on in silence.
//! Two parts, both pure where it matters:
//!   * `Attention`: from "since when is each agent waiting for the user" and "how long has the user been idle" it decides
//!     who to tell, each wait exactly once (plus a reminder every 10 minutes while the user is still away, at most 3).
//!   * the away log: one line per agent about what it did while the user was gone ("Claude Code finished clean",
//!     "Codex needs your input"), kept in the sealed memory store and spoken ONCE when the user is back at the keyboard.
//!
//! An agent only reports a wait through its Notification hook (Claude Code, Gemini CLI, OpenCode, Devin, Kilo, Copilot);
//! Codex, Antigravity, Cursor, Windsurf and Trae cannot, so they are never reported here (no guessing).

use crate::agent_hooks::{self, Agent, ALL_AGENTS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Duration;

/// The user counts as away after this much keyboard and mouse silence.
pub const AWAY_MS: u64 = 30_000;
/// A wait must be visible this long before it is told (a prompt that is answered at once is not news).
const DEBOUNCE_MS: u64 = 2_000;
/// While the user stays away, a still-unanswered wait is mentioned again this often...
const REMIND_EVERY_MS: u64 = 10 * 60_000;
/// ...at most this many times.
const MAX_REMINDERS: u32 = 3;
/// Input within this long after the prompt arrived could be the user's own typing finishing a sentence elsewhere.
const INPUT_SLACK_MS: u64 = 5_000;
const POLL: Duration = Duration::from_secs(3);
const KEY: &str = "agent_away_log";
/// The log keeps the last day only.
const MAX_AGE_SECS: i64 = 24 * 3600;
const MAX_ENTRIES: usize = 30;

// ─── Who is waiting, and who to tell ─────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct WaitObs {
    pub name: String,
    pub label: String,
    /// Wall-clock ms when the agent's prompt arrived and it is still the latest thing it said; `None` = not waiting.
    pub waiting_since_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// First time this wait is told.
    New { label: String },
    /// Still unanswered after `minutes`.
    Still { label: String, minutes: u64 },
}

#[derive(Debug)]
struct Wait {
    since_ms: u64,
    told: u32,
    last_told_ms: u64,
}

#[derive(Debug, Default)]
pub struct Attention {
    waits: BTreeMap<String, Wait>,
}

impl Attention {
    /// `idle_ms` = how long the user has not touched keyboard or mouse (`None` = unknown: nothing is told).
    /// A wait is told only while the user is away AND has not touched anything since the prompt arrived: someone who
    /// answered it, or who is sitting right there, does not need to be told.
    pub fn step(&mut self, now_ms: u64, idle_ms: Option<u64>, obs: &[WaitObs]) -> Vec<Notice> {
        let mut out = Vec::new();
        self.waits.retain(|name, _| obs.iter().any(|o| &o.name == name && o.waiting_since_ms.is_some()));
        let Some(idle) = idle_ms else { return out };
        for o in obs {
            let Some(since) = o.waiting_since_ms else { continue };
            let w = self.waits.entry(o.name.clone()).or_insert(Wait { since_ms: since, told: 0, last_told_ms: 0 });
            if w.since_ms != since {
                *w = Wait { since_ms: since, told: 0, last_told_ms: 0 };
            }
            let age = now_ms.saturating_sub(since);
            let untouched_since_prompt = idle + INPUT_SLACK_MS >= age;
            if idle < AWAY_MS || !untouched_since_prompt || age < DEBOUNCE_MS {
                continue;
            }
            if w.told == 0 {
                w.told = 1;
                w.last_told_ms = now_ms;
                out.push(Notice::New { label: o.label.clone() });
            } else if w.told <= MAX_REMINDERS && now_ms.saturating_sub(w.last_told_ms) >= REMIND_EVERY_MS {
                w.told += 1;
                w.last_told_ms = now_ms;
                out.push(Notice::Still { label: o.label.clone(), minutes: age / 60_000 });
            }
        }
        out
    }
}

/// What goes into the away log for a notice.
pub fn needs_fragment(label: &str) -> String {
    format!("{label} needs your input")
}

// ─── The away log ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Unix seconds.
    pub at: i64,
    pub label: String,
    /// "Claude Code finished clean", "Codex needs your input": no trailing full stop, no "sir".
    pub text: String,
}

/// Add an entry; an agent's older entry is replaced (only its latest state matters), the log is capped.
pub fn push(log: &mut Vec<Entry>, entry: Entry, now: i64) {
    log.retain(|e| e.label != entry.label && now - e.at <= MAX_AGE_SECS);
    log.push(entry);
    while log.len() > MAX_ENTRIES {
        log.remove(0);
    }
}

/// The sentence spoken when the user is back, or `None` when there is nothing to say.
pub fn summary(log: &[Entry], now: i64) -> Option<String> {
    let fresh: Vec<&Entry> = log.iter().filter(|e| now - e.at >= 0 && now - e.at <= MAX_AGE_SECS).collect();
    if fresh.is_empty() {
        return None;
    }
    // what needs the user comes first
    let (mut needs, rest): (Vec<&Entry>, Vec<&Entry>) = fresh.into_iter().partition(|e| e.text.contains("needs your input"));
    needs.extend(rest);
    let shown: Vec<&str> = needs.iter().take(6).map(|e| e.text.as_str()).collect();
    let more = needs.len().saturating_sub(shown.len());
    let mut s = format!("While you were away, sir: {}", shown.join(". "));
    if more > 0 {
        s.push_str(&format!(". And {more} more"));
    }
    s.push('.');
    Some(s)
}

fn load(dir: &std::path::Path) -> Vec<Entry> {
    crate::memcore::with_store(dir, |s| s.meta_get(KEY)).flatten().and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
}

fn save(dir: &std::path::Path, log: &[Entry]) {
    if let Ok(json) = serde_json::to_string(log) {
        crate::memcore::with_store(dir, |s| s.meta_set(KEY, &json));
    }
}

/// Remember something an agent did while the user is away.
pub fn remember(dir: &std::path::Path, label: &str, text: &str) {
    let now = chrono::Utc::now().timestamp();
    let mut log = load(dir);
    push(&mut log, Entry { at: now, label: label.to_string(), text: text.to_string() }, now);
    save(dir, &log);
}

/// The log for the user who just came back; it is emptied so it is told once.
pub fn take_entries(dir: &std::path::Path) -> Vec<Entry> {
    let log = load(dir);
    if !log.is_empty() {
        save(dir, &[]);
    }
    log
}

/// Whether a user who was away for `away_ms` gets a spoken welcome-back summary. A short absence is told quietly
/// (a toast per agent, no speech, no greeting): `agentMonitorWelcomeAfterMin`, default 20 minutes.
pub fn welcome_due(away_ms: u64, welcome_after_min: u32) -> bool {
    away_ms >= u64::from(welcome_after_min) * 60_000
}

fn welcome_after_min(dir: &std::path::Path) -> u32 {
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|j| j.get("agentMonitorWelcomeAfterMin").and_then(|v| v.as_u64()))
        .map_or(20, |m| m.min(24 * 60) as u32)
}

// ─── The loop ────────────────────────────────────────────────────────

fn idle_ms() -> Option<u64> {
    crate::memcore::resume::user_idle_secs().map(|s| s * 1000)
}

/// Pure: whether the user is currently away from the PC based on idle duration.
pub fn is_away(idle_ms: Option<u64>) -> bool {
    idle_ms.map_or(false, |i| i >= AWAY_MS)
}

/// Pure: whether an agent alert toast should be published (only when user is present, so the away summary is not duplicated).
pub fn should_publish_toast(away: bool) -> bool {
    !away
}

/// How long a prompt stays unanswered before a present user gets the toast (someone typing into the agent answers sooner).
const TOAST_AFTER_MS: u64 = 15_000;

/// Pure: should the "needs your input" toast be published now? Only for a user who is present, once per prompt
/// (`last` = the prompt time already toasted), and only after the prompt has waited `TOAST_AFTER_MS`.
pub fn toast_due(since_ms: u64, now_ms: u64, away: bool, last: Option<u64>) -> bool {
    should_publish_toast(away) && now_ms.saturating_sub(since_ms) >= TOAST_AFTER_MS && last != Some(since_ms)
}

/// Poll the agents' hooks; tell the user about a wait once, remember it for the return, and speak the log when the
/// user is back. Cheap: nothing runs when no agent has ever reported a prompt.
pub fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    use tauri::Manager;
    tauri::async_runtime::spawn(async move {
        let started = agent_hooks::now_ms();
        let mut attention = Attention::default();
        let mut was_away = false;
        let mut away_since: Option<u64> = None;
        let mut stops: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut toasted: BTreeMap<&'static str, u64> = BTreeMap::new();
        loop {
            tokio::time::sleep(POLL).await;
            let now = agent_hooks::now_ms();
            let idle = idle_ms();
            let away = is_away(idle);
            let obs: Vec<WaitObs> = ALL_AGENTS
                .iter()
                .map(|a| WaitObs { name: a.name().to_string(), label: a.label().to_string(), waiting_since_ms: waiting_since(*a, started) })
                .collect();
            let Ok(dir) = app.path().app_data_dir() else { continue };
            for n in attention.step(now, idle, &obs) {
                match n {
                    Notice::New { label } => {
                        // (An away user is told on return; the toast below is for a user who is present.)
                        remember(&dir, &label, &needs_fragment(&label));
                    }
                    // the reminder keeps the same log line; it is spoken at the return like the first
                    Notice::Still { label, .. } => remember(&dir, &label, &needs_fragment(&label)),
                }
            }
            // A user who is at the PC and has left a prompt unanswered for a while gets a heads-up toast, once per prompt
            // (`Attention::step` above only tells users who are away, so it never reaches this case).
            for o in &obs {
                let Some(since) = o.waiting_since_ms else { continue };
                if !toast_due(since, now, away, toasted.get(o.name.as_str()).copied()) {
                    continue;
                }
                if let Some(agent) = ALL_AGENTS.iter().find(|a| a.name() == o.name) {
                    toasted.insert(agent.name(), since);
                    let cwd = agent_hooks::last_cwd(*agent);
                    crate::notify_hub::publish_agent(&app, agent.name(), agent.label(), cwd.as_deref(), true, since);
                }
            }
            // a finish while nobody is here is remembered even when no plan is watching (a plan's own, richer line
            // replaces this one for the same agent a few seconds later)
            for a in ALL_AGENTS.iter() {
                let Some(stop) = agent_hooks::last_event_ms(*a, agent_hooks::EventKind::Stop).filter(|t| *t > started) else { continue };
                if stops.get(a.name()).map_or(true, |seen| stop > *seen) {
                    stops.insert(a.name(), stop);
                    if agent_hooks::stop_is_latest(*a, started) {
                        if away {
                            remember(&dir, a.label(), &format!("{} has finished", a.label()));
                        } else if should_publish_toast(away) {
                            let cwd = agent_hooks::last_cwd(*a);
                            crate::notify_hub::publish_agent(&app, a.name(), a.label(), cwd.as_deref(), false, stop);
                        }
                    }
                }
            }
            if away && !was_away {
                away_since = Some(now);
            }
            if was_away && !away {
                let away_ms = away_since.take().map_or(0, |t| now.saturating_sub(t));
                // the user is back: tell it once, after the hello if there is one
                tokio::time::sleep(Duration::from_secs(4)).await;
                let entries = take_entries(&dir);
                if welcome_due(away_ms, welcome_after_min(&dir)) {
                    // away long enough for a welcome-back: the spoken summary
                    if let Some(line) = summary(&entries, chrono::Utc::now().timestamp()) {
                        crate::proactive_policy::submit(
                            &app,
                            format!("agent_away_{}", chrono::Utc::now().timestamp()),
                            line,
                            crate::google::types::AlertUrgency::High,
                        );
                    }
                } else {
                    // a short absence: no welcome and no speech, one quiet toast per agent
                    for e in &entries {
                        let Some(agent) = ALL_AGENTS.iter().find(|a| a.label() == e.label) else { continue };
                        let needs = e.text.contains("needs your input") || e.text.contains("permission");
                        crate::notify_hub::publish_agent(&app, agent.name(), agent.label(), agent_hooks::last_cwd(*agent).as_deref(), needs, now);
                    }
                }
            }
            was_away = away;
        }
    });
}

/// When did this agent's latest prompt arrive, if it is still waiting for the user?
fn waiting_since(agent: Agent, since_ms: u64) -> Option<u64> {
    if !agent_hooks::needs_user(agent, since_ms) {
        return None;
    }
    agent_hooks::last_event_ms(agent, agent_hooks::EventKind::Notification)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(name: &str, label: &str, since: Option<u64>) -> WaitObs {
        WaitObs { name: name.into(), label: label.into(), waiting_since_ms: since }
    }

    const T: u64 = 1_000_000;

    #[test]
    fn a_welcome_back_needs_a_long_enough_absence() {
        let min = 60_000;
        assert!(!welcome_due(19 * min + 59_000, 20), "under 20 minutes: no welcome");
        assert!(welcome_due(20 * min, 20), "20 minutes: welcome");
        assert!(welcome_due(3 * 60 * min, 20));
        assert!(welcome_due(0, 0), "0 means always welcome");
        assert!(!welcome_due(0, 20));
    }

    #[test]
    fn an_unanswered_wait_is_told_once_when_the_user_is_away() {
        let mut a = Attention::default();
        let obs = [waiting("claude", "Claude Code", Some(T))];
        assert!(a.step(T + 1_000, Some(120_000), &obs).is_empty(), "too soon: a prompt answered at once is not news");
        let first = a.step(T + 5_000, Some(125_000), &obs);
        assert_eq!(first, vec![Notice::New { label: "Claude Code".into() }]);
        assert!(a.step(T + 8_000, Some(128_000), &obs).is_empty(), "told once");
    }

    #[test]
    fn nothing_is_told_while_the_user_is_at_the_keyboard() {
        let mut a = Attention::default();
        let obs = [waiting("codex", "Codex", Some(T))];
        assert!(a.step(T + 10_000, Some(2_000), &obs).is_empty());
        assert!(a.step(T + 20_000, None, &obs).is_empty(), "unknown idle time tells nothing");
    }

    #[test]
    fn a_prompt_the_user_already_answered_is_not_told() {
        let mut a = Attention::default();
        let obs = [waiting("claude", "Claude Code", Some(T))];
        // the user typed 20 s ago, the prompt came 60 s ago: input happened after it, so it was probably answered
        assert!(a.step(T + 60_000, Some(40_000), &obs).is_empty());
    }

    #[test]
    fn a_wait_that_ends_is_forgotten_and_a_new_one_is_told_again() {
        let mut a = Attention::default();
        let w1 = [waiting("claude", "Claude Code", Some(T))];
        assert_eq!(a.step(T + 5_000, Some(125_000), &w1).len(), 1);
        assert!(a.step(T + 9_000, Some(129_000), &[waiting("claude", "Claude Code", None)]).is_empty());
        let w2 = [waiting("claude", "Claude Code", Some(T + 600_000))];
        assert_eq!(a.step(T + 606_000, Some(726_000), &w2).len(), 1, "a second prompt is a second wait");
    }

    #[test]
    fn reminders_come_every_ten_minutes_at_most_three_times() {
        let mut a = Attention::default();
        let obs = [waiting("claude", "Claude Code", Some(T))];
        assert_eq!(a.step(T + 5_000, Some(125_000), &obs).len(), 1);
        let mut reminders = 0;
        let mut t = T + 5_000;
        for _ in 0..80 {
            t += 60_000;
            let idle = 120_000 + (t - T);
            for n in a.step(t, Some(idle), &obs) {
                assert!(matches!(n, Notice::Still { .. }));
                reminders += 1;
            }
        }
        assert_eq!(reminders, MAX_REMINDERS as usize);
    }

    #[test]
    fn several_agents_are_told_separately() {
        let mut a = Attention::default();
        let obs = [waiting("claude", "Claude Code", Some(T)), waiting("opencode", "OpenCode", Some(T + 1_000)), waiting("codex", "Codex", None)];
        let got = a.step(T + 10_000, Some(130_000), &obs);
        assert_eq!(got.len(), 2);
    }

    fn entry(at: i64, label: &str, text: &str) -> Entry {
        Entry { at, label: label.into(), text: text.into() }
    }

    #[test]
    fn the_summary_puts_what_needs_the_user_first_and_keeps_the_latest_state_per_agent() {
        let mut log = Vec::new();
        push(&mut log, entry(100, "Codex", "Codex needs your input"), 100);
        push(&mut log, entry(110, "Claude Code", "Claude Code finished clean"), 110);
        push(&mut log, entry(120, "Codex", "Codex finished clean"), 120); // it was answered and finished
        push(&mut log, entry(130, "OpenCode", "OpenCode needs your input"), 130);
        assert_eq!(log.len(), 3, "one line per agent");
        let s = summary(&log, 200).unwrap();
        assert_eq!(s, "While you were away, sir: OpenCode needs your input. Claude Code finished clean. Codex finished clean.");
    }

    #[test]
    fn an_empty_or_old_log_says_nothing() {
        assert_eq!(summary(&[], 100), None);
        assert_eq!(summary(&[entry(0, "Codex", "Codex finished clean")], MAX_AGE_SECS + 1), None);
    }

    #[test]
    fn a_long_log_is_shortened_honestly() {
        let mut log = Vec::new();
        for i in 0..8 {
            push(&mut log, entry(100 + i, &format!("A{i}"), &format!("A{i} finished clean")), 100 + i);
        }
        let s = summary(&log, 200).unwrap();
        assert!(s.ends_with("And 2 more."), "{s}");
    }

    static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_log_survives_a_restart_and_is_told_exactly_once() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("nexus_attention_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        remember(&dir, "Claude Code", "Claude Code finished clean");
        remember(&dir, "Codex", &needs_fragment("Codex"));
        let entries = take_entries(&dir);
        let said = summary(&entries, chrono::Utc::now().timestamp()).expect("something to say");
        assert!(said.starts_with("While you were away, sir: Codex needs your input."), "{said}");
        assert!(take_entries(&dir).is_empty(), "told once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_needs_input_toast_is_for_a_present_user_once_per_prompt_after_a_delay() {
        assert!(!toast_due(1_000, 5_000, false, None), "too soon: the user may be answering");
        assert!(toast_due(1_000, 16_000, false, None));
        assert!(!toast_due(1_000, 16_000, false, Some(1_000)), "already toasted this prompt");
        assert!(toast_due(1_000, 16_000, false, Some(500)), "a newer prompt toasts again");
        assert!(!toast_due(1_000, 99_000, true, None), "an away user hears it on return instead");
    }

    #[test]
    fn should_publish_toast_and_is_away_helpers() {
        // User is away when idle_ms >= AWAY_MS (30_000)
        assert!(!is_away(None));
        assert!(!is_away(Some(0)));
        assert!(!is_away(Some(10_000)));
        assert!(!is_away(Some(29_999)));
        assert!(is_away(Some(30_000)));
        assert!(is_away(Some(60_000)));

        // Toasts are published ONLY when user is present (!away)
        assert!(should_publish_toast(false), "present user gets real-time toast");
        assert!(!should_publish_toast(true), "away user toast is suppressed so return summary is not duplicated");
    }
}
