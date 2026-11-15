// NEXUS-owned orb designs that are not part of the vendored thinking-orbs engine.
// Same contract as an engine mode: pure geometry over (size, t, opts) -> a finished frame.

import type { Dot, OrbFrame } from "./engine/core";
import type { ModeFrame } from "./engine/types";

/** Circle radius as a fraction of the canvas side (the breathing ring sits at about the same size). */
export const CIRCLE_RADIUS_FRAC = 0.36;
/** Gap between neighbouring dots, as a fraction of the canvas side. */
const CIRCLE_SPACING_FRAC = 0.05;
/** Dot radius as a fraction of the canvas side. */
const CIRCLE_DOT_FRAC = 0.017;
/** Peak dimming of the slow shimmer (ink value, 0 = brightest). Small on purpose: idle is calm. */
const SHIMMER_DEPTH = 0.2;
/** Shimmer angular speed in radians per second of the (preset-scaled) clock. */
const SHIMMER_SPEED = 0.55;

/** Hand mode: the same ring as the idle circle, but as a triangle (pointer on) or a square (pointer off). */
type Vert = readonly [number, number];
const SHAPE_R = CIRCLE_RADIUS_FRAC;
// vertices as fractions of the circle radius: triangle point up, square upright, both fitting the same ring
const TRIANGLE_VERTS: Vert[] = [[0, -1], [0.866, 0.5], [-0.866, 0.5]];
const SQUARE_VERTS: Vert[] = [[-0.78, -0.78], [0.78, -0.78], [0.78, 0.78], [-0.78, 0.78]];

/** Point `f` (0..1) of the way round a closed polygon, by arc length, starting at the first vertex. */
export function polygonPoint(verts: readonly Vert[], f: number): [number, number] {
  const n = verts.length;
  const lens = verts.map((v, i) => Math.hypot(verts[(i + 1) % n][0] - v[0], verts[(i + 1) % n][1] - v[1]));
  const total = lens.reduce((a, b) => a + b, 0);
  let d = (((f % 1) + 1) % 1) * total;
  for (let i = 0; i < n; i++) {
    if (d <= lens[i] || i === n - 1) {
      const k = lens[i] ? Math.min(1, d / lens[i]) : 0;
      const a = verts[i];
      const b = verts[(i + 1) % n];
      return [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k];
    }
    d -= lens[i];
  }
  return [verts[0][0], verts[0][1]];
}

function shapeFrame(verts: readonly Vert[]): ModeFrame {
  return (size, t): OrbFrame => {
    const n = circleDotCount(size);
    const cx = size / 2;
    const cy = size / 2;
    const dotR = Math.max(0.9, CIRCLE_DOT_FRAC * size);
    const dots: Dot[] = [];
    for (let i = 0; i < n; i++) {
      const f = i / n;
      const [x, y] = polygonPoint(verts, f);
      const a = f * Math.PI * 2 - Math.PI / 2;
      const wave = 0.5 + 0.5 * Math.cos(a - t * SHIMMER_SPEED);
      dots.push({ x: cx + x * SHAPE_R * size, y: cy + y * SHAPE_R * size, z: 0, r: dotR, white: SHIMMER_DEPTH * (1 - wave), a: 1 });
    }
    return { dots, lines: [] };
  };
}

export const frameTriangle: ModeFrame = shapeFrame(TRIANGLE_VERTS);
export const frameSquare: ModeFrame = shapeFrame(SQUARE_VERTS);

/** Number of dots on the idle circle at a given canvas size. */
export function circleDotCount(size: number): number {
  const circumference = 2 * Math.PI * CIRCLE_RADIUS_FRAC * size;
  return Math.max(24, Math.round(circumference / (CIRCLE_SPACING_FRAC * size)));
}

/**
 * Idle: a plain circle of evenly spaced, equal dots. It does not wobble or pulse; the only life is a very slow,
 * faint brightness wave travelling around it so the orb never looks frozen.
 */
export const frameCircle: ModeFrame = (size, t): OrbFrame => {
  const n = circleDotCount(size);
  const cx = size / 2;
  const cy = size / 2;
  const radius = CIRCLE_RADIUS_FRAC * size;
  const dotR = Math.max(0.9, CIRCLE_DOT_FRAC * size);
  const dots: Dot[] = [];
  for (let i = 0; i < n; i++) {
    const a = (i / n) * Math.PI * 2 - Math.PI / 2;
    const wave = 0.5 + 0.5 * Math.cos(a - t * SHIMMER_SPEED);
    dots.push({
      x: cx + Math.cos(a) * radius,
      y: cy + Math.sin(a) * radius,
      z: 0,
      r: dotR,
      white: SHIMMER_DEPTH * (1 - wave),
      a: 1,
    });
  }
  return { dots, lines: [] };
};
