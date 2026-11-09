import { describe, expect, it } from "vitest";
import { asMode, clampScale, islandSize, PANEL, PILL_H, pillWidth, SCALE_MAX, SCALE_MIN } from "./agentIslandModel";

describe("island size (mirrors agent_island.rs size_for)", () => {
  it("the pill grows with the logos, has a minimum and a cap of eight", () => {
    expect(pillWidth(1)).toBe(64);
    expect(pillWidth(0)).toBe(pillWidth(1));
    expect(pillWidth(3)).toBe(28 + 32 * 3);
    expect(pillWidth(3)).toBeGreaterThan(pillWidth(2));
    expect(pillWidth(50)).toBe(pillWidth(8));
  });

  it("the panel is a fixed size and the pill is one row tall", () => {
    expect(islandSize("expanded", 3)).toEqual({ w: PANEL.w, h: PANEL.h });
    expect(islandSize("collapsed", 2)).toEqual({ w: pillWidth(2), h: PILL_H });
  });
});

describe("size setting", () => {
  it("stays within the limits and falls back to the default for junk", () => {
    expect(clampScale(1.2)).toBe(1.2);
    expect(clampScale(9)).toBe(SCALE_MAX);
    expect(clampScale(0.1)).toBe(SCALE_MIN);
    expect(clampScale("big")).toBe(0.75);
    expect(clampScale(Number.NaN)).toBe(0.75);
    expect(clampScale(undefined)).toBe(0.75);
  });

  it("is read from the mode payload and clamped there too", () => {
    expect(asMode({ mode: "collapsed", byApproval: false, scale: 1.25 }).scale).toBe(1.25);
    expect(asMode({ mode: "collapsed", scale: 40 }).scale).toBe(SCALE_MAX);
    expect(asMode({ mode: "expanded" }).scale).toBe(0.75);
    expect(asMode(null).scale).toBe(0.75);
  });
});
