import { describe, expect, it } from "vitest";
import { normalizeLearnedCorrections } from "./learnedCorrections";

describe("normalizeLearnedCorrections", () => {
  it("accepts the object shape Rust actually returns", () => {
    const rust = [
      { from: "zink", to: "zync", context_before: "in", count: 3 },
      { from: "post mode", to: "ghost mode", context_before: "", count: 4 },
    ];
    expect(normalizeLearnedCorrections(rust)).toEqual([
      { from: "zink", to: "zync" },
      { from: "post mode", to: "ghost mode" },
    ]);
  });

  it("still accepts legacy [from, to] tuples", () => {
    expect(normalizeLearnedCorrections([["zink", "zync"]])).toEqual([{ from: "zink", to: "zync" }]);
  });

  it("drops malformed entries and non-arrays instead of throwing", () => {
    expect(
      normalizeLearnedCorrections([null, 5, { from: "a" }, { from: "", to: "b" }, ["x"], { from: "ok", to: "fine" }]),
    ).toEqual([{ from: "ok", to: "fine" }]);
    expect(normalizeLearnedCorrections(undefined)).toEqual([]);
    expect(normalizeLearnedCorrections({})).toEqual([]);
  });
});
