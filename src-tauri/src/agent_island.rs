//! The Agent Island window: a small always-on-top pill at the top of the screen that shows the logos of the agents
//! that are running; hovering it (or a permission request) expands it into the full island.
//!
//! The window is its own small OS window, shown without taking focus (notify_toast::show_without_focus) and resized
//! between the pill and the panel, so the rest of the screen is never covered or click-blocked. Placement and the
//! cursor-over-window test follow Louis-CFM/coucou's `island.rs` (MIT, see THIRD_PARTY_NOTICES.md); the state machine
//! is NEXUS's own.
//!
//! Rules (pure `Sm`, tested):
//!   * nothing running -> hidden;
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
/// Cursor margin around the window that still counts as "on the island", logical px.
const MARGIN: f64 = 10.0;
const HOVER_DWELL_MS: u64 = 120;
const LEAVE_DWELL_MS: u64 = 450;
/// A hidden island's window is destroyed after this long (a WebView2 costs RAM).
const HIDE_DESTROY_MS: u64 = 120_000;

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
    /// The cursor is over the island window.
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

/// Logical window size for a mode and a number of distinct agent logos.
pub fn size_for(mode: Mode, logos: usize) -> (f64, f64) {
    match mode {
        Mode::Expanded => (PANEL_W, PANEL_H),
        _ => ((PILL_PAD * 2.0 + LOGO_W * logos.clamp(1, 8) as f64).max(PILL_MIN_W), PILL_H),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModePayload {
    pub mode: Mode,
    pub by_approval: bool,
}

static CURRENT: Mutex<ModePayload> = Mutex::new(ModePayload { mode: Mode::Hidden, by_approval: false });

/// The page reads this on mount (a cold window misses the first event).
#[tauri::command]
pub fn agent_island_mode() -> ModePayload {
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
pub struct Island {
    sm: Sm,
    shown: Mode,
    logos: usize,
    hidden_since: Option<u64>,
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

    pub fn update(&mut self, app: &AppHandle, snap: &Snapshot, hover: bool) {
        let now = crate::agent_hooks::now_ms();
        let wanted = !snap.sessions.is_empty() || !snap.approvals.is_empty();
        let logos = distinct_agents(snap);
        let inside = self.shown != Mode::Hidden && cursor_inside(app);
        let mode = self.sm.step(&In { now_ms: now, wanted, approvals: snap.approvals.len(), inside, hover });
        let payload = ModePayload { mode, by_approval: self.sm.by_approval() };
        if mode == self.shown && (logos == self.logos || mode != Mode::Collapsed) {
            if mode == Mode::Hidden {
                let since = *self.hidden_since.get_or_insert(now);
                if now.saturating_sub(since) >= HIDE_DESTROY_MS && app.get_webview_window(LABEL).is_some() {
                    let _ = crate::dyn_windows::destroy_window(app, LABEL);
                }
            }
            return;
        }
        *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = payload;
        self.logos = logos;
        match mode {
            Mode::Hidden => {
                if let Some(w) = app.get_webview_window(LABEL) {
                    let _ = w.hide();
                }
                self.hidden_since = Some(now);
            }
            Mode::Collapsed | Mode::Expanded => {
                self.hidden_since = None;
                if let Err(e) = place_and_show(app, mode, logos, self.shown == Mode::Hidden) {
                    tracing::warn!("agent island: {e}");
                }
            }
        }
        self.shown = mode;
        let _ = app.emit_to(LABEL, "agent_island:mode", payload);
    }
}

fn place_and_show(app: &AppHandle, mode: Mode, logos: usize, was_hidden: bool) -> Result<(), String> {
    let monitor = app.primary_monitor().map_err(|e| e.to_string())?.ok_or("no monitor")?;
    let scale = monitor.scale_factor();
    let (lw, lh) = size_for(mode, logos);
    let (w, h) = ((lw * scale).round() as i32, (lh * scale).round() as i32);
    let mp = monitor.position();
    let ms = monitor.size();
    let x = mp.x + (ms.width as i32 - w) / 2;
    let y = mp.y + (TOP_GAP * scale).round() as i32;
    let win = crate::dyn_windows::get_or_create_window(app, crate::dyn_windows::WindowConfig::agent_island())?;
    let _ = win.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    if was_hidden || !win.is_visible().unwrap_or(false) {
        crate::notify_toast::show_without_focus(&win, crate::work_area::PRect { x, y, w, h })?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn cursor_inside(app: &AppHandle) -> bool {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    let Some(win) = app.get_webview_window(LABEL) else { return false };
    let (Ok(p), Ok(s)) = (win.outer_position(), win.outer_size()) else { return false };
    let scale = win.scale_factor().unwrap_or(1.0);
    let mut pt = POINT::default();
    // SAFETY: `pt` is a valid POINT; GetCursorPos only writes it.
    if !unsafe { GetCursorPos(&mut pt) }.as_bool() {
        return false;
    }
    let m = (MARGIN * scale) as i32;
    pt.x >= p.x - m && pt.x < p.x + s.width as i32 + m && pt.y >= p.y - m && pt.y < p.y + s.height as i32 + m
}

#[cfg(not(target_os = "windows"))]
fn cursor_inside(_app: &AppHandle) -> bool {
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
        assert_eq!(size_for(Mode::Expanded, 3), (PANEL_W, PANEL_H));
        let one = size_for(Mode::Collapsed, 1).0;
        let three = size_for(Mode::Collapsed, 3).0;
        assert!(three > one);
        assert_eq!(size_for(Mode::Collapsed, 0), size_for(Mode::Collapsed, 1));
        assert_eq!(size_for(Mode::Collapsed, 50), size_for(Mode::Collapsed, 8), "capped");
    }
}
