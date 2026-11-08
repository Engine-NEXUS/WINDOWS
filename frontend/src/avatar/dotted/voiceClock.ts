// Voice-driven motion for the listening orb: the ring moves only while the mic hears sound, and moves in step
// with it. Pure maths, so the timing is unit-tested rather than judged by eye.

/** Smoothed mic level below this counts as silence: the ring holds still. */
export const LEVEL_GATE = 0.05;
/** Time constants (seconds) for following the mic: quick up (sync with the voice), slower down (settles softly). */
export const ATTACK_TAU = 0.04;
export const RELEASE_TAU = 0.2;

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);

/** Exponential follower with different attack and release speeds. `dtSec` is the frame time. */
export function smoothLevel(prev: number, target: number, dtSec: number): number {
  const t = clamp01(Number.isFinite(target) ? target : 0);
  const dt = Math.max(0, dtSec);
  const tau = t > prev ? ATTACK_TAU : RELEASE_TAU;
  const k = 1 - Math.exp(-dt / tau);
  return prev + (t - prev) * k;
}

/**
 * How far the animation clock advances this frame. Zero in silence (the ring does not move at all), then faster
 * the louder the voice is.
 */
export function voiceClockStep(dtSec: number, level: number): number {
  if (!(level >= LEVEL_GATE)) return 0;
  return Math.max(0, dtSec) * (0.35 + 2.4 * clamp01(level));
}

/** Multiplier for the ring's wobble depth: nearly round in silence, deeper as the voice gets louder. */
export function wobbleScale(level: number): number {
  return 0.15 + 1.35 * clamp01(level);
}
