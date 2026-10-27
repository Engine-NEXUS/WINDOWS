import { describe, it, expect } from "vitest";
import { lensBackground, zoomAt, clampPan } from "./lensMath";

describe("lensMath", () => {
  describe("zoomAt", () => {
    it("maintains anchor invariance before and after zoom (tolerance 1e-6)", () => {
      const testCases = [
        { view: { scale: 1, tx: 0, ty: 0 }, factor: 2.0, anchor: { x: 150, y: 100 } },
        { view: { scale: 1.5, tx: -60, ty: 40 }, factor: 1.8, anchor: { x: 250, y: 180 } },
        { view: { scale: 3.0, tx: -200, ty: -150 }, factor: 0.6, anchor: { x: 300, y: 220 } },
        { view: { scale: 2.5, tx: 100, ty: -80 }, factor: 1.25, anchor: { x: 0, y: 0 } },
        { view: { scale: 4.0, tx: -400, ty: -300 }, factor: 1.5, anchor: { x: 500, y: 400 } },
      ];

      for (const { view, factor, anchor } of testCases) {
        const next = zoomAt(view, factor, anchor, { min: 0.1, max: 20 });

        // Content point under anchor before zoom
        const cxBefore = (anchor.x - view.tx) / view.scale;
        const cyBefore = (anchor.y - view.ty) / view.scale;

        // Content point under anchor after zoom
        const cxAfter = (anchor.x - next.tx) / next.scale;
        const cyAfter = (anchor.y - next.ty) / next.scale;

        expect(Math.abs(cxBefore - cxAfter)).toBeLessThan(1e-6);
        expect(Math.abs(cyBefore - cyAfter)).toBeLessThan(1e-6);
      }
    });

    it("clamps scale to min and max limits", () => {
      const view = { scale: 2, tx: 50, ty: 50 };
      const anchor = { x: 100, y: 100 };

      // Zooming far out
      const zoomedOut = zoomAt(view, 0.01, anchor, { min: 1, max: 8 });
      expect(zoomedOut.scale).toBe(1);

      // Zooming far in
      const zoomedIn = zoomAt(view, 50, anchor, { min: 1, max: 8 });
      expect(zoomedIn.scale).toBe(8);
    });

    it("handles zero or non-finite scales safely without NaN", () => {
      const res = zoomAt({ scale: 0, tx: 0, ty: 0 }, 2, { x: 50, y: 50 });
      expect(Number.isFinite(res.scale)).toBe(true);
      expect(Number.isFinite(res.tx)).toBe(true);
      expect(Number.isFinite(res.ty)).toBe(true);
      expect(res.scale).toBe(1);
    });
  });

  describe("lensBackground", () => {
    it("keeps bubble fully inside the box for pointer at 4 corners and centre", () => {
      const box = { w: 400, h: 300 };
      const lensSize = 120;
      const zoom = 2.5;

      const cornersAndCenter = [
        { name: "top-left", pointer: { x: 0, y: 0 } },
        { name: "top-right", pointer: { x: 400, y: 0 } },
        { name: "bottom-left", pointer: { x: 0, y: 300 } },
        { name: "bottom-right", pointer: { x: 400, y: 300 } },
        { name: "centre", pointer: { x: 200, y: 150 } },
      ];

      for (const { name, pointer } of cornersAndCenter) {
        const result = lensBackground(pointer, box, zoom, lensSize);

        // Bubble stays fully inside the box
        expect(result.left, `${name} left >= 0`).toBeGreaterThanOrEqual(0);
        expect(result.top, `${name} top >= 0`).toBeGreaterThanOrEqual(0);
        expect(result.left + lensSize, `${name} right <= box.w`).toBeLessThanOrEqual(box.w);
        expect(result.top + lensSize, `${name} bottom <= box.h`).toBeLessThanOrEqual(box.h);

        // Background size is scaled by zoom
        expect(result.bgW).toBe(box.w * zoom);
        expect(result.bgH).toBe(box.h * zoom);

        // Magnified point under pointer is centered in the bubble
        // (bgX + pointer.x * zoom === lensSize / 2)
        expect(result.bgX).toBe(lensSize / 2 - pointer.x * zoom);
        expect(result.bgY).toBe(lensSize / 2 - pointer.y * zoom);
      }
    });

    it("centres bubble on pointer when far from edges", () => {
      const box = { w: 500, h: 500 };
      const lensSize = 100;
      const pointer = { x: 250, y: 250 };
      const result = lensBackground(pointer, box, 2, lensSize);

      expect(result.left).toBe(200); // 250 - 50
      expect(result.top).toBe(200); // 250 - 50
    });

    it("handles zero-size boxes without NaN", () => {
      const result = lensBackground({ x: 0, y: 0 }, { w: 0, h: 0 }, 2.5, 140);
      expect(Number.isFinite(result.bgX)).toBe(true);
      expect(Number.isFinite(result.bgY)).toBe(true);
      expect(Number.isFinite(result.bgW)).toBe(true);
      expect(Number.isFinite(result.bgH)).toBe(true);
      expect(Number.isFinite(result.left)).toBe(true);
      expect(Number.isFinite(result.top)).toBe(true);
      expect(result.left).toBe(0);
      expect(result.top).toBe(0);
    });
  });

  describe("clampPan", () => {
    const frame = { w: 600, h: 400 };
    const content = { w: 600, h: 400 };

    it("centres and locks pan at scale 1", () => {
      // Even if view.tx/ty are offset, at scale 1 they must lock to centre
      const view = { scale: 1, tx: 250, ty: -180 };
      const clamped = clampPan(view, content, frame);

      expect(clamped.tx).toBe(0); // (600 - 600) / 2
      expect(clamped.ty).toBe(0); // (400 - 400) / 2
    });

    it("centres image when content is smaller than frame at scale 1", () => {
      const smallContent = { w: 400, h: 200 };
      const view = { scale: 1, tx: 99, ty: -99 };
      const clamped = clampPan(view, smallContent, frame);

      expect(clamped.tx).toBe(100); // (600 - 400) / 2
      expect(clamped.ty).toBe(100); // (400 - 200) / 2
    });

    it("prevents image from being dragged beyond 25% visibility boundary", () => {
      const view = { scale: 2, tx: -9999, ty: 9999 };
      const clamped = clampPan(view, content, frame);

      const sw = content.w * view.scale; // 1200
      const sh = content.h * view.scale; // 800

      // Leftmost clamp: -0.75 * sw = -900
      expect(clamped.tx).toBe(-0.75 * sw);
      // Rightmost/bottom clamp: frame.h - 0.25 * sh = 400 - 200 = 200
      expect(clamped.ty).toBe(frame.h - 0.25 * sh);

      // Verify that at the clamp limit, at least 25% of image width is inside the frame
      // Image right edge is clamped.tx + sw = -900 + 1200 = 300
      // Frame width is 600. Visible width = 300, which is 300 / 1200 = 25% of sw!
      const visibleW = clamped.tx + sw;
      expect(visibleW / sw).toBeCloseTo(0.25, 5);
    });

    it("handles zero dimensions safely without NaN", () => {
      const res1 = clampPan({ scale: 1, tx: 0, ty: 0 }, { w: 0, h: 0 }, frame);
      expect(Number.isFinite(res1.tx)).toBe(true);
      expect(Number.isFinite(res1.ty)).toBe(true);

      const res2 = clampPan({ scale: 2, tx: 0, ty: 0 }, content, { w: 0, h: 0 });
      expect(Number.isFinite(res2.tx)).toBe(true);
      expect(Number.isFinite(res2.ty)).toBe(true);
    });
  });
});
