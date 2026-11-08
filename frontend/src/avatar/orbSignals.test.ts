import { describe, expect, it } from "vitest";
import { tourFetchingFor, trackAgentWatches } from "./orbSignals";

describe("tourFetchingFor", () => {
  it("turns on for the fetching phase and off for fallback, start and end", () => {
    expect(tourFetchingFor("screen:tour_phase", { phase: "fetching" })).toBe(true);
    expect(tourFetchingFor("screen:tour_phase", { phase: "fallback" })).toBe(false);
    expect(tourFetchingFor("screen:tour_start", {})).toBe(false);
    expect(tourFetchingFor("screen:tour_end", { reason: "done" })).toBe(false);
  });
  it("ignores unknown phases, malformed payloads and other events", () => {
    expect(tourFetchingFor("screen:tour_phase", { phase: "other" })).toBeNull();
    expect(tourFetchingFor("screen:tour_phase", null)).toBeNull();
    expect(tourFetchingFor("screen:callout", {})).toBeNull();
  });
});

describe("trackAgentWatches", () => {
  const none = new Set<string>();
  it("progress adds a watch and done/error/stopped remove it", () => {
    const a = trackAgentWatches(none, "agent_watch:progress", { id: "w1" });
    expect([...a]).toEqual(["w1"]);
    for (const ev of ["agent_watch:done", "agent_watch:error", "agent_watch:stopped"]) {
      expect(trackAgentWatches(a, ev, { id: "w1" }).size).toBe(0);
    }
  });
  it("tracks several watches independently", () => {
    let s = trackAgentWatches(none, "agent_watch:progress", { id: "a" });
    s = trackAgentWatches(s, "agent_watch:progress", { id: "b" });
    s = trackAgentWatches(s, "agent_watch:done", { id: "a" });
    expect([...s]).toEqual(["b"]);
  });
  it("a watch that needs input is still a watch", () => {
    expect(trackAgentWatches(none, "agent_watch:login_required", { id: "w" }).has("w")).toBe(true);
  });
  it("does not mutate its input and handles id-less payloads", () => {
    const s = new Set(["x"]);
    trackAgentWatches(s, "agent_watch:done", { id: "x" });
    expect(s.has("x")).toBe(true);
    const anon = trackAgentWatches(none, "agent_watch:progress", {});
    expect(anon.size).toBe(1);
    expect(trackAgentWatches(anon, "agent_watch:done", null).size).toBe(0);
  });
  it("ignores unrelated events", () => {
    expect(trackAgentWatches(none, "agent_watch:other", { id: "w" }).size).toBe(0);
  });
});
