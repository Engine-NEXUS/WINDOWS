import { describe, expect, it } from "vitest";
import { applyLearnedCorrections, normalizeLearnedCorrections } from "./learnedCorrections";

describe("normalizeLearnedCorrections", () => {
  it("accepts the object shape Rust actually returns", () => {
    const rust = [
      { from: "zink", to: "zync", context_before: "in", count: 3 },
      { from: "post mode", to: "ghost mode", context_before: "", count: 4 },
    ];
    expect(normalizeLearnedCorrections(rust)).toEqual([
      { from: "zink", to: "zync", context: "in" },
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

describe("applyLearnedCorrections", () => {
  it("replaces whole words only", () => {
    const r = applyLearnedCorrections("open the painting in zink", [{ from: "in", to: "inn" }]);
    expect(r.text).toBe("open the painting inn zink");
    expect(applyLearnedCorrections("think of ink", [{ from: "ink", to: "zync" }]).text).toBe("think of zync");
    expect(applyLearnedCorrections("it's the syncing", [{ from: "sync", to: "zync" }]).text).toBe("it's the syncing");
  });

  it("is case-insensitive in BOTH the check and the replacement", () => {
    // The old code tested includes() case-sensitively but replaced with /gi, so "Zink" never matched.
    const r = applyLearnedCorrections("Analyse PR 5 in Zink", [{ from: "zink", to: "zync" }]);
    expect(r.text).toBe("Analyse PR 5 in zync");
    expect(r.applied).toEqual([{ from: "zink", to: "zync" }]);
  });

  it("honours the learned context word", () => {
    const c = [{ from: "zink", to: "zync", context: "in" }];
    expect(applyLearnedCorrections("merge pr 5 in zink", c).text).toBe("merge pr 5 in zync");
    expect(applyLearnedCorrections("a pink zink", c).text).toBe("a pink zink");
  });

  it("handles multi-word phrases and regex metacharacters, and reports nothing when unchanged", () => {
    expect(applyLearnedCorrections("turn on the post mode", [{ from: "post mode", to: "ghost mode" }]).text)
      .toBe("turn on the ghost mode");
    // Regex metacharacters in a learned phrase are escaped, never interpreted (and never throw).
    expect(applyLearnedCorrections("a (b) c", [{ from: "(b)", to: "x" }]).text).toBe("a x c");
    expect(applyLearnedCorrections("a b c", [{ from: "(b)", to: "x" }]).text).toBe("a b c");
    const r = applyLearnedCorrections("open chrome", [{ from: "zink", to: "zync" }]);
    expect(r.applied).toEqual([]);
    expect(r.text).toBe("open chrome");
  });
});
