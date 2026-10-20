import { describe, expect, it } from "vitest";
import type { HudState } from "../hand/payload";
import {
  chipPlacement,
  chipText,
  gestureLabel,
  overlayModel,
  taskbarBottomPx,
} from "./handOverlayModel";

const baseState: HudState = {
  tracking: true,
  armed: false,
  pose: "palm",
  pinch: 1,
  pressed: false,
  dragging: false,
  scrolling: false,
  cursor: [960, 540],
  arm_stage: 1,
  live: true,
  screen: [1920, 1080],
  primary: true,
  fps: 30,
  last_action: null,
  action_seq: 0,
  note: null,
  cam_fps: 30,
  gpu_busy: false,
  covered: false,
};

describe("gestureLabel boundaries", () => {
  it("returns null for null, empty, or unknown action strings", () => {
    expect(gestureLabel(null)).toBeNull();
    expect(gestureLabel("")).toBeNull();
    expect(gestureLabel("click")).toBeNull();
    expect(gestureLabel("double_click")).toBeNull();
    expect(gestureLabel("drag")).toBeNull();
  });

  it("maps all recognized window/desktop gesture commands", () => {
    expect(gestureLabel("next_tab")).toBe("next tab");
    expect(gestureLabel("prev_tab")).toBe("previous tab");
    expect(gestureLabel("window_switcher")).toBe("windows");
    expect(gestureLabel("window_next")).toBe("next window");
    expect(gestureLabel("window_prev")).toBe("previous window");
    expect(gestureLabel("window_commit")).toBe("window picked");
    expect(gestureLabel("window_cancel")).toBe("cancelled");
    expect(gestureLabel("desktop_prev")).toBe("previous desktop");
    expect(gestureLabel("desktop_next")).toBe("next desktop");
    expect(gestureLabel("task_view")).toBe("all windows");
    expect(gestureLabel("show_desktop")).toBe("show desktop");
  });
});

describe("chipText status and environmental notes", () => {
  it("prioritizes covered camera over dim room and gpu contention", () => {
    const s: HudState = {
      ...baseState,
      covered: true,
      gpu_busy: true,
      cam_fps: 5,
    };
    const text = chipText(s);
    expect(text).toContain("can't see, camera covered?");
    expect(text).not.toContain("dim room");
    expect(text).not.toContain("heavy graphics load");
  });

  it("displays heavy graphics load only when not covered", () => {
    const s: HudState = {
      ...baseState,
      covered: false,
      gpu_busy: true,
      cam_fps: 30,
    };
    expect(chipText(s)).toContain("heavy graphics load");
  });

  it("checks exact cam_fps dim room threshold (< 15 fps)", () => {
    expect(chipText({ ...baseState, cam_fps: 14 })).toContain("dim room, add light");
    expect(chipText({ ...baseState, cam_fps: 15 })).not.toContain("dim room");
    expect(chipText({ ...baseState, cam_fps: 0 })).not.toContain("dim room");
  });

  it("clamps arm_stage display to at most 3", () => {
    expect(chipText({ ...baseState, arm_stage: 1 })).toContain("1/3");
    expect(chipText({ ...baseState, arm_stage: 2 })).toContain("2/3");
    expect(chipText({ ...baseState, arm_stage: 3 })).toContain("3/3");
    expect(chipText({ ...baseState, arm_stage: 5 })).toContain("3/3");
  });
});

describe("chipPlacement boundaries", () => {
  it("places chip at top margin when orb is docked at bottom", () => {
    const pos = chipPlacement("bottom", { w: 1000, h: 800 }, 200, 40, 50);
    expect(pos.top).toBe(12);
    expect(pos.left).toBe((1000 - 200) / 2);
  });

  it("places chip above taskbar when orb is docked at top", () => {
    const pos = chipPlacement("top", { w: 1000, h: 800 }, 200, 40, 60);
    // top = 800 - 40 - (60 + 12) = 688
    expect(pos.top).toBe(688);
    expect(pos.left).toBe(400);
  });

  it("clamps coordinates to zero if viewport is smaller than chip or margins", () => {
    const pos = chipPlacement("top", { w: 100, h: 50 }, 300, 100, 40);
    expect(pos.left).toBe(0);
    expect(pos.top).toBe(0);
  });
});

describe("taskbarBottomPx boundaries", () => {
  it("computes taskbar height when on bottom", () => {
    expect(taskbarBottomPx({ height: 1080, availHeight: 1040, availTop: 0 })).toBe(40);
    expect(taskbarBottomPx({ height: 900, availHeight: 852 })).toBe(48);
  });

  it("returns 0 when taskbar is auto-hidden or fullscreen", () => {
    expect(taskbarBottomPx({ height: 1080, availHeight: 1080 })).toBe(0);
  });

  it("returns 0 when taskbar is on top edge (availTop > 0)", () => {
    expect(taskbarBottomPx({ height: 1080, availHeight: 1040, availTop: 40 })).toBe(0);
  });

  it("rejects abnormal covered values exceeding one third of screen height", () => {
    // 1080 - 600 = 480 covered, which is > 1080/3 = 360
    expect(taskbarBottomPx({ height: 1080, availHeight: 600, availTop: 0 })).toBe(0);
  });
});

describe("overlayModel ring scaling and classes", () => {
  it("computes scaled cursor ring in preview mode on primary screen", () => {
    const s: HudState = {
      ...baseState,
      armed: true,
      live: false,
      primary: true,
      screen: [1920, 1080],
      cursor: [960, 540],
    };
    const vp = { w: 1280, h: 720 };
    const res = overlayModel(s, vp, false);
    expect(res.ring).toEqual({ x: 640, y: 360 });
    expect(res.ringClass).toContain("hand-ring--preview");
  });

  it("handles zero physical screen dimensions gracefully without dividing by zero", () => {
    const s: HudState = {
      ...baseState,
      armed: true,
      live: false,
      screen: [0, 0],
      cursor: [100, 100],
    };
    const res = overlayModel(s, { w: 1280, h: 720 }, false);
    expect(res.ring).toEqual({ x: 100, y: 100 });
  });
});
