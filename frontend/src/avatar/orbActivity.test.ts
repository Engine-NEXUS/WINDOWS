import { describe, expect, it } from "vitest";
import {
  DEFAULT_ORB_DESIGNS,
  deriveOrbActivity,
  designFor,
  normalizeOrbDesigns,
  NO_SIGNALS,
  ORB_ACTIVITIES,
  settleActivity,
  type OrbActivityInputs,
} from "./orbActivity";
import { ORB_DESIGNS } from "./dotted/resolveOrb";
import { orbLevel } from "./useOrbActivity";

const base: OrbActivityInputs = { ...NO_SIGNALS, state: "idle", ghostActive: false, awaitingInput: false };
const at = (o: Partial<OrbActivityInputs>): OrbActivityInputs => ({ ...base, ...o });

describe("deriveOrbActivity priority", () => {
  const cases: [string, Partial<OrbActivityInputs>, string][] = [
    ["nothing going on", {}, "idle"],
    ["listening", { state: "listening" }, "listening"],
    ["thinking", { state: "thinking" }, "thinking"],
    ["speaking", { state: "speaking" }, "speaking"],
    ["speaking while waiting on the user", { state: "speaking", awaitingInput: true }, "waiting"],
    ["awaitingInput without speaking is not waiting", { state: "idle", awaitingInput: true }, "idle"],
    ["screen tour fetching beats thinking", { state: "thinking", tourFetching: true }, "analysing"],
    ["screen tour fetching beats speaking (ack line)", { state: "speaking", tourFetching: true }, "analysing"],
    ["waiting beats the tour", { state: "speaking", awaitingInput: true, tourFetching: true }, "waiting"],
    ["ghost session while idle", { ghostActive: true }, "ghost"],
    ["listening beats ghost (hot mic turn)", { ghostActive: true, state: "listening" }, "listening"],
    ["speaking beats ghost", { ghostActive: true, state: "speaking" }, "speaking"],
    ["hand mode while idle", { handActive: true }, "hand"],
    ["ghost beats hand", { ghostActive: true, handActive: true }, "ghost"],
    ["dictation while idle", { dictationActive: true }, "dictation"],
    ["hand beats dictation", { handActive: true, dictationActive: true }, "hand"],
    ["agent watch while idle", { agentWatching: true }, "agent"],
    ["dictation beats agent", { dictationActive: true, agentWatching: true }, "dictation"],
    ["background tasks while idle", { backgroundTasks: 2 }, "background"],
    ["agent beats background", { agentWatching: true, backgroundTasks: 1 }, "agent"],
    ["zero tasks is idle", { backgroundTasks: 0 }, "idle"],
    ["thinking beats every ambient mode", { state: "thinking", ghostActive: true, handActive: true, backgroundTasks: 3 }, "thinking"],
  ];
  for (const [name, input, want] of cases) {
    it(name, () => expect(deriveOrbActivity(at(input))).toBe(want));
  }
});

describe("mapping", () => {
  it("every activity has a known default design", () => {
    for (const { id } of ORB_ACTIVITIES) {
      expect(ORB_DESIGNS).toContain(DEFAULT_ORB_DESIGNS[id]);
    }
    expect(ORB_ACTIVITIES.map((a) => a.id).sort()).toEqual(Object.keys(DEFAULT_ORB_DESIGNS).sort());
  });

  it("designFor uses a valid saved choice and falls back otherwise", () => {
    expect(designFor("ghost", { ghost: "weaving" })).toBe("weaving");
    expect(designFor("ghost", { ghost: "not-a-design" })).toBe("connecting");
    expect(designFor("ghost", {})).toBe("connecting");
    expect(designFor("ghost", null)).toBe("connecting");
    expect(designFor("idle", undefined)).toBe("circle");
    expect(designFor("speaking", undefined)).toBe("composing");
    expect(designFor("listening", undefined)).toBe("breathing");
    expect(designFor("waiting", undefined)).toBe("breathing");
  });

  it("normalizeOrbDesigns keeps only known activities with known designs", () => {
    expect(normalizeOrbDesigns({ idle: "solving", bogus: "solving", ghost: "nope", hand: 3 })).toEqual({ idle: "solving" });
    expect(normalizeOrbDesigns(null)).toEqual({});
    expect(normalizeOrbDesigns("x")).toEqual({});
  });
});

describe("settleActivity (dwell)", () => {
  it("shows the first activity at once", () => {
    expect(settleActivity(null, "idle", 0, 0)).toEqual({ activity: "idle", waitMs: 0 });
  });
  it("keeps the shown activity until the dwell has passed, then says how long to wait", () => {
    expect(settleActivity("thinking", "speaking", 1000, 1100)).toEqual({ activity: "thinking", waitMs: 250 });
    expect(settleActivity("thinking", "speaking", 1000, 1350)).toEqual({ activity: "speaking", waitMs: 0 });
    expect(settleActivity("thinking", "speaking", 1000, 5000)).toEqual({ activity: "speaking", waitMs: 0 });
  });
  it("the same activity is immediate", () => {
    expect(settleActivity("idle", "idle", 1000, 1001)).toEqual({ activity: "idle", waitMs: 0 });
  });
});

describe("orbLevel (voice coupling)", () => {
  const i = { ttsActive: false, audioVolume: 0.2, micLevel: 0.6, envelope: null as number | null };
  it("listening follows the louder of mic level and browser level, clamped", () => {
    expect(orbLevel("listening", i)).toBe(0.6);
    expect(orbLevel("listening", { ...i, micLevel: 0, audioVolume: 0.3 })).toBe(0.3);
    expect(orbLevel("listening", { ...i, micLevel: 5 })).toBe(1);
  });
  it("speaking follows the TTS envelope, else a floor while audio plays, else silence", () => {
    expect(orbLevel("speaking", { ...i, envelope: 0.7 })).toBe(0.7);
    expect(orbLevel("speaking", { ...i, envelope: 0 })).toBe(0);
    expect(orbLevel("speaking", { ...i, ttsActive: true })).toBe(0.4);
    expect(orbLevel("speaking", { ...i, ttsActive: true, audioVolume: 0.9 })).toBe(0.9);
    expect(orbLevel("speaking", i)).toBe(0);
  });
  it("every other activity is a steady animation", () => {
    for (const a of ["idle", "thinking", "ghost", "hand", "dictation", "agent", "background", "analysing", "waiting"] as const) {
      expect(orbLevel(a, { ...i, envelope: 1, ttsActive: true })).toBe(0);
    }
  });
});
