import { describe, expect, it } from "vitest";
import { captionSlot, CAPTION, intersects, SLOT_GAP, taskStackSlot, type Rect } from "./layoutSlots";
import { chipPlacement } from "./handOverlayModel";

// Common screens as [physical width, physical height, scale]. The stage is that physical size divided by the scale.
const SCREENS: [number, number, number][] = [
  [1366, 768, 1],
  [1920, 1080, 1],
  [1920, 1080, 1.25],
  [1920, 1080, 1.5],
  [2560, 1440, 1],
  [2560, 1440, 1.5],
];

describe("task stack under the spinner", () => {
  it("hangs below the spinner and lines up with its right edge", () => {
    const vp = { w: 1920, h: 1080 };
    const spinner: Rect = { x: 1800, y: 54, w: 80, h: 80 };
    expect(taskStackSlot(spinner, vp)).toEqual({ top: 54 + 80 + SLOT_GAP, right: 40 });
  });

  it("follows a larger spinner instead of overlapping it", () => {
    const vp = { w: 1920, h: 1080 };
    const small = taskStackSlot({ x: 1800, y: 54, w: 80, h: 80 }, vp);
    const large = taskStackSlot({ x: 1720, y: 54, w: 160, h: 160 }, vp);
    expect(large.top).toBeGreaterThan(small.top);
  });

  it("uses the old fixed slot until the spinner is known", () => {
    expect(taskStackSlot(null, { w: 1920, h: 1080 })).toEqual({ top: 112, right: 24 });
  });
});

describe("hand chip stays above the caption", () => {
  it("never overlaps the caption, with the taskbar hidden or showing, on every common screen", () => {
    for (const [pw, ph, scale] of SCREENS) {
      const vp = { w: Math.round(pw / scale), h: Math.round(ph / scale) };
      const caption = captionSlot(vp);
      for (const taskbar of [0, Math.round(48 / scale)]) {
        const chipW = 220;
        const chipH = 28;
        const p = chipPlacement("top", vp, chipW, chipH, taskbar, caption.y);
        const chip: Rect = { x: p.left, y: p.top, w: chipW, h: chipH };
        expect(intersects(chip, caption), `${pw}x${ph}@${scale} taskbar ${taskbar}`).toBe(false);
        expect(p.top, "stays on screen").toBeGreaterThanOrEqual(0);
      }
    }
  });

  it("keeps the old position when there is no caption to avoid", () => {
    expect(chipPlacement("top", { w: 1440, h: 900 }, 200, 28).top).toBe(900 - 28 - 12);
  });
});

describe("slots on common screens", () => {
  it("the orb capsule, spinner, task stack and caption never intersect", () => {
    for (const [pw, ph, scale] of SCREENS) {
      const vp = { w: Math.round(pw / scale), h: Math.round(ph / scale) };
      const orb: Rect = { x: (vp.w - 301) / 2, y: 0, w: 301, h: 105 }; // pebble, top dock
      const spinner: Rect = { x: vp.w * 0.95 - 40, y: vp.h * 0.05, w: 80, h: 80 }; // default 0.95 / 0.05
      const stack = taskStackSlot(spinner, vp);
      const chipW = 320;
      const stackRect: Rect = { x: vp.w - stack.right - chipW, y: stack.top, w: chipW, h: 3 * 32 + 2 * 6 };
      const caption = captionSlot(vp);
      const slots: [string, Rect][] = [
        ["orb", orb],
        ["spinner", spinner],
        ["tasks", stackRect],
        ["caption", caption],
      ];
      for (let i = 0; i < slots.length; i++) {
        for (let j = i + 1; j < slots.length; j++) {
          expect(intersects(slots[i][1], slots[j][1]), `${slots[i][0]} / ${slots[j][0]} on ${pw}x${ph}@${scale}`).toBe(false);
        }
      }
    }
  });

  it("the caption keeps its tuned position", () => {
    const c = captionSlot({ w: 1000, h: 1000 });
    expect(c.y).toBe(1000 * CAPTION.top);
    expect(c.x + c.w / 2).toBeCloseTo(1000 * CAPTION.centerX, 5);
  });
});
