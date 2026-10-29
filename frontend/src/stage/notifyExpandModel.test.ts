import { describe, expect, it } from "vitest";
import { clampRect, durationFor, expandStyle, type ExpandPayload } from "./notifyExpandModel";

const vp = { w: 1920, h: 1080 };
const payload: ExpandPayload = {
  title: "PR failed in zync",
  status: "failed",
  from: { x: 1544, y: 972, w: 364, h: 96 },
  to: { x: 1305, y: 20, w: 595, h: 975 },
  durationMs: 280,
};

describe("notify expand model", () => {
  it("starts at the toast and ends at the sidebar rectangle", () => {
    expect(expandStyle("start", payload, vp)).toEqual({ left: 1544, top: 972, width: 364, height: 96, opacity: 1 });
    expect(expandStyle("end", payload, vp)).toEqual({ left: 1305, top: 20, width: 595, height: 975, opacity: 1 });
    expect(expandStyle("leaving", payload, vp).opacity).toBe(0);
  });
  it("keeps any rectangle inside the overlay and a visible size", () => {
    expect(clampRect({ x: 1900, y: 1070, w: 600, h: 900 }, vp)).toEqual({ x: 1320, y: 180, w: 600, h: 900 });
    expect(clampRect({ x: -50, y: -50, w: 10, h: 10 }, vp)).toEqual({ x: 0, y: 0, w: 40, h: 40 });
    expect(clampRect({ x: 0, y: 0, w: 99999, h: 99999 }, vp)).toEqual({ x: 0, y: 0, w: 1920, h: 1080 });
  });
  it("survives a malformed payload", () => {
    const r = clampRect({ x: Number.NaN, y: undefined as unknown as number, w: "big" as unknown as number }, vp);
    expect(Object.values(r).every(Number.isFinite)).toBe(true);
  });
  it("has no motion when reduced motion is requested and bounds the duration", () => {
    expect(durationFor(payload, true)).toBe(0);
    expect(durationFor(payload, false)).toBe(280);
    expect(durationFor({ durationMs: 99999 }, false)).toBe(1000);
    expect(durationFor({ durationMs: -5 }, false)).toBe(0);
  });
});
