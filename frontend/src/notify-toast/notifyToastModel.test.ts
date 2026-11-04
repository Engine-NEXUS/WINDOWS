import { describe, it, expect } from "vitest";
import { FADE_MS, asPayload, isFading, moreLabel, remainingAfter, statusLabel } from "./notifyToastModel";

describe("notify toast model", () => {
  it("counts down and pauses while hovered", () => {
    expect(remainingAfter(5000, 100, false)).toBe(4900);
    expect(remainingAfter(5000, 100, true)).toBe(5000);
    expect(remainingAfter(50, 100, false)).toBe(0);
    expect(remainingAfter(50, -5, false)).toBe(50);
  });
  it("fades only in the last stretch", () => {
    expect(isFading(5000)).toBe(false);
    expect(isFading(FADE_MS)).toBe(true);
    expect(isFading(1)).toBe(true);
    expect(isFading(0)).toBe(false);
  });
  it("labels status and grouped repeats", () => {
    expect(statusLabel("failed")).toBe("Failed");
    expect(statusLabel("recovered")).toBe("Recovered");
    expect(statusLabel("info")).toBe("New");
    expect(moreLabel(1)).toBe("");
    expect(moreLabel(4)).toBe("+3 more");
  });
  it("only accepts a payload with an event id and clamps the rest", () => {
    expect(asPayload(null)).toBeNull();
    expect(asPayload({ title: "x" })).toBeNull();
    const p = asPayload({ eventId: "ev_1", status: "weird", durationMs: 99999999, count: 0 });
    expect(p).toMatchObject({ eventId: "ev_1", status: "info", durationMs: 300000, count: 1, provider: "NEXUS", action: "Open", rows: [] });
  });
  it("reads the headline and keeps at most four clean rows", () => {
    const p = asPayload({
      eventId: "ev_1",
      headline: "Build failed in zync",
      rows: [
        { label: "Repo", value: "a/b" },
        { label: "Branch", value: "main" },
        { label: "Empty", value: "  " },
        { label: 5, value: "x" },
        { label: "Run", value: "#9" },
        { label: "Step", value: "step 3" },
        { label: "Extra", value: "dropped" },
      ],
    });
    expect(p?.headline).toBe("Build failed in zync");
    expect(p?.rows.map((r) => r.label)).toEqual(["Repo", "Branch", "Run", "Step"]);
  });
  it("falls back to the old title field for the headline", () => {
    expect(asPayload({ eventId: "ev_1", title: "Old title" })?.headline).toBe("Old title");
  });
});
