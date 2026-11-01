import { describe, expect, it } from "vitest";
import { formatKm, formatMinutes, formatRupees, isPlaceCard, safeImageSrc } from "./placeCardModel";
import { pinnedView } from "./UnifiedSidebar";

describe("place card model", () => {
  it("accepts only payloads with a name and a position", () => {
    expect(isPlaceCard({ name: "Charminar", lat: 17.36, lon: 78.47 })).toBe(true);
    for (const bad of [null, undefined, 5, {}, { name: "", lat: 1, lon: 1 }, { name: "x", lat: "1", lon: 1 }]) expect(isPlaceCard(bad)).toBe(false);
  });
  it("shows only inline image data", () => {
    expect(safeImageSrc("data:image/jpeg;base64,AAAA")).toBe("data:image/jpeg;base64,AAAA");
    for (const bad of ["https://evil.example/x.jpg", "javascript:alert(1)", "data:text/html;base64,AAAA", "", null, 4]) expect(safeImageSrc(bad)).toBeNull();
  });
  it("formats distance, time and rupees", () => {
    expect(formatKm(4.26)).toBe("4.3 km");
    expect(formatKm(22.6)).toBe("23 km");
    expect(formatMinutes(48)).toBe("48 min");
    expect(formatMinutes(125)).toBe("2 h 5 min");
    expect(formatMinutes(120)).toBe("2 h");
    expect(formatRupees(145, 170)).toBe("₹145–170");
    expect(formatRupees(30, 30)).toBe("₹30");
  });
  it("is a pinned view", () => {
    expect(pinnedView("?pin=place")).toBe("place");
  });
});
