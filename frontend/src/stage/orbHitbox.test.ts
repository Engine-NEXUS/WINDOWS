import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// Regression guard for the click-blocking bug: the orb rect (200x200 logical at the top centre) must only be an
// interactive stage hitbox while animation calibration is open. Source-scan test, like the Agent Island ones.
describe("orb hitbox", () => {
  const src = readFileSync(new URL("./OrbFrame.tsx", import.meta.url), "utf8");

  it("asks for the orb to be interactive only through the calibrating flag", () => {
    const calls = src.split("\n").filter((l) => l.includes('"set_orb_interactive"'));
    expect(calls).toHaveLength(1);
    expect(calls[0]).toContain("interactive: calibrating");
    expect(src).toContain("const calibrating = useAssistant((s) => !!s.calibrationTarget);");
  });

  it("never makes the orb interactive just because the state is not idle", () => {
    expect(src).not.toMatch(/state === "idle"[\s\S]{0,120}set_orb_interactive/);
    expect(src).not.toContain("{ interactive: true }");
  });
});
