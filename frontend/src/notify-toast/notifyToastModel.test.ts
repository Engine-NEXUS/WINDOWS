import { describe, it, expect } from "vitest";
import { asPayload, moreLabel, progress, remainingAfter, statusLabel } from "./notifyToastModel";

describe("notify toast model", () => {
  it("counts down and pauses while hovered", () => {
    expect(remainingAfter(5000, 100, false)).toBe(4900);
    expect(remainingAfter(5000, 100, true)).toBe(5000);
    expect(remainingAfter(50, 100, false)).toBe(0);
    expect(remainingAfter(50, -5, false)).toBe(50);
  });
  it("reports the share of time left", () => {
    expect(progress(2500, 5000)).toBe(0.5);
    expect(progress(9000, 5000)).toBe(1);
    expect(progress(10, 0)).toBe(0);
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
    expect(p).toMatchObject({ eventId: "ev_1", status: "info", durationMs: 300000, count: 1, provider: "NEXUS", action: "Open" });
  });
});
