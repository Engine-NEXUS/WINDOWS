// Pure helpers for the full-screen "toast expands into the sidebar" animation (no DOM, no Tauri).

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface ExpandPayload {
  title: string;
  status: "failed" | "recovered" | "info";
  /** Where the Windows toast was (bottom-right) and the sidebar's final rectangle, in CSS px of the overlay. */
  from: Rect;
  to: Rect;
  durationMs: number;
}

export type ExpandPhase = "start" | "end" | "leaving";

/** The overlay never waits for the sidebar longer than this; it fades out anyway. */
export const HOLD_TIMEOUT_MS = 1500;
export const LEAVE_MS = 180;
const MIN = 40;

const num = (v: unknown, fallback: number) => (typeof v === "number" && Number.isFinite(v) ? v : fallback);

/** Keeps a rectangle inside the overlay and a sensible size, whatever the payload says. */
export function clampRect(r: Partial<Rect> | undefined, vp: { w: number; h: number }): Rect {
  const w = Math.min(Math.max(num(r?.w, MIN), MIN), vp.w);
  const h = Math.min(Math.max(num(r?.h, MIN), MIN), vp.h);
  return {
    x: Math.min(Math.max(num(r?.x, 0), 0), Math.max(vp.w - w, 0)),
    y: Math.min(Math.max(num(r?.y, 0), 0), Math.max(vp.h - h, 0)),
    w,
    h,
  };
}

/** Animation length: the payload's, bounded; zero when the user prefers reduced motion. */
export function durationFor(payload: Pick<ExpandPayload, "durationMs">, reducedMotion: boolean): number {
  if (reducedMotion) return 0;
  return Math.min(Math.max(num(payload.durationMs, 280), 0), 1000);
}

/** Position/size/opacity of the expanding card in each phase. Layout properties animate (not a scaled snapshot), so
 *  the text is never squashed or stretched. */
export function expandStyle(phase: ExpandPhase, p: ExpandPayload, vp: { w: number; h: number }) {
  const r = clampRect(phase === "start" ? p.from : p.to, vp);
  return { left: r.x, top: r.y, width: r.w, height: r.h, opacity: phase === "leaving" ? 0 : 1 };
}
