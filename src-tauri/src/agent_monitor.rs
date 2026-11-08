//! Agent monitor: which coding agents are running right now, what they are doing, and the permission questions they
//! are waiting on. Feeds the Agent Island window (agent_island.rs) and keeps the screen awake while any agent works.
//!
//! The design follows Louis-CFM/coucou (MIT, see THIRD_PARTY_NOTICES.md): agents report through their own hooks,
//! a permission request holds the hook open until a human clicks Allow / Deny, and with no answer the hook prints
//! nothing so the agent asks in its own terminal. Unlike coucou the transport is NEXUS's authenticated loopback
//! webhook (webhook.rs), not a named pipe.
//!
//! Pure parts (tested): `parse`, `Monitor`, `summarize`, `Power`. Impure parts: the broker statics, the loop.

use crate::agent_hooks::Agent;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// A finished agent stays in the snapshot this long so the page can fade its logo out.
pub const DONE_LINGER_MS: u64 = 1_500;
/// A "running" session that has said nothing for this long is treated as lost (a Stop that never arrived).
pub const STALE_MS: u64 = 15 * 60_000;
/// How long a permission request waits for a click before the agent is left to ask in its own terminal.
pub const APPROVAL_WAIT: Duration = Duration::from_secs(108);
const MAX_SUMMARY_CHARS: usize = 600;
const MAX_APPROVALS: usize = 8;
const MAX_SESSIONS: usize = 24;
/// The tool steps kept per session (the island shows a short timeline, not a log).
const MAX_STEPS: usize = 12;
const MAX_DETAIL_CHARS: usize = 60;
/// The old and new text of one edit the island may show. Over this the diff is only flagged "too large" (the same
/// guard coucou uses on its diff engine, scaled down: the text travels to the page on every update).
const MAX_EDIT_BYTES: usize = 64 * 1024;
/// After the last agent stops, the screen is held this much longer (a Stop is often followed by the next prompt).
pub const SETTLE_MS: u64 = 10_000;
/// "Blank the display when done" is retried for this long while the user is still at the PC, then dropped.
pub const BLANK_GIVE_UP_MS: u64 = 15 * 60_000;

// ─── Sessions ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Thinking,
    Working,
    Waiting,
    Done,
}

impl State {
    pub fn running(self) -> bool {
        matches!(self, State::Thinking | State::Working | State::Waiting)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StepState {
    Running,
    Done,
}

/// One tool call of the current turn: "Read", "Edit", "Bash", ... and what it touches.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub tool: String,
    pub detail: Option<String>,
    pub state: StepState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditPair {
    pub old: String,
    pub new: String,
}

/// What an Edit / MultiEdit / Write is about to change, for the live diff. The page computes the line diff.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditView {
    pub path: String,
    /// "edit", "multi" or "write".
    pub kind: &'static str,
    pub pairs: Vec<EditPair>,
    pub too_large: bool,
}

/// The tool call a PreToolUse hook announces.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ToolUse {
    pub name: Option<String>,
    pub detail: Option<String>,
    pub edit: Option<EditView>,
}

impl ToolUse {
    #[cfg(test)]
    pub fn named(name: &str) -> Self {
        Self { name: Some(name.to_string()), ..Self::default() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub key: String,
    pub agent: String,
    pub label: String,
    pub cwd: Option<String>,
    pub state: State,
    pub tool: Option<String>,
    pub steps: Vec<Step>,
    pub edit: Option<EditView>,
    pub started_ms: u64,
    pub updated_ms: u64,
    pub ended_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Approval {
    pub id: u64,
    pub session_key: String,
    pub agent: String,
    pub label: String,
    pub tool: String,
    pub summary: String,
    pub question: Option<Value>,
    pub requested_ms: u64,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Prompt,
    PreTool(ToolUse),
    PostTool,
    Stop,
    Notify,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ev {
    pub agent: Agent,
    pub session_id: String,
    pub cwd: Option<String>,
    pub kind: Kind,
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

fn clean_token(s: &str, max: usize) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/')).take(max).collect()
}

fn session_id_of(json: Option<&Value>) -> String {
    json.and_then(|v| {
        ["session_id", "sessionId", "conversation_id", "id"].iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()))
    })
    .map(|s| clean_token(s, 64))
    .filter(|s| !s.is_empty())
    .unwrap_or_else(|| "main".to_string())
}

fn cwd_of(json: Option<&Value>) -> Option<String> {
    json.and_then(|v| v.get("cwd").and_then(|c| c.as_str()))
        .map(|c| c.chars().filter(|ch| !ch.is_control()).take(260).collect::<String>())
        .filter(|c| !c.trim().is_empty())
}

fn last_component(path: &str) -> &str {
    path.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().unwrap_or(path)
}

/// A short, masked "what it touches" for a step: the command, the file name, the URL, the query.
fn detail_of(input: Option<&Value>) -> Option<String> {
    let input = input?;
    let pick = |k: &str| input.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty());
    let raw = if let Some(c) = pick("command") {
        c.lines().next().unwrap_or(c).to_string()
    } else if let Some(f) = pick("file_path").or_else(|| pick("path")) {
        last_component(f).to_string()
    } else {
        pick("url").or_else(|| pick("query")).or_else(|| pick("pattern")).map(str::to_string)?
    };
    let masked = crate::session_verdict::prepare_tail(&raw);
    let one: String = masked.chars().filter(|c| !c.is_control()).take(MAX_DETAIL_CHARS).collect();
    Some(one).filter(|d| !d.trim().is_empty())
}

/// The old and new text of an Edit, MultiEdit or Write, or `None` for any other tool.
fn edit_of(name: &str, input: Option<&Value>) -> Option<EditView> {
    let input = input?;
    let text = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let path: String = text(input, "file_path")?.chars().filter(|c| !c.is_control()).take(260).collect();
    let (kind, pairs): (&'static str, Vec<EditPair>) = match name {
        "Edit" => ("edit", vec![EditPair { old: text(input, "old_string")?, new: text(input, "new_string")? }]),
        "MultiEdit" => (
            "multi",
            input
                .get("edits")?
                .as_array()?
                .iter()
                .filter_map(|e| Some(EditPair { old: text(e, "old_string")?, new: text(e, "new_string")? }))
                .collect(),
        ),
        "Write" => ("write", vec![EditPair { old: String::new(), new: text(input, "content")? }]),
        _ => return None,
    };
    if pairs.is_empty() || pairs.iter().all(|p| p.old.is_empty() && p.new.is_empty()) {
        return None;
    }
    let bytes: usize = pairs.iter().map(|p| p.old.len() + p.new.len()).sum();
    if bytes > MAX_EDIT_BYTES {
        return Some(EditView { path, kind, pairs: Vec::new(), too_large: true });
    }
    Some(EditView { path, kind, pairs, too_large: false })
}

fn tool_use_of(json: Option<&Value>) -> ToolUse {
    let name = json.and_then(|v| v.get("tool_name")).and_then(|t| t.as_str()).map(|t| clean_token(t, 40)).filter(|t| !t.is_empty());
    let input = json.and_then(|v| v.get("tool_input"));
    let edit = name.as_deref().and_then(|n| edit_of(n, input));
    ToolUse { detail: detail_of(input), edit, name }
}

/// `?agent=claude&event=pretool` plus the hook's stdin JSON -> a monitor event. Events the monitor does not use
/// ("message") and unknown agents give `None`. Nothing in the body ever becomes a command.
pub fn parse(query: &str, body: &str) -> Option<Ev> {
    let json: Option<Value> = serde_json::from_str(body).ok();
    let agent = Agent::from_name(query_value(query, "agent")?)?;
    let kind = match query_value(query, "event")? {
        "start" => Kind::Prompt,
        "pretool" => Kind::PreTool(tool_use_of(json.as_ref())),
        "posttool" => Kind::PostTool,
        "stop" => Kind::Stop,
        "notification" => Kind::Notify,
        "end" => Kind::End,
        _ => return None,
    };
    Some(Ev { agent, session_id: session_id_of(json.as_ref()), cwd: cwd_of(json.as_ref()), kind })
}

/// Events only the monitor uses: the older tracker has no kind for them, so the webhook answers them itself.
pub fn is_monitor_only(query: &str) -> bool {
    matches!(query_value(query, "event"), Some("pretool" | "posttool" | "end"))
}

// ─── The reducer ─────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct Monitor {
    sessions: Vec<Session>,
    approvals: Vec<Approval>,
    next_id: u64,
}

impl Monitor {
    pub const fn new() -> Self {
        Self { sessions: Vec::new(), approvals: Vec::new(), next_id: 1 }
    }

    fn index(&self, key: &str) -> Option<usize> {
        self.sessions.iter().position(|s| s.key == key)
    }

    fn upsert(&mut self, agent: Agent, session_id: &str, now: u64) -> usize {
        let key = format!("{}:{}", agent.name(), session_id);
        if let Some(i) = self.index(&key) {
            return i;
        }
        if self.sessions.len() >= MAX_SESSIONS {
            // make room: the oldest session that is no longer running, else the oldest
            let drop_at = self.sessions.iter().position(|s| !s.state.running()).unwrap_or(0);
            self.sessions.remove(drop_at);
        }
        self.sessions.push(Session {
            key,
            agent: agent.name().to_string(),
            label: agent.label().to_string(),
            cwd: None,
            state: State::Thinking,
            tool: None,
            steps: Vec::new(),
            edit: None,
            started_ms: now,
            updated_ms: now,
            ended_ms: None,
        });
        self.sessions.len() - 1
    }

    pub fn apply(&mut self, ev: &Ev, now: u64) {
        let key = format!("{}:{}", ev.agent.name(), ev.session_id);
        match ev.kind {
            Kind::End => {
                self.sessions.retain(|s| s.key != key);
                self.approvals.retain(|a| a.session_key != key);
                return;
            }
            // A Stop for an agent we never saw start (NEXUS began after it) says nothing worth a logo.
            Kind::Stop if self.index(&key).is_none() => return,
            _ => {}
        }
        let i = self.upsert(ev.agent, &ev.session_id, now);
        let has_approval = self.approvals.iter().any(|a| a.session_key == key);
        let s = &mut self.sessions[i];
        let was_running = s.state.running();
        if let Some(c) = &ev.cwd {
            s.cwd = Some(c.clone());
        }
        match &ev.kind {
            Kind::Prompt => {
                s.state = State::Thinking;
                s.tool = None;
                // a new turn: a new timeline
                s.steps.clear();
                s.edit = None;
                s.ended_ms = None;
                // a new run after a finished one restarts the clock
                if !was_running {
                    s.started_ms = now;
                }
            }
            Kind::PreTool(tool) => {
                if !has_approval {
                    s.state = State::Working;
                }
                s.tool = tool.name.clone();
                s.ended_ms = None;
                // the call before this one has finished; this one is now running
                for st in s.steps.iter_mut() {
                    st.state = StepState::Done;
                }
                s.steps.push(Step { tool: tool.name.clone().unwrap_or_else(|| "Tool".into()), detail: tool.detail.clone(), state: StepState::Running });
                if s.steps.len() > MAX_STEPS {
                    s.steps.remove(0);
                }
                if tool.edit.is_some() {
                    s.edit = tool.edit.clone();
                }
            }
            Kind::PostTool => {
                if !has_approval {
                    s.state = State::Thinking;
                }
                s.tool = None;
            }
            Kind::Notify => s.state = State::Waiting,
            Kind::Stop => {
                s.state = State::Done;
                s.tool = None;
                for st in s.steps.iter_mut() {
                    st.state = StepState::Done;
                }
                s.steps.push(Step { tool: "Done".into(), detail: None, state: StepState::Done });
                if s.steps.len() > MAX_STEPS {
                    s.steps.remove(0);
                }
                s.ended_ms = Some(now);
                self.approvals.retain(|a| a.session_key != key);
            }
            Kind::End => unreachable!("handled above"),
        }
        s.updated_ms = now;
    }

    /// A permission request arrived. `None` when too many are already waiting (the agent then asks in its terminal).
    pub fn add_approval(&mut self, agent: Agent, session_id: &str, cwd: Option<&str>, tool: String, summary: String, question: Option<Value>, now: u64, wait: Duration) -> Option<u64> {
        if self.approvals.len() >= MAX_APPROVALS {
            return None;
        }
        let i = self.upsert(agent, session_id, now);
        let s = &mut self.sessions[i];
        s.state = State::Waiting;
        s.updated_ms = now;
        s.ended_ms = None;
        if let Some(c) = cwd {
            s.cwd = Some(c.to_string());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.approvals.push(Approval {
            id,
            session_key: s.key.clone(),
            agent: agent.name().to_string(),
            label: agent.label().to_string(),
            tool,
            summary,
            question,
            requested_ms: now,
            deadline_ms: now + wait.as_millis() as u64,
        });
        Some(id)
    }

    /// The request is over (answered, timed out, or the agent stopped). The session goes back to thinking.
    pub fn remove_approval(&mut self, id: u64, now: u64) -> bool {
        let Some(pos) = self.approvals.iter().position(|a| a.id == id) else { return false };
        let a = self.approvals.remove(pos);
        let still = self.approvals.iter().any(|x| x.session_key == a.session_key);
        if let Some(i) = self.index(&a.session_key) {
            let s = &mut self.sessions[i];
            if s.state == State::Waiting && !still {
                s.state = State::Thinking;
                s.updated_ms = now;
            }
        }
        true
    }

    pub fn prune(&mut self, now: u64) {
        let pending: Vec<String> = self.approvals.iter().map(|a| a.session_key.clone()).collect();
        self.sessions.retain(|s| match s.ended_ms {
            Some(t) if !s.state.running() => now.saturating_sub(t) < DONE_LINGER_MS,
            _ => pending.contains(&s.key) || now.saturating_sub(s.updated_ms) < STALE_MS,
        });
        self.approvals.retain(|a| now < a.deadline_ms + 5_000);
    }

    pub fn running(&self) -> usize {
        self.sessions.iter().filter(|s| s.state.running()).count()
    }

    #[cfg(test)]
    pub fn approvals_len(&self) -> usize {
        self.approvals.len()
    }

    pub fn snapshot(&self, now: u64) -> Snapshot {
        Snapshot {
            running: self.running(),
            at_ms: now,
            sessions: self
                .sessions
                .iter()
                .map(|s| SessionView {
                    key: s.key.clone(),
                    agent: s.agent.clone(),
                    label: s.label.clone(),
                    folder: s.cwd.as_deref().and_then(folder_of),
                    state: s.state,
                    tool: s.tool.clone(),
                    steps: s.steps.iter().map(|x| StepView { tool: x.tool.clone(), detail: x.detail.clone(), state: x.state }).collect(),
                    edit: s.edit.clone(),
                    started_ms: s.started_ms,
                    updated_ms: s.updated_ms,
                })
                .collect(),
            approvals: self
                .approvals
                .iter()
                .map(|a| ApprovalView {
                    id: a.id,
                    agent: a.agent.clone(),
                    label: a.label.clone(),
                    tool: a.tool.clone(),
                    summary: a.summary.clone(),
                    question: a.question.clone(),
                    requested_ms: a.requested_ms,
                    deadline_ms: a.deadline_ms,
                })
                .collect(),
        }
    }
}

/// The last folder name of a path, for display ("D:\\work\\zync" -> "zync").
fn folder_of(cwd: &str) -> Option<String> {
    cwd.trim_end_matches(['\\', '/']).rsplit(['\\', '/']).next().map(|s| s.to_string()).filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub key: String,
    pub agent: String,
    pub label: String,
    pub folder: Option<String>,
    pub state: State,
    pub tool: Option<String>,
    pub steps: Vec<StepView>,
    pub edit: Option<EditView>,
    pub started_ms: u64,
    pub updated_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub tool: String,
    pub detail: Option<String>,
    pub state: StepState,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalView {
    pub id: u64,
    pub agent: String,
    pub label: String,
    pub tool: String,
    pub summary: String,
    pub question: Option<Value>,
    pub requested_ms: u64,
    pub deadline_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub running: usize,
    pub at_ms: u64,
    pub sessions: Vec<SessionView>,
    pub approvals: Vec<ApprovalView>,
}

/// One line saying what the tool wants to do. Secrets are masked and the text is cut: this is shown on screen
/// (possibly shared) and never stored.
pub fn summarize(tool: &str, input: Option<&Value>) -> String {
    let pick = |k: &str| input.and_then(|i| i.get(k)).and_then(|v| v.as_str()).map(String::from);
    let raw = match tool {
        "Bash" | "PowerShell" => pick("command"),
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "Read" => pick("file_path"),
        "WebFetch" => pick("url"),
        "WebSearch" => pick("query"),
        _ => None,
    }
    .or_else(|| input.map(|i| i.to_string()))
    .unwrap_or_default();
    let masked = crate::session_verdict::prepare_tail(&raw);
    masked.chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_SUMMARY_CHARS).collect()
}

// ─── Power: keep the screen on while an agent works ──────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    /// Release the screen hold and let the OS sleep rules apply.
    StayAwake,
    /// Also blank the display once the agents are done, when the user has been away and no meeting is on.
    Blank,
}

impl DisplayMode {
    pub fn from_setting(s: &str) -> Self {
        if s.eq_ignore_ascii_case("blank") {
            DisplayMode::Blank
        } else {
            DisplayMode::StayAwake
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAct {
    Hold,
    Release,
    Blank,
}

pub struct PowerObs {
    pub now_ms: u64,
    pub running: usize,
    pub idle_secs: Option<u64>,
    pub meeting: bool,
}

#[derive(Debug, Default)]
pub struct Power {
    holding: bool,
    zero_since: Option<u64>,
    pending_blank_since: Option<u64>,
}

impl Power {
    /// `blank_ok(idle_secs, meeting)` is `window_tile::may_blank_display(..).is_ok()`; injected so tests never
    /// touch the display.
    pub fn step(&mut self, o: &PowerObs, mode: DisplayMode, blank_ok: impl Fn(Option<u64>, bool) -> bool) -> Vec<PowerAct> {
        let mut out = Vec::new();
        if o.running > 0 {
            self.zero_since = None;
            self.pending_blank_since = None;
            if !self.holding {
                self.holding = true;
                out.push(PowerAct::Hold);
            }
            return out;
        }
        if self.holding {
            let since = *self.zero_since.get_or_insert(o.now_ms);
            if o.now_ms.saturating_sub(since) >= SETTLE_MS {
                self.holding = false;
                self.zero_since = None;
                out.push(PowerAct::Release);
                if mode == DisplayMode::Blank {
                    self.pending_blank_since = Some(o.now_ms);
                }
            }
            return out;
        }
        if let Some(t) = self.pending_blank_since {
            if o.now_ms.saturating_sub(t) > BLANK_GIVE_UP_MS {
                self.pending_blank_since = None;
            } else if blank_ok(o.idle_secs, o.meeting) {
                self.pending_blank_since = None;
                out.push(PowerAct::Blank);
            }
        }
        out
    }
}

// ─── The broker: hold a permission request until someone answers ─────

struct Slot {
    decision: Mutex<Option<String>>,
    cv: Condvar,
}

static MONITOR: Mutex<Monitor> = Mutex::new(Monitor::new());
static WAITERS: Mutex<Vec<(u64, Arc<Slot>)>> = Mutex::new(Vec::new());
static DIRTY: AtomicBool = AtomicBool::new(true);
static ENABLED: AtomicBool = AtomicBool::new(true);
static DIR: OnceLock<std::path::PathBuf> = OnceLock::new();

fn lock_monitor() -> std::sync::MutexGuard<'static, Monitor> {
    MONITOR.lock().unwrap_or_else(|e| e.into_inner())
}

fn lock_waiters() -> std::sync::MutexGuard<'static, Vec<(u64, Arc<Slot>)>> {
    WAITERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called by the webhook for every agent event.
pub fn ingest(query: &str, body: &str) {
    if let Some(ev) = parse(query, body) {
        lock_monitor().apply(&ev, crate::agent_hooks::now_ms());
        DIRTY.store(true, Ordering::Relaxed);
    }
}

pub fn snapshot_now() -> Snapshot {
    let now = crate::agent_hooks::now_ms();
    let mut m = lock_monitor();
    m.prune(now);
    m.snapshot(now)
}

/// Wait for the slot to be filled, up to `timeout`. Returns the decision, or `None` on timeout.
fn wait_for(slot: &Slot, timeout: Duration) -> Option<String> {
    let guard = slot.decision.lock().unwrap_or_else(|e| e.into_inner());
    let (guard, _) = slot.cv.wait_timeout_while(guard, timeout, |d| d.is_none()).unwrap_or_else(|e| e.into_inner());
    guard.clone()
}

pub struct ApprovalRequest {
    pub agent: Agent,
    pub session_id: String,
    pub cwd: Option<String>,
    pub tool: String,
    pub summary: String,
    pub question: Option<Value>,
}

/// Show a permission request on the island and block until it is answered or `timeout` passes. `None` means
/// "nobody decided": the caller prints nothing and the agent asks in its own terminal.
pub fn request_approval(req: ApprovalRequest, timeout: Duration) -> Option<String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return None;
    }
    let now = crate::agent_hooks::now_ms();
    let id = lock_monitor().add_approval(req.agent, &req.session_id, req.cwd.as_deref(), req.tool, req.summary, req.question, now, timeout)?;
    let slot = Arc::new(Slot { decision: Mutex::new(None), cv: Condvar::new() });
    lock_waiters().push((id, slot.clone()));
    DIRTY.store(true, Ordering::Relaxed);
    let decision = wait_for(&slot, timeout);
    lock_waiters().retain(|(i, _)| *i != id);
    lock_monitor().remove_approval(id, crate::agent_hooks::now_ms());
    DIRTY.store(true, Ordering::Relaxed);
    if decision.is_none() {
        note_unanswered(req.agent.label());
    }
    decision
}

/// Nobody answered: if the user was away, say so when they are back (the agent is waiting in its terminal).
fn note_unanswered(label: &str) {
    let away = crate::agent_attention::is_away(crate::memcore::resume::user_idle_secs().map(|s| s * 1000));
    if away {
        if let Some(dir) = DIR.get() {
            crate::agent_attention::remember(dir, label, &format!("{label} asked for permission and is waiting in its terminal"));
        }
    }
}

/// The webhook's `/agent-approval` handler: the hook's stdin JSON in, the line the agent expects on stdout out
/// (empty = print nothing).
pub fn handle_approval(query: &str, body: &str) -> String {
    let Some(agent) = query_value(query, "agent").and_then(Agent::from_name) else { return String::new() };
    let reply_name = crate::agent_approval_reply::reply_agent(agent.name());
    if !crate::agent_approval_reply::takes_decisions(reply_name) {
        return String::new();
    }
    let json: Option<Value> = serde_json::from_str(body).ok();
    let tool = json.as_ref().and_then(|v| v.get("tool_name")).and_then(|t| t.as_str()).map(|t| clean_token(t, 40)).unwrap_or_default();
    let input = json.as_ref().and_then(|v| v.get("tool_input"));
    let question = (tool == "AskUserQuestion").then(|| input.cloned()).flatten();
    let req = ApprovalRequest {
        agent,
        session_id: session_id_of(json.as_ref()),
        cwd: cwd_of(json.as_ref()),
        summary: summarize(&tool, input),
        tool,
        question: question.clone(),
    };
    let decision = request_approval(req, APPROVAL_WAIT);
    crate::agent_approval_reply::stdout(reply_name, "PermissionRequest", decision.as_deref(), question.as_ref()).unwrap_or_default()
}

fn decision_is_plausible(d: &str) -> bool {
    matches!(d, "allow" | "deny" | "always") || (d.trim_start().starts_with('{') && d.len() <= 4096)
}

// ─── Commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn agent_monitor_list() -> Snapshot {
    snapshot_now()
}

/// The island's Allow / Deny / answer click. Returns false when the request is already gone.
#[tauri::command]
pub fn agent_approval_decide(id: u64, decision: String) -> bool {
    if !decision_is_plausible(&decision) {
        return false;
    }
    let slot = lock_waiters().iter().find(|(i, _)| *i == id).map(|(_, s)| s.clone());
    let Some(slot) = slot else { return false };
    *slot.decision.lock().unwrap_or_else(|e| e.into_inner()) = Some(decision);
    slot.cv.notify_all();
    true
}

// ─── The loop ────────────────────────────────────────────────────────

fn read_settings(dir: &std::path::Path) -> (bool, bool, DisplayMode) {
    let json = std::fs::read_to_string(dir.join("settings.json")).ok().and_then(|c| serde_json::from_str::<Value>(&c).ok());
    let flag = |k: &str, d: bool| json.as_ref().and_then(|j| j.get(k)).and_then(|v| v.as_bool()).unwrap_or(d);
    let mode = json.as_ref().and_then(|j| j.get("agentMonitorDisplay")).and_then(|v| v.as_str()).map(DisplayMode::from_setting).unwrap_or(DisplayMode::StayAwake);
    (flag("agentMonitor", true), flag("agentMonitorHover", true), mode)
}

/// Take or drop the pinned screen-awake lease (it keeps the display and system on while it is held).
fn set_hold(lease: &mut Option<crate::keep_awake::KeepAwakeLease>, on: bool) {
    *lease = if on { crate::keep_awake::acquire_pinned(crate::keep_awake::MAX_WATCH_HOLD) } else { None };
    tracing::info!("agent monitor: screen hold {} (held = {})", if on { "taken" } else { "released" }, lease.is_some());
}

pub fn start(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = DIR.set(dir);
    }
    tauri::async_runtime::spawn(async move {
        let mut power = Power::default();
        let mut lease: Option<crate::keep_awake::KeepAwakeLease> = None;
        let mut island = crate::agent_island::Island::default();
        let mut last_json = String::new();
        let mut settings = (true, true, DisplayMode::StayAwake);
        let mut settings_at = 0u64;
        let mut power_at = 0u64;
        loop {
            let busy = DIRTY.load(Ordering::Relaxed) || lock_monitor().running() > 0 || island.visible();
            tokio::time::sleep(Duration::from_millis(if busy { 150 } else { 1000 })).await;
            let now = crate::agent_hooks::now_ms();
            if now.saturating_sub(settings_at) > 2_000 {
                if let Some(dir) = DIR.get() {
                    settings = read_settings(dir);
                }
                settings_at = now;
                ENABLED.store(settings.0, Ordering::Relaxed);
            }
            let (enabled, hover, mode) = settings;
            let snap = if enabled { snapshot_now() } else { Snapshot { running: 0, at_ms: now, sessions: vec![], approvals: vec![] } };
            DIRTY.store(false, Ordering::Relaxed);
            let json = serde_json::to_string(&snap).unwrap_or_default();
            if json != last_json {
                let _ = app.emit("agent_monitor:update", &snap);
                last_json = json;
            }
            island.update(&app, &snap, hover);
            if now.saturating_sub(power_at) >= 1_000 {
                power_at = now;
                let idle = crate::memcore::resume::user_idle_secs();
                let meeting = app
                    .try_state::<Arc<crate::meeting_detect::MeetingState>>()
                    .is_some_and(|m| m.should_suppress_tts());
                let obs = PowerObs { now_ms: now, running: snap.running, idle_secs: idle, meeting };
                for act in power.step(&obs, mode, |i, m| crate::window_tile::may_blank_display(i, m).is_ok()) {
                    match act {
                        PowerAct::Hold => set_hold(&mut lease, true),
                        PowerAct::Release => set_hold(&mut lease, false),
                        PowerAct::Blank => {
                            if let Err(e) = crate::window_tile::display_off() {
                                tracing::warn!("agent monitor: display off failed: {e}");
                            }
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const T: u64 = 1_000_000;

    fn ev(agent: Agent, sid: &str, kind: Kind) -> Ev {
        Ev { agent, session_id: sid.into(), cwd: None, kind }
    }

    #[test]
    fn a_session_goes_thinking_working_thinking_done() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        assert_eq!(m.running(), 1);
        m.apply(&ev(Agent::Claude, "s1", Kind::PreTool(ToolUse::named("Bash"))), T + 1);
        let s = m.snapshot(T + 2);
        assert_eq!(s.sessions[0].state, State::Working);
        assert_eq!(s.sessions[0].tool.as_deref(), Some("Bash"));
        m.apply(&ev(Agent::Claude, "s1", Kind::PostTool), T + 3);
        assert_eq!(m.snapshot(T + 4).sessions[0].state, State::Thinking);
        m.apply(&ev(Agent::Claude, "s1", Kind::Stop), T + 5);
        assert_eq!(m.running(), 0);
        assert_eq!(m.snapshot(T + 6).sessions[0].state, State::Done);
    }

    #[test]
    fn a_finished_logo_leaves_the_row_after_the_linger() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        m.apply(&ev(Agent::Claude, "s1", Kind::Stop), T + 10);
        m.prune(T + 10 + DONE_LINGER_MS - 1);
        assert_eq!(m.snapshot(T).sessions.len(), 1, "still fading out");
        m.prune(T + 10 + DONE_LINGER_MS);
        assert!(m.snapshot(T).sessions.is_empty(), "gone from the row");
    }

    #[test]
    fn two_sessions_of_one_agent_are_kept_apart() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "a", Kind::Prompt), T);
        m.apply(&ev(Agent::Claude, "b", Kind::Prompt), T);
        m.apply(&ev(Agent::Claude, "a", Kind::Stop), T + 1);
        assert_eq!(m.running(), 1, "b is still running although a stopped");
    }

    #[test]
    fn a_stop_for_an_unseen_agent_makes_no_logo() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Codex, "x", Kind::Stop), T);
        assert!(m.snapshot(T).sessions.is_empty());
    }

    #[test]
    fn a_lost_stop_does_not_hold_the_screen_forever() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        m.prune(T + STALE_MS - 1);
        assert_eq!(m.running(), 1);
        m.prune(T + STALE_MS);
        assert_eq!(m.running(), 0);
    }

    #[test]
    fn session_end_removes_without_a_finish() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        m.apply(&ev(Agent::Claude, "s1", Kind::End), T + 1);
        assert!(m.snapshot(T + 2).sessions.is_empty());
    }

    #[test]
    fn an_approval_makes_the_session_wait_and_the_answer_resumes_it() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        let id = m.add_approval(Agent::Claude, "s1", None, "Bash".into(), "rm -rf x".into(), None, T + 1, APPROVAL_WAIT).unwrap();
        assert_eq!(m.snapshot(T + 2).sessions[0].state, State::Waiting);
        assert_eq!(m.approvals_len(), 1);
        // a tool event while waiting must not hide the wait
        m.apply(&ev(Agent::Claude, "s1", Kind::PreTool(ToolUse::default())), T + 3);
        assert_eq!(m.snapshot(T + 4).sessions[0].state, State::Waiting);
        assert!(m.remove_approval(id, T + 5));
        assert_eq!(m.snapshot(T + 6).sessions[0].state, State::Thinking);
        assert!(!m.remove_approval(id, T + 7), "answered once");
    }

    #[test]
    fn too_many_waiting_requests_fall_back_to_the_terminal() {
        let mut m = Monitor::new();
        for i in 0..MAX_APPROVALS {
            assert!(m.add_approval(Agent::Claude, &format!("s{i}"), None, "Bash".into(), "x".into(), None, T, APPROVAL_WAIT).is_some());
        }
        assert!(m.add_approval(Agent::Claude, "extra", None, "Bash".into(), "x".into(), None, T, APPROVAL_WAIT).is_none());
    }

    #[test]
    fn a_stop_drops_the_sessions_pending_approvals() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        m.add_approval(Agent::Claude, "s1", None, "Bash".into(), "x".into(), None, T + 1, APPROVAL_WAIT);
        m.apply(&ev(Agent::Claude, "s1", Kind::Stop), T + 2);
        assert_eq!(m.approvals_len(), 0);
    }

    #[test]
    fn parse_reads_the_query_and_the_hook_json() {
        let body = r#"{"session_id":"abc-1","cwd":"D:\\work\\zync","tool_name":"Bash","tool_input":{"command":"ls"}}"#;
        let e = parse("agent=claude&event=pretool", body).unwrap();
        assert_eq!(e.agent, Agent::Claude);
        assert_eq!(e.session_id, "abc-1");
        assert_eq!(e.kind, Kind::PreTool(ToolUse { name: Some("Bash".into()), detail: Some("ls".into()), edit: None }));
        assert_eq!(e.cwd.as_deref(), Some("D:\\work\\zync"));
        assert_eq!(parse("agent=claude&event=stop", "").unwrap().session_id, "main");
        assert!(parse("agent=nobody&event=stop", "{}").is_none());
        assert!(parse("agent=claude&event=message", "{}").is_none());
        assert!(parse("event=stop", "{}").is_none());
    }

    #[test]
    fn parse_never_lets_body_text_through_unchecked() {
        let body = r#"{"session_id":"a b;rm -rf","tool_name":"Ba sh$(x)"}"#;
        let e = parse("agent=claude&event=pretool", body).unwrap();
        assert_eq!(e.session_id, "abrm-rf");
        assert_eq!(e.kind, Kind::PreTool(ToolUse::named("Bashx")));
    }

    #[test]
    fn a_tool_call_becomes_a_step_and_the_previous_one_is_done() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        let call = |name: &str, detail: &str| Kind::PreTool(ToolUse { name: Some(name.into()), detail: Some(detail.into()), edit: None });
        m.apply(&ev(Agent::Claude, "s1", call("Read", "invoice.ts")), T + 1);
        m.apply(&ev(Agent::Claude, "s1", call("Edit", "invoice.ts")), T + 2);
        let steps = &m.snapshot(T + 3).sessions[0].steps;
        assert_eq!(steps.len(), 2);
        assert_eq!((steps[0].tool.as_str(), steps[0].state), ("Read", StepState::Done));
        assert_eq!((steps[1].tool.as_str(), steps[1].state), ("Edit", StepState::Running));
        m.apply(&ev(Agent::Claude, "s1", Kind::Stop), T + 4);
        let steps = &m.snapshot(T + 5).sessions[0].steps;
        assert!(steps.iter().all(|x| x.state == StepState::Done));
        assert_eq!(steps.last().unwrap().tool, "Done");
    }

    #[test]
    fn a_new_prompt_starts_a_new_timeline_and_the_timeline_is_capped() {
        let mut m = Monitor::new();
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T);
        for i in 0..30 {
            m.apply(&ev(Agent::Claude, "s1", Kind::PreTool(ToolUse::named("Read"))), T + i);
        }
        assert_eq!(m.snapshot(T + 40).sessions[0].steps.len(), MAX_STEPS);
        m.apply(&ev(Agent::Claude, "s1", Kind::Prompt), T + 50);
        assert!(m.snapshot(T + 51).sessions[0].steps.is_empty());
    }

    #[test]
    fn an_edit_carries_its_old_and_new_text_for_the_live_diff() {
        let body = r#"{"session_id":"a","tool_name":"Edit","tool_input":{"file_path":"D:\\w\\src\\invoice.ts","old_string":"const TVA = 0.196","new_string":"const TVA = 0.2"}}"#;
        let e = parse("agent=claude&event=pretool", body).unwrap();
        let Kind::PreTool(t) = e.kind else { panic!("pretool") };
        assert_eq!(t.detail.as_deref(), Some("invoice.ts"));
        let edit = t.edit.expect("an edit");
        assert_eq!(edit.kind, "edit");
        assert_eq!(edit.pairs, vec![EditPair { old: "const TVA = 0.196".into(), new: "const TVA = 0.2".into() }]);
        let mut m = Monitor::new();
        m.apply(&Ev { agent: Agent::Claude, session_id: "a".into(), cwd: None, kind: Kind::PreTool(ToolUse { name: Some("Edit".into()), detail: None, edit: Some(edit) }) }, T);
        assert_eq!(m.snapshot(T).sessions[0].edit.as_ref().unwrap().path, "D:\\w\\src\\invoice.ts");
    }

    #[test]
    fn write_and_multiedit_are_diffable_and_other_tools_are_not() {
        let write = r#"{"tool_name":"Write","tool_input":{"file_path":"a.txt","content":"hi\n"}}"#;
        let Kind::PreTool(t) = parse("agent=claude&event=pretool", write).unwrap().kind else { panic!() };
        assert_eq!(t.edit.unwrap().kind, "write");
        let multi = r#"{"tool_name":"MultiEdit","tool_input":{"file_path":"a.rs","edits":[{"old_string":"a","new_string":"b"},{"old_string":"c","new_string":"d"}]}}"#;
        let Kind::PreTool(t) = parse("agent=claude&event=pretool", multi).unwrap().kind else { panic!() };
        assert_eq!(t.edit.unwrap().pairs.len(), 2);
        let bash = r#"{"tool_name":"Bash","tool_input":{"command":"cargo test\nsecond line"}}"#;
        let Kind::PreTool(t) = parse("agent=claude&event=pretool", bash).unwrap().kind else { panic!() };
        assert!(t.edit.is_none());
        assert_eq!(t.detail.as_deref(), Some("cargo test"));
    }

    #[test]
    fn a_huge_edit_is_flagged_not_sent() {
        let big = "x".repeat(MAX_EDIT_BYTES + 1);
        let body = serde_json::json!({"tool_name":"Write","tool_input":{"file_path":"big.txt","content":big}}).to_string();
        let Kind::PreTool(t) = parse("agent=claude&event=pretool", &body).unwrap().kind else { panic!() };
        let edit = t.edit.unwrap();
        assert!(edit.too_large && edit.pairs.is_empty());
    }

    #[test]
    fn the_folder_is_the_last_path_segment() {
        assert_eq!(folder_of("D:\\work\\zync\\"), Some("zync".into()));
        assert_eq!(folder_of("/home/me/proj"), Some("proj".into()));
        assert_eq!(folder_of(""), None);
    }

    #[test]
    fn summaries_name_the_command_or_file_and_are_cut() {
        assert_eq!(summarize("Bash", Some(&json!({"command":"cargo test"}))), "cargo test");
        assert_eq!(summarize("Edit", Some(&json!({"file_path":"src/a.rs","old_string":"x"}))), "src/a.rs");
        let long = "x".repeat(5000);
        assert!(summarize("Bash", Some(&json!({ "command": long }))).chars().count() <= MAX_SUMMARY_CHARS);
    }

    // ─── power ───

    fn obs(now: u64, running: usize, idle: Option<u64>) -> PowerObs {
        PowerObs { now_ms: now, running, idle_secs: idle, meeting: false }
    }

    #[test]
    fn the_screen_is_held_while_running_and_released_after_the_settle() {
        let mut p = Power::default();
        assert_eq!(p.step(&obs(T, 1, Some(0)), DisplayMode::StayAwake, |_, _| false), vec![PowerAct::Hold]);
        assert!(p.step(&obs(T + 1000, 1, Some(0)), DisplayMode::StayAwake, |_, _| false).is_empty(), "held once");
        assert!(p.step(&obs(T + 2000, 0, Some(0)), DisplayMode::StayAwake, |_, _| false).is_empty(), "settling");
        assert!(p.step(&obs(T + 2000 + SETTLE_MS - 1, 0, Some(0)), DisplayMode::StayAwake, |_, _| false).is_empty());
        assert_eq!(p.step(&obs(T + 2000 + SETTLE_MS, 0, Some(0)), DisplayMode::StayAwake, |_, _| false), vec![PowerAct::Release]);
    }

    #[test]
    fn a_new_prompt_during_the_settle_keeps_the_hold() {
        let mut p = Power::default();
        p.step(&obs(T, 1, Some(0)), DisplayMode::StayAwake, |_, _| false);
        p.step(&obs(T + 1000, 0, Some(0)), DisplayMode::StayAwake, |_, _| false);
        assert!(p.step(&obs(T + 5000, 1, Some(0)), DisplayMode::StayAwake, |_, _| false).is_empty(), "no release, no second hold");
        assert!(p.step(&obs(T + 5000 + SETTLE_MS, 1, Some(0)), DisplayMode::StayAwake, |_, _| false).is_empty());
    }

    #[test]
    fn stay_awake_mode_never_blanks() {
        let mut p = Power::default();
        p.step(&obs(T, 1, Some(0)), DisplayMode::StayAwake, |_, _| true);
        p.step(&obs(T + 1000, 0, Some(900)), DisplayMode::StayAwake, |_, _| true);
        let acts = p.step(&obs(T + 1000 + SETTLE_MS, 0, Some(900)), DisplayMode::StayAwake, |_, _| true);
        assert_eq!(acts, vec![PowerAct::Release]);
        assert!(p.step(&obs(T + 2000 + SETTLE_MS, 0, Some(900)), DisplayMode::StayAwake, |_, _| true).is_empty());
    }

    #[test]
    fn blank_mode_waits_for_the_user_to_be_away_then_blanks_once() {
        let mut p = Power::default();
        p.step(&obs(T, 1, Some(0)), DisplayMode::Blank, |_, _| false);
        p.step(&obs(T + 1000, 0, Some(5)), DisplayMode::Blank, |_, _| false);
        let release_at = T + 1000 + SETTLE_MS;
        assert_eq!(p.step(&obs(release_at, 0, Some(5)), DisplayMode::Blank, |_, _| false), vec![PowerAct::Release]);
        // user still at the PC: refused, retried
        assert!(p.step(&obs(release_at + 1000, 0, Some(5)), DisplayMode::Blank, |i, m| crate::window_tile::may_blank_display(i, m).is_ok()).is_empty());
        // user left
        assert_eq!(p.step(&obs(release_at + 90_000, 0, Some(120)), DisplayMode::Blank, |i, m| crate::window_tile::may_blank_display(i, m).is_ok()), vec![PowerAct::Blank]);
        assert!(p.step(&obs(release_at + 91_000, 0, Some(121)), DisplayMode::Blank, |_, _| true).is_empty(), "blanks once");
    }

    #[test]
    fn a_meeting_stops_the_blank_and_it_is_given_up_after_a_while() {
        let mut p = Power::default();
        p.step(&obs(T, 1, Some(0)), DisplayMode::Blank, |_, _| true);
        p.step(&obs(T + 1000, 0, Some(0)), DisplayMode::Blank, |_, _| true);
        let release_at = T + 1000 + SETTLE_MS;
        p.step(&obs(release_at, 0, Some(0)), DisplayMode::Blank, |_, _| true);
        let meeting = |i, m| crate::window_tile::may_blank_display(i, m).is_ok();
        let mut o = obs(release_at + 5_000, 0, Some(600));
        o.meeting = true;
        assert!(p.step(&o, DisplayMode::Blank, meeting).is_empty(), "meeting wins");
        assert!(p.step(&obs(release_at + BLANK_GIVE_UP_MS + 1, 0, Some(600)), DisplayMode::Blank, |_, _| true).is_empty(), "given up");
    }

    // ─── broker ───

    #[test]
    fn a_decision_reaches_the_waiting_request() {
        let slot = Arc::new(Slot { decision: Mutex::new(None), cv: Condvar::new() });
        let s2 = slot.clone();
        let t = std::thread::spawn(move || wait_for(&s2, Duration::from_secs(5)));
        std::thread::sleep(Duration::from_millis(50));
        *slot.decision.lock().unwrap() = Some("allow".into());
        slot.cv.notify_all();
        assert_eq!(t.join().unwrap().as_deref(), Some("allow"));
    }

    #[test]
    fn no_answer_times_out_to_none() {
        let slot = Slot { decision: Mutex::new(None), cv: Condvar::new() };
        assert_eq!(wait_for(&slot, Duration::from_millis(30)), None);
    }

    #[test]
    fn only_plausible_decisions_are_accepted() {
        assert!(decision_is_plausible("allow"));
        assert!(decision_is_plausible("deny"));
        assert!(decision_is_plausible(r#"{"answers":{"q":"a"}}"#));
        assert!(!decision_is_plausible("rm -rf"));
        assert!(!decision_is_plausible(""));
        assert!(!agent_approval_decide(u64::MAX, "allow".into()), "unknown request");
    }

    #[test]
    fn display_mode_parses_the_setting() {
        assert_eq!(DisplayMode::from_setting("blank"), DisplayMode::Blank);
        assert_eq!(DisplayMode::from_setting("Blank"), DisplayMode::Blank);
        assert_eq!(DisplayMode::from_setting("stay-awake"), DisplayMode::StayAwake);
        assert_eq!(DisplayMode::from_setting(""), DisplayMode::StayAwake);
    }
}
