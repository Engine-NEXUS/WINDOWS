import { describe, expect, it } from "vitest";
import { frameCircle, frameSquare, frameTriangle, polygonPoint } from "./dotted/nexusModes";
import { handSignalsFor } from "./orbSignals";
import { designFor } from "./orbActivity";
import { resolveOrb } from "./dotted/resolveOrb";

describe("hand-mode shapes", () => {
  it("each shape has as many dots as the circle, all inside the canvas", () => {
    for (const f of [frameTriangle, frameSquare]) {
      const fr = f(120, 0.5, {});
      expect(fr.dots.length).toBe(frameCircle(120, 0.5, {}).dots.length);
      for (const d of fr.dots) {
        expect(d.x).toBeGreaterThan(0);
        expect(d.x).toBeLessThan(120);
        expect(d.y).toBeGreaterThan(0);
        expect(d.y).toBeLessThan(120);
      }
    }
  });

  it("the triangle points up and the square is axis-aligned", () => {
    const tri = frameTriangle(100, 0, {}).dots;
    expect(Math.min(...tri.map((d) => d.y))).toBeCloseTo(50 - 36, 0);
    expect(tri[0].x).toBeCloseTo(50, 0);
    const sq = frameSquare(100, 0, {}).dots;
    const xs = sq.map((d) => d.x);
    const ys = sq.map((d) => d.y);
    expect(Math.max(...xs) - Math.min(...xs)).toBeCloseTo(Math.max(...ys) - Math.min(...ys), 0);
  });

  it("polygonPoint walks the outline by arc length and wraps", () => {
    const sq = [[0, 0], [1, 0], [1, 1], [0, 1]] as const;
    expect(polygonPoint(sq, 0)).toEqual([0, 0]);
    const q = polygonPoint(sq, 0.125);
    expect(q[0]).toBeCloseTo(0.5);
    expect(q[1]).toBeCloseTo(0);
    expect(polygonPoint(sq, 1)[0]).toBeCloseTo(0);
    expect(polygonPoint(sq, 1.25)[0]).toBeCloseTo(polygonPoint(sq, 0.25)[0]);
  });

  it("the three hand activities use circle, triangle and square", () => {
    expect(designFor("hand")).toBe("circle");
    expect(designFor("handArmed")).toBe("triangle");
    expect(designFor("handPaused")).toBe("square");
    expect(resolveOrb("triangle", 100).design).toBe("triangle");
    expect(resolveOrb("square", 100).design).toBe("square");
  });
});

describe("hand:state to orb signals", () => {
  it("armed means the triangle", () => {
    expect(handSignalsFor({ armed: true }, false)).toEqual({ handActive: true, handArmed: true, handPaused: false });
  });
  it("a held palm makes the square, which stays until the pointer is armed again", () => {
    const paused = handSignalsFor({ armed: false, note: "disarmed (palm-hold)" }, false);
    expect(paused).toEqual({ handActive: true, handArmed: false, handPaused: true });
    expect(handSignalsFor({ armed: false, note: "" }, paused.handPaused).handPaused).toBe(true);
    expect(handSignalsFor({ armed: true }, true).handPaused).toBe(false);
  });
  it("other ways of switching off, and a fresh start, show the circle", () => {
    expect(handSignalsFor({ armed: false, note: "disarmed (hand-lost)" }, false).handPaused).toBe(false);
    expect(handSignalsFor(null, false)).toEqual({ handActive: true, handArmed: false, handPaused: false });
  });
});
