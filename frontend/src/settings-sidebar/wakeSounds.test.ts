import { describe, expect, it } from "vitest";
import { WAKE_SOUNDS, clampWakeVolume, normalizeWakeSound } from "./wakeSounds";

describe("wake sounds", () => {
  it("offers the five candidates the engine knows", () => {
    expect(WAKE_SOUNDS.map((s) => s.key)).toEqual(["rise", "glass", "pluck", "triplet", "sweep"]);
  });
  it("falls back to rise for anything unknown", () => {
    expect(normalizeWakeSound("GLASS")).toBe("glass");
    expect(normalizeWakeSound(" sweep ")).toBe("sweep");
    expect(normalizeWakeSound("nonsense")).toBe("rise");
    expect(normalizeWakeSound(undefined)).toBe("rise");
  });
  it("keeps the volume between 0 and 100", () => {
    expect(clampWakeVolume(250)).toBe(100);
    expect(clampWakeVolume(-5)).toBe(0);
    expect(clampWakeVolume("40")).toBe(40);
    expect(clampWakeVolume("abc")).toBe(70);
  });
});
