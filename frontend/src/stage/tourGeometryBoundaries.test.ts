import { describe, expect, it } from "vitest";
import {
  arrowHead,
  motionFor,
  physToCss,
  visibleTarget,
  type Rect,
  type Size,
} from "./tourGeometry";

const SCREEN: Size = { w: 1920, h: 1080 };

describe("visibleTarget boundaries", () => {
  it("rejects non-finite or missing targets and zero screen sizes", () => {
    expect(visibleTarget(null, SCREEN)).toBeNull();
    expect(visibleTarget({ x: 0, y: 0, w: 100, h: 100 }, { w: 0, h: 1080 })).toBeNull();
    expect(visibleTarget({ x: 0, y: 0, w: 100, h: 100 }, { w: 1920, h: -5 })).toBeNull();
    expect(visibleTarget({ x: Number.NaN, y: 0, w: 50, h: 50 }, SCREEN)).toBeNull();
    expect(visibleTarget({ x: 0, y: Infinity, w: 50, h: 50 }, SCREEN)).toBeNull();
  });

  it("rejects targets smaller than 16px minimum in either dimension", () => {
    expect(visibleTarget({ x: 10, y: 10, w: 15, h: 50 }, SCREEN)).toBeNull();
    expect(visibleTarget({ x: 10, y: 10, w: 50, h: 15 }, SCREEN)).toBeNull();
    expect(visibleTarget({ x: 10, y: 10, w: 16, h: 16 }, SCREEN)).not.toBeNull();
  });

  it("rejects completely offscreen boxes", () => {
    // Completely to the left
    expect(visibleTarget({ x: -100, y: 100, w: 50, h: 50 }, SCREEN)).toBeNull();
    // Completely to the right
    expect(visibleTarget({ x: 2000, y: 100, w: 50, h: 50 }, SCREEN)).toBeNull();
    // Completely above
    expect(visibleTarget({ x: 100, y: -200, w: 50, h: 50 }, SCREEN)).toBeNull();
    // Completely below
    expect(visibleTarget({ x: 100, y: 1200, w: 50, h: 50 }, SCREEN)).toBeNull();
  });

  it("clips partially visible boxes on all four edges and enforces 16px remaining size", () => {
    // Left edge clip
    const leftClipped = visibleTarget({ x: -20, y: 100, w: 60, h: 60 }, SCREEN);
    expect(leftClipped).toEqual({ x: 0, y: 100, w: 40, h: 60 });

    // Top edge clip
    const topClipped = visibleTarget({ x: 100, y: -10, w: 50, h: 50 }, SCREEN);
    expect(topClipped).toEqual({ x: 100, y: 0, w: 50, h: 40 });

    // Right edge clip
    const rightClipped = visibleTarget({ x: 1900, y: 100, w: 50, h: 50 }, SCREEN);
    expect(rightClipped).toEqual({ x: 1900, y: 100, w: 20, h: 50 });

    // Bottom edge clip
    const bottomClipped = visibleTarget({ x: 100, y: 1060, w: 50, h: 50 }, SCREEN);
    expect(bottomClipped).toEqual({ x: 100, y: 1060, w: 50, h: 20 });

    // Clipping results in < 16px -> rejected
    expect(visibleTarget({ x: -40, y: 100, w: 50, h: 50 }, SCREEN)).toBeNull(); // 10px visible
  });
});

describe("motionFor category vocabulary and size cap", () => {
  it("enforces 35% screen area cap returning classic style for huge targets", () => {
    // Exactly 35% = 725,760
    const underCap: Rect = { x: 100, y: 100, w: 800, h: 800 }; // 640,000 area
    expect(motionFor("person", underCap, SCREEN)).toBe("ripple");

    const overCap: Rect = { x: 100, y: 100, w: 1000, h: 800 }; // 800,000 area (> 35%)
    expect(motionFor("person", overCap, SCREEN)).toBe("classic");
  });

  it("handles case-insensitivity and all category variants", () => {
    const t: Rect = { x: 100, y: 100, w: 200, h: 100 };
    expect(motionFor("PERSON", t, SCREEN)).toBe("ripple");
    expect(motionFor("Product", t, SCREEN)).toBe("ripple");
    expect(motionFor("Food", t, SCREEN)).toBe("ripple");
    expect(motionFor("OBJECT", t, SCREEN)).toBe("ripple");

    expect(motionFor("Text_Block", t, SCREEN)).toBe("scan");
    expect(motionFor("CODE", t, SCREEN)).toBe("scan");

    expect(motionFor("Chart", t, SCREEN)).toBe("trace");
    expect(motionFor("DIAGRAM", t, SCREEN)).toBe("trace");

    expect(motionFor("ui_element", t, SCREEN)).toBe("beam");
    expect(motionFor("UI_ELEMENT", t, SCREEN)).toBe("beam");

    expect(motionFor("unknown", t, SCREEN)).toBe("classic");
    expect(motionFor(undefined, t, SCREEN)).toBe("classic");
  });
});

describe("arrowHead geometry boundaries", () => {
  it("calculates arrowhead polygon pointing along vector", () => {
    const poly = arrowHead({ x: 0, y: 0 }, { x: 50, y: 0 }, 10, 5);
    const points = poly.split(" ");
    expect(points).toHaveLength(3);
    // Tip is at 50,0
    expect(points[0]).toBe("50,0");
  });

  it("handles identical tail and tip without NaN", () => {
    const poly = arrowHead({ x: 10, y: 10 }, { x: 10, y: 10 });
    expect(poly).not.toContain("NaN");
  });
});

describe("physToCss scaling boundaries", () => {
  it("handles zero monitor dimensions without dividing by zero", () => {
    const phys: Rect = { x: 10, y: 20, w: 30, h: 40 };
    const css = physToCss(phys, { w: 0, h: 0 }, { w: 1920, h: 1080 });
    expect(css).toEqual(phys);
  });

  it("scales physical coordinates down proportionally", () => {
    const phys: Rect = { x: 200, y: 100, w: 400, h: 200 };
    const css = physToCss(phys, { w: 3840, h: 2160 }, { w: 1920, h: 1080 });
    expect(css).toEqual({ x: 100, y: 50, w: 200, h: 100 });
  });
});
