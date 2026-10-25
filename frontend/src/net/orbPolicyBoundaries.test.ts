import { describe, expect, it } from "vitest";
import {
  GHOST_ORB_LINGER_MS,
  GHOST_VOICE_LEVEL,
  ghostOrbBusy,
  initialGhostOrbState,
  stepGhostOrb,
  type GhostOrbInputs,
  type GhostOrbState,
} from "./orbPolicy";

const quietListening: GhostOrbInputs = {
  state: "listening",
  micLevel: 0,
  captionActive: false,
  ttsActive: false,
  loadingVisible: false,
  awaitingInput: false,
  parked: false,
};

const quietIdle: GhostOrbInputs = {
  ...quietListening,
  state: "idle",
};

describe("ghostOrbBusy boundary conditions", () => {
  it("evaluates false for all-quiet inputs", () => {
    expect(ghostOrbBusy(quietListening)).toBe(false);
    expect(ghostOrbBusy(quietIdle)).toBe(false);
  });

  it("checks exact GHOST_VOICE_LEVEL boundary only in listening state", () => {
    expect(ghostOrbBusy({ ...quietListening, micLevel: GHOST_VOICE_LEVEL - 0.001 })).toBe(false);
    expect(ghostOrbBusy({ ...quietListening, micLevel: GHOST_VOICE_LEVEL })).toBe(true);
    expect(ghostOrbBusy({ ...quietListening, micLevel: GHOST_VOICE_LEVEL + 0.001 })).toBe(true);

    // High mic level in idle state does NOT count as ghostOrbBusy
    expect(ghostOrbBusy({ ...quietIdle, micLevel: 0.9 })).toBe(false);
  });

  it("handles negative, zero, and NaN mic levels without false triggers", () => {
    expect(ghostOrbBusy({ ...quietListening, micLevel: -0.5 })).toBe(false);
    expect(ghostOrbBusy({ ...quietListening, micLevel: 0 })).toBe(false);
    expect(ghostOrbBusy({ ...quietListening, micLevel: Number.NaN })).toBe(false);
  });

  it("triggers busy state for each distinct active condition", () => {
    expect(ghostOrbBusy({ ...quietIdle, state: "thinking" })).toBe(true);
    expect(ghostOrbBusy({ ...quietIdle, state: "speaking" })).toBe(true);
    expect(ghostOrbBusy({ ...quietIdle, captionActive: true })).toBe(true);
    expect(ghostOrbBusy({ ...quietIdle, ttsActive: true })).toBe(true);
    expect(ghostOrbBusy({ ...quietIdle, loadingVisible: true })).toBe(true);
    expect(ghostOrbBusy({ ...quietIdle, awaitingInput: true })).toBe(true);
  });
});

describe("stepGhostOrb boundary conditions", () => {
  it("extends shownUntil by GHOST_ORB_LINGER_MS whenever parked", () => {
    const prev: GhostOrbState = { shownUntil: 1000 };
    const res = stepGhostOrb(prev, { ...quietIdle, parked: true }, 5000);
    expect(res.shown).toBe(true);
    expect(res.next.shownUntil).toBe(5000 + GHOST_ORB_LINGER_MS);
  });

  it("evaluates exact boundary timestamps against shownUntil", () => {
    const prev: GhostOrbState = { shownUntil: 10_000 };

    // 1ms before expiry -> still shown
    const before = stepGhostOrb(prev, quietListening, 9_999);
    expect(before.shown).toBe(true);
    expect(before.next).toBe(prev); // Identity preserved

    // Exact expiry millisecond -> hidden
    const at = stepGhostOrb(prev, quietListening, 10_000);
    expect(at.shown).toBe(false);
    expect(at.next).toBe(prev);

    // 1ms after expiry -> hidden
    const after = stepGhostOrb(prev, quietListening, 10_001);
    expect(after.shown).toBe(false);
    expect(after.next).toBe(prev);
  });

  it("supports negative or zero epoch timestamps", () => {
    const prev: GhostOrbState = { shownUntil: 0 };
    const res = stepGhostOrb(prev, quietListening, -100);
    expect(res.shown).toBe(true); // -100 < 0
  });
});

describe("initialGhostOrbState boundary conditions", () => {
  it("initializes shownUntil to now + linger", () => {
    expect(initialGhostOrbState(0).shownUntil).toBe(GHOST_ORB_LINGER_MS);
    expect(initialGhostOrbState(10_000).shownUntil).toBe(10_000 + GHOST_ORB_LINGER_MS);
    expect(initialGhostOrbState(-2000).shownUntil).toBe(-2000 + GHOST_ORB_LINGER_MS);
  });
});
