import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error the gate is a plain .mjs script shared with CI
import { loadThemes, parseColor, over, ratio, PAIRS } from "../../../scripts/check-theme-contrast.mjs";

const read = (p: string) => readFileSync(resolve(__dirname, "../../..", p), "utf8");

describe("contrast maths", () => {
  it("black on white is 21:1 and a colour on itself is 1:1", () => {
    expect(ratio(parseColor("#000"), parseColor("#fff"))).toBeCloseTo(21, 1);
    expect(ratio(parseColor("#777"), parseColor("#777"))).toBeCloseTo(1, 5);
  });

  it("composites translucent text over its surface before measuring", () => {
    const half = over(parseColor("rgba(255,255,255,0.5)"), parseColor("#000"));
    expect(Math.round(half.r)).toBe(128);
  });
});

describe("the real tokens", () => {
  const themes = loadThemes([read("frontend/src/theme/tokens.css"), read("frontend/src/sidebar/unified.css"), read("frontend/src/pr-list/pr-list.css")]);

  it("define every token the gate measures, in both themes", () => {
    for (const map of [themes.dark, themes.light]) {
      for (const [fg, bg] of PAIRS as [string, string][]) {
        expect(map.has(fg), fg).toBe(true);
        expect(map.has(bg), bg).toBe(true);
      }
    }
  });

  it("light overrides dark for the roles that differ", () => {
    expect(themes.dark.get("--ui-focus-ring")).not.toBe(themes.light.get("--ui-focus-ring"));
    expect(themes.light.get("--nx-bg")).toBe("#f8f9fa");
  });
});
