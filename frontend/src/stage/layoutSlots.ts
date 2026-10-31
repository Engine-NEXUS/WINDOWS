// Where each always-on stage element sits. These were scattered literals in five components (caption 0.515 / 0.895 in
// ResponseCaption, a magic `top: 112px` for the task stack, the hand chip's own bottom-edge rule), which is how the hand chip
// ended up on the same row as the caption and the task stack on top of a larger spinner. One module, pure functions, and a test
// that no two slots intersect on the common screens (layoutSlots.test.ts).
//
// All numbers are CSS px of the stage window (physical px from Rust are divided by devicePixelRatio by the caller).

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface Viewport {
  w: number;
  h: number;
}

/** The response caption: centre and top as fractions of the stage (user-tuned with caption-tuner, 2026-10-09). */
export const CAPTION = { centerX: 0.515, top: 0.895, width: 520, minHeight: 60 } as const;

/** Gap between stacked stage elements. */
export const SLOT_GAP = 8;

export function captionSlot(vp: Viewport): Rect {
  const w = CAPTION.width;
  return { x: vp.w * CAPTION.centerX - w / 2, y: vp.h * CAPTION.top, w, h: CAPTION.minHeight };
}

/** Fallback when the spinner has not been placed yet: where the default spinner sits. */
const TASK_STACK_FALLBACK = { top: 112, right: 24 };

/**
 * The background-task chips hang directly under the spinner, right-aligned to it. `spinner` is the spinner's rect in CSS px;
 * null before it is known. (The stack used to sit at a fixed 112 px, so a spinner enlarged in settings overlapped it.)
 */
export function taskStackSlot(spinner: Rect | null, vp: Viewport): { top: number; right: number } {
  if (!spinner || spinner.w <= 0 || spinner.h <= 0) return { ...TASK_STACK_FALLBACK };
  return {
    top: Math.round(spinner.y + spinner.h + SLOT_GAP),
    right: Math.max(0, Math.round(vp.w - (spinner.x + spinner.w))),
  };
}

export function intersects(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}
