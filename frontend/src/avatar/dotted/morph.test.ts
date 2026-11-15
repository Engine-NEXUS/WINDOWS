import { describe, expect, it } from "vitest";
import type { Dot, OrbFrame } from "./engine/core";
import { blendFrames, easeInOutCubic, MORPH_MS } from "./morph";
import { circleDotCount, CIRCLE_RADIUS_FRAC, frameCircle } from "./nexusModes";
import { frameFor, ORB_DESIGNS } from "./resolveOrb";

const SIZE = 100;
const C = SIZE / 2;

function ring(n: number, radius: number, z = 0, r = 2): Dot[] {
  return Array.from({ length: n }, (_, i) => {
    const a = (i / n) * Math.PI * 2;
    return { x: C + Math.cos(a) * radius, y: C + Math.sin(a) * radius, z, r, white: 0, a: 1 };
  });
}
const frame = (dots: Dot[], lines: OrbFrame["lines"] = []): OrbFrame => ({ dots, lines });

describe("easeInOutCubic", () => {
  it("goes 0 to 1, is symmetric, never overshoots and is monotonic", () => {
    expect(easeInOutCubic(0)).toBe(0);
    expect(easeInOutCubic(1)).toBe(1);
    expect(easeInOutCubic(0.5)).toBeCloseTo(0.5, 10);
    let prev = 0;
    for (let k = 0; k <= 1.0001; k += 0.01) {
      const v = easeInOutCubic(k);
      expect(v).toBeGreaterThanOrEqual(prev - 1e-12);
      expect(v).toBeLessThanOrEqual(1);
      prev = v;
    }
    expect(easeInOutCubic(-2)).toBe(0);
    expect(easeInOutCubic(9)).toBe(1);
  });
  it("is a real morph length, not a blink", () => {
    expect(MORPH_MS).toBeGreaterThanOrEqual(400);
    expect(MORPH_MS).toBeLessThanOrEqual(1200);
  });
});

describe("blendFrames", () => {
  const A = frame(ring(40, 30));
  const B = frame(ring(40, 15, 0.3, 3));

  it("returns the source at k=0 and the target at k=1, as the same objects", () => {
    expect(blendFrames(A, B, 0, C, C)).toBe(A);
    expect(blendFrames(A, B, -1, C, C)).toBe(A);
    expect(blendFrames(A, B, 1, C, C)).toBe(B);
    expect(blendFrames(A, B, 2, C, C)).toBe(B);
  });

  it("moves every dot between its two ends (a shrinking ring stays between radius 15 and 30)", () => {
    const mid = blendFrames(A, B, 0.5, C, C);
    expect(mid.dots).toHaveLength(40);
    for (const d of mid.dots) {
      const rad = Math.hypot(d.x - C, d.y - C);
      expect(rad).toBeGreaterThan(15 - 1e-6);
      expect(rad).toBeLessThan(30 + 1e-6);
      expect(d.r).toBeGreaterThan(2);
      expect(d.r).toBeLessThan(3);
    }
    const avg = mid.dots.reduce((s, d) => s + Math.hypot(d.x - C, d.y - C), 0) / mid.dots.length;
    expect(avg).toBeCloseTo(22.5, 0);
  });

  it("progress is monotonic: the average radius shrinks steadily from 30 to 15", () => {
    let prev = 31;
    for (let k = 0.05; k < 1; k += 0.05) {
      const f = blendFrames(A, B, k, C, C);
      const avg = f.dots.reduce((s, d) => s + Math.hypot(d.x - C, d.y - C), 0) / f.dots.length;
      expect(avg).toBeLessThanOrEqual(prev + 1e-9);
      prev = avg;
    }
  });

  it("different dot counts: extra dots fade so the total brightness stays level", () => {
    const few = frame(ring(10, 20));
    const many = frame(ring(40, 20));
    const mid = blendFrames(few, many, 0.5, C, C);
    expect(mid.dots).toHaveLength(40);
    const total = mid.dots.reduce((s, d) => s + (d.a ?? 1), 0);
    expect(total).toBeGreaterThan(10);
    expect(total).toBeLessThan(40);
    const start = blendFrames(few, many, 0.001, C, C);
    expect(start.dots.filter((d) => (d.a ?? 1) > 0.5)).toHaveLength(10);
    const back = blendFrames(many, few, 0.5, C, C);
    expect(back.dots).toHaveLength(40);
  });

  it("an empty shape fades the other one in or out instead of crashing", () => {
    const empty = frame([]);
    const inn = blendFrames(empty, A, 0.5, C, C);
    expect(inn.dots).toHaveLength(40);
    expect(inn.dots.every((d) => (d.a ?? 1) <= 0.5 + 1e-9)).toBe(true);
    const out = blendFrames(A, empty, 0.25, C, C);
    expect(out.dots).toHaveLength(40);
    expect(out.dots.every((d) => (d.a ?? 1) > 0.5)).toBe(true);
  });

  it("edges fade out of the old shape and into the new one and never travel", () => {
    const line = { x1: 0, y1: 0, x2: 10, y2: 10, white: 0, a: 1, w: 1 };
    const withLines = frame(ring(5, 20), [line]);
    const none = frame(ring(5, 20));
    const outgoing = blendFrames(withLines, none, 0.5, C, C);
    expect(outgoing.lines).toHaveLength(1);
    expect(outgoing.lines[0].a).toBeCloseTo(0.5, 6);
    expect(outgoing.lines[0].x2).toBe(10);
    const incoming = blendFrames(none, withLines, 0.5, C, C);
    expect(incoming.lines[0].a).toBeCloseTo(0.5, 6);
  });

  it("does not mutate its inputs and returns far-to-near order", () => {
    const a = frame(ring(8, 20, 0.2));
    const b = frame(ring(8, 10, -0.2));
    const before = JSON.stringify([a, b]);
    const out = blendFrames(a, b, 0.4, C, C);
    expect(JSON.stringify([a, b])).toBe(before);
    for (let i = 1; i < out.dots.length; i++) expect(out.dots[i].z).toBeGreaterThanOrEqual(out.dots[i - 1].z);
  });

  it("every pair of real designs blends to finite numbers at every step", () => {
    for (const from of ORB_DESIGNS) {
      for (const to of ORB_DESIGNS) {
        if (from === to) continue;
        const fa = frameFor(from, 100, 1.3);
        const fb = frameFor(to, 100, 1.3);
        for (const k of [0.1, 0.5, 0.9]) {
          const m = blendFrames(fa, fb, k, 50, 50);
          for (const d of m.dots) {
            expect(Number.isFinite(d.x) && Number.isFinite(d.y) && Number.isFinite(d.r) && Number.isFinite(d.white), `${from}->${to}`).toBe(true);
            expect(d.a ?? 1).toBeGreaterThanOrEqual(0);
            expect(d.a ?? 1).toBeLessThanOrEqual(1.0001);
          }
        }
      }
    }
  });
});

describe("circle design (idle)", () => {
  it("is evenly spaced on one constant radius around the centre", () => {
    const f = frameCircle(100, 0, {});
    expect(f.lines).toHaveLength(0);
    expect(f.dots).toHaveLength(circleDotCount(100));
    const radius = CIRCLE_RADIUS_FRAC * 100;
    for (const d of f.dots) expect(Math.hypot(d.x - 50, d.y - 50)).toBeCloseTo(radius, 6);
    const n = f.dots.length;
    const step = (Math.PI * 2) / n;
    for (let i = 0; i < n; i++) {
      const a = Math.atan2(f.dots[i].y - 50, f.dots[i].x - 50);
      const b = Math.atan2(f.dots[(i + 1) % n].y - 50, f.dots[(i + 1) % n].x - 50);
      let delta = b - a;
      if (delta < 0) delta += Math.PI * 2;
      expect(delta).toBeCloseTo(step, 6);
    }
  });

  it("keeps its shape for all time (only a faint brightness shimmer moves) and is deterministic", () => {
    const a = frameCircle(100, 0, {});
    const b = frameCircle(100, 37.2, {});
    for (let i = 0; i < a.dots.length; i++) {
      expect(b.dots[i].x).toBe(a.dots[i].x);
      expect(b.dots[i].y).toBe(a.dots[i].y);
      expect(b.dots[i].r).toBe(a.dots[i].r);
    }
    const whites = b.dots.map((d) => d.white);
    expect(Math.max(...whites) - Math.min(...whites)).toBeLessThan(0.25);
    expect(JSON.stringify(frameCircle(100, 5, {}))).toBe(JSON.stringify(frameCircle(100, 5, {})));
  });

  it("scales with size and never has fewer than 24 dots", () => {
    expect(circleDotCount(20)).toBeGreaterThanOrEqual(24);
    expect(circleDotCount(200)).toBeGreaterThanOrEqual(circleDotCount(100));
  });
});
