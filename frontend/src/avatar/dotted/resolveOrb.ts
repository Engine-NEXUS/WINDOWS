// Resolve an orb design + render size to engine draw options.
// Upstream tunes two sizes (64 and 20 px). NEXUS draws the orb larger (about 100-200 px), so we start from the
// 64 px tuning and add dots in proportion to the extra area (never past the dense 300 px base profile).

import { MODE_FRAMES } from "./engine/registry";
import { BASE_PROFILES, scaleCounts, scaleRadii, type ModeOpts } from "./engine/profiles";
import type { ModeFrame } from "./engine/types";
import { frameCircle } from "./nexusModes";
import { PRESETS, STATE_TO_MODE } from "./presets";
import type { OrbState } from "./types";

/** The nine upstream designs plus NEXUS's own plain `circle` (idle). */
export type OrbDesign = OrbState | "circle";

export const ORB_DESIGNS: readonly OrbDesign[] = [
  "circle",
  "breathing",
  "listening",
  "working",
  "searching",
  "connecting",
  "weaving",
  "composing",
  "solving",
  "shaping",
];

export interface ResolvedOrb {
  design: OrbDesign;
  /** Geometry for one instant: (size, t, opts) -> finished frame. */
  frame: ModeFrame;
  /** Clock multiplier baked into the preset. */
  speed: number;
  opts: ModeOpts;
}

const cache = new Map<string, ResolvedOrb>();

/** How many times denser than the 64 px tuning we draw at `size` px (1 at <= 64 px). */
export function densityFor(size: number): number {
  if (!(size > 64)) return 1;
  return Math.min(4, (size / 64) ** 2 * 0.55 + 0.45);
}

export function resolveOrb(design: OrbDesign, size: number): ResolvedOrb {
  const key = `${design}-${Math.round(size)}`;
  const hit = cache.get(key);
  if (hit) return hit;

  if (design === "circle") {
    const own: ResolvedOrb = { design, frame: frameCircle, speed: 1, opts: {} };
    cache.set(key, own);
    return own;
  }

  const mode = STATE_TO_MODE[design];
  const preset = PRESETS[mode][64];
  // Count multiplier relative to the dense base profile; never denser than the base (1).
  const count = Math.min(1, preset.count * densityFor(size));
  let opts: ModeOpts = { ...BASE_PROFILES[mode] };
  if (count !== 1) opts = scaleCounts(opts, count);
  if (preset.size !== 1) opts = scaleRadii(opts, preset.size);
  if (preset.extra) opts = { ...opts, ...preset.extra };

  const resolved: ResolvedOrb = { design, frame: MODE_FRAMES[mode], speed: preset.speed, opts };
  cache.set(key, resolved);
  return resolved;
}

/** One finished frame (pure geometry) for tests and the preview thumbnails. */
export function frameFor(design: OrbDesign, size: number, tSeconds: number) {
  const r = resolveOrb(design, size);
  return r.frame(size, tSeconds * r.speed, r.opts);
}
