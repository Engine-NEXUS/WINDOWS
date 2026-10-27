import { describe, it, expect } from "vitest";
import {
  confidenceLabel,
  securityScore,
  formatBytes,
  sortForDisplay,
  highlightSet,
  formatCategoryLabel,
  groupDetections,
  donutSlicesByCategory,
  donutSlicesByConfidence,
  findTopTechnology,
  filterDetections,
} from "./stackModel";
import type { StackDetection, StackPayload } from "./stackTypes";

describe("stackModel pure helpers", () => {
  describe("confidenceLabel", () => {
    it("returns Confirmed for confidence >= 0.8", () => {
      expect(confidenceLabel(0.8)).toBe("Confirmed");
      expect(confidenceLabel(0.85)).toBe("Confirmed");
      expect(confidenceLabel(1.0)).toBe("Confirmed");
    });

    it("returns Likely for confidence >= 0.5 and < 0.8", () => {
      expect(confidenceLabel(0.5)).toBe("Likely");
      expect(confidenceLabel(0.65)).toBe("Likely");
      expect(confidenceLabel(0.799)).toBe("Likely");
    });

    it("returns Possible for confidence < 0.5 or invalid", () => {
      expect(confidenceLabel(0.49)).toBe("Possible");
      expect(confidenceLabel(0.1)).toBe("Possible");
      expect(confidenceLabel(0)).toBe("Possible");
      expect(confidenceLabel(-0.5)).toBe("Possible");
      expect(confidenceLabel(NaN)).toBe("Possible");
      expect(confidenceLabel(null)).toBe("Possible");
      expect(confidenceLabel(undefined)).toBe("Possible");
    });
  });

  describe("securityScore", () => {
    it("returns 6/6 when all security headers are present and true", () => {
      const sec = {
        hsts: true,
        csp: true,
        x_frame_options: true,
        x_content_type_options: true,
        referrer_policy: true,
        permissions_policy: true,
      };
      const score = securityScore(sec);
      expect(score.passed).toBe(6);
      expect(score.total).toBe(6);
      expect(score.missing).toEqual([]);
    });

    it("returns 0/6 and lists all missing headers when empty or null", () => {
      const scoreNull = securityScore(null);
      expect(scoreNull.passed).toBe(0);
      expect(scoreNull.total).toBe(6);
      expect(scoreNull.missing).toHaveLength(6);

      const scoreEmpty = securityScore({});
      expect(scoreEmpty.passed).toBe(0);
      expect(scoreEmpty.total).toBe(6);
      expect(scoreEmpty.missing).toContain("HSTS");
      expect(scoreEmpty.missing).toContain("CSP");
      expect(scoreEmpty.missing).toContain("X-Frame-Options");
      expect(scoreEmpty.missing).toContain("X-Content-Type-Options");
      expect(scoreEmpty.missing).toContain("Referrer-Policy");
      expect(scoreEmpty.missing).toContain("Permissions-Policy");
    });

    it("calculates partial scores and correctly identifies missing headers", () => {
      const sec = {
        hsts: true,
        csp: false,
        x_frame_options: true,
        x_content_type_options: false,
        referrer_policy: true,
        permissions_policy: false,
      };
      const score = securityScore(sec);
      expect(score.passed).toBe(3);
      expect(score.total).toBe(6);
      expect(score.missing).toEqual(["CSP", "X-Content-Type-Options", "Permissions-Policy"]);
    });
  });

  describe("formatBytes", () => {
    it("handles zero, negative, or invalid bytes", () => {
      expect(formatBytes(0)).toBe("0 B");
      expect(formatBytes(-10)).toBe("0 B");
      expect(formatBytes(NaN)).toBe("0 B");
      expect(formatBytes(null)).toBe("0 B");
      expect(formatBytes(undefined)).toBe("0 B");
    });

    it("formats bytes, kilobytes, and megabytes", () => {
      expect(formatBytes(512)).toBe("512 B");
      expect(formatBytes(1024)).toBe("1.0 KB");
      expect(formatBytes(2048)).toBe("2.0 KB");
      expect(formatBytes(1536)).toBe("1.5 KB");
      expect(formatBytes(1048576)).toBe("1.0 MB");
      expect(formatBytes(2.5 * 1024 * 1024)).toBe("2.5 MB");
      expect(formatBytes(150 * 1024)).toBe("150 KB");
    });
  });

  describe("sortForDisplay", () => {
    it("orders by status: detected > likely > inferred", () => {
      const items: StackDetection[] = [
        { id: "1", name: "Tech C", category: "ui", confidence: 0.9, status: "inferred", evidence: [] },
        { id: "2", name: "Tech A", category: "ui", confidence: 0.9, status: "detected", evidence: [] },
        { id: "3", name: "Tech B", category: "ui", confidence: 0.9, status: "likely", evidence: [] },
      ];
      const sorted = sortForDisplay(items);
      expect(sorted.map((s) => s.id)).toEqual(["2", "3", "1"]);
    });

    it("orders by confidence descending within same status", () => {
      const items: StackDetection[] = [
        { id: "1", name: "Low", category: "ui", confidence: 0.6, status: "detected", evidence: [] },
        { id: "2", name: "High", category: "ui", confidence: 0.95, status: "detected", evidence: [] },
        { id: "3", name: "Mid", category: "ui", confidence: 0.8, status: "detected", evidence: [] },
      ];
      const sorted = sortForDisplay(items);
      expect(sorted.map((s) => s.id)).toEqual(["2", "3", "1"]);
    });

    it("orders by name ascending when status and confidence match", () => {
      const items: StackDetection[] = [
        { id: "1", name: "Zebra", category: "ui", confidence: 0.9, status: "detected", evidence: [] },
        { id: "2", name: "Apple", category: "ui", confidence: 0.9, status: "detected", evidence: [] },
        { id: "3", name: "Mango", category: "ui", confidence: 0.9, status: "detected", evidence: [] },
      ];
      const sorted = sortForDisplay(items);
      expect(sorted.map((s) => s.id)).toEqual(["2", "3", "1"]);
    });

    it("handles null and empty gracefully", () => {
      expect(sortForDisplay(null)).toEqual([]);
      expect(sortForDisplay([])).toEqual([]);
    });
  });

  describe("highlightSet", () => {
    it("creates a set with provided IDs", () => {
      const set = highlightSet(["nextjs", "tailwind"]);
      expect(set.has("nextjs")).toBe(true);
      expect(set.has("tailwind")).toBe(true);
      expect(set.has("vue")).toBe(false);
    });

    it("handles null, undefined, and empty arrays", () => {
      expect(highlightSet(null).size).toBe(0);
      expect(highlightSet(undefined).size).toBe(0);
      expect(highlightSet([]).size).toBe(0);
    });
  });

  describe("formatCategoryLabel", () => {
    it("translates known category slugs to human-friendly labels", () => {
      expect(formatCategoryLabel("javascript_framework")).toBe("JavaScript Frameworks");
      expect(formatCategoryLabel("frontend")).toBe("Frontend Frameworks");
      expect(formatCategoryLabel("analytics")).toBe("Analytics & Tracking");
      expect(formatCategoryLabel("hosting")).toBe("Hosting & Cloud");
      expect(formatCategoryLabel("cdn")).toBe("Content Delivery Network (CDN)");
      expect(formatCategoryLabel("cms")).toBe("Content Management (CMS)");
    });

    it("formats unknown slugs with proper title-casing and acronyms", () => {
      expect(formatCategoryLabel("custom_ui_tool")).toBe("Custom UI Tool");
      expect(formatCategoryLabel("my-api-service")).toBe("My API Service");
      expect(formatCategoryLabel("")).toBe("General");
    });
  });

  describe("groupDetections", () => {
    const mockDetections: StackDetection[] = [
      { id: "nextjs", name: "Next.js", category: "frontend", confidence: 0.95, status: "detected", evidence: ["window.__NEXT_DATA__"] },
      { id: "react", name: "React", category: "frontend", confidence: 0.99, status: "detected", evidence: ["react-root"] },
      { id: "tailwind", name: "Tailwind CSS", category: "ui_library", confidence: 0.9, status: "detected", evidence: ["class utilities"] },
      { id: "stripe", name: "Stripe", category: "ecommerce", confidence: 0.85, status: "detected", evidence: ["js.stripe.com"] },
      { id: "ga4", name: "Google Analytics 4", category: "analytics", confidence: 0.9, status: "detected", evidence: ["gtag"] },
    ];

    it("uses payload.groups when present and consistent", () => {
      const payload: Partial<StackPayload> = {
        detections: mockDetections,
        groups: [
          { category: "frontend", label: "Frameworks", ids: ["nextjs", "react"] },
          { category: "ui_library", label: "Styling", ids: ["tailwind"] },
        ],
      };

      const groups = groupDetections(payload);
      expect(groups).toHaveLength(4); // 2 specified groups + 2 unassigned category groups (ecommerce, analytics)
      expect(groups[0].label).toBe("Frameworks");
      expect(groups[0].detections.map((d) => d.id)).toEqual(["react", "nextjs"]); // sorted by confidence/name
      expect(groups[1].label).toBe("Styling");
      expect(groups[1].detections[0].id).toBe("tailwind");
    });

    it("falls back to category grouping when payload.groups is empty or missing", () => {
      const payload: Partial<StackPayload> = {
        detections: mockDetections,
        groups: [],
      };

      const groups = groupDetections(payload);
      expect(groups.length).toBeGreaterThanOrEqual(4);
      const catNames = groups.map((g) => g.category);
      expect(catNames).toContain("frontend");
      expect(catNames).toContain("ui_library");
      expect(catNames).toContain("ecommerce");
      expect(catNames).toContain("analytics");
    });

    it("handles null/undefined payload or empty detections", () => {
      expect(groupDetections(null)).toEqual([]);
      expect(groupDetections(undefined)).toEqual([]);
      expect(groupDetections({ detections: [] })).toEqual([]);
    });
  });

  describe("donutSlicesByCategory & donutSlicesByConfidence", () => {
    const payload: Partial<StackPayload> = {
      detections: [
        { id: "next", name: "Next.js", category: "frontend", confidence: 0.95, status: "detected", evidence: [] },
        { id: "react", name: "React", category: "frontend", confidence: 0.9, status: "detected", evidence: [] },
        { id: "stripe", name: "Stripe", category: "payments", confidence: 0.7, status: "likely", evidence: [] },
        { id: "custom", name: "Custom Tool", category: "other", confidence: 0.3, status: "inferred", evidence: [] },
      ],
    };

    it("creates donut slices by category with color indices", () => {
      const slices = donutSlicesByCategory(payload);
      expect(slices.length).toBeGreaterThanOrEqual(3);
      const total = slices.reduce((acc, s) => acc + s.value, 0);
      expect(total).toBe(4);
      expect(slices[0].colorIndex).toBe(0);
    });

    it("creates donut slices by confidence tier", () => {
      const slices = donutSlicesByConfidence(payload);
      expect(slices).toEqual([
        { label: "Confirmed", value: 2, colorIndex: 1 },
        { label: "Likely", value: 1, colorIndex: 2 },
        { label: "Possible", value: 1, colorIndex: 0 },
      ]);
    });

    it("returns empty slices for empty payloads", () => {
      expect(donutSlicesByCategory(null)).toEqual([]);
      expect(donutSlicesByConfidence(null)).toEqual([]);
    });
  });

  describe("findTopTechnology & filterDetections", () => {
    const list: StackDetection[] = [
      { id: "react", name: "React", category: "frontend", confidence: 0.9, status: "detected", evidence: [] },
      { id: "next", name: "Next.js", category: "frontend", confidence: 0.98, status: "detected", evidence: [] },
      { id: "tailwind", name: "Tailwind CSS", category: "ui_library", confidence: 0.6, status: "likely", evidence: [] },
    ];

    it("finds top technology with highest confidence", () => {
      const top = findTopTechnology(list);
      expect(top?.id).toBe("next");
      expect(top?.confidence).toBe(0.98);
      expect(findTopTechnology([])).toBeNull();
      expect(findTopTechnology(null)).toBeNull();
    });

    it("prefers a framework over header/plumbing detections of the same status", () => {
      const mixed: StackDetection[] = [
        { id: "hsts", name: "HTTP Strict Transport Security", category: "security", confidence: 0.99, status: "detected", evidence: [] },
        { id: "gtm", name: "Google Tag Manager", category: "tag-manager", confidence: 0.99, status: "detected", evidence: [] },
        { id: "vercel", name: "Vercel", category: "hosting", confidence: 0.9, status: "detected", evidence: [] },
        { id: "nextjs", name: "Next.js", category: "meta-framework", confidence: 0.85, status: "detected", evidence: [] },
      ];
      expect(findTopTechnology(mixed)?.id).toBe("nextjs");
      // a confirmed detection still beats a merely likely framework
      const likely: StackDetection[] = [
        { id: "vue", name: "Vue", category: "frontend-framework", confidence: 0.6, status: "likely", evidence: [] },
        { id: "vercel", name: "Vercel", category: "hosting", confidence: 0.9, status: "detected", evidence: [] },
      ];
      expect(findTopTechnology(likely)?.id).toBe("vercel");
    });

    it("filters detections by query string", () => {
      expect(filterDetections(list, "tailwind")).toHaveLength(1);
      expect(filterDetections(list, "frontend")).toHaveLength(2);
      expect(filterDetections(list, "nonexistent")).toHaveLength(0);
    });

    it("filters detections by confidence tag", () => {
      expect(filterDetections(list, "", "Confirmed")).toHaveLength(2);
      expect(filterDetections(list, "", "Likely")).toHaveLength(1);
      expect(filterDetections(list, "", "Possible")).toHaveLength(0);
      expect(filterDetections(list, "", "All")).toHaveLength(3);
    });
  });
});
