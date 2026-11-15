//! Pure logic for the scrolling page tour behind "Nexus, reverse engineer this
//! website": where the browser is placed next to the sidebar, when one
//! viewport step is done, whether the user took over, and how far the content
//! actually moved between two captured frames. No Win32, no network, no Tauri — the caller supplies the
//! work area and the frames; everything here is arithmetic that can be unit
//! tested on any platform.

#[cfg(test)]
use base64::Engine;
use crate::window_tile::Rect;

/// Narrower work areas get no sidebar share at all.
pub const MIN_WORK_W: i32 = 1100;
/// The browser slice itself must stay at least this wide.
pub const MIN_PAGE_W: i32 = 640;

/// The browser rectangle for a sidebar whose REAL right edge is `sidebar_right` (physical px, read from the placed
/// window, so the pair is flush at any display scale): from just right of it to the work area's right/bottom edge.
/// None when the work area is too small to share (the caller then scans in place and moves nothing).
pub fn plan_page_rect_beside(work: Rect, sidebar_right: i32, gap: i32) -> Option<Rect> {
    if work.width() < MIN_WORK_W {
        return None;
    }
    let left = (sidebar_right + gap).max(work.left);
    if work.right - left < MIN_PAGE_W {
        return None;
    }
    Some(Rect { left, top: work.top, right: work.right, bottom: work.bottom })
}

/// What the tour does after one page step (UI Automation scroll or PageDown)
/// and a fresh capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDecision {
    /// Moved far enough: tour this new view.
    Moved,
    /// Moved too little: step once more before touring.
    Retry,
    /// Moved and reached the bottom: tour this view, then stop.
    LastView,
    /// The page did not move at all: stop now, nothing new to show.
    NoMove,
    /// Could not tell whether it moved: stop rather than scroll blindly.
    Stalled,
}

/// Steps allowed per viewport (each one is a whole page, so two is plenty).
pub const MAX_STEPS_PER_VIEWPORT: u32 = 2;
/// A scroll percentage at or above this is the bottom of the page.
pub const BOTTOM_PERCENT: f64 = 99.5;

/// Decides one viewport's step. Created fresh for every viewport, so a long
/// page can never run out of steps halfway (the old shared controller ended
/// the whole tour after 12 wheel bursts in total).
pub struct ViewportStepper {
    viewport_h: i32,
    steps: u32,
}

impl ViewportStepper {
    pub fn new(viewport_h: i32) -> Self {
        Self { viewport_h: viewport_h.max(1), steps: 0 }
    }

    /// `shift_px`: measured content movement between the frames (None = could
    /// not measure). `pct_before` / `pct_after`: the page's vertical scroll
    /// percentage from UI Automation when available.
    pub fn observe(&mut self, shift_px: Option<i32>, pct_before: Option<f64>, pct_after: Option<f64>) -> StepDecision {
        self.steps += 1;
        let last_try = self.steps >= MAX_STEPS_PER_VIEWPORT;
        let pct_moved = matches!((pct_before, pct_after), (Some(b), Some(a)) if a - b > 0.1);
        let pixels_moved = shift_px.is_some_and(|s| s >= 4);
        let at_bottom = pct_after.is_some_and(|p| p >= BOTTOM_PERCENT);
        if at_bottom && (pct_moved || pixels_moved) {
            return StepDecision::LastView;
        }
        match shift_px {
            Some(s) if s as f64 >= 0.5 * self.viewport_h as f64 => StepDecision::Moved,
            Some(s) if s >= 4 => {
                if last_try { StepDecision::Moved } else { StepDecision::Retry }
            }
            Some(_) => {
                if pct_moved {
                    StepDecision::Moved
                } else if last_try {
                    StepDecision::NoMove
                } else {
                    StepDecision::Retry
                }
            }
            None => {
                if pct_moved {
                    StepDecision::Moved
                } else if last_try {
                    StepDecision::Stalled
                } else {
                    StepDecision::Retry
                }
            }
        }
    }
}

/// Did the user touch the mouse or keyboard since the tour started?
/// Ticks are `GetTickCount` milliseconds (they wrap every ~49 days, hence the
/// wrapping arithmetic). NEXUS's own key presses also reset the input clock,
/// so input within `slack_ms` of `our_send_tick` is ours, not the user's.
pub fn user_took_over(last_input_tick: u32, baseline_tick: u32, our_send_tick: Option<u32>, slack_ms: u32) -> bool {
    let newer = (last_input_tick.wrapping_sub(baseline_tick) as i32) > 0;
    if !newer {
        return false;
    }
    if let Some(ours) = our_send_tick {
        let delta = last_input_tick.wrapping_sub(ours) as i32;
        if (0..=slack_ms as i32).contains(&delta) {
            return false;
        }
    }
    true
}

/// The area whose movement tells whether the page scrolled: the browser's
/// document rectangle when UI Automation gives one (no tab strip, no address
/// bar, which never move and fooled the estimator), otherwise the window minus
/// its top 12 %.
pub fn measure_region(doc: Option<(i32, i32, i32, i32)>, win: (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    if let Some((x, y, w, h)) = doc {
        if w >= 100 && h >= 100 {
            return (x, y, w, h);
        }
    }
    let (x, y, w, h) = win;
    let cut = h * 12 / 100;
    (x, y + cut, w, (h - cut).max(1))
}

/// Grayscale rows for shift estimation: a frame reduced to at most [`GRAY_W`]
/// columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// Column budget for a frame kept by [`gray_from_jpeg_b64`].
pub const GRAY_W: usize = 96;

/// base64 -> decode with the `image` crate -> resize so width <= [`GRAY_W`]
/// preserving aspect -> luma8.
pub fn gray_from_jpeg_b64(b64: &str) -> Option<Gray> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let (w0, h0) = (img.width(), img.height());
    if w0 == 0 || h0 == 0 {
        return None;
    }
    let (tw, th) = if w0 > GRAY_W as u32 {
        let th = ((h0 as u64 * GRAY_W as u64) / w0 as u64).max(1) as u32;
        (GRAY_W as u32, th)
    } else {
        (w0, h0)
    };
    let resized = image::imageops::resize(&img, tw, th, image::imageops::FilterType::Triangle);
    let luma = image::DynamicImage::ImageRgba8(resized).to_luma8();
    Some(Gray {
        w: luma.width() as usize,
        h: luma.height() as usize,
        px: luma.into_raw(),
    })
}

/// Mean absolute row difference of two same-size frames for one candidate
/// shift, over the central 70 % of the columns. `shift = 0` compares the frames
/// as they are.
fn mean_abs_diff(prev: &Gray, next: &Gray, shift: usize) -> f64 {
    let w = prev.w;
    let x0 = w * 15 / 100;
    let x1 = w - x0; // exclusive; leaves 15 % off each side
    if x1 <= x0 || shift >= prev.h {
        return f64::INFINITY;
    }
    let mut sum = 0.0f64;
    let mut n = 0u64;
    for y in 0..(prev.h - shift) {
        let a = (y + shift) * w;
        let b = y * w;
        for x in x0..x1 {
            sum += (prev.px[a + x] as i64 - next.px[b + x] as i64).abs() as f64;
            n += 1;
        }
    }
    if n == 0 {
        return f64::INFINITY;
    }
    sum / n as f64
}

/// How many rows did the content move UP between two frames of the same page
/// (positive = scrolled down)? None when the frames do not match well enough.
pub fn estimate_scroll_shift(prev: &Gray, next: &Gray) -> Option<usize> {
    if prev.w != next.w || prev.h != next.h {
        return None;
    }
    if prev.px.len() != prev.w * prev.h || next.px.len() != next.w * next.h {
        return None;
    }
    let h = prev.h;
    if h < 24 {
        return None;
    }
    let d0 = mean_abs_diff(prev, next, 0);
    let max_s = (h as f64 * 0.9) as usize;
    let mut best_s = 0usize;
    let mut best_d = d0;
    for s in 1..=max_s {
        let d = mean_abs_diff(prev, next, s);
        if d < best_d {
            best_d = d;
            best_s = s;
        }
    }
    if best_s == 0 {
        return if d0 < 3.0 { Some(0) } else { None };
    }
    let overlap_ok = h - best_s >= h / 8;
    if best_d < 18.0 && best_d <= d0 * 0.75 && overlap_ok {
        Some(best_s)
    } else if d0 < 3.0 {
        // s = 0 already matches well and nothing beat it clearly.
        Some(0)
    } else {
        None
    }
}

/// Convert rows of a [`Gray`] frame to pixels of the captured image.
pub fn rows_to_px(rows: usize, gray_h: usize, region_h_px: i32) -> i32 {
    if gray_h == 0 {
        return 0;
    }
    (rows as i32) * region_h_px / (gray_h as i32)
}

/// How many viewports the tour should walk, from the spoken prompt. Case
/// insensitive, whole-word matches only; quick/brief wins over full/detailed
/// when both appear.
pub fn max_viewports(prompt: &str) -> usize {
    const QUICK: [&str; 4] = ["quick", "brief", "short", "fast"];
    const FULL: [&str; 7] = [
        "full",
        "detailed",
        "thorough",
        "everything",
        "whole",
        "complete",
        "entire",
    ];
    let lower = prompt.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.iter().any(|w| QUICK.contains(w)) {
        return 2;
    }
    if words.iter().any(|w| FULL.contains(w)) {
        return 6;
    }
    4
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(left: i32, top: i32, w: i32, h: i32) -> Rect {
        Rect {
            left,
            top,
            right: left + w,
            bottom: top + h,
        }
    }

    #[test]
    fn page_rect_sits_flush_beside_the_real_sidebar() {
        let w = work(0, 0, 1920, 1040);
        // a 520 px sidebar at x=10 on a 150 % display is 780 px wide: the page starts right after it, not under it
        let r = plan_page_rect_beside(w, 10 + 780, 12).expect("shares");
        assert_eq!(r.left, 802);
        assert_eq!((r.top, r.right, r.bottom), (0, 1920, 1040));
        // too little room left, or a small screen: nothing is moved
        assert!(plan_page_rect_beside(w, 1500, 12).is_none());
        assert!(plan_page_rect_beside(work(0, 0, 1024, 768), 500, 12).is_none());
        // a sidebar left of the work area never pushes the page outside it
        assert_eq!(plan_page_rect_beside(work(100, 0, 1920, 1000), 0, 0).unwrap().left, 100);
    }

    #[test]
    fn stepper_tours_after_one_full_page() {
        let mut s = ViewportStepper::new(900);
        assert_eq!(s.observe(Some(860), Some(0.0), Some(30.0)), StepDecision::Moved);
    }

    #[test]
    fn stepper_retries_once_then_accepts_a_small_move() {
        let mut s = ViewportStepper::new(900);
        assert_eq!(s.observe(Some(120), None, None), StepDecision::Retry);
        assert_eq!(s.observe(Some(120), None, None), StepDecision::Moved);
    }

    #[test]
    fn stepper_reports_the_bottom_as_a_last_view() {
        let mut s = ViewportStepper::new(900);
        assert_eq!(s.observe(Some(400), Some(80.0), Some(100.0)), StepDecision::LastView);
        let mut s2 = ViewportStepper::new(900);
        // already at the bottom and nothing moved: no new view
        assert_eq!(s2.observe(Some(0), Some(100.0), Some(100.0)), StepDecision::Retry);
        assert_eq!(s2.observe(Some(0), Some(100.0), Some(100.0)), StepDecision::NoMove);
    }

    #[test]
    fn stepper_trusts_the_scroll_percentage_when_pixels_are_unclear() {
        let mut s = ViewportStepper::new(900);
        assert_eq!(s.observe(None, Some(10.0), Some(35.0)), StepDecision::Moved);
        let mut s2 = ViewportStepper::new(900);
        assert_eq!(s2.observe(Some(0), Some(10.0), Some(35.0)), StepDecision::Moved);
    }

    #[test]
    fn stepper_stalls_when_nothing_can_be_measured() {
        let mut s = ViewportStepper::new(800);
        assert_eq!(s.observe(None, None, None), StepDecision::Retry);
        assert_eq!(s.observe(None, None, None), StepDecision::Stalled);
    }

    #[test]
    fn stepper_cap_is_per_viewport_not_per_tour() {
        // Ten viewports in a row, each needing its retry, never run out.
        for _ in 0..10 {
            let mut s = ViewportStepper::new(900);
            assert_eq!(s.observe(Some(200), None, None), StepDecision::Retry);
            assert_eq!(s.observe(Some(200), None, None), StepDecision::Moved);
        }
        assert_eq!(MAX_STEPS_PER_VIEWPORT, 2);
    }

    #[test]
    fn stepper_needs_no_sane_viewport() {
        let mut s = ViewportStepper::new(0);
        assert_eq!(s.observe(Some(100), None, None), StepDecision::Moved);
    }

    #[test]
    fn user_took_over_ignores_old_input_and_our_own_keys() {
        // no input since the tour started
        assert!(!user_took_over(1_000, 1_000, None, 300));
        assert!(!user_took_over(900, 1_000, None, 300));
        // the user moved the mouse
        assert!(user_took_over(1_500, 1_000, None, 300));
        // our PageDown at 1_400 reset the clock to 1_450: ours
        assert!(!user_took_over(1_450, 1_000, Some(1_400), 300));
        // input long after our key press: the user
        assert!(user_took_over(2_000, 1_000, Some(1_400), 300));
    }

    #[test]
    fn user_took_over_survives_tick_wraparound() {
        let baseline = u32::MAX - 100;
        assert!(user_took_over(50, baseline, None, 300), "input after the wrap is newer");
        assert!(!user_took_over(baseline - 10, baseline, None, 300));
        assert!(!user_took_over(20, baseline, Some(u32::MAX - 5), 300), "ours across the wrap");
    }

    #[test]
    fn measure_region_prefers_the_document_rect() {
        assert_eq!(measure_region(Some((560, 120, 1300, 900)), (550, 0, 1370, 1040)), (560, 120, 1300, 900));
        // too small or missing: window minus its toolbar band
        assert_eq!(measure_region(Some((0, 0, 50, 50)), (0, 0, 1000, 1000)), (0, 120, 1000, 880));
        assert_eq!(measure_region(None, (10, 20, 800, 500)), (10, 80, 800, 440));
    }

    /// Deterministic pseudo-random frame (no `rand` dependency).
    fn lcg_frame(w: usize, h: usize, seed: u32) -> Gray {
        let mut state = seed;
        let mut px = Vec::with_capacity(w * h);
        for _ in 0..(w * h) {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            px.push((state >> 24) as u8);
        }
        Gray { w, h, px }
    }

    /// `prev` with its first `drop` rows removed and `drop` fresh rows appended.
    fn shift_frame(prev: &Gray, drop: usize) -> Gray {
        let w = prev.w;
        let mut px = Vec::with_capacity(prev.px.len());
        px.extend_from_slice(&prev.px[drop * w..]);
        let extra = lcg_frame(w, drop, 0xDEAD_BEEF);
        px.extend_from_slice(&extra.px);
        Gray {
            w,
            h: prev.h,
            px,
        }
    }

    #[test]
    fn estimate_shift_finds_a_known_seventeen_row_scroll() {
        let prev = lcg_frame(96, 60, 42);
        let next = shift_frame(&prev, 17);
        assert_eq!(estimate_scroll_shift(&prev, &next), Some(17));
    }

    #[test]
    fn estimate_shift_reports_no_movement_for_identical_frames() {
        let prev = lcg_frame(96, 60, 7);
        let next = prev.clone();
        assert_eq!(estimate_scroll_shift(&prev, &next), Some(0));
    }

    #[test]
    fn estimate_shift_rejects_unrelated_frames() {
        let prev = lcg_frame(96, 60, 1);
        let next = lcg_frame(96, 60, 2);
        assert_eq!(estimate_scroll_shift(&prev, &next), None);
    }

    #[test]
    fn estimate_shift_rejects_mismatched_sizes() {
        let prev = lcg_frame(96, 60, 3);
        let next = lcg_frame(96, 59, 3);
        assert_eq!(estimate_scroll_shift(&prev, &next), None);
        assert_eq!(estimate_scroll_shift(&prev, &lcg_frame(48, 60, 3)), None);
    }

    #[test]
    fn estimate_shift_sees_still_frames_but_not_noise() {
        let white = Gray {
            w: 96,
            h: 60,
            px: vec![255; 96 * 60],
        };
        assert_eq!(estimate_scroll_shift(&white, &white.clone()), Some(0));
        let noise = lcg_frame(96, 60, 99);
        assert_eq!(estimate_scroll_shift(&white, &noise), None);
    }

    #[test]
    fn estimate_shift_refuses_a_shift_without_enough_overlap() {
        let prev = lcg_frame(96, 60, 11);
        let next = shift_frame(&prev, 55);
        assert_eq!(estimate_scroll_shift(&prev, &next), None);
    }

    #[test]
    fn estimate_shift_needs_tall_enough_frames() {
        let prev = lcg_frame(96, 10, 5);
        assert_eq!(estimate_scroll_shift(&prev, &prev.clone()), None);
    }

    #[test]
    fn rows_to_px_scales_into_the_captured_region() {
        assert_eq!(rows_to_px(17, 60, 900), 255);
        assert_eq!(rows_to_px(5, 0, 900), 0);
    }

    #[test]
    fn gray_from_jpeg_b64_downscales_and_keeps_aspect() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(200, 100, |x, _| {
            image::Rgb([(x % 256) as u8, 128, 200])
        }));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&img)
            .expect("encode");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        let g = gray_from_jpeg_b64(&b64).expect("decode");
        assert_eq!(g.w, GRAY_W);
        assert!(g.w <= GRAY_W);
        let exact_h = 100.0 * GRAY_W as f64 / 200.0;
        assert!((g.h as f64 - exact_h).abs() <= 1.0, "aspect drifted: {}", g.h);
        assert_eq!(g.px.len(), g.w * g.h);
    }

    #[test]
    fn gray_from_jpeg_b64_keeps_small_frames() {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(40, 20, |_, _| {
            image::Rgb([10, 20, 30])
        }));
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 80)
            .encode_image(&img)
            .expect("encode");
        let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        let g = gray_from_jpeg_b64(&b64).expect("decode");
        assert_eq!((g.w, g.h), (40, 20));
    }

    #[test]
    fn gray_from_jpeg_b64_rejects_garbage() {
        assert!(gray_from_jpeg_b64("not base64 at all!!").is_none());
        // Valid base64, not an image.
        assert!(gray_from_jpeg_b64("aGVsbG8gd29ybGQ=").is_none());
        assert!(gray_from_jpeg_b64("").is_none());
    }

    #[test]
    fn max_viewports_follows_the_phrase_table() {
        assert_eq!(max_viewports("show me everything"), 6);
        assert_eq!(max_viewports("give a full tour"), 6);
        assert_eq!(max_viewports("a detailed walk"), 6);
        assert_eq!(max_viewports("be thorough"), 6);
        assert_eq!(max_viewports("the whole page"), 6);
        assert_eq!(max_viewports("a complete pass"), 6);
        assert_eq!(max_viewports("the entire site"), 6);
        assert_eq!(max_viewports("QUICK look"), 2);
        assert_eq!(max_viewports("a brief scan"), 2);
        assert_eq!(max_viewports("give a short tour"), 2);
        assert_eq!(max_viewports("just a fast pass"), 2);
    }

    #[test]
    fn max_viewports_defaults_and_stays_whole_word() {
        assert_eq!(max_viewports("walk me through it"), 4);
        assert_eq!(max_viewports(""), 4);
        // Whole-word only: "quickly" is not "quick".
        assert_eq!(max_viewports("quickly now"), 4);
        assert_eq!(max_viewports("breakfast tour"), 4);
        // Quick wins when a prompt contains both families.
        assert_eq!(max_viewports("quick full overview"), 2);
    }
}
