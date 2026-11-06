//! Production glue for notification-to-sidebar: one process-wide ledger + opener, the producer entry point
//! (`publish_github`), the activation path (`activate`) and the IPC commands the sidebar calls. Identity,
//! resolution and the opening transaction live in `notify_event` / `notify_open`; this file only connects them to Tauri.
//!
//! An alert becomes a Windows notification-center toast only when it was recorded as new (or as a recovery of a known
//! failure); grouped/duplicate observations never produce a second toast. Clicking the toast calls `activate`: the
//! full-screen overlay plays the expand animation, then the alert opens in the CodeRabbit sidebar. Opening is read-only.

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime};

use crate::notify_brain::{Fact, Line};
use crate::notify_event::{
    entity_key, event_id, Delivery, Ledger, NotifyEvent, PrivacyClass, Provider, ReadState, Recorded, Severity, Status, Target,
    SCHEMA_VERSION,
};
use crate::notify_native::{channel_for, toast_xml, Channel};
use crate::work_area::{expand_rects, LRect, PRect};
use crate::notify_open::{Accept, Opened, Opener};

/// Opaque local binding of the single connected GitHub sign-in (the vault holds one GitHub token).
const GITHUB_ACCOUNT: &str = "gh-primary";
const DEPLOY_ACCOUNT: &str = "deploys";
const EVENT_TTL_MS: u64 = 7 * 24 * 3600 * 1000;
pub use crate::memcore::deploy_watch::DeployExtras;
/// GitHub alerts are shown in the CodeRabbit interface, docked right (the toast appears bottom-right).
pub const DETAIL_VIEW: &str = "coderabbit";
pub const DETAIL_DOCK: &str = "right";
/// How long the overlay's expand animation runs before the sidebar is shown.
pub const EXPAND_MS: u64 = 280;

struct Hub {
    ledger: Ledger,
    opener: Opener,
}

fn hub() -> &'static Mutex<Hub> {
    static HUB: OnceLock<Mutex<Hub>> = OnceLock::new();
    HUB.get_or_init(|| Mutex::new(Hub { ledger: Ledger::new(), opener: Opener::new() }))
}

/// Opens are serialized so two quick clicks cannot interleave across the sidebar's awaits and leave the sidebar
/// showing alert A while the live transaction is B.
fn open_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn lock() -> std::sync::MutexGuard<'static, Hub> {
    hub().lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

static APP_DATA_DIR: OnceLock<std::sync::RwLock<Option<std::path::PathBuf>>> = OnceLock::new();

fn set_app_data_dir(path: std::path::PathBuf) {
    let lock = APP_DATA_DIR.get_or_init(|| std::sync::RwLock::new(None));
    if let Ok(mut g) = lock.write() {
        *g = Some(path);
    }
}

fn get_app_data_dir() -> Option<std::path::PathBuf> {
    let lock = APP_DATA_DIR.get_or_init(|| std::sync::RwLock::new(None));
    lock.read().ok().and_then(|g| g.clone())
}

fn save_event_best_effort(ev: &NotifyEvent) {
    if let Some(dir) = get_app_data_dir() {
        if let Err(e) = crate::memcore::with_store(&dir, |s| s.save_notify_event(ev)).unwrap_or(Ok(())) {
            tracing::warn!("[NOTIFY] failed to persist event {}: {e}", ev.event_id);
        }
    }
}

/// Pure: the ledger event for a GitHub observation of a UNIQUELY resolved `owner/repo`. None when the run id or
/// the repository identity is not usable (no card is better than a card that opens the wrong thing).
/// Mail that must never show its words on a toast: codes, resets and bank messages. Pure; lowercase match on subject
/// and sender.
pub fn looks_security(subject: &str, from: &str) -> bool {
    let s = format!("{} {}", subject.to_lowercase(), from.to_lowercase());
    [
        "verification code", "one-time", "one time password", "otp", "password reset", "reset your password",
        "security code", "sign-in code", "login code", "your code", "2-step", "two-step", "bank", "upi",
        "transaction alert", "debited", "credited", "account statement",
    ]
    .iter()
    .any(|k| s.contains(k))
}

/// Pure: the ledger event for one important mail of `account`. Sender is the title and the subject the preview; the
/// body and snippet are never stored. Security mail is reduced to "Security message". None when ids are unusable.
pub fn gmail_event(account: &str, m: &crate::memcore::mailtriage::MailMeta, severity: Severity, now: u64) -> Option<NotifyEvent> {
    if m.id.is_empty() || m.thread_id.is_empty() || account.is_empty() {
        return None;
    }
    let target = Target::Gmail { thread_id: m.thread_id.clone(), message_id: Some(m.id.clone()) };
    let key = entity_key(Provider::Gmail, account, &target, "mail");
    let (title, preview) = if looks_security(&m.subject, &m.from_email) {
        ("Security message".to_string(), String::new())
    } else {
        let who = if m.from_name.trim().is_empty() { m.from_email.clone() } else { m.from_name.clone() };
        (crate::notify_event::sanitize_text(&who, 120), crate::notify_event::sanitize_text(&m.subject, 240))
    };
    let hints = crate::notify_links::OpenHints {
        gmail_account: Some(account.to_string()),
        rfc822_id: (!m.rfc822_id.is_empty()).then(|| m.rfc822_id.clone()),
        ..Default::default()
    };
    let hints = if crate::notify_links::validate_hints(Provider::Gmail, &hints).is_ok() {
        hints
    } else {
        crate::notify_links::OpenHints { gmail_account: hints.gmail_account.filter(|a| crate::notify_links::validate_hints(Provider::Gmail, &crate::notify_links::OpenHints { gmail_account: Some(a.clone()), ..Default::default() }).is_ok()), ..Default::default() }
    };
    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &m.id, Status::Info),
        entity_key: key,
        provider: Provider::Gmail,
        account_id: account.to_string(),
        kind: "mail".into(),
        status: Status::Info,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title,
        preview,
        privacy: PrivacyClass::Public,
        target,
        source_ref: m.id.clone(),
        original_source_ref: m.id.clone(),
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

/// An important mail arrived for `account`: show it as a toast (sender + subject only), click opens that message.
pub fn publish_gmail<R: Runtime>(app: &AppHandle<R>, account: &str, m: &crate::memcore::mailtriage::MailMeta, severity: Severity) -> Option<String> {
    let ev = gmail_event(account, m, severity, now_ms())?;
    publish_event(app, ev, None)
}

/// What the producer learned beyond the bare fact: the job / step to open, the PR number and the branch.
#[derive(Debug, Clone, Default)]
pub struct GithubExtras {
    pub hints: crate::notify_links::OpenHints,
    pub pr: Option<u64>,
    pub branch: Option<String>,
}

#[cfg(test)]
pub fn github_event(full_name: &str, fact: &Fact, line: &Line, now: u64) -> Option<NotifyEvent> {
    github_event_with(full_name, fact, line, now, GithubExtras::default())
}

/// `github_event` with the extras. The PR number lives in the target; a hint that does not validate is dropped (the
/// alert is still useful and still opens the run).
pub fn github_event_with(full_name: &str, fact: &Fact, line: &Line, now: u64, extras: GithubExtras) -> Option<NotifyEvent> {
    let (owner, repo) = full_name.split_once('/')?;
    let run_id: u64 = fact.run_ref.parse().ok()?;
    let hints = if crate::notify_links::validate_hints(Provider::GitHub, &extras.hints).is_ok() { extras.hints } else { Default::default() };
    let pr = extras.pr.filter(|n| *n > 0);
    let target = Target::GitHub { host: "github.com".into(), owner: owner.into(), repo: repo.into(), pr, run_id: Some(run_id), sha: None };
    let kind = fact.kind.as_str();
    let key = entity_key(Provider::GitHub, GITHUB_ACCOUNT, &target, kind);
    let (status, severity) = match line {
        Line::Fixed { .. } => (Status::Recovered, Severity::Low),
        Line::Failed { .. } | Line::StillFailing { .. } => (Status::Failed, Severity::Medium),
    };
    let source_ref = run_id.to_string();
    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &source_ref, status),
        entity_key: key,
        provider: Provider::GitHub,
        account_id: GITHUB_ACCOUNT.into(),
        kind: kind.into(),
        status,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title: crate::notify_brain::render_for("github", line, None).trim_end_matches(['.', ',', ' ']).to_string(),
        preview: match extras.branch.as_deref().map(|b| crate::notify_event::sanitize_text(b, 60)).filter(|b| !b.is_empty()) {
            Some(b) => format!("{full_name} · {b} · run {run_id}"),
            None => format!("{full_name} · run {run_id}"),
        },
        privacy: PrivacyClass::Public,
        target,
        source_ref: source_ref.clone(),
        original_source_ref: source_ref,
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

fn status_str(s: Status) -> &'static str {
    match s {
        Status::Failed => "failed",
        Status::Recovered => "recovered",
        Status::Info => "info",
    }
}

/// `publish_github` with the self-test knob: a shorter lifetime for the alert.
pub fn publish_github_with<R: Runtime>(app: &AppHandle<R>, full_name: &str, fact: &Fact, line: &Line, ttl_ms: Option<u64>) {
    publish_github_extras(app, full_name, fact, line, ttl_ms, GithubExtras::default());
}

/// Publish a GitHub alert with the job / step / PR the watcher fetched, so a click opens the failing step.
pub fn publish_github_extras<R: Runtime>(app: &AppHandle<R>, full_name: &str, fact: &Fact, line: &Line, ttl_ms: Option<u64>, extras: GithubExtras) {
    let now = now_ms();
    let Some(ev) = github_event_with(full_name, fact, line, now, extras) else {
        return;
    };
    publish_event(app, ev, ttl_ms);
}

/// Pure: the ledger event for a deployment observation (Vercel, Render, Netlify, Supabase).
/// Returns None when any target id is unusable or the service is unknown.
pub fn deploy_event(
    service: &str,
    display_project: &str,
    fact: &Fact,
    line: &Line,
    now: u64,
    extras: DeployExtras,
) -> Option<NotifyEvent> {
    let provider = match service.to_ascii_lowercase().as_str() {
        "vercel" => Provider::Vercel,
        "render" => Provider::Render,
        "netlify" => Provider::Netlify,
        "supabase" => Provider::Supabase,
        _ => return None,
    };
    let target = Target::Deployment {
        provider,
        team: extras.team,
        project: extras.slug,
        deployment_id: extras.deployment_id.clone(),
        revision: None,
    };
    if crate::notify_event::validate_target(&target).is_err() {
        return None;
    }
    let hints = if let Some(url) = extras.url.filter(|u| crate::notify_links::provider_page_ok(provider, u)) {
        crate::notify_links::OpenHints {
            html_url: Some(url),
            ..Default::default()
        }
    } else {
        crate::notify_links::OpenHints::default()
    };
    let key = entity_key(provider, DEPLOY_ACCOUNT, &target, "deploy");
    let (status, severity) = match line {
        Line::Fixed { .. } => (Status::Recovered, Severity::Low),
        Line::Failed { .. } | Line::StillFailing { .. } => (Status::Failed, Severity::Medium),
    };
    let source_ref = if !extras.deployment_id.is_empty() {
        extras.deployment_id
    } else {
        fact.run_ref.clone()
    };
    if source_ref.is_empty()
        || source_ref.len() > 100
        || !source_ref.chars().all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c))
    {
        return None;
    }
    let short_id: String = source_ref.chars().take(12).collect();
    let preview = if !short_id.is_empty() {
        format!("{display_project} · {short_id}")
    } else {
        display_project.to_string()
    };
    let raw_title = crate::notify_brain::render_for(service, line, None);
    let title = raw_title.trim_end_matches(['.', ',', ' ']).to_string();
    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &source_ref, status),
        entity_key: key,
        provider,
        account_id: DEPLOY_ACCOUNT.into(),
        kind: "deploy".into(),
        status,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title: crate::notify_event::sanitize_text(&title, 120),
        preview: crate::notify_event::sanitize_text(&preview, 240),
        privacy: PrivacyClass::Public,
        target,
        source_ref: source_ref.clone(),
        original_source_ref: source_ref,
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

/// Publish a deployment alert: Windows/NEXUS notification toast with typed target to open the exact deploy page.
pub fn publish_deploy<R: Runtime>(
    app: &AppHandle<R>,
    service: &str,
    display_project: &str,
    fact: &Fact,
    line: &Line,
    extras: DeployExtras,
) {
    let now = now_ms();
    let Some(ev) = deploy_event(service, display_project, fact, line, now, extras) else {
        return;
    };
    publish_event(app, ev, None);
}

/// What the producer knows about a calendar reminder.
#[derive(Debug, Clone, Default)]
pub struct CalendarAlertData<'a> {
    pub id: &'a str,
    /// Start time (epoch seconds): a moved event reminds again, so it must be a different alert.
    pub start_epoch: i64,
    pub summary: &'a str,
    pub location: Option<&'a str>,
    pub html_link: Option<&'a str>,
    pub meet_url: Option<&'a str>,
}

impl<'a> From<&'a crate::memcore::leavewatch::LeaveEvent> for CalendarAlertData<'a> {
    fn from(ev: &'a crate::memcore::leavewatch::LeaveEvent) -> Self {
        Self {
            id: &ev.id,
            start_epoch: ev.start_epoch,
            summary: &ev.summary,
            location: Some(&ev.location),
            html_link: ev.html_link.as_deref(),
            meet_url: ev.meet_url.as_deref(),
        }
    }
}

impl From<crate::google::types::AlertUrgency> for Severity {
    fn from(u: crate::google::types::AlertUrgency) -> Self {
        match u {
            crate::google::types::AlertUrgency::Low => Severity::Low,
            crate::google::types::AlertUrgency::Medium => Severity::Medium,
            crate::google::types::AlertUrgency::High => Severity::High,
            crate::google::types::AlertUrgency::Critical => Severity::Critical,
        }
    }
}

/// Pure: the ledger event for a calendar reminder.
/// Provider::Calendar, Target::Calendar { event_id }, kind "event", status Info, title = summary (sanitized, <=120),
/// preview = the reminder text trimmed (<=240), hints.html_url only if `provider_page_ok(Provider::Calendar, url)`,
/// hints.meet_url only if valid per `validate_hints`; invalid hints are dropped (never fail the event).
/// account_id: the Google account email when the loop has it (`acct.email`), else "calendar".
/// Returns None when the event_id is unusable (fails validate_target).
pub fn calendar_event(
    account: &str,
    ev: &CalendarAlertData<'_>,
    text: &str,
    severity: Severity,
    now: u64,
) -> Option<NotifyEvent> {
    let _ = ev.location;
    let target = Target::Calendar { event_id: ev.id.to_string() };
    if crate::notify_event::validate_target(&target).is_err() {
        return None;
    }
    let account_id = if account.trim().is_empty() { "calendar" } else { account.trim() };
    let key = entity_key(Provider::Calendar, account_id, &target, "event");
    let status = Status::Info;
    let source_ref = format!("{}-{}", ev.id, ev.start_epoch);
    let raw_title = crate::notify_event::sanitize_text(ev.summary, 120);
    let title = if raw_title.is_empty() { "(No title)".to_string() } else { raw_title };
    let preview = crate::notify_event::sanitize_text(text.trim(), 240);

    let html_url = ev
        .html_link
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .filter(|u| crate::notify_links::provider_page_ok(Provider::Calendar, u))
        .map(String::from);

    let meet_url = ev
        .meet_url
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .filter(|u| {
            let test_hints = crate::notify_links::OpenHints {
                meet_url: Some(u.to_string()),
                ..Default::default()
            };
            crate::notify_links::validate_hints(Provider::Calendar, &test_hints).is_ok()
        })
        .map(String::from);

    let hints = crate::notify_links::OpenHints {
        html_url,
        meet_url,
        ..Default::default()
    };

    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &source_ref, status),
        entity_key: key,
        provider: Provider::Calendar,
        account_id: account_id.to_string(),
        kind: "event".into(),
        status,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title,
        preview,
        privacy: PrivacyClass::Public,
        target,
        source_ref: source_ref.clone(),
        original_source_ref: source_ref,
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

/// Publish a calendar leave-by reminder: show it on the toast, click opens the event page / Meet link.
pub fn publish_calendar<R: Runtime>(
    app: &AppHandle<R>,
    account: &str,
    ev: &crate::memcore::leavewatch::LeaveEvent,
    text: &str,
    urgency: crate::google::types::AlertUrgency,
) -> Option<String> {
    let now = now_ms();
    let data = CalendarAlertData::from(ev);
    let event = calendar_event(account, &data, text, urgency.into(), now)?;
    publish_event(app, event, None)
}

/// Pure: the ledger event for a WhatsApp chat alert.
/// Provider::WhatsApp, Target::WhatsApp { chat: first 24 hex of sha256(jid) }, kind "chat", status Info,
/// account_id "whatsapp", title = contact name (sanitized, <=120; if empty "WhatsApp"), preview = EMPTY STRING ALWAYS,
/// hints.phone_digits = digits before '@' of the jid ONLY when the jid ends with "@s.whatsapp.net" and has 6..=15 digits.
/// Returns None when jid is empty or chat id is invalid.
pub fn whatsapp_event(
    alert_name: &str,
    jid: &str,
    last_ts: Option<i64>,
    severity: Severity,
    now: u64,
) -> Option<NotifyEvent> {
    let jid_trimmed = jid.trim();
    if jid_trimmed.is_empty() {
        return None;
    }
    let mut h = Sha256::new();
    h.update(jid_trimmed.as_bytes());
    let full_hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let chat = full_hex[..24].to_string();
    let target = Target::WhatsApp { chat };
    if crate::notify_event::validate_target(&target).is_err() {
        return None;
    }
    let key = entity_key(Provider::WhatsApp, "whatsapp", &target, "chat");
    let status = Status::Info;
    let ts = last_ts.unwrap_or(now as i64).max(0);
    let source_ref = format!("{}-{}", &full_hex[..12], ts);

    let raw_title = crate::notify_event::sanitize_text(alert_name, 120);
    let title = if raw_title.is_empty() { "WhatsApp".to_string() } else { raw_title };
    let preview = String::new();

    let phone_digits = if let Some(local) = jid_trimmed.strip_suffix("@s.whatsapp.net") {
        if (6..=15).contains(&local.len()) && local.chars().all(|c| c.is_ascii_digit()) {
            Some(local.to_string())
        } else {
            None
        }
    } else {
        None
    };

    let hints = crate::notify_links::OpenHints {
        phone_digits,
        ..Default::default()
    };

    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &source_ref, status),
        entity_key: key,
        provider: Provider::WhatsApp,
        account_id: "whatsapp".into(),
        kind: "chat".into(),
        status,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title,
        preview,
        privacy: PrivacyClass::Public,
        target,
        source_ref: source_ref.clone(),
        original_source_ref: source_ref,
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

/// Publish a WhatsApp alert: show it on the toast, click opens the chat link.
pub fn publish_whatsapp<R: Runtime>(
    app: &AppHandle<R>,
    alert_name: &str,
    jid: &str,
    last_ts: Option<i64>,
    severity: Severity,
) -> Option<String> {
    let now = now_ms();
    let ev = whatsapp_event(alert_name, jid, last_ts, severity, now)?;
    publish_event(app, ev, None)
}

/// Sanitize folder name for `Target::Agent` project field: `[A-Za-z0-9._-]`, no `..`, never empty, <=60 chars.
pub fn sanitize_project_name(cwd: Option<&str>) -> String {
    let raw = match cwd {
        Some(s) => {
            let trimmed = s.trim_end_matches(['/', '\\']);
            let seg = trimmed.rsplit(['/', '\\']).next().unwrap_or("").trim();
            if seg.is_empty() {
                "session"
            } else {
                seg
            }
        }
        None => "session",
    };
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    while out.contains("..") {
        out = out.replace("..", "-");
    }
    let truncated: String = out.chars().take(60).collect();
    let trimmed = truncated.trim_matches('.');
    if trimmed.is_empty() {
        "session".to_string()
    } else {
        trimmed.to_string()
    }
}

fn sanitize_agent_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
            out.push(c);
        } else {
            out.push('-');
        }
    }
    while out.contains("..") {
        out = out.replace("..", "-");
    }
    let truncated: String = out.chars().take(60).collect();
    let trimmed = truncated.trim_matches('.');
    if trimmed.is_empty() {
        "agent".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Pure: the ledger event for a coding agent alert ("needs your input" or "finished").
/// Provider::Agent, Target::Agent { agent, project }, kind "needs_input" or "finished",
/// status Info, severity Medium (needs_input) / Low (finished), account_id "local",
/// title = "<label> needs your input" or "<label> finished", preview = project folder name only,
/// source_ref = "<at_ms>", hints.agent_cwd only if validate_hints accepts it.
pub fn agent_event(
    agent_name: &str,
    agent_label: &str,
    cwd: Option<&str>,
    needs_input: bool,
    at_ms: u64,
    now: u64,
) -> Option<NotifyEvent> {
    let project = sanitize_project_name(cwd);
    let agent = sanitize_agent_name(agent_name);
    let target = Target::Agent { agent, project: project.clone() };
    if crate::notify_event::validate_target(&target).is_err() {
        return None;
    }
    let kind = if needs_input { "needs_input" } else { "finished" };
    let severity = if needs_input { Severity::Medium } else { Severity::Low };
    let status = Status::Info;
    let key = entity_key(Provider::Agent, "local", &target, kind);
    let source_ref = format!("{at_ms}");

    let raw_title = if needs_input {
        format!("{agent_label} needs your input")
    } else {
        format!("{agent_label} finished")
    };
    let title = crate::notify_event::sanitize_text(&raw_title, 120);
    let preview = project;

    let test_hints = crate::notify_links::OpenHints {
        agent_cwd: cwd.filter(|c| !c.is_empty()).map(str::to_string),
        ..Default::default()
    };
    let hints = if crate::notify_links::validate_hints(Provider::Agent, &test_hints).is_ok() {
        test_hints
    } else {
        crate::notify_links::OpenHints::default()
    };

    Some(NotifyEvent {
        schema_version: SCHEMA_VERSION,
        event_id: event_id(&key, &source_ref, status),
        entity_key: key,
        provider: Provider::Agent,
        account_id: "local".into(),
        kind: kind.into(),
        status,
        severity,
        occurred_at_ms: now,
        observed_at_ms: now,
        updated_at_ms: now,
        title,
        preview,
        privacy: PrivacyClass::Public,
        target,
        source_ref: source_ref.clone(),
        original_source_ref: source_ref,
        occurrences: 1,
        delivery: Delivery::default(),
        read: ReadState::Unread,
        expires_at_ms: now + EVENT_TTL_MS,
        hints,
    })
}

/// Publish a coding agent alert: show it on the toast, click focuses the agent window or opens the folder.
pub fn publish_agent<R: Runtime>(
    app: &AppHandle<R>,
    agent_name: &str,
    agent_label: &str,
    cwd: Option<&str>,
    needs_input: bool,
    at_ms: u64,
) {
    let now = now_ms();
    if let Some(ev) = agent_event(agent_name, agent_label, cwd, needs_input, at_ms, now) {
        publish_event(app, ev, None);
    }
}

/// Record any source's alert in the ledger and deliver it: the NEXUS toast, the Windows toast, or history only.
pub fn publish_event<R: Runtime>(app: &AppHandle<R>, mut ev: NotifyEvent, ttl_ms: Option<u64>) -> Option<String> {
    let now = now_ms();
    if let Some(ttl) = ttl_ms {
        ev.expires_at_ms = now + ttl;
    }
    let (title, preview, status) = (ev.title.clone(), ev.preview.clone(), ev.status);
    let toast_on = tauri::Manager::path(app)
        .app_data_dir()
        .map(|d| crate::memcore::flag(&d, "notifyNative", true))
        .unwrap_or(true);
    let channel = channel_for(toast_on);
    // Hybrid delivery: the NEXUS toast window while the user is present and the shell allows a banner; the native
    // Windows toast otherwise (Focus Session, fullscreen, presentation, nobody at the PC, window failure).
    let dir = tauri::Manager::path(app).app_data_dir().ok();
    if let Some(d) = dir.as_ref() {
        set_app_data_dir(d.clone());
    }
    let custom_on = dir.as_ref().map(|d| crate::memcore::flag(d, "notifyToast", true)).unwrap_or(true);
    let muted = read_muted_providers(dir.as_deref());
    let (delivery, reason) = crate::notify_toast::decide_delivery(
        custom_on,
        toast_on,
        crate::proactive_policy::someone_is_here(),
        crate::notify_toast::shell_state(),
        ev.provider,
        ev.status,
        &muted,
    );
    tracing::info!("[NOTIFY] delivery={delivery:?} reason={reason}");
    let privacy = crate::notify_toast::Privacy::parse(&read_setting_str(dir.as_deref(), "notifyPrivacy").unwrap_or_default());
    let channel = if delivery == crate::notify_toast::Delivery::History { Channel::HistoryOnly } else { channel };
    let (outcome, affected_event) = {
        let mut h = lock();
        h.ledger.prune(now);
        let out = h.ledger.record(ev);
        if let Ok(Recorded::Created(id) | Recorded::Recovered(id)) = &out {
            h.ledger.mark_delivered(id, false, channel == Channel::Toast, true);
        }
        let affected = match &out {
            Ok(Recorded::Created(id) | Recorded::Recovered(id) | Recorded::Grouped(id)) => {
                h.ledger.get(id).cloned()
            }
            _ => None,
        };
        (out, affected)
    };
    if let Some(ev_to_save) = &affected_event {
        save_event_best_effort(ev_to_save);
    }
    let (Recorded::Created(id) | Recorded::Recovered(id)) = (match outcome {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("[NOTIFY] event refused: {e:?}");
            return None;
        }
    }) else {
        return None;
    };
    let event_id_ret = id.clone();
    if channel == Channel::HistoryOnly {
        return Some(event_id_ret);
    }
    if delivery == crate::notify_toast::Delivery::Custom {
        let toast_ev = lock().ledger.get(&id).cloned();
        if let Some(toast_ev) = toast_ev {
            let app3 = app.clone();
            let xml_fallback = (id.clone(), title.clone(), preview.clone(), status);
            tauri::async_runtime::spawn(async move {
                match crate::notify_toast::show(&app3, &toast_ev, privacy).await {
                    Ok(()) => tracing::info!("[NOTIFY] NEXUS toast shown for {}", xml_fallback.0),
                    Err(e) => {
                        tracing::warn!("[NOTIFY] NEXUS toast failed ({e}); using the Windows toast");
                        show_native(&app3, &xml_fallback.0, &xml_fallback.1, &xml_fallback.2, xml_fallback.3);
                    }
                }
            });
            return Some(event_id_ret);
        }
    }
    show_native(app, &id, &title, &preview, status);
    Some(event_id_ret)
}

/// Sets delivery.spoken = true in the ledger and saves the event to the sealed store.
pub fn mark_spoken(event_id: &str) {
    let affected = {
        let mut h = lock();
        h.ledger.mark_delivered(event_id, true, false, false);
        h.ledger.get(event_id).cloned()
    };
    if let Some(ev) = affected {
        save_event_best_effort(&ev);
    }
}

/// Restore saved events from the sealed store into the in-memory ledger at startup.
pub fn restore_ledger<R: Runtime>(app: &AppHandle<R>) {
    let Some(dir) = tauri::Manager::path(app).app_data_dir().ok() else {
        return;
    };
    set_app_data_dir(dir.clone());
    let now = now_ms();
    let events = crate::memcore::with_store(&dir, |s| s.load_notify_events(now)).unwrap_or_default();
    if !events.is_empty() {
        let count = events.len();
        let mut h = lock();
        h.ledger.restore(events);
        tracing::info!("[NOTIFY] restored {count} events into ledger");
    }
}

/// The Windows notification-centre toast for an alert already recorded in the ledger.
fn show_native<R: Runtime>(app: &AppHandle<R>, id: &str, title: &str, preview: &str, status: Status) {
    // Only failures ask to pass Do Not Disturb; a recovery is good news and can wait. `notifyUrgent` turns it off.
    let urgent_on = tauri::Manager::path(app)
        .app_data_dir()
        .map(|d| crate::memcore::flag(&d, "notifyUrgent", true))
        .unwrap_or(true);
    let urgent = wants_urgent(status, urgent_on);
    let Some(xml) = toast_xml(id, title, preview, urgent) else {
        return;
    };
    let app2 = app.clone();
    let shown = crate::notify_native::show(&xml, move |launch| {
        // Only the opaque id is trusted; anything else the toast hands back is ignored.
        if let Some(event_id) = crate::notify_activation::parse_activation(&launch) {
            tauri::async_runtime::spawn(activate(app2.clone(), event_id));
        }
    });
    match shown {
        Ok(()) => tracing::info!("[NOTIFY] toast shown for {id}"),
        Err(e) => tracing::warn!("[NOTIFY] toast failed (alert kept in history): {e}"),
    }
}

/// A string setting from settings.json (None when unreadable or absent).
fn read_setting_str(dir: Option<&std::path::Path>, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir?.join("settings.json")).ok()?;
    serde_json::from_str::<Value>(&text).ok()?.get(key)?.as_str().map(String::from)
}

/// Only a failure asks Windows to pass Do Not Disturb, and only while the owner has not switched `notifyUrgent` off.
pub fn wants_urgent(status: Status, setting_on: bool) -> bool {
    setting_on && status == Status::Failed
}

/// Pure: what the overlay needs to animate a toast into the sidebar.
pub fn expand_json(title: &str, status: Status, from: LRect, to: LRect) -> Value {
    let r = |x: LRect| json!({ "x": x.x, "y": x.y, "w": x.w, "h": x.h });
    json!({ "title": title, "status": status_str(status), "from": r(from), "to": r(to), "durationMs": EXPAND_MS })
}

fn expand_payload<R: Runtime>(app: &AppHandle<R>, event_id: &str) -> Option<Value> {
    let (title, status) = {
        let h = lock();
        let e = h.ledger.get(event_id)?;
        (e.title.clone(), e.status)
    };
    let m = app.primary_monitor().ok()??;
    let rect = |p: &tauri::PhysicalPosition<i32>, s: &tauri::PhysicalSize<u32>| PRect { x: p.x, y: p.y, w: s.width as i32, h: s.height as i32 };
    let mon = rect(m.position(), m.size());
    let wa = m.work_area();
    let (from, to) = expand_rects(mon, rect(&wa.position, &wa.size), m.scale_factor(), DETAIL_VIEW, Some(DETAIL_DOCK));
    Some(expand_json(&title, status, from, to))
}

/// A toast was clicked (or `nexus notify open` simulated it): play the full-screen expand animation, then open the
/// alert in the CodeRabbit sidebar. Without a live overlay the sidebar simply opens.
pub async fn activate<R: Runtime>(app: AppHandle<R>, event_id: String) {
    if crate::stage::is_shown() && crate::stage::is_ready() {
        if let Some(p) = expand_payload(&app, &event_id) {
            let _ = app.emit_to("stage", "notify:expand", p);
            tokio::time::sleep(std::time::Duration::from_millis(EXPAND_MS)).await;
        }
    }
    if let Err(e) = open_event(app, event_id).await {
        tracing::warn!("[NOTIFY] could not open the alert: {e}");
    }
}

/// Pure: only https pages and the WhatsApp send link may be handed to the OS; nothing else a ladder could hold.
pub fn scheme_allowed(url: &str) -> bool {
    let ok_chars = !url.is_empty() && url.len() <= 600 && !url.chars().any(|c| c.is_control() || c.is_whitespace());
    ok_chars
        && (url.starts_with("https://")
            || url
                .strip_prefix("whatsapp://send?phone=")
                .is_some_and(|d| (6..=15).contains(&d.len()) && d.chars().all(|c| c.is_ascii_digit())))
}

fn folder_name_from_cwd(cwd: &str) -> Option<&str> {
    let trimmed = cwd.trim_end_matches(['/', '\\']);
    let seg = trimmed.rsplit(['/', '\\']).next()?.trim();
    if seg.is_empty() {
        None
    } else {
        Some(seg)
    }
}

fn focus_agent(cwd: &str) -> bool {
    // A very short folder name ("src", "app") would match unrelated window titles, so it is never used to pick a window.
    if let Some(folder) = folder_name_from_cwd(cwd).filter(|f| f.chars().count() >= 4) {
        if crate::live::commands::window::wait_and_focus_app(folder, 600) {
            return true;
        }
    }
    std::path::Path::new(cwd).is_dir() && open::that(cwd).is_ok()
}

async fn run_action(a: &crate::notify_links::OpenAction) -> bool {
    use crate::notify_links::OpenAction;
    match a {
        OpenAction::Url(u) if scheme_allowed(u) => open::that(u).is_ok(),
        OpenAction::FocusAgent { cwd } => {
            let cwd_owned = cwd.clone();
            tauri::async_runtime::spawn_blocking(move || focus_agent(&cwd_owned))
                .await
                .unwrap_or(false)
        }
        _ => false,
    }
}

/// Is `event_id` currently recorded in the ledger?
pub fn has_event(event_id: &str) -> bool {
    lock().ledger.get(event_id).is_some()
}

#[cfg(test)]
pub fn is_spoken(event_id: &str) -> bool {
    lock().ledger.get(event_id).map(|e| e.delivery.spoken).unwrap_or(false)
}

#[cfg(test)]
pub fn record_test_event(ev: NotifyEvent) -> String {
    let id = ev.event_id.clone();
    let _ = lock().ledger.record(ev);
    id
}

/// Pure: is this action allowed to be opened by the OS.
pub fn action_allowed(a: &crate::notify_links::OpenAction) -> bool {
    use crate::notify_links::OpenAction;
    match a {
        OpenAction::Url(u) => scheme_allowed(u),
        OpenAction::FocusAgent { cwd } => !cwd.trim().is_empty(),
    }
}

/// Pure: which ladder entry to run, or an error when none can be opened.
pub fn first_openable_action<'a>(ladder: &'a [crate::notify_links::OpenAction]) -> Result<&'a crate::notify_links::OpenAction, &'static str> {
    ladder.iter().find(|a| action_allowed(a)).ok_or("This alert has no page to open.")
}

/// Open the real item (the first entry of the alert's ladder that works).
/// If `fallback_to_activate` is true (e.g. toast fallback), show the alert's detail in the sidebar.
/// If `fallback_to_activate` is false (e.g. "Open in app" button in the sidebar), return Err("This alert has no page to open.").
pub async fn open_external<R: Runtime>(app: &AppHandle<R>, event_id: &str, fallback_to_activate: bool) -> Result<(), String> {
    let ladder = lock().ledger.get(event_id).map(crate::notify_open::event_ladder);
    let Some(ladder) = ladder else {
        return Err("this alert is no longer available".into());
    };
    if !fallback_to_activate && first_openable_action(&ladder).is_err() {
        return Err("This alert has no page to open.".into());
    }
    for a in &ladder {
        if run_action(a).await {
            return Ok(());
        }
    }
    if fallback_to_activate {
        activate(app.clone(), event_id.to_string()).await;
        Ok(())
    } else {
        Err("This alert has no page to open.".into())
    }
}

/// The most recent alert's id (for `nexus notify open`).
pub fn latest_event_id() -> Option<String> {
    lock().ledger.latest_id().map(str::to_string)
}

fn target_rows(t: &Target) -> Vec<Value> {
    let row = |k: &str, v: String| json!({ "kind": k, "label": v });
    match t {
        Target::GitHub { owner, repo, pr, run_id, .. } => {
            let mut v = vec![row("Repository", format!("{owner}/{repo}"))];
            if let Some(n) = pr {
                v.push(row("Pull request", format!("#{n}")));
            }
            if let Some(r) = run_id {
                v.push(row("Run", r.to_string()));
            }
            v
        }
        Target::Repo { owner, repo, .. } => vec![row("Repository", format!("{owner}/{repo}"))],
        Target::Gmail { thread_id, .. } => vec![row("Thread", thread_id.clone())],
        Target::Deployment { team, project, deployment_id, .. } => {
            vec![row("Project", format!("{team}/{project}")), row("Deployment", deployment_id.clone())]
        }
        Target::WhatsApp { .. } => vec![row("Source", "WhatsApp chat".into())],
        Target::Calendar { .. } => vec![row("Source", "Calendar event".into())],
        Target::Agent { agent, project } => vec![row("Agent", agent.clone()), row("Project", project.clone())],
    }
}

/// Pure: the sidebar payload for an opened alert (what `EventDetailView` renders).
pub fn detail_json(opened: &Opened, restore_view: Option<&str>, pr_hint: Option<u64>) -> Value {
    match opened {
        Opened::Detail(d) => json!({
            "kind": d.kind,
            "repo": match &d.target { Target::GitHub { owner, repo, .. } | Target::Repo { owner, repo, .. } => Some(format!("{owner}/{repo}")), _ => None },
            "prNumber": match &d.target { Target::GitHub { pr: Some(n), .. } => Some(*n), _ => pr_hint },
            "mode": "event_detail",
            "token": d.token,
            "eventId": d.event_id,
            "provider": format!("{:?}", d.provider),
            "status": status_str(d.status),
            "title": d.title,
            "preview": d.preview,
            "target": target_rows(&d.target),
            "originalSourceRef": d.original_source_ref,
            "sourceRef": d.source_ref,
            "occurrences": d.occurrences,
            "providerUrl": d.provider_url,
            "restoreView": restore_view,
            "canOpen": d.can_open,
            "openLabel": d.open_label,
        }),
        Opened::Unavailable(u) => json!({
            "mode": "event_detail",
            "token": u.token,
            "unavailable": u.message,
            "restoreView": restore_view,
            "canOpen": false,
        }),
    }
}

fn github_connected() -> bool {
    crate::auth_vault::get_token("github").is_some() || crate::auth_vault::get_api_key("github").is_some()
}

/// The pull request a PR-kind alert belongs to when its target only carries the workflow run (looked up on GitHub
/// with the owner's own sign-in; read-only). None for builds, test repos or any lookup failure.
async fn resolve_pr_hint(event_id: &str) -> Option<u64> {
    let (owner, repo, run) = {
        let h = lock();
        let e = h.ledger.get(event_id)?;
        match &e.target {
            Target::GitHub { owner, repo, pr: None, run_id: Some(r), host, .. } if e.kind == "pr" && host == "github.com" => {
                (owner.clone(), repo.clone(), *r)
            }
            _ => return None,
        }
    };
    if owner == "nexus-test" {
        return None;
    }
    let token = crate::github_data::resolve_token(None, None).await.ok()?;
    crate::coderabbit::pr_number_for_run(&format!("{owner}/{repo}"), &run.to_string(), &token).await.map(u64::from)
}

/// Opens the alert in the CodeRabbit sidebar for a toast click (via `activate`) with only the opaque id, so an
/// unknown/expired/forged one ends in the read-only "unavailable" view.
pub async fn open_event<R: Runtime>(app: AppHandle<R>, event_id: String) -> Result<u64, String> {
    let pr_hint = resolve_pr_hint(&event_id).await;
    let _serial = open_gate().lock().await;
    let (current_view, pinned) = crate::commands::active_sidebar_view_for_notify(&app);
    let (token, payload, affected_event) = {
        let mut guard = lock();
        let Hub { ledger, opener } = &mut *guard;
        let connected = |p: Provider, _: &str| match p {
            Provider::GitHub => github_connected(),
            _ => false,
        };
        let opened = opener.begin(&event_id, ledger, now_ms(), &connected, current_view.as_deref(), pinned);
        let ev = ledger.get(&event_id).cloned();
        (opened.token(), detail_json(&opened, opener.restore_view(), pr_hint), ev)
    };
    if let Some(ev) = affected_event {
        save_event_best_effort(&ev);
    }
    crate::commands::show_event_detail(app, payload).await?;
    Ok(token)
}

/// IPC: the sidebar finished rendering the detail for `token`. Only the live transaction is accepted; the overlay
/// (waiting with a bounded timeout) fades out when it hears it.
#[tauri::command]
pub fn notify_event_ready<R: Runtime>(app: AppHandle<R>, token: u64) -> bool {
    let accepted = lock().opener.ready(token) == Accept::Accepted;
    if accepted {
        let _ = app.emit_to("stage", "notify:ready", json!({ "token": token }));
    }
    accepted
}

/// IPC: the detail ended (back, Escape, sidebar closed). A stale token is ignored; returns the view to restore.
#[tauri::command]
pub fn notify_close_event(token: u64) -> Option<String> {
    lock().opener.close(token)
}

/// IPC: open the real external item for `event_id` using the link ladder.
/// Returns an error if no page/action can be opened, without looping back to activate.
#[tauri::command]
pub async fn notify_open_external<R: Runtime>(app: AppHandle<R>, event_id: String) -> Result<(), String> {
    open_external(&app, &event_id, false).await
}

pub fn provider_id(p: Provider) -> &'static str {
    match p {
        Provider::GitHub => "github",
        Provider::Gmail => "mail",
        Provider::Vercel => "vercel",
        Provider::Render => "render",
        Provider::Netlify => "netlify",
        Provider::Supabase => "supabase",
        Provider::WhatsApp => "whatsapp",
        Provider::Calendar => "calendar",
        Provider::Agent => "agent",
    }
}

pub fn parse_muted_providers(json: &str) -> Vec<String> {
    let Ok(val) = serde_json::from_str::<Value>(json) else { return Vec::new(); };
    let arr = val.get("notifyMuted").and_then(|v| v.as_array()).or_else(|| val.as_array());
    let Some(arr) = arr else { return Vec::new(); };
    arr.iter().filter_map(|v| v.as_str().map(|s| s.trim().to_ascii_lowercase())).collect()
}

pub fn read_muted_providers(dir: Option<&std::path::Path>) -> Vec<String> {
    let text = dir.and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok());
    text.map(|t| parse_muted_providers(&t)).unwrap_or_default()
}

pub fn mute_provider(dir: &std::path::Path, provider: &str) -> Result<bool, String> {
    let provider = provider.trim().to_ascii_lowercase();
    if provider.is_empty() {
        return Ok(false);
    }
    let path = dir.join("settings.json");
    let mut val = if let Ok(text) = std::fs::read_to_string(&path) {
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };
    let mut list = val
        .get("notifyMuted")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.trim().to_ascii_lowercase())).collect::<Vec<_>>())
        .unwrap_or_default();
    if !list.contains(&provider) {
        list.push(provider);
        val["notifyMuted"] = json!(list);
        let text = serde_json::to_string_pretty(&val).map_err(|e| e.to_string())?;
        std::fs::write(&path, text).map_err(|e| e.to_string())?;
    }
    Ok(true)
}

pub fn latest_event() -> Option<NotifyEvent> {
    let h = lock();
    let id = h.ledger.latest_id()?;
    h.ledger.get(id).cloned()
}

pub fn clear_store_events_best_effort() {
    if let Some(dir) = get_app_data_dir() {
        if let Err(e) = crate::memcore::with_store(&dir, |s| s.clear_notify_events()).unwrap_or(Ok(0)) {
            tracing::warn!("[NOTIFY] failed to clear stored events: {e}");
        }
    }
}

pub fn clear_alerts() {
    lock().ledger.clear();
    clear_store_events_best_effort();
}

pub fn dismiss_latest() -> bool {
    let mut h = lock();
    if let Some(id) = h.ledger.latest_id().map(str::to_string) {
        let ok = h.ledger.dismiss(&id);
        if ok {
            if let Some(ev) = h.ledger.get(&id).cloned() {
                drop(h);
                save_event_best_effort(&ev);
                return true;
            }
        }
    }
    false
}

pub fn mark_seen_events(ids: &[String]) {
    let affected: Vec<NotifyEvent> = {
        let mut h = lock();
        let mut list = Vec::new();
        for id in ids {
            if h.ledger.mark_seen(id) {
                if let Some(ev) = h.ledger.get(id).cloned() {
                    list.push(ev);
                }
            }
        }
        list
    };
    for ev in affected {
        save_event_best_effort(&ev);
    }
}

pub fn list_ledger_events() -> Vec<NotifyEvent> {
    let mut items: Vec<NotifyEvent> = lock().ledger.entries().to_vec();
    items.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    items
}

pub fn list_events_with_privacy(privacy: crate::notify_toast::Privacy) -> Vec<Value> {
    let mut items = list_ledger_events();
    items.truncate(50);
    items
        .into_iter()
        .map(|ev| {
            let (title, preview) = crate::notify_toast::visible_text(privacy, &ev);
            json!({
                "eventId": ev.event_id,
                "provider": crate::notify_toast::label(ev.provider),
                "icon": crate::notify_toast::icon_id(&ev),
                "status": match ev.status {
                    Status::Failed => "failed",
                    Status::Recovered => "recovered",
                    Status::Info => "info",
                },
                "title": title,
                "preview": preview,
                "updatedAtMs": ev.updated_at_ms,
                "read": ev.read != ReadState::Unread,
                "spoken": ev.delivery.spoken,
                "count": ev.occurrences,
            })
        })
        .collect()
}

/// IPC: list recent alerts for the Command Hub.
/// Newest first, max 50, from the in-memory ledger. Respects notifyPrivacy mode.
#[tauri::command]
pub fn notify_list_events() -> Vec<serde_json::Value> {
    let dir = get_app_data_dir();
    let privacy = crate::notify_toast::Privacy::parse(
        &read_setting_str(dir.as_deref(), "notifyPrivacy").unwrap_or_default(),
    );
    list_events_with_privacy(privacy)
}

/// IPC: activate an alert from the Command Hub list.
#[tauri::command]
pub async fn notify_alert_open<R: Runtime>(app: AppHandle<R>, event_id: String) -> Result<(), String> {
    if !has_event(&event_id) {
        return Err("alert not found in ledger".into());
    }
    activate(app, event_id).await;
    Ok(())
}

/// IPC: dismiss/clear all alerts from ledger and persistent store.
#[tauri::command]
pub fn notify_alerts_clear() {
    clear_alerts();
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_moved_event_is_a_new_alert_with_the_same_entity() {
        let mk = |start: i64| CalendarAlertData { id: "cal_ev_1", summary: "Dentist", location: None, html_link: None, meet_url: None, start_epoch: start };
        let a = calendar_event("me@gmail.com", &mk(1_000), "Leave now.", Severity::High, 1).unwrap();
        let b = calendar_event("me@gmail.com", &mk(2_000), "Leave now.", Severity::High, 1).unwrap();
        assert_eq!(a.entity_key, b.entity_key);
        assert_ne!(a.event_id, b.event_id, "moved event must not be swallowed as a duplicate");
    }

    fn mail(subject: &str, from: &str) -> crate::memcore::mailtriage::MailMeta {
        crate::memcore::mailtriage::MailMeta {
            id: "18c2e".into(),
            thread_id: "18c2f".into(),
            from_name: from.into(),
            from_email: "x@y.com".into(),
            subject: subject.into(),
            snippet: "secret snippet text".into(),
            labels: vec![],
            date_ms: 0,
            rfc822_id: "<CAF+abc@mail.gmail.com>".into(),
        }
    }

    #[test]
    fn a_mail_event_holds_sender_and_subject_never_the_snippet_and_opens_that_message() {
        let ev = gmail_event("me@gmail.com", &mail("Interview on Monday", "Asha R"), Severity::High, 1_000).expect("event");
        assert_eq!((ev.title.as_str(), ev.preview.as_str()), ("Asha R", "Interview on Monday"));
        assert!(!ev.title.contains("secret") && !ev.preview.contains("secret"));
        let url = crate::notify_open::event_url(&ev).expect("url");
        assert!(url.starts_with("https://mail.google.com/mail/?authuser=me%40gmail.com#search/rfc822msgid%3A"), "{url}");
        assert!(matches!(&ev.target, Target::Gmail { thread_id, message_id: Some(m) } if thread_id == "18c2f" && m == "18c2e"));
    }

    #[test]
    fn codes_and_bank_mail_are_reduced_to_a_security_message() {
        for subj in ["Your verification code is 123456", "Password reset requested", "Transaction alert: debited INR 500", "OTP for login"] {
            let ev = gmail_event("me@gmail.com", &mail(subj, "Someone"), Severity::High, 1_000).expect("event");
            assert_eq!((ev.title.as_str(), ev.preview.as_str()), ("Security message", ""), "{subj}");
        }
        assert!(!looks_security("Lunch tomorrow?", "friend@x.com"));
    }

    #[test]
    fn a_mail_without_ids_or_an_account_gives_no_event() {
        let mut m = mail("Hi", "A");
        m.thread_id.clear();
        assert!(gmail_event("me@gmail.com", &m, Severity::Low, 1).is_none());
        assert!(gmail_event("", &mail("Hi", "A"), Severity::Low, 1).is_none());
    }

    #[test]
    fn the_account_email_is_kept_as_the_open_hint() {
        let ev = gmail_event("me@gmail.com", &mail("Hi", "A"), Severity::Low, 1).expect("event");
        assert_eq!(ev.hints.gmail_account.as_deref(), Some("me@gmail.com"));
    }

    #[test]
    fn only_https_pages_and_the_whatsapp_link_reach_the_os() {
        assert!(scheme_allowed("https://github.com/o/r"));
        assert!(scheme_allowed("whatsapp://send?phone=919876543210"));
        for bad in ["http://x.example", "file:///C:/Windows/notepad.exe", "javascript:alert(1)", "whatsapp://send?phone=12", "whatsapp://send?phone=1234567a", "https://a b", "", "vscode://x"] {
            assert!(!scheme_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn a_failed_job_alert_opens_at_the_failing_step_and_keeps_pr_and_branch() {
        let extras = GithubExtras {
            hints: crate::notify_links::OpenHints {
                job_id: Some(22),
                failed_step: Some(5),
                html_url: Some("https://github.com/alice/zync/actions/runs/555/job/22".into()),
                ..Default::default()
            },
            pr: Some(34),
            branch: Some("feat/x".into()),
        };
        let line = Line::Failed { project: "zync".into(), kind: Kind::Build, count: 1 };
        let ev = github_event_with("alice/zync", &fact("zync", Kind::Build, "555"), &line, 1_000, extras).expect("event");
        assert!(matches!(&ev.target, Target::GitHub { pr: Some(34), run_id: Some(555), .. }));
        assert!(ev.preview.contains("feat/x"));
        let url = crate::notify_open::event_url(&ev).expect("url");
        assert_eq!(url, "https://github.com/alice/zync/actions/runs/555/job/22#step:5:1");
    }

    #[test]
    fn a_bad_hint_is_dropped_but_the_alert_still_opens_the_run() {
        let extras = GithubExtras {
            hints: crate::notify_links::OpenHints { html_url: Some("https://evil.example/x".into()), ..Default::default() },
            pr: None,
            branch: None,
        };
        let line = Line::Failed { project: "zync".into(), kind: Kind::Build, count: 1 };
        let ev = github_event_with("alice/zync", &fact("zync", Kind::Build, "555"), &line, 1_000, extras).expect("event");
        assert_eq!(ev.hints, Default::default());
        assert_eq!(crate::notify_open::event_url(&ev).as_deref(), Some("https://github.com/alice/zync/actions/runs/555"));
    }

    use super::*;
    use crate::notify_brain::{Kind, Obs};

    fn fact(project: &str, kind: Kind, run: &str) -> Fact {
        Fact { service: "github", project: project.into(), kind, obs: Obs::Failed, run_ref: run.into() }
    }

    fn failed_line(project: &str, kind: Kind) -> Line {
        Line::Failed { project: project.into(), kind, count: 1 }
    }

    #[test]
    fn a_github_failure_becomes_an_event_with_the_full_owner_and_run() {
        let ev = github_event("alice/zync", &fact("zync", Kind::Pr, "555"), &failed_line("zync", Kind::Pr), 1_000).unwrap();
        assert_eq!(ev.status, Status::Failed);
        assert_eq!(ev.title, "PR failed in zync");
        assert!(matches!(&ev.target, Target::GitHub { owner, repo, run_id: Some(555), .. } if owner == "alice" && repo == "zync"));
        // The same bare project under another owner is a different entity.
        let other = github_event("bob/zync", &fact("zync", Kind::Pr, "555"), &failed_line("zync", Kind::Pr), 1_000).unwrap();
        assert_ne!(ev.entity_key, other.entity_key);
        assert_ne!(ev.event_id, other.event_id);
    }

    #[test]
    fn an_unusable_identity_gives_no_event() {
        let l = failed_line("zync", Kind::Build);
        assert!(github_event("zync", &fact("zync", Kind::Build, "1"), &l, 1).is_none(), "no owner");
        assert!(github_event("a/b", &fact("b", Kind::Build, "not-a-number"), &l, 1).is_none(), "bad run id");
        let traversal = github_event("a/../b", &fact("b", Kind::Build, "1"), &l, 1).unwrap();
        assert!(Ledger::new().record(traversal).is_err(), "a traversal repo is refused when recorded");
    }

    #[test]
    fn a_recovery_updates_the_failure_and_a_repeat_makes_no_second_card() {
        let mut led = Ledger::new();
        let f = github_event("a/zync", &fact("zync", Kind::Build, "10"), &failed_line("zync", Kind::Build), 1).unwrap();
        let id = f.event_id.clone();
        assert_eq!(led.record(f.clone()), Ok(Recorded::Created(id.clone())));
        assert_eq!(led.record(f), Ok(Recorded::Duplicate(id.clone())));
        let fixed = github_event("a/zync", &fact("zync", Kind::Build, "11"), &Line::Fixed { project: "zync".into(), kind: Kind::Build }, 2).unwrap();
        assert_eq!(led.record(fixed), Ok(Recorded::Recovered(id)));
    }

    #[test]
    fn only_failures_ask_to_pass_do_not_disturb_and_the_setting_can_stop_it() {
        assert!(wants_urgent(Status::Failed, true));
        assert!(!wants_urgent(Status::Recovered, true));
        assert!(!wants_urgent(Status::Info, true));
        assert!(!wants_urgent(Status::Failed, false));
    }

    #[test]
    fn alerts_open_in_the_coderabbit_interface_docked_right() {
        assert_eq!((DETAIL_VIEW, DETAIL_DOCK), ("coderabbit", "right"));
        // The animation target must be the rectangle the sidebar really gets.
        let work = PRect { x: 0, y: 0, w: 1920, h: 1032 };
        let (_, to) = expand_rects(PRect { x: 0, y: 0, w: 1920, h: 1080 }, work, 1.0, DETAIL_VIEW, Some(DETAIL_DOCK));
        let real = crate::work_area::sidebar_rect(DETAIL_VIEW, Some(DETAIL_DOCK), PRect { x: 0, y: 0, w: 1920, h: 1080 }, 1.0);
        assert_eq!((to.x, to.w), (real.x as f64, real.w as f64));
        let v = expand_json("PR failed in zync", Status::Failed, LRect { x: 1.0, y: 2.0, w: 3.0, h: 4.0 }, to);
        assert_eq!(v["status"], "failed");
        assert_eq!(v["durationMs"], EXPAND_MS);
        assert_eq!(v["from"]["w"], 3.0);
    }

    #[test]
    fn payloads_carry_only_validated_fields() {
        let mut led = Ledger::new();
        let ev = github_event("a/zync", &fact("zync", Kind::Pr, "9"), &failed_line("zync", Kind::Pr), 1).unwrap();
        let id = ev.event_id.clone();
        led.record(ev).unwrap();
        let mut op = Opener::new();
        let opened = op.begin(&id, &mut led, 5, &|_, _| true, Some("pr-list"), true);
        let v = detail_json(&opened, op.restore_view(), Some(42));
        assert_eq!(v["mode"], "event_detail");
        assert_eq!(v["restoreView"], "pr-list");
        assert_eq!(v["providerUrl"], "https://github.com/a/zync/actions/runs/9");
        assert_eq!(v["repo"], "a/zync");
        assert_eq!(v["kind"], "pr");
        assert_eq!(v["prNumber"], 42, "the PR found for the run is what the CodeRabbit review is loaded for");
        let gone = op.begin("ev_missing", &mut led, 5, &|_, _| true, None, false);
        let v = detail_json(&gone, None, None);
        assert!(v["unavailable"].is_string() && v.get("title").is_none());
    }

    #[test]
    fn deploy_event_open_urls_and_fallback_for_all_services() {
        let now = 1_000_000u64;
        let fail_line = Line::Failed { project: "zync".into(), kind: Kind::Deploy, count: 1 };
        let fixed_line = Line::Fixed { project: "zync".into(), kind: Kind::Deploy };

        // 1. Vercel
        let fact_vercel = Fact { service: "vercel", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Failed, run_ref: "dpl_1".into() };
        // Failure with inspectorUrl
        let extras_v1 = DeployExtras {
            team: "acme".into(),
            slug: "zync".into(),
            deployment_id: "dpl_1".into(),
            url: Some("https://vercel.com/acme/zync/dpl_1".into()),
        };
        let ev_v1 = deploy_event("vercel", "zync", &fact_vercel, &fail_line, now, extras_v1).expect("vercel failure event");
        assert_eq!(ev_v1.status, Status::Failed);
        assert_eq!(crate::notify_open::event_url(&ev_v1).as_deref(), Some("https://vercel.com/acme/zync/dpl_1"));

        // Recovery without inspectorUrl (uses fallback)
        let fact_vercel_rec = Fact { service: "vercel", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Fixed, run_ref: "dpl_2".into() };
        let extras_v2 = DeployExtras {
            team: "acme".into(),
            slug: "zync".into(),
            deployment_id: "dpl_2".into(),
            url: None,
        };
        let ev_v2 = deploy_event("vercel", "zync", &fact_vercel_rec, &fixed_line, now, extras_v2).expect("vercel recovery event");
        assert_eq!(ev_v2.status, Status::Recovered);
        assert_eq!(crate::notify_open::event_url(&ev_v2).as_deref(), Some("https://vercel.com/acme/zync/dpl_2"));

        // Evil inspectorUrl on another host is dropped and fallback is used
        let extras_v_evil = DeployExtras {
            team: "acme".into(),
            slug: "zync".into(),
            deployment_id: "dpl_3".into(),
            url: Some("https://evil.com/phish".into()),
        };
        let ev_v_evil = deploy_event("vercel", "zync", &fact_vercel, &fail_line, now, extras_v_evil).expect("vercel evil url event");
        assert_eq!(ev_v_evil.hints.html_url, None);
        assert_eq!(crate::notify_open::event_url(&ev_v_evil).as_deref(), Some("https://vercel.com/acme/zync/dpl_3"));

        // Unusable id returns None
        let extras_v_bad = DeployExtras {
            team: "acme".into(),
            slug: "zync".into(),
            deployment_id: "bad id with spaces".into(),
            url: None,
        };
        assert!(deploy_event("vercel", "zync", &fact_vercel, &fail_line, now, extras_v_bad).is_none());

        // 2. Render
        let fact_render = Fact { service: "render", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Failed, run_ref: "dep-101".into() };
        let extras_r1 = DeployExtras {
            team: "default".into(),
            slug: "srv-c123456".into(),
            deployment_id: "dep-101".into(),
            url: None,
        };
        let ev_r1 = deploy_event("render", "zync", &fact_render, &fail_line, now, extras_r1).expect("render failure event");
        assert_eq!(ev_r1.status, Status::Failed);
        assert_eq!(crate::notify_open::event_url(&ev_r1).as_deref(), Some("https://dashboard.render.com/web/srv-c123456/deploys/dep-101"));

        let fact_render_rec = Fact { service: "render", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Fixed, run_ref: "dep-102".into() };
        let extras_r2 = DeployExtras {
            team: "default".into(),
            slug: "srv-c123456".into(),
            deployment_id: "dep-102".into(),
            url: None,
        };
        let ev_r2 = deploy_event("render", "zync", &fact_render_rec, &fixed_line, now, extras_r2).expect("render recovery event");
        assert_eq!(ev_r2.status, Status::Recovered);
        assert_eq!(crate::notify_open::event_url(&ev_r2).as_deref(), Some("https://dashboard.render.com/web/srv-c123456/deploys/dep-102"));

        let extras_r_bad = DeployExtras {
            team: "default".into(),
            slug: "srv-c123456".into(),
            deployment_id: "bad/id".into(),
            url: None,
        };
        assert!(deploy_event("render", "zync", &fact_render, &fail_line, now, extras_r_bad).is_none());

        // 3. Netlify
        let fact_netlify = Fact { service: "netlify", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Failed, run_ref: "net-201".into() };
        let extras_n1 = DeployExtras {
            team: "default".into(),
            slug: "my-site".into(),
            deployment_id: "net-201".into(),
            url: None,
        };
        let ev_n1 = deploy_event("netlify", "zync", &fact_netlify, &fail_line, now, extras_n1).expect("netlify failure event");
        assert_eq!(ev_n1.status, Status::Failed);
        assert_eq!(crate::notify_open::event_url(&ev_n1).as_deref(), Some("https://app.netlify.com/projects/my-site/deploys/net-201"));

        let fact_netlify_rec = Fact { service: "netlify", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Fixed, run_ref: "net-202".into() };
        let extras_n2 = DeployExtras {
            team: "default".into(),
            slug: "my-site".into(),
            deployment_id: "net-202".into(),
            url: None,
        };
        let ev_n2 = deploy_event("netlify", "zync", &fact_netlify_rec, &fixed_line, now, extras_n2).expect("netlify recovery event");
        assert_eq!(ev_n2.status, Status::Recovered);
        assert_eq!(crate::notify_open::event_url(&ev_n2).as_deref(), Some("https://app.netlify.com/projects/my-site/deploys/net-202"));

        let extras_n_bad = DeployExtras {
            team: "default".into(),
            slug: "bad site with space".into(),
            deployment_id: "net-201".into(),
            url: None,
        };
        assert!(deploy_event("netlify", "zync", &fact_netlify, &fail_line, now, extras_n_bad).is_none());

        // 4. Supabase
        let fact_supabase = Fact { service: "supabase", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Failed, run_ref: "INIT_FAILED".into() };
        let extras_s1 = DeployExtras {
            team: "default".into(),
            slug: "abcdefghijkl".into(),
            deployment_id: "INIT_FAILED".into(),
            url: None,
        };
        let ev_s1 = deploy_event("supabase", "zync", &fact_supabase, &fail_line, now, extras_s1).expect("supabase failure event");
        assert_eq!(ev_s1.status, Status::Failed);
        assert_eq!(crate::notify_open::event_url(&ev_s1).as_deref(), Some("https://supabase.com/dashboard/project/abcdefghijkl"));

        let fact_supabase_rec = Fact { service: "supabase", project: "zync".into(), kind: Kind::Deploy, obs: Obs::Fixed, run_ref: "ACTIVE_HEALTHY".into() };
        let extras_s2 = DeployExtras {
            team: "default".into(),
            slug: "abcdefghijkl".into(),
            deployment_id: "ACTIVE_HEALTHY".into(),
            url: None,
        };
        let ev_s2 = deploy_event("supabase", "zync", &fact_supabase_rec, &fixed_line, now, extras_s2).expect("supabase recovery event");
        assert_eq!(ev_s2.status, Status::Recovered);
        assert_eq!(crate::notify_open::event_url(&ev_s2).as_deref(), Some("https://supabase.com/dashboard/project/abcdefghijkl"));

        let extras_s_bad = DeployExtras {
            team: "default".into(),
            slug: "".into(),
            deployment_id: "ACTIVE_HEALTHY".into(),
            url: None,
        };
        assert!(deploy_event("supabase", "zync", &fact_supabase, &fail_line, now, extras_s_bad).is_none());
    }

    #[test]
    fn calendar_event_urls_fallbacks_and_validation() {
        let now = 1_000_000u64;

        // 1. calendar_event -> notify_open::event_url(&ev) equals the event page when html_link is a google calendar URL
        let valid_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "cal_ev_123",
            summary: "Doctor Appointment",
            location: Some("123 Health Ave"),
            html_link: Some("https://calendar.google.com/calendar/event?eid=abc123xyz"),
            meet_url: Some("https://meet.google.com/abc-defg-hij"),
        };
        let ev = calendar_event("calendar", &valid_data, "Time to leave for Doctor Appointment.", Severity::High, now)
            .expect("valid calendar event");
        assert_eq!(ev.provider, Provider::Calendar);
        assert_eq!(ev.status, Status::Info);
        assert_eq!(ev.severity, Severity::High);
        assert_eq!(ev.title, "Doctor Appointment");
        assert_eq!(ev.preview, "Time to leave for Doctor Appointment.");
        assert_eq!(ev.hints.html_url.as_deref(), Some("https://calendar.google.com/calendar/event?eid=abc123xyz"));
        assert_eq!(ev.hints.meet_url.as_deref(), Some("https://meet.google.com/abc-defg-hij"));
        assert_eq!(
            crate::notify_open::event_url(&ev).as_deref(),
            Some("https://calendar.google.com/calendar/event?eid=abc123xyz")
        );

        // 2. A link on another host is dropped (falls back to https://calendar.google.com/calendar/)
        let evil_url_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "cal_ev_456",
            summary: "Team Sync",
            location: Some("Office"),
            html_link: Some("https://evil.com/phish"),
            meet_url: None,
        };
        let ev_evil = calendar_event("calendar", &evil_url_data, "Leave soon.", Severity::Medium, now)
            .expect("evil url event");
        assert_eq!(ev_evil.hints.html_url, None);
        assert_eq!(
            crate::notify_open::event_url(&ev_evil).as_deref(),
            Some("https://calendar.google.com/calendar/")
        );

        // 3. An unusable event id gives None
        let bad_id_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "invalid event id with spaces!",
            summary: "Invalid",
            location: None,
            html_link: None,
            meet_url: None,
        };
        assert!(calendar_event("calendar", &bad_id_data, "Leave now.", Severity::Low, now).is_none());

        let empty_id_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "",
            summary: "Empty",
            location: None,
            html_link: None,
            meet_url: None,
        };
        assert!(calendar_event("calendar", &empty_id_data, "Leave now.", Severity::Low, now).is_none());

        // 4. Meet link is kept only for meet.google.com
        let good_meet_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "cal_ev_789",
            summary: "Virtual Check-in",
            location: None,
            html_link: None,
            meet_url: Some("https://meet.google.com/xyz-uvwx-rst"),
        };
        let ev_meet = calendar_event("calendar", &good_meet_data, "Call starting.", Severity::Medium, now)
            .expect("good meet event");
        assert_eq!(ev_meet.hints.meet_url.as_deref(), Some("https://meet.google.com/xyz-uvwx-rst"));
        // Since html_link is None, event_url opens the Meet link!
        assert_eq!(
            crate::notify_open::event_url(&ev_meet).as_deref(),
            Some("https://meet.google.com/xyz-uvwx-rst")
        );

        let bad_meet_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "cal_ev_890",
            summary: "Zoom Call",
            location: None,
            html_link: None,
            meet_url: Some("https://zoom.us/j/123456789"),
        };
        let ev_bad_meet = calendar_event("calendar", &bad_meet_data, "Call starting.", Severity::Medium, now)
            .expect("bad meet event");
        assert_eq!(ev_bad_meet.hints.meet_url, None);
        // Both html_link and meet_url are None, so event_url falls back to calendar base URL
        assert_eq!(
            crate::notify_open::event_url(&ev_bad_meet).as_deref(),
            Some("https://calendar.google.com/calendar/")
        );

        // 5. Account id handling: email used when non-empty, otherwise "calendar"
        let acct_data = CalendarAlertData {
            start_epoch: 1_700_000_000,
            id: "cal_ev_999",
            summary: "Personal sync",
            location: None,
            html_link: None,
            meet_url: None,
        };
        let ev_acct = calendar_event("user@gmail.com", &acct_data, "Leave now.", Severity::Low, now).unwrap();
        assert_eq!(ev_acct.account_id, "user@gmail.com");

        let ev_default = calendar_event("", &acct_data, "Leave now.", Severity::Low, now).unwrap();
        assert_eq!(ev_default.account_id, "calendar");
    }

    #[test]
    fn whatsapp_event_normal_jid_ladder_preview_title_and_hash() {
        let now = 1_700_000_000u64;
        let jid = "919876543210@s.whatsapp.net";
        let ev = whatsapp_event("Alice Smith", jid, Some(100), Severity::Medium, now).expect("whatsapp event");

        // normal jid gives phone digits and a ladder of whatsapp://send?phone=... then https://wa.me/...
        assert_eq!(ev.hints.phone_digits.as_deref(), Some("919876543210"));
        let ladder = crate::notify_links::open_ladder(ev.provider, &ev.target, &ev.hints);
        assert_eq!(
            ladder,
            vec![
                crate::notify_links::OpenAction::Url("whatsapp://send?phone=919876543210".into()),
                crate::notify_links::OpenAction::Url("https://wa.me/919876543210".into()),
            ]
        );

        // preview is always empty and the title never contains message text
        assert_eq!(ev.preview, "");
        assert_eq!(ev.title, "Alice Smith");

        // empty contact name falls back to "WhatsApp"
        let ev_anon = whatsapp_event("", jid, Some(100), Severity::Medium, now).expect("anon event");
        assert_eq!(ev_anon.title, "WhatsApp");
        assert_eq!(ev_anon.preview, "");

        // chat id is a 24-hex hash (no '@')
        let chat = match &ev.target {
            Target::WhatsApp { chat } => chat.clone(),
            _ => panic!("expected Target::WhatsApp"),
        };
        assert_eq!(chat.len(), 24);
        assert!(!chat.contains('@'));
        assert!(chat.chars().all(|c| c.is_ascii_hexdigit()));

        // ledger validation passes
        assert!(crate::notify_event::validate_target(&ev.target).is_ok());

        // two alerts with different timestamps have different event ids but the same entity key
        let ev2 = whatsapp_event("Alice Smith", jid, Some(200), Severity::Medium, now).expect("second event");
        assert_eq!(ev.entity_key, ev2.entity_key);
        assert_ne!(ev.event_id, ev2.event_id);
    }

    #[test]
    fn whatsapp_event_lid_and_group_jids_give_empty_ladder_no_panic() {
        let now = 1_700_000_000u64;

        // @lid jid
        let ev_lid = whatsapp_event("Secret Contact", "12345678901234@lid", None, Severity::Low, now)
            .expect("lid event");
        assert_eq!(ev_lid.hints.phone_digits, None);
        assert_eq!(ev_lid.preview, "");
        assert!(crate::notify_links::open_ladder(ev_lid.provider, &ev_lid.target, &ev_lid.hints).is_empty());

        // @g.us group jid
        let ev_group = whatsapp_event("Project Group", "123456789-987654@g.us", None, Severity::High, now)
            .expect("group event");
        assert_eq!(ev_group.hints.phone_digits, None);
        assert_eq!(ev_group.preview, "");
        assert!(crate::notify_links::open_ladder(ev_group.provider, &ev_group.target, &ev_group.hints).is_empty());

        // invalid empty jid gives None
        assert!(whatsapp_event("Alice", "", None, Severity::Low, now).is_none());
        assert!(whatsapp_event("Alice", "   ", None, Severity::Low, now).is_none());
    }

    #[test]
    fn agent_event_needs_input_vs_finished_titles_kind_and_severity() {
        let now = 1_000_000u64;
        let ev_needs = agent_event("claude", "Claude Code", Some("/home/user/nexus"), true, 500, now).expect("needs_input");
        assert_eq!(ev_needs.title, "Claude Code needs your input");
        assert_eq!(ev_needs.kind, "needs_input");
        assert_eq!(ev_needs.severity, Severity::Medium);
        assert_eq!(ev_needs.status, Status::Info);
        assert_eq!(ev_needs.account_id, "local");
        assert_eq!(ev_needs.preview, "nexus");

        let ev_fin = agent_event("codex", "Codex", Some("/home/user/nexus"), false, 600, now).expect("finished");
        assert_eq!(ev_fin.title, "Codex finished");
        assert_eq!(ev_fin.kind, "finished");
        assert_eq!(ev_fin.severity, Severity::Low);
        assert_eq!(ev_fin.status, Status::Info);
        assert_eq!(ev_fin.account_id, "local");
        assert_eq!(ev_fin.preview, "nexus");
    }

    #[test]
    fn agent_event_project_sanitization_and_cwd_hints() {
        let now = 1_000_000u64;

        // Spaces
        let ev_space = agent_event("claude", "Claude Code", Some("/home/user/my cool project"), true, 1, now).unwrap();
        assert_eq!(ev_space.preview, "my-cool-project");
        assert!(matches!(&ev_space.target, Target::Agent { project, .. } if project == "my-cool-project"));

        // Unicode
        let ev_unicode = agent_event("claude", "Claude Code", Some("/projects/proj🦀ect"), true, 2, now).unwrap();
        assert_eq!(ev_unicode.preview, "proj-ect");
        assert!(matches!(&ev_unicode.target, Target::Agent { project, .. } if project == "proj-ect"));

        // Drive letters
        let ev_drive = agent_event("claude", "Claude Code", Some("C:\\"), true, 3, now).unwrap();
        assert_eq!(ev_drive.preview, "C-");
        assert!(matches!(&ev_drive.target, Target::Agent { project, .. } if project == "C-"));

        // Trailing slash
        let ev_slash = agent_event("claude", "Claude Code", Some("D:/ULTRON/"), true, 4, now).unwrap();
        assert_eq!(ev_slash.preview, "ULTRON");
        assert!(matches!(&ev_slash.target, Target::Agent { project, .. } if project == "ULTRON"));

        // Empty cwd -> "session"
        let ev_empty = agent_event("claude", "Claude Code", Some(""), true, 5, now).unwrap();
        assert_eq!(ev_empty.preview, "session");
        assert!(matches!(&ev_empty.target, Target::Agent { project, .. } if project == "session"));

        let ev_none = agent_event("claude", "Claude Code", None, true, 6, now).unwrap();
        assert_eq!(ev_none.preview, "session");
        assert!(matches!(&ev_none.target, Target::Agent { project, .. } if project == "session"));

        // source_ref differs per prompt
        let ev_p1 = agent_event("claude", "Claude Code", Some("/path/proj"), true, 100, now).unwrap();
        let ev_p2 = agent_event("claude", "Claude Code", Some("/path/proj"), true, 200, now).unwrap();
        assert_eq!(ev_p1.source_ref, "100");
        assert_eq!(ev_p2.source_ref, "200");
        assert_ne!(ev_p1.event_id, ev_p2.event_id);

        // Invalid cwd with control characters is dropped from hints but event still builds
        let ev_bad_cwd = agent_event("claude", "Claude Code", Some("/bad/path\x07\x08"), true, 300, now).unwrap();
        assert_eq!(ev_bad_cwd.hints.agent_cwd, None);
        let ladder_bad = crate::notify_links::open_ladder(ev_bad_cwd.provider, &ev_bad_cwd.target, &ev_bad_cwd.hints);
        assert!(ladder_bad.is_empty(), "ladder is empty when hints have no valid cwd");

        // Valid cwd gives FocusAgent ladder
        let ev_valid_cwd = agent_event("claude", "Claude Code", Some("/valid/path"), true, 400, now).unwrap();
        assert_eq!(ev_valid_cwd.hints.agent_cwd.as_deref(), Some("/valid/path"));
        let ladder_valid = crate::notify_links::open_ladder(ev_valid_cwd.provider, &ev_valid_cwd.target, &ev_valid_cwd.hints);
        assert_eq!(ladder_valid, vec![crate::notify_links::OpenAction::FocusAgent { cwd: "/valid/path".into() }]);
    }

    #[test]
    fn folder_name_from_cwd_extracts_basename() {
        assert_eq!(folder_name_from_cwd("D:\\ULTRON"), Some("ULTRON"));
        assert_eq!(folder_name_from_cwd("D:/ULTRON/"), Some("ULTRON"));
        assert_eq!(folder_name_from_cwd("/home/user/project"), Some("project"));
        assert_eq!(folder_name_from_cwd("/home/user/project/"), Some("project"));
        assert_eq!(folder_name_from_cwd(""), None);
        assert_eq!(folder_name_from_cwd("   "), None);
    }

    #[test]
    fn detail_json_contains_event_id_and_can_open_flag() {
        let extras = GithubExtras { hints: crate::notify_links::OpenHints::default(), pr: None, branch: None };
        let line = Line::Failed { project: "zync".into(), kind: Kind::Build, count: 1 };
        let gh_ev = github_event_with("alice/zync", &fact("zync", Kind::Build, "555"), &line, 1_000, extras).unwrap();
        let gh_detail = crate::notify_open::detail(1, &gh_ev);
        let gh_json = detail_json(&crate::notify_open::Opened::Detail(Box::new(gh_detail)), None, None);
        assert_eq!(gh_json["eventId"], gh_ev.event_id);
        assert_eq!(gh_json["canOpen"], true);

        // WhatsApp event without phone digits gives an empty ladder and canOpen false.
        let wa_ev = whatsapp_event("Support", "group@g.us", None, Severity::Low, 1_000).unwrap();
        assert!(wa_ev.hints.phone_digits.is_none());
        assert!(crate::notify_open::event_ladder(&wa_ev).is_empty());
        let wa_detail = crate::notify_open::detail(2, &wa_ev);
        let wa_json = detail_json(&crate::notify_open::Opened::Detail(Box::new(wa_detail)), None, None);
        assert_eq!(wa_json["eventId"], wa_ev.event_id);
        assert_eq!(wa_json["canOpen"], false);
    }

    #[test]
    fn notify_open_external_pure_helper_logic() {
        use crate::notify_links::OpenAction;

        // Empty ladder returns error
        assert_eq!(first_openable_action(&[]), Err("This alert has no page to open."));

        // Blocked schemes return error
        let blocked = vec![
            OpenAction::Url("javascript:alert(1)".into()),
            OpenAction::Url("http://insecure.example.com".into()),
            OpenAction::Url("file:///C:/secret.txt".into()),
        ];
        assert_eq!(first_openable_action(&blocked), Err("This alert has no page to open."));

        // Valid url is picked first
        let valid_url = OpenAction::Url("https://github.com/alice/zync/actions/runs/555".into());
        let ladder = vec![
            OpenAction::Url("javascript:void(0)".into()),
            valid_url.clone(),
            OpenAction::Url("https://github.com/alice/zync".into()),
        ];
        assert_eq!(first_openable_action(&ladder), Ok(&valid_url));

        // FocusAgent with valid cwd is picked
        let agent = OpenAction::FocusAgent { cwd: "D:/ULTRON".into() };
        assert_eq!(first_openable_action(&[agent.clone()]), Ok(&agent));

        // FocusAgent with empty cwd returns error
        let empty_agent = OpenAction::FocusAgent { cwd: "   ".into() };
        assert_eq!(first_openable_action(&[empty_agent]), Err("This alert has no page to open."));
    }

    fn make_test_event(i: u64, now: u64) -> NotifyEvent {
        let line = Line::Failed { project: format!("proj_{i}"), kind: Kind::Build, count: 1 };
        let extras = GithubExtras {
            hints: Default::default(),
            pr: Some(i),
            branch: Some("main".into()),
        };
        github_event_with(&format!("owner/repo_{i}"), &fact(&format!("proj_{i}"), Kind::Build, &format!("{i}")), &line, now, extras).unwrap()
    }

    static TEST_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_list_ordering_and_cap_50() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_alerts();
        for i in 1..=55 {
            let ev = make_test_event(i, 10_000 + i * 10);
            record_test_event(ev);
        }
        let list = list_events_with_privacy(crate::notify_toast::Privacy::Normal);
        assert_eq!(list.len(), 50, "must cap at 50 events");
        for w in list.windows(2) {
            let t1 = w[0]["updatedAtMs"].as_u64().unwrap();
            let t2 = w[1]["updatedAtMs"].as_u64().unwrap();
            assert!(t1 >= t2, "expected newest first: {t1} >= {t2}");
        }
        let first = &list[0];
        assert!(first.get("eventId").is_some());
        assert_eq!(first["provider"], "GitHub");
        assert_eq!(first["status"], "failed");
        assert!(first.get("title").is_some());
        assert!(first.get("preview").is_some());
        assert!(first.get("icon").is_some());
        assert_eq!(first["read"], false);
        assert_eq!(first["spoken"], false);
        assert_eq!(first["count"], 1);
        clear_alerts();
    }

    #[test]
    fn test_privacy_hidden_masks() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_alerts();
        let ev = make_test_event(1, 1_000);
        record_test_event(ev);

        let normal = list_events_with_privacy(crate::notify_toast::Privacy::Normal);
        assert_eq!(normal.len(), 1);
        assert_ne!(normal[0]["title"], "GitHub alert");
        assert_ne!(normal[0]["preview"], "");

        let hidden = list_events_with_privacy(crate::notify_toast::Privacy::Hidden);
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0]["title"], "GitHub alert");
        assert_eq!(hidden[0]["preview"], "");
        clear_alerts();
    }

    #[test]
    fn test_clear_empties_both_ledger_and_store() {
        let _g = TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        clear_alerts();
        let temp = tempfile::tempdir().expect("tempdir");
        let temp_path = temp.path().to_path_buf();
        set_app_data_dir(temp_path.clone());

        let ev = make_test_event(42, 5_000);
        let id = ev.event_id.clone();
        record_test_event(ev.clone());
        save_event_best_effort(&ev);

        assert!(has_event(&id));
        let stored_before = crate::memcore::with_store(&temp_path, |s| s.load_notify_events(0)).unwrap_or_default();
        assert!(!stored_before.is_empty(), "store should have the saved event");

        notify_alerts_clear();

        assert!(!has_event(&id), "ledger must be empty after clear");
        assert!(lock().ledger.entries().is_empty());

        let stored_after = crate::memcore::with_store(&temp_path, |s| s.load_notify_events(0)).unwrap_or_default();
        assert!(stored_after.is_empty(), "store must be empty after clear");
    }

    #[test]
    fn test_mute_list_parsing() {
        assert_eq!(
            parse_muted_providers(r#"{"notifyMuted": ["GitHub", "Vercel", "MAIL"]}"#),
            vec!["github", "vercel", "mail"]
        );
        assert_eq!(
            parse_muted_providers(r#"["whatsapp", "supabase"]"#),
            vec!["whatsapp", "supabase"]
        );
        assert_eq!(parse_muted_providers(r#"{}"#), Vec::<String>::new());
        assert_eq!(parse_muted_providers("not json"), Vec::<String>::new());
    }

    #[test]
    fn test_publish_decision_with_muted_provider_and_failure_exception() {
        use crate::notify_toast::{decide_delivery, Delivery};

        let muted = vec!["github".to_string(), "vercel".to_string(), "whatsapp".to_string()];

        let (d, r) = decide_delivery(true, true, true, None, Provider::WhatsApp, Status::Info, &muted);
        assert_eq!(d, Delivery::History);
        assert_eq!(r, "muted");

        let (d, r) = decide_delivery(true, true, true, None, Provider::GitHub, Status::Recovered, &muted);
        assert_eq!(d, Delivery::History);
        assert_eq!(r, "muted");

        let (d, r) = decide_delivery(true, true, true, None, Provider::GitHub, Status::Failed, &muted);
        assert_eq!(d, Delivery::Custom);
        assert_eq!(r, "ok");

        let (d, r) = decide_delivery(true, true, true, None, Provider::Vercel, Status::Failed, &muted);
        assert_eq!(d, Delivery::Custom);
        assert_eq!(r, "ok");

        let (d, r) = decide_delivery(true, true, true, None, Provider::Vercel, Status::Recovered, &muted);
        assert_eq!(d, Delivery::History);
        assert_eq!(r, "muted");

        let (d, r) = decide_delivery(true, true, true, None, Provider::Gmail, Status::Info, &muted);
        assert_eq!(d, Delivery::Custom);
        assert_eq!(r, "ok");
    }
}
