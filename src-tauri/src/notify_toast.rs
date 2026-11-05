//! The five-second heads-up toast window (notification toast, phase 4).
//!
//! A small NEXUS window at the top-right of the work area that shows one alert, never takes focus, and opens the real
//! item (browser / app) when clicked. The Windows-native toast stays the fallback channel (Focus Session, fullscreen,
//! presentation, nobody at the PC, window failure) so the owner's earlier "Windows notification centre" decision holds.
//!
//! The window only ever receives a display payload; a click carries the opaque event id and Rust builds the URL from the
//! typed target (`notify_links`), so nothing the toast shows can steer where a click goes.
//!
//! Pure parts (placement, channel decision, privacy, payload) are unit-tested; the window glue is Windows-only.

use crate::notify_event::{NotifyEvent, Provider, Status};
use crate::work_area::PRect;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, Runtime};

pub const LABEL: &str = "notify-toast";
/// Gap to the work-area edge and to the right-docked sidebar, in logical pixels.
const MARGIN: f64 = 16.0;
/// How long the toast stays (the page runs the timer and pauses it on hover).
pub const DEFAULT_MS: u64 = 5_000;
/// Rust-side safety net: the window is destroyed even if the page never reports back.
const HARD_CAP_MS: u64 = 60_000;

// ─── pure: placement ──────────────────────────────────────────────────

/// Toast width in logical px (the owner picked this size on 2026-11-04: a compact notification, not a panel).
pub const TOAST_WIDTH: f64 = 320.0;
/// Tallest the toast may be (logical px).
pub const TOAST_MAX_HEIGHT: f64 = 352.0;

/// Logical height that fits `rows` detail lines: header, a two-line headline, the rows and the action row. Pure.
pub fn toast_height(rows: usize) -> f64 {
    (148.0 + 20.0 * rows.min(4) as f64).min(TOAST_MAX_HEIGHT)
}

/// Top-right rectangle (physical px) of the toast inside `work`: `TOAST_WIDTH` wide and `height_logical` tall,
/// `MARGIN` from the edges, and left of the sidebar when that is open on the right. Always inside `work`.
pub fn toast_rect(work: PRect, scale: f64, sidebar_open: bool, height_logical: f64) -> PRect {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let w = ((TOAST_WIDTH * scale).round() as i32).clamp(1, work.w.max(1));
    let h = ((height_logical.clamp(60.0, TOAST_MAX_HEIGHT) * scale).round() as i32).clamp(1, work.h.max(1));
    let margin = (MARGIN * scale).round() as i32;
    // The open sidebar (right dock) is wider than the toast: step left of ITS edge, not just of our own width.
    let sidebar_w = crate::commands::sidebar_geometry("assistant", None, work.w as f64 / scale, work.h as f64 / scale).2;
    let shift = if sidebar_open { (sidebar_w * scale).round() as i32 + margin } else { 0 };
    let x = (work.x + work.w - w - margin - shift).clamp(work.x, work.x + (work.w - w).max(0));
    let y = (work.y + margin).clamp(work.y, work.y + (work.h - h).max(0));
    PRect { x, y, w, h }
}

// ─── pure: which channel ──────────────────────────────────────────────

/// `SHQueryUserNotificationState`: 1 not present, 2 busy (fullscreen), 3 D3D fullscreen, 4 presentation, 5 accepts,
/// 6 quiet time (Focus Session / Do Not Disturb), 7 Store app. A banner would be wrong in 1-4 and 6.
pub fn shell_blocks_banner(state: i32) -> bool {
    matches!(state, 1 | 2 | 3 | 4 | 6)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// The NEXUS toast window (5 s heads-up).
    Custom,
    /// The Windows notification centre toast.
    Native,
    /// Recorded in the ledger only.
    History,
}

/// Pure channel decision. `custom_on`/`native_on` are the owner's switches; `present` is "someone is at the PC";
/// `shell_state` is `SHQueryUserNotificationState` (None = unknown, treated as free).
pub fn decide(custom_on: bool, native_on: bool, present: bool, shell_state: Option<i32>) -> Delivery {
    let fallback = if native_on { Delivery::Native } else { Delivery::History };
    if !custom_on {
        return fallback;
    }
    if !present || shell_state.is_some_and(shell_blocks_banner) {
        return fallback;
    }
    Delivery::Custom
}

/// Pure reason string for why a delivery channel was chosen.
pub fn delivery_reason(
    custom_on: bool,
    native_on: bool,
    present: bool,
    shell_state: Option<i32>,
) -> &'static str {
    if !custom_on {
        return if native_on { "native fallback" } else { "toast switched off" };
    }
    if !present {
        return if native_on { "nobody at the PC" } else { "history only" };
    }
    match shell_state {
        Some(1) => if native_on { "nobody at the PC" } else { "history only" },
        Some(2 | 3 | 4) => if native_on { "fullscreen or presentation" } else { "history only" },
        Some(6) => if native_on { "focus session" } else { "history only" },
        _ => "ok",
    }
}

/// Pure: is this provider muted? Muted providers drop the toast, but failures from GitHub or deploy
/// services are never muted silently.
pub fn is_muted(provider: Provider, status: Status, muted: &[String]) -> bool {
    let id = match provider {
        Provider::GitHub => "github",
        Provider::Gmail => "mail",
        Provider::Vercel => "vercel",
        Provider::Render => "render",
        Provider::Netlify => "netlify",
        Provider::Supabase => "supabase",
        Provider::WhatsApp => "whatsapp",
        Provider::Calendar => "calendar",
        Provider::Agent => "agent",
    };
    let is_in_muted = muted.iter().any(|m| {
        let m = m.trim().to_ascii_lowercase();
        m == id || (id == "mail" && m == "gmail") || (id == "gmail" && m == "mail")
    });
    if !is_in_muted {
        return false;
    }
    let is_failure_exception = status == Status::Failed
        && matches!(
            provider,
            Provider::GitHub | Provider::Vercel | Provider::Render | Provider::Netlify | Provider::Supabase
        );
    !is_failure_exception
}

/// Pure: channel decision considering muted providers and failure exceptions.
pub fn decide_delivery(
    custom_on: bool,
    native_on: bool,
    present: bool,
    shell_state: Option<i32>,
    provider: Provider,
    status: Status,
    muted_providers: &[String],
) -> (Delivery, &'static str) {
    if is_muted(provider, status, muted_providers) {
        return (Delivery::History, "muted");
    }
    let d = decide(custom_on, native_on, present, shell_state);
    let r = delivery_reason(custom_on, native_on, present, shell_state);
    (d, r)
}

// ─── pure: what the toast shows ───────────────────────────────────────

/// How much of an alert's text the toast shows. Content that may be private (mail, chat) is cut first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Privacy {
    /// Everything the event carries (sender, subject, one line).
    Normal,
    /// Sender and subject only; no snippet. The default.
    Private,
    /// Source and count only.
    Hidden,
}

impl Privacy {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "normal" => Privacy::Normal,
            "hidden" => Privacy::Hidden,
            _ => Privacy::Private,
        }
    }
}

pub fn label(p: Provider) -> &'static str {
    match p {
        Provider::GitHub => "GitHub",
        Provider::Gmail => "Mail",
        Provider::Vercel => "Vercel",
        Provider::Render => "Render",
        Provider::Netlify => "Netlify",
        Provider::Supabase => "Supabase",
        Provider::WhatsApp => "WhatsApp",
        Provider::Calendar => "Calendar",
        Provider::Agent => "Agent",
    }
}

/// Pure: the stable logo id the page maps to a bundled file (`/activity-logos/<id>.svg`). Agents use their own id so a
/// Codex alert shows the Codex mark; an unknown agent gets "agent" and the page falls back to a monogram.
pub fn icon_id(ev: &NotifyEvent) -> String {
    use crate::notify_event::Target;
    match ev.provider {
        Provider::GitHub => "github".into(),
        Provider::Gmail => "gmail".into(),
        Provider::Vercel => "vercel".into(),
        Provider::Render => "render".into(),
        Provider::Netlify => "netlify".into(),
        Provider::Supabase => "supabase".into(),
        Provider::WhatsApp => "whatsapp".into(),
        Provider::Calendar => "googlecalendar".into(),
        Provider::Agent => {
            let name = match &ev.target {
                Target::Agent { agent, .. } => agent.to_ascii_lowercase(),
                _ => String::new(),
            };
            for (needle, id) in [
                ("claude", "claude"),
                ("codex", "codex"),
                ("antigravity", "antigravity"),
                ("opencode", "opencode"),
                ("cursor", "cursor"),
                ("gemini", "gemini"),
                ("copilot", "copilot"),
                ("windsurf", "windsurf"),
                ("aider", "aider"),
            ] {
                if name.contains(needle) {
                    return id.into();
                }
            }
            "agent".into()
        }
    }
}

pub fn status_word(s: Status) -> &'static str {
    match s {
        Status::Failed => "failed",
        Status::Recovered => "recovered",
        Status::Info => "info",
    }
}

fn action_label(ev: &NotifyEvent) -> &'static str {
    match ev.provider {
        Provider::GitHub => {
            if ev.hints.html_url.is_some() {
                "View log"
            } else if ev.kind == "pr" {
                "View PR"
            } else {
                "View run"
            }
        }
        Provider::Gmail => "Open mail",
        Provider::Vercel | Provider::Render | Provider::Netlify | Provider::Supabase => "View deploy",
        Provider::WhatsApp => "Open chat",
        Provider::Calendar => "Open event",
        Provider::Agent => "Go to agent",
    }
}

/// Pure: the (title, preview) the page may show for `ev` under `mode`.
pub fn visible_text(mode: Privacy, ev: &NotifyEvent) -> (String, String) {
    match mode {
        Privacy::Normal => (ev.title.clone(), ev.preview.clone()),
        // Private = sender and subject (mail) or contact (chat): that is all an event stores, snippets are never kept.
        Privacy::Private => (ev.title.clone(), ev.preview.clone()),
        Privacy::Hidden => (format!("{} alert", label(ev.provider)), String::new()),
    }
}

/// Pure: the big line and up to four label/value rows shown under it, per source. Built from the event's typed fields and
/// its (already sanitized) title and preview; nothing here can carry a URL.
pub fn headline_and_rows(ev: &NotifyEvent, mode: Privacy) -> (String, Vec<(String, String)>) {
    use crate::notify_event::Target;
    let (title, preview) = visible_text(mode, ev);
    if mode == Privacy::Hidden {
        return (title, Vec::new());
    }
    let clip = |s: &str| -> String { s.chars().take(80).collect() };
    let mut rows: Vec<(String, String)> = Vec::new();
    let mut add = |l: &str, v: &str| {
        if !v.trim().is_empty() && rows.len() < 4 {
            rows.push((l.to_string(), clip(v.trim())));
        }
    };
    let parts: Vec<&str> = preview.split(" · ").collect();
    let headline = match &ev.target {
        Target::GitHub { owner, repo, pr, run_id, .. } => {
            add("Repo", &format!("{owner}/{repo}"));
            if parts.len() >= 3 {
                add("Branch", parts[1]);
            }
            if let Some(n) = pr {
                add("PR", &format!("#{n}"));
            } else if let Some(r) = run_id {
                add("Run", &format!("#{r}"));
            }
            if let Some(s) = ev.hints.failed_step {
                add("Failed step", &format!("step {s}"));
            }
            title
        }
        Target::Repo { owner, repo, .. } => {
            add("Repo", &format!("{owner}/{repo}"));
            title
        }
        Target::Gmail { .. } => {
            if preview.is_empty() {
                title
            } else {
                add("From", &title);
                preview
            }
        }
        Target::Deployment { .. } => {
            add("Project", parts.first().copied().unwrap_or(""));
            if parts.len() >= 2 {
                add("Deploy", parts[1]);
            }
            title
        }
        Target::Calendar { .. } => {
            add("Reminder", &preview);
            title
        }
        Target::WhatsApp { .. } => {
            add("Chat", "new message");
            title
        }
        Target::Agent { .. } => {
            add("Folder", &preview);
            title
        }
    };
    (headline, rows)
}

/// Pure: the display payload sent to the toast page. Carries the opaque event id, never a URL.
pub fn payload(ev: &NotifyEvent, mode: Privacy, duration_ms: u64, seq: u64) -> Value {
    let (title, preview) = visible_text(mode, ev);
    let (headline, rows) = headline_and_rows(ev, mode);
    let rows: Vec<Value> = rows.into_iter().map(|(l, v)| json!({ "label": l, "value": v })).collect();
    json!({
        "headline": headline,
        "rows": rows,
        "seq": seq,
        "eventId": ev.event_id,
        "provider": label(ev.provider),
        "icon": icon_id(ev),
        "status": status_word(ev.status),
        "kind": ev.kind,
        "title": title,
        "preview": preview,
        "count": ev.occurrences,
        "action": action_label(ev),
        "durationMs": duration_ms,
    })
}

// ─── window glue ──────────────────────────────────────────────────────

static SEQ: AtomicU64 = AtomicU64::new(0);
static PENDING: Mutex<Option<Value>> = Mutex::new(None);

fn pending() -> std::sync::MutexGuard<'static, Option<Value>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Windows reports "busy" (2) whenever a full-screen window exists, and NEXUS's own stage overlay is one (measured
/// 2026-11-04: state 2 with NEXUS running, 5 with it stopped), so the custom toast would never show. "Busy" counts only
/// when the window in FRONT is really full screen. Pure.
pub fn resolve_busy(raw: i32, foreground_fullscreen: bool) -> i32 {
    if raw == 2 && !foreground_fullscreen {
        5
    } else {
        raw
    }
}

/// Is the foreground window a real full-screen window (a video, a game, F11)? It covers its whole monitor, has no
/// caption and is not maximised (a maximised window also overhangs the monitor by its border). Pure core + Win32 glue.
pub fn is_fullscreen_rect(win: (i32, i32, i32, i32), monitor: (i32, i32, i32, i32), has_caption: bool, maximized: bool) -> bool {
    let covers = win.0 <= monitor.0 && win.1 <= monitor.1 && win.2 >= monitor.2 && win.3 >= monitor.3;
    covers && !has_caption && !maximized
}

#[cfg(target_os = "windows")]
fn foreground_fullscreen() -> bool {
    #[repr(C)]
    struct Rect {
        l: i32,
        t: i32,
        r: i32,
        b: i32,
    }
    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
        fn GetWindowRect(hwnd: isize, r: *mut Rect) -> i32;
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn IsZoomed(hwnd: isize) -> i32;
        fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
        fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
    }
    const GWL_STYLE: i32 = -16;
    const WS_CAPTION: isize = 0x00C0_0000;
    const MONITOR_DEFAULTTONEAREST: u32 = 2;
    // SAFETY: plain user32 calls with valid out pointers; the handle is only read.
    unsafe {
        let fg = GetForegroundWindow() as isize;
        if fg == 0 {
            return false;
        }
        let mut r = Rect { l: 0, t: 0, r: 0, b: 0 };
        if GetWindowRect(fg, &mut r) == 0 {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST);
        let mut mi = MonitorInfo { size: std::mem::size_of::<MonitorInfo>() as u32, monitor: Rect { l: 0, t: 0, r: 0, b: 0 }, work: Rect { l: 0, t: 0, r: 0, b: 0 }, flags: 0 };
        if mon == 0 || GetMonitorInfoW(mon, &mut mi) == 0 {
            return false;
        }
        let has_caption = GetWindowLongPtrW(fg, GWL_STYLE) & WS_CAPTION == WS_CAPTION;
        is_fullscreen_rect((r.l, r.t, r.r, r.b), (mi.monitor.l, mi.monitor.t, mi.monitor.r, mi.monitor.b), has_caption, IsZoomed(fg) != 0)
    }
}

/// The shell's view of whether a banner is welcome (Windows only; None elsewhere or on failure).
pub fn shell_state() -> Option<i32> {
    #[cfg(target_os = "windows")]
    {
        #[link(name = "shell32")]
        extern "system" {
            fn SHQueryUserNotificationState(state: *mut i32) -> i32;
        }
        let mut s = 0i32;
        // SAFETY: plain shell32 call with a valid out pointer.
        let hr = unsafe { SHQueryUserNotificationState(&mut s) };
        (hr >= 0).then(|| resolve_busy(s, foreground_fullscreen()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Accessibility: Windows lets the user keep banners up for longer; never show it for less than that.
fn duration_ms() -> u64 {
    DEFAULT_MS
}

/// Page start-up: the pending payload (cold mount), taken once.
#[tauri::command]
pub fn get_pending_toast() -> Option<Value> {
    pending().take()
}

/// The page ran out of time or the user dismissed it.
#[tauri::command]
pub async fn notify_toast_close<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    close(&app);
    Ok(())
}

/// The user clicked the toast (or its button): open the sidebar detail first, then close.
#[tauri::command]
pub async fn notify_toast_click<R: Runtime>(app: AppHandle<R>, event_id: String) -> Result<(), String> {
    close(&app);
    if crate::notify_hub::has_event(&event_id) {
        crate::notify_hub::activate(app, event_id).await;
        Ok(())
    } else {
        crate::notify_hub::open_external(&app, &event_id, false).await
    }
}

fn close<R: Runtime>(app: &AppHandle<R>) {
    *pending() = None;
    let _ = crate::dyn_windows::destroy_window(app, LABEL);
}

/// Where the toast goes now (physical px): the primary monitor's work area, left of the sidebar when it is open.
fn placement<R: Runtime>(app: &AppHandle<R>, height_logical: f64) -> Result<PRect, String> {
    let monitor = app.primary_monitor().map_err(|e| e.to_string())?.ok_or("no monitor")?;
    let wa = monitor.work_area();
    let work = PRect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 };
    let sidebar_open = app.get_webview_window("sidebar").map(|w| w.is_visible().unwrap_or(false)).unwrap_or(false);
    Ok(toast_rect(work, monitor.scale_factor(), sidebar_open, height_logical))
}

/// The page measured its content: fit the window to it exactly (the first size is only an estimate by row count).
#[tauri::command]
pub async fn notify_toast_fit<R: Runtime>(app: AppHandle<R>, height: f64) -> Result<(), String> {
    if !height.is_finite() {
        return Err("bad height".into());
    }
    let rect = placement(&app, height)?;
    let win = app.get_webview_window(LABEL).ok_or("no toast window")?;
    win.set_size(tauri::PhysicalSize::new(rect.w as u32, rect.h as u32)).map_err(|e| e.to_string())?;
    win.set_position(tauri::PhysicalPosition::new(rect.x, rect.y)).map_err(|e| e.to_string())
}

/// Show `ev` as the toast. Returns Err when the window could not be made (the caller then falls back to native).
pub async fn show<R: Runtime>(app: &AppHandle<R>, ev: &NotifyEvent, mode: Privacy) -> Result<(), String> {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;

    // In a meeting or a call the toast shows only the source: a mail subject on a shared screen is not ours to show.
    let mode = if app.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>().is_some_and(|m| m.should_suppress_tts()) {
        Privacy::Hidden
    } else {
        mode
    };
    let data = payload(ev, mode, duration_ms(), seq);
    *pending() = Some(data.clone());
    let rows = data.get("rows").and_then(|r| r.as_array()).map_or(0, |r| r.len());
    let rect = placement(app, toast_height(rows))?;

    let warm = app.get_webview_window(LABEL).is_some();
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::notify_toast())?;
    let _ = win.set_size(tauri::PhysicalSize::new(rect.w as u32, rect.h as u32));
    let _ = win.set_position(tauri::PhysicalPosition::new(rect.x, rect.y));
    show_without_focus(&win, rect)?;
    if warm {
        // A running page takes the new alert straight away (the cold page reads `get_pending_toast` on mount).
        *pending() = None;
        let _ = app.emit_to(LABEL, "toast:show", data);
    }

    // Safety net: whatever the page does, the window does not outlive the cap.
    let app2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(HARD_CAP_MS)).await;
        if SEQ.load(Ordering::SeqCst) == seq {
            close(&app2);
        }
    });
    Ok(())
}

/// Show the window topmost WITHOUT activating it. The foreground window is read before and after so the log proves
/// whether focus moved (the on-device experiment of the plan, phase 0).
#[cfg(target_os = "windows")]
fn show_without_focus<R: Runtime>(win: &tauri::WebviewWindow<R>, rect: PRect) -> Result<(), String> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
        fn GetForegroundWindow() -> *mut std::ffi::c_void;
    }
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const SW_SHOWNOACTIVATE: i32 = 4;
    const HWND_TOPMOST: isize = -1;
    const SWP_NOACTIVATE: u32 = 0x0010;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
    // SAFETY: plain user32 calls on the toast's own window handle.
    unsafe {
        let before = GetForegroundWindow();
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(hwnd, HWND_TOPMOST, rect.x, rect.y, rect.w, rect.h, SWP_NOACTIVATE);
        let after = GetForegroundWindow();
        if before == after {
            tracing::info!("[TOAST] shown without moving focus");
        } else {
            tracing::warn!("[TOAST] the foreground window changed while showing the toast");
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn show_without_focus<R: Runtime>(win: &tauri::WebviewWindow<R>, _rect: PRect) -> Result<(), String> {
    win.show().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify_event::{Delivery as EvDelivery, PrivacyClass, ReadState, Severity, Target};
    use crate::notify_links::OpenHints;

    fn ev(provider: Provider, status: Status) -> NotifyEvent {
        NotifyEvent {
            schema_version: 1,
            event_id: "ev_aaaaaaaaaaaaaaaaaaaaaaaa".into(),
            entity_key: "k".into(),
            provider,
            account_id: "a".into(),
            kind: "build".into(),
            status,
            severity: Severity::Medium,
            occurred_at_ms: 1,
            observed_at_ms: 1,
            updated_at_ms: 1,
            title: "Build failed in zync".into(),
            preview: "alice/zync · main · run 5".into(),
            privacy: PrivacyClass::Public,
            target: Target::Repo { host: "github.com".into(), owner: "alice".into(), repo: "zync".into() },
            source_ref: "5".into(),
            original_source_ref: "5".into(),
            occurrences: 1,
            delivery: EvDelivery::default(),
            read: ReadState::Unread,
            expires_at_ms: 2,
            hints: OpenHints::default(),
        }
    }

    #[test]
    fn the_toast_is_a_compact_320_wide_card_top_right_in_the_work_area() {
        let work = PRect { x: 0, y: 0, w: 1920, h: 1040 }; // taskbar at the bottom
        let r = toast_rect(work, 1.0, false, toast_height(3));
        assert_eq!(r.w, 320);
        assert_eq!(r.h, 208);
        assert!(toast_height(0) < toast_height(4) && toast_height(99) <= TOAST_MAX_HEIGHT);
        assert_eq!(r.x, 1920 - r.w - 16);
        assert_eq!(r.y, 16);
    }

    #[test]
    fn it_slides_left_of_an_open_sidebar_and_stays_inside_the_work_area() {
        let work = PRect { x: 0, y: 0, w: 1920, h: 1040 };
        let open = toast_rect(work, 1.0, true, 180.0);
        let closed = toast_rect(work, 1.0, false, 180.0);
        let (_, _, sidebar_w, _) = crate::commands::sidebar_geometry("assistant", None, 1920.0, 1040.0);
        assert_eq!(open.x, closed.x - sidebar_w.round() as i32 - 16, "left of the sidebar's edge, not just of the toast");
        // a tiny display never pushes it off-screen
        let small = PRect { x: 100, y: 50, w: 400, h: 300 };
        let r = toast_rect(small, 1.0, true, 180.0);
        assert!(r.x >= small.x && r.y >= small.y && r.x + r.w <= small.x + small.w && r.y + r.h <= small.y + small.h, "{r:?}");
    }

    #[test]
    fn placement_follows_the_monitor_origin_and_scale() {
        // a monitor left of the primary (negative origin) at 150 %
        let work = PRect { x: -2560, y: 0, w: 2560, h: 1400 };
        let r = toast_rect(work, 1.5, false, 180.0);
        assert!(r.x >= -2560 && r.x + r.w <= 0, "{r:?}");
        assert_eq!(r.x, -2560 + 2560 - r.w - 24);
        assert_eq!(r.y, 24);
        assert_eq!(toast_rect(work, 0.0, false, 180.0), toast_rect(work, 1.0, false, 180.0), "bad scale falls back to 1");
    }

    #[test]
    fn our_own_fullscreen_stage_does_not_count_as_a_busy_shell() {
        assert_eq!(resolve_busy(2, false), 5, "busy only because of a full-screen overlay (NEXUS itself)");
        assert_eq!(resolve_busy(2, true), 2, "a real full-screen window in front still blocks");
        for s in [1, 3, 4, 5, 6, 7] {
            assert_eq!(resolve_busy(s, false), s);
        }
    }

    #[test]
    fn a_maximised_window_is_not_full_screen_but_a_bare_one_that_covers_the_monitor_is() {
        let mon = (0, 0, 1920, 1080);
        assert!(!is_fullscreen_rect((-8, -8, 1928, 1088), mon, true, true), "maximised: overhangs by its border");
        assert!(!is_fullscreen_rect((100, 100, 900, 700), mon, true, false));
        assert!(is_fullscreen_rect((0, 0, 1920, 1080), mon, false, false), "F11 video");
        assert!(!is_fullscreen_rect((0, 0, 1920, 1080), mon, true, false), "has a title bar");
    }

    #[test]
    fn the_channel_follows_presence_and_the_shell_state() {
        assert_eq!(decide(true, true, true, Some(5)), Delivery::Custom);
        assert_eq!(decide(true, true, true, None), Delivery::Custom);
        for blocked in [1, 2, 3, 4, 6] {
            assert_eq!(decide(true, true, true, Some(blocked)), Delivery::Native, "state {blocked}");
        }
        assert_eq!(decide(true, true, false, Some(5)), Delivery::Native, "nobody at the PC");
        assert_eq!(decide(true, false, false, Some(5)), Delivery::History);
        assert_eq!(decide(false, true, true, Some(5)), Delivery::Native, "custom switched off");
        assert_eq!(decide(false, false, true, Some(5)), Delivery::History);
        assert_eq!(decide(true, true, true, Some(7)), Delivery::Custom, "a Store app in front is fine");
    }

    #[test]
    fn delivery_reason_mirrors_decide_branches() {
        assert_eq!(delivery_reason(true, true, true, None), "ok");
        assert_eq!(delivery_reason(true, true, true, Some(5)), "ok");
        assert_eq!(delivery_reason(false, true, true, None), "native fallback");
        assert_eq!(delivery_reason(false, false, true, None), "toast switched off");
        assert_eq!(delivery_reason(true, true, false, None), "nobody at the PC");
        assert_eq!(delivery_reason(true, true, true, Some(1)), "nobody at the PC");
        assert_eq!(delivery_reason(true, true, true, Some(2)), "fullscreen or presentation");
        assert_eq!(delivery_reason(true, true, true, Some(3)), "fullscreen or presentation");
        assert_eq!(delivery_reason(true, true, true, Some(4)), "fullscreen or presentation");
        assert_eq!(delivery_reason(true, true, true, Some(6)), "focus session");
        assert_eq!(delivery_reason(true, false, false, None), "history only");
        assert_eq!(delivery_reason(true, false, true, Some(2)), "history only");
        assert_eq!(delivery_reason(true, false, true, Some(6)), "history only");
    }

    #[test]
    fn privacy_cuts_private_content_first() {
        let mail = ev(Provider::Gmail, Status::Info);
        assert_eq!(visible_text(Privacy::Private, &mail).1, "alice/zync · main · run 5", "sender and subject are what a mail event holds");
        assert_eq!(visible_text(Privacy::Private, &mail).0, "Build failed in zync");
        assert_eq!(visible_text(Privacy::Normal, &mail).1, "alice/zync · main · run 5");
        assert_eq!(visible_text(Privacy::Hidden, &mail), ("Mail alert".to_string(), String::new()));
        let ci = ev(Provider::GitHub, Status::Failed);
        assert_eq!(visible_text(Privacy::Private, &ci).1, "alice/zync · main · run 5", "repo and branch are not private");
        assert_eq!(Privacy::parse("Normal"), Privacy::Normal);
        assert_eq!(Privacy::parse("nonsense"), Privacy::Private);
    }

    #[test]
    fn each_source_gets_a_headline_and_neat_detail_rows() {
        use crate::notify_event::Target;
        let mut ci = ev(Provider::GitHub, Status::Failed);
        ci.target = Target::GitHub { host: "github.com".into(), owner: "alice".into(), repo: "zync".into(), pr: None, run_id: Some(5), sha: None };
        ci.hints.failed_step = Some(3);
        let (h, rows) = headline_and_rows(&ci, Privacy::Private);
        assert_eq!(h, "Build failed in zync");
        let labels: Vec<&str> = rows.iter().map(|(l, _)| l.as_str()).collect();
        assert_eq!(labels, ["Repo", "Branch", "Run", "Failed step"]);
        assert_eq!(rows[1].1, "main");
        assert_eq!(rows[3].1, "step 3");

        let mut mail = ev(Provider::Gmail, Status::Info);
        mail.target = Target::Gmail { thread_id: "t".into(), message_id: None };
        mail.title = "Asha R".into();
        mail.preview = "Interview on Monday".into();
        let (h, rows) = headline_and_rows(&mail, Privacy::Private);
        assert_eq!((h.as_str(), rows.len(), rows[0].1.as_str()), ("Interview on Monday", 1, "Asha R"), "subject is the headline, sender a row");
        mail.preview = String::new();
        assert_eq!(headline_and_rows(&mail, Privacy::Private).0, "Asha R", "a security message has no subject");

        let mut dep = ev(Provider::Vercel, Status::Failed);
        dep.target = Target::Deployment { provider: Provider::Vercel, team: "t".into(), project: "zync".into(), deployment_id: "dpl_1".into(), revision: None };
        dep.preview = "zync · dpl_1".into();
        let (_, rows) = headline_and_rows(&dep, Privacy::Private);
        assert_eq!(rows, vec![("Project".to_string(), "zync".to_string()), ("Deploy".to_string(), "dpl_1".to_string())]);

        let (h, rows) = headline_and_rows(&mail, Privacy::Hidden);
        assert_eq!((h.as_str(), rows.len()), ("Mail alert", 0), "hidden mode shows only the source");
    }

    #[test]
    fn the_payload_carries_the_id_and_display_text_but_never_a_url() {
        let mut e = ev(Provider::GitHub, Status::Failed);
        e.hints.html_url = Some("https://github.com/alice/zync/actions/runs/5/job/9".into());
        let p = payload(&e, Privacy::Private, 5000, 3);
        let s = p.to_string();
        assert!(!s.contains("http"), "no url may reach the page: {s}");
        assert_eq!(p["eventId"], "ev_aaaaaaaaaaaaaaaaaaaaaaaa");
        assert_eq!(p["provider"], "GitHub");
        assert_eq!(p["status"], "failed");
        assert_eq!(p["action"], "View log");
        assert_eq!(p["durationMs"], 5000);
        assert_eq!(payload(&ev(Provider::Gmail, Status::Info), Privacy::Normal, 5000, 1)["action"], "Open mail");
    }
}
