import { describe, expect, it } from "vitest";
import { depthForKey, nextDock } from "./dockKeys";

describe("sidebar depth keys", () => {
  it("Down sends the window behind, Up brings it to the front, other keys do nothing", () => {
    expect(depthForKey("ArrowDown")).toBe("back");
    expect(depthForKey("ArrowUp")).toBe("front");
    expect(depthForKey("ArrowLeft")).toBeNull();
    expect(depthForKey("Enter")).toBeNull();
  });

  it("depth keys never change the left/centre/right dock", () => {
    expect(nextDock("left", "ArrowDown")).toBe("left");
    expect(nextDock("right", "ArrowUp")).toBe("right");
  });
});
