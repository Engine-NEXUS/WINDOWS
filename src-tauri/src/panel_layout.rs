//! Where the NEXUS panels sit when more than one is open (the response sidebar, the Architecture map, the Maps view).
//!
//! The response sidebar keeps the place its dock gives it. Every other panel is arranged in the free span of the work
//! area next to it, in the order they were opened (oldest furthest from the sidebar), never overlapping while they fit.
//! When they do not fit they shrink towards their minimum width, and only then overlap evenly. The user can resize a
//! panel (Shift + Left/Right in the panel); the width is remembered per kind. Pure functions here, glue at the bottom.

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};

use crate::work_area::PRect;

/// Space between a panel and the screen edge / the next panel (logical px).
pub const MARGIN: f64 = 10.0;
pub const GAP: f64 = 10.0;
/// Top inset, like every other sidebar window.
pub const TOP: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Architect,
    Maps,
    /// Real Google Maps (the normal site, no API key) as the top-level page of a plain panel.
    MapsWeb,
    /// The place card (distance, times, address, photo, fares) beside the map.
    PlaceCard,
}

impl Panel {
    pub fn label(self) -> &'static str {
        match self {
            Panel::Architect => "architect-map",
            Panel::Maps => "maps-panel",
            Panel::MapsWeb => "maps-web",
            Panel::PlaceCard => "place-card",
        }
    }
    pub fn from_label(label: &str) -> Option<Panel> {
        match label {
            "architect-map" => Some(Panel::Architect),
            "maps-panel" => Some(Panel::Maps),
            "maps-web" => Some(Panel::MapsWeb),
            "place-card" => Some(Panel::PlaceCard),
            _ => None,
        }
    }
    /// (preferred, minimum, maximum) logical width.
    pub fn widths(self) -> (f64, f64, f64) {
        match self {
            Panel::Architect => (960.0, 480.0, 1400.0),
            Panel::Maps | Panel::MapsWeb => (720.0, 420.0, 1200.0),
            // Fixed narrow column; the height follows the screen.
            Panel::PlaceCard => (315.0, 315.0, 315.0),
        }
    }
}

/// Place panels (left-to-right order given) inside `span = (left, right)` in logical px.
/// `desired` / `min` per panel. Right-aligned means they hug the span's right end (next to a right-docked sidebar).
/// Returns `(x, width)` per panel, same order. Pure.
pub fn arrange(span: (f64, f64), panels: &[(f64, f64)], align_right: bool, gap: f64) -> Vec<(f64, f64)> {
    let n = panels.len();
    if n == 0 {
        return Vec::new();
    }
    let avail = (span.1 - span.0).max(1.0);
    let gaps = gap * (n as f64 - 1.0);
    let mut w: Vec<f64> = panels.iter().map(|p| p.0.max(p.1)).collect();
    let mut total: f64 = w.iter().sum::<f64>() + gaps;
    if total > avail {
        // Shrink towards the minimums, proportionally to how much each can give.
        let give: Vec<f64> = panels.iter().zip(&w).map(|(p, w)| (w - p.1).max(0.0)).collect();
        let can: f64 = give.iter().sum();
        let need = total - avail;
        if can > 0.0 {
            let take = need.min(can);
            for (wi, g) in w.iter_mut().zip(&give) {
                *wi -= take * g / can;
            }
        }
        total = w.iter().sum::<f64>() + gaps;
    }
    // Still too wide: overlap evenly (negative gap), but never move a panel off the span.
    let eff_gap = if n > 1 && total > avail { gap - (total - avail) / (n as f64 - 1.0) } else { gap };
    let used = w.iter().sum::<f64>() + eff_gap * (n as f64 - 1.0);
    let mut x = if align_right { span.1 - used } else { span.0 };
    x = x.max(span.0.min(span.1 - 1.0)).min(span.1 - 1.0);
    let mut out = Vec::with_capacity(n);
    for wi in w {
        out.push((x, wi));
        x += wi + eff_gap;
    }
    out
}

/// Remembered widths (Shift + arrows), logical px per panel kind.
static OVERRIDES: Mutex<Option<HashMap<Panel, f64>>> = Mutex::new(None);
/// Order the panels were opened in (oldest first).
static ORDER: Mutex<Vec<Panel>> = Mutex::new(Vec::new());

pub fn note_opened(p: Panel) {
    let mut o = ORDER.lock().unwrap();
    o.retain(|x| *x != p);
    o.push(p);
}

pub fn note_closed(p: Panel) {
    ORDER.lock().unwrap().retain(|x| *x != p);
}

pub fn width_for(p: Panel) -> f64 {
    let (pref, min, max) = p.widths();
    OVERRIDES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(&p).copied())
        .unwrap_or(pref)
        .clamp(min, max)
}

/// Grow or shrink a panel by `delta` logical px (clamped to its limits). Returns the new width.
pub fn resize(p: Panel, delta: f64) -> f64 {
    let (_, min, max) = p.widths();
    let new = (width_for(p) + delta).clamp(min, max);
    OVERRIDES.lock().unwrap().get_or_insert_with(HashMap::new).insert(p, new);
    new
}

/// The free span (left, right) in logical px beside the response sidebar, and whether panels hug its right end.
/// `sidebar` is `(x, width)` of the visible sidebar, or `None`. A sidebar on the right half leaves the span to its left;
/// one on the left half leaves the span to its right. Pure.
pub fn free_span(width: f64, sidebar: Option<(f64, f64)>) -> ((f64, f64), bool) {
    let mut span = (MARGIN, width - MARGIN);
    let mut align_right = true;
    if let Some((sx, sw)) = sidebar {
        if sx + sw / 2.0 >= width / 2.0 {
            span.1 = (sx - GAP).max(span.0 + 1.0);
        } else {
            span.0 = (sx + sw + GAP).min(span.1 - 1.0);
            align_right = false;
        }
    }
    (span, align_right)
}

/// Place every open panel. Safe to call any time; ignores panels whose window does not exist.
pub fn apply<R: Runtime>(app: &AppHandle<R>) {
    let order: Vec<Panel> = ORDER.lock().unwrap().iter().copied().filter(|p| app.get_webview_window(p.label()).is_some()).collect();
    if order.is_empty() {
        return;
    }
    let Some(any) = app.get_webview_window(order[0].label()) else { return };
    let Ok(Some(monitor)) = any.current_monitor().or_else(|_| any.primary_monitor()) else { return };
    let scale = if monitor.scale_factor().is_finite() && monitor.scale_factor() > 0.0 { monitor.scale_factor() } else { 1.0 };
    let (mp, ms) = (monitor.position(), monitor.size());
    let work = PRect { x: mp.x, y: mp.y, w: ms.width as i32, h: ms.height as i32 };
    let w_log = work.w as f64 / scale;
    let h_log = ((work.h as f64 / scale) - 2.0 * TOP).min(1040.0).max(400.0);

    // The response sidebar keeps its own place; the panels use the free span on its inner side.
    let mut sidebar = None;
    if let Some(sb) = app.get_webview_window("sidebar").filter(|w| w.is_visible().unwrap_or(false)) {
        if let (Ok(pos), Ok(size)) = (sb.outer_position(), sb.outer_size()) {
            sidebar = Some(((pos.x - work.x) as f64 / scale, size.width as f64 / scale));
        }
    }
    let (span, align_right) = free_span(w_log, sidebar);
    let specs: Vec<(f64, f64)> = order.iter().map(|p| (width_for(*p), p.widths().1)).collect();
    // Panels sit left-to-right in open order; the oldest is furthest from the sidebar.
    let placed = if align_right { arrange(span, &specs, true, GAP) } else {
        let mut rev: Vec<(f64, f64)> = specs.clone();
        rev.reverse();
        let mut p = arrange(span, &rev, false, GAP);
        p.reverse();
        p
    };
    for (panel, (x, w)) in order.iter().zip(placed) {
        if let Some(win) = app.get_webview_window(panel.label()) {
            let _ = win.set_size(tauri::PhysicalSize::new((w * scale).round() as u32, (h_log * scale).round() as u32));
            let _ = win.set_position(tauri::PhysicalPosition::new(work.x + (x * scale).round() as i32, work.y + (TOP * scale).round() as i32));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overlaps(a: (f64, f64), b: (f64, f64)) -> bool {
        a.0 < b.0 + b.1 - 0.001 && b.0 < a.0 + a.1 - 0.001
    }

    #[test]
    fn panels_that_fit_keep_their_preferred_width_and_do_not_overlap() {
        let r = arrange((10.0, 1900.0), &[(960.0, 480.0), (720.0, 420.0)], true, 10.0);
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].1, r[1].1), (960.0, 720.0));
        assert!(!overlaps(r[0], r[1]));
        assert!((r[1].0 + r[1].1 - 1900.0).abs() < 0.001, "hugs the sidebar side");
    }

    #[test]
    fn narrow_screens_shrink_towards_minimums_before_overlapping() {
        // A 1000 wide free span: the minimums (480 + 420 + gap) fit, the preferred widths do not.
        let r = arrange((10.0, 1000.0), &[(960.0, 480.0), (720.0, 420.0)], true, 10.0);
        assert!(r[0].1 >= 480.0 - 0.001 && r[1].1 >= 420.0 - 0.001, "{:?}", r);
        assert!(r[0].1 + r[1].1 + 10.0 <= 990.001, "{:?}", r);
        assert!(!overlaps(r[0], r[1]), "{:?}", r);
        assert!(r[0].0 >= 10.0 - 0.001 && r[1].0 + r[1].1 <= 1000.001);
    }

    #[test]
    fn when_even_the_minimums_do_not_fit_they_overlap_evenly_inside_the_span() {
        let r = arrange((10.0, 700.0), &[(960.0, 480.0), (720.0, 420.0)], true, 10.0);
        assert!(r[0].0 >= 10.0 - 0.001, "{:?}", r);
        assert!(r[1].0 + r[1].1 <= 700.001, "{:?}", r);
        assert!(r[1].0 > r[0].0, "later panel stays to the right");
    }

    #[test]
    fn many_screens_and_counts_stay_inside_the_work_area() {
        for (w, sidebar) in [(1366.0, 520.0), (1536.0, 520.0), (1920.0, 520.0), (2560.0, 520.0), (1280.0, 520.0)] {
            let span = (MARGIN, w - MARGIN - sidebar - GAP);
            for n in 1..=3usize {
                let specs: Vec<(f64, f64)> = [(960.0, 480.0), (720.0, 420.0), (720.0, 420.0)][..n].to_vec();
                for right in [true, false] {
                    let r = arrange(span, &specs, right, GAP);
                    assert_eq!(r.len(), n);
                    for (x, wi) in &r {
                        assert!(*x >= span.0 - 0.001 && x + wi <= span.1 + 0.001, "{w} n={n} right={right}: {r:?} span={span:?}");
                        assert!(*wi > 0.0);
                    }
                    // Whenever the minimums fit, nothing overlaps.
                    let min_total: f64 = specs.iter().map(|s| s.1).sum::<f64>() + GAP * (n as f64 - 1.0);
                    if min_total <= span.1 - span.0 {
                        for i in 0..n {
                            for j in i + 1..n {
                                assert!(!overlaps(r[i], r[j]), "{w} n={n}: {r:?}");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn resize_clamps_to_the_panel_limits_and_is_remembered() {
        let (_, min, max) = Panel::Maps.widths();
        assert_eq!(resize(Panel::Maps, -10_000.0), min);
        assert_eq!(width_for(Panel::Maps), min);
        assert_eq!(resize(Panel::Maps, 10_000.0), max);
        // restore for other tests
        OVERRIDES.lock().unwrap().take();
        assert_eq!(width_for(Panel::Maps), Panel::Maps.widths().0);
    }

    #[test]
    fn labels_round_trip() {
        for p in [Panel::Architect, Panel::Maps, Panel::MapsWeb, Panel::PlaceCard] {
            assert_eq!(Panel::from_label(p.label()), Some(p));
        }
        assert_eq!(Panel::from_label("sidebar"), None);
    }

    #[test]
    fn panels_sit_beside_the_sidebar_never_under_it() {
        // 1920 wide, a 520 px sidebar docked right (x = 1390): panels get the span to its left.
        let (span, right) = free_span(1920.0, Some((1390.0, 520.0)));
        assert!(right && span.1 <= 1390.0 - GAP + 0.001 && span.0 == MARGIN, "{span:?}");
        let placed = arrange(span, &[(960.0, 480.0), (720.0, 420.0)], right, GAP);
        for (x, w) in &placed {
            assert!(x + w <= 1390.0, "a panel reaches under the sidebar: {placed:?}");
        }
        // Sidebar docked left: the span is to its right and panels start after it.
        let (span, right) = free_span(1920.0, Some((10.0, 520.0)));
        assert!(!right && span.0 >= 530.0 && span.1 == 1920.0 - MARGIN, "{span:?}");
        // No sidebar: the whole width.
        assert_eq!(free_span(1920.0, None), ((MARGIN, 1920.0 - MARGIN), true));
    }
}
