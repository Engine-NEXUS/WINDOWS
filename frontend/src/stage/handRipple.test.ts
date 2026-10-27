import { describe, expect, it } from "vitest";
import type { HudState } from "../hand/payload";
import { overlayModel, ripplePosition } from "./handOverlayModel";

const base: HudState = {
  tracking: true, armed: true, pose: "palm", pinch: 1, pressed: false, dragging: false, scrolling: false,
  cursor: [0, 0], arm_stage: 3, live: true, screen: [1920, 1080], primary: true, fps: 20, last_action: "click",
  action_seq: 1, note: null, cam_fps: 30, gpu_busy: false, covered: false,
};
const vp = { w: 1920, h: 1080 };

describe("click ripple lands where the click landed", () => {
  it("uses the reported click position, not the (possibly zero) hand cursor", () => {
    // Live mode leaves the tracked cursor at 0,0; the ripple used to appear in the top-left corner.
    expect(ripplePosition({ ...base, action_pos: [812, 407] }, vp)).toEqual({ x: 812, y: 407 });
  });
  it("converts physical pixels to CSS pixels on a scaled display", () => {
    expect(ripplePosition({ ...base, action_pos: [1500, 750] }, { w: 1000, h: 600 }, 1.5)).toEqual({ x: 1000, y: 500 });
  });
  it("draws nothing for a click on another monitor", () => {
    expect(ripplePosition({ ...base, action_pos: [2500, 300] }, vp)).toBeNull();
    expect(ripplePosition({ ...base, action_pos: [-300, 300] }, vp)).toBeNull();
  });
  it("falls back to the tracked cursor on the primary monitor, and to nothing elsewhere", () => {
    expect(ripplePosition({ ...base, cursor: [960, 540] }, { w: 960, h: 540 })).toEqual({ x: 480, y: 270 });
    expect(ripplePosition({ ...base, primary: false }, vp)).toBeNull();
  });
  it("only a real action produces a ripple position", () => {
    expect(overlayModel({ ...base, action_pos: [10, 20] }, vp, true).ripplePos).toEqual({ x: 10, y: 20 });
    expect(overlayModel({ ...base, action_pos: [10, 20] }, vp, false).ripplePos).toBeNull();
    expect(overlayModel({ ...base, last_action: "armed", action_pos: [10, 20] }, vp, true).ripplePos).toBeNull();
  });
});
