// NEXUS painter for the vendored thinking-orbs engine (the engine itself is unmodified upstream code,
// see LICENSE-thinking-orbs.txt). Differences from upstream `paintFrame`:
//   - ink is a colour (white by default, the user's orb colour) instead of pure grayscale on dark;
//   - an optional voice `level` (0..1) swells the dot radius a little;
//   - an optional `alpha` so two designs can be cross-faded on one canvas.
// Light substrates keep upstream behaviour (dark grayscale ink), which is the "black ink in light theme" rule.

import type { Dot, Line, OrbFrame } from "./engine/core";

export type Ink = readonly [number, number, number];

export const WHITE_INK: Ink = [255, 255, 255];

/** Radius gain at full voice level (applied to every dot). */
export const LEVEL_RADIUS_GAIN = 0.32;

function clamp01(v: number): number {
  return v < 0 ? 0 : v > 1 ? 1 : v;
}

/** CSS colour for one mark. `white` is the paper-theme ink value; dark substrates mirror it. */
export function inkColor(white: number, alpha: number, dark: boolean, ink: Ink): string {
  const w = clamp01(white);
  if (!dark) {
    const g = Math.round(w * 255);
    return `rgba(${g},${g},${g},${alpha})`;
  }
  const b = 1 - w;
  return `rgba(${Math.round(ink[0] * b)},${Math.round(ink[1] * b)},${Math.round(ink[2] * b)},${alpha})`;
}

export function paintDots(
  ctx: CanvasRenderingContext2D,
  dots: Dot[],
  dark: boolean,
  ink: Ink,
  level: number,
  alpha: number,
): void {
  const swell = 1 + LEVEL_RADIUS_GAIN * clamp01(level);
  for (const d of dots) {
    const a = (d.a ?? 1) * alpha;
    if (a < 0.02) continue;
    ctx.fillStyle = inkColor(d.white, a, dark, ink);
    ctx.beginPath();
    ctx.arc(d.x, d.y, d.r * swell, 0, Math.PI * 2);
    ctx.fill();
  }
}

export function paintEdges(
  ctx: CanvasRenderingContext2D,
  lines: Line[],
  dark: boolean,
  ink: Ink,
  alpha: number,
): void {
  for (const l of lines) {
    const a = (l.a ?? 1) * alpha;
    if (a < 0.02) continue;
    ctx.strokeStyle = inkColor(l.white, a, dark, ink);
    ctx.lineWidth = l.w;
    ctx.beginPath();
    ctx.moveTo(l.x1, l.y1);
    ctx.lineTo(l.x2, l.y2);
    ctx.stroke();
  }
}

/** Lines first so nodes sit on top of their edges (same order as upstream). */
export function paintFrameInk(
  ctx: CanvasRenderingContext2D,
  frame: OrbFrame,
  dark: boolean,
  ink: Ink = WHITE_INK,
  level = 0,
  alpha = 1,
): void {
  if (frame.lines.length) paintEdges(ctx, frame.lines, dark, ink, alpha);
  paintDots(ctx, frame.dots, dark, ink, level, alpha);
}

/** "#rrggbb" -> ink, or white for anything that is not a 6-digit hex. */
export function parseInk(hex: string | null | undefined): Ink {
  const m = /^#?([0-9a-f]{6})$/i.exec((hex ?? "").trim());
  if (!m) return WHITE_INK;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}
