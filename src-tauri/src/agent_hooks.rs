//! Agent Stop hooks (doc 142, phase W4): the only fully reliable way to know that a coding agent stopped
//! is for the agent to say so. Claude Code runs a hook command when it stops or needs the user; Codex
//! runs its `notify` program after each turn. NEXUS installs a tiny script that posts "claude stopped" to
//! its own localhost endpoint, and records the event.
//!
//! Safety rules (all tested):
//!   * installing is ALWAYS approved by the user first (voice offer), never silent;
//!   * the user's file is backed up before it is touched, written atomically, and left alone when it
//!     cannot be understood (invalid JSON, an unexpected shape, a `notify` command of their own);
//!   * only NEXUS's own entries (recognised by `MARKER` in their command) are ever replaced or removed;
//!   * the endpoint has its OWN token. The `/trigger` token can run any spoken command, so it is never
//!     written anywhere an agent config can see it; this one can only record "agent X stopped";
//!   * an event carries no text: just the agent name and the kind. Output is judged from the screen or
//!     the sidecar, never from what a hook posts.

use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Appears in the script's file name, so it is in every command line NEXUS writes into a config.
pub const MARKER: &str = "nexus-agent-event";
const TOKEN_SERVICE: &str = "agent_event";
const MAX_EVENTS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    pub fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Agent::Claude => "Claude Code",
            Agent::Codex => "Codex",
        }
    }

    pub fn from_name(s: &str) -> Option<Agent> {
        match s {
            "claude" => Some(Agent::Claude),
            "codex" => Some(Agent::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Stop,
    /// The agent needs the user (a permission prompt, an elicitation).
    Notification,
    /// The user sent a new prompt: a new turn began, so an older stop no longer counts.
    Start,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HookError {
    /// The config file exists but is not something we can edit safely.
    Unreadable(String),
    /// Codex already has a `notify` command that is not ours.
    HasOwnNotify,
    Io(String),
}

impl HookError {
    pub fn spoken(&self, agent: Agent) -> String {
        match self {
            HookError::HasOwnNotify => format!(
                "{} already has a notify command of its own, so I left its settings alone, sir.",
                agent.label()
            ),
            HookError::Unreadable(_) => format!(
                "I couldn't read the {} settings file safely, so I didn't touch it, sir.",
                agent.label()
            ),
            HookError::Io(_) => format!("I couldn't write the {} settings file, sir; nothing was changed.", agent.label()),
        }
    }
}

// ─── The token ───────────────────────────────────────────────────────

/// Get-or-create the endpoint token (keychain). Different from the `/trigger` token on purpose.
pub fn agent_event_token() -> String {
    if let Some(tok) = crate::auth_vault::get_api_key(TOKEN_SERVICE) {
        if !tok.is_empty() {
            return tok;
        }
    }
    let tok = crate::webhook::generate_token();
    crate::auth_vault::set_api_key(TOKEN_SERVICE, &tok);
    tok
}

// ─── The script the agents run ───────────────────────────────────────

#[cfg(windows)]
pub const SCRIPT_NAME: &str = "nexus-agent-event.cmd";
#[cfg(not(windows))]
pub const SCRIPT_NAME: &str = "nexus-agent-event.sh";

/// `script <agent> <stop|notification|start> [ignored extra arguments]`. The agent and event travel in the URL;
/// the BODY is whatever the agent's hook gives on stdin (Claude Code sends JSON with its last message). Codex
/// appends its own JSON as an extra argument, which is ignored. The token lives only in this file (user's app
/// data), not in any agent config.
pub fn script_text(token: &str, port: u16) -> String {
    #[cfg(windows)]
    {
        format!(
            "@echo off\r\nrem NEXUS agent hook: tells NEXUS that an agent stopped. Remove with \"disconnect <agent>\".\r\n\
             set EV=%~2\r\nif \"%EV%\"==\"\" set EV=stop\r\n\
             curl.exe -s -m 3 -X POST \"http://127.0.0.1:{port}/agent-event?agent=%~1&event=%EV%\" -H \"Authorization: Bearer {token}\" \
             -H \"Content-Type: application/json\" --data-binary @- >nul 2>&1\r\nexit /b 0\r\n"
        )
    }
    #[cfg(not(windows))]
    {
        format!(
            "#!/bin/sh\n# NEXUS agent hook: tells NEXUS that an agent stopped.\nEV=\"${{2:-stop}}\"\n\
             curl -s -m 3 -X POST \"http://127.0.0.1:{port}/agent-event?agent=$1&event=$EV\" -H \"Authorization: Bearer {token}\" \
             -H \"Content-Type: application/json\" --data-binary @- >/dev/null 2>&1\nexit 0\n"
        )
    }
}

pub fn script_path(app_data: &Path) -> PathBuf {
    app_data.join("agent_hooks").join(SCRIPT_NAME)
}

/// Write (or refresh) the script. Returns its path.
pub fn write_script(app_data: &Path, token: &str, port: u16) -> Result<PathBuf, HookError> {
    let path = script_path(app_data);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| HookError::Io(e.to_string()))?;
    }
    std::fs::write(&path, script_text(token, port)).map_err(|e| HookError::Io(e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(path)
}

// ─── Editing Claude Code's settings.json ─────────────────────────────

/// (Claude Code event, our kind, matcher). `Notification` is limited to prompts that really wait for the user:
/// "idle_prompt" also fires after a finished turn and would look like "needs you" forever.
const CLAUDE_EVENTS: &[(&str, &str, Option<&str>)] = &[
    ("Stop", "stop", None),
    ("Notification", "notification", Some("permission_prompt|elicitation_dialog")),
    ("UserPromptSubmit", "start", None),
];

fn command_line(script: &Path, agent: &str, kind: &str) -> String {
    format!("\"{}\" {agent} {kind}", script.display())
}

fn entry_is_ours(entry: &Value) -> bool {
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .map_or(false, |hs| hs.iter().any(|h| h.get("command").and_then(|c| c.as_str()).map_or(false, |c| c.contains(MARKER))))
}

fn parse_settings(existing: &str) -> Result<Value, HookError> {
    if existing.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(existing).map_err(|e| HookError::Unreadable(e.to_string()))?;
    if !v.is_object() {
        return Err(HookError::Unreadable("settings is not a JSON object".into()));
    }
    Ok(v)
}

fn pretty(v: &Value) -> Result<String, HookError> {
    serde_json::to_string_pretty(v).map(|s| s + "\n").map_err(|e| HookError::Io(e.to_string()))
}

/// Add (or refresh) NEXUS's Stop and Notification hooks, leaving every other setting and hook as it is.
pub fn claude_add_hook(existing: &str, script: &Path) -> Result<String, HookError> {
    let mut root = parse_settings(existing)?;
    let obj = root.as_object_mut().expect("checked object");
    let hooks = obj.entry("hooks").or_insert_with(|| json!({}));
    let hooks = hooks.as_object_mut().ok_or_else(|| HookError::Unreadable("\"hooks\" is not an object".into()))?;
    for (event, kind, matcher) in CLAUDE_EVENTS {
        let list = hooks.entry(*event).or_insert_with(|| json!([]));
        let list = list.as_array_mut().ok_or_else(|| HookError::Unreadable(format!("\"{event}\" is not a list")))?;
        list.retain(|e| !entry_is_ours(e));
        let mut entry = json!({ "hooks": [ { "type": "command", "command": command_line(script, "claude", kind), "timeout": 10 } ] });
        if let Some(m) = matcher {
            entry["matcher"] = json!(m);
        }
        list.push(entry);
    }
    pretty(&root)
}

/// Remove only NEXUS's entries. Returns the new text and whether anything was removed.
pub fn claude_remove_hook(existing: &str) -> Result<(String, bool), HookError> {
    let mut root = parse_settings(existing)?;
    let mut removed = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(|h| h.as_object_mut()) {
        for (event, _, _) in CLAUDE_EVENTS {
            if let Some(list) = hooks.get_mut(*event).and_then(|l| l.as_array_mut()) {
                let before = list.len();
                list.retain(|e| !entry_is_ours(e));
                removed |= list.len() != before;
            }
            // an event list we emptied is dropped, so the file goes back to how it was
            if hooks.get(*event).and_then(|l| l.as_array()).map_or(false, |l| l.is_empty()) {
                hooks.remove(*event);
            }
        }
    }
    if root.get("hooks").and_then(|h| h.as_object()).map_or(false, |h| h.is_empty()) {
        root.as_object_mut().expect("object").remove("hooks");
    }
    if !removed {
        return Ok((existing.to_string(), false));
    }
    Ok((pretty(&root)?, true))
}

pub fn claude_has_hook(existing: &str) -> bool {
    parse_settings(existing)
        .ok()
        .and_then(|r| r.get("hooks").and_then(|h| h.as_object()).cloned())
        .map_or(false, |hooks| {
            CLAUDE_EVENTS.iter().all(|(event, _, _)| hooks.get(*event).and_then(|l| l.as_array()).map_or(false, |l| l.iter().any(entry_is_ours)))
        })
}

// ─── Editing Codex's config.toml (`notify`) ──────────────────────────

const CODEX_COMMENT: &str = "# NEXUS: tells NEXUS when Codex finishes a turn (remove with \"disconnect codex\")";

fn notify_line(script: &Path) -> Result<String, HookError> {
    let p = script.display().to_string();
    if p.contains('\'') || p.contains('\n') {
        return Err(HookError::Io("the script path contains a quote".into()));
    }
    // TOML literal strings (single quotes) need no escaping of backslashes
    Ok(format!("notify = ['{p}', 'codex', 'stop']"))
}

/// Index of the first `[table]` header line: top-level keys must stay before it.
fn first_table_line(lines: &[&str]) -> usize {
    lines.iter().position(|l| l.trim_start().starts_with('[')).unwrap_or(lines.len())
}

fn is_notify_key(line: &str) -> bool {
    let t = line.trim_start();
    t.strip_prefix("notify").map_or(false, |r| r.trim_start().starts_with('='))
}

pub fn codex_add_notify(existing: &str, script: &Path) -> Result<String, HookError> {
    let ours = notify_line(script)?;
    let lines: Vec<&str> = existing.lines().collect();
    let top = first_table_line(&lines);
    let mut out: Vec<String> = Vec::new();
    let mut replaced = false;
    for (i, l) in lines.iter().enumerate() {
        if i < top && is_notify_key(l) {
            if !l.contains(MARKER) {
                return Err(HookError::HasOwnNotify);
            }
            out.push(ours.clone());
            replaced = true;
        } else {
            out.push((*l).to_string());
        }
    }
    if !replaced {
        let mut head = vec![CODEX_COMMENT.to_string(), ours];
        head.extend(out);
        out = head;
    }
    Ok(out.join("\n") + "\n")
}

pub fn codex_remove_notify(existing: &str) -> (String, bool) {
    let lines: Vec<&str> = existing.lines().collect();
    let top = first_table_line(&lines);
    let mut out: Vec<&str> = Vec::new();
    let mut removed = false;
    for (i, l) in lines.iter().enumerate() {
        if i < top && is_notify_key(l) && l.contains(MARKER) {
            removed = true;
            if out.last() == Some(&CODEX_COMMENT) {
                out.pop();
            }
            continue;
        }
        out.push(l);
    }
    if !removed {
        return (existing.to_string(), false);
    }
    (out.join("\n") + "\n", true)
}

pub fn codex_has_notify(existing: &str) -> bool {
    let lines: Vec<&str> = existing.lines().collect();
    let top = first_table_line(&lines);
    lines.iter().take(top).any(|l| is_notify_key(l) && l.contains(MARKER))
}

// ─── Files ───────────────────────────────────────────────────────────

pub fn config_path(agent: Agent) -> Option<PathBuf> {
    match agent {
        Agent::Claude => {
            let base = std::env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from).or_else(|| dirs_next::home_dir().map(|h| h.join(".claude")))?;
            Some(base.join("settings.json"))
        }
        Agent::Codex => {
            let base = std::env::var_os("CODEX_HOME").map(PathBuf::from).or_else(|| dirs_next::home_dir().map(|h| h.join(".codex")))?;
            Some(base.join("config.toml"))
        }
    }
}

fn read_or_empty(path: &Path) -> Result<String, HookError> {
    match std::fs::read_to_string(path) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(HookError::Unreadable(e.to_string())),
    }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Backup (if the file exists), then write atomically. Returns the backup path.
fn write_with_backup(path: &Path, new_text: &str) -> Result<Option<PathBuf>, HookError> {
    let mut backup = None;
    if path.exists() {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let b = path.with_file_name(format!("{name}.nexus-backup-{}", unix_secs()));
        std::fs::copy(path, &b).map_err(|e| HookError::Io(format!("backup failed: {e}")))?;
        backup = Some(b);
    } else if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| HookError::Io(e.to_string()))?;
    }
    let tmp = path.with_extension("nexus-tmp");
    std::fs::write(&tmp, new_text).map_err(|e| HookError::Io(e.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        HookError::Io(e.to_string())
    })?;
    Ok(backup)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Installed {
    pub backup: Option<PathBuf>,
}

/// Install the hook for `agent` into `cfg` (the script must already exist at `script`).
pub fn install(agent: Agent, cfg: &Path, script: &Path) -> Result<Installed, HookError> {
    let existing = read_or_empty(cfg)?;
    let updated = match agent {
        Agent::Claude => claude_add_hook(&existing, script)?,
        Agent::Codex => codex_add_notify(&existing, script)?,
    };
    if updated == existing {
        return Ok(Installed { backup: None });
    }
    Ok(Installed { backup: write_with_backup(cfg, &updated)? })
}

/// Remove NEXUS's hook. `Ok(false)` = there was none.
pub fn uninstall(agent: Agent, cfg: &Path) -> Result<bool, HookError> {
    if !cfg.exists() {
        return Ok(false);
    }
    let existing = read_or_empty(cfg)?;
    let (updated, removed) = match agent {
        Agent::Claude => claude_remove_hook(&existing)?,
        Agent::Codex => codex_remove_notify(&existing),
    };
    if removed {
        write_with_backup(cfg, &updated)?;
    }
    Ok(removed)
}

pub fn is_installed(agent: Agent, cfg: &Path) -> bool {
    let Ok(text) = read_or_empty(cfg) else { return false };
    match agent {
        Agent::Claude => claude_has_hook(&text),
        Agent::Codex => codex_has_notify(&text),
    }
}

// ─── Events ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEvent {
    pub agent: String,
    pub kind: EventKind,
    pub at_ms: u64,
    /// The agent's last message, for a Stop (already cut to the last 4 KB and masked).
    pub tail: Option<String>,
}

static EVENTS: Mutex<VecDeque<AgentEvent>> = Mutex::new(VecDeque::new());

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedEvent {
    pub agent: Agent,
    pub kind: EventKind,
    pub tail: Option<String>,
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == key).map(|(_, v)| v))
}

fn kind_from(s: &str) -> Option<EventKind> {
    Some(match s {
        "stop" => EventKind::Stop,
        "notification" => EventKind::Notification,
        "start" => EventKind::Start,
        _ => return None,
    })
}

/// The agent's final text in the JSON its hook receives (Claude Code: `last_assistant_message`; Codex's
/// notify payload: `last-assistant-message`).
fn final_message(v: &Value) -> Option<String> {
    ["last_assistant_message", "last-assistant-message"].iter().find_map(|k| v.get(*k).and_then(|x| x.as_str()).map(String::from))
}

/// `?agent=claude&event=stop` (new script) or a body `{"agent":"claude","event":"stop"}` (old script) ->
/// the event. Only two known agents and three known events are accepted. For a stop, the agent's final
/// message is kept as DATA for the verdict (cut to the last 4 KB and masked); nothing else in the body is
/// read or stored, and no text ever becomes a command.
pub fn parse_event(query: &str, body: &str) -> Option<ParsedEvent> {
    let json: Option<Value> = serde_json::from_str(body).ok();
    let (agent, kind) = match (query_value(query, "agent"), query_value(query, "event")) {
        (Some(a), Some(e)) => (Agent::from_name(a)?, kind_from(e)?),
        (None, None) => {
            let v = json.as_ref()?;
            (Agent::from_name(v.get("agent")?.as_str()?)?, kind_from(v.get("event")?.as_str()?)?)
        }
        _ => return None,
    };
    let tail = if kind == EventKind::Stop {
        json.as_ref().and_then(final_message).map(|m| crate::session_verdict::prepare_tail(&m)).filter(|t| !t.trim().is_empty())
    } else {
        None
    };
    Some(ParsedEvent { agent, kind, tail })
}

pub fn record_event_at(agent: Agent, kind: EventKind, at_ms: u64, tail: Option<String>) {
    if let Ok(mut q) = EVENTS.lock() {
        q.push_back(AgentEvent { agent: agent.name().to_string(), kind, at_ms, tail });
        while q.len() > MAX_EVENTS {
            q.pop_front();
        }
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn last_index(q: &VecDeque<AgentEvent>, agent: Agent, kind: EventKind) -> Option<usize> {
    q.iter().rposition(|e| e.agent == agent.name() && e.kind == kind)
}

/// When did this agent last report this kind of event?
pub fn last_event_ms(agent: Agent, kind: EventKind) -> Option<u64> {
    EVENTS.lock().ok().and_then(|q| last_index(&q, agent, kind).map(|i| q[i].at_ms))
}

/// Has this agent stopped (at or after `since_ms`) with no new prompt since? A new prompt means a new turn
/// is running, so the older stop no longer says "finished".
pub fn stop_is_latest(agent: Agent, since_ms: u64) -> bool {
    EVENTS.lock().map_or(false, |q| match last_index(&q, agent, EventKind::Stop) {
        Some(stop) => q[stop].at_ms >= since_ms && last_index(&q, agent, EventKind::Start).map_or(true, |start| stop > start),
        None => false,
    })
}

/// The final message of the agent's latest stop, if its hook sent one.
pub fn last_stop_tail(agent: Agent) -> Option<String> {
    EVENTS.lock().ok().and_then(|q| last_index(&q, agent, EventKind::Stop).and_then(|i| q[i].tail.clone()))
}

/// A permission prompt arrived (at or after `since_ms`) after the agent's latest stop and prompt: it is
/// waiting for the user.
pub fn needs_user(agent: Agent, since_ms: u64) -> bool {
    EVENTS.lock().map_or(false, |q| match last_index(&q, agent, EventKind::Notification) {
        Some(n) => {
            q[n].at_ms >= since_ms
                && last_index(&q, agent, EventKind::Stop).map_or(true, |i| n > i)
                && last_index(&q, agent, EventKind::Start).map_or(true, |i| n > i)
        }
        None => false,
    })
}

/// "5 minutes ago" for the status answer.
pub fn ago(now_ms: u64, then_ms: u64) -> String {
    let secs = now_ms.saturating_sub(then_ms) / 1000;
    match secs {
        0..=59 => "moments ago".to_string(),
        60..=3599 => format!("{} minute{} ago", secs / 60, if secs / 60 == 1 { "" } else { "s" }),
        _ => format!("{} hour{} ago", secs / 3600, if secs / 3600 == 1 { "" } else { "s" }),
    }
}

// ─── What the endpoint answers (pure) ────────────────────────────────

/// The decision for `POST /agent-event`: (HTTP status, body, event to record).
pub fn route_event(authorized: bool, query: &str, body: &str) -> (u16, &'static str, Option<ParsedEvent>) {
    if !authorized {
        return (401, r#"{"error":"unauthorized"}"#, None);
    }
    match parse_event(query, body) {
        Some(ev) => (202, r#"{"ok":true}"#, Some(ev)),
        None => (400, r#"{"error":"bad event"}"#, None),
    }
}

// ─── Voice ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookAction {
    Connect,
    Disconnect,
    Status,
}

impl HookAction {
    pub fn as_str(self) -> &'static str {
        match self {
            HookAction::Connect => "connect",
            HookAction::Disconnect => "disconnect",
            HookAction::Status => "status",
        }
    }

    pub fn from_str(s: &str) -> Option<HookAction> {
        Some(match s {
            "connect" => HookAction::Connect,
            "disconnect" => HookAction::Disconnect,
            "status" => HookAction::Status,
            _ => return None,
        })
    }
}

fn norm(text: &str) -> String {
    let mut t = text
        .to_lowercase()
        .replace('\u{2019}', "'")
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '\'' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    loop {
        let before = t.clone();
        for p in ["hey nexus ", "nexus ", "okay ", "ok ", "please ", "can you ", "could you ", "alright "] {
            if let Some(r) = t.strip_prefix(p) {
                t = r.to_string();
            }
        }
        for sfx in [" please"] {
            if let Some(r) = t.strip_suffix(sfx) {
                t = r.to_string();
            }
        }
        if t == before {
            return t;
        }
    }
}

const HOOK_VOCAB: &[&str] = &[
    "connect", "hook", "hooks", "up", "set", "setup", "install", "add", "enable", "the", "stop", "to", "nexus", "claude", "codex", "code",
    "let", "know", "when", "stops", "finishes", "is", "done", "with", "notifications", "notification", "signal", "for", "my", "me", "a",
    "disconnect", "remove", "uninstall", "delete", "disable", "unhook", "take", "out", "of", "from", "connected", "installed", "status",
    "has", "reported", "yet", "been", "hooked", "on", "in", "set-up", "tell",
];

/// "connect claude's stop hook", "let nexus know when codex stops", "remove the claude hook", "is the
/// claude hook installed". Strict: every word must be in the hook vocabulary AND the sentence must name
/// the hook / stop / nexus, so "install claude" or "connect to claude" are never claimed.
pub fn parse_command(text: &str) -> Option<(HookAction, Agent)> {
    let t = norm(text);
    let tokens: Vec<&str> = t.split(' ').collect();
    if tokens.is_empty() || !tokens.iter().all(|w| HOOK_VOCAB.contains(w)) {
        return None;
    }
    let p = format!(" {t} ");
    let agent = if p.contains(" codex ") {
        Agent::Codex
    } else if p.contains(" claude ") {
        Agent::Claude
    } else {
        return None;
    };
    if p.contains(" claude ") && p.contains(" codex ") {
        return None;
    }
    let context = [" hook ", " hooks ", " unhook ", " nexus ", " notification ", " notifications ", " signal ", " reported "].iter().any(|c| p.contains(c));
    if !context {
        return None;
    }
    let has = |ws: &[&str]| ws.iter().any(|w| p.contains(&format!(" {w} ")));
    if has(&["disconnect", "remove", "uninstall", "delete", "disable", "unhook"]) || p.contains(" take out ") {
        return Some((HookAction::Disconnect, agent));
    }
    if (has(&["is", "has", "status"]) && has(&["connected", "installed", "status", "reported", "hooked"])) || p.contains(" set up yet ") {
        return Some((HookAction::Status, agent));
    }
    if has(&["connect", "hook", "install", "add", "enable", "setup", "set-up", "let"]) || p.contains(" set up ") || p.contains(" hook up ") {
        return Some((HookAction::Connect, agent));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> PathBuf {
        PathBuf::from(r"C:\Users\me\AppData\Roaming\com.nexus.assistant\agent_hooks").join("nexus-agent-event.cmd")
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexus_hooks_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn claude_settings_get_both_hooks_and_keep_everything_else() {
        let before = r#"{"theme":"dark","permissions":{"allow":["Bash(ls)"]},"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo hi"}]}],"Stop":[{"hooks":[{"type":"command","command":"my-own-stop.sh"}]}]}}"#;
        let after = claude_add_hook(before, &script()).unwrap();
        let v: Value = serde_json::from_str(&after).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"], "echo hi");
        // the user's own Stop hook is still there, ours is added beside it
        let stops = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stops.len(), 2);
        assert_eq!(stops[0]["hooks"][0]["command"], "my-own-stop.sh");
        let ours = stops[1]["hooks"][0]["command"].as_str().unwrap();
        assert!(ours.contains(MARKER) && ours.ends_with("claude stop"), "{ours}");
        assert_eq!(v["hooks"]["Notification"][0]["hooks"][0]["command"].as_str().unwrap().ends_with("claude notification"), true);
        // only prompts that really wait for the user (idle_prompt would also fire after a finished turn)
        assert_eq!(v["hooks"]["Notification"][0]["matcher"], "permission_prompt|elicitation_dialog");
        // a new prompt is reported too, so an older stop cannot be mistaken for the end of the new turn
        assert!(v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"].as_str().unwrap().ends_with("claude start"));
        assert!(v["hooks"]["UserPromptSubmit"][0].get("matcher").is_none());
        assert!(claude_has_hook(&after));
        assert!(!claude_has_hook(before));
    }

    #[test]
    fn installing_twice_changes_nothing_and_a_moved_script_is_refreshed_not_duplicated() {
        let once = claude_add_hook("", &script()).unwrap();
        assert_eq!(claude_add_hook(&once, &script()).unwrap(), once);
        let moved = PathBuf::from(r"D:\elsewhere").join("nexus-agent-event.cmd");
        let twice = claude_add_hook(&once, &moved).unwrap();
        let v: Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(v["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().contains("elsewhere"));
    }

    #[test]
    fn removing_takes_out_only_ours_and_restores_the_file() {
        let own = r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-own-stop.sh"}]}]}}"#;
        let with = claude_add_hook(own, &script()).unwrap();
        let (back, removed) = claude_remove_hook(&with).unwrap();
        assert!(removed);
        let v: Value = serde_json::from_str(&back).unwrap();
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "my-own-stop.sh");
        assert!(v["hooks"].get("Notification").is_none());
        // a file that held only ours goes back to empty settings
        let only = claude_add_hook("{}", &script()).unwrap();
        let (empty, removed) = claude_remove_hook(&only).unwrap();
        assert!(removed);
        assert_eq!(serde_json::from_str::<Value>(&empty).unwrap(), json!({}));
        // nothing of ours: untouched, byte for byte
        let (same, removed) = claude_remove_hook(own).unwrap();
        assert!(!removed);
        assert_eq!(same, own);
    }

    #[test]
    fn a_settings_file_that_cannot_be_understood_is_never_overwritten() {
        for bad in ["{not json", "[1,2,3]", "\"text\"", r#"{"hooks":[]}"#, r#"{"hooks":{"Stop":"x"}}"#, r#"{"hooks":{"Notification":{}}}"#] {
            assert!(matches!(claude_add_hook(bad, &script()), Err(HookError::Unreadable(_))), "{bad}");
        }
        assert!(claude_remove_hook("{not json").is_err());
        assert!(!claude_has_hook("{not json"));
    }

    #[test]
    fn codex_gets_a_top_level_notify_line_before_any_table() {
        let before = "model = \"gpt-5\"\n\n[profiles.fast]\nmodel = \"x\"\n";
        let after = codex_add_notify(before, &script()).unwrap();
        let lines: Vec<&str> = after.lines().collect();
        let notify = lines.iter().position(|l| l.starts_with("notify")).unwrap();
        let table = lines.iter().position(|l| l.starts_with("[profiles")).unwrap();
        assert!(notify < table);
        assert!(after.contains("model = \"gpt-5\"") && after.contains("[profiles.fast]"));
        assert!(lines[notify].contains(MARKER) && lines[notify].contains("'codex', 'stop']"), "{}", lines[notify]);
        assert!(codex_has_notify(&after) && !codex_has_notify(before));
        // empty file
        assert!(codex_add_notify("", &script()).unwrap().contains("notify = ["));
    }

    #[test]
    fn codex_keeps_the_users_own_notify_and_edits_ours_in_place() {
        let own = "notify = [\"python\", \"notify.py\"]\n";
        assert_eq!(codex_add_notify(own, &script()), Err(HookError::HasOwnNotify));
        // a notify inside a [table] is a different key and does not count
        let in_table = "[tui]\nnotify = true\n";
        let after = codex_add_notify(in_table, &script()).unwrap();
        assert!(after.starts_with("# NEXUS") && after.contains("[tui]\nnotify = true"));
        // ours is rewritten, not duplicated
        let moved = PathBuf::from(r"D:\elsewhere").join("nexus-agent-event.cmd");
        let again = codex_add_notify(&after, &moved).unwrap();
        assert_eq!(again.matches("notify = [").count(), 1);
        assert!(again.contains("elsewhere"));
        // identical script: byte for byte the same
        assert_eq!(codex_add_notify(&after, &script()).unwrap(), after);
    }

    #[test]
    fn codex_removal_takes_only_our_line_and_its_comment() {
        let before = "model = \"x\"\n\n[tui]\nnotify = true\n";
        let with = codex_add_notify(before, &script()).unwrap();
        let (back, removed) = codex_remove_notify(&with);
        assert!(removed);
        assert_eq!(back, before);
        let own = "notify = [\"python\", \"notify.py\"]\n";
        let (same, removed) = codex_remove_notify(own);
        assert!(!removed && same == own);
        let bad = "notify = ['x']";
        assert!(codex_add_notify("", Path::new("it's/bad.cmd")).is_err());
        let _ = bad;
    }

    #[test]
    fn install_backs_up_writes_atomically_and_uninstall_restores() {
        let d = tmp("install");
        let cfg = d.join("settings.json");
        std::fs::write(&cfg, r#"{"theme":"dark"}"#).unwrap();
        let r = install(Agent::Claude, &cfg, &script()).unwrap();
        let b = r.backup.expect("backup made");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), r#"{"theme":"dark"}"#);
        assert!(is_installed(Agent::Claude, &cfg));
        assert!(!cfg.with_extension("nexus-tmp").exists());
        // second install is a no-op (no new backup)
        assert_eq!(install(Agent::Claude, &cfg, &script()).unwrap().backup, None);
        assert!(uninstall(Agent::Claude, &cfg).unwrap());
        assert!(!is_installed(Agent::Claude, &cfg));
        assert_eq!(serde_json::from_str::<Value>(&std::fs::read_to_string(&cfg).unwrap()).unwrap(), json!({"theme": "dark"}));
        assert!(!uninstall(Agent::Claude, &cfg).unwrap());
        // no file yet: created, no backup
        let fresh = d.join("sub").join("config.toml");
        assert_eq!(install(Agent::Codex, &fresh, &script()).unwrap().backup, None);
        assert!(is_installed(Agent::Codex, &fresh));
        // unreadable file: error, file untouched
        let bad = d.join("bad.json");
        std::fs::write(&bad, "{nope").unwrap();
        assert!(install(Agent::Claude, &bad, &script()).is_err());
        assert_eq!(std::fs::read_to_string(&bad).unwrap(), "{nope");
        // uninstall with no file at all
        assert!(!uninstall(Agent::Codex, &d.join("missing.toml")).unwrap());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_script_carries_the_token_only_for_the_agent_event_endpoint() {
        let s = script_text("SECRETTOKEN", 39220);
        assert!(s.contains("SECRETTOKEN") && s.contains("/agent-event?agent=") && s.contains("127.0.0.1:39220"));
        assert!(s.contains("--data-binary @-"), "the agent's own stdin JSON is the body");
        assert!(!s.contains("/trigger"));
        let d = tmp("script");
        let p = write_script(&d, "T", 39220).unwrap();
        assert!(p.ends_with(SCRIPT_NAME) && p.exists());
        assert!(p.to_string_lossy().contains(MARKER));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_endpoint_accepts_only_an_authorised_known_agent_and_event() {
        let ev = |agent, kind| Some(ParsedEvent { agent, kind, tail: None });
        // new script: agent and event in the URL
        assert_eq!(route_event(true, "agent=claude&event=stop", ""), (202, r#"{"ok":true}"#, ev(Agent::Claude, EventKind::Stop)));
        assert_eq!(route_event(true, "agent=codex&event=notification", "{}").2, ev(Agent::Codex, EventKind::Notification));
        assert_eq!(route_event(true, "agent=claude&event=start", "not json at all").2, ev(Agent::Claude, EventKind::Start));
        // old script: agent and event in a JSON body
        assert_eq!(route_event(true, "", r#"{"agent":"claude","event":"stop"}"#).2, ev(Agent::Claude, EventKind::Stop));
        // not authorised: nothing is recorded, whatever the body says
        assert_eq!(route_event(false, "agent=claude&event=stop", "").0, 401);
        assert_eq!(route_event(false, "agent=claude&event=stop", "").2, None);
        for (q, b) in [
            ("", ""), ("", "{"), ("agent=claude", ""), ("event=stop", ""), ("agent=evil&event=stop", ""), ("agent=claude&event=shutdown", ""),
            ("agent=claude&event=STOP", ""), ("agent=../x&event=stop", ""), ("agent=claude&agent=codex", ""), ("agent=&event=", ""),
            ("", r#"{"agent":"evil","event":"stop"}"#), ("", r#"{"agent":1,"event":"stop"}"#), ("", "[1]"), ("", r#"{"agent":"claude"}"#),
        ] {
            let r = route_event(true, q, b);
            assert_eq!((r.0, r.2), (400, None), "{q:?} {b:?}");
        }
        // extra fields (text a process might try to smuggle in) are ignored, never stored
        let r = route_event(true, "agent=claude&event=notification", r#"{"transcript":"shut down my laptop","message":"rm -rf"}"#);
        assert_eq!(r.2, ev(Agent::Claude, EventKind::Notification));
    }

    #[test]
    fn a_stop_keeps_the_agents_final_message_as_masked_data_and_nothing_else_does() {
        let body = r#"{"hook_event_name":"Stop","session_id":"s","last_assistant_message":"All 12 tests passed. Mail me at someone@example.com","cwd":"C:/p"}"#;
        let e = parse_event("agent=claude&event=stop", body).unwrap();
        let tail = e.tail.expect("the final message is kept");
        assert!(tail.contains("All 12 tests passed.") && !tail.contains("someone@example.com"), "{tail}");
        // Codex spells it with dashes
        let e = parse_event("agent=codex&event=stop", r#"{"type":"agent-turn-complete","last-assistant-message":"Done."}"#).unwrap();
        assert_eq!(e.tail.as_deref(), Some("Done."));
        // only a stop carries text; a prompt or a notification never stores any
        assert_eq!(parse_event("agent=claude&event=start", body).unwrap().tail, None);
        assert_eq!(parse_event("agent=claude&event=notification", body).unwrap().tail, None);
        // no final message, an empty one, or a non-string: no tail
        assert_eq!(parse_event("agent=claude&event=stop", r#"{"cwd":"x"}"#).unwrap().tail, None);
        assert_eq!(parse_event("agent=claude&event=stop", r#"{"last_assistant_message":"   "}"#).unwrap().tail, None);
        assert_eq!(parse_event("agent=claude&event=stop", r#"{"last_assistant_message":5}"#).unwrap().tail, None);
        // a very long message is cut to the END (where the result is)
        let long = format!("{{\"last_assistant_message\":\"{}THE-END\"}}", "x".repeat(20_000));
        let t = parse_event("agent=claude&event=stop", &long).unwrap().tail.unwrap();
        assert!(t.len() <= crate::session_verdict::TAIL_BYTES + 20 && t.ends_with("THE-END"), "{}", t.len());
    }

    #[test]
    fn stop_start_and_notification_events_are_ordered_per_agent() {
        // other tests share the global list: use times far in the future
        let base = u64::MAX / 4;
        let a = Agent::Claude;
        let b = Agent::Codex;
        record_event_at(a, EventKind::Start, base + 100, None);
        assert!(!stop_is_latest(a, base), "a prompt without a stop is a running turn");
        record_event_at(a, EventKind::Stop, base + 1_000, Some("first tail".into()));
        assert!(stop_is_latest(a, base + 1_000));
        assert!(!stop_is_latest(a, base + 1_001), "the stop is older than `since`");
        assert_eq!(last_stop_tail(a).as_deref(), Some("first tail"));
        assert_eq!(last_event_ms(a, EventKind::Stop), Some(base + 1_000));
        // a new prompt: the old stop no longer says "finished"
        record_event_at(a, EventKind::Start, base + 2_000, None);
        assert!(!stop_is_latest(a, base));
        // and the next stop does, with its own message
        record_event_at(a, EventKind::Stop, base + 3_000, Some("second tail".into()));
        assert!(stop_is_latest(a, base));
        assert_eq!(last_stop_tail(a).as_deref(), Some("second tail"));
        // another agent's events do not count for this one
        assert!(!stop_is_latest(b, base));
        record_event_at(b, EventKind::Notification, base + 4_000, None);
        assert!(!stop_is_latest(b, base), "a notification is not a stop");
        // a permission prompt after the latest stop = waiting for the user, until the agent moves on
        assert!(needs_user(b, base));
        assert!(!needs_user(b, base + 4_001), "older than `since`");
        record_event_at(b, EventKind::Stop, base + 5_000, None);
        assert!(!needs_user(b, base), "a stop after the prompt answers it");
        record_event_at(a, EventKind::Notification, base + 6_000, None);
        assert!(needs_user(a, base));
        record_event_at(a, EventKind::Start, base + 7_000, None);
        assert!(!needs_user(a, base), "a new prompt answers it too");
        // the list stays capped
        for i in 0..(MAX_EVENTS as u64 + 10) {
            record_event_at(b, EventKind::Stop, base + 10_000 + i, None);
        }
        assert!(EVENTS.lock().unwrap().len() <= MAX_EVENTS);
        assert_eq!(ago(120_000, 0), "2 minutes ago");
        assert_eq!(ago(10_000, 0), "moments ago");
        assert_eq!(ago(3_600_000, 0), "1 hour ago");
    }

    #[test]
    fn the_whole_parser_hands_hook_sentences_to_the_hooks_and_leaves_other_agent_talk_alone() {
        use crate::intent_parser::{parse_deterministic, ParsedIntent};
        for (t, action, agent) in [
            ("hook up claude", "connect", "claude"),
            ("remove the codex hook", "disconnect", "codex"),
            ("is the claude hook installed", "status", "claude"),
        ] {
            match parse_deterministic(t).map(|r| r.intent) {
                Some(ParsedIntent::AgentHookCommand { hook_action, agent: a }) => assert_eq!((hook_action.as_str(), a.as_str()), (action, agent), "{t}"),
                other => panic!("{t:?} -> {other:?}"),
            }
        }
        for t in ["open claude", "install claude", "watch claude until it's done and shut down my laptop", "cancel the shutdown plan"] {
            assert!(!matches!(parse_deterministic(t).map(|r| r.intent), Some(ParsedIntent::AgentHookCommand { .. })), "{t}");
        }
        // the question that asks for the setup carries the agent through the offer and a yes is a plain yes
        use crate::memcore::offer::{classify_for, Offer, Reply};
        let offer = Offer::HookSetup { agent: "claude".into() };
        assert_eq!(classify_for(&offer, "yes"), Reply::Yes);
        assert_eq!(classify_for(&offer, "no"), Reply::No);
        assert_eq!(classify_for(&offer, "shut down"), Reply::Other, "a power phrase does not approve a settings change");
    }

    #[test]
    fn voice_commands_for_the_hooks_are_strict() {
        for t in [
            "connect nexus to claude", "hook up claude", "set up the claude hook", "install the claude hook", "add the stop hook to claude",
            "let nexus know when claude stops", "set up claude notifications", "enable the claude hook", "nexus install the stop hook for claude",
            "connect the claude stop hook", "Nexus, hook up claude.", "please set up the codex hook", "let nexus know when codex finishes",
            "install the codex hook", "connect nexus to codex",
        ] {
            let r = parse_command(t);
            assert!(matches!(r, Some((HookAction::Connect, _))), "{t} -> {r:?}");
        }
        assert_eq!(parse_command("hook up claude"), Some((HookAction::Connect, Agent::Claude)));
        assert_eq!(parse_command("install the codex hook"), Some((HookAction::Connect, Agent::Codex)));
        for t in ["disconnect claude from nexus", "remove the claude hook", "uninstall the codex hook", "take out the claude hook", "disable the claude stop hook", "unhook claude"] {
            assert!(matches!(parse_command(t), Some((HookAction::Disconnect, _))), "{t}");
        }
        for t in ["is the claude hook installed", "is claude connected to nexus", "claude hook status", "has claude reported yet", "is the codex hook set up yet"] {
            assert!(matches!(parse_command(t), Some((HookAction::Status, _))), "{t}");
        }
        for t in [
            "", "install claude", "connect to claude", "open claude", "claude", "remove claude", "uninstall codex", "hook up", "set up the hook",
            "connect claude and codex hooks", "install the claude hook and open chrome", "tell me when claude stops", "what is a hook",
            "install the claude extension", "remove the hook from my fishing rod", "let me know when claude stops",
            "hook up the claude codex hook", "set up the claude hook for codex", "remove the codex claude hook",
        ] {
            assert_eq!(parse_command(t), None, "{t:?}");
        }
    }
}
