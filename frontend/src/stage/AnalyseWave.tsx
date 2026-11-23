import { useEffect, useRef, useState } from "react";
import "./analyse-wave.css";
import { DEFAULT_WAVE, waveFrame } from "./analyseWaveModel";

/**
 * "Analyse my screen" wave: one burst leaves the orb and covers the whole screen (design: docs/features/195).
 *
 * Rust emits `screen:analyse_wave` right AFTER the screenshot was taken (the stage is capture-excluded for the shot,
 * so the wave can never be in the picture). The loop only runs while the burst plays, then the canvas is cleared and
 * hidden. Esc / "stop" (`orb:cancel`) and the end of a tour clear it at once. Click-through, never takes input.
 * Drawn on a canvas: the stage CSP drops inline `style` props, and canvas is not affected.
 */

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** One ring: a white core with a dark rim on both sides, so it reads on a light AND a dark screen. */
function drawRing(ctx: CanvasRenderingContext2D, ox: number, oy: number, r: number, thick: number, alpha: number): void {
  if (alpha <= 0.003 || r < 1) return;
  const inner = Math.max(0, r - thick * 1.6);
  const outer = r + thick * 1.6;
  const g = ctx.createRadialGradient(ox, oy, inner, ox, oy, outer);
  const white = (a: number) => `rgba(255,255,255,${(a * alpha).toFixed(3)})`;
  const dark = (a: number) => `rgba(0,0,0,${(a * alpha).toFixed(3)})`;
  g.addColorStop(0, "rgba(0,0,0,0)");
  g.addColorStop(0.18, dark(0.22));
  g.addColorStop(0.42, white(0.55));
  g.addColorStop(0.5, white(1));
  g.addColorStop(0.58, white(0.55));
  g.addColorStop(0.82, dark(0.18));
  g.addColorStop(1, "rgba(0,0,0,0)");
  ctx.fillStyle = g;
  ctx.beginPath();
  ctx.arc(ox, oy, outer, 0, Math.PI * 2);
  ctx.arc(ox, oy, inner, 0, Math.PI * 2, true);
  ctx.fill("evenodd");
}

export function AnalyseWave() {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const rectRef = useRef<OrbRect | null>(null);
  const rafRef = useRef<number | null>(null);
  const [on, setOn] = useState(false);

  useEffect(() => {
    let alive = true;
    let unlisteners: (() => void)[] = [];

    const stop = () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
      const c = canvasRef.current;
      c?.getContext("2d")?.clearRect(0, 0, c.width, c.height);
      setOn(false);
    };

    const start = () => {
      const canvas = canvasRef.current;
      if (!canvas) return;
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
      const dpr = window.devicePixelRatio || 1;
      const w = window.innerWidth;
      const h = window.innerHeight;
      canvas.width = Math.round(w * dpr);
      canvas.height = Math.round(h * dpr);
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      const reduced = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
      setOn(true);

      // The wave leaves from the orb's centre (Rust sends its rect in physical px); top-centre until it is known.
      const r = rectRef.current;
      const origin = r ? { x: (r.x + r.w / 2) / dpr, y: (r.y + r.h / 2) / dpr } : { x: w / 2, y: 52 };

      const t0 = performance.now();
      const loop = (now: number) => {
        const frame = waveFrame(now - t0, origin, { w, h }, reduced, DEFAULT_WAVE);
        ctx.clearRect(0, 0, w, h);
        if (frame.wash > 0) {
          ctx.fillStyle = `rgba(255,255,255,${frame.wash.toFixed(3)})`;
          ctx.fillRect(0, 0, w, h);
        }
        for (const ring of frame.rings) drawRing(ctx, origin.x, origin.y, ring.radius, ring.thick, ring.alpha);
        if (frame.done) {
          stop();
          return;
        }
        rafRef.current = requestAnimationFrame(loop);
      };
      rafRef.current = requestAnimationFrame(loop);
    };

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const subs = await Promise.all([
        listen("screen:analyse_wave", () => start()),
        listen("orb:cancel", () => stop()),
        listen("screen:tour_end", () => stop()),
        listen<OrbRect>("stage:orb_rect", (e) => {
          if (e.payload) rectRef.current = e.payload;
        }),
      ]);
      if (!alive) subs.forEach((u) => u());
      else unlisteners = subs;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const pending = await invoke<OrbRect | null>("get_pending_orb_rect");
        if (pending) rectRef.current = pending;
      } catch {
        /* non-Tauri (dev/tests): fall back to top-centre */
      }
    })().catch(() => {});

    return () => {
      alive = false;
      unlisteners.forEach((u) => u());
      stop();
    };
  }, []);

  return <canvas ref={canvasRef} className={on ? "analyse-wave analyse-wave--on" : "analyse-wave"} aria-hidden="true" />;
}
