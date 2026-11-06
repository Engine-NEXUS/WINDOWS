import { describe, expect, it } from "vitest";
import { logoSrc, tileColor } from "./ProviderLogo";

describe("ProviderLogo pure helpers", () => {
  describe("logoSrc", () => {
    it("returns correct svg path for known ids", () => {
      expect(logoSrc("github")).toBe("/activity-logos/github.svg");
      expect(logoSrc("googlecalendar")).toBe("/activity-logos/googlecalendar.svg");
      expect(logoSrc("aider")).toBe("/activity-logos/aider.svg");
      expect(logoSrc("GitHub")).toBe("/activity-logos/github.svg");
      expect(logoSrc("claude")).toBe("/activity-logos/claude.svg");
      expect(logoSrc("gemini")).toBe("/activity-logos/gemini.svg");
      expect(logoSrc("youtube")).toBe("/activity-logos/youtube.svg");
      expect(logoSrc("notepad")).toBe("/activity-logos/notepad.svg");
      expect(logoSrc("vscode")).toBe("/activity-logos/vscode.svg");
    });

    it("returns null for unknown id", () => {
      expect(logoSrc("unknown_app")).toBeNull();
      expect(logoSrc("nonexistent")).toBeNull();
    });

    it("returns null for empty id or whitespace", () => {
      expect(logoSrc("")).toBeNull();
      expect(logoSrc("   ")).toBeNull();
      expect(logoSrc(null)).toBeNull();
      expect(logoSrc(undefined)).toBeNull();
    });

    it("returns null for agent icon (uses monogram fallback)", () => {
      expect(logoSrc("agent")).toBeNull();
      expect(logoSrc("Agent")).toBeNull();
    });
  });

  describe("tileColor", () => {
    it("returns brand color for known ids including new ones", () => {
      expect(tileColor("github")).toBe("#181717");
      expect(tileColor("googlecalendar")).toBe("#4285f4");
      expect(tileColor("aider")).toBe("#14b014");
      expect(tileColor("vercel")).toBe("#000000");
      expect(tileColor("codex")).toBe("#ffffff");
      expect(tileColor("antigravity")).toBe("#ffffff");
      expect(tileColor("copilot")).toBe("#ffffff");
      expect(tileColor("GitHub")).toBe("#181717");
    });

    it("returns undefined for unknown id", () => {
      expect(tileColor("unknown_app")).toBeUndefined();
      expect(tileColor("nonexistent")).toBeUndefined();
    });

    it("returns undefined for empty id or whitespace", () => {
      expect(tileColor("")).toBeUndefined();
      expect(tileColor("   ")).toBeUndefined();
      expect(tileColor(null)).toBeUndefined();
      expect(tileColor(undefined)).toBeUndefined();
    });

    it("returns undefined for agent icon", () => {
      expect(tileColor("agent")).toBeUndefined();
      expect(tileColor("Agent")).toBeUndefined();
    });
  });
});
