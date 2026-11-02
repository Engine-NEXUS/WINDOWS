//! A small native orb + caption shown while the early hello is spoken (doc 158, slice 2).
//!
//! The real orb lives in the `stage` WebView, which does not exist until 5-25 s after logon. So while the cached
//! greeting plays, this draws the same idea natively: the black capsule at the top (or bottom) of the screen with a white
//! particle sphere that breathes with the voice, and below it the black caption pill that reveals the words as they are
//! spoken. It is a lightweight stand-in: drawn in software into a layered window (per-pixel alpha, click-through, never
//! takes focus), shown for the length of the clip plus a short linger, then it fades away and the window is destroyed.
//!
//! Everything about the picture is pure and tested (geometry, fades, the revealed words, the sphere, the compositing);
//! only the window and the text rasteriser are Windows glue at the bottom.

/// Logical (96-dpi) sizes. The capsule size comes from the user's orb settings (`orbScoopWidth` / `orbScoopHeight`).
pub const DEFAULT_CAPSULE_W: f32 = 301.0;
pub const DEFAULT_CAPSULE_H: f32 = 105.0;
const GAP: f32 = 12.0;
const CAPTION_MAX_W: f32 = 520.0;
const CAPTION_H: f32 = 44.0;
const PILL_CORNER: f32 = 10.0;
const SPHERE_RADIUS: f32 = 34.0;
const DOTS: usize = 640;

const FADE_IN_MS: u64 = 160;
const ORB_LINGER_MS: u64 = 900;
const CAPTION_LINGER_MS: u64 = 1_500;
const FADE_OUT_MS: u64 = 320;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where the overlay window goes: it spans the capsule and the caption, anchored to the top (or bottom) edge of the work
/// area, placed horizontally by the user's `orbHorizontalPct`. Returns (window, capsule inside the window).
pub fn layout(work: Rect, scale: f32, top: bool, x_pct: f32, capsule_w: f32, capsule_h: f32) -> (Rect, Rect) {
    let cap_w = (capsule_w * scale).round() as i32;
    let cap_h = (capsule_h * scale).round() as i32;
    let win_w = ((CAPTION_MAX_W.max(capsule_w)) * scale).round() as i32;
    let win_h = cap_h + ((GAP + CAPTION_H) * scale).round() as i32;
    let pct = x_pct.clamp(0.0, 1.0);
    let x = work.x + ((work.w - win_w).max(0) as f32 * pct).round() as i32;
    let y = if top { work.y } else { work.y + work.h - win_h };
    let win = Rect { x, y, w: win_w, h: win_h };
    // inside the window: the capsule hugs the screen edge, the caption sits on the other side of it
    let cx = (win_w - cap_w) / 2;
    let cy = if top { 0 } else { win_h - cap_h };
    (win, Rect { x: cx, y: cy, w: cap_w, h: cap_h })
}

/// The fades: (orb opacity, caption opacity, finished) for `t_ms` since the audio started and a clip of `clip_ms`.
pub fn alphas(t_ms: u64, clip_ms: u64) -> (f32, f32, bool) {
    let fade = |t: u64, start: u64, len: u64| -> f32 {
        if t < start {
            0.0
        } else {
            ((t - start) as f32 / len.max(1) as f32).clamp(0.0, 1.0)
        }
    };
    let orb_end = clip_ms + ORB_LINGER_MS;
    let cap_end = clip_ms + CAPTION_LINGER_MS;
    let orb = fade(t_ms, 0, FADE_IN_MS) * (1.0 - fade(t_ms, orb_end, FADE_OUT_MS));
    let cap = fade(t_ms, 40, FADE_IN_MS) * (1.0 - fade(t_ms, cap_end, FADE_OUT_MS));
    (orb, cap, t_ms >= cap_end.max(orb_end) + FADE_OUT_MS)
}

/// The loudness (0..1) at `t_ms`, from the clip's envelope frames.
pub fn envelope_at(env: &[f32], frame_ms: u64, t_ms: u64) -> f32 {
    if env.is_empty() || frame_ms == 0 {
        return 0.0;
    }
    env.get((t_ms / frame_ms) as usize).copied().unwrap_or(0.0).clamp(0.0, 1.0)
}

/// The words spoken so far, a word being revealed letter by letter across its duration. `words` = (text, start ms, ms).
/// With no word timings the whole line is shown.
pub fn visible_prefix(words: &[(String, u64, u64)], full: &str, t_ms: u64) -> String {
    if words.is_empty() {
        return full.to_string();
    }
    let mut out: Vec<String> = Vec::new();
    for (w, start, dur) in words {
        if t_ms < *start {
            break;
        }
        let chars: Vec<char> = w.chars().collect();
        if t_ms >= start + dur || *dur == 0 {
            out.push(w.clone());
        } else {
            let shown = (((t_ms - start) as f32 / *dur as f32) * chars.len() as f32).ceil() as usize;
            out.push(chars.iter().take(shown.clamp(1, chars.len())).collect());
            break;
        }
    }
    out.join(" ")
}

/// Evenly spread points on the unit sphere (Fibonacci lattice).
pub fn sphere_points(n: usize) -> Vec<[f32; 3]> {
    let golden = std::f32::consts::PI * (3.0 - 5.0f32.sqrt());
    (0..n)
        .map(|i| {
            let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
            let r = (1.0 - y * y).max(0.0).sqrt();
            let a = golden * i as f32;
            [r * a.cos(), y, r * a.sin()]
        })
        .collect()
}

// ─── software drawing into a premultiplied BGRA buffer ───────────────

#[inline]
fn pack(a: f32, r: f32, g: f32, b: f32) -> u32 {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
    (c(a) << 24) | (c(r) << 16) | (c(g) << 8) | c(b)
}

/// Source-over of a premultiplied colour (r,g,b already multiplied by a) onto the buffer.
#[inline]
fn over(buf: &mut [u32], idx: usize, a: f32, r: f32, g: f32, b: f32) {
    let d = buf[idx];
    let inv = 1.0 - a;
    let (da, dr, dg, db) = (((d >> 24) & 255) as f32 / 255.0, ((d >> 16) & 255) as f32 / 255.0, ((d >> 8) & 255) as f32 / 255.0, (d & 255) as f32 / 255.0);
    buf[idx] = pack(a + da * inv, r + dr * inv, g + dg * inv, b + db * inv);
}

/// Signed distance to a rounded rectangle whose top and bottom corners can have different radii.
fn sdf(px: f32, py: f32, r: Rect, r_top: f32, r_bottom: f32) -> f32 {
    let (cx, cy) = (r.x as f32 + r.w as f32 / 2.0, r.y as f32 + r.h as f32 / 2.0);
    let (hw, hh) = (r.w as f32 / 2.0, r.h as f32 / 2.0);
    let rad = if py < cy { r_top } else { r_bottom }.min(hw).min(hh);
    let (qx, qy) = ((px - cx).abs() - (hw - rad), (py - cy).abs() - (hh - rad));
    (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad
}

/// A solid black rounded shape with a faint 1 px white border (the capsule and the caption pill look alike).
pub fn fill_black_shape(buf: &mut [u32], width: i32, r: Rect, r_top: f32, r_bottom: f32, alpha: f32, border_alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let (y0, y1) = (r.y.max(0), (r.y + r.h).min(buf.len() as i32 / width.max(1)));
    for y in y0..y1 {
        for x in r.x.max(0)..(r.x + r.w).min(width) {
            let d = sdf(x as f32 + 0.5, y as f32 + 0.5, r, r_top, r_bottom);
            let cov = (0.5 - d).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let idx = (y * width + x) as usize;
            over(buf, idx, cov * alpha, 0.0, 0.0, 0.0);
            // the border: the outermost pixel ring, as a white film
            let ring = (1.0 - (-d - 0.5).abs().min(1.0)) * border_alpha * alpha * cov;
            if ring > 0.0 {
                over(buf, idx, ring, ring, ring, ring);
            }
        }
    }
}

/// The white particle sphere. `amp` (0..1) swells it with the voice; `t_s` rotates it.
pub fn draw_orb(buf: &mut [u32], width: i32, cx: f32, cy: f32, radius: f32, t_s: f32, amp: f32, alpha: f32, points: &[[f32; 3]]) {
    if alpha <= 0.0 {
        return;
    }
    let height = buf.len() as i32 / width.max(1);
    let (sy, cyw) = ((t_s * (0.55 + 0.9 * amp)).sin(), (t_s * (0.55 + 0.9 * amp)).cos());
    let (sx, cxw) = (0.38f32.sin(), 0.38f32.cos());
    let swell = radius * (1.0 + 0.16 * amp);
    for p in points {
        // rotate about Y, then tilt about X
        let (x1, z1) = (p[0] * cyw + p[2] * sy, -p[0] * sy + p[2] * cyw);
        let (y2, z2) = (p[1] * cxw - z1 * sx, p[1] * sx + z1 * cxw);
        let depth = (z2 + 1.0) / 2.0; // 0 far .. 1 near
        let persp = 1.0 + 0.12 * z2;
        let (dx, dy) = (cx + x1 * swell * persp, cy + y2 * swell * persp);
        let dot_r = (0.55 + 0.75 * depth) * (radius / SPHERE_RADIUS).max(0.5);
        let a = (0.25 + 0.75 * depth) * alpha;
        let (bx0, bx1) = ((dx - dot_r - 1.0).floor() as i32, (dx + dot_r + 1.0).ceil() as i32);
        let (by0, by1) = ((dy - dot_r - 1.0).floor() as i32, (dy + dot_r + 1.0).ceil() as i32);
        for y in by0.max(0)..by1.min(height) {
            for x in bx0.max(0)..bx1.min(width) {
                let dist = ((x as f32 + 0.5 - dx).powi(2) + (y as f32 + 0.5 - dy).powi(2)).sqrt();
                let cov = (dot_r + 0.5 - dist).clamp(0.0, 1.0) * a;
                if cov > 0.0 {
                    over(buf, (y * width + x) as usize, cov, cov, cov, cov);
                }
            }
        }
    }
}

/// White text from a coverage mask (0..255 per pixel) onto the buffer at (dx, dy).
pub fn composite_mask(buf: &mut [u32], width: i32, mask: &[u8], mw: i32, mh: i32, dx: i32, dy: i32, alpha: f32) {
    let height = buf.len() as i32 / width.max(1);
    for y in 0..mh {
        for x in 0..mw {
            let (tx, ty) = (dx + x, dy + y);
            if tx < 0 || ty < 0 || tx >= width || ty >= height {
                continue;
            }
            let c = mask[(y * mw + x) as usize] as f32 / 255.0 * alpha;
            if c > 0.0 {
                over(buf, (ty * width + tx) as usize, c, c, c, c);
            }
        }
    }
}

/// The background shape the user chose in the Command Hub (`orbCapsuleStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The fluid bezel scoop: a catenary bowl hanging from the screen edge (the default).
    Pebble,
    /// The plain black capsule: a square box with the two outer corners rounded by 10 px.
    Classic,
}

/// Everything the overlay needs to know about the clip being spoken.
#[derive(Debug, Clone)]
pub struct Show {
    pub style: Style,
    /// The faint rim line of the pebble (`orbBaseRimOpacity` / 100).
    pub rim_alpha: f32,
    pub text: String,
    pub words: Vec<(String, u64, u64)>,
    pub env: Vec<f32>,
    pub env_frame_ms: u64,
    pub clip_ms: u64,
    pub top: bool,
    pub x_pct: f32,
    pub capsule_w: f32,
    pub capsule_h: f32,
}

/// The depth of the pebble bowl (px from the screen edge) at every pixel column 0..w. The real orb draws two cubic
/// Beziers from the top corners to the lowest point in the middle: (0,0) (wing,0) (cx-0.26w,h) (cx,h), mirrored.
pub fn bowl_profile(w: f32, h: f32, wing: f32) -> Vec<f32> {
    let cx = w / 2.0;
    let (p0, p1, p2, p3) = ((0.0f32, 0.0f32), (wing, 0.0f32), (cx - 0.26 * w, h), (cx, h));
    // sample the left curve and read it back by x (x is monotonic along it)
    let n = 600;
    let pts: Vec<(f32, f32)> = (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            let bx = u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0;
            let by = u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1;
            (bx, by)
        })
        .collect();
    let depth_at = |x: f32| -> f32 {
        let x = x.clamp(0.0, cx);
        let i = pts.partition_point(|p| p.0 < x).min(n);
        if i == 0 {
            return pts[0].1;
        }
        let ((x0, y0), (x1, y1)) = (pts[i - 1], pts[i]);
        if (x1 - x0).abs() < 1e-6 {
            y1
        } else {
            y0 + (y1 - y0) * ((x - x0) / (x1 - x0)).clamp(0.0, 1.0)
        }
    };
    (0..w.ceil() as usize)
        .map(|i| {
            let x = i as f32 + 0.5;
            depth_at(if x <= cx { x } else { w - x })
        })
        .collect()
}

/// The pebble: a black bowl hanging from the screen edge (top) or rising from it (bottom), with a faint rim line.
pub fn fill_pebble(buf: &mut [u32], width: i32, r: Rect, profile: &[f32], top: bool, alpha: f32, rim_alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let height = buf.len() as i32 / width.max(1);
    for y in r.y.max(0)..(r.y + r.h).min(height) {
        for x in r.x.max(0)..(r.x + r.w).min(width) {
            let u = (x - r.x) as usize;
            let Some(&depth) = profile.get(u) else { continue };
            let slope = (profile.get(u + 1).copied().unwrap_or(depth) - profile.get(u.saturating_sub(1)).copied().unwrap_or(depth)) / 2.0;
            let y_rel = if top { (y - r.y) as f32 + 0.5 } else { (r.y + r.h - y) as f32 - 0.5 };
            let d = (depth - y_rel) / (1.0 + slope * slope).sqrt(); // > 0 inside the bowl
            let cov = (0.5 + d).clamp(0.0, 1.0);
            if cov > 0.0 {
                let idx = (y * width + x) as usize;
                over(buf, idx, cov * alpha, 0.0, 0.0, 0.0);
                let ring = (1.0 - (d.abs() / 1.2).min(1.0)) * rim_alpha * alpha;
                if ring > 0.0 && d.abs() < 1.2 {
                    over(buf, idx, ring, ring, ring, ring);
                }
            }
        }
    }
}

/// Draw one frame of everything except the caption text. Returns the caption pill's centre-top anchor so the glue can
/// rasterise the revealed text there. Pure.
pub fn render_frame(buf: &mut [u32], width: i32, capsule: Rect, scale: f32, top: bool, t_ms: u64, s: &Show, points: &[[f32; 3]]) -> (f32, f32) {
    for p in buf.iter_mut() {
        *p = 0;
    }
    let (orb_a, _, _) = alphas(t_ms, s.clip_ms);
    let amp = envelope_at(&s.env, s.env_frame_ms, t_ms);
    let (cx, mid_y) = (capsule.x as f32 + capsule.w as f32 / 2.0, capsule.y as f32 + capsule.h as f32 / 2.0);
    let (cy, radius) = match s.style {
        Style::Pebble => {
            let wing = (120.0 * scale).min(capsule.w as f32 * 0.25);
            let profile = bowl_profile(capsule.w as f32, capsule.h as f32, wing);
            fill_pebble(buf, width, capsule, &profile, top, orb_a, s.rim_alpha);
            // the sphere nestles in the pocket, a little above the lowest point
            let off = capsule.h as f32 * 0.04;
            (if top { mid_y - off } else { mid_y + off }, SPHERE_RADIUS * scale)
        }
        Style::Classic => {
            // the plain capsule: the screen-side corners are square, the other two are rounded by 10 px
            let corner = PILL_CORNER * scale;
            let (r_top, r_bottom) = if top { (0.0, corner) } else { (corner, 0.0) };
            fill_black_shape(buf, width, capsule, r_top, r_bottom, orb_a, 0.16);
            (mid_y, capsule.w as f32 * 0.30)
        }
    };
    draw_orb(buf, width, cx, cy, radius, t_ms as f32 / 1000.0, amp, orb_a, points);
    let pill_y = if top { (capsule.y + capsule.h) as f32 + GAP * scale } else { capsule.y as f32 - GAP * scale };
    (cx, pill_y)
}

/// The pill rectangle for a measured text, centred on `cx`, hanging below (top) or rising above (bottom) `anchor_y`.
pub fn pill_rect(cx: f32, anchor_y: f32, text_w: i32, text_h: i32, scale: f32, top: bool) -> Rect {
    let (pad_x, pad_y) = ((18.0 * scale).round() as i32, (8.0 * scale).round() as i32);
    let (w, h) = (text_w + 2 * pad_x, text_h + 2 * pad_y);
    Rect { x: (cx - w as f32 / 2.0).round() as i32, y: if top { anchor_y.round() as i32 } else { anchor_y.round() as i32 - h }, w, h }
}

pub fn pill_corner(scale: f32) -> f32 {
    PILL_CORNER * scale
}

pub fn dot_count() -> usize {
    DOTS
}

pub fn caption_max_width(scale: f32) -> i32 {
    ((CAPTION_MAX_W - 36.0) * scale).round() as i32
}

pub fn caption_height(scale: f32) -> i32 {
    ((CAPTION_H - 16.0) * scale).round() as i32
}

// ─── the window (Windows only) ───────────────────────────────────────

/// Show the overlay for the length of the clip. Returns at once; the window lives on its own thread. Best effort: any
/// failure is logged and the greeting is simply heard without a picture.
#[cfg(target_os = "windows")]
pub fn show(s: Show) {
    let _ = std::thread::Builder::new().name("early-orb".into()).spawn(move || {
        if let Err(e) = win::run(s) {
            println!("[EARLY-ORB] not shown: {e}");
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub fn show(_s: Show) {}

#[cfg(target_os = "windows")]
mod win {
    use super::*;
    use std::cell::RefCell;
    use std::ffi::c_void;

    type Hwnd = isize;

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct Size {
        cx: i32,
        cy: i32,
    }
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct RectRaw {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }
    #[repr(C)]
    struct WndClassW {
        style: u32,
        proc_: unsafe extern "system" fn(Hwnd, u32, usize, isize) -> isize,
        cls_extra: i32,
        wnd_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu: *const u16,
        class_name: *const u16,
    }
    #[repr(C)]
    struct BlendFunction {
        op: u8,
        flags: u8,
        alpha: u8,
        format: u8,
    }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32,
        width: i32,
        height: i32,
        planes: u16,
        bit_count: u16,
        compression: u32,
        size_image: u32,
        x_ppm: i32,
        y_ppm: i32,
        clr_used: u32,
        clr_important: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterClassW(class: *const WndClassW) -> u16;
        fn CreateWindowExW(ex: u32, class: *const u16, title: *const u16, style: u32, x: i32, y: i32, w: i32, h: i32, parent: Hwnd, menu: isize, inst: isize, param: *const c_void) -> Hwnd;
        fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
        fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
        fn SetTimer(h: Hwnd, id: usize, ms: u32, f: *const c_void) -> usize;
        fn GetMessageW(m: *mut Msg, h: Hwnd, a: u32, b: u32) -> i32;
        fn TranslateMessage(m: *const Msg) -> i32;
        fn DispatchMessageW(m: *const Msg) -> isize;
        fn PostQuitMessage(code: i32);
        fn DestroyWindow(h: Hwnd) -> i32;
        fn UpdateLayeredWindow(h: Hwnd, dst: isize, pos: *const Point, size: *const Size, src: isize, src_pos: *const Point, key: u32, blend: *const BlendFunction, flags: u32) -> i32;
        fn GetSystemMetrics(i: i32) -> i32;
        fn SystemParametersInfoW(action: u32, p1: u32, p2: *mut c_void, flags: u32) -> i32;
        fn SetThreadDpiAwarenessContext(ctx: isize) -> isize;
        fn GetDpiForSystem() -> u32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateCompatibleDC(dc: isize) -> isize;
        fn CreateDIBSection(dc: isize, bmi: *const BitmapInfoHeader, usage: u32, bits: *mut *mut c_void, section: isize, offset: u32) -> isize;
        fn SelectObject(dc: isize, obj: isize) -> isize;
        fn DeleteObject(obj: isize) -> i32;
        fn DeleteDC(dc: isize) -> i32;
        fn CreateFontW(h: i32, w: i32, esc: i32, orient: i32, weight: i32, italic: u32, underline: u32, strike: u32, charset: u32, out: u32, clip: u32, quality: u32, pitch: u32, face: *const u16) -> isize;
        fn SetTextColor(dc: isize, color: u32) -> u32;
        fn SetBkMode(dc: isize, mode: i32) -> i32;
        fn TextOutW(dc: isize, x: i32, y: i32, s: *const u16, n: i32) -> i32;
        fn GetTextExtentPoint32W(dc: isize, s: *const u16, n: i32, out: *mut Size) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> isize;
    }

    const WM_DESTROY: u32 = 0x0002;
    const WM_TIMER: u32 = 0x0113;
    const TIMER_ID: usize = 1;

    struct Dib {
        dc: isize,
        bitmap: isize,
        bits: *mut u32,
        w: i32,
        h: i32,
    }

    impl Dib {
        fn new(w: i32, h: i32) -> Option<Dib> {
            // SAFETY: a memory DC and a top-down 32-bit DIB section; the pixel pointer lives as long as the bitmap.
            unsafe {
                let dc = CreateCompatibleDC(0);
                if dc == 0 {
                    return None;
                }
                let bmi = BitmapInfoHeader {
                    size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                    width: w,
                    height: -h,
                    planes: 1,
                    bit_count: 32,
                    compression: 0,
                    size_image: 0,
                    x_ppm: 0,
                    y_ppm: 0,
                    clr_used: 0,
                    clr_important: 0,
                };
                let mut bits: *mut c_void = std::ptr::null_mut();
                let bitmap = CreateDIBSection(dc, &bmi, 0, &mut bits, 0, 0);
                if bitmap == 0 || bits.is_null() {
                    DeleteDC(dc);
                    return None;
                }
                SelectObject(dc, bitmap);
                Some(Dib { dc, bitmap, bits: bits as *mut u32, w, h })
            }
        }
        fn pixels(&mut self) -> &mut [u32] {
            // SAFETY: the section is w*h 32-bit pixels and exclusively owned by this struct.
            unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
        }
    }

    impl Drop for Dib {
        fn drop(&mut self) {
            // SAFETY: handles created in `new`.
            unsafe {
                DeleteObject(self.bitmap);
                DeleteDC(self.dc);
            }
        }
    }

    struct State {
        show: Show,
        window: Rect,
        capsule: Rect,
        scale: f32,
        started: std::time::Instant,
        canvas: Dib,
        text: Dib,
        font: isize,
        points: Vec<[f32; 3]>,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Rasterise `text` into the mask bitmap with GDI and return (coverage mask, width, height, measured width).
    fn text_mask(st: &mut State, text: &str) -> (Vec<u8>, i32, i32, i32, i32) {
        let w16: Vec<u16> = text.encode_utf16().collect();
        // SAFETY: GDI calls on the DC and font owned by `st`; the string outlives the calls.
        unsafe {
            for p in st.text.pixels().iter_mut() {
                *p = 0;
            }
            SelectObject(st.text.dc, st.font);
            SetTextColor(st.text.dc, 0x00FF_FFFF);
            SetBkMode(st.text.dc, 1);
            let mut size = Size::default();
            GetTextExtentPoint32W(st.text.dc, w16.as_ptr(), w16.len() as i32, &mut size);
            TextOutW(st.text.dc, 0, 0, w16.as_ptr(), w16.len() as i32);
            let (mw, mh) = (st.text.w, st.text.h);
            let mask: Vec<u8> = st.text.pixels().iter().map(|p| (*p & 255) as u8).collect();
            (mask, mw, mh, size.cx.min(mw), size.cy.min(mh))
        }
    }

    /// Draw the whole picture for `t_ms` into the canvas. `true` when the show is over.
    fn compose(st: &mut State, t_ms: u64) -> bool {
        let (_, cap_a, done) = alphas(t_ms, st.show.clip_ms);
        let width = st.window.w;
        let (cx, anchor) = {
            let (show, capsule, scale, top) = (st.show.clone(), st.capsule, st.scale, st.show.top);
            let points = std::mem::take(&mut st.points);
            let r = render_frame(st.canvas.pixels(), width, capsule, scale, top, t_ms, &show, &points);
            st.points = points;
            r
        };
        let shown = visible_prefix(&st.show.words, &st.show.text, t_ms);
        if cap_a > 0.0 && !shown.is_empty() {
            let (mask, mw, mh, tw, th) = text_mask(st, &shown);
            let pill = pill_rect(cx, anchor, tw, th, st.scale, st.show.top);
            fill_black_shape(st.canvas.pixels(), width, pill, pill_corner(st.scale), pill_corner(st.scale), cap_a, 0.16);
            let (pad_x, pad_y) = ((18.0 * st.scale).round() as i32, (8.0 * st.scale).round() as i32);
            // the mask bitmap is wider than the text: cut out exactly the text rectangle (the mask has its own row width)
            let (cw, ch) = (tw.min(mw).max(0), th.min(mh).max(0));
            let mut cropped = Vec::with_capacity((cw * ch) as usize);
            for y in 0..ch {
                cropped.extend_from_slice(&mask[(y * mw) as usize..(y * mw + cw) as usize]);
            }
            composite_mask(st.canvas.pixels(), width, &cropped, cw, ch, pill.x + pad_x, pill.y + pad_y, cap_a);
        }
        done
    }

    fn draw(st: &mut State, hwnd: Hwnd) -> bool {
        let done = compose(st, st.started.elapsed().as_millis() as u64);
        // SAFETY: the canvas DC holds a 32-bit premultiplied bitmap the size of the window.
        unsafe {
            let pos = Point { x: st.window.x, y: st.window.y };
            let size = Size { cx: st.window.w, cy: st.window.h };
            let src = Point::default();
            let blend = BlendFunction { op: 0, flags: 0, alpha: 255, format: 1 };
            UpdateLayeredWindow(hwnd, 0, &pos, &size, st.canvas.dc, &src, 0, &blend, 2);
        }
        done
    }

    unsafe extern "system" fn proc_(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
        match msg {
            WM_TIMER => {
                let done = STATE.with(|s| s.borrow_mut().as_mut().map(|st| draw(st, hwnd)).unwrap_or(true));
                if done {
                    DestroyWindow(hwnd);
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    /// Everything except the window: the layout, the bitmaps, the font and the sphere.
    unsafe fn build_state(s: Show) -> Result<State, String> {
        // draw in real pixels whatever the DPI of the process is this early in startup
        SetThreadDpiAwarenessContext(-4);
        let scale = GetDpiForSystem().max(96) as f32 / 96.0;
        let mut raw = RectRaw::default();
        let work = if SystemParametersInfoW(0x0030, 0, &mut raw as *mut RectRaw as *mut c_void, 0) != 0 {
            Rect { x: raw.left, y: raw.top, w: raw.right - raw.left, h: raw.bottom - raw.top }
        } else {
            Rect { x: 0, y: 0, w: GetSystemMetrics(0), h: GetSystemMetrics(1) }
        };
        let (window, capsule) = layout(work, scale, s.top, s.x_pct, s.capsule_w, s.capsule_h);
        let canvas = Dib::new(window.w, window.h).ok_or("no canvas")?;
        let text = Dib::new(caption_max_width(scale), caption_height(scale)).ok_or("no text bitmap")?;
        let face = wide("Segoe UI");
        let font = CreateFontW(-((18.0 * scale).round() as i32), 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, 4, 0, face.as_ptr());
        if font == 0 {
            return Err("no font".into());
        }
        Ok(State { show: s, window, capsule, scale, started: std::time::Instant::now(), canvas, text, font, points: sphere_points(dot_count()) })
    }

    pub fn run(s: Show) -> Result<(), String> {
        // SAFETY: plain user32/gdi32 setup on this thread; every handle is released before returning.
        unsafe {
            let state = build_state(s)?;
            let window = state.window;
            let class = wide("NexusEarlyOrb");
            let inst = GetModuleHandleW(std::ptr::null());
            let wc = WndClassW {
                style: 0,
                proc_,
                cls_extra: 0,
                wnd_extra: 0,
                instance: inst,
                icon: 0,
                cursor: 0,
                background: 0,
                menu: std::ptr::null(),
                class_name: class.as_ptr(),
            };
            RegisterClassW(&wc); // 0 when it already exists, which is fine
            // layered | topmost | toolwindow (no taskbar button) | noactivate | transparent (clicks pass through)
            let ex = 0x0008_0000 | 0x0000_0008 | 0x0000_0080 | 0x0800_0000 | 0x0000_0020;
            let hwnd = CreateWindowExW(ex, class.as_ptr(), class.as_ptr(), 0x8000_0000, window.x, window.y, window.w, window.h, 0, 0, inst, std::ptr::null());
            if hwnd == 0 {
                DeleteObject(state.font);
                return Err("CreateWindowExW failed".into());
            }
            STATE.with(|st| *st.borrow_mut() = Some(State { started: std::time::Instant::now(), ..state }));
            ShowWindow(hwnd, 4); // SW_SHOWNOACTIVATE
            SetTimer(hwnd, TIMER_ID, 16, std::ptr::null());
            let mut msg = std::mem::zeroed::<Msg>();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            STATE.with(|st| {
                if let Some(state) = st.borrow_mut().take() {
                    DeleteObject(state.font);
                }
            });
        }
        Ok(())
    }

    /// Test helper: the composed canvas at `t_ms` as (premultiplied BGRA pixels, width, height), no window involved.
    #[cfg(test)]
    pub fn preview(s: Show, t_ms: u64) -> Option<(Vec<u32>, i32, i32)> {
        // SAFETY: as `run`; the state is dropped (font deleted) before returning.
        unsafe {
            let mut st = build_state(s).ok()?;
            compose(&mut st, t_ms);
            let out = (st.canvas.pixels().to_vec(), st.window.w, st.window.h);
            DeleteObject(st.font);
            Some(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    fn show_for(clip_ms: u64) -> Show {
        Show {
            style: Style::Pebble,
            rim_alpha: 0.25,
            text: "Good evening, sir".into(),
            words: vec![("Good".into(), 0, 300), ("evening,".into(), 300, 600), ("sir".into(), 900, 400)],
            env: vec![0.2, 0.9, 0.5],
            env_frame_ms: 20,
            clip_ms,
            top: true,
            x_pct: 0.5,
            capsule_w: DEFAULT_CAPSULE_W,
            capsule_h: DEFAULT_CAPSULE_H,
        }
    }

    #[test]
    fn the_window_hugs_the_top_or_bottom_edge_and_follows_the_horizontal_setting() {
        let (win, cap) = layout(WORK, 1.0, true, 0.5, 301.0, 105.0);
        assert_eq!((win.y, cap.y), (0, 0), "top: flush with the screen edge");
        assert_eq!(win.w, 520);
        assert_eq!(win.x, (1920 - 520) / 2);
        assert_eq!(cap.x, (520 - 301) / 2, "the capsule is centred in the window");
        assert_eq!(win.h, 105 + 12 + 44);
        let (win_b, cap_b) = layout(WORK, 1.0, false, 0.5, 301.0, 105.0);
        assert_eq!(win_b.y + win_b.h, 1040, "bottom: on the bottom of the work area");
        assert_eq!(cap_b.y, win_b.h - 105, "the capsule is at the screen side, the caption above it");
        let (left, _) = layout(WORK, 1.0, true, 0.0, 301.0, 105.0);
        assert_eq!(left.x, 0);
        let (hi, _) = layout(WORK, 1.5, true, 0.5, 301.0, 105.0);
        assert_eq!(hi.w, 780, "scaled by the display scale");
    }

    #[test]
    fn it_fades_in_lingers_fades_out_and_ends() {
        let clip = 2_000;
        let (o0, c0, d0) = alphas(0, clip);
        assert!(o0 < 0.01 && c0 < 0.01 && !d0, "starts invisible");
        let (o1, c1, _) = alphas(500, clip);
        assert!(o1 > 0.99 && c1 > 0.99, "fully there while speaking");
        let (o2, c2, d2) = alphas(clip + 900 + 320 / 2, clip);
        assert!(o2 < 0.6 && c2 > 0.99 && !d2, "the orb goes first, the caption stays a little longer");
        let (_, _, done) = alphas(clip + 1_500 + 320, clip);
        assert!(done);
        assert!(!alphas(clip + 1_000, clip).2);
    }

    #[test]
    fn the_caption_reveals_as_spoken_letter_by_letter() {
        let s = show_for(1_300);
        assert_eq!(visible_prefix(&s.words, &s.text, 0), "G", "the first letter appears with the first word");
        assert_eq!(visible_prefix(&s.words, &s.text, 150), "Go");
        assert_eq!(visible_prefix(&s.words, &s.text, 299), "Good", "the first word is complete");
        assert_eq!(visible_prefix(&s.words, &s.text, 300), "Good e", "and the next one starts with its first letter");
        assert_eq!(visible_prefix(&s.words, &s.text, 600), "Good even", "half way through the 8 letters of 'evening,'");
        assert_eq!(visible_prefix(&s.words, &s.text, 5_000), "Good evening, sir");
        assert_eq!(visible_prefix(&[], "Good evening, sir", 10), "Good evening, sir", "no timings: the whole line");
    }

    #[test]
    fn the_envelope_is_read_by_time_and_is_safe_out_of_range() {
        let env = [0.2, 0.9, 0.5];
        assert_eq!(envelope_at(&env, 20, 0), 0.2);
        assert_eq!(envelope_at(&env, 20, 25), 0.9);
        assert_eq!(envelope_at(&env, 20, 10_000), 0.0);
        assert_eq!(envelope_at(&[], 20, 5), 0.0);
        assert_eq!(envelope_at(&env, 0, 5), 0.0);
    }

    #[test]
    fn the_sphere_points_lie_on_the_unit_sphere_and_are_spread_out() {
        let pts = sphere_points(400);
        assert_eq!(pts.len(), 400);
        for p in &pts {
            let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
            assert!((r - 1.0).abs() < 1e-4, "{r}");
        }
        let mean_y: f32 = pts.iter().map(|p| p[1]).sum::<f32>() / 400.0;
        assert!(mean_y.abs() < 0.01, "balanced top and bottom");
    }

    #[test]
    fn the_pebble_is_a_bowl_deepest_in_the_middle_and_flush_with_the_edge_at_the_corners() {
        let prof = bowl_profile(301.0, 105.0, 75.25);
        assert_eq!(prof.len(), 301);
        assert!(prof[0] < 3.0 && prof[300] < 3.0, "the tips touch the screen edge: {} {}", prof[0], prof[300]);
        assert!((prof[150] - 105.0).abs() < 1.0, "deepest in the middle: {}", prof[150]);
        assert!((prof[60] - prof[240]).abs() < 0.5, "symmetric");
        assert!(prof[100] > prof[60] && prof[150] >= prof[100], "it deepens towards the middle");
    }

    #[test]
    fn each_style_draws_its_own_background() {
        let (win, cap) = layout(WORK, 1.0, true, 0.5, 301.0, 105.0);
        let pts = sphere_points(dot_count());
        let alpha = |buf: &Vec<u32>, x: i32, y: i32| buf[(y * win.w + x) as usize] >> 24;
        // pebble: the bowl's upper corner area (just inside the capsule rect, near the top-left) is empty sky beside the wing
        let mut pebble = vec![0u32; (win.w * win.h) as usize];
        render_frame(&mut pebble, win.w, cap, 1.0, true, 800, &show_for(2_000), &pts);
        assert_eq!(alpha(&pebble, cap.x + 20, cap.y + 40), 0, "below the wing, outside the bowl");
        assert!(alpha(&pebble, cap.x + cap.w / 2, cap.y + 90) > 200, "inside the bowl at the middle");
        assert!(alpha(&pebble, cap.x + cap.w / 2, cap.y + cap.h + 4) == 0, "nothing under the lowest point");
        // classic: a solid box, so the same corner area IS black; the bottom corners are rounded by 10 px
        let mut classic = vec![0u32; (win.w * win.h) as usize];
        let mut s = show_for(2_000);
        s.style = Style::Classic;
        let (_, cap_c) = layout(WORK, 1.0, true, 0.5, 200.0, 200.0);
        let (win_c, _) = layout(WORK, 1.0, true, 0.5, 200.0, 200.0);
        let mut classic_buf = vec![0u32; (win_c.w * win_c.h) as usize];
        render_frame(&mut classic_buf, win_c.w, cap_c, 1.0, true, 800, &s, &pts);
        let ca = |x: i32, y: i32| classic_buf[(y * win_c.w + x) as usize] >> 24;
        assert_eq!(ca(cap_c.x + 20, cap_c.y + 40), 255, "classic is a solid box");
        assert_eq!(ca(cap_c.x, cap_c.y + cap_c.h - 1), 0, "the outer bottom corner is rounded");
        assert!(ca(cap_c.x + 14, cap_c.y + cap_c.h - 1) > 200, "but the box reaches the bottom edge");
        assert!(ca(cap_c.x, cap_c.y) > 200, "the screen-side corners are square");
        let _ = &mut classic;
    }

    #[test]
    fn a_frame_has_a_black_capsule_a_white_sphere_and_transparent_corners() {
        let (win, cap) = layout(WORK, 1.0, true, 0.5, 301.0, 105.0);
        let mut buf = vec![0u32; (win.w * win.h) as usize];
        let mut s = show_for(2_000);
        s.style = Style::Classic;
        let pts = sphere_points(dot_count());
        render_frame(&mut buf, win.w, cap, 1.0, true, 800, &s, &pts);
        let px = |x: i32, y: i32| buf[(y * win.w + x) as usize];
        let alpha = |v: u32| v >> 24;
        // inside the capsule, away from the sphere: opaque black
        assert_eq!(alpha(px(cap.x + 12, cap.y + 6)), 255);
        assert_eq!(px(cap.x + 12, cap.y + 6) & 0x00FF_FFFF, 0);
        // the bottom-left corner is rounded away: still transparent
        assert_eq!(alpha(px(cap.x, cap.y + cap.h - 1)), 0);
        // the top corners are square (the capsule hugs the screen edge)
        assert!(alpha(px(cap.x, cap.y)) > 200);
        // outside the capsule, in the caption area: nothing yet (the glue draws the pill)
        assert_eq!(alpha(px(2, win.h - 3)), 0);
        // the sphere: some bright pixel near the centre
        let (cx, cy) = (cap.x + cap.w / 2, cap.y + cap.h / 2);
        let bright = (-30..30).flat_map(|dy| (-30..30).map(move |dx| (dx, dy))).filter(|(dx, dy)| px(cx + dx, cy + dy) & 255 > 200).count();
        assert!(bright > 40, "the white sphere should light up pixels, found {bright}");
    }

    #[test]
    fn nothing_is_drawn_while_invisible_and_text_composites_where_asked() {
        let (win, cap) = layout(WORK, 1.0, true, 0.5, 301.0, 105.0);
        let mut buf = vec![0u32; (win.w * win.h) as usize];
        render_frame(&mut buf, win.w, cap, 1.0, true, 0, &show_for(2_000), &sphere_points(50));
        assert!(buf.iter().all(|p| p >> 24 == 0), "alpha is zero at t=0");
        let mask = vec![255u8; 4 * 3];
        composite_mask(&mut buf, win.w, &mask, 4, 3, 10, 20, 1.0);
        assert_eq!(buf[(20 * win.w + 10) as usize], 0xFFFF_FFFF);
        assert_eq!(buf[(22 * win.w + 13) as usize], 0xFFFF_FFFF);
        assert_eq!(buf[(23 * win.w + 10) as usize], 0, "outside the mask");
        // off-buffer positions do not panic
        composite_mask(&mut buf, win.w, &mask, 4, 3, -2, -1, 1.0);
        composite_mask(&mut buf, win.w, &mask, 4, 3, win.w - 1, win.h - 1, 1.0);
    }

    #[test]
    fn the_caption_pill_hangs_below_at_the_top_and_rises_above_at_the_bottom() {
        let top = pill_rect(260.0, 117.0, 200, 24, 1.0, true);
        assert_eq!((top.y, top.w, top.h), (117, 236, 40));
        assert_eq!(top.x, 260 - 118);
        let bottom = pill_rect(260.0, 40.0, 200, 24, 1.0, false);
        assert_eq!(bottom.y + bottom.h, 40);
    }

    /// Run by hand on a signed-in desktop: shows the overlay for about 5 s (screenshot it while it runs).
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "shows a window on the real desktop"]
    fn the_overlay_appears_on_this_desktop() {
        show(show_for(2_300));
        std::thread::sleep(std::time::Duration::from_millis(5_500));
    }

    /// Run by hand: writes frames of the overlay (on a grey desktop) as BMP files for a look.
    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "writes preview images"]
    fn write_preview_images() {
        let dir = std::env::var("NEXUS_PREVIEW_DIR").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
        let mut classic = show_for(2_000);
        classic.style = Style::Classic;
        classic.capsule_w = 200.0;
        classic.capsule_h = 200.0;
        let mut bottom = show_for(2_000);
        bottom.top = false;
        for (name, t, show) in [
            ("a_start", 120u64, show_for(2_000)),
            ("b_mid", 700, show_for(2_000)),
            ("c_end", 1_800, show_for(2_000)),
            ("d_fade", 3_000, show_for(2_000)),
            ("e_classic", 1_800, classic),
            ("f_pebble_bottom", 1_800, bottom),
        ] {
            let (px, w, h) = win::preview(show, t).expect("preview");
            // composite premultiplied pixels over a grey desktop and write a 24-bit bottom-up BMP
            let row = ((w * 3 + 3) / 4 * 4) as usize;
            let mut bmp = vec![0u8; 54 + row * h as usize];
            bmp[0..2].copy_from_slice(b"BM");
            bmp[2..6].copy_from_slice(&((54 + row * h as usize) as u32).to_le_bytes());
            bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
            bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
            bmp[18..22].copy_from_slice(&w.to_le_bytes());
            bmp[22..26].copy_from_slice(&h.to_le_bytes());
            bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
            bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
            for y in 0..h {
                for x in 0..w {
                    let p = px[(y * w + x) as usize];
                    let a = (p >> 24) as f32 / 255.0;
                    let bg = 110.0;
                    let ch = |v: u32| ((v & 255) as f32 + bg * (1.0 - a)).clamp(0.0, 255.0) as u8;
                    let o = 54 + (h - 1 - y) as usize * row + x as usize * 3;
                    bmp[o] = ch(p);
                    bmp[o + 1] = ch(p >> 8);
                    bmp[o + 2] = ch(p >> 16);
                }
            }
            std::fs::write(std::path::Path::new(&dir).join(format!("early_orb_{name}.bmp")), bmp).unwrap();
        }
    }
}
