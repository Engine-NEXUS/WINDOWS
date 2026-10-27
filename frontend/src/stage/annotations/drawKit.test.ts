import { describe, expect, it } from "vitest";
import { Rect, Size } from "../tourGeometry";
import {
  bracketPaths,
  connectPath,
  DRAW_STYLES,
  ellipseFor,
  highlightRect,
  lensGeometry,
  normalizeDraw,
  pinPos,
  spotlightPath,
  underlinePath,
} from "./drawKit";

describe("drawKit model", () => {
  const screen: Size = { w: 1920, h: 1080 };
  const normalTarget: Rect = { x: 200, y: 200, w: 300, h: 150 };
  const tinyTarget: Rect = { x: 50, y: 50, w: 6, h: 6 };
  const cornerTarget: Rect = { x: 0, y: 0, w: 100, h: 80 };
  const bottomCornerTarget: Rect = { x: 1820, y: 1000, w: 100, h: 80 };

  describe("DRAW_STYLES", () => {
    it("contains all 11 required draw styles", () => {
      expect(DRAW_STYLES).toEqual([
        "ring",
        "underline",
        "bracket",
        "circle",
        "arrow",
        "highlight",
        "spotlight",
        "scan",
        "lens",
        "connect",
        "pin",
      ]);
    });
  });

  describe("normalizeDraw", () => {
    it("returns ring when target is null regardless of raw or category", () => {
      expect(normalizeDraw(undefined, undefined, null, screen)).toBe("ring");
      expect(normalizeDraw("pin", undefined, null, screen)).toBe("ring");
      expect(normalizeDraw("lens", "ui_element", null, screen)).toBe("ring");
    });

    it("accepts valid raw styles case-insensitively with trimming", () => {
      expect(normalizeDraw("  ring  ", undefined, normalTarget, screen)).toBe("ring");
      expect(normalizeDraw("UNDERLINE", undefined, normalTarget, screen)).toBe("underline");
      expect(normalizeDraw("bracket", undefined, normalTarget, screen)).toBe("bracket");
      expect(normalizeDraw("Circle", undefined, normalTarget, screen)).toBe("circle");
      expect(normalizeDraw("arrow", undefined, normalTarget, screen)).toBe("arrow");
      expect(normalizeDraw("highlight", undefined, normalTarget, screen)).toBe("highlight");
      expect(normalizeDraw("scan", undefined, normalTarget, screen)).toBe("scan");
      expect(normalizeDraw("pin", undefined, normalTarget, screen)).toBe("pin");
    });

    it("downgrades connect to ring when opts.hasLink is missing or false", () => {
      expect(normalizeDraw("connect", undefined, normalTarget, screen)).toBe("ring");
      expect(normalizeDraw("connect", undefined, normalTarget, screen, { hasLink: false })).toBe("ring");
      expect(normalizeDraw("connect", undefined, normalTarget, screen, { hasLink: true })).toBe("connect");
    });

    it("downgrades lens to ring when opts.hasThumb is missing or false", () => {
      expect(normalizeDraw("lens", undefined, normalTarget, screen)).toBe("ring");
      expect(normalizeDraw("lens", undefined, normalTarget, screen, { hasThumb: false })).toBe("ring");
      expect(normalizeDraw("lens", undefined, normalTarget, screen, { hasThumb: true })).toBe("lens");
    });

    it("downgrades lens to ring when target area exceeds 30% of screen", () => {
      // 1920 * 1080 = 2,073,600. 30% is 622,080.
      const largeTarget: Rect = { x: 100, y: 100, w: 1000, h: 700 }; // 700,000 > 30%
      expect(normalizeDraw("lens", undefined, largeTarget, screen, { hasThumb: true })).toBe("ring");
    });

    it("downgrades spotlight to ring when target area exceeds 60% of screen", () => {
      // 60% of 2,073,600 is 1,244,160
      const moderateTarget: Rect = { x: 50, y: 50, w: 800, h: 600 }; // 480,000 < 60%
      expect(normalizeDraw("spotlight", undefined, moderateTarget, screen)).toBe("spotlight");

      const hugeTarget: Rect = { x: 50, y: 50, w: 1500, h: 900 }; // 1,350,000 > 60%
      expect(normalizeDraw("spotlight", undefined, hugeTarget, screen)).toBe("ring");
    });

    it("falls back to category when raw is invalid or missing", () => {
      expect(normalizeDraw(undefined, "ui_element", normalTarget, screen)).toBe("arrow");
      expect(normalizeDraw("", "UI_ELEMENT", normalTarget, screen)).toBe("arrow");
      expect(normalizeDraw("unknown_style", "text_block", normalTarget, screen)).toBe("highlight");
      expect(normalizeDraw(undefined, "code", normalTarget, screen)).toBe("highlight");
      expect(normalizeDraw(undefined, "chart", normalTarget, screen)).toBe("bracket");
      expect(normalizeDraw(undefined, "diagram", normalTarget, screen)).toBe("bracket");
      expect(normalizeDraw(undefined, "person", normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, "product", normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, "food", normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, "object", normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, "other", normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, undefined, normalTarget, screen)).toBe("ring");
      expect(normalizeDraw(undefined, "unrecognized_category", normalTarget, screen)).toBe("ring");
    });
  });

  describe("pure geometry builders", () => {
    const boxes = [
      { name: "normal", box: normalTarget },
      { name: "tiny (6x6)", box: tinyTarget },
      { name: "corner (0,0)", box: cornerTarget },
      { name: "bottom corner", box: bottomCornerTarget },
    ];

    function assertFiniteNums(str: string) {
      const nums = str.match(/-?\d+(\.\d+)?/g);
      expect(nums).not.toBeNull();
      nums?.forEach((n) => {
        expect(Number.isFinite(parseFloat(n))).toBe(true);
      });
    }

    boxes.forEach(({ name, box }) => {
      it(`underlinePath returns non-empty string with finite numbers for ${name} box`, () => {
        const p = underlinePath(box);
        expect(typeof p).toBe("string");
        expect(p.length).toBeGreaterThan(0);
        expect(p.startsWith("M ")).toBe(true);
        assertFiniteNums(p);
      });

      it(`bracketPaths returns non-empty left and right bracket paths for ${name} box`, () => {
        const b = bracketPaths(box);
        expect(typeof b.left).toBe("string");
        expect(typeof b.right).toBe("string");
        expect(b.left.length).toBeGreaterThan(0);
        expect(b.right.length).toBeGreaterThan(0);
        expect(b[0]).toBe(b.left);
        expect(b[1]).toBe(b.right);
        assertFiniteNums(b.left);
        assertFiniteNums(b.right);
      });

      it(`ellipseFor returns finite numbers with rx/ry padded 14% for ${name} box`, () => {
        const el = ellipseFor(box);
        expect(Number.isFinite(el.cx)).toBe(true);
        expect(Number.isFinite(el.cy)).toBe(true);
        expect(Number.isFinite(el.rx)).toBe(true);
        expect(Number.isFinite(el.ry)).toBe(true);
        expect(el.rx).toBeGreaterThan(box.w / 2);
        expect(el.ry).toBeGreaterThan(box.h / 2);
      });

      it(`highlightRect returns finite inset rect for ${name} box`, () => {
        const hr = highlightRect(box);
        expect(Number.isFinite(hr.x)).toBe(true);
        expect(Number.isFinite(hr.y)).toBe(true);
        expect(Number.isFinite(hr.w)).toBe(true);
        expect(Number.isFinite(hr.h)).toBe(true);
        expect(hr.w).toBeGreaterThan(0);
        expect(hr.h).toBeGreaterThan(0);
      });

      it(`pinPos returns finite top-left coordinates for ${name} box`, () => {
        const pin = pinPos(box);
        expect(Number.isFinite(pin.x)).toBe(true);
        expect(Number.isFinite(pin.y)).toBe(true);
        expect(pin.x).toBe(box.x);
        expect(pin.y).toBe(box.y);
      });

      it(`connectPath returns non-empty cubic Bézier with finite numbers for ${name} box`, () => {
        const other: Rect = { x: 600, y: 500, w: 100, h: 80 };
        const cp = connectPath(box, other);
        expect(typeof cp).toBe("string");
        expect(cp.startsWith("M ")).toBe(true);
        expect(cp.includes(" C ")).toBe(true);
        assertFiniteNums(cp);
      });

      it(`spotlightPath returns valid two-sub-path SVG path for ${name} box`, () => {
        const sp = spotlightPath(screen, box);
        expect(typeof sp).toBe("string");
        assertFiniteNums(sp);
        // Spotlight path must contain two subpaths: full viewport rect + rounded-rect hole
        const mMatches = sp.match(/M\s/g);
        const zMatches = sp.match(/Z/g);
        expect(mMatches?.length).toBe(2);
        expect(zMatches?.length).toBe(2);
      });

      it(`lensGeometry keeps the bubble inside viewport with margin for ${name} box`, () => {
        const lg = lensGeometry(box, screen, 190);
        expect(Number.isFinite(lg.cx)).toBe(true);
        expect(Number.isFinite(lg.cy)).toBe(true);
        expect(Number.isFinite(lg.r)).toBe(true);
        expect(Number.isFinite(lg.connectorStart.x)).toBe(true);
        expect(Number.isFinite(lg.connectorStart.y)).toBe(true);
        expect(Number.isFinite(lg.connectorEnd.x)).toBe(true);
        expect(Number.isFinite(lg.connectorEnd.y)).toBe(true);

        const margin = 16;
        expect(lg.cx - lg.r).toBeGreaterThanOrEqual(margin - 0.1);
        expect(lg.cx + lg.r).toBeLessThanOrEqual(screen.w - margin + 0.1);
        expect(lg.cy - lg.r).toBeGreaterThanOrEqual(margin - 0.1);
        expect(lg.cy + lg.r).toBeLessThanOrEqual(screen.h - margin + 0.1);
      });
    });
  });
});
