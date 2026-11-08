import { describe, expect, it } from "vitest";
import { resolveOrb } from "./resolveOrb";
import { ATTACK_TAU, LEVEL_GATE, RELEASE_TAU, smoothLevel, voiceClockStep, wobbleScale } from "./voiceClock";

describe("voiceClockStep (moves only when sound is heard)", () => {
  it("is exactly zero in silence and below the gate", () => {
    expect(voiceClockStep(0.016, 0)).toBe(0);
    expect(voiceClockStep(0.016, LEVEL_GATE - 0.001)).toBe(0);
    expect(voiceClockStep(0.016, Number.NaN)).toBe(0);
    expect(voiceClockStep(0.016, -1)).toBe(0);
  });

  it("advances when there is sound, and faster when it is louder", () => {
    const quiet = voiceClockStep(0.016, 0.1);
    const loud = voiceClockStep(0.016, 0.9);
    expect(quiet).toBeGreaterThan(0);
    expect(loud).toBeGreaterThan(quiet);
  });

  it("scales with the frame time and never goes backwards", () => {
    expect(voiceClockStep(0.032, 0.5)).toBeCloseTo(2 * voiceClockStep(0.016, 0.5), 10);
    expect(voiceClockStep(-1, 0.5)).toBe(0);
  });
});

describe("smoothLevel (follows the mic)", () => {
  it("rises quickly: most of a step within a few frames", () => {
    let l = 0;
    for (let i = 0; i < 4; i++) l = smoothLevel(l, 1, 0.016);
    expect(l).toBeGreaterThan(0.6);
    expect(ATTACK_TAU).toBeLessThan(RELEASE_TAU);
  });

  it("falls slower than it rises, and settles to zero", () => {
    let up = 0;
    let down = 1;
    for (let i = 0; i < 6; i++) {
      up = smoothLevel(up, 1, 0.016);
      down = smoothLevel(down, 0, 0.016);
    }
    expect(1 - down).toBeLessThan(up);
    let l = 1;
    for (let i = 0; i < 200; i++) l = smoothLevel(l, 0, 0.016);
    expect(l).toBeLessThan(0.001);
  });

  it("clamps and ignores junk input", () => {
    expect(smoothLevel(0.5, 5, 0.5)).toBeLessThanOrEqual(1);
    expect(smoothLevel(0.5, Number.NaN, 0.1)).toBeLessThan(0.5);
    expect(smoothLevel(0.3, 0.3, 0.016)).toBeCloseTo(0.3, 10);
    expect(smoothLevel(0.2, 0.9, 0)).toBe(0.2);
  });

  it("a 12 Hz mic feed with gaps still gives a continuous level (no flicker to zero between updates)", () => {
    // mic reports 0.7 every 83 ms; the orb samples at 60 fps and keeps the last value
    let level = 0;
    let minAfterWarmup = 1;
    for (let frame = 0; frame < 120; frame++) {
      level = smoothLevel(level, 0.7, 0.0166);
      if (frame > 10) minAfterWarmup = Math.min(minAfterWarmup, level);
    }
    expect(minAfterWarmup).toBeGreaterThan(0.65);
  });
});

describe("wobbleScale", () => {
  it("is small in silence, 1.5 at full voice and monotonic", () => {
    expect(wobbleScale(0)).toBeCloseTo(0.15, 10);
    expect(wobbleScale(1)).toBeCloseTo(1.5, 10);
    expect(wobbleScale(9)).toBeCloseTo(1.5, 10);
    let prev = 0;
    for (let l = 0; l <= 1.0001; l += 0.05) {
      expect(wobbleScale(l)).toBeGreaterThanOrEqual(prev);
      prev = wobbleScale(l);
    }
  });
});

describe("the ring is rounder when the wobble is scaled down", () => {
  const radialSpread = (wob: number) => {
    const r = resolveOrb("breathing", 100);
    expect(typeof r.opts.wobMul).toBe("number");
    const f = r.frame(100, 1.7, { ...r.opts, wobMul: (r.opts.wobMul ?? 1) * wob });
    const radii = f.dots.map((d) => Math.hypot(d.x - 50, d.y - 50));
    const mean = radii.reduce((a, b) => a + b, 0) / radii.length;
    return Math.sqrt(radii.reduce((a, b) => a + (b - mean) ** 2, 0) / radii.length);
  };
  it("silence (0.15x) is measurably rounder than a loud voice (1.5x)", () => {
    expect(radialSpread(wobbleScale(0))).toBeLessThan(radialSpread(wobbleScale(1)));
  });
});
