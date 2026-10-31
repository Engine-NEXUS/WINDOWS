import { describe, expect, it } from "vitest";
import { isEmbedSrc } from "./MapsPanel";
import { panelResizeDelta } from "./dockKeys";

describe("maps panel source guard", () => {
  it("accepts only Google's embed endpoint", () => {
    expect(isEmbedSrc("https://www.google.com/maps/embed/v1/place?key=K&q=Charminar")).toBe(true);
    for (const bad of [null, undefined, 5, "", "https://evil.example/maps/embed/v1/place", "http://www.google.com/maps/embed/v1/place", "https://www.google.com/maps/search/?q=x", "https://www.google.com/maps/embed/v1/place?q=a b", "javascript:alert(1)"]) {
      expect(isEmbedSrc(bad as unknown), String(bad)).toBe(false);
    }
  });
});

describe("panel resize keys", () => {
  const key = (k: string, extra: Partial<KeyboardEvent> = {}) => ({ key: k, shiftKey: true, ctrlKey: false, altKey: false, metaKey: false, target: null, ...extra }) as unknown as KeyboardEvent;
  it("Shift + Right widens and Shift + Left narrows", () => {
    expect(panelResizeDelta(key("ArrowRight"))).toBeGreaterThan(0);
    expect(panelResizeDelta(key("ArrowLeft"))).toBeLessThan(0);
  });
  it("needs Shift and ignores other keys, other modifiers and text fields", () => {
    expect(panelResizeDelta(key("ArrowRight", { shiftKey: false }))).toBe(0);
    expect(panelResizeDelta(key("ArrowUp"))).toBe(0);
    expect(panelResizeDelta(key("ArrowRight", { ctrlKey: true }))).toBe(0);
    expect(panelResizeDelta(key("ArrowRight", { target: { tagName: "INPUT" } as unknown as EventTarget }))).toBe(0);
  });
});
