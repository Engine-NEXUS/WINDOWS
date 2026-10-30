import { describe, expect, it } from "vitest";
import { pinnedView } from "./UnifiedSidebar";

describe("pinned sidebar window", () => {
  it("pins to a known view from ?pin=", () => {
    expect(pinnedView("?pin=architect")).toBe("architect");
    expect(pinnedView("?view=settings&pin=pr-list")).toBe("pr-list");
  });
  it("ignores unknown or missing pins, so the response sidebar keeps following set_view", () => {
    expect(pinnedView("")).toBeNull();
    expect(pinnedView("?view=architect")).toBeNull();
    expect(pinnedView("?pin=bogus")).toBeNull();
    expect(pinnedView("?pin=")).toBeNull();
  });
});
