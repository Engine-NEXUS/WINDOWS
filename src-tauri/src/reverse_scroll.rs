//! Pure logic for the scrolling page tour behind "Nexus, reverse engineer this
//! website": where the browser is placed next to the sidebar, how the wheel
//! burst adapts to the page, and how far the content actually moved between two
//! captured frames. No Win32, no network, no Tauri — the caller supplies the
//! work area and the frames; everything here is arithmetic that can be unit
//! tested on any platform.

#[cfg(test)]
use base64::Engine;
use crate::window_tile::Rect;

/// Left-docked sidebar width + the gap kept between it and the browser.
pub const SIDEBAR_W: i32 = 540;
/// Narrower work areas get no sidebar share at all.
pub const MIN_WORK_W: i32 = 1100;
/// The browser slice itself must stay at least this wide.
pub const MIN_PAGE_W: i32 = 640;

/// Where the browser goes and how wide the NEXUS sidebar is. Returns None when
/// the work area is too small to share (caller then tours in place).
pub fn plan_page_rect(work: Rect, sidebar_w: i32) -> Option<Rect> {
    if work.width() < MIN_WORK_W {
        return None;
    }
    let remaining = work.width() - sidebar_w - 10;
    if remaining < MIN_PAGE_W {
        return None;
    }
    Some(Rect {
        left: work.left + sidebar_w + 10,
        top: work.top,
        right: work.right,
        bottom: work.bottom,
    })
}

/// What the tour should do after a scroll + capture step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDecision {
    /// Keep scrolling (possibly with a different notch count).
    Continue,
    /// The page cannot move any further.
    AtEnd,
    /// The shift could not be measured; stop rather than scroll blindly.
    Stalled,
}

/// Wheel-notch controller: targets a shift of about 80 % of the viewport per
/// step, widening the burst when the page barely moved and narrowing it when a
/// single step overshot the viewport.
pub struct ScrollController {
    viewport_h: i32,
    notches: i32,
    consecutive_zero: i32,
    consecutive_none: i32,
    steps: i32,
}

impl ScrollController {
    pub fn new(viewport_h: i32) -> Self {
        Self {
            viewport_h: if viewport_h <= 0 { 1 } else { viewport_h },
            notches: 3,
            consecutive_zero: 0,
            consecutive_none: 0,
            steps: 0,
        }
    }

    /// How many wheel notches to send next (starts at 3, always 1..=40).
    pub fn notches(&self) -> i32 {
        self.notches
    }

    /// Call after each scroll + capture with the measured vertical shift in px
    /// (None = could not measure).
    pub fn observe(&mut self, shift_px: Option<i32>) -> ScrollDecision {
        self.steps += 1;
        if self.steps >= 12 {
            return ScrollDecision::AtEnd;
        }
        let Some(s) = shift_px else {
            self.consecutive_none += 1;
            return if self.consecutive_none >= 2 {
                ScrollDecision::Stalled
            } else {
                ScrollDecision::Continue
            };
        };
        self.consecutive_none = 0;
        if s < 4 {
            self.consecutive_zero += 1;
            return if self.consecutive_zero >= 2 {
                ScrollDecision::AtEnd
            } else {
                ScrollDecision::Continue
            };
        }
        self.consecutive_zero = 0;
        let vh = self.viewport_h as f64;
        if (s as f64) < 0.55 * vh {
            self.notches = (self.notches * 3 / 2 + 1).clamp(1, 40);
        } else if (s as f64) > 0.95 * vh {
            self.notches = (self.notches * 2 / 3).max(1);
        }
        ScrollDecision::Continue
    }
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
    fn plan_page_rect_splits_a_wide_work_area() {
        let w = work(100, 50, 1920, 1040);
        let r = plan_page_rect(w, SIDEBAR_W).expect("1920 shares");
        assert_eq!(r.left, w.left + SIDEBAR_W + 10);
        assert_eq!(r.left, w.left + 550);
        assert_eq!(r.right, w.right);
        assert_eq!(r.top, w.top);
        assert_eq!(r.bottom, w.bottom);
    }

    #[test]
    fn plan_page_rect_refuses_a_narrow_work_area() {
        assert!(plan_page_rect(work(0, 0, 1024, 768), SIDEBAR_W).is_none());
        // 1100 is the floor, but the leftover page half must also fit.
        assert!(plan_page_rect(work(0, 0, 1100, 800), SIDEBAR_W).is_none());
    }

    #[test]
    fn plan_page_rect_holds_the_minimum_page_width() {
        let w = work(0, 0, 1100, 800);
        // 1100 - 450 - 10 = 640 == MIN_PAGE_W -> allowed, left = 460.
        let r = plan_page_rect(w, 450).expect("exact minimum fits");
        assert_eq!(r.left, w.left + 450 + 10);
        assert_eq!(r.right - r.left, MIN_PAGE_W);
        // One more pixel of sidebar and the page half is too narrow.
        assert!(plan_page_rect(w, 451).is_none());
    }

    #[test]
    fn controller_converges_to_most_of_a_viewport() {
        // Each notch scrolls 40 px; the viewport is 900 px.
        let mut c = ScrollController::new(900);
        assert_eq!(c.notches(), 3);
        let mut in_band = false;
        for _ in 0..5 {
            let s = c.notches() * 40;
            let frac = s as f64 / 900.0;
            if (0.55..=0.95).contains(&frac) {
                in_band = true;
            }
            assert_eq!(c.observe(Some(s)), ScrollDecision::Continue);
        }
        assert!(in_band, "never landed a step within 55-95 % of the viewport");
    }

    #[test]
    fn controller_ends_a_finite_page_without_asking_for_too_many_notches() {
        // 3000 px page, 900 px viewport -> 2100 px of scrollable range.
        let mut c = ScrollController::new(900);
        let max_scroll = 3000 - 900;
        let mut pos = 0;
        let mut ended = false;
        for step in 0..40 {
            let want = c.notches() * 40;
            assert!((1..=40).contains(&c.notches()), "notch out of range");
            let s = want.min(max_scroll - pos);
            pos += s;
            if c.observe(Some(s)) == ScrollDecision::AtEnd {
                ended = true;
                assert!(step < 12, "took too long to notice the end");
                break;
            }
        }
        assert!(ended, "never reported AtEnd");
        assert_eq!(pos, max_scroll, "never reached the page bottom");
    }

    #[test]
    fn controller_stalls_after_two_unmeasurable_steps() {
        let mut c = ScrollController::new(800);
        assert_eq!(c.observe(None), ScrollDecision::Continue);
        assert_eq!(c.observe(None), ScrollDecision::Stalled);
    }

    #[test]
    fn controller_gives_up_after_twelve_steps() {
        let mut c = ScrollController::new(900);
        for _ in 0..11 {
            assert_eq!(c.observe(Some(700)), ScrollDecision::Continue);
        }
        assert_eq!(c.observe(Some(700)), ScrollDecision::AtEnd);
    }

    #[test]
    fn controller_needs_no_sane_viewport() {
        let mut c = ScrollController::new(0);
        assert_eq!(c.observe(Some(100)), ScrollDecision::Continue);
        assert_eq!(c.observe(Some(0)), ScrollDecision::Continue);
        assert_eq!(c.observe(None), ScrollDecision::Continue);
        assert!((1..=40).contains(&c.notches()));
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
