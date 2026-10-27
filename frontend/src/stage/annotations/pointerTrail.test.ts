import { beforeEach, describe, expect, it } from "vitest";
import { Rect, Size } from "../tourGeometry";
import {
  computePointerTrail,
  getLastPoint,
  resetPointerTrail,
  setLastPoint,
} from "./pointerTrail";

describe("pointerTrail model", () => {
  const viewport: Size = { w: 1920, h: 1080 };
  const target1: Rect = { x: 200, y: 300, w: 100, h: 80 };
  const target2: Rect = { x: 800, y: 600, w: 200, h: 100 };
  const orb: Rect = { x: 50, y: 50, w: 60, h: 60 };

  beforeEach(() => {
    resetPointerTrail();
  });

  it("resets lastPoint to null", () => {
    setLastPoint({ x: 123, y: 456 });
    expect(getLastPoint()).toEqual({ x: 123, y: 456 });
    resetPointerTrail();
    expect(getLastPoint()).toBeNull();
  });

  it("returns null when target is null", () => {
    expect(computePointerTrail(null, viewport)).toBeNull();
  });

  it("starts at viewport top-centre when no lastPoint and no orb", () => {
    const res = computePointerTrail(target1, viewport);
    expect(res).not.toBeNull();
    expect(res?.start).toEqual({ x: 960, y: 0 });
    expect(res?.end).toEqual({ x: 200, y: 300 });
    expect(res?.dx).toBe(200 - 960);
    expect(res?.dy).toBe(300 - 0);
    expect(getLastPoint()).toEqual({ x: 200, y: 300 });
  });

  it("starts at orb centre when first callout and orb is provided", () => {
    const res = computePointerTrail(target1, viewport, orb);
    expect(res).not.toBeNull();
    expect(res?.start).toEqual({ x: 80, y: 80 });
    expect(res?.end).toEqual({ x: 200, y: 300 });
    expect(res?.dx).toBe(200 - 80);
    expect(res?.dy).toBe(300 - 80);
    expect(getLastPoint()).toEqual({ x: 200, y: 300 });
  });

  it("glides from previous target corner to next target corner on successive callouts", () => {
    const first = computePointerTrail(target1, viewport);
    expect(first?.end).toEqual({ x: 200, y: 300 });

    const second = computePointerTrail(target2, viewport);
    expect(second?.start).toEqual({ x: 200, y: 300 });
    expect(second?.end).toEqual({ x: 800, y: 600 });
    expect(second?.dx).toBe(800 - 200);
    expect(second?.dy).toBe(600 - 300);
    expect(getLastPoint()).toEqual({ x: 800, y: 600 });
  });
});
