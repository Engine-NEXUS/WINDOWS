import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { DottedOrb } from "./DottedOrb";
import { inkColor, parseInk, WHITE_INK, LEVEL_RADIUS_GAIN, paintDots } from "./paintInk";
import { densityFor, frameFor, ORB_DESIGNS, resolveOrb } from "./resolveOrb";

describe("inkColor", () => {
  it("dark substrate mirrors the ink value so near dots read bright, tinted by the ink colour", () => {
    expect(inkColor(0, 1, true, WHITE_INK)).toBe("rgba(255,255,255,1)");
    expect(inkColor(1, 1, true, WHITE_INK)).toBe("rgba(0,0,0,1)");
    expect(inkColor(0.5, 0.4, true, [200, 100, 0])).toBe("rgba(100,50,0,0.4)");
  });

  it("light substrate keeps dark grayscale ink regardless of the tint (black ink in light theme)", () => {
    expect(inkColor(0, 1, false, [200, 100, 0])).toBe("rgba(0,0,0,1)");
    expect(inkColor(1, 1, false, [200, 100, 0])).toBe("rgba(255,255,255,1)");
  });

  it("clamps out-of-range ink values", () => {
    expect(inkColor(-3, 1, true, WHITE_INK)).toBe("rgba(255,255,255,1)");
    expect(inkColor(9, 1, true, WHITE_INK)).toBe("rgba(0,0,0,1)");
  });
});

describe("parseInk", () => {
  it("parses #rrggbb and falls back to white for anything else", () => {
    expect(parseInk("#ff8000")).toEqual([255, 128, 0]);
    expect(parseInk("ff8000")).toEqual([255, 128, 0]);
    expect(parseInk("#fff")).toEqual(WHITE_INK);
    expect(parseInk("")).toEqual(WHITE_INK);
    expect(parseInk(null)).toEqual(WHITE_INK);
    expect(parseInk("red")).toEqual(WHITE_INK);
  });
});

describe("paintDots voice level", () => {
  function recorder() {
    const radii: number[] = [];
    const ctx = {
      fillStyle: "",
      beginPath() {},
      arc(_x: number, _y: number, r: number) {
        radii.push(r);
      },
      fill() {},
    } as unknown as CanvasRenderingContext2D;
    return { ctx, radii };
  }
  const dots = [{ x: 1, y: 1, z: 0, r: 2, white: 0 }];

  it("swells dots with level and leaves them alone at level 0", () => {
    const quiet = recorder();
    paintDots(quiet.ctx, dots, true, WHITE_INK, 0, 1);
    const loud = recorder();
    paintDots(loud.ctx, dots, true, WHITE_INK, 1, 1);
    expect(quiet.radii[0]).toBe(2);
    expect(loud.radii[0]).toBeCloseTo(2 * (1 + LEVEL_RADIUS_GAIN), 6);
  });

  it("skips fully faded dots", () => {
    const r = recorder();
    paintDots(r.ctx, dots, true, WHITE_INK, 0, 0.01);
    expect(r.radii).toHaveLength(0);
  });
});

describe("resolveOrb / frames", () => {
  it("covers all nine designs", () => {
    expect(ORB_DESIGNS).toHaveLength(9);
    expect(new Set(ORB_DESIGNS).size).toBe(9);
  });

  it("every design yields marks at the capsule size, deterministically", () => {
    for (const d of ORB_DESIGNS) {
      const a = frameFor(d, 120, 1.7);
      const b = frameFor(d, 120, 1.7);
      expect(a.dots.length + a.lines.length, d).toBeGreaterThan(0);
      expect(JSON.stringify(a), d).toBe(JSON.stringify(b));
      for (const dot of a.dots) {
        expect(Number.isFinite(dot.x) && Number.isFinite(dot.y) && Number.isFinite(dot.r), d).toBe(true);
      }
    }
  });

  it("is denser than the 64 px tuning at larger sizes but never past the dense base profile", () => {
    expect(densityFor(64)).toBe(1);
    expect(densityFor(20)).toBe(1);
    expect(densityFor(128)).toBeGreaterThan(1);
    expect(densityFor(4000)).toBeLessThanOrEqual(4);
    const small = frameFor("searching", 64, 1).dots.length;
    const big = frameFor("searching", 140, 1).dots.length;
    expect(big).toBeGreaterThan(small);
  });

  it("caches resolutions per design and size", () => {
    expect(resolveOrb("weaving", 120)).toBe(resolveOrb("weaving", 120));
    expect(resolveOrb("weaving", 120)).not.toBe(resolveOrb("weaving", 140));
  });
});

describe("DottedOrb markup", () => {
  it("renders an accessible canvas with a per-design label", () => {
    const html = renderToStaticMarkup(<DottedOrb design="searching" size={100} />);
    expect(html).toContain("<canvas");
    expect(html).toContain('role="img"');
    expect(html).toContain('aria-label="Searching"');
  });

  it("lets the caller override the label", () => {
    const html = renderToStaticMarkup(<DottedOrb design="breathing" size={100} aria-label="Ready" />);
    expect(html).toContain('aria-label="Ready"');
  });
});
