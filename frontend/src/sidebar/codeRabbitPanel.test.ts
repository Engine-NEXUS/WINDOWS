import { describe, expect, it } from "vitest";
import { isReview, nextDock } from "./CodeRabbitPanel";

describe("CodeRabbit panel arrow docking", () => {
  it("steps left - centre - right and stops at the ends", () => {
    expect(nextDock("center", "ArrowLeft")).toBe("left");
    expect(nextDock("left", "ArrowLeft")).toBe("left");
    expect(nextDock("center", "ArrowRight")).toBe("right");
    expect(nextDock("right", "ArrowRight")).toBe("right");
    expect(nextDock("right", "ArrowLeft")).toBe("center");
    expect(nextDock("left", "ArrowRight")).toBe("center");
  });

  it("ignores other keys", () => {
    expect(nextDock("center", "ArrowUp")).toBe("center");
  });

  it("tells a review from a PR list", () => {
    expect(isReview({ mode: "pr_list", activePrs: [] })).toBe(false);
    expect(isReview({ findings: [] } as never)).toBe(true);
  });
});
