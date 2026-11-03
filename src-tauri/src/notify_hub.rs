//! Production glue for notification-to-sidebar: one process-wide ledger + opener, the producer entry point
//! (`publish_github`), the activation path (`activate`) and the IPC commands the sidebar calls. Identity,
//! resolution and the opening transaction live in `notify_event` / `notify_open`; this file only connects them to Tauri.
//!
//! An alert becomes a Windows notification-center toast only when it was recorded as new (or as a recovery of a known
//! failure); grouped/duplicate observations never produce a second toast. Clicking the toast calls `activate`: the
//! full-screen overlay plays the expand animation, then the alert opens in the CodeRabbit sidebar. Opening is read-only.

use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
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
const EVENT_TTL_MS: u64 = 7 * 24 * 3600 * 1000;
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

/// Pure: the ledger event for a GitHub observation of a UNIQUELY resolved `owner/repo`. None when the run id or
/// the repository identity is not usable (no card is better than a card that opens the wrong thing).
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
    let Some(mut ev) = github_event_with(full_name, fact, line, now, extras) else {
        return;
    };
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
    let custom_on = dir.as_ref().map(|d| crate::memcore::flag(d, "notifyToast", true)).unwrap_or(true);
    let delivery = crate::notify_toast::decide(custom_on, toast_on, crate::proactive_policy::someone_is_here(), crate::notify_toast::shell_state());
    let privacy = crate::notify_toast::Privacy::parse(&read_setting_str(dir.as_deref(), "notifyPrivacy").unwrap_or_default());
    let channel = if delivery == crate::notify_toast::Delivery::History { Channel::HistoryOnly } else { channel };
    let outcome = {
        let mut h = lock();
        h.ledger.prune(now);
        let out = h.ledger.record(ev);
        if let Ok(Recorded::Created(id) | Recorded::Recovered(id)) = &out {
            h.ledger.mark_delivered(id, false, channel == Channel::Toast, true);
        }
        out
    };
    let (Recorded::Created(id) | Recorded::Recovered(id)) = (match outcome {
        Ok(o) => o,
        Err(e) => {
            tracing::warn!("[NOTIFY] event refused: {e:?}");
            return;
        }
    }) else {
        return;
    };
    if channel == Channel::HistoryOnly {
        return;
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
            return;
        }
    }
    show_native(app, &id, &title, &preview, status);
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

fn run_action(a: &crate::notify_links::OpenAction) -> bool {
    use crate::notify_links::OpenAction;
    match a {
        OpenAction::Url(u) if scheme_allowed(u) => open::that(u).is_ok(),
        OpenAction::FocusAgent { cwd } => std::path::Path::new(cwd).is_dir() && open::that(cwd).is_ok(),
        _ => false,
    }
}

/// A toast click: open the real item (the first entry of the alert's ladder that works); if none can be opened, show
/// the alert's detail in the sidebar instead. The id is resolved in the ledger; no URL ever comes from the page.
pub async fn open_external<R: Runtime>(app: &AppHandle<R>, event_id: &str) -> Result<(), String> {
    let ladder = lock().ledger.get(event_id).map(crate::notify_open::event_ladder);
    let Some(ladder) = ladder else {
        return Err("this alert is no longer available".into());
    };
    if ladder.iter().any(run_action) {
        return Ok(());
    }
    activate(app.clone(), event_id.to_string()).await;
    Ok(())
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
        }),
        Opened::Unavailable(u) => json!({
            "mode": "event_detail",
            "token": u.token,
            "unavailable": u.message,
            "restoreView": restore_view,
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
    let (token, payload) = {
        let mut guard = lock();
        let Hub { ledger, opener } = &mut *guard;
        let connected = |p: Provider, _: &str| match p {
            Provider::GitHub => github_connected(),
            _ => false,
        };
        let opened = opener.begin(&event_id, ledger, now_ms(), &connected, current_view.as_deref(), pinned);
        (opened.token(), detail_json(&opened, opener.restore_view(), pr_hint))
    };
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

#[cfg(test)]
mod tests {
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
}
