import { describe, expect, it } from "vitest";
import { stateAfterBargeResume } from "./bargeOrb";

describe("stateAfterBargeResume", () => {
  it("returns a ducked orb to speaking while NEXUS is still talking", () => {
    expect(stateAfterBargeResume("listening", true)).toBe("speaking");
    expect(stateAfterBargeResume("thinking", true)).toBe("speaking");
  });
  it("returns it to idle when the speech is over - not stuck in thinking", () => {
    expect(stateAfterBargeResume("thinking", false)).toBe("idle");
    expect(stateAfterBargeResume("listening", false)).toBe("idle");
  });
  it("leaves every other state alone", () => {
    for (const s of ["speaking", "idle"]) expect(stateAfterBargeResume(s, true)).toBeNull();
  });
});
