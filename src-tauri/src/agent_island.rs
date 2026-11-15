//! The Agent Island window: a small always-on-top pill at the top of the screen that shows the logos of the agents
//! that are running and that you are not already looking at; hovering it (or a permission request) expands it into
//! the full island.
//!
//! The OS window is created ONCE at the (scaled) panel size and never resized on hover; the page morphs one element
//! between the pill and the panel (CSS transition), so nothing jumps. Outside the drawn island the window ignores the
//! mouse (set_ignore_cursor_events), so the rest of the screen is never click-blocked: Rust polls the cursor and
//! turns clicks on only while it is over the island. Placement and the cursor-over test follow Louis-CFM/coucou's
//! `island.rs` (MIT, see THIRD_PARTY_NOTICES.md); the state machine is NEXUS's own.
//!
//! Rules (pure `Sm`, tested):
//!   * nothing to show -> hidden;
//!   * a permission request -> expanded at once, and after the answer back to the one-row pill;
//!   * hover on the pill for a moment -> expanded; leaving it for a moment -> pill.

use crate::agent_monitor::Snapshot;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

pub const LABEL: &str = "agent-island";
const LOGO_W: f64 = 32.0;
const PILL_H: f64 = 40.0;
const PILL_PAD: f64 = 14.0;
const PILL_MIN_W: f64 = 64.0;
const PANEL_W: f64 = 760.0;
const PANEL_H: f64 = 430.0;
const TOP_GAP: f64 = 6.0;
/// Cursor margin around the drawn island that still counts as "on the island", logical px. Generous on purpose: the
/// one-agent pill is only ~48 px wide at the smallest size, and the hover must be easy to hit.
const MARGIN: f64 = 24.0;
const HOVER_DWELL_MS: u64 = 180;
const LEAVE_DWELL_MS: u64 = 450;
/// A hidden island's window is destroyed after this long (a WebView2 costs RAM).
const HIDE_DESTROY_MS: u64 = 120_000;
pub const MIN_SCALE: f64 = 0.75;
pub const MAX_SCALE: f64 = 1.5;
/// The size the owner chose on the preview (the smallest the slider allows).
pub const DEFAULT_SCALE: f64 = 0.75;

/// The island size setting, kept inside its limits (a hand-edited or missing value falls back to the default size).
pub fn clamp_scale(s: f64) -> f64 {
    if s.is_finite() {
        s.clamp(MIN_SCALE, MAX_SCALE)
    } else {
        DEFAULT_SCALE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Hidden,
    Collapsed,
    Expanded,
}

pub struct In {
    pub now_ms: u64,
    /// Any session or request to show.
    pub wanted: bool,
    pub approvals: usize,
    /// The cursor is over the island.
    pub inside: bool,
    pub hover: bool,
}

#[derive(Debug, Default)]
pub struct Sm {
    mode: Mode,
    inside_since: Option<u64>,
    outside_since: Option<u64>,
    /// After an answer the cursor may still be over the card; hover must not re-open it until the cursor left once.
    need_leave: bool,
    by_approval: bool,
}

impl Sm {
    pub fn by_approval(&self) -> bool {
        self.by_approval
    }

    pub fn step(&mut self, i: &In) -> Mode {
        if !i.wanted {
            *self = Sm::default();
            return Mode::Hidden;
        }
        if i.approvals > 0 {
            self.mode = Mode::Expanded;
            self.by_approval = true;
            self.need_leave = false;
            self.inside_since = None;
            self.outside_since = None;
            return Mode::Expanded;
        }
        if self.by_approval {
            // the last request was answered (or timed out): back to the one-row pill
            self.by_approval = false;
            self.mode = Mode::Collapsed;
            self.need_leave = i.inside;
            self.inside_since = None;
            self.outside_since = None;
            return Mode::Collapsed;
        }
        match self.mode {
            Mode::Hidden => self.mode = Mode::Collapsed,
            Mode::Collapsed => {
                if !i.inside {
                    self.need_leave = false;
                    self.inside_since = None;
                } else if i.hover && !self.need_leave {
                    let since = *self.inside_since.get_or_insert(i.now_ms);
                    if i.now_ms.saturating_sub(since) >= HOVER_DWELL_MS {
                        self.mode = Mode::Expanded;
                        self.outside_since = None;
                    }
                }
            }
            Mode::Expanded => {
                if i.inside {
                    self.outside_since = None;
                } else {
                    let since = *self.outside_since.get_or_insert(i.now_ms);
                    if i.now_ms.saturating_sub(since) >= LEAVE_DWELL_MS {
                        self.mode = Mode::Collapsed;
                        self.inside_since = None;
                        self.need_leave = false;
                    }
                }
            }
        }
        self.mode
    }
}

/// Logical size of what is DRAWN for a mode, a number of distinct agent logos and the size setting.
pub fn size_for(mode: Mode, logos: usize, scale: f64) -> (f64, f64) {
    let s = clamp_scale(scale);
    match mode {
        Mode::Expanded => (PANEL_W * s, PANEL_H * s),
        _ => ((PILL_PAD * 2.0 + LOGO_W * logos.clamp(1, 8) as f64).max(PILL_MIN_W) * s, PILL_H * s),
    }
}

/// Logical size of the OS window: always the panel, so a hover never resizes it.
pub fn window_size(scale: f64) -> (f64, f64) {
    size_for(Mode::Expanded, 0, scale)
}

/// The drawn island's rectangle inside the window, in the same units as the inputs: horizontally centred, at the top.
pub fn drawn_rect(win_x: f64, win_y: f64, win_w: f64, mode: Mode, logos: usize, scale: f64, dpi: f64) -> (f64, f64, f64, f64) {
    let (lw, lh) = size_for(mode, logos, scale);
    let (w, h) = (lw * dpi, lh * dpi);
    (win_x + (win_w - w) / 2.0, win_y, w, h)
}

/// Is a point within `margin` of a rectangle (x, y, w, h)?
pub fn near(rect: (f64, f64, f64, f64), px: f64, py: f64, margin: f64) -> bool {
    let (x, y, w, h) = rect;
    px >= x - margin && px < x + w + margin && py >= y - margin && py < y + h + margin
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModePayload {
    pub mode: Mode,
    pub by_approval: bool,
    /// The size setting, so the page and the window never disagree.
    pub scale: f64,
    /// The cursor is in the hot zone of the one-row pill (the page lifts it a little so the hover is visible).
    pub hot: bool,
}

static CURRENT: Mutex<ModePayload> = Mutex::new(ModePayload { mode: Mode::Hidden, by_approval: false, scale: DEFAULT_SCALE, hot: false });

/// The page reads this on mount (a cold window misses the first event).
#[tauri::command]
pub fn agent_island_mode() -> ModePayload {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the island page draws: the running agents the user is not already looking at (permission requests are never
/// left out). The page reads this on mount; later changes arrive as `agent_island:snapshot`.
static SHOWN: Mutex<Option<Snapshot>> = Mutex::new(None);

pub fn set_shown(snap: &Snapshot) {
    *SHOWN.lock().unwrap_or_else(|e| e.into_inner()) = Some(snap.clone());
}

#[tauri::command]
pub fn agent_island_snapshot() -> Snapshot {
    SHOWN.lock().unwrap_or_else(|e| e.into_inner()).clone().unwrap_or_else(|| crate::agent_monitor::snapshot_now())
}

/// How far down the wake orb's capsule reaches from the top of the screen, logical px (0 = no orb at the top). The stage
/// reports it; the island sits below it and slides when it changes.
static OBSTACLE: Mutex<f64> = Mutex::new(0.0);

#[tauri::command]
pub fn agent_island_set_obstacle(bottom: f64) {
    *OBSTACLE.lock().unwrap_or_else(|e| e.into_inner()) = clamp_obstacle(bottom);
}

fn obstacle() -> f64 {
    *OBSTACLE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A sane obstacle: finite, not negative, at most a quarter of a tall screen.
pub fn clamp_obstacle(v: f64) -> f64 {
    if v.is_finite() {
        v.clamp(0.0, 300.0)
    } else {
        0.0
    }
}

/// One animation step of the slide: ease toward the target, snap when close.
pub fn slide_step(cur: f64, target: f64) -> f64 {
    let d = target - cur;
    if d.abs() < 0.75 {
        target
    } else {
        cur + d * 0.28
    }
}

static PREVIEW_UNTIL: Mutex<u64> = Mutex::new(0);
const PREVIEW_MS: u64 = 10_000;

/// "Show on my screen": the island with two demo agents for ten seconds, at the current size setting, so the size
/// can be judged without a running agent.
#[tauri::command]
pub fn agent_island_preview() {
    *PREVIEW_UNTIL.lock().unwrap_or_else(|e| e.into_inner()) = crate::agent_hooks::now_ms() + PREVIEW_MS;
    crate::agent_monitor::mark_dirty();
}

pub fn preview_active(now_ms: u64) -> bool {
    now_ms < *PREVIEW_UNTIL.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
pub struct Island {
    sm: Sm,
    shown: Mode,
    logos: usize,
    scale: f64,
    hidden_since: Option<u64>,
    ignoring: Option<bool>,
    hot: bool,
    was_inside: bool,
    /// The last snapshot summary, so the 33 ms hover tick can run without rebuilding the snapshot.
    last: Option<(bool, usize, usize)>,
    /// Current slide below the wake orb, logical px.
    y_off: f64,
}

fn distinct_agents(snap: &Snapshot) -> usize {
    let mut names: Vec<&str> = snap.sessions.iter().map(|s| s.agent.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    names.len()
}

impl Island {
    pub fn visible(&self) -> bool {
        self.shown != Mode::Hidden
    }

    /// A new snapshot (already without the agents the user is watching).
    pub fn update(&mut self, app: &AppHandle, snap: &Snapshot, hover: bool, scale: f64) {
        let wanted = !snap.sessions.is_empty() || !snap.approvals.is_empty();
        self.last = Some((wanted, snap.approvals.len(), distinct_agents(snap)));
        self.tick(app, hover, scale);
    }

    /// The cheap part that must run often: the cursor, the hover state machine, the window's click-through state.
    pub fn tick(&mut self, app: &AppHandle, hover: bool, scale: f64) {
        let Some((wanted, approvals, logos)) = self.last else { return };
        let scale = clamp_scale(scale);
        let now = crate::agent_hooks::now_ms();
        let inside = self.shown != Mode::Hidden && cursor_inside(app, self.shown, logos, scale);
        let mode = self.sm.step(&In { now_ms: now, wanted, approvals, inside, hover });
        let hot = inside && mode == Mode::Collapsed;
        if inside != self.was_inside {
            tracing::info!("agent island: cursor {} the island ({:?})", if inside { "entered" } else { "left" }, mode);
            self.was_inside = inside;
        }
        if mode != self.shown {
            tracing::info!("agent island: {:?} -> {:?} (cursor inside = {inside}, scale {scale})", self.shown, mode);
        }
        let payload = ModePayload { mode, by_approval: self.sm.by_approval(), scale, hot };
        let changed = mode != self.shown || (scale - self.scale).abs() > f64::EPSILON || logos != self.logos || hot != self.hot;
        if changed {
            self.hot = hot;
            *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = payload;
            match mode {
                Mode::Hidden => {
                    if let Some(w) = app.get_webview_window(LABEL) {
                        let _ = w.hide();
                    }
                    self.hidden_since = Some(now);
                    self.ignoring = None;
                }
                Mode::Collapsed | Mode::Expanded => {
                    self.hidden_since = None;
                    let place = self.shown == Mode::Hidden || (scale - self.scale).abs() > f64::EPSILON;
                    if place {
                        if let Err(e) = place_and_show(app, scale, self.shown == Mode::Hidden) {
                            tracing::warn!("agent island: {e}");
                        }
                    }
                }
            }
            self.shown = mode;
            self.logos = logos;
            self.scale = scale;
            let _ = app.emit_to(LABEL, "agent_island:mode", payload);
        } else if mode == Mode::Hidden {
            let since = *self.hidden_since.get_or_insert(now);
            if now.saturating_sub(since) >= HIDE_DESTROY_MS && app.get_webview_window(LABEL).is_some() {
                let _ = crate::dyn_windows::destroy_window(app, LABEL);
            }
        }
        // keep clear of the wake orb: slide down while its capsule is at the top, back up when it leaves
        if self.shown != Mode::Hidden {
            let target = obstacle();
            if (self.y_off - target).abs() > f64::EPSILON {
                self.y_off = slide_step(self.y_off, target);
                if let Some(w) = app.get_webview_window(LABEL) {
                    move_to_offset(app, &w, self.y_off);
                }
            }
        } else {
            self.y_off = obstacle();
        }
        // clicks pass through everywhere except over the drawn island
        if self.shown != Mode::Hidden {
            let want_ignore = !inside;
            if self.ignoring != Some(want_ignore) {
                if let Some(w) = app.get_webview_window(LABEL) {
                    if let Err(e) = set_click_through(&w, want_ignore) {
                        tracing::warn!("agent island: click-through: {e}");
                    }
                }
                self.ignoring = Some(want_ignore);
            }
        }
    }
}

/// Create (if needed), size, place and show the window: panel-sized, centred at the top of the primary monitor.
fn place_and_show(app: &AppHandle, scale: f64, was_hidden: bool) -> Result<(), String> {
    let monitor = app.primary_monitor().map_err(|e| e.to_string())?.ok_or("no monitor")?;
    let dpi = monitor.scale_factor();
    let (lw, lh) = window_size(scale);
    let (w, h) = ((lw * dpi).round() as i32, (lh * dpi).round() as i32);
    let mp = monitor.position();
    let ms = monitor.size();
    let x = mp.x + (ms.width as i32 - w) / 2;
    let y = mp.y + ((TOP_GAP + obstacle()) * dpi).round() as i32;
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::agent_island())?;
    let _ = win.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    if was_hidden || !win.is_visible().unwrap_or(false) {
        crate::notify_toast::show_without_focus(&win, crate::work_area::PRect { x, y, w, h })?;
    }
    Ok(())
}

/// Move the island window to `TOP_GAP + off` below the top of the primary monitor, keeping its x.
fn move_to_offset(app: &AppHandle, win: &tauri::WebviewWindow, off: f64) {
    let Ok(Some(monitor)) = app.primary_monitor() else { return };
    let dpi = monitor.scale_factor();
    let Ok(p) = win.outer_position() else { return };
    let y = monitor.position().y + ((TOP_GAP + off) * dpi).round() as i32;
    if p.y != y {
        let _ = win.set_position(tauri::PhysicalPosition::new(p.x, y));
    }
}

const WS_EX_TRANSPARENT: isize = 0x0000_0020;

/// The extended window style with the click-through bit on or off, every other bit untouched.
pub fn click_through_style(ex: isize, through: bool) -> isize {
    if through {
        ex | WS_EX_TRANSPARENT
    } else {
        ex & !WS_EX_TRANSPARENT
    }
}

/// Click-through on or off by flipping ONLY WS_EX_TRANSPARENT on the window. Do NOT use `set_ignore_cursor_events` here:
/// the island is shown with raw Win32 calls (`show_without_focus`, so it never takes focus), and tao, which does not
/// know that, rebuilds the whole style from its own flags on that call: it hid the window the moment the cursor
/// entered it (the pill vanished on hover) and dropped TOOLWINDOW / NOACTIVATE.
#[cfg(target_os = "windows")]
fn set_click_through(win: &tauri::WebviewWindow, through: bool) -> Result<(), String> {
    #[link(name = "user32")]
    extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    }
    const GWL_EXSTYLE: i32 = -20;
    let hwnd = win.hwnd().map_err(|e| e.to_string())?.0 as isize;
    // SAFETY: plain user32 calls on the island's own window handle; only one style bit changes.
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = click_through_style(ex, through);
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_click_through(win: &tauri::WebviewWindow, through: bool) -> Result<(), String> {
    win.set_ignore_cursor_events(through).map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn cursor_inside(app: &AppHandle, mode: Mode, logos: usize, scale: f64) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let Some(win) = app.get_webview_window(LABEL) else { return false };
    let (Ok(p), Ok(s)) = (win.outer_position(), win.outer_size()) else { return false };
    let dpi = win.scale_factor().unwrap_or(1.0);
    let mut pt = POINT::default();
    // SAFETY: `pt` is a valid POINT; GetCursorPos only writes it.
    if !unsafe { GetCursorPos(&mut pt) }.as_bool() {
        return false;
    }
    let rect = drawn_rect(p.x as f64, p.y as f64, s.width as f64, mode, logos, scale, dpi);
    near(rect, pt.x as f64, pt.y as f64, MARGIN * dpi)
}

#[cfg(not(target_os = "windows"))]
fn cursor_inside(_app: &AppHandle, _mode: Mode, _logos: usize, _scale: f64) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(now: u64, wanted: bool, approvals: usize, inside: bool) -> In {
        In { now_ms: now, wanted, approvals, inside, hover: true }
    }

    #[test]
    fn hidden_until_something_runs() {
        let mut sm = Sm::default();
        assert_eq!(sm.step(&input(0, false, 0, false)), Mode::Hidden);
        assert_eq!(sm.step(&input(10, true, 0, false)), Mode::Collapsed);
        assert_eq!(sm.step(&input(20, false, 0, false)), Mode::Hidden);
    }

    #[test]
    fn hover_expands_after_a_short_dwell_and_leaving_collapses() {
        let mut sm = Sm::default();
        sm.step(&input(0, true, 0, false));
        assert_eq!(sm.step(&input(100, true, 0, true)), Mode::Collapsed, "just arrived");
        assert_eq!(sm.step(&input(100 + HOVER_DWELL_MS, true, 0, true)), Mode::Expanded);
        assert_eq!(sm.step(&input(1000, true, 0, true)), Mode::Expanded, "stays while the cursor is on it");
        assert_eq!(sm.step(&input(1100, true, 0, false)), Mode::Expanded, "leave dwell");
        assert_eq!(sm.step(&input(1100 + LEAVE_DWELL_MS, true, 0, false)), Mode::Collapsed);
    }

    #[test]
    fn a_pass_through_does_not_open_it() {
        let mut sm = Sm::default();
        sm.step(&input(0, true, 0, false));
        sm.step(&input(100, true, 0, true));
        assert_eq!(sm.step(&input(150, true, 0, false)), Mode::Collapsed, "cursor left before the dwell");
        assert_eq!(sm.step(&input(160 + HOVER_DWELL_MS, true, 0, false)), Mode::Collapsed);
    }

    #[test]
    fn hover_off_never_expands_by_hover() {
        let mut sm = Sm::default();
        let mut i = input(0, true, 0, true);
        i.hover = false;
        sm.step(&i);
        i.now_ms = 5_000;
        assert_eq!(sm.step(&i), Mode::Collapsed);
    }

    #[test]
    fn a_permission_request_opens_it_and_the_answer_returns_to_one_row() {
        let mut sm = Sm::default();
        sm.step(&input(0, true, 0, false));
        assert_eq!(sm.step(&input(10, true, 1, false)), Mode::Expanded);
        assert!(sm.by_approval());
        assert_eq!(sm.step(&input(5_000, true, 1, false)), Mode::Expanded, "stays until answered");
        // answered while the cursor still sits on the card
        assert_eq!(sm.step(&input(6_000, true, 0, true)), Mode::Collapsed);
        assert!(!sm.by_approval());
        // the cursor is still over it: hover must not re-open it
        assert_eq!(sm.step(&input(6_000 + HOVER_DWELL_MS + 1, true, 0, true)), Mode::Collapsed);
        assert_eq!(sm.step(&input(9_000, true, 0, true)), Mode::Collapsed);
        // after the cursor leaves once, hover works again
        sm.step(&input(9_100, true, 0, false));
        sm.step(&input(9_200, true, 0, true));
        assert_eq!(sm.step(&input(9_200 + HOVER_DWELL_MS, true, 0, true)), Mode::Expanded);
    }

    #[test]
    fn the_last_agent_finishing_hides_even_an_open_island() {
        let mut sm = Sm::default();
        sm.step(&input(0, true, 0, false));
        sm.step(&input(10, true, 1, false));
        assert_eq!(sm.step(&input(20, false, 0, true)), Mode::Hidden);
    }

    #[test]
    fn sizes_grow_with_the_logos_and_the_panel_is_fixed() {
        assert_eq!(size_for(Mode::Expanded, 3, 1.0), (PANEL_W, PANEL_H));
        let one = size_for(Mode::Collapsed, 1, 1.0).0;
        let three = size_for(Mode::Collapsed, 3, 1.0).0;
        assert!(three > one);
        assert_eq!(size_for(Mode::Collapsed, 0, 1.0), size_for(Mode::Collapsed, 1, 1.0));
        assert_eq!(size_for(Mode::Collapsed, 50, 1.0), size_for(Mode::Collapsed, 8, 1.0), "capped");
    }

    #[test]
    fn the_size_setting_scales_everything_and_stays_in_range() {
        let (w1, h1) = size_for(Mode::Collapsed, 2, 1.0);
        let (w2, h2) = size_for(Mode::Collapsed, 2, 1.5);
        assert!((w2 / w1 - 1.5).abs() < 1e-9 && (h2 / h1 - 1.5).abs() < 1e-9);
        assert_eq!(size_for(Mode::Expanded, 2, 0.75), (PANEL_W * 0.75, PANEL_H * 0.75));
        assert_eq!(clamp_scale(9.0), MAX_SCALE);
        assert_eq!(clamp_scale(0.1), MIN_SCALE);
        assert_eq!(clamp_scale(f64::NAN), DEFAULT_SCALE);
        assert_eq!(clamp_scale(1.2), 1.2);
        assert_eq!(size_for(Mode::Collapsed, 1, 99.0), size_for(Mode::Collapsed, 1, MAX_SCALE), "never beyond the limit");
    }

    #[test]
    fn the_window_never_changes_size_between_pill_and_panel() {
        assert_eq!(window_size(1.0), size_for(Mode::Expanded, 0, 1.0));
        assert_eq!(window_size(1.0), window_size(1.0));
        assert!(window_size(1.25).0 > window_size(1.0).0);
    }

    #[test]
    fn the_drawn_pill_is_centred_at_the_top_of_the_window() {
        // a 760-wide window at x=580, y=6 (scale 1, dpi 1); a one-logo pill is 64 wide
        let (x, y, w, h) = drawn_rect(580.0, 6.0, 760.0, Mode::Collapsed, 1, 1.0, 1.0);
        assert_eq!((w, h), (64.0, 40.0));
        assert_eq!(x, 580.0 + (760.0 - 64.0) / 2.0);
        assert_eq!(y, 6.0);
        let (px, _, pw, ph) = drawn_rect(580.0, 6.0, 760.0, Mode::Expanded, 1, 1.0, 1.0);
        assert_eq!((px, pw, ph), (580.0, 760.0, 430.0), "the panel fills the window");
    }

    #[test]
    fn only_the_drawn_island_takes_the_mouse_not_the_empty_window() {
        let pill = drawn_rect(580.0, 6.0, 760.0, Mode::Collapsed, 1, 1.0, 1.0);
        let centre = (580.0 + 380.0, 6.0 + 20.0);
        assert!(near(pill, centre.0, centre.1, MARGIN));
        // far left of the pill but still inside the (transparent) window: must click through
        assert!(!near(pill, 600.0, 26.0, MARGIN));
        // far below the pill but inside the window
        assert!(!near(pill, centre.0, 300.0, MARGIN));
        // within the margin
        assert!(near(pill, pill.0 - MARGIN + 1.0, centre.1, MARGIN));
    }

    #[test]
    fn the_hot_zone_is_easy_to_hit_but_not_huge() {
        // the smallest one-agent pill (75%): 48 x 30 logical px at x 261..309, y 0..30 in a 570-wide window
        let (lw, lh) = size_for(Mode::Collapsed, 1, 0.75);
        let rect = drawn_rect(0.0, 0.0, 570.0, Mode::Collapsed, 1, 0.75, 1.0);
        assert_eq!((rect.2, rect.3), (lw, lh));
        let cx = rect.0 + rect.2 / 2.0;
        assert!(near(rect, cx, rect.1 + rect.3 + 20.0, MARGIN), "20 px below the pill still counts");
        assert!(near(rect, rect.0 - 20.0, rect.1 + 10.0, MARGIN), "20 px to the side still counts");
        assert!(!near(rect, cx, rect.1 + rect.3 + 60.0, MARGIN), "60 px below does not");
        assert!(!near(rect, rect.0 - 80.0, rect.1 + 10.0, MARGIN), "80 px to the side does not");
    }

    #[test]
    fn click_through_changes_only_its_own_bit() {
        let ex: isize = 0x080C_01B8; // layered, tool window, no-activate, topmost ... as the live island has it
        let through = click_through_style(ex, true);
        assert_eq!(through & !WS_EX_TRANSPARENT, ex & !WS_EX_TRANSPARENT, "no other bit moves");
        assert_ne!(through & WS_EX_TRANSPARENT, 0);
        assert_eq!(click_through_style(through, false), ex & !WS_EX_TRANSPARENT);
        assert_eq!(click_through_style(click_through_style(ex, true), true), through, "idempotent");
    }

    /// The island is shown with raw Win32 calls, so tao's `set_ignore_cursor_events` would hide it on hover (found by the
    /// desktop e2e test: the window turned invisible the moment the cursor entered it). Keep it out of this file.
    #[test]
    fn the_island_never_uses_tao_set_ignore_cursor_events() {
        let src = include_str!("agent_island.rs");
        let body = src.split("#[cfg(test)]").next().unwrap();
        let calls: Vec<&str> = body.lines().filter(|l| !l.trim_start().starts_with("//") && l.contains(".set_ignore_cursor_events(")).collect();
        // the only allowed call is the non-Windows fallback inside set_click_through
        assert_eq!(calls.len(), 1, "unexpected calls: {calls:?}");
        assert!(calls[0].contains("through"), "{calls:?}");
    }

    #[test]
    fn the_island_slides_down_below_the_orb_and_settles_there() {
        assert_eq!(clamp_obstacle(f64::NAN), 0.0);
        assert_eq!(clamp_obstacle(-5.0), 0.0);
        assert_eq!(clamp_obstacle(9_000.0), 300.0);
        assert_eq!(clamp_obstacle(110.0), 110.0);
        let mut y = 0.0;
        let mut steps = 0;
        while y != 110.0 {
            let next = slide_step(y, 110.0);
            assert!(next > y && next <= 110.0, "moves toward the target, never past it");
            y = next;
            steps += 1;
            assert!(steps < 60, "it settles");
        }
        let mut y = 110.0;
        while y != 0.0 {
            y = slide_step(y, 0.0);
            steps += 1;
            assert!(steps < 120);
        }
    }
}
