/**
 * Pure timing/geometry for the "analyse my screen" wave: one burst of rings leaves the orb once and covers the screen.
 * No DOM here, so it is unit-tested in node (analyseWaveModel.test.ts). The canvas drawing is in AnalyseWave.tsx.
 * Design record: docs/features/195-screen-analysis-wave-animation-plan.md.
 */

export interface WaveParams {
  /** Rings in the burst. */
  rings: number;
  /** Time one ring takes to travel from the orb to the farthest corner, ms. */
  durationMs: number;
  /** Pause before the first ring leaves, ms. */
  leadMs: number;
  /** Delay between rings, ms. */
  staggerMs: number;
  /** Overall strength, 0..1. */
  amp: number;
}

/**
 * The owner's choice after the design review (2026-11-22): Rings style, ONE ring, strength 80, at the review page's
 * 0.25x speed. 1400 ms at 0.25x is 5600 ms: a slow, deliberate ring that covers the model's thinking time.
 */
export const DEFAULT_WAVE: WaveParams = { rings: 1, durationMs: 5600, leadMs: 80, staggerMs: 110, amp: 0.8 };

export interface RingDraw {
  radius: number;
  thick: number;
  alpha: number;
}

export interface WaveFrame {
  rings: RingDraw[];
  /** Full-screen white wash alpha (reduced-motion mode only; 0 otherwise). */
  wash: number;
  /** True once nothing is left to draw, so the loop can stop and the canvas can be cleared. */
  done: boolean;
}

/** Fast start, long soft landing. */
export function easeOutCubic(u: number): number {
  const c = Math.min(1, Math.max(0, u));
  return 1 - Math.pow(1 - c, 3);
}

/** Distance from the origin to the farthest screen corner, with a little margin so the front leaves the screen. */
export function farthestCorner(ox: number, oy: number, w: number, h: number): number {
  return Math.hypot(Math.max(ox, w - ox), Math.max(oy, h - oy)) * 1.04;
}

/** Reduced motion: no travelling wave, one calm wash. */
export const REDUCED_WASH_MS = 600;

/** Total time the animation can be on screen, ms. */
export function totalMs(p: WaveParams = DEFAULT_WAVE, reduced = false): number {
  if (reduced) return p.leadMs + REDUCED_WASH_MS + 50;
  return p.leadMs + Math.max(0, p.rings - 1) * p.staggerMs + p.durationMs;
}

export function waveFrame(
  elapsedMs: number,
  origin: { x: number; y: number },
  size: { w: number; h: number },
  reduced = false,
  p: WaveParams = DEFAULT_WAVE,
): WaveFrame {
  if (elapsedMs >= totalMs(p, reduced)) return { rings: [], wash: 0, done: true };

  if (reduced) {
    const u = Math.min(1, Math.max(0, (elapsedMs - p.leadMs) / REDUCED_WASH_MS));
    return { rings: [], wash: Math.sin(u * Math.PI) * 0.16 * (p.amp / 0.8), done: false };
  }

  const rMax = farthestCorner(origin.x, origin.y, size.w, size.h);
  const rings: RingDraw[] = [];
  for (let i = 0; i < p.rings; i++) {
    const u = (elapsedMs - p.leadMs - i * p.staggerMs) / p.durationMs;
    if (u <= 0 || u >= 1) continue;
    rings.push({
      radius: rMax * easeOutCubic(u),
      thick: 20 - 13 * u, // the front thins as it travels
      alpha: Math.pow(1 - u, 1.35) * p.amp * (1 - i * 0.2), // later rings are fainter
    });
  }
  return { rings, wash: 0, done: false };
}
