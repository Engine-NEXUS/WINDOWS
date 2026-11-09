import { useEffect, useRef } from "react";
import type { OrbFrame } from "./engine/core";
import { blendFrames, MORPH_MS } from "./morph";
import { paintFrameInk, WHITE_INK, type Ink } from "./paintInk";
import { resolveOrb, type OrbDesign } from "./resolveOrb";
import { smoothLevel, voiceClockStep, wobbleScale } from "./voiceClock";

/** Voice level raises the clock by at most this much (1 = +100 %). */
export const LEVEL_SPEED_GAIN = 0.9;
/** Static frame time used for reduced motion and for a paused first paint. */
export const STATIC_FRAME_T = 0.6;

const LABELS: Record<OrbDesign, string> = {
  circle: "Ready",
  triangle: "Hand pointer on",
  square: "Hand pointer off",
  breathing: "Responding",
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
  /** Morph duration in ms (defaults to MORPH_MS; the preview slows it down to inspect it). */
  morphMs?: number;
  /** Polled once per frame, 0..1 (mic or TTS envelope). Refs keep React out of the animation loop. */
  getLevel?: () => number;
  /**
   * Listening mode: the animation clock only advances while `getLevel` reports sound (the shape holds still in silence),
   * runs faster the louder it is, and the wobble depth follows the level. The level is smoothed so it tracks the mic.
   */
  voiceDriven?: boolean;
  paused?: boolean;
  reducedMotion?: boolean;
  className?: string;
  "aria-label"?: string;
}

interface MorphState {
  design: OrbDesign | null;
  /** What was last drawn: the starting shape when the design changes (even mid-morph, so changes chain smoothly). */
  last: OrbFrame | null;
  from: OrbFrame | null;
  since: number;
  t: number;
  /** Smoothed mic level (voice-driven mode). */
  level: number;
}

/**
 * Canvas orb built on the vendored thinking-orbs engine. One rAF loop per orb: the clock accumulates (so a speed
 * change never jumps the phase) and a design change MORPHS: every particle of the current shape travels to a particle
 * of the new one over MORPH_MS (see morph.ts). Nothing runs while `paused` or when the tab is hidden. Canvas size is
 * set through the CSSOM because the stage CSP drops React inline styles.
 */
export function DottedOrb({
  design,
  size,
  dark = true,
  ink = WHITE_INK,
  speed = 1,
  morphMs = MORPH_MS,
  getLevel,
  voiceDriven = false,
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
  const morphMsRef = useRef(morphMs);
  morphMsRef.current = morphMs;
  const stateRef = useRef<MorphState>({ design: null, last: null, from: null, since: 0, t: 0, level: 0 });
  const voiceDrivenRef = useRef(voiceDriven);
  voiceDrivenRef.current = voiceDriven;

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

    const st = stateRef.current;
    const next = resolveOrb(design, size);
    if (st.design !== null && st.design !== design && st.last && !reducedMotion) {
      st.from = st.last;
      st.since = performance.now();
    }
    if (reducedMotion) st.from = null;
    st.design = design;

    const paint = (now: number) => {
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, size, size);
      const vd = voiceDrivenRef.current;
      const level = vd ? st.level : Math.min(1, Math.max(0, levelRef.current?.() ?? 0));
      const opts =
        vd && typeof next.opts.wobMul === "number" ? { ...next.opts, wobMul: next.opts.wobMul * wobbleScale(st.level) } : next.opts;
      let frame = next.frame(size, st.t * next.speed, opts);
      if (st.from) {
        const k = (now - st.since) / Math.max(1, morphMsRef.current);
        if (k >= 1) st.from = null;
        else frame = blendFrames(st.from, frame, k, size / 2, size / 2);
      }
      st.last = frame;
      paintFrameInk(ctx, frame, dark, ink, level);
    };

    if (reducedMotion) {
      st.t = STATIC_FRAME_T;
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
      const raw = Math.min(1, Math.max(0, levelRef.current?.() ?? 0));
      if (voiceDrivenRef.current) {
        st.level = smoothLevel(st.level, raw, dt);
        st.t += voiceClockStep(dt * speedRef.current, st.level);
      } else {
        st.level = raw;
        st.t += dt * speedRef.current * (1 + LEVEL_SPEED_GAIN * raw);
      }
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
