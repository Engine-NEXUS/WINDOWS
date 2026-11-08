// Particle morph between two orb frames: every dot of the old shape travels to a dot of the new one.
// Pure geometry (no canvas), so it is unit-tested and the same maths runs in the app and the previews.

import type { Dot, Line, OrbFrame } from "./engine/core";

/** How long a design change takes to play. */
export const MORPH_MS = 650;

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);
const lerp = (a: number, b: number, k: number) => a + (b - a) * k;

/** Ease-in-out, no overshoot (so dots never fly past their target and snap back). */
export function easeInOutCubic(k: number): number {
  const x = clamp01(k);
  return x < 0.5 ? 4 * x * x * x : 1 - Math.pow(-2 * x + 2, 3) / 2;
}

/** Dots ordered by angle around the centre (then radius): neighbours on a ring stay neighbours, so rings and
 *  globes turn into each other as a swirl instead of crossing paths. */
function byAngle(dots: Dot[], cx: number, cy: number): Dot[] {
  const keyed = dots.map((d) => {
    const dx = d.x - cx;
    const dy = d.y - cy;
    let ang = Math.atan2(dy, dx);
    if (ang < 0) ang += Math.PI * 2;
    return { d, ang, rad: dx * dx + dy * dy };
  });
  keyed.sort((p, q) => p.ang - q.ang || p.rad - q.rad);
  return keyed.map((p) => p.d);
}

/**
 * Frame at progress `k` (0 = `from`, 1 = `to`).
 * - Dots are paired by rank, so counts may differ: a dot that must split fades its twin in as it travels, dots
 *   that must merge fade out, which keeps the total brightness steady.
 * - Edges (the `connecting` web) fade out and in; they never travel.
 * - k <= 0 returns `from` and k >= 1 returns `to` unchanged (same object), so the end state is exact.
 */
export function blendFrames(from: OrbFrame, to: OrbFrame, k: number, cx: number, cy: number): OrbFrame {
  if (k <= 0) return from;
  if (k >= 1) return to;
  const e = easeInOutCubic(k);

  const lines: Line[] = [];
  for (const l of from.lines) lines.push({ ...l, a: (l.a ?? 1) * (1 - e) });
  for (const l of to.lines) lines.push({ ...l, a: (l.a ?? 1) * e });

  const A = byAngle(from.dots, cx, cy);
  const B = byAngle(to.dots, cx, cy);
  const na = A.length;
  const nb = B.length;
  const dots: Dot[] = [];

  if (na === 0 || nb === 0) {
    // Nothing to pair with: fade the whole shape.
    for (const d of A) dots.push({ ...d, a: (d.a ?? 1) * (1 - e) });
    for (const d of B) dots.push({ ...d, a: (d.a ?? 1) * e });
    return { dots, lines };
  }

  const n = Math.max(na, nb);
  for (let i = 0; i < n; i++) {
    const ia = Math.floor((i * na) / n);
    const ib = Math.floor((i * nb) / n);
    const a = A[ia];
    const b = B[ib];
    const splitsFromA = i > 0 && Math.floor(((i - 1) * na) / n) === ia; // this A dot already left on an earlier pair
    const mergesIntoB = i > 0 && Math.floor(((i - 1) * nb) / n) === ib; // this B dot already has a dot arriving
    let alpha = lerp(a.a ?? 1, b.a ?? 1, e);
    if (splitsFromA) alpha *= e;
    else if (mergesIntoB) alpha *= 1 - e;
    dots.push({
      x: lerp(a.x, b.x, e),
      y: lerp(a.y, b.y, e),
      z: lerp(a.z, b.z, e),
      r: lerp(a.r, b.r, e),
      white: lerp(a.white, b.white, e),
      a: clamp01(alpha),
    });
  }
  dots.sort((p, q) => p.z - q.z); // far to near, like a finished engine frame
  return { dots, lines };
}
