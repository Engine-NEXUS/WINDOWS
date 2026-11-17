import { describe, expect, it } from "vitest";
import { resolveSidebarView } from "./sidebarView";

describe("resolveSidebarView", () => {
  it("shows the alert detail ('coderabbit') in the assistant view", () => {
    expect(resolveSidebarView("coderabbit")).toBe("assistant");
  });
  it("keeps every real view as it is", () => {
    for (const v of ["assistant", "settings", "architect", "pr-list", "spatial", "annotate", "stack", "maps", "place"]) {
      expect(resolveSidebarView(v)).toBe(v);
    }
  });
  it("ignores unknown or empty names", () => {
    expect(resolveSidebarView("nope")).toBeNull();
    expect(resolveSidebarView("")).toBeNull();
    expect(resolveSidebarView(null)).toBeNull();
  });
});
