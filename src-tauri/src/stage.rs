//! Single fullscreen stage window + blackout watchdog.
//!
//! Step 1 (2026-09-25) proved the shell empty and parallel-run: windowing
//! (fullscreen transparent overlay, hitbox click-through, video-safe
//! toolwindow flag) and resilience (heartbeat, blackout auto-shutdown,
//! background auto-fix) first, before anything depended on it.
//!
//! Step 2 (this pass) moves the voice orb and loading indicator in as
//! positioned divs (see `window_manager.rs`'s orb_rect/emit_loading_rect
//! + the frontend's OrbFrame/LoadingIndicator components) — `stage` is
//! now shown at boot and stays up for the life of the app, instead of the
//! old on-demand/ghost-session-only model. A blackout now briefly hides
//! the orb + captions too, not just the ghost ring; the existing
//! auto-fix watchdog below is unchanged and still recovers within
//! seconds — this tradeoff was discussed and accepted.
//!
//! Blackout policy (user requirement: never a full blackout):
//! - DETECT: window missing OR renderer heartbeat stale >6s while marked
//!   visible (2s watchdog cadence; 8s grace after (re)show for cold boot).
//! - SHUTDOWN: destroy the dead window immediately — a black fullscreen
//!   surface is worse than no surface. Mark not-visible.
//! - INFORM: one orb-spoken message per incident (voice-first product;
//!   skipped when a real user turn is active so it never talks over one).
//! - AUTO-FIX: exponential backoff recreate (2s, 4s, 8s — budget caps
//!   attempts before the cap could matter), wait up to 10s for a fresh
//!   heartbeat, then healthy (fail counter reset). After 3 failed fixes:
//!   stop recreating, one final orb message, stay hidden until restart
//!   or manual `stage_show`.
//! - KILL-SWITCH: Ctrl+Alt+X destroys the stage and disables it for the
//!   session (watchdog + hitbox loop go quiet). `stage_show` re-arms.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const LABEL: &str = "stage";
const HEARTBEAT_STALE_SECS: u64 = 6;
const SHOW_GRACE_SECS: u64 = 8;
const MAX_AUTO_FIXES: u32 = 3;

/// Interactive rect in PHYSICAL pixels (frontend multiplies CSS px by
/// devicePixelRatio before sending). The hitbox loop opens mouse holes
/// exactly on these; everywhere else stays click-through.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StageRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl StageRect {
    fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
}

/// Hitboxes keyed by source so independent consumers (the orb, the
/// loading indicator, spatial annotations) can register/clear their own
/// interactive rects without clobbering each other. `stage_set_hitboxes`
/// (frontend-facing, legacy single-source callers) writes under the
/// "legacy" key; `set_hitbox_source` (Rust-internal, window_manager.rs /
/// calibration.rs) writes under a named key.
static HITBOXES: once_cell::sync::Lazy<parking_lot::Mutex<std::collections::HashMap<String, Vec<StageRect>>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// Rust-internal: replace one named source's hitbox rects. Empty = that
/// source has nothing interactive right now (does not affect other
/// sources). Used by window_manager.rs (orb) and calibration.rs (orb +
/// loading preview, both interactive for drag during a calibration
/// session).
pub fn set_hitbox_source(source: &str, rects: Vec<StageRect>) {
    let mut map = HITBOXES.lock();
    if rects.is_empty() {
        map.remove(source);
    } else {
        map.insert(source.to_string(), rects);
    }
}

static LAST_BEAT: once_cell::sync::Lazy<parking_lot::Mutex<Option<std::time::Instant>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

static SHOWN_AT: once_cell::sync::Lazy<parking_lot::Mutex<Option<std::time::Instant>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

static VISIBLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static DISABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FAILS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn visible() -> bool {
    VISIBLE.load(std::sync::atomic::Ordering::Relaxed)
}

fn disabled() -> bool {
    DISABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Ensure the stage window exists (created hidden). Does NOT show it.
/// The stage is a visual-only canvas: it must NEVER intercept mouse
/// clicks. `set_ignore_cursor_events(true)` is enforced here at creation
/// and re-asserted in `stage_show` so a fresh WebView incarnation can
/// never swallow the desktop (hitbox loop re-opens holes when needed).
fn ensure_stage<R: Runtime>(app: &AppHandle<R>) -> Result<tauri::WebviewWindow<R>, String> {
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::stage())?;
    win.set_ignore_cursor_events(true)
        .map_err(|e| format!("stage click-through: {e}"))?;
    Ok(win)
}

/// Synchronous core of `stage_show` — no actual `.await` happens in this
/// logic (window creation/show/cursor-events are all sync Tauri calls), so
/// it's split out as a plain fn callable from `lib.rs`'s setup hook, which
/// must create+show the stage BEFORE `mic_permissions::init` looks it up
/// by label — an `async_runtime::spawn`'d call wouldn't have run yet by
/// that point in the same synchronous setup closure.
pub fn stage_show_sync<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    DISABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
    let win = ensure_stage(app)?;
    win.show().map_err(|e| format!("stage show: {e}"))?;
    // Re-assert on every show: a recreated window resets to the default
    // (cursor-intercepting) state — the fullscreen overlay must be
    // strictly click-through before the hitbox loop re-opens holes.
    win.set_ignore_cursor_events(true)
        .map_err(|e| format!("stage click-through: {e}"))?;
    VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
    *SHOWN_AT.lock() = Some(std::time::Instant::now());
    tracing::info!("stage: shown (click-through enforced)");

    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = win.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                FindWindowW, SetWindowPos, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
            };
            use windows::core::PCWSTR;
            unsafe {
                let class_name: Vec<u16> = "Shell_TrayWnd\0".encode_utf16().collect();
                let taskbar = FindWindowW(PCWSTR(class_name.as_ptr()), PCWSTR(std::ptr::null()));
                if taskbar != HWND(0) {
                    let _ = SetWindowPos(
                        HWND(hwnd.0 as _),
                        taskbar,
                        0, 0, 0, 0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }
        }
    }

    Ok(())
}

/// IPC: show the stage. Clears kill-switch disable + fail counter — an
/// explicit show is a deliberate re-arm.
#[tauri::command]
pub async fn stage_show<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    stage_show_sync(&app)
}

/// IPC: hide the stage (destroy — frees the WebView2 tree like all windows).
#[tauri::command]
pub async fn stage_hide<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::ghost::stand_down(crate::ghost::ghost_wry::g_wry_ref(&app));
    crate::dyn_windows::destroy_window(&app, LABEL)?;
    tracing::info!("stage: hidden");
    Ok(())
}

/// IPC: renderer heartbeat — the stage frontend calls this every 2s.
/// A missing window or a stale beat while marked visible == blackout.
/// The optional `client` marker identifies the frontend build talking to
/// us (logged once per incarnation) — answers "is the current UI even in
/// this binary?" without devtools.
#[tauri::command]
pub fn stage_heartbeat(client: Option<String>) -> Result<(), String> {
    let mut beat = LAST_BEAT.lock();
    let first_or_stale = beat
        .map(|t| t.elapsed().as_secs() > HEARTBEAT_STALE_SECS)
        .unwrap_or(true);
    *beat = Some(std::time::Instant::now());
    drop(beat);
    if first_or_stale {
        tracing::info!(
            "stage: frontend alive (client={})",
            client.as_deref().unwrap_or("unknown")
        );
    }
    Ok(())
}

/// IPC: replace the interactive hitbox set (physical px) for ONE named
/// source (e.g. "spatial" for screen-annotation pins). Other sources'
/// hitboxes (orb, loading) are untouched — see `set_hitbox_source`.
#[tauri::command]
pub fn stage_set_hitboxes(source: String, rects: Vec<StageRect>) -> Result<(), String> {
    set_hitbox_source(&source, rects);
    Ok(())
}

/// IPC: kill-switch — destroy + disable for the session. Watchdog and
/// hitbox loop go quiet. Only `stage_show` re-arms.
#[tauri::command]
pub async fn stage_hide_kill<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    DISABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
    crate::ghost::stand_down(crate::ghost::ghost_wry::g_wry_ref(&app));
    crate::dyn_windows::destroy_window(&app, LABEL)?;
    tracing::warn!("stage: kill-switch engaged (disabled for session)");
    Ok(())
}

/// True while the stage window is up and not kill-switched.
pub fn is_shown() -> bool {
    visible() && !disabled()
}

/// Toggle `WDA_EXCLUDEFROMCAPTURE` on the stage window so our own screen
/// capture (screen tour / analysis) never contains the orb, spinner or old
/// annotations. Deliberately temporary: a permanent exclusion would also
/// hide NEXUS from the user's recordings and screen shares. Returns false
/// when the window/handle is unavailable. The GDI desktop-DC capture used
/// by the sidebar backdrop already relies on this affinity being honoured
/// for the (permanently excluded) sidebar window.
pub fn set_capture_excluded<R: Runtime>(app: &AppHandle<R>, excluded: bool) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowDisplayAffinity, WINDOW_DISPLAY_AFFINITY,
        };
        let Some(win) = app.get_webview_window(LABEL) else {
            return false;
        };
        let Ok(hwnd) = win.hwnd() else {
            return false;
        };
        // WDA_EXCLUDEFROMCAPTURE = 0x11, WDA_NONE = 0.
        let affinity = if excluded { 0x11 } else { 0 };
        unsafe {
            let _ = SetWindowDisplayAffinity(HWND(hwnd.0 as _), WINDOW_DISPLAY_AFFINITY(affinity));
        }
        true
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, excluded);
        false
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let mut pt = POINT { x: 0, y: 0 };
    // windows 0.36 returns BOOL here, not Result.
    let ok = unsafe { GetCursorPos(&mut pt).as_bool() };
    if ok {
        Some((pt.x, pt.y))
    } else {
        None
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn cursor_pos() -> Option<(i32, i32)> {
    None
}

/// Hitbox loop: ~30ms cursor poll toggling click-through. Whole stage
/// ignores the cursor unless it sits inside a registered hitbox (the
/// documented Tauri workaround for per-pixel-alpha hit-testing, which
/// neither Tauri nor Electron implements natively).
pub fn spawn_hitbox_loop<R: Runtime>(app: AppHandle<R>) {
    std::thread::Builder::new()
        .name("stage-hitbox".into())
        .spawn(move || {
            let mut last_ignore: Option<bool> = None;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(30));
                if !visible() || disabled() {
                    last_ignore = None;
                    continue;
                }
                let Some(win) = app.get_webview_window(LABEL) else {
                    continue;
                };
                let pos = cursor_pos();
                // Ghost ring ride-along shares this poll (one thread, two
                // consumers). No takeover judgment — mouse use is free.
                // No-op unless a ghost session is active.
                crate::ghost::observe_cursor(crate::ghost::ghost_wry::g_wry_ref(&app), pos);
                let inside = match pos {
                    Some((x, y)) => {
                        HITBOXES.lock().values().flatten().any(|r| r.contains(x, y))
                            || crate::live_glass::is_cursor_inside_hitbox(x, y)
                    }
                    // No cursor API on this platform: stay fully click-through.
                    None => false,
                };
                let want_ignore = !inside;
                if last_ignore != Some(want_ignore) && win.set_ignore_cursor_events(want_ignore).is_ok() {
                    last_ignore = Some(want_ignore);
                }
            }
        })
        .ok();
}

/// Speak one line through the orb (separate window — unaffected by a
/// stage blackout). Skipped when a real user turn is active: the
/// watchdog must never talk over the user.
///
/// Uses a dedicated `stage:notice` channel — deliberately NOT
/// `orchestrator:event` Result: a Result would overwrite the orb's
/// in-flight request id and clear the long-running flag, orphaning a
/// real turn's `done` handshake (the stuck-orb class) if a wake lands
/// mid-announcement. The notice handler speaks + resets locally with
/// zero handshake contact.
fn inform_via_orb<R: Runtime>(app: &AppHandle<R>, text: &str) {
    if crate::orchestrator::has_active_request() {
        tracing::info!("stage: watchdog silent (user turn active): {text}");
        return;
    }
    let payload = serde_json::json!({ "text": text });
    let _ = app.emit("stage:notice", payload);
}

/// Blackout watchdog: 2s cadence. Detect → shutdown → inform → auto-fix
/// (see module docs). Gives up recreating after MAX_AUTO_FIXES but keeps
/// watching so a manual `stage_show` still recovers.
pub fn spawn_stage_watchdog<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            if !visible() || disabled() {
                continue;
            }
            // Cold-boot grace: the renderer needs seconds to mount + beat.
            let fresh_show = SHOWN_AT
                .lock()
                .map(|t| t.elapsed().as_secs() < SHOW_GRACE_SECS)
                .unwrap_or(false);
            if fresh_show {
                continue;
            }
            let window_gone = app.get_webview_window(LABEL).is_none();
            let stale = LAST_BEAT
                .lock()
                .map(|t| t.elapsed().as_secs() > HEARTBEAT_STALE_SECS)
                .unwrap_or(true);
            if !window_gone && !stale {
                continue;
            }

            // ── DETECTED: blackout ──
            let fails = FAILS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            tracing::warn!(
                "stage: blackout detected (window_gone={window_gone}, stale_beat={stale}) — incident #{fails}"
            );

            // ── SHUTDOWN: a black fullscreen surface is worse than none ──
            VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
            crate::ghost::note_blackout();
            let _ = crate::dyn_windows::destroy_window(&app, LABEL);

            if fails > MAX_AUTO_FIXES {
                // ── GIVE UP: stay hidden, say so once, keep watching ──
                inform_via_orb(
                    &app,
                    "Display overlay kept failing, sir. I switched visuals off until restart — voice still works.",
                );
                tracing::error!("stage: auto-fix budget exhausted — staying hidden");
                continue;
            }

            // ── INFORM ──
            inform_via_orb(
                &app,
                "Display overlay hit a problem, sir. I hid it and I'm rebuilding it in the background.",
            );

            // ── AUTO-FIX: backoff, recreate, verify heartbeat ──
            let backoff = std::cmp::min(1u64 << fails.min(5), 30);
            tokio::time::sleep(std::time::Duration::from_secs(backoff)).await;
            if disabled() {
                // Kill-switch engaged mid-fix: stand down.
                continue;
            }
            match ensure_stage(&app) {
                Ok(win) => {
                    if win.show().is_err() {
                        continue;
                    }
                    VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
                    *SHOWN_AT.lock() = Some(std::time::Instant::now());
                    // Wait up to 10s for a fresh heartbeat (proves the
                    // renderer is alive, not just the window).
                    let mut healthy = false;
                    for _ in 0..20 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let fresh = LAST_BEAT
                            .lock()
                            .map(|t| t.elapsed().as_secs() <= HEARTBEAT_STALE_SECS)
                            .unwrap_or(false);
                        if fresh {
                            healthy = true;
                            break;
                        }
                    }
                    if healthy {
                        if disabled() {
                            // Kill-switch landed during verification: stay down.
                            VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
                            tracing::info!("stage: healthy beat but kill-switched — staying down");
                        } else {
                            FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
                            tracing::info!("stage: auto-fix verified healthy (heartbeat resumed)");
                        }
                    } else {
                        tracing::warn!("stage: recreated window never beat — will retry");
                    }
                }
                Err(e) => {
                    tracing::warn!("stage: auto-fix recreate failed: {e}");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hitbox_contains_edges() {
        let r = StageRect { x: 100, y: 100, w: 200, h: 100 };
        assert!(r.contains(100, 100));
        assert!(r.contains(299, 199));
        assert!(!r.contains(300, 100));
        assert!(!r.contains(99, 100));
        assert!(!r.contains(100, 200));
    }

    #[test]
    fn test_hitbox_empty_never_inside() {
        let boxes: Vec<StageRect> = vec![];
        assert!(!boxes.iter().any(|r| r.contains(960, 540)));
    }
}
