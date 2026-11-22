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
const HEARTBEAT_STALE_SECS: u64 = 10;
/// Cold-boot grace before a silent renderer counts as a blackout. The 2026-10-10 log shows a
/// cold WebView2 (profile cleared at every start) missing the old 20 s window.
const SHOW_GRACE_SECS: u64 = 45;
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

    /// Minimum distance in pixels from a point (px, py) to the rectangle boundary.
    /// Returns 0 if the point is inside.
    fn distance_to_point(&self, px: i32, py: i32) -> i32 {
        let dx = if px < self.x {
            self.x - px
        } else if px > self.x + self.w {
            px - (self.x + self.w)
        } else {
            0
        };
        let dy = if py < self.y {
            self.y - py
        } else if py > self.y + self.h {
            py - (self.y + self.h)
        } else {
            0
        };
        dx.max(dy)
    }
}

/// Hitboxes keyed by source so independent consumers (the orb, the
/// loading indicator, spatial annotations) can register/clear their own
/// interactive rects without clobbering each other. `stage_set_hitboxes`
/// (frontend-facing, legacy single-source callers) writes under the
/// "legacy" key; `set_hitbox_source` (Rust-internal, window_manager.rs /
/// calibration.rs) writes under a named key.
static HITBOXES: once_cell::sync::Lazy<parking_lot::Mutex<HitBook>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(HitBook::default()));

/// A source that registers rects and then never clears them (a hide that skipped its clean-up, a stuck state, a crashed
/// component) used to block that part of the user's screen until NEXUS restarted. Every source therefore has to
/// refresh its rects or they expire, and a rect that covers most of the screen is refused outright.
const DEFAULT_HIT_TTL_SECS: u64 = 60;
/// Sources whose rects legitimately stay up for a long time (a callout, annotation pins).
const LONG_HIT_TTL_SECS: u64 = 180;
/// A single interactive rect larger than this share of the screen is a bug, not a button.
const MAX_RECT_SCREEN_SHARE: f64 = 0.35;

fn hit_ttl(source: &str) -> std::time::Duration {
    match source {
        "tour" | "spatial" | "annotation" => std::time::Duration::from_secs(LONG_HIT_TTL_SECS),
        _ => std::time::Duration::from_secs(DEFAULT_HIT_TTL_SECS),
    }
}

struct HitEntry {
    rects: Vec<StageRect>,
    at: std::time::Instant,
    /// Calibration sessions hold their drag targets for as long as the session lasts: never expire, never size-capped.
    pinned: bool,
}

#[derive(Default)]
pub(crate) struct HitBook {
    entries: std::collections::HashMap<String, HitEntry>,
    /// Physical size of the stage (the primary monitor), once known.
    screen: Option<(i32, i32)>,
}

impl HitBook {
    pub(crate) fn set_screen(&mut self, w: i32, h: i32) {
        if w > 0 && h > 0 {
            self.screen = Some((w, h));
        }
    }

    /// Clamp a rect to the screen and refuse an empty, off-screen or oversized one.
    fn sanitise(&self, r: StageRect, pinned: bool) -> Option<StageRect> {
        if r.w <= 0 || r.h <= 0 {
            return None;
        }
        let Some((sw, sh)) = self.screen else { return Some(r) };
        let (x0, y0) = (r.x.max(0), r.y.max(0));
        let (x1, y1) = ((r.x + r.w).min(sw), (r.y + r.h).min(sh));
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let c = StageRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
        if !pinned && (c.w as f64) * (c.h as f64) > MAX_RECT_SCREEN_SHARE * (sw as f64) * (sh as f64) {
            return None;
        }
        Some(c)
    }

    /// Replace one source's rects (empty clears it). Returns how many rects were refused.
    pub(crate) fn set(&mut self, source: &str, rects: Vec<StageRect>, pinned: bool, now: std::time::Instant) -> usize {
        let total = rects.len();
        let kept: Vec<StageRect> = rects.into_iter().filter_map(|r| self.sanitise(r, pinned)).collect();
        let refused = total - kept.len();
        if kept.is_empty() {
            self.entries.remove(source);
        } else {
            self.entries.insert(source.to_string(), HitEntry { rects: kept, at: now, pinned });
        }
        refused
    }

    /// Drop every source that was not refreshed in time; returns the live rects and the names that expired.
    pub(crate) fn live(&mut self, now: std::time::Instant) -> (Vec<StageRect>, Vec<(String, u64)>) {
        let mut expired = Vec::new();
        self.entries.retain(|name, e| {
            let age = now.saturating_duration_since(e.at);
            if !e.pinned && age > hit_ttl(name) {
                expired.push((name.clone(), age.as_secs()));
                false
            } else {
                true
            }
        });
        (self.entries.values().flat_map(|e| e.rects.iter().copied()).collect(), expired)
    }

    /// One short line per source, sorted, so a change in the set can be logged once instead of every tick.
    pub(crate) fn signature(&self) -> String {
        let mut parts: Vec<String> = self
            .entries
            .iter()
            .map(|(n, e)| {
                let rs: Vec<String> = e.rects.iter().map(|r| format!("{}x{}@{},{}", r.w, r.h, r.x, r.y)).collect();
                format!("{n}[{}]", rs.join(" "))
            })
            .collect();
        parts.sort();
        parts.join(" ")
    }
}

/// Rust-internal: replace one named source's hitbox rects. Empty = that
/// source has nothing interactive right now (does not affect other
/// sources). The rects expire unless the source refreshes them (see `HitBook`).
pub fn set_hitbox_source(source: &str, rects: Vec<StageRect>) {
    set_hitbox_source_with(source, rects, false);
}

/// Same, for a calibration session: its drag targets stay for the whole session (no expiry, no size cap).
pub fn set_hitbox_source_pinned(source: &str, rects: Vec<StageRect>) {
    set_hitbox_source_with(source, rects, true);
}

fn set_hitbox_source_with(source: &str, rects: Vec<StageRect>, pinned: bool) {
    let refused = HITBOXES.lock().set(source, rects, pinned, std::time::Instant::now());
    if refused > 0 {
        tracing::warn!(
            "[HITBOX] '{source}': refused {refused} rect(s) (empty, off-screen, or covering over {}% of the screen)",
            (MAX_RECT_SCREEN_SHARE * 100.0) as u32
        );
    }
}

/// Windows extended-style bits that decide how the stage treats the mouse and the keyboard focus.
const WS_EX_TRANSPARENT_BIT: isize = 0x0000_0020;
const WS_EX_TOOLWINDOW_BIT: isize = 0x0000_0080;
const WS_EX_APPWINDOW_BIT: isize = 0x0004_0000;
const WS_EX_NOACTIVATE_BIT: isize = 0x0800_0000;

/// The stage's extended style: never takes keyboard focus (a click on one of its rects used to activate NEXUS, so the
/// next Ctrl+C or Ctrl+V went to the overlay instead of the user's app), never appears in Alt+Tab or the taskbar, and
/// is click-through unless the cursor is over a registered rect. Every other bit is left exactly as it is.
pub(crate) fn stage_ex_style(ex: isize, click_through: bool) -> isize {
    let v = (ex | WS_EX_TOOLWINDOW_BIT | WS_EX_NOACTIVATE_BIT) & !WS_EX_APPWINDOW_BIT;
    if click_through {
        v | WS_EX_TRANSPARENT_BIT
    } else {
        v & !WS_EX_TRANSPARENT_BIT
    }
}

/// Set the stage's click-through state by flipping the style bits directly. tao's `set_ignore_cursor_events` rebuilds the
/// whole style from tao's own flags and drops NOACTIVATE / TOOLWINDOW, which is how the overlay could take focus.
/// Reading the style first makes every call also repair any other code that changed the bits. Returns false if the
/// window handle is unavailable.
#[cfg(target_os = "windows")]
fn apply_click_through<R: Runtime>(win: &tauri::WebviewWindow<R>, through: bool) -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    }
    const GWL_EXSTYLE: i32 = -20;
    let Ok(hwnd) = win.hwnd() else { return false };
    let hwnd = hwnd.0 as isize;
    // SAFETY: plain user32 calls on the stage's own window handle; only the style bits named above change.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = stage_ex_style(ex, through);
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
    true
}

#[cfg(not(target_os = "windows"))]
fn apply_click_through<R: Runtime>(win: &tauri::WebviewWindow<R>, through: bool) -> bool {
    win.set_ignore_cursor_events(through).is_ok()
}

static LAST_BEAT: once_cell::sync::Lazy<parking_lot::Mutex<Option<std::time::Instant>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

static SHOWN_AT: once_cell::sync::Lazy<parking_lot::Mutex<Option<std::time::Instant>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

/// A final transcript emitted while no live stage could receive it. Events are fire-and-forget,
/// so a turn finished during a stage rebuild vanished and left the orb parked. The recreated
/// stage pulls this on mount (`stage_take_pending_transcript`), like `get_pending_*`.
static PENDING_TRANSCRIPT: once_cell::sync::Lazy<parking_lot::Mutex<Option<(std::time::Instant, serde_json::Value)>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));
/// How long a stashed transcript stays replayable.
const PENDING_TRANSCRIPT_TTL_SECS: u64 = 20;

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
    apply_click_through(&win, true);
    Ok(win)
}

/// Synchronous core of `stage_show` — no actual `.await` happens in this
/// logic (window creation/show/cursor-events are all sync Tauri calls), so
/// it's split out as a plain fn callable from `lib.rs`'s setup hook, which
/// must create+show the stage BEFORE `mic_permissions::init` looks it up
/// by label — an `async_runtime::spawn`'d call wouldn't have run yet by
/// that point in the same synchronous setup closure.
/// The physical rectangle the stage should cover: the primary monitor's own. `None` for a monitor that reports no size.
/// Pure, so the 100/125/150 % and 1366x768 / 2560x1440 cases are unit-tested.
pub fn stage_target_rect(monitor_pos: (i32, i32), monitor_size: (u32, u32)) -> Option<(i32, i32, u32, u32)> {
    if monitor_size.0 == 0 || monitor_size.1 == 0 {
        return None;
    }
    Some((monitor_pos.0, monitor_pos.1, monitor_size.0, monitor_size.1))
}

/// The stage window is created at a fixed 1920x1080 LOGICAL size (`WindowConfig::stage`), which only matches a 1080p screen at
/// 100 %. At 125 % or 150 %, or on another resolution, it was too big or too small, and the captions (placed as a fraction of
/// the stage height) could land off the visible screen. Fit it to the primary monitor on every show.
fn fit_to_primary_monitor<R: Runtime>(win: &tauri::WebviewWindow<R>) {
    let Ok(Some(monitor)) = win.primary_monitor() else {
        tracing::warn!("stage: no primary monitor reported; keeping the configured size");
        return;
    };
    let pos = monitor.position();
    let size = monitor.size();
    let Some((x, y, w, h)) = stage_target_rect((pos.x, pos.y), (size.width, size.height)) else {
        tracing::warn!("stage: primary monitor reported an empty size; keeping the configured size");
        return;
    };
    HITBOXES.lock().set_screen(w as i32, h as i32);
    let moved = win.set_position(tauri::PhysicalPosition::new(x, y));
    let sized = win.set_size(tauri::PhysicalSize::new(w, h));
    tracing::info!(
        "stage: fitted to the primary monitor {w}x{h} at ({x},{y}), scale {} (position ok={}, size ok={})",
        monitor.scale_factor(),
        moved.is_ok(),
        sized.is_ok()
    );
}

pub fn stage_show_sync<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    DISABLED.store(false, std::sync::atomic::Ordering::Relaxed);
    FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
    let win = ensure_stage(app)?;
    fit_to_primary_monitor(&win);
    win.show().map_err(|e| format!("stage show: {e}"))?;
    // Re-assert on every show: a recreated window resets to the default
    // (cursor-intercepting) state — the fullscreen overlay must be
    // strictly click-through before the hitbox loop re-opens holes.
    win.set_ignore_cursor_events(true)
        .map_err(|e| format!("stage click-through: {e}"))?;
    // tao just rebuilt the style from its own flags: put NOACTIVATE / TOOLWINDOW / TRANSPARENT back before anything can click it.
    apply_click_through(&win, true);
    VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
    *SHOWN_AT.lock() = Some(std::time::Instant::now());
    tracing::info!("stage: shown (click-through enforced)");

    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = win.hwnd() {
            use windows::Win32::Foundation::HWND;
            use windows::Win32::UI::WindowsAndMessaging::{
                SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE, SWP_NOACTIVATE,
            };
            unsafe {
                let _ = SetWindowPos(
                    HWND(hwnd.0 as _),
                    HWND_TOPMOST,
                    0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
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
    note_window_destroyed();
    tracing::info!("stage: hidden");
    Ok(())
}

/// IPC: renderer heartbeat — the stage frontend calls this every 2s.
/// A missing window or a stale beat while marked visible == blackout.
/// The optional `client` marker identifies the frontend build talking to
/// us (logged once per incarnation) — answers "is the current UI even in
/// this binary?" without devtools.
#[tauri::command]
pub fn stage_heartbeat<R: Runtime>(app: AppHandle<R>, client: Option<String>) -> Result<(), String> {
    stage_heartbeat_inner(client)?;
    // `ghost:session` fires only on enter/exit, so after a watchdog rebuild the
    // overlay never learned that a session was still live — no waves, no
    // hot-mic loop — while Rust stayed Active with Esc armed. Re-announce the
    // live session on every beat: the frontend listener is idempotent
    // (it only acts when `ghostActive` changes), and doing it per beat (2s)
    // also covers the first beat racing ahead of the listener's registration.
    if crate::ghost::session_active() {
        let _ = app.emit("ghost:session", serde_json::json!({ "active": true }));
    }
    Ok(())
}

/// Test-only helper: set the heartbeat timestamp without an AppHandle.
pub fn stage_heartbeat_inner(_client: Option<String>) -> Result<(), String> {
    let mut beat = LAST_BEAT.lock();
    let first_or_stale = beat
        .map(|t| t.elapsed().as_secs() > HEARTBEAT_STALE_SECS)
        .unwrap_or(true);
    *beat = Some(std::time::Instant::now());
    drop(beat);
    if first_or_stale {
        crate::boot::mark("stage_alive");
    }
    Ok(())
}

/// IPC: replace the interactive hitbox set (physical px) for ONE named
/// source (e.g. "spatial", "tour", "legacy"). Other sources'
/// hitboxes (orb, loading) are untouched — see `set_hitbox_source`.
#[tauri::command]
pub fn stage_set_hitboxes(source: Option<String>, rects: Vec<StageRect>) -> Result<(), String> {
    let src = source.unwrap_or_else(|| "legacy".to_string());
    set_hitbox_source(&src, rects);
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
    note_window_destroyed();
    tracing::warn!("stage: kill-switch engaged (disabled for session)");
    Ok(())
}

/// True while the stage window is up and not kill-switched.
pub fn is_shown() -> bool {
    visible() && !disabled()
}

/// True when the stage renderer has beaten recently, i.e. events emitted now will be received.
pub fn is_ready() -> bool {
    LAST_BEAT
        .lock()
        .map(|t| t.elapsed().as_secs() <= HEARTBEAT_STALE_SECS)
        .unwrap_or(false)
}

/// Wait (up to `max_ms`) for a live renderer. Used before emitting the wake so the orb events
/// are not lost to a stage that is being rebuilt. Returns whether it became ready.
pub async fn wait_until_ready(max_ms: u64) -> bool {
    let mut waited = 0u64;
    loop {
        if is_ready() {
            return true;
        }
        if waited >= max_ms {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        waited += 100;
    }
}

/// Forget the heartbeat when the window is destroyed (otherwise `is_ready` stays true for 10 s
/// after the renderer is gone).
fn note_window_destroyed() {
    *LAST_BEAT.lock() = None;
}

/// Keep a final transcript for the next stage mount when no live stage can receive the event.
pub fn stash_transcript_if_not_ready(payload: serde_json::Value) {
    if is_ready() {
        return;
    }
    tracing::warn!("stage: not ready - stashing the transcript for replay on the next mount");
    *PENDING_TRANSCRIPT.lock() = Some((std::time::Instant::now(), payload));
}

/// Pure TTL rule (unit-tested).
pub fn pending_is_fresh(age_secs: u64) -> bool {
    age_secs <= PENDING_TRANSCRIPT_TTL_SECS
}

/// IPC: the stage calls this once on mount; returns (and clears) a transcript that arrived while
/// it was down, if still fresh.
#[tauri::command]
pub fn stage_take_pending_transcript() -> Option<serde_json::Value> {
    let taken = PENDING_TRANSCRIPT.lock().take();
    match taken {
        Some((at, v)) if pending_is_fresh(at.elapsed().as_secs()) => Some(v),
        _ => None,
    }
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

/// Hitbox loop: adaptive cursor poll toggling click-through. Whole stage
/// ignores the cursor unless it sits inside a registered hitbox (the
/// documented Tauri workaround for per-pixel-alpha hit-testing, which
/// neither Tauri nor Electron implements natively).
///
/// Adaptive Pacing (Intel/AMD Package C-State & Battery Optimization):
/// - Empty hitboxes + no ghost: sleeps 250ms (eliminates 94% of wakeups).
/// - Active hitboxes, cursor far (>120px): sleeps 100ms (C8/C10 residency).
/// - Cursor in proximity (<=120px) or inside: sleeps 16ms (60Hz responsiveness).
pub fn spawn_hitbox_loop<R: Runtime>(app: AppHandle<R>) {
    std::thread::Builder::new()
        .name("stage-hitbox".into())
        .spawn(move || {
            let mut state = HitLoopState::default();
            loop {
                // A panic in one tick (a poisoned lock elsewhere, a bad handle) must never leave the full-screen overlay
                // stuck in its interactive state: that would block every click on the user's screen until restart.
                let sleep_ms = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hit_tick(&app, &mut state))) {
                    Ok(ms) => ms,
                    Err(_) => {
                        tracing::error!("[HITBOX] the hitbox loop panicked; forcing the stage click-through and carrying on");
                        if let Some(win) = app.get_webview_window(LABEL) {
                            apply_click_through(&win, true);
                        }
                        state = HitLoopState::default();
                        500
                    }
                };
                std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
            }
        })
        .ok();
}

#[derive(Default)]
struct HitLoopState {
    last_ignore: Option<bool>,
    last_signature: String,
    interactive_since: Option<std::time::Instant>,
    warned_stuck: bool,
}

/// How long the stage may stay interactive without a break before it is logged as a possible stuck hitbox.
const STUCK_INTERACTIVE_WARN_SECS: u64 = 20;

/// One pass of the loop: returns how long to sleep before the next one.
fn hit_tick<R: Runtime>(app: &AppHandle<R>, st: &mut HitLoopState) -> u64 {
    if !visible() || disabled() {
        st.last_ignore = None;
        st.interactive_since = None;
        return 500;
    }
    let Some(win) = app.get_webview_window(LABEL) else { return 500 };

    let ghost_active = crate::ghost::session_active();
    let (rects, expired, signature) = {
        let mut book = HITBOXES.lock();
        let (rects, expired) = book.live(std::time::Instant::now());
        (rects, expired, book.signature())
    };
    for (name, age) in expired {
        tracing::warn!("[HITBOX] '{name}' expired after {age}s without a refresh (a stale hitbox would have blocked that part of the screen)");
    }
    if signature != st.last_signature {
        tracing::info!("[HITBOX] interactive rects now: {}", if signature.is_empty() { "none".to_string() } else { signature.clone() });
        st.last_signature = signature.clone();
    }
    let has_glass = crate::live_glass::has_active_hitboxes();

    // If no hitboxes are registered and no ghost session is running,
    // the transparent stage window has ZERO interactive elements.
    // Force click-through and deep-sleep for 250ms to allow the CPU
    // package to enter deep low-power C-states (C8/C10). The style is re-checked
    // on every pass, so anything that changed it behind our back is repaired.
    if rects.is_empty() && !ghost_active && !has_glass {
        apply_click_through(&win, true);
        st.last_ignore = Some(true);
        st.interactive_since = None;
        st.warned_stuck = false;
        return 250;
    }

    let pos = cursor_pos();
    if ghost_active {
        crate::ghost::observe_cursor(crate::ghost::ghost_wry::g_wry_ref(app), pos);
    }

    let (inside, min_dist) = match pos {
        Some((x, y)) => {
            let mut in_rect = false;
            let mut min_d = i32::MAX;
            for r in &rects {
                if r.contains(x, y) {
                    in_rect = true;
                    min_d = 0;
                    break;
                }
                let d = r.distance_to_point(x, y);
                if d < min_d {
                    min_d = d;
                }
            }
            let inside = in_rect || crate::live_glass::is_cursor_inside_hitbox(x, y);
            (inside, if inside { 0 } else { min_d })
        }
        None => (false, i32::MAX),
    };

    let want_ignore = !inside;
    if apply_click_through(&win, want_ignore) {
        st.last_ignore = Some(want_ignore);
    }

    // Diagnostics: the cursor resting on an interactive rect for a long time is either a user reading a callout or a
    // stuck hitbox. Say so once, with the rects, so the next report names its cause.
    if inside {
        let since = *st.interactive_since.get_or_insert_with(std::time::Instant::now);
        if !st.warned_stuck && since.elapsed().as_secs() >= STUCK_INTERACTIVE_WARN_SECS {
            st.warned_stuck = true;
            tracing::warn!(
                "[HITBOX] the stage has been interactive for {STUCK_INTERACTIVE_WARN_SECS}s (cursor at {:?}); rects: {}",
                pos,
                st.last_signature
            );
        }
    } else {
        st.interactive_since = None;
        st.warned_stuck = false;
    }

    // Adaptive Sleep Interval:
    // - Proximity (inside or <= 120px): 16ms (60Hz seamless entry)
    // - Ghost active: 30ms (smooth cursor follower)
    // - Distant: 100ms (enables CPU deep sleep)
    if inside || min_dist <= 120 {
        16
    } else if ghost_active {
        30
    } else {
        100
    }
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

static RESUMED_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// The machine just woke from sleep: the watchdog gives the renderer a short grace instead of calling it a blackout.
pub fn note_resume() {
    if let Ok(mut g) = RESUMED_AT.lock() {
        *g = Some(std::time::Instant::now());
    }
}

fn resumed_recently() -> bool {
    RESUMED_AT.lock().ok().and_then(|g| *g).map_or(false, |t| t.elapsed().as_secs() < RESUME_GRACE_SECS)
}

/// After a wake from sleep the renderer can take far longer than 40 s to beat again (measured 2026-11-06 and
/// 2026-11-07: the recreated window needed more than the old grace and the budget ran out), so wait longer.
const RESUME_GRACE_SECS: u64 = 120;
/// The watchdog ticks every 2 s; a wall-clock gap this large between two ticks means the machine slept.
const TICK_RESUME_GAP_SECS: u64 = 15;
/// After the auto-fix budget is spent, try one quiet rebuild this often instead of staying hidden until restart.
const GIVE_UP_RETRY_SECS: u64 = 60;
/// How long a rebuilt window gets to send its first heartbeat.
const VERIFY_BEAT_SECS: u64 = 60;

static GAVE_UP_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// True when two watchdog ticks were far enough apart on the wall clock that the machine must have slept.
fn is_resume_gap(gap_secs: u64) -> bool {
    gap_secs > TICK_RESUME_GAP_SECS
}

/// True when the slow background retry after a spent budget is due.
fn give_up_retry_due(since_secs: u64) -> bool {
    since_secs >= GIVE_UP_RETRY_SECS
}

/// Recreate the stage, show it and wait for its first heartbeat. `None` = the window could not be built.
async fn recreate_and_verify<R: Runtime>(app: &AppHandle<R>) -> Option<bool> {
    let win = ensure_stage(app).map_err(|e| tracing::warn!("stage: auto-fix recreate failed: {e}")).ok()?;
    win.show().ok()?;
    VISIBLE.store(true, std::sync::atomic::Ordering::Relaxed);
    *SHOWN_AT.lock() = Some(std::time::Instant::now());
    for _ in 0..(VERIFY_BEAT_SECS * 2) {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if disabled() {
            return Some(false);
        }
        if LAST_BEAT.lock().map(|t| t.elapsed().as_secs() <= HEARTBEAT_STALE_SECS).unwrap_or(false) {
            return Some(true);
        }
    }
    Some(false)
}

/// Blackout watchdog: 2s cadence. Detect → shutdown → inform → auto-fix
/// (see module docs). Gives up recreating after MAX_AUTO_FIXES but keeps
/// watching so a manual `stage_show` still recovers.
pub fn spawn_stage_watchdog<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut last_tick = std::time::SystemTime::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            // The sleep-wake thread only looks every 10 s, so on a wake this tick often runs first and would call the
            // frozen renderer a blackout (2026-11-08: blackout logged 2 ms before "resumed from sleep"). Notice the
            // jump here, on the same clock the watchdog uses.
            let now_wall = std::time::SystemTime::now();
            let tick_gap = now_wall.duration_since(last_tick).unwrap_or_default().as_secs();
            last_tick = now_wall;
            if is_resume_gap(tick_gap) {
                note_resume();
            }
            if disabled() {
                continue;
            }
            if !visible() {
                // The budget is spent: try one quiet rebuild every GIVE_UP_RETRY_SECS rather than staying dark until restart.
                let due = GAVE_UP_AT
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .map(|t| give_up_retry_due(t.elapsed().as_secs()))
                    .unwrap_or(false);
                if due && !resumed_recently() {
                    if let Ok(mut g) = GAVE_UP_AT.lock() {
                        *g = Some(std::time::Instant::now());
                    }
                    if recreate_and_verify(&app).await == Some(true) && !disabled() {
                        FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
                        if let Ok(mut g) = GAVE_UP_AT.lock() {
                            *g = None;
                        }
                        tracing::info!("stage: background retry restored the overlay");
                    } else {
                        VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
                        let _ = crate::dyn_windows::destroy_window(&app, LABEL);
                        note_window_destroyed();
                        tracing::warn!("stage: background retry did not bring the overlay back — will try again");
                    }
                }
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
            // Just woke from sleep: the renderer was frozen, so its heartbeat is stale for a few seconds. That is not a blackout
            // (measured 2026-11-03: a needless destroy + rebuild at every wake). Give it time to beat again.
            if !window_gone && resumed_recently() {
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
            note_window_destroyed();

            if fails > MAX_AUTO_FIXES {
                // ── GIVE UP: stay hidden, say so once, keep watching ──
                // A Ghost session cannot outlive its overlay: with no stage
                // the session UI is gone, yet its global Esc shortcut would
                // keep swallowing Esc system-wide. End it, and say so.
                let was_ghost = crate::ghost::session_active();
                crate::ghost::stand_down(crate::ghost::ghost_wry::g_wry_ref(&app));
                inform_via_orb(
                    &app,
                    if was_ghost {
                        "Display overlay kept failing, sir. I switched visuals off until restart and ended Ghost mode — voice still works."
                    } else {
                        "Display overlay kept failing, sir. I switched visuals off until restart — voice still works."
                    },
                );
                tracing::error!("stage: auto-fix budget exhausted — hidden, will retry quietly every {GIVE_UP_RETRY_SECS} s");
                if let Ok(mut g) = GAVE_UP_AT.lock() {
                    *g = Some(std::time::Instant::now());
                }
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
            // Waits up to VERIFY_BEAT_SECS for a fresh heartbeat (proves the renderer is alive, not just the window).
            match recreate_and_verify(&app).await {
                Some(true) if disabled() => {
                    // Kill-switch landed during verification: stay down.
                    VISIBLE.store(false, std::sync::atomic::Ordering::Relaxed);
                    tracing::info!("stage: healthy beat but kill-switched — staying down");
                }
                Some(true) => {
                    FAILS.store(0, std::sync::atomic::Ordering::Relaxed);
                    tracing::info!("stage: auto-fix verified healthy (heartbeat resumed)");
                }
                Some(false) => tracing::warn!("stage: recreated window never beat — will retry"),
                None => {}
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
    fn a_wall_clock_jump_between_ticks_counts_as_a_resume() {
        assert!(!is_resume_gap(2), "a normal 2 s tick");
        assert!(!is_resume_gap(TICK_RESUME_GAP_SECS), "right at the limit is still normal");
        assert!(is_resume_gap(TICK_RESUME_GAP_SECS + 1));
        assert!(is_resume_gap(1533), "the 25 minute sleep from the 2026-11-08 log");
    }

    #[test]
    fn the_quiet_retry_after_a_spent_budget_waits_a_minute() {
        assert!(!give_up_retry_due(0));
        assert!(!give_up_retry_due(GIVE_UP_RETRY_SECS - 1));
        assert!(give_up_retry_due(GIVE_UP_RETRY_SECS));
    }

    #[test]
    fn resume_grace_covers_a_slow_renderer() {
        // The failed recoveries needed more than 40 s; the grace must outlast them and the first verify window.
        assert!(RESUME_GRACE_SECS >= 120);
        assert!(VERIFY_BEAT_SECS >= 60);
    }

    #[test]
    fn pending_transcript_expires() {
        assert!(pending_is_fresh(0));
        assert!(pending_is_fresh(20));
        assert!(!pending_is_fresh(21));
    }

    // One test: these three share the process-global heartbeat/stash, so they must not race.
    #[test]
    fn readiness_and_stash_follow_the_heartbeat() {
        note_window_destroyed();
        assert!(!is_ready(), "no beat => not ready");
        stage_heartbeat_inner(Some("test".into())).unwrap();
        assert!(is_ready());
        note_window_destroyed();
        assert!(!is_ready(), "a destroyed window must stop counting as ready at once");

        let _ = stage_take_pending_transcript();
        stash_transcript_if_not_ready(serde_json::json!({"text": "open notepad"}));
        let got = stage_take_pending_transcript().expect("stashed while down");
        assert_eq!(got["text"], "open notepad");
        assert!(stage_take_pending_transcript().is_none(), "take clears it");

        stage_heartbeat_inner(None).unwrap();
        stash_transcript_if_not_ready(serde_json::json!({"text": "ignored"}));
        assert!(stage_take_pending_transcript().is_none(), "no stash while the stage is alive");
        note_window_destroyed();
    }

    #[test]
    fn test_hitbox_empty_never_inside() {
        let boxes: Vec<StageRect> = vec![];
        assert!(!boxes.iter().any(|r| r.contains(960, 540)));
    }
}

#[cfg(test)]
mod stage_fit_tests {
    use super::stage_target_rect;

    #[test]
    fn the_stage_covers_the_primary_monitor_at_every_common_size() {
        // Physical pixels as the monitor reports them: scaling does not change the physical rectangle.
        for size in [(1366, 768), (1920, 1080), (2560, 1440), (3840, 2160)] {
            assert_eq!(stage_target_rect((0, 0), size), Some((0, 0, size.0, size.1)));
        }
    }

    #[test]
    fn a_primary_monitor_that_is_not_at_the_origin_keeps_its_position() {
        assert_eq!(stage_target_rect((-1920, 0), (1920, 1080)), Some((-1920, 0, 1920, 1080)));
    }

    #[test]
    fn a_monitor_with_no_size_is_ignored() {
        assert_eq!(stage_target_rect((0, 0), (0, 1080)), None);
        assert_eq!(stage_target_rect((0, 0), (1920, 0)), None);
    }
}


#[cfg(test)]
mod hitbook_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn r(x: i32, y: i32, w: i32, h: i32) -> StageRect {
        StageRect { x, y, w, h }
    }

    fn book() -> HitBook {
        let mut b = HitBook::default();
        b.set_screen(2880, 1800);
        b
    }

    #[test]
    fn a_source_that_is_never_refreshed_expires() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("orb", vec![r(1240, 0, 400, 400)], false, t0);
        let (live, expired) = b.live(t0 + Duration::from_secs(DEFAULT_HIT_TTL_SECS - 1));
        assert_eq!(live.len(), 1);
        assert!(expired.is_empty());
        let (live, expired) = b.live(t0 + Duration::from_secs(DEFAULT_HIT_TTL_SECS + 1));
        assert!(live.is_empty(), "a stale hitbox must stop blocking the screen");
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].0, "orb");
    }

    #[test]
    fn refreshing_restarts_the_clock() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("orb", vec![r(10, 10, 50, 50)], false, t0);
        b.set("orb", vec![r(10, 10, 50, 50)], false, t0 + Duration::from_secs(50));
        let (live, _) = b.live(t0 + Duration::from_secs(100));
        assert_eq!(live.len(), 1);
    }

    #[test]
    fn long_lived_sources_get_the_long_window() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("tour", vec![r(100, 100, 300, 120)], false, t0);
        b.set("other", vec![r(500, 100, 300, 120)], false, t0);
        let (live, expired) = b.live(t0 + Duration::from_secs(DEFAULT_HIT_TTL_SECS + 5));
        assert_eq!(live.len(), 1, "the tour callout is still up, the unknown source is gone");
        assert_eq!(expired.len(), 1);
        let (live, _) = b.live(t0 + Duration::from_secs(LONG_HIT_TTL_SECS + 5));
        assert!(live.is_empty());
    }

    #[test]
    fn calibration_sources_are_pinned_for_the_whole_session() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("orb", vec![r(1240, 0, 400, 400)], true, t0);
        let (live, expired) = b.live(t0 + Duration::from_secs(3600));
        assert_eq!(live.len(), 1);
        assert!(expired.is_empty());
    }

    #[test]
    fn a_rect_covering_most_of_the_screen_is_refused() {
        let mut b = book();
        let refused = b.set("bug", vec![r(0, 0, 2880, 1800)], false, Instant::now());
        assert_eq!(refused, 1);
        assert!(b.live(Instant::now()).0.is_empty());
        // a 200x200 logical orb (400 px at 200%) is fine
        assert_eq!(b.set("ok", vec![r(1240, 0, 400, 400)], false, Instant::now()), 0);
        // just over the cap is refused, just under is kept
        let w = (2880.0 * 0.6) as i32;
        assert_eq!(b.set("big", vec![r(0, 0, w, 1800)], false, Instant::now()), 1);
    }

    #[test]
    fn empty_and_off_screen_rects_are_refused_and_edges_are_clamped() {
        let mut b = book();
        assert_eq!(b.set("a", vec![r(0, 0, 0, 40)], false, Instant::now()), 1);
        assert_eq!(b.set("b", vec![r(5000, 5000, 100, 100)], false, Instant::now()), 1);
        assert_eq!(b.set("c", vec![r(-50, -50, 150, 150)], false, Instant::now()), 0);
        let (live, _) = b.live(Instant::now());
        assert_eq!(live.len(), 1);
        assert_eq!((live[0].x, live[0].y, live[0].w, live[0].h), (0, 0, 100, 100));
    }

    #[test]
    fn replacing_a_source_with_only_bad_rects_clears_it() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("tour", vec![r(100, 100, 300, 100)], false, t0);
        b.set("tour", vec![r(0, 0, 2880, 1800)], false, t0);
        assert!(b.live(t0).0.is_empty(), "the old rect must not linger after a refused update");
        b.set("tour", vec![r(100, 100, 300, 100)], false, t0);
        b.set("tour", Vec::new(), false, t0);
        assert!(b.live(t0).0.is_empty());
    }

    #[test]
    fn sources_do_not_clobber_each_other() {
        let mut b = book();
        let t0 = Instant::now();
        b.set("tour", vec![r(100, 100, 300, 100)], false, t0);
        b.set("spatial", vec![r(900, 100, 50, 50), r(1000, 100, 50, 50)], false, t0);
        b.set("tour", Vec::new(), false, t0);
        assert_eq!(b.live(t0).0.len(), 2);
        assert!(b.signature().starts_with("spatial["));
    }

    #[test]
    fn without_a_known_screen_only_empty_rects_are_refused() {
        let mut b = HitBook::default();
        assert_eq!(b.set("x", vec![r(0, 0, 0, 0), r(1, 1, 5000, 5000)], false, Instant::now()), 1);
        assert_eq!(b.live(Instant::now()).0.len(), 1);
    }

    #[test]
    fn the_stage_style_never_takes_focus_and_is_click_through_on_demand() {
        let base: isize = 0x000C_0138; // the style measured on the running stage: APPWINDOW, TRANSPARENT, LAYERED, TOPMOST ...
        let through = stage_ex_style(base, true);
        assert_ne!(through & WS_EX_NOACTIVATE_BIT, 0, "a click must never activate the overlay (it stole Ctrl+C)");
        assert_ne!(through & WS_EX_TOOLWINDOW_BIT, 0, "no Alt+Tab entry for an invisible overlay");
        assert_eq!(through & WS_EX_APPWINDOW_BIT, 0);
        assert_ne!(through & WS_EX_TRANSPARENT_BIT, 0);
        let interactive = stage_ex_style(base, false);
        assert_eq!(interactive & WS_EX_TRANSPARENT_BIT, 0);
        assert_ne!(interactive & WS_EX_NOACTIVATE_BIT, 0, "still non-activating while the mouse is over a rect");
        // layered + topmost bits are never touched
        assert_eq!(through & 0x0008_0008, base & 0x0008_0008);
        // idempotent
        assert_eq!(stage_ex_style(through, true), through);
    }

    fn production_source(file: &str) -> String {
        let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src").join(file)).unwrap();
        text.split("#[cfg(test)]").next().unwrap().to_string()
    }

    #[test]
    fn the_hit_loop_never_uses_taos_set_ignore_cursor_events() {
        // tao rebuilds the whole style on that call and drops NOACTIVATE; the loop must flip the bits itself.
        let src = production_source("stage.rs");
        let start = src.find("fn hit_tick").expect("hit_tick");
        let end = src[start..].find("/// Speak one line through the orb").map(|i| start + i).expect("end marker");
        let body: Vec<&str> = src[start..end].lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        assert!(
            !body.iter().any(|l| l.contains(".set_ignore_cursor_events(")),
            "hit_tick must call apply_click_through, not tao's set_ignore_cursor_events"
        );
        assert!(body.iter().any(|l| l.contains("apply_click_through(")));
    }

    #[test]
    fn waking_the_orb_never_registers_an_interactive_hitbox() {
        // The 200x200 logical orb rect at the top centre swallowed Brave's tabs and address bar after every wake.
        let src = production_source("window_manager.rs");
        let start = src.find("pub fn show_orb_interactive").expect("show_orb_interactive");
        let end = src[start..].find("\n}\n").map(|i| start + i).expect("end of fn");
        let body: Vec<&str> = src[start..end].lines().filter(|l| !l.trim_start().starts_with("//")).collect();
        assert!(!body.iter().any(|l| l.contains("set_orb_hitbox_interactive") || l.contains("set_hitbox_source")));
    }

    #[test]
    fn the_orb_is_only_interactive_during_calibration() {
        assert!(crate::window_manager::orb_hitbox_allowed(true));
        assert!(!crate::window_manager::orb_hitbox_allowed(false));
    }
}