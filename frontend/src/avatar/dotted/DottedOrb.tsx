import { useEffect, useRef } from "react";
import { MODE_FRAMES } from "./engine/registry";
import { paintFrameInk, WHITE_INK, type Ink } from "./paintInk";
import { resolveOrb, type OrbDesign, type ResolvedOrb } from "./resolveOrb";

/** Cross-fade between two designs (the engine cannot morph between modes). */
export const CROSSFADE_MS = 180;
/** Voice level raises the clock by at most this much (1 = +100 %). */
export const LEVEL_SPEED_GAIN = 0.9;
/** Static frame time used for reduced motion and for a paused first paint. */
export const STATIC_FRAME_T = 0.6;

const LABELS: Record<OrbDesign, string> = {
  breathing: "Idle",
  listening: "Listening",
  working: "Working",
  searching: "Searching",
  connecting: "Connecting",
  weaving: "Working in the background",
  composing: "Responding",
  solving: "Solving",
  shaping: "Shaping",
};

export interface DottedOrbProps {
  design: OrbDesign;
  /** Square size in CSS px. */
  size: number;
  /** Dark substrate = light ink (NEXUS capsule is black); false = dark ink for the light theme. */
  dark?: boolean;
  ink?: Ink;
  /** Speed multiplier on top of the design's tuned speed. */
  speed?: number;
  /** Polled once per frame, 0..1 (mic or TTS envelope). Refs keep React out of the animation loop. */
  getLevel?: () => number;
  paused?: boolean;
  reducedMotion?: boolean;
  className?: string;
  "aria-label"?: string;
}

function drawDesign(
  ctx: CanvasRenderingContext2D,
  r: ResolvedOrb,
  size: number,
  t: number,
  dark: boolean,
  ink: Ink,
  level: number,
  alpha: number,
): void {
  paintFrameInk(ctx, MODE_FRAMES[r.mode](size, t * r.speed, r.opts), dark, ink, level, alpha);
}

/**
 * Canvas orb built on the vendored thinking-orbs engine. One rAF loop per orb: the clock accumulates
 * (so a speed change never jumps the phase), a design change cross-fades for CROSSFADE_MS, and nothing
 * runs while `paused` or when the tab is hidden. Canvas size is set through the CSSOM because the stage
 * CSP drops React inline styles.
 */
export function DottedOrb({
  design,
  size,
  dark = true,
  ink = WHITE_INK,
  speed = 1,
  getLevel,
  paused = false,
  reducedMotion = false,
  className,
  "aria-label": ariaLabel,
}: DottedOrbProps) {
  const ref = useRef<HTMLCanvasElement | null>(null);
  const levelRef = useRef(getLevel);
  levelRef.current = getLevel;
  const speedRef = useRef(speed);
  speedRef.current = speed;
  // Persist the fade state across effect re-runs (design changes re-run the effect).
  const fadeRef = useRef<{ prev: ResolvedOrb | null; since: number; current: ResolvedOrb | null; t: number }>({
    prev: null,
    since: 0,
    current: null,
    t: 0,
  });

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const dpr = Math.min(2, (typeof devicePixelRatio !== "undefined" && devicePixelRatio) || 1);
    canvas.width = Math.round(size * dpr);
    canvas.height = Math.round(size * dpr);
    canvas.style.width = `${size}px`;
    canvas.style.height = `${size}px`;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const fade = fadeRef.current;
    const next = resolveOrb(design, size);
    if (fade.current && fade.current !== next) {
      fade.prev = fade.current;
      fade.since = performance.now();
    }
    fade.current = next;

    const paint = (now: number) => {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, size, size);
      const level = Math.min(1, Math.max(0, levelRef.current?.() ?? 0));
      const k = fade.prev ? Math.min(1, (now - fade.since) / CROSSFADE_MS) : 1;
      if (fade.prev && k < 1) {
        drawDesign(ctx, fade.prev, size, fade.t, dark, ink, level, 1 - k);
        drawDesign(ctx, next, size, fade.t, dark, ink, level, k);
      } else {
        fade.prev = null;
        drawDesign(ctx, next, size, fade.t, dark, ink, level, 1);
      }
    };

    if (reducedMotion) {
      fade.prev = null;
      fade.t = STATIC_FRAME_T;
      paint(performance.now());
      return;
    }

    let raf = 0;
    let running = false;
    let last = performance.now();
    const loop = () => {
      const now = performance.now();
      const dt = Math.min(0.1, (now - last) / 1000);
      last = now;
      const level = Math.min(1, Math.max(0, levelRef.current?.() ?? 0));
      fade.t += dt * speedRef.current * (1 + LEVEL_SPEED_GAIN * level);
      paint(now);
      if (running) raf = requestAnimationFrame(loop);
    };
    const start = () => {
      if (running || paused) return;
      running = true;
      last = performance.now();
      raf = requestAnimationFrame(loop);
    };
    const stop = () => {
      running = false;
      cancelAnimationFrame(raf);
    };
    const onVis = () => {
      if (document.visibilityState === "hidden") stop();
      else start();
    };

    paint(performance.now()); // always draw one frame, even when paused
    document.addEventListener("visibilitychange", onVis);
    start();
    return () => {
      stop();
      document.removeEventListener("visibilitychange", onVis);
    };
  }, [design, size, dark, ink, paused, reducedMotion]);

  return <canvas ref={ref} className={className} role="img" aria-label={ariaLabel ?? LABELS[design]} />;
}
