import { describe, expect, it } from "vitest";
import { DEFAULT_WAVE, easeOutCubic, farthestCorner, totalMs, waveFrame } from "./analyseWaveModel";

const SIZE = { w: 1920, h: 1080 };
const ORIGIN = { x: 960, y: 52 };
const THREE = { ...DEFAULT_WAVE, rings: 3 }; // the model supports a multi-ring burst; the shipped default is one ring

describe("analyse wave model", () => {
  it("ships the owner's reviewed settings: one ring, slow (1400 ms at 0.25x = 5600 ms), strength 80", () => {
    expect(DEFAULT_WAVE).toMatchObject({ rings: 1, durationMs: 5600, amp: 0.8 });
    expect(waveFrame(DEFAULT_WAVE.leadMs + 10, ORIGIN, SIZE).rings).toHaveLength(1);
    expect(waveFrame(DEFAULT_WAVE.leadMs + 300, ORIGIN, SIZE).rings).toHaveLength(1);
  });

  it("draws nothing before the first ring leaves (lead pause)", () => {
    const f = waveFrame(DEFAULT_WAVE.leadMs - 1, ORIGIN, SIZE);
    expect(f.rings).toHaveLength(0);
    expect(f.done).toBe(false);
  });

  it("launches the rings one after another", () => {
    const { leadMs, staggerMs } = THREE;
    expect(waveFrame(leadMs + 10, ORIGIN, SIZE, false, THREE).rings).toHaveLength(1);
    expect(waveFrame(leadMs + staggerMs + 10, ORIGIN, SIZE, false, THREE).rings).toHaveLength(2);
    expect(waveFrame(leadMs + staggerMs * 2 + 10, ORIGIN, SIZE, false, THREE).rings).toHaveLength(3);
  });

  it("the front grows, thins and fades, and later rings are fainter", () => {
    const a = waveFrame(300, ORIGIN, SIZE).rings[0];
    const b = waveFrame(900, ORIGIN, SIZE).rings[0];
    expect(b.radius).toBeGreaterThan(a.radius);
    expect(b.thick).toBeLessThan(a.thick);
    expect(b.alpha).toBeLessThan(a.alpha);
    const three = waveFrame(THREE.leadMs + THREE.staggerMs * 2 + 200, ORIGIN, SIZE, false, THREE).rings;
    expect(three[0].alpha).toBeGreaterThan(three[2].alpha);
  });

  it("reaches the farthest corner by the end of the first ring", () => {
    const rMax = farthestCorner(ORIGIN.x, ORIGIN.y, SIZE.w, SIZE.h);
    const nearEnd = waveFrame(DEFAULT_WAVE.leadMs + DEFAULT_WAVE.durationMs - 5, ORIGIN, SIZE).rings[0];
    expect(nearEnd.radius).toBeGreaterThan(rMax * 0.99);
    // the corner distance really exceeds the screen diagonal's half, so the front leaves the screen
    expect(rMax).toBeGreaterThan(Math.hypot(SIZE.w / 2, SIZE.h / 2) * 0.9);
  });

  it("is done exactly when the last ring has finished, and draws nothing after", () => {
    const end = totalMs();
    expect(waveFrame(end - 1, ORIGIN, SIZE).done).toBe(false);
    const f = waveFrame(end, ORIGIN, SIZE);
    expect(f.done).toBe(true);
    expect(f.rings).toHaveLength(0);
    expect(waveFrame(end + 5000, ORIGIN, SIZE).done).toBe(true);
  });

  it("an orb docked at the bottom still reaches the far corner", () => {
    expect(farthestCorner(960, 1028, 1920, 1080)).toBeGreaterThan(1000);
  });

  it("reduced motion never travels: no rings, one wash that rises and falls", () => {
    const mid = waveFrame(DEFAULT_WAVE.leadMs + 300, ORIGIN, SIZE, true);
    expect(mid.rings).toHaveLength(0);
    expect(mid.wash).toBeGreaterThan(0.1);
    expect(mid.wash).toBeLessThan(0.2);
    expect(waveFrame(DEFAULT_WAVE.leadMs + 1, ORIGIN, SIZE, true).wash).toBeLessThan(0.01);
    expect(waveFrame(totalMs(DEFAULT_WAVE, true), ORIGIN, SIZE, true).done).toBe(true);
  });

  it("easing is clamped and monotonic", () => {
    expect(easeOutCubic(-1)).toBe(0);
    expect(easeOutCubic(2)).toBe(1);
    expect(easeOutCubic(0.5)).toBeGreaterThan(easeOutCubic(0.25));
  });
});
