import type {
  StackDetection,
  StackPayload,
  StackSecurity,
  DisplayGroup,
  SecurityScore,
} from "./stackTypes";

/**
 * Returns human-readable confidence label:
 * - "Confirmed" (≥ 0.8)
 * - "Likely" (≥ 0.5)
 * - "Possible" (< 0.5)
 */
export function confidenceLabel(confidence: number | null | undefined): "Confirmed" | "Likely" | "Possible" {
  if (typeof confidence !== "number" || isNaN(confidence)) {
    return "Possible";
  }
  if (confidence >= 0.8) {
    return "Confirmed";
  }
  if (confidence >= 0.5) {
    return "Likely";
  }
  return "Possible";
}

/**
 * Known security headers for evaluation (6 headers total).
 */
export const SECURITY_HEADER_KEYS: Array<{ key: keyof StackSecurity; label: string }> = [
  { key: "hsts", label: "HSTS" },
  { key: "csp", label: "CSP" },
  { key: "x_frame_options", label: "X-Frame-Options" },
  { key: "x_content_type_options", label: "X-Content-Type-Options" },
  { key: "referrer_policy", label: "Referrer-Policy" },
  { key: "permissions_policy", label: "Permissions-Policy" },
];

/**
 * Computes security headers score:
 * passed: count of true headers
 * total: 6
 * missing: labels of missing/falsy headers
 */
export function securityScore(security?: Partial<StackSecurity> | null): SecurityScore {
  const missing: string[] = [];
  let passed = 0;

  for (const { key, label } of SECURITY_HEADER_KEYS) {
    if (security && Boolean(security[key])) {
      passed += 1;
    } else {
      missing.push(label);
    }
  }

  return {
    passed,
    total: SECURITY_HEADER_KEYS.length,
    missing,
  };
}

/**
 * Format raw byte count into human-readable size string.
 */
export function formatBytes(bytes: number | null | undefined): string {
  if (typeof bytes !== "number" || isNaN(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  const k = 1024;
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  const idx = Math.min(i, units.length - 1);
  if (idx === 0) {
    return `${Math.round(bytes)} B`;
  }
  const val = bytes / Math.pow(k, idx);
  const formatted = val < 10 ? val.toFixed(1) : Math.round(val).toString();
  return `${formatted} ${units[idx]}`;
}

const STATUS_ORDER: Record<string, number> = {
  detected: 0,
  likely: 1,
  inferred: 2,
};

/**
 * Sorts detections for display:
 * 1. Status: "detected" > "likely" > "inferred"
 * 2. Confidence: descending (highest first)
 * 3. Name: ascending (alphabetical, case-insensitive)
 */
export function sortForDisplay(detections: StackDetection[] | null | undefined): StackDetection[] {
  if (!Array.isArray(detections)) return [];

  return [...detections].sort((a, b) => {
    const statusA = STATUS_ORDER[a?.status] ?? 3;
    const statusB = STATUS_ORDER[b?.status] ?? 3;
    if (statusA !== statusB) {
      return statusA - statusB;
    }

    const confA = typeof a?.confidence === "number" ? a.confidence : 0;
    const confB = typeof b?.confidence === "number" ? b.confidence : 0;
    if (confA !== confB) {
      return confB - confA;
    }

    const nameA = a?.name || "";
    const nameB = b?.name || "";
    return nameA.localeCompare(nameB, undefined, { sensitivity: "base" });
  });
}

/**
 * Returns a Set from array of technology IDs for fast O(1) membership checks.
 */
export function highlightSet(ids: string[] | null | undefined): Set<string> {
  return new Set(Array.isArray(ids) ? ids.filter(Boolean) : []);
}

const KNOWN_ACRONYMS = new Set(["UI", "JS", "CSS", "HTML", "API", "CDN", "CMS", "SEO", "SDK", "HSTS", "CSP"]);

/**
 * Translates a raw category slug (e.g. "ui_framework", "javascript_framework") into a clean display label.
 */
export function formatCategoryLabel(category: string | null | undefined): string {
  if (!category || typeof category !== "string") return "General";

  const lower = category.toLowerCase().trim();
  const KNOWN_MAP: Record<string, string> = {
    javascript_framework: "JavaScript Frameworks",
    frontend: "Frontend Frameworks",
    backend: "Backend & APIs",
    ui_library: "UI Libraries & Styling",
    css_framework: "CSS Frameworks",
    styling: "UI Libraries & Styling",
    analytics: "Analytics & Tracking",
    hosting: "Hosting & Cloud",
    infrastructure: "Hosting & Infrastructure",
    cdn: "Content Delivery Network (CDN)",
    cms: "Content Management (CMS)",
    ecommerce: "E-Commerce & Payments",
    payments: "Payments",
    database: "Databases & Storage",
    security: "Security & Privacy",
    fonts: "Web Fonts",
    marketing: "Marketing & Tags",
    tag_manager: "Tag Managers",
    web_server: "Web Server",
  };

  if (KNOWN_MAP[lower]) {
    return KNOWN_MAP[lower];
  }

  // Generic title casing: split by _, -, or space
  return lower
    .split(/[-_\s]+/)
    .map((word) => {
      const up = word.toUpperCase();
      if (KNOWN_ACRONYMS.has(up)) return up;
      return word.charAt(0).toUpperCase() + word.slice(1);
    })
    .join(" ");
}

/**
 * Groups detections for display.
 * Uses `payload.groups` when present and consistent (mapping group.ids to detections).
 * Falls back to grouping detections by category with the same label rules.
 * Detections in each group are sorted via `sortForDisplay`.
 */
export function groupDetections(payload?: Partial<StackPayload> | null): DisplayGroup[] {
  if (!payload) return [];

  const detections = Array.isArray(payload.detections) ? payload.detections : [];
  if (detections.length === 0) return [];

  const detectionMap = new Map<string, StackDetection>();
  for (const d of detections) {
    if (d && d.id) {
      detectionMap.set(d.id, d);
    }
  }

  const rawGroups = payload.groups;
  const isConsistent =
    Array.isArray(rawGroups) &&
    rawGroups.length > 0 &&
    rawGroups.every(
      (g) =>
        g &&
        typeof g.category === "string" &&
        Array.isArray(g.ids) &&
        g.ids.length > 0 &&
        g.ids.some((id) => detectionMap.has(id)),
    );

  if (isConsistent) {
    const result: DisplayGroup[] = [];
    const usedIds = new Set<string>();

    for (const g of rawGroups) {
      const groupDetections: StackDetection[] = [];
      for (const id of g.ids) {
        const item = detectionMap.get(id);
        if (item) {
          groupDetections.push(item);
          usedIds.add(id);
        }
      }

      if (groupDetections.length > 0) {
        result.push({
          category: g.category,
          label: g.label || formatCategoryLabel(g.category),
          detections: sortForDisplay(groupDetections),
        });
      }
    }

    // Capture any detections not included in any group
    const remaining = detections.filter((d) => d && d.id && !usedIds.has(d.id));
    if (remaining.length > 0) {
      const remainingByCategory = new Map<string, StackDetection[]>();
      for (const d of remaining) {
        const cat = d.category || "other";
        if (!remainingByCategory.has(cat)) remainingByCategory.set(cat, []);
        remainingByCategory.get(cat)!.push(d);
      }

      for (const [cat, items] of remainingByCategory) {
        result.push({
          category: cat,
          label: formatCategoryLabel(cat),
          detections: sortForDisplay(items),
        });
      }
    }

    return result;
  }

  // Fallback: group all detections by category
  const byCategory = new Map<string, StackDetection[]>();
  for (const d of detections) {
    const cat = d.category || "other";
    if (!byCategory.has(cat)) byCategory.set(cat, []);
    byCategory.get(cat)!.push(d);
  }

  const result: DisplayGroup[] = [];
  for (const [cat, items] of byCategory) {
    result.push({
      category: cat,
      label: formatCategoryLabel(cat),
      detections: sortForDisplay(items),
    });
  }

  return result;
}

export interface StackChartSlice {
  label: string;
  value: number;
  colorIndex: number;
}

/**
 * Creates Donut slices grouped by category using groupDetections.
 */
export function donutSlicesByCategory(payload?: Partial<StackPayload> | null): StackChartSlice[] {
  const groups = groupDetections(payload);
  return groups.map((g, idx) => ({
    label: g.label,
    value: g.detections.length,
    colorIndex: idx % 8,
  }));
}

/**
 * Creates Donut slices grouped by confidence status (Confirmed, Likely, Possible).
 */
export function donutSlicesByConfidence(payload?: Partial<StackPayload> | null): StackChartSlice[] {
  if (!payload || !Array.isArray(payload.detections) || payload.detections.length === 0) {
    return [];
  }

  let confirmed = 0;
  let likely = 0;
  let possible = 0;

  for (const d of payload.detections) {
    if (!d) continue;
    const tag = confidenceLabel(d.confidence);
    if (tag === "Confirmed") confirmed++;
    else if (tag === "Likely") likely++;
    else possible++;
  }

  const slices: StackChartSlice[] = [];
  if (confirmed > 0) slices.push({ label: "Confirmed", value: confirmed, colorIndex: 1 });
  if (likely > 0) slices.push({ label: "Likely", value: likely, colorIndex: 2 });
  if (possible > 0) slices.push({ label: "Possible", value: possible, colorIndex: 0 });

  return slices;
}

/**
 * Finds the detection with the highest confidence for the hero card top badge.
 */
/** What best describes "what this site is built with": frameworks first, plumbing last. */
const TOP_CATEGORY_RANK: Record<string, number> = {
  "meta-framework": 0,
  "frontend-framework": 1,
  cms: 2,
  "headless-cms": 2,
  ecommerce: 2,
  "backend-framework": 3,
  "programming-language": 4,
  "backend-language": 4,
  hosting: 5,
  "css-framework": 6,
  "ui-library": 7,
  "database-baas": 7,
  cdn: 8,
};
const TOP_CATEGORY_DEFAULT_RANK = 20;

export function findTopTechnology(detections?: StackDetection[] | null): StackDetection | null {
  if (!Array.isArray(detections) || detections.length === 0) return null;
  const rank = (d: StackDetection) => TOP_CATEGORY_RANK[d?.category] ?? TOP_CATEGORY_DEFAULT_RANK;
  const sorted = sortForDisplay(detections).sort((a, b) => {
    // Keep the status order (confirmed before likely before inferred), then prefer
    // the more descriptive category; sortForDisplay already ordered ties by confidence.
    const statusA = STATUS_ORDER[a?.status] ?? 3;
    const statusB = STATUS_ORDER[b?.status] ?? 3;
    if (statusA !== statusB) return statusA - statusB;
    return rank(a) - rank(b);
  });
  return sorted[0] || null;
}

/**
 * Filters detections by a search term (name/category) and confidence filter.
 */
export function filterDetections(
  detections: StackDetection[] | null | undefined,
  query: string,
  filterConfidence: string = "All",
): StackDetection[] {
  if (!Array.isArray(detections)) return [];

  const q = query.trim().toLowerCase();
  const confFilter = filterConfidence.trim().toLowerCase();

  return detections.filter((d) => {
    if (!d) return false;

    // Confidence filter check
    if (confFilter && confFilter !== "all") {
      const tag = confidenceLabel(d.confidence).toLowerCase();
      if (tag !== confFilter) {
        return false;
      }
    }

    // Search query check (matches name or category)
    if (q) {
      const nameMatch = (d.name || "").toLowerCase().includes(q);
      const catMatch = (d.category || "").toLowerCase().includes(q);
      const labelMatch = formatCategoryLabel(d.category).toLowerCase().includes(q);
      if (!nameMatch && !catMatch && !labelMatch) {
        return false;
      }
    }

    return true;
  });
}
