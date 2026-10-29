//! Work-area-aware placement (notification-to-sidebar N4). Pure integer/float maths, no Tauri.
//!
//! Tauri reports a monitor's physical position/size and its work area (the monitor minus taskbar/dock). A window must
//! be placed from the work area's ORIGIN (which is negative for a monitor left of or above the primary) and kept inside
//! it; building positions from a zero-based screen size puts the panel on the wrong monitor or under the taskbar.
//! Logical sizes come from `commands::sidebar_geometry` (resolved against the work area's logical size) and are
//! converted to physical pixels once, here.

/// Physical-pixel rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Physical rectangle for a sidebar `view`/`dock` inside `work`, at `scale`. Always within `work`, even when the
/// panel's preferred size is larger than a small display.
pub fn sidebar_rect(view: &str, dock: Option<&str>, work: PRect, scale: f64) -> PRect {
    let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let (lx, ly, lw, lh) = crate::commands::sidebar_geometry(view, dock, work.w as f64 / scale, work.h as f64 / scale);
    let w = ((lw * scale).round() as i32).clamp(1, work.w.max(1));
    let h = ((lh * scale).round() as i32).clamp(1, work.h.max(1));
    let x = (work.x + (lx * scale).round() as i32).clamp(work.x, work.x + (work.w - w).max(0));
    let y = (work.y + (ly * scale).round() as i32).clamp(work.y, work.y + (work.h - h).max(0));
    PRect { x, y, w, h }
}

/// A rectangle in logical pixels, relative to the monitor's top-left corner (the full-screen overlay's origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Logical size and margin of a Windows toast (bottom-right of the work area, above the taskbar).
pub const TOAST_W: f64 = 364.0;
pub const TOAST_H: f64 = 96.0;
pub const TOAST_MARGIN: f64 = 12.0;

/// (from, to) for the expand animation: `from` is where the toast was (bottom-right of the work area, above the
/// taskbar), `to` is the sidebar's final rectangle: the same `sidebar_rect(.., monitor, ..)` the window is placed
/// with. Both are converted to logical pixels relative to `monitor`'s origin, which is what the overlay draws in.
pub fn expand_rects(monitor: PRect, work: PRect, scale: f64, view: &str, dock: Option<&str>) -> (LRect, LRect) {
    let s = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    let tw = ((TOAST_W * s).round() as i32).clamp(1, work.w.max(1));
    let th = ((TOAST_H * s).round() as i32).clamp(1, work.h.max(1));
    let m = (TOAST_MARGIN * s).round() as i32;
    let from = PRect {
        x: (work.x + work.w - tw - m).clamp(work.x, work.x + (work.w - tw).max(0)),
        y: (work.y + work.h - th - m).clamp(work.y, work.y + (work.h - th).max(0)),
        w: tw,
        h: th,
    };
    let to = sidebar_rect(view, dock, monitor, s);
    let l = |r: PRect| LRect {
        x: (r.x - monitor.x) as f64 / s,
        y: (r.y - monitor.y) as f64 / s,
        w: r.w as f64 / s,
        h: r.h as f64 / s,
    };
    (l(from), l(to))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> PRect {
        PRect { x, y, w, h }
    }

    fn inside(inner: PRect, outer: PRect) -> bool {
        inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.w <= outer.x + outer.w && inner.y + inner.h <= outer.y + outer.h
    }

    #[test]
    fn a_right_docked_panel_sits_inside_the_work_area_of_the_primary_monitor() {
        // 1920x1080 with a 48px taskbar at the bottom.
        let work = r(0, 0, 1920, 1032);
        let p = sidebar_rect("assistant", Some("right"), work, 1.0);
        assert!(inside(p, work));
        assert_eq!((p.w, p.x + p.w), (520, 1920 - 10), "10px margin from the right edge of the work area");
        assert!(p.y + p.h <= 1032, "never under the taskbar");
    }

    #[test]
    fn a_monitor_left_of_the_primary_uses_its_negative_origin() {
        let work = r(-1920, 0, 1920, 1032);
        let p = sidebar_rect("assistant", Some("right"), work, 1.0);
        assert!(inside(p, work), "{p:?}");
        assert!(p.x < 0, "stays on the left monitor, not teleported to x>=0: {p:?}");
        // Above the primary: negative y.
        let above = r(0, -1080, 1920, 1032);
        let q = sidebar_rect("assistant", Some("right"), above, 1.0);
        assert!(inside(q, above) && q.y < 0, "{q:?}");
    }

    #[test]
    fn scale_is_applied_once_at_the_boundary() {
        // 150%: 2880x1620 physical = 1920x1080 logical.
        let work = r(0, 0, 2880, 1548);
        let p = sidebar_rect("assistant", Some("right"), work, 1.5);
        assert_eq!(p.w, (520.0_f64 * 1.5).round() as i32);
        assert!(inside(p, work));
        assert_eq!(p.x + p.w, 2880 - 15, "10 logical px margin = 15 physical px");
    }

    #[test]
    fn a_small_display_never_pushes_the_panel_off_screen() {
        let work = r(0, 0, 500, 400);
        for view in ["assistant", "settings", "coderabbit", "architect"] {
            for dock in [Some("left"), Some("right"), None] {
                let p = sidebar_rect(view, dock, work, 1.0);
                assert!(inside(p, work), "{view} {dock:?} {p:?}");
            }
        }
    }

    #[test]
    fn a_work_area_that_does_not_start_at_the_monitor_origin_is_respected() {
        // Taskbar on the left (60px): the work area starts at x=60.
        let work = r(60, 0, 1860, 1080);
        let p = sidebar_rect("assistant", Some("left"), work, 1.0);
        assert!(p.x >= 60, "left dock clears a left taskbar: {p:?}");
    }

    fn inside_l(r: LRect, mon: PRect, scale: f64) -> bool {
        r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= mon.w as f64 / scale + 0.5 && r.y + r.h <= mon.h as f64 / scale + 0.5
    }

    #[test]
    fn the_toast_origin_sits_bottom_right_above_the_taskbar_and_the_target_is_the_sidebar() {
        let mon = r(0, 0, 1920, 1080);
        let work = r(0, 0, 1920, 1032); // 48px taskbar at the bottom
        let (from, to) = expand_rects(mon, work, 1.0, "coderabbit", Some("right"));
        assert_eq!((from.w, from.h), (364.0, 96.0));
        assert_eq!(from.x + from.w, 1920.0 - 12.0, "12px from the right edge");
        assert_eq!(from.y + from.h, 1032.0 - 12.0, "above the taskbar");
        let p = sidebar_rect("coderabbit", Some("right"), mon, 1.0);
        assert_eq!((to.x, to.y, to.w, to.h), (p.x as f64, p.y as f64, p.w as f64, p.h as f64), "animates into the real sidebar rectangle");
        assert_eq!((p.y, p.h), (20, 1040), "full height: 20px margins top and bottom, as before");
        assert!(inside_l(from, mon, 1.0) && inside_l(to, mon, 1.0));
    }

    #[test]
    fn expand_rects_are_relative_to_the_monitor_origin_and_scale() {
        // Monitor to the right of the primary at 150%: its stage window origin is the monitor's top-left.
        let mon = r(2880, 0, 2880, 1620);
        let work = r(2880, 0, 2880, 1548);
        let (from, to) = expand_rects(mon, work, 1.5, "coderabbit", Some("right"));
        assert!(inside_l(from, mon, 1.5), "{from:?}");
        assert!(inside_l(to, mon, 1.5), "{to:?}");
        assert_eq!(from.w, 364.0, "logical size is kept at any scale");
    }

    #[test]
    fn the_animation_rectangles_never_leave_a_small_work_area() {
        let mon = r(0, 0, 500, 400);
        let (from, to) = expand_rects(mon, mon, 1.0, "coderabbit", Some("right"));
        assert!(inside_l(from, mon, 1.0) && inside_l(to, mon, 1.0), "{from:?} {to:?}");
    }

    #[test]
    fn nonsense_scale_falls_back_to_one() {
        let work = r(0, 0, 1000, 800);
        assert_eq!(sidebar_rect("assistant", Some("right"), work, 0.0), sidebar_rect("assistant", Some("right"), work, 1.0));
        assert_eq!(sidebar_rect("assistant", Some("right"), work, f64::NAN), sidebar_rect("assistant", Some("right"), work, 1.0));
    }
}
