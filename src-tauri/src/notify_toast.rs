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

/// Top-right rectangle (physical px) of the toast inside `work`: the assistant sidebar's width and half its height,
/// `MARGIN` from the edges, and left of the sidebar when that is open on the right. Always inside `work`.
pub fn toast_rect(work: PRect, scale: f64, sidebar_open: bool) -> PRect {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let (_, _, lw, lh) = crate::commands::sidebar_geometry("assistant", None, work.w as f64 / scale, work.h as f64 / scale);
    let w = ((lw * scale).round() as i32).clamp(1, work.w.max(1));
    let h = (((lh / 2.0) * scale).round() as i32).clamp(1, work.h.max(1));
    let margin = (MARGIN * scale).round() as i32;
    let shift = if sidebar_open { w + margin } else { 0 };
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

fn label(p: Provider) -> &'static str {
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

fn private_content(p: Provider) -> bool {
    matches!(p, Provider::Gmail | Provider::WhatsApp)
}

fn status_word(s: Status) -> &'static str {
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
        Privacy::Private if private_content(ev.provider) => (ev.title.clone(), String::new()),
        Privacy::Private => (ev.title.clone(), ev.preview.clone()),
        Privacy::Hidden => (format!("{} alert", label(ev.provider)), String::new()),
    }
}

/// Pure: the display payload sent to the toast page. Carries the opaque event id, never a URL.
pub fn payload(ev: &NotifyEvent, mode: Privacy, duration_ms: u64, seq: u64) -> Value {
    let (title, preview) = visible_text(mode, ev);
    json!({
        "seq": seq,
        "eventId": ev.event_id,
        "provider": label(ev.provider),
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
        (hr >= 0).then_some(s)
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

/// The user clicked the toast (or its button): open the real item, then close.
#[tauri::command]
pub async fn notify_toast_click<R: Runtime>(app: AppHandle<R>, event_id: String) -> Result<(), String> {
    close(&app);
    crate::notify_hub::open_external(&app, &event_id).await
}

fn close<R: Runtime>(app: &AppHandle<R>) {
    *pending() = None;
    let _ = crate::dyn_windows::destroy_window(app, LABEL);
}

/// Show `ev` as the toast. Returns Err when the window could not be made (the caller then falls back to native).
pub async fn show<R: Runtime>(app: &AppHandle<R>, ev: &NotifyEvent, mode: Privacy) -> Result<(), String> {
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    let data = payload(ev, mode, duration_ms(), seq);
    *pending() = Some(data.clone());

    let monitor = app.primary_monitor().map_err(|e| e.to_string())?.ok_or("no monitor")?;
    let wa = monitor.work_area();
    let work = PRect { x: wa.position.x, y: wa.position.y, w: wa.size.width as i32, h: wa.size.height as i32 };
    let sidebar_open = app.get_webview_window("sidebar").map(|w| w.is_visible().unwrap_or(false)).unwrap_or(false);
    let rect = toast_rect(work, monitor.scale_factor(), sidebar_open);

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
    fn the_toast_sits_top_right_in_the_work_area_at_sidebar_width_and_half_height() {
        let work = PRect { x: 0, y: 0, w: 1920, h: 1040 }; // taskbar at the bottom
        let r = toast_rect(work, 1.0, false);
        let (_, _, sw, sh) = crate::commands::sidebar_geometry("assistant", None, 1920.0, 1040.0);
        assert_eq!(r.w, sw.round() as i32);
        assert_eq!(r.h, (sh / 2.0).round() as i32);
        assert_eq!(r.x, 1920 - r.w - 16);
        assert_eq!(r.y, 16);
    }

    #[test]
    fn it_slides_left_of_an_open_sidebar_and_stays_inside_the_work_area() {
        let work = PRect { x: 0, y: 0, w: 1920, h: 1040 };
        let open = toast_rect(work, 1.0, true);
        let closed = toast_rect(work, 1.0, false);
        assert_eq!(open.x, closed.x - closed.w - 16);
        // a tiny display never pushes it off-screen
        let small = PRect { x: 100, y: 50, w: 400, h: 300 };
        let r = toast_rect(small, 1.0, true);
        assert!(r.x >= small.x && r.y >= small.y && r.x + r.w <= small.x + small.w && r.y + r.h <= small.y + small.h, "{r:?}");
    }

    #[test]
    fn placement_follows_the_monitor_origin_and_scale() {
        // a monitor left of the primary (negative origin) at 150 %
        let work = PRect { x: -2560, y: 0, w: 2560, h: 1400 };
        let r = toast_rect(work, 1.5, false);
        assert!(r.x >= -2560 && r.x + r.w <= 0, "{r:?}");
        assert_eq!(r.x, -2560 + 2560 - r.w - 24);
        assert_eq!(r.y, 24);
        assert_eq!(toast_rect(work, 0.0, false), toast_rect(work, 1.0, false), "bad scale falls back to 1");
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
    fn privacy_cuts_private_content_first() {
        let mail = ev(Provider::Gmail, Status::Info);
        assert_eq!(visible_text(Privacy::Private, &mail).1, "", "no snippet by default");
        assert_eq!(visible_text(Privacy::Private, &mail).0, "Build failed in zync");
        assert_eq!(visible_text(Privacy::Normal, &mail).1, "alice/zync · main · run 5");
        assert_eq!(visible_text(Privacy::Hidden, &mail), ("Mail alert".to_string(), String::new()));
        let ci = ev(Provider::GitHub, Status::Failed);
        assert_eq!(visible_text(Privacy::Private, &ci).1, "alice/zync · main · run 5", "repo and branch are not private");
        assert_eq!(Privacy::parse("Normal"), Privacy::Normal);
        assert_eq!(Privacy::parse("nonsense"), Privacy::Private);
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
