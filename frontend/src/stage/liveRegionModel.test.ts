import { describe, expect, it } from "vitest";
import { buildLiveRegionText, extractCaptionText } from "./liveRegionModel";

describe("liveRegionModel pure logic", () => {
  describe("buildLiveRegionText", () => {
    it("returns trimmed caption text when present and loading is false", () => {
      expect(buildLiveRegionText({ captionText: "Good morning, sir.", loading: false })).toBe(
        "Good morning, sir.",
      );
    });

    it("prioritizes caption text over loading state when both are active", () => {
      expect(buildLiveRegionText({ captionText: "  I've found your meeting notes.  ", loading: true })).toBe(
        "I've found your meeting notes.",
      );
    });

    it("returns 'Working' when loading is true and caption is null/empty/whitespace", () => {
      expect(buildLiveRegionText({ captionText: "", loading: true })).toBe("Working");
      expect(buildLiveRegionText({ captionText: "   ", loading: true })).toBe("Working");
      expect(buildLiveRegionText({ captionText: null, loading: true })).toBe("Working");
      expect(buildLiveRegionText({ loading: true })).toBe("Working");
    });

    it("returns empty string when neither caption nor loading is active", () => {
      expect(buildLiveRegionText({ captionText: "", loading: false })).toBe("");
      expect(buildLiveRegionText({ captionText: null, loading: false })).toBe("");
      expect(buildLiveRegionText({})).toBe("");
      expect(buildLiveRegionText()).toBe("");
    });

    it("supports positional arguments (captionText, loading)", () => {
      expect(buildLiveRegionText("Opening browser...", false)).toBe("Opening browser...");
      expect(buildLiveRegionText(null, true)).toBe("Working");
      expect(buildLiveRegionText("", false)).toBe("");
      expect(buildLiveRegionText("   ", true)).toBe("Working");
    });
  });

  describe("extractCaptionText", () => {
    it("extracts text from active event", () => {
      expect(extractCaptionText({ text: "Checking your calendar...", phase: "active" })).toBe(
        "Checking your calendar...",
      );
    });

    it("extracts text from fading event", () => {
      expect(extractCaptionText({ text: "Done.", phase: "fading" })).toBe("Done.");
    });

    it("returns empty string for cleared phase event", () => {
      expect(extractCaptionText({ text: "Old line", phase: "cleared" })).toBe("");
    });

    it("returns empty string for null or undefined event", () => {
      expect(extractCaptionText(null)).toBe("");
      expect(extractCaptionText(undefined)).toBe("");
    });
  });
});
