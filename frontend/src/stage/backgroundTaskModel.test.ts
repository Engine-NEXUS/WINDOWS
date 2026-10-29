import { describe, expect, it } from "vitest";
import { elapsedLabel, MAX_CHIPS, parseTasks, visibleTasks } from "./backgroundTaskModel";

const task = (id: string, startedMs: number) => ({ id, kind: "coderabbit", label: id, startedMs });

describe("background task chips", () => {
  it("shows the oldest few and sums up the rest", () => {
    const list = [task("d", 4), task("a", 1), task("c", 3), task("b", 2)];
    const { shown, extra } = visibleTasks(list);
    expect(shown.map((t) => t.id)).toEqual(["a", "b", "c"]);
    expect(shown.length).toBe(MAX_CHIPS);
    expect(extra).toBe(1);
  });

  it("formats elapsed time as m:ss and never goes negative", () => {
    expect(elapsedLabel(0, 65_000)).toBe("1:05");
    expect(elapsedLabel(10_000, 5_000)).toBe("0:00");
  });

  it("ignores malformed events instead of blanking the stage", () => {
    expect(parseTasks(null)).toEqual([]);
    expect(parseTasks("x")).toEqual([]);
    expect(parseTasks([{ id: 1 }, null, task("ok", 1)])).toEqual([task("ok", 1)]);
  });
});
