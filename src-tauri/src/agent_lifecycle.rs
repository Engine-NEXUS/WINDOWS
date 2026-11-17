//! One bounded, persistable task state machine for hooks and owned CLI watches.
//! No I/O, timers, OS assertions, model calls or executable actions live here.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, VecDeque};

pub const SETTLE_MS: u64 = 15_000;
pub const MAX_RUN_MS: u64 = 6 * 60 * 60_000;
const RETAIN_MS: u64 = 7 * 24 * 60 * 60_000;
const MAX_TASKS: usize = 256;
const MAX_EVENTS: usize = 2048;

pub fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for part in parts { h.update(part.len().to_le_bytes()); h.update(part.as_bytes()); }
    format!("{:x}", h.finalize())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Identity {
    pub session: Option<String>,
    pub execution: Option<String>,
    pub event: Option<String>,
    pub owned_watch: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind { Start, Busy, Wait, Stop }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase { Running, Waiting, Settling, Complete, Failed, Cancelled, Expired, Unknown }

impl Phase {
    pub fn active(self) -> bool { matches!(self, Self::Running | Self::Settling) }
    fn terminal(self) -> bool { matches!(self, Self::Complete | Self::Failed | Self::Cancelled | Self::Expired) }
}

pub struct Input<'a> {
    pub agent: &'a str,
    pub cwd: Option<&'a str>,
    pub identity: &'a Identity,
    pub kind: Kind,
    pub at_ms: u64,
    pub fully_idle: Option<bool>,
    pub ok: Option<bool>,
    pub signature: &'a str,
    /// A proper hook/adapter end, never merely file inactivity or a generic "Done" word.
    pub authoritative: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub key: String,
    pub scope: String,
    pub agent: String,
    pub phase: Phase,
    pub generation: u64,
    pub started_ms: u64,
    pub observed_ms: u64,
    pub stop_ms: Option<u64>,
    pub fingerprint: String,
    pub failed: bool,
    pub busy_veto: bool,
    /// Reserved durably before handing the completion to the notification/speech policy.
    pub claimed: bool,
}

impl Task {
    pub fn receipt(&self) -> String { digest(&[&self.key, &self.generation.to_string(), "terminal"]) }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registry {
    schema: u8,
    generation: u64,
    tasks: BTreeMap<String, Task>,
    events: VecDeque<String>,
}

impl Default for Registry {
    fn default() -> Self { Self { schema: 1, generation: 0, tasks: BTreeMap::new(), events: VecDeque::new() } }
}

pub fn scope(agent: &str, cwd: Option<&str>, id: &Identity) -> String {
    if let Some(watch) = &id.owned_watch { return digest(&[agent, "owned", watch]); }
    // Stable provider IDs take precedence over optional/changing workspace metadata.
    if let Some(session) = &id.session { return digest(&[agent, "session", session]); }
    digest(&[agent, cwd.unwrap_or(""), "legacy"])
}

impl Registry {
    pub fn observe(&mut self, i: Input<'_>) -> Option<String> {
        if i.agent.is_empty() || i.agent.len() > 64 { return None; }
        self.prune(i.at_ms);
        let scope = scope(i.agent, i.cwd, i.identity);
        let execution = if i.identity.owned_watch.is_some() { "owned" } else { i.identity.execution.as_deref().unwrap_or("legacy") };
        let key = digest(&[&scope, execution]);
        if let Some(event) = &i.identity.event {
            let event = digest(&[&key, event, &format!("{:?}", i.kind)]);
            if self.events.contains(&event) { return None; }
            self.events.push_back(event);
            while self.events.len() > MAX_EVENTS { self.events.pop_front(); }
        }
        let fresh = !self.tasks.contains_key(&key);
        if fresh {
            if self.tasks.len() >= MAX_TASKS { return None; }
            self.generation = self.generation.saturating_add(1);
            self.tasks.insert(key.clone(), Task { key: key.clone(), scope, agent: i.agent.into(), phase: Phase::Unknown,
                generation: self.generation, started_ms: i.at_ms, observed_ms: i.at_ms, stop_ms: None,
                fingerprint: String::new(), failed: false, busy_veto: false, claimed: false });
        }
        let t = self.tasks.get_mut(&key)?;
        // Reordered old events cannot rewind a newer state.
        if i.at_ms < t.observed_ms { return None; }
        if t.phase.terminal() {
            if i.kind != Kind::Start || i.identity.execution.is_some() || i.identity.owned_watch.is_some() { return None; }
            self.generation = self.generation.saturating_add(1);
            t.generation = self.generation;
            t.started_ms = i.at_ms;
            t.claimed = false;
            t.failed = false;
            t.busy_veto = false;
        }
        t.observed_ms = i.at_ms;
        match i.kind {
            Kind::Start | Kind::Busy => { t.phase = Phase::Running; t.stop_ms = None; }
            Kind::Wait => { t.phase = Phase::Waiting; t.stop_ms = None; }
            Kind::Stop => {
                if i.fully_idle == Some(false) {
                    t.phase = Phase::Running;
                    t.busy_veto = true;
                    t.stop_ms = None;
                } else if i.authoritative && (!t.busy_veto || i.fully_idle == Some(true)) {
                    t.busy_veto = false;
                    t.failed |= i.ok == Some(false);
                    t.fingerprint = digest(&[i.signature]);
                    // A repeated stop must not reset the quiet interval.
                    t.stop_ms.get_or_insert(i.at_ms);
                    t.phase = Phase::Settling;
                }
            }
        }
        Some(key)
    }

    pub fn tick(&mut self, now: u64) {
        for t in self.tasks.values_mut() {
            if !t.phase.terminal() && now.saturating_sub(t.started_ms) >= MAX_RUN_MS {
                t.phase = Phase::Expired;
            } else if t.phase == Phase::Settling && t.stop_ms.is_some_and(|s| now.saturating_sub(s) >= SETTLE_MS) {
                t.phase = if t.failed { Phase::Failed } else { Phase::Complete };
            }
        }
        self.prune(now);
    }
    pub fn active(&self) -> bool { self.tasks.values().any(|t| t.phase.active()) }
    pub fn busy_since(&self, agent: &str, since: u64) -> bool {
        self.tasks.values().any(|t| t.agent == agent && t.observed_ms >= since && t.phase.active())
    }
    pub fn ready(&self) -> Vec<Task> {
        self.tasks.values().filter(|t| !t.claimed && matches!(t.phase, Phase::Complete | Phase::Failed | Phase::Expired)).cloned().collect()
    }
    pub fn contains(&self, key: &str) -> bool { self.tasks.contains_key(key) }
    pub fn owned_phase(&self, agent: &str, watch: &str) -> Option<Phase> {
        let scope=digest(&[agent,"owned",watch]);
        self.tasks.values().find(|t|t.scope==scope).map(|t|t.phase)
    }
    pub fn terminal_since(&self, agent: &str, since: u64) -> Vec<Task> {
        self.tasks.values().filter(|t| t.agent==agent && t.observed_ms>=since && t.phase.terminal()).cloned().collect()
    }
    pub fn claim(&mut self, key: &str) -> Option<Task> {
        let t = self.tasks.get_mut(key)?;
        if t.claimed || !matches!(t.phase, Phase::Complete | Phase::Failed | Phase::Expired) { return None; }
        t.claimed = true;
        Some(t.clone())
    }
    pub fn unclaim(&mut self, key: &str) { if let Some(t) = self.tasks.get_mut(key) { t.claimed = false; } }
    pub fn cancel_owned(&mut self, watch: &str, at: u64) {
        for t in self.tasks.values_mut() {
            if t.scope == digest(&[&t.agent, "owned", watch]) && !t.claimed {
                t.phase = Phase::Cancelled; t.observed_ms = at; t.stop_ms=None;
            }
        }
    }
    fn prune(&mut self, now: u64) {
        self.tasks.retain(|_, t| !t.phase.terminal() || now.saturating_sub(t.observed_ms) <= RETAIN_MS);
        // Evict only old terminal rows, never another running task to admit a new one.
        while self.tasks.len() >= MAX_TASKS {
            let old = self.tasks.iter().filter(|(_, t)| t.phase.terminal() && t.claimed)
                .min_by_key(|(_, t)| t.observed_ms).map(|(k, _)| k.clone());
            if let Some(old) = old { self.tasks.remove(&old); } else { break; }
        }
    }
    pub fn encode(&self) -> Result<String, String> { serde_json::to_string(self).map_err(|e| e.to_string()) }
    pub fn restore(json: &str, now: u64) -> Result<Self, String> {
        if json.len() > 512 * 1024 { return Err("agent state too large".into()); }
        let mut s: Self = serde_json::from_str(json).map_err(|_| "invalid agent state")?;
        let hex = |v: &str| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit());
        if s.schema != 1 || s.tasks.len() > MAX_TASKS || s.events.len() > MAX_EVENTS ||
            s.tasks.iter().any(|(k, t)| !hex(k) || k != &t.key || !hex(&t.scope) || t.agent.len() > 64) ||
            s.events.iter().any(|e| !hex(e)) { return Err("invalid agent state bounds".into()); }
        for t in s.tasks.values_mut() {
            // Never keep the laptop awake solely because a pre-restart process used to run.
            if t.phase.active() || t.phase == Phase::Waiting { t.phase = Phase::Unknown; t.stop_ms = None; }
            if t.started_ms > now.saturating_add(300_000) { t.phase = Phase::Unknown; }
        }
        s.prune(now);
        Ok(s)
    }
}
