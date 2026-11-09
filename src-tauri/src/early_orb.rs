//! A small native orb + caption shown while the early hello is spoken (doc 158, slice 2).
//!
//! The real orb lives in the `stage` WebView, which does not exist until 5-25 s after logon. So while the cached
//! greeting plays, this draws the same picture natively: the black capsule at the top of the screen with the NEW dotted
//! orb (the idle circle morphing into the speaking design, baked from the real engine by `scripts/bake-early-orb.mjs`),
//! and at the stage slot (screen x=51.5%, top=89.5%) the black caption pill that types the words as they are spoken. It uses two layered windows (per-pixel alpha, click-through, never
//! takes focus), shown for the length of the clip plus a short linger, then they fade away and the windows are destroyed.
//!
//! Everything about the picture is pure and tested (geometry, fades, the typed words, the baked orb, the compositing);
//! only the windows and the text rasteriser are Windows glue at the bottom.

/// Logical (96-dpi) sizes. Fixed capsule sizes for top docking:
/// 301x105 for pebble scoop, 200x200 for classic.
pub const DEFAULT_CAPSULE_W: f32 = 301.0;
pub const DEFAULT_CAPSULE_H: f32 = 105.0;
pub const CLASSIC_CAPSULE_W: f32 = 200.0;
pub const CLASSIC_CAPSULE_H: f32 = 200.0;

/// Stage slot constants matching frontend/src/stage/layoutSlots.ts and ghost.css.
pub const CAPTION_CENTER_X: f32 = 0.515;
pub const CAPTION_TOP: f32 = 0.895;
pub const CAPTION_PILL_MAX_W: f32 = 620.0;
pub const CAPTION_PILL_MIN_H: f32 = 30.0;
pub const SHADOW_MARGIN: f32 = 24.0;
pub const PILL_CORNER: f32 = 10.0;

const FADE_IN_MS: u64 = 160;
const ORB_LINGER_MS: u64 = 900;
const CAPTION_LINGER_MS: u64 = 1_500;
const FADE_OUT_MS: u64 = 320;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The capsule dimensions (logical px) for each style.
pub fn capsule_size(style: Style) -> (f32, f32) {
    match style {
        Style::Pebble => (DEFAULT_CAPSULE_W, DEFAULT_CAPSULE_H),
        Style::Classic => (CLASSIC_CAPSULE_W, CLASSIC_CAPSULE_H),
    }
}

/// Where the capsule window goes: docked at the top centre of the primary monitor (y = 0), sized exactly to the capsule.
/// Returns (window rect, capsule rect inside the window). Pure.
pub fn layout(screen_w: i32, scale: f32, capsule_w: f32, capsule_h: f32) -> (Rect, Rect) {
    let cap_w = (capsule_w * scale).round() as i32;
    let cap_h = (capsule_h * scale).round() as i32;
    let x = (screen_w - cap_w) / 2;
    let y = 0;
    (Rect { x, y, w: cap_w, h: cap_h }, Rect { x: 0, y: 0, w: cap_w, h: cap_h })
}

/// Where the caption window goes: centred horizontally at screen_w * 0.515, with the pill's top edge
/// at screen_h * 0.895, padded by SHADOW_MARGIN * scale for soft drop shadows. Pure.
pub fn caption_slot(screen_w: i32, screen_h: i32, scale: f32) -> Rect {
    let win_w = ((CAPTION_PILL_MAX_W + 2.0 * SHADOW_MARGIN) * scale).round() as i32;
    let win_h = ((CAPTION_PILL_MIN_H + 2.0 * SHADOW_MARGIN) * scale).round() as i32;
    let center_x = screen_w as f32 * CAPTION_CENTER_X;
    let win_x = (center_x - win_w as f32 / 2.0).round() as i32;
    let pill_top = screen_h as f32 * CAPTION_TOP;
    let win_y = (pill_top - SHADOW_MARGIN * scale).round() as i32;
    Rect { x: win_x, y: win_y, w: win_w, h: win_h }
}

/// The pill rectangle inside the caption window for measured text dimensions.
/// Centred horizontally in the window, top at SHADOW_MARGIN * scale. Pure.
pub fn pill_in_window(win_w: i32, text_w: i32, text_h: i32, scale: f32) -> Rect {
    let max_w = (CAPTION_PILL_MAX_W * scale).round() as i32;
    let w = ((text_w as f32 + 48.0 * scale + 2.0).round() as i32).min(max_w);
    let min_h = (CAPTION_PILL_MIN_H * scale).round() as i32;
    let h = min_h.max((text_h as f32 + 4.0 * scale + 2.0).round() as i32);
    let x = (win_w - w) / 2;
    let y = (SHADOW_MARGIN * scale).round() as i32;
    Rect { x, y, w, h }
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

/// The caption typed so far at `t_ms`: ONE steady typing rate across the whole line (first word start to last word end),
/// 1 character as soon as the line starts and all of it when it ends. Same maths as `typedText` in
/// `frontend/src/audio/typedCaption.ts`, which the stage uses. `words` = (text, start ms, ms). With no word timings the
/// whole line is shown. The result never ends in a space.
pub fn visible_prefix(words: &[(String, u64, u64)], full: &str, t_ms: u64) -> String {
    if words.is_empty() {
        return full.to_string();
    }
    let line: String = words.iter().map(|w| w.0.as_str()).collect::<Vec<_>>().join(" ");
    let total = line.chars().count();
    if total == 0 {
        return String::new();
    }
    let start = words[0].1;
    let last = &words[words.len() - 1];
    let end = (last.1 + last.2.max(1)).max(start + 1);
    let shown = typed_length(total, start, end, t_ms);
    line.chars().take(shown).collect::<String>().trim_end().to_string()
}

/// How many of `total` characters are typed at `pos_ms` (the stage's `typedLength`).
pub fn typed_length(total: usize, start_ms: u64, end_ms: u64, pos_ms: u64) -> usize {
    if total == 0 {
        return 0;
    }
    if total == 1 {
        return 1;
    }
    let span = end_ms.saturating_sub(start_ms).max(1) as f64;
    let p = ((pos_ms as f64 - start_ms as f64) / span).clamp(0.0, 1.0);
    (1 + ((total - 1) as f64 * p).floor() as usize).clamp(1, total)
}

/// One dot of the baked orb: position and radius as fractions of the orb's square (0..1 across), the ink value
/// (`white`: 0 = full ink, as in the engine) and the alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakedDot {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub white: f32,
    pub a: f32,
}

/// The frames of the new dotted orb (idle circle -> speaking design) baked from the real engine, see
/// `scripts/bake-early-orb.mjs` (layout described there).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BakedOrb {
    pub fps: u32,
    pub frames: Vec<Vec<BakedDot>>,
}

const BAKED: &[u8] = include_bytes!("../resources/early_orb_frames.bin");
/// The last part of the bake is a steady speaking loop; a clip longer than the bake repeats it.
const LOOP_TAIL_FRAMES: usize = 60;

impl BakedOrb {
    /// The orb baked into the program. An unreadable file draws no orb (the capsule and the caption still show).
    pub fn load() -> BakedOrb {
        BakedOrb::parse(BAKED).unwrap_or_default()
    }

    pub fn parse(b: &[u8]) -> Result<BakedOrb, String> {
        if b.len() < 10 || &b[0..4] != b"EOB1" {
            return Err("not an early orb file".into());
        }
        let u16at = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]) as usize;
        let (fps, count) = (u16at(4), u16at(6));
        let mut o = 10usize;
        let mut frames = Vec::with_capacity(count);
        for _ in 0..count {
            if o + 2 > b.len() {
                return Err("truncated".into());
            }
            let n = u16at(o);
            o += 2;
            if o + n * 7 > b.len() {
                return Err("truncated".into());
            }
            let mut dots = Vec::with_capacity(n);
            for _ in 0..n {
                dots.push(BakedDot {
                    x: u16at(o) as f32 / 32767.0 - 0.25,
                    y: u16at(o + 2) as f32 / 32767.0 - 0.25,
                    r: b[o + 4] as f32 / 1000.0,
                    white: b[o + 5] as f32 / 255.0,
                    a: b[o + 6] as f32 / 255.0,
                });
                o += 7;
            }
            frames.push(dots);
        }
        Ok(BakedOrb { fps: fps.max(1) as u32, frames })
    }

    /// The dots to draw `t_ms` after the speech started.
    pub fn frame_at(&self, t_ms: u64) -> &[BakedDot] {
        if self.frames.is_empty() {
            return &[];
        }
        let mut i = (t_ms * self.fps as u64 / 1000) as usize;
        let n = self.frames.len();
        if i >= n {
            let tail = LOOP_TAIL_FRAMES.min(n);
            i = n - tail + (i - n) % tail;
        }
        &self.frames[i]
    }
}

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

/// A solid black rounded shape with an optional faint white border ring (the capsule and caption pill look alike).
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
            if border_alpha > 0.0 {
                let ring = (1.0 - (-d - 0.5).abs().min(1.0)) * border_alpha * alpha * cov;
                if ring > 0.0 {
                    over(buf, idx, ring, ring, ring, ring);
                }
            }
        }
    }
}

/// The dotted orb: `side` px across, centred at (cx, cy), white ink on the black capsule. `amp` (0..1, the voice) swells every dot
/// a little, as the stage does (`LEVEL_RADIUS_GAIN`). `alpha` is the fade of the whole orb.
pub fn draw_baked(buf: &mut [u32], width: i32, cx: f32, cy: f32, side: f32, amp: f32, alpha: f32, dots: &[BakedDot]) {
    if alpha <= 0.0 {
        return;
    }
    let height = buf.len() as i32 / width.max(1);
    let swell = 1.0 + 0.32 * amp.clamp(0.0, 1.0);
    let (left, top) = (cx - side / 2.0, cy - side / 2.0);
    for d in dots {
        let a = d.a * alpha;
        if a < 0.02 {
            continue;
        }
        let ink = 1.0 - d.white.clamp(0.0, 1.0);
        let (dx, dy, dot_r) = (left + d.x * side, top + d.y * side, d.r * side * swell);
        let (bx0, bx1) = ((dx - dot_r - 1.0).floor() as i32, (dx + dot_r + 1.0).ceil() as i32);
        let (by0, by1) = ((dy - dot_r - 1.0).floor() as i32, (dy + dot_r + 1.0).ceil() as i32);
        for y in by0.max(0)..by1.min(height) {
            for x in bx0.max(0)..bx1.min(width) {
                let dist = ((x as f32 + 0.5 - dx).powi(2) + (y as f32 + 0.5 - dy).powi(2)).sqrt();
                let cov = (dot_r + 0.5 - dist).clamp(0.0, 1.0) * a;
                if cov > 0.0 {
                    over(buf, (y * width + x) as usize, cov, cov * ink, cov * ink, cov * ink);
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

/// The background shape chosen for the orb (`orbCapsuleStyle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The fluid bezel scoop: a catenary bowl hanging from the top screen edge (default).
    Pebble,
    /// The plain black capsule: a 200x200 square box with the bottom two corners rounded by 10 px.
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

/// The pebble: a black bowl hanging from the top screen edge, with a faint rim line. Pure.
pub fn fill_pebble(buf: &mut [u32], width: i32, r: Rect, profile: &[f32], alpha: f32, rim_alpha: f32) {
    if alpha <= 0.0 {
        return;
    }
    let height = buf.len() as i32 / width.max(1);
    for y in r.y.max(0)..(r.y + r.h).min(height) {
        for x in r.x.max(0)..(r.x + r.w).min(width) {
            let u = (x - r.x) as usize;
            let Some(&depth) = profile.get(u) else { continue };
            let slope = (profile.get(u + 1).copied().unwrap_or(depth) - profile.get(u.saturating_sub(1)).copied().unwrap_or(depth)) / 2.0;
            let y_rel = (y - r.y) as f32 + 0.5;
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

/// Draw one frame of the capsule window: the capsule background and the dotted orb. Pure.
pub fn render_frame(buf: &mut [u32], width: i32, capsule: Rect, scale: f32, t_ms: u64, s: &Show, orb: &BakedOrb) {
    for p in buf.iter_mut() {
        *p = 0;
    }
    let (orb_a, _, _) = alphas(t_ms, s.clip_ms);
    let amp = envelope_at(&s.env, s.env_frame_ms, t_ms);
    let (cx, mid_y) = (capsule.x as f32 + capsule.w as f32 / 2.0, capsule.y as f32 + capsule.h as f32 / 2.0);
    match s.style {
        Style::Pebble => {
            let wing = (120.0 * scale).min(capsule.w as f32 * 0.25);
            let profile = bowl_profile(capsule.w as f32, capsule.h as f32, wing);
            fill_pebble(buf, width, capsule, &profile, orb_a, s.rim_alpha);
        }
        Style::Classic => {
            // the plain capsule: the top screen-side corners are square, the bottom two are rounded by 10 px
            let corner = PILL_CORNER * scale;
            fill_black_shape(buf, width, capsule, 0.0, corner, orb_a, 0.16);
        }
    }
    // The stage measures the orb's cradle and draws the dotted orb as a square of its shorter side, centred.
    let side = capsule.w.min(capsule.h) as f32;
    draw_baked(buf, width, cx, mid_y, side, amp, orb_a, orb.frame_at(t_ms));
}

/// Draw the caption pill with soft black shadows and composited text into the caption window buffer. Pure.
pub fn render_caption(
    buf: &mut [u32],
    width: i32,
    pill: Rect,
    scale: f32,
    alpha: f32,
    mask: &[u8],
    mw: i32,
    mh: i32,
    tw: i32,
    th: i32,
) {
    for p in buf.iter_mut() {
        *p = 0;
    }
    if alpha <= 0.0 {
        return;
    }
    let corner = pill_corner(scale);
    // The pill shadow: draw 3 slightly larger translucent black rounded rects behind the pill
    // expanded by 22, 14 and 6 logical px with alpha 0.07, 0.14 and 0.25 scaled by the caption alpha.
    for (exp_logical, a_mult) in [(22.0, 0.07), (14.0, 0.14), (6.0, 0.25)] {
        let exp = (exp_logical * scale).round() as i32;
        let r = Rect {
            x: pill.x - exp,
            y: pill.y - exp,
            w: pill.w + 2 * exp,
            h: pill.h + 2 * exp,
        };
        let rad = corner + exp as f32;
        fill_black_shape(buf, width, r, rad, rad, alpha * a_mult, 0.0);
    }

    // The solid black pill with 1 px white border (16% opacity)
    fill_black_shape(buf, width, pill, corner, corner, alpha, 0.16);

    // Composite the text inside the pill, centered horizontally and vertically
    if !mask.is_empty() && tw > 0 && th > 0 {
        let (cw, ch) = (tw.min(mw).max(0), th.min(mh).max(0));
        let mut cropped = Vec::with_capacity((cw * ch) as usize);
        for y in 0..ch {
            let start = (y * mw) as usize;
            let end = start + cw as usize;
            if end <= mask.len() {
                cropped.extend_from_slice(&mask[start..end]);
            }
        }
        let text_x = pill.x + (pill.w - cw) / 2;
        let text_y = pill.y + (pill.h - ch) / 2;
        composite_mask(buf, width, &cropped, cw, ch, text_x, text_y, alpha);
    }
}

pub fn pill_corner(scale: f32) -> f32 {
    PILL_CORNER * scale
}

pub fn caption_max_width(scale: f32) -> i32 {
    (CAPTION_PILL_MAX_W * scale).round() as i32
}

pub fn caption_height(scale: f32) -> i32 {
    (44.0 * scale).round() as i32
}

// ─── the windows (Windows only) ──────────────────────────────────────

/// Show the overlay for the length of the clip. Returns at once; the windows live on their own thread. Best effort: any
/// failure is logged and the greeting is simply heard without a picture.
#[cfg(target_os = "windows")]
pub fn show(s: Show) {
    FIRST_PAINT.store(false, std::sync::atomic::Ordering::SeqCst);
    let _ = std::thread::Builder::new().name("early-orb".into()).spawn(move || {
        if let Err(e) = win::run(s) {
            println!("[EARLY-ORB] not shown: {e}");
        }
    });
    // The sound must never start without its picture: wait (bounded) until the first frame is really on the screen.
    let t = std::time::Instant::now();
    while !FIRST_PAINT.load(std::sync::atomic::Ordering::SeqCst) && t.elapsed().as_millis() < 1500 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    println!("[EARLY-ORB] first frame {} after {} ms", if FIRST_PAINT.load(std::sync::atomic::Ordering::SeqCst) { "painted" } else { "NOT painted" }, t.elapsed().as_millis());
}

/// Set by the window thread after the first successful `UpdateLayeredWindow`.
#[cfg(target_os = "windows")]
static FIRST_PAINT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
        fn CreateWindowExW(
            ex: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            parent: Hwnd,
            menu: isize,
            inst: isize,
            param: *const c_void,
        ) -> Hwnd;
        fn DefWindowProcW(h: Hwnd, m: u32, w: usize, l: isize) -> isize;
        fn ShowWindow(h: Hwnd, cmd: i32) -> i32;
        fn SetTimer(h: Hwnd, id: usize, ms: u32, f: *const c_void) -> usize;
        fn GetMessageW(m: *mut Msg, h: Hwnd, a: u32, b: u32) -> i32;
        fn TranslateMessage(m: *const Msg) -> i32;
        fn DispatchMessageW(m: *const Msg) -> isize;
        fn PostQuitMessage(code: i32);
        fn DestroyWindow(h: Hwnd) -> i32;
        fn UpdateLayeredWindow(
            h: Hwnd,
            dst: isize,
            pos: *const Point,
            size: *const Size,
            src: isize,
            src_pos: *const Point,
            key: u32,
            blend: *const BlendFunction,
            flags: u32,
        ) -> i32;
        fn GetSystemMetrics(i: i32) -> i32;
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
        fn CreateFontW(
            h: i32,
            w: i32,
            esc: i32,
            orient: i32,
            weight: i32,
            italic: u32,
            underline: u32,
            strike: u32,
            charset: u32,
            out: u32,
            clip: u32,
            quality: u32,
            pitch: u32,
            face: *const u16,
        ) -> isize;
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
        capsule_win: Rect,
        caption_win: Rect,
        capsule_rect: Rect,
        scale: f32,
        started: std::time::Instant,
        capsule_canvas: Dib,
        caption_canvas: Dib,
        text: Dib,
        font: isize,
        orb: std::sync::Arc<BakedOrb>,
        hwnd_capsule: Hwnd,
        hwnd_caption: Hwnd,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Rasterise `text` into the mask bitmap with GDI and return (coverage mask, width, height, measured width, measured height).
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

    /// Draw both windows for `t_ms` into their canvases. `true` when the show is over.
    fn compose(st: &mut State, t_ms: u64) -> bool {
        let (_, cap_a, done) = alphas(t_ms, st.show.clip_ms);

        // 1. Render capsule window (capsule + dotted orb)
        {
            let show = st.show.clone();
            let capsule_rect = st.capsule_rect;
            let scale = st.scale;
            let orb = st.orb.clone();
            render_frame(st.capsule_canvas.pixels(), st.capsule_win.w, capsule_rect, scale, t_ms, &show, &orb);
        }

        // 2. Render caption window (pill + shadows + text)
        let shown = visible_prefix(&st.show.words, &st.show.text, t_ms);
        if cap_a > 0.0 && !shown.is_empty() {
            let (mask, mw, mh, tw, th) = text_mask(st, &shown);
            let pill = pill_in_window(st.caption_win.w, tw, th, st.scale);
            render_caption(st.caption_canvas.pixels(), st.caption_win.w, pill, st.scale, cap_a, &mask, mw, mh, tw, th);
        } else {
            for p in st.caption_canvas.pixels().iter_mut() {
                *p = 0;
            }
        }

        done
    }

    fn draw(st: &mut State) -> bool {
        let done = compose(st, st.started.elapsed().as_millis() as u64);
        // SAFETY: both canvas DCs hold 32-bit premultiplied bitmaps matching their window sizes.
        unsafe {
            let src = Point::default();
            let blend = BlendFunction { op: 0, flags: 0, alpha: 255, format: 1 };

            // Update capsule window
            let pos_cap = Point { x: st.capsule_win.x, y: st.capsule_win.y };
            let size_cap = Size { cx: st.capsule_win.w, cy: st.capsule_win.h };
            let ok = UpdateLayeredWindow(st.hwnd_capsule, 0, &pos_cap, &size_cap, st.capsule_canvas.dc, &src, 0, &blend, 2);
            if ok != 0 {
                FIRST_PAINT.store(true, std::sync::atomic::Ordering::SeqCst);
            }

            // Update caption window
            let pos_txt = Point { x: st.caption_win.x, y: st.caption_win.y };
            let size_txt = Size { cx: st.caption_win.w, cy: st.caption_win.h };
            UpdateLayeredWindow(st.hwnd_caption, 0, &pos_txt, &size_txt, st.caption_canvas.dc, &src, 0, &blend, 2);
        }
        done
    }

    unsafe extern "system" fn proc_(hwnd: Hwnd, msg: u32, wp: usize, lp: isize) -> isize {
        match msg {
            WM_TIMER => {
                let (done, h_cap, h_txt) = STATE.with(|s| {
                    let mut b = s.borrow_mut();
                    if let Some(st) = b.as_mut() {
                        let done = draw(st);
                        (done, st.hwnd_capsule, st.hwnd_caption)
                    } else {
                        (true, 0, 0)
                    }
                });
                if done {
                    if h_cap != 0 {
                        DestroyWindow(h_cap);
                    }
                    if h_txt != 0 {
                        DestroyWindow(h_txt);
                    }
                }
                0
            }
            WM_DESTROY => {
                let is_first = STATE.with(|s| s.borrow().as_ref().map_or(false, |st| st.hwnd_capsule == hwnd));
                if is_first {
                    PostQuitMessage(0);
                }
                0
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }

    /// Everything except the windows: layout, bitmaps, font and the baked orb.
    unsafe fn build_state(s: Show) -> Result<State, String> {
        // draw in real pixels whatever the DPI of the process is this early in startup
        SetThreadDpiAwarenessContext(-4);
        let scale = GetDpiForSystem().max(96) as f32 / 96.0;
        let screen_w = GetSystemMetrics(0);
        let screen_h = GetSystemMetrics(1);
        let (cap_w, cap_h) = capsule_size(s.style);
        let (capsule_win, capsule_rect) = layout(screen_w, scale, cap_w, cap_h);
        let caption_win = caption_slot(screen_w, screen_h, scale);

        let capsule_canvas = Dib::new(capsule_win.w, capsule_win.h).ok_or("no capsule canvas")?;
        let caption_canvas = Dib::new(caption_win.w, caption_win.h).ok_or("no caption canvas")?;
        let text = Dib::new(caption_max_width(scale), caption_height(scale)).ok_or("no text bitmap")?;
        let face = wide("Segoe UI");
        let font = CreateFontW(-((16.5 * scale).round() as i32), 0, 0, 0, 600, 0, 0, 0, 1, 0, 0, 4, 0, face.as_ptr());
        if font == 0 {
            return Err("no font".into());
        }
        Ok(State {
            show: s,
            capsule_win,
            caption_win,
            capsule_rect,
            scale,
            started: std::time::Instant::now(),
            capsule_canvas,
            caption_canvas,
            text,
            font,
            orb: std::sync::Arc::new(BakedOrb::load()),
            hwnd_capsule: 0,
            hwnd_caption: 0,
        })
    }

    pub fn run(s: Show) -> Result<(), String> {
        // SAFETY: user32/gdi32 setup for two layered windows on this thread; all handles released before returning.
        unsafe {
            let mut state = build_state(s)?;
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
            let hwnd_capsule = CreateWindowExW(
                ex,
                class.as_ptr(),
                class.as_ptr(),
                0x8000_0000,
                state.capsule_win.x,
                state.capsule_win.y,
                state.capsule_win.w,
                state.capsule_win.h,
                0,
                0,
                inst,
                std::ptr::null(),
            );
            if hwnd_capsule == 0 {
                DeleteObject(state.font);
                return Err("CreateWindowExW capsule failed".into());
            }

            let hwnd_caption = CreateWindowExW(
                ex,
                class.as_ptr(),
                class.as_ptr(),
                0x8000_0000,
                state.caption_win.x,
                state.caption_win.y,
                state.caption_win.w,
                state.caption_win.h,
                0,
                0,
                inst,
                std::ptr::null(),
            );
            if hwnd_caption == 0 {
                DestroyWindow(hwnd_capsule);
                DeleteObject(state.font);
                return Err("CreateWindowExW caption failed".into());
            }

            state.hwnd_capsule = hwnd_capsule;
            state.hwnd_caption = hwnd_caption;

            STATE.with(|st| *st.borrow_mut() = Some(State { started: std::time::Instant::now(), ..state }));

            ShowWindow(hwnd_capsule, 4); // SW_SHOWNOACTIVATE
            ShowWindow(hwnd_caption, 4); // SW_SHOWNOACTIVATE
            SetTimer(hwnd_capsule, TIMER_ID, 16, std::ptr::null());

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

    /// Test helper: the composed canvases at `t_ms` as ((capsule pixels, w, h), (caption pixels, w, h)).
    #[cfg(test)]
    pub fn preview(s: Show, t_ms: u64) -> Option<((Vec<u32>, i32, i32), (Vec<u32>, i32, i32))> {
        // SAFETY: as `run`; the state is dropped (font deleted) before returning.
        unsafe {
            let mut st = build_state(s).ok()?;
            compose(&mut st, t_ms);
            let cap = (st.capsule_canvas.pixels().to_vec(), st.capsule_win.w, st.capsule_win.h);
            let txt = (st.caption_canvas.pixels().to_vec(), st.caption_win.w, st.caption_win.h);
            DeleteObject(st.font);
            Some((cap, txt))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn show_for(clip_ms: u64) -> Show {
        Show {
            style: Style::Pebble,
            rim_alpha: 0.25,
            text: "Good evening, sir".into(),
            words: vec![("Good".into(), 0, 300), ("evening,".into(), 300, 600), ("sir".into(), 900, 400)],
            env: vec![0.2, 0.9, 0.5],
            env_frame_ms: 20,
            clip_ms,
        }
    }

    #[test]
    fn caption_slot_and_pill_geometry() {
        // At scale 1.0 on a 1920x1080 screen:
        // window width = (620 + 2*24) * 1.0 = 668, height = (30 + 2*24) * 1.0 = 78
        // center_x = 1920 * 0.515 = 988.8 -> win_x = (988.8 - 334.0) = 655
        // pill top = 1080 * 0.895 = 966.6 -> win_y = (966.6 - 24.0) = 943
        let slot = caption_slot(1920, 1080, 1.0);
        assert_eq!(slot.w, 668);
        assert_eq!(slot.h, 78);
        assert_eq!(slot.x, 655);
        assert_eq!(slot.y, 943);

        // At scale 1.5 on a 1920x1080 screen:
        // win_w = (668 * 1.5) = 1002, win_h = (78 * 1.5) = 117
        // win_x = (988.8 - 501.0) = 488, win_y = (966.6 - 36.0) = 931
        let slot_hi = caption_slot(1920, 1080, 1.5);
        assert_eq!(slot_hi.w, 1002);
        assert_eq!(slot_hi.h, 117);
        assert_eq!(slot_hi.x, 488);
        assert_eq!(slot_hi.y, 931);

        // pill_in_window inside slot:
        // measured text 200x24:
        // pill width = 200 + 48 + 2 = 250
        // height = max(30, 24 + 4 + 2) = 30
        // x centered = (668 - 250) / 2 = 209
        // top = 24
        let pill = pill_in_window(slot.w, 200, 24, 1.0);
        assert_eq!(pill, Rect { x: 209, y: 24, w: 250, h: 30 });

        // Taller text exceeds 30px min-height:
        // measured text 200x40 -> height = max(30, 40 + 4 + 2) = 46
        let tall_pill = pill_in_window(slot.w, 200, 40, 1.0);
        assert_eq!(tall_pill.h, 46);

        // Max width clamp at 620:
        let wide_pill = pill_in_window(slot.w, 600, 24, 1.0);
        assert_eq!(wide_pill.w, 620);
    }

    #[test]
    fn drift_check_caption_slot_constants_match_frontend() {
        let ts_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/src/stage/layoutSlots.ts");
        let ts = std::fs::read_to_string(ts_path).expect("read layoutSlots.ts");
        let line = ts.lines().find(|l| l.contains("export const CAPTION =")).expect("CAPTION line in layoutSlots.ts");

        let cx_str = line.split("centerX:").nth(1).unwrap().split(',').next().unwrap().trim();
        let cx: f32 = cx_str.parse().expect("parse centerX");
        assert_eq!(cx, CAPTION_CENTER_X);

        let top_str = line.split("top:").nth(1).unwrap().split(',').next().unwrap().trim();
        let top: f32 = top_str.parse().expect("parse top");
        assert_eq!(top, CAPTION_TOP);

        let css_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../frontend/src/stage/ghost.css");
        let css = std::fs::read_to_string(css_path).expect("read ghost.css");
        let capsule_pos = css.find(".caption-black-capsule").expect(".caption-black-capsule in ghost.css");
        let after_capsule = &css[capsule_pos..];
        let max_w_line = after_capsule.lines().take(20).find(|l| l.contains("max-width:")).expect("max-width in caption-black-capsule");
        assert!(max_w_line.contains("620px"), "ghost.css max-width should be 620px, got: {max_w_line}");
        assert_eq!(CAPTION_PILL_MAX_W, 620.0);
    }

    #[test]
    fn the_capsule_window_is_docked_at_top_centre() {
        let (win, cap) = layout(1920, 1.0, 301.0, 105.0);
        assert_eq!((win.y, cap.y), (0, 0), "top: flush with the screen edge y=0");
        assert_eq!(win.w, 301);
        assert_eq!(win.h, 105);
        assert_eq!(win.x, (1920 - 301) / 2, "centred horizontally");
        assert_eq!(cap, Rect { x: 0, y: 0, w: 301, h: 105 });

        let (hi, _) = layout(1920, 1.5, 301.0, 105.0);
        assert_eq!(hi.w, (301.0 * 1.5f32).round() as i32, "scaled by the display scale");
        assert_eq!(hi.h, (105.0 * 1.5f32).round() as i32);
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
    fn the_caption_types_at_one_steady_rate_like_the_stage() {
        let s = show_for(1_300);
        let total = "Good evening, sir".chars().count();
        let (start, end) = (s.words[0].1, s.words[s.words.len() - 1].1 + s.words[s.words.len() - 1].2);
        assert_eq!(visible_prefix(&s.words, &s.text, 0), "G", "the first letter appears as the line starts");
        assert_eq!(visible_prefix(&s.words, &s.text, 5_000), "Good evening, sir", "all of it by the end");
        assert_eq!(visible_prefix(&[], "Good evening, sir", 10), "Good evening, sir", "no timings: the whole line");
        // never behind a straight line from 1 character to all of them, never ahead of it by more than one
        let mut last = 0;
        for t in (start..=end).step_by(10) {
            let n = visible_prefix(&s.words, &s.text, t).chars().count();
            assert!(n >= last, "the caption only grows");
            let line = 1.0 + (total as f64 - 1.0) * ((t - start) as f64 / (end - start) as f64);
            assert!((n as f64 - line).abs() <= 2.0, "t={t}: {n} letters vs {line:.1}");
            last = n;
        }
        // the same numbers as typedLength in frontend/src/audio/typedCaption.ts
        assert_eq!(typed_length(10, 0, 900, 0), 1);
        assert_eq!(typed_length(10, 0, 900, 450), 5);
        assert_eq!(typed_length(10, 0, 900, 900), 10);
        assert_eq!(typed_length(10, 100, 900, 50), 1, "before the start: the first letter");
        assert_eq!(typed_length(1, 0, 900, 123), 1);
        assert_eq!(typed_length(0, 0, 900, 123), 0);
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
    fn the_baked_new_orb_loads_and_is_a_circle_that_morphs_into_the_speaking_design() {
        let orb = BakedOrb::load();
        assert_eq!(orb.fps, 30);
        assert_eq!(orb.frames.len(), 180, "6 s at 30 fps");
        let first = orb.frame_at(0);
        let later = orb.frame_at(3_000);
        assert!(first.len() >= 24 && first.len() < later.len(), "the idle circle has fewer dots than the speaking sash: {} vs {}", first.len(), later.len());
        for d in orb.frames.iter().flatten() {
            assert!((-0.25..=1.25).contains(&d.x) && (-0.25..=1.25).contains(&d.y), "dot off the square: {d:?}");
            assert!(d.r > 0.0 && d.r < 0.1 && (0.0..=1.0).contains(&d.a) && (0.0..=1.0).contains(&d.white));
        }
        // the circle: its dots sit about 0.36 of the square from the centre (frameCircle)
        let radii: Vec<f32> = first.iter().map(|d| ((d.x - 0.5).powi(2) + (d.y - 0.5).powi(2)).sqrt()).collect();
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        assert!((mean - 0.36).abs() < 0.04, "mean radius {mean}");
    }

    #[test]
    fn a_clip_longer_than_the_bake_repeats_its_speaking_loop_and_garbage_does_not_panic() {
        let orb = BakedOrb::load();
        assert_eq!(orb.frame_at(6_000).len(), orb.frames[120].len(), "7 s wraps to the start of the loop tail");
        assert_eq!(orb.frame_at(60_000_000).len() > 0, true);
        assert!(BakedOrb::parse(b"nope").is_err());
        assert!(BakedOrb::parse(b"EOB1\x1e\x00\x02\x00\x69\x00\x05\x00").is_err(), "truncated");
        assert!(BakedOrb::default().frame_at(100).is_empty());
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
        let (win, cap) = layout(1920, 1.0, 301.0, 105.0);
        let pts = BakedOrb::load();
        let alpha = |buf: &Vec<u32>, x: i32, y: i32| buf[(y * win.w + x) as usize] >> 24;

        // pebble: the bowl's upper corner area (near top-left) is empty sky beside the wing
        let mut pebble = vec![0u32; (win.w * win.h) as usize];
        render_frame(&mut pebble, win.w, cap, 1.0, 800, &show_for(2_000), &pts);
        assert_eq!(alpha(&pebble, cap.x + 20, cap.y + 40), 0, "below the wing, outside the bowl");
        assert!(alpha(&pebble, cap.x + cap.w / 2, cap.y + 90) > 200, "inside the bowl at the middle");

        // classic: a solid 200x200 box, so the same corner area IS black; the bottom corners are rounded by 10 px
        let mut s = show_for(2_000);
        s.style = Style::Classic;
        let (win_c, cap_c) = layout(1920, 1.0, 200.0, 200.0);
        let mut classic_buf = vec![0u32; (win_c.w * win_c.h) as usize];
        render_frame(&mut classic_buf, win_c.w, cap_c, 1.0, 800, &s, &pts);
        let ca = |x: i32, y: i32| classic_buf[(y * win_c.w + x) as usize] >> 24;
        assert_eq!(ca(cap_c.x + 20, cap_c.y + 40), 255, "classic is a solid box");
        assert_eq!(ca(cap_c.x, cap_c.y + cap_c.h - 1), 0, "the outer bottom corner is rounded");
        assert!(ca(cap_c.x + 14, cap_c.y + cap_c.h - 1) > 200, "but the box reaches the bottom edge");
        assert!(ca(cap_c.x, cap_c.y) > 200, "the screen-side top corners are square");
    }

    #[test]
    fn a_frame_has_a_black_capsule_a_white_dotted_orb_and_transparent_corners() {
        let (win, cap) = layout(1920, 1.0, 200.0, 200.0);
        let mut buf = vec![0u32; (win.w * win.h) as usize];
        let mut s = show_for(2_000);
        s.style = Style::Classic;
        let pts = BakedOrb::load();
        render_frame(&mut buf, win.w, cap, 1.0, 800, &s, &pts);
        let px = |x: i32, y: i32| buf[(y * win.w + x) as usize];
        let alpha = |v: u32| v >> 24;
        // inside the capsule, away from the orb: opaque black
        assert_eq!(alpha(px(cap.x + 12, cap.y + 6)), 255);
        assert_eq!(px(cap.x + 12, cap.y + 6) & 0x00FF_FFFF, 0);
        // the bottom-left corner is rounded away: still transparent
        assert_eq!(alpha(px(cap.x, cap.y + cap.h - 1)), 0);
        // the top corners are square (the capsule hugs the screen edge)
        assert!(alpha(px(cap.x, cap.y)) > 200);
        // the orb: some bright pixel near the centre
        let (cx, cy) = (cap.x + cap.w / 2, cap.y + cap.h / 2);
        let bright = (-80..80).flat_map(|dy| (-80..80).map(move |dx| (dx, dy))).filter(|(dx, dy)| px(cx + dx, cy + dy) & 255 > 200).count();
        assert!(bright > 40, "the white orb should light up pixels, found {bright}");
    }

    #[test]
    fn nothing_is_drawn_while_invisible_and_text_composites_where_asked() {
        let (win, cap) = layout(1920, 1.0, 301.0, 105.0);
        let mut buf = vec![0u32; (win.w * win.h) as usize];
        render_frame(&mut buf, win.w, cap, 1.0, 0, &show_for(2_000), &BakedOrb::load());
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
    fn render_caption_draws_pill_with_three_shadows_and_border() {
        let slot = caption_slot(1920, 1080, 1.0);
        let pill = pill_in_window(slot.w, 200, 24, 1.0);
        let mut buf = vec![0u32; (slot.w * slot.h) as usize];

        // 1. Invisible when alpha = 0
        render_caption(&mut buf, slot.w, pill, 1.0, 0.0, &[], 0, 0, 0, 0);
        assert!(buf.iter().all(|&p| p == 0));

        // 2. Visible with alpha = 1.0:
        render_caption(&mut buf, slot.w, pill, 1.0, 1.0, &[], 0, 0, 0, 0);
        let px = |x: i32, y: i32| buf[(y * slot.w + x) as usize];
        let alpha = |v: u32| v >> 24;

        // Inside pill center: solid black (alpha=255, RGB=0)
        let center_p = px(pill.x + pill.w / 2, pill.y + pill.h / 2);
        assert_eq!(alpha(center_p), 255);
        assert_eq!(center_p & 0x00FF_FFFF, 0);

        // Outside pill, within shadow expansion (e.g. 10 px outside left edge):
        // has shadow alpha > 0, RGB = 0 (black shadow)
        let shadow_p = px(pill.x - 10, pill.y + pill.h / 2);
        assert!(alpha(shadow_p) > 0, "shadow should be visible outside pill");
        assert_eq!(shadow_p & 0x00FF_FFFF, 0, "shadow is pure black");

        // Far outside (past 24px shadow margin): alpha is 0
        assert_eq!(alpha(px(1, 1)), 0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "shows a window on the real desktop"]
    fn the_overlay_appears_on_this_desktop() {
        show(show_for(2_300));
        std::thread::sleep(std::time::Duration::from_millis(5_500));
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "writes preview images"]
    fn write_preview_images() {
        let dir = std::env::var("NEXUS_PREVIEW_DIR").unwrap_or_else(|_| std::env::temp_dir().to_string_lossy().to_string());
        let mut classic = show_for(2_000);
        classic.style = Style::Classic;
        for (name, t, show) in [
            ("a_start", 120u64, show_for(2_000)),
            ("b_mid", 700, show_for(2_000)),
            ("c_end", 1_800, show_for(2_000)),
            ("d_fade", 3_000, show_for(2_000)),
            ("e_classic", 1_800, classic),
        ] {
            let ((cap_px, cap_w, cap_h), (txt_px, txt_w, txt_h)) = win::preview(show, t).expect("preview");
            let write_bmp = |filename: &str, px: &[u32], w: i32, h: i32| {
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
                std::fs::write(std::path::Path::new(&dir).join(filename), bmp).unwrap();
            };
            write_bmp(&format!("early_orb_{name}_capsule.bmp"), &cap_px, cap_w, cap_h);
            write_bmp(&format!("early_orb_{name}_caption.bmp"), &txt_px, txt_w, txt_h);
        }
    }

    /// At 125, 150 and 200 % display scaling the capsule stays at the top centre, the caption slot stays on the screen, and the
    /// dotted orb grows with the scale but never leaves the square the stage gives it (the capsule's shorter side).
    #[test]
    fn the_early_overlay_fits_at_125_150_and_200_percent_scaling() {
        let orb = BakedOrb::load();
        let mut widths = Vec::new();
        for scale in [1.0f32, 1.25, 1.5, 2.0] {
            let (sw, sh) = ((1920.0 * scale).round() as i32, (1080.0 * scale).round() as i32);
            let (win, cap) = layout(sw, scale, 301.0, 105.0);
            assert!(win.x >= 0 && win.x + win.w <= sw && win.y == 0, "capsule window on screen at {scale}: {win:?}");
            assert!((win.x * 2 + win.w - sw).abs() <= 1, "centred at {scale}");
            let slot = caption_slot(sw, sh, scale);
            assert!(slot.x >= 0 && slot.x + slot.w <= sw && slot.y >= 0 && slot.y + slot.h <= sh, "caption slot on screen at {scale}: {slot:?}");

            let mut buf = vec![0u32; (win.w * win.h) as usize];
            render_frame(&mut buf, win.w, cap, scale, 1_200, &show_for(2_000), &orb);
            let side = cap.w.min(cap.h) as f32;
            let (mut x0, mut x1, mut y0, mut y1) = (i32::MAX, i32::MIN, i32::MAX, i32::MIN);
            for y in 0..win.h {
                for x in 0..win.w {
                    let p = buf[(y * win.w + x) as usize];
                    if p & 255 > 110 && (p >> 24) > 200 {
                        x0 = x0.min(x);
                        x1 = x1.max(x);
                        y0 = y0.min(y);
                        y1 = y1.max(y);
                    }
                }
            }
            assert!(x1 > x0 && y1 > y0, "the orb is drawn at {scale}");
            let (bw, bh) = ((x1 - x0) as f32, (y1 - y0) as f32);
            assert!(bw <= side * 1.05 && bh <= side * 1.05, "orb {bw}x{bh} inside its {side}px square at {scale}");
            assert!(bw >= side * 0.4, "and it fills a good part of it ({bw} of {side}) at {scale}");
            let cx = (x0 + x1) as f32 / 2.0;
            assert!((cx - (cap.x as f32 + cap.w as f32 / 2.0)).abs() < side * 0.1, "centred in the capsule at {scale}");
            widths.push(bw);
        }
        assert!(widths.windows(2).all(|w| w[1] > w[0] * 1.1), "the orb grows with the scale: {widths:?}");
    }
}
