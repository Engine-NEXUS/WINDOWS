import { useEffect, useRef } from "react";

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Particle {
  startX: number;
  startY: number;
  stagger: number;
  swirl: number;
  radius: number;
}

const DURATION_MS = 900;
const COUNT = 140;

// Cubic in-out — same easing voice-orb.js's own assemble() tween uses
// (voice-orb.js `_tick`'s `_assembling` branch), so the two converge in
// visual lockstep rather than fighting each other's pacing.
function ease(p: number): number {
  return p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2;
}

function buildParticles(viewportW: number, viewportH: number): Particle[] {
  const particles: Particle[] = [];
  for (let i = 0; i < COUNT; i++) {
    // Edge/corner-biased start points — "coming from all places" reads
    // better when particles visibly originate from the screen's margins
    // rather than a uniform scatter that clusters unnoticed near center.
    const edge = Math.floor(Math.random() * 4);
    let startX: number;
    let startY: number;
    if (edge === 0) { startX = Math.random() * viewportW; startY = -20 - Math.random() * 60; }
    else if (edge === 1) { startX = viewportW + 20 + Math.random() * 60; startY = Math.random() * viewportH; }
    else if (edge === 2) { startX = Math.random() * viewportW; startY = viewportH + 20 + Math.random() * 60; }
    else { startX = -20 - Math.random() * 60; startY = Math.random() * viewportH; }
    particles.push({
      startX,
      startY,
      stagger: Math.random() * 0.35,
      swirl: Math.random() * Math.PI * 2,
      radius: 1.2 + Math.random() * 1.8,
    });
  }
  return particles;
}

/**
 * EntranceBurst — the "particles gather from across the whole screen" leg
 * of the orb's wake entrance (sub-phase A). voice-orb.js's own assemble()
 * burst is bounded to the orb's own small canvas (it has no projection
 * matrix and the canvas is sized to the orb's own small host element —
 * particles can never render outside the canvas's own pixel rect, so the
 * shape itself can't be widened to reach screen edges). This is a
 * deliberately separate, lightweight 2D-canvas overlay — not the WebGL
 * shader/particle system — covering the full stage viewport ONLY for the
 * ~900ms entrance window, converging into the same point voice-orb.js's
 * own assemble() is simultaneously forming into. It unmounts/clears
 * itself the moment the burst finishes; the persistent orb underneath is
 * untouched.
 *
 * Respects prefers-reduced-motion (skips the burst outright, matching
 * voice-orb.js's own reduced-motion short-circuit in assemble()).
 */
export function EntranceBurst({ burstSeq, targetRect }: { burstSeq: number; targetRect: OrbRect | null }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const frameRef = useRef(0);

  useEffect(() => {
    if (burstSeq <= 0 || !targetRect) return;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    if (motion.matches) return;
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const viewportW = window.innerWidth;
    const viewportH = window.innerHeight;
    canvas.width = Math.round(viewportW * dpr);
    canvas.height = Math.round(viewportH * dpr);
    ctx.scale(dpr, dpr);

    const dpr2 = window.devicePixelRatio || 1;
    const targetX = targetRect.x / dpr2 + targetRect.w / dpr2 / 2;
    const targetY = targetRect.y / dpr2 + targetRect.h / dpr2 / 2;

    const particles = buildParticles(viewportW, viewportH);
    const start = performance.now();
    canvas.style.opacity = "1";

    const tick = (now: number) => {
      const elapsed = (now - start) / DURATION_MS;
      ctx.clearRect(0, 0, viewportW, viewportH);
      if (elapsed >= 1) {
        canvas.style.opacity = "0";
        frameRef.current = 0;
        return;
      }
      // Fade the whole overlay out over the final ~25% so it blends into
      // the real orb's own particles taking over, instead of a hard cut.
      const fade = elapsed > 0.75 ? 1 - (elapsed - 0.75) / 0.25 : 1;
      ctx.globalCompositeOperation = "lighter";
      for (const p of particles) {
        const raw = Math.max(0, Math.min(1, (elapsed - p.stagger) / (1 - p.stagger)));
        const e = ease(raw);
        const swirlAmt = (1 - e) * 24;
        const x = p.startX + (targetX - p.startX) * e + Math.sin(p.swirl + now * 0.004) * swirlAmt;
        const y = p.startY + (targetY - p.startY) * e + Math.cos(p.swirl + now * 0.004) * swirlAmt;
        const alpha = (0.15 + 0.55 * e) * fade;
        ctx.beginPath();
        ctx.fillStyle = `rgba(226,230,255,${alpha})`;
        ctx.arc(x, y, p.radius, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.globalCompositeOperation = "source-over";
      frameRef.current = requestAnimationFrame(tick);
    };
    frameRef.current = requestAnimationFrame(tick);

    return () => {
      if (frameRef.current) cancelAnimationFrame(frameRef.current);
      frameRef.current = 0;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [burstSeq]);

  return (
    <canvas
      aria-hidden
      ref={(el) => {
        canvasRef.current = el;
        if (!el) return;
        el.style.position = "fixed";
        el.style.inset = "0";
        el.style.width = "100vw";
        el.style.height = "100vh";
        el.style.pointerEvents = "none";
        el.style.zIndex = "1";
        el.style.opacity = "0";
      }}
    />
  );
}
