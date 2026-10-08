import { describe, expect, it } from "vitest";
import { resolveThemeMode } from "./theme";

describe("resolveThemeMode", () => {
  it("passes light through", () => {
    expect(resolveThemeMode("light")).toBe("light");
  });

  it("falls back to dark for anything else (never transparent, never broken)", () => {
    for (const raw of [undefined, null, "", "dark", "Dark", "transparent", "glass", 42]) {
      expect(resolveThemeMode(raw)).toBe("dark");
    }
  });
});
