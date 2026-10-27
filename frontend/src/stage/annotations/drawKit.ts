import { Point, Rect, Size } from "../tourGeometry";

export const DRAW_STYLES = [
  "ring",
  "underline",
  "bracket",
  "circle",
  "arrow",
  "highlight",
  "spotlight",
  "scan",
  "lens",
  "connect",
  "pin",
] as const;

export type DrawStyle = typeof DRAW_STYLES[number];

const DRAW_STYLE_SET = new Set<string>(DRAW_STYLES);

const round = (n: number) => Math.round(n * 10) / 10;

/**
 * Normalizes a raw draw request or falls back to category-driven defaults.
 * Tolerate missing / invalid values gracefully.
 */
export function normalizeDraw(
  raw: string | undefined,
  category: string | undefined,
  target: Rect | null,
  screen: Size,
  opts?: { hasLink?: boolean; hasThumb?: boolean },
): DrawStyle {
  // No target (null) -> return "ring" and let the component draw nothing.
  if (!target) {
    return "ring";
  }

  const rawTrimmed = raw?.trim().toLowerCase();
  const screenArea = screen.w * screen.h;
  const targetArea = target.w * target.h;

  if (rawTrimmed && DRAW_STYLE_SET.has(rawTrimmed)) {
    if (rawTrimmed === "connect") {
      return opts?.hasLink ? "connect" : "ring";
    }
    if (rawTrimmed === "lens") {
      if (!opts?.hasThumb) return "ring";
      if (screenArea > 0 && targetArea > screenArea * 0.3) return "ring";
      return "lens";
    }
    if (rawTrimmed === "spotlight") {
      if (screenArea > 0 && targetArea > screenArea * 0.6) return "ring";
      return "spotlight";
    }
    return rawTrimmed as DrawStyle;
  }

  // Derive by category
  const cat = category?.trim().toLowerCase();
  switch (cat) {
    case "ui_element":
      return "arrow";
    case "text_block":
    case "code":
      return "highlight";
    case "chart":
    case "diagram":
      return "bracket";
    case "person":
    case "product":
    case "food":
    case "object":
    case "other":
    default:
      return "ring";
  }
}

/** Slightly wavy hand-drawn underline along the bottom edge. */
export function underlinePath(t: Rect): string {
  const y0 = round(t.y + t.h + 2);
  const w = t.w;
  const x0 = round(t.x);
  const x1 = round(t.x + w);
  const wave = Math.max(1, Math.min(5, t.h * 0.1));
  const cp1x = round(x0 + w * 0.33);
  const cp1y = round(y0 + wave);
  const cp2x = round(x0 + w * 0.66);
  const cp2y = round(y0 - wave);
  return `M ${x0} ${y0} C ${cp1x} ${cp1y}, ${cp2x} ${cp2y}, ${x1} ${round(y0 + wave * 0.4)}`;
}

export type BracketPaths = [string, string] & { left: string; right: string };

/** Two `[` `]` brackets, left and right of the box. */
export function bracketPaths(t: Rect): BracketPaths {
  const pad = 4;
  const arm = Math.max(2, Math.min(12, t.w * 0.25));
  const lx = round(t.x - pad);
  const rx = round(t.x + t.w + pad);
  const y0 = round(t.y);
  const y1 = round(t.y + t.h);

  const left = `M ${round(lx + arm)} ${y0} L ${lx} ${y0} L ${lx} ${y1} L ${round(lx + arm)} ${y1}`;
  const right = `M ${round(rx - arm)} ${y0} L ${rx} ${y0} L ${rx} ${y1} L ${round(rx - arm)} ${y1}`;

  const res = [left, right] as BracketPaths;
  res.left = left;
  res.right = right;
  return res;
}

/** cx, cy, rx, ry, padded 14 %. */
export function ellipseFor(t: Rect): { cx: number; cy: number; rx: number; ry: number } {
  return {
    cx: round(t.x + t.w / 2),
    cy: round(t.y + t.h / 2),
    rx: round((t.w / 2) * 1.14),
    ry: round((t.h / 2) * 1.14),
  };
}

/** Marker band: x,y,w,h = box inset by 2px. */
export function highlightRect(t: Rect): Rect {
  return {
    x: round(t.x + 2),
    y: round(t.y + 2),
    w: round(Math.max(1, t.w - 4)),
    h: round(Math.max(1, t.h - 4)),
  };
}

/** Top-left corner offset for teardrop pin placement. */
export function pinPos(t: Rect): Point {
  return {
    x: round(t.x),
    y: round(t.y),
  };
}

export function connectEndpoints(a: Rect, b: Rect): { from: Point; to: Point } {
  const edges = (r: Rect) => [
    { p: { x: r.x + r.w / 2, y: r.y }, n: { x: 0, y: -1 } }, // top
    { p: { x: r.x + r.w / 2, y: r.y + r.h }, n: { x: 0, y: 1 } }, // bottom
    { p: { x: r.x, y: r.y + r.h / 2 }, n: { x: -1, y: 0 } }, // left
    { p: { x: r.x + r.w, y: r.y + r.h / 2 }, n: { x: 1, y: 0 } }, // right
  ];

  let bestDist = Infinity;
  let pA = { x: a.x + a.w, y: a.y + a.h / 2 };
  let pB = { x: b.x, y: b.y + b.h / 2 };

  for (const ea of edges(a)) {
    for (const eb of edges(b)) {
      const d = Math.hypot(ea.p.x - eb.p.x, ea.p.y - eb.p.y);
      if (d < bestDist) {
        bestDist = d;
        pA = ea.p;
        pB = eb.p;
      }
    }
  }

  return { from: { x: round(pA.x), y: round(pA.y) }, to: { x: round(pB.x), y: round(pB.y) } };
}

/**
 * Cubic Bézier from the centre of a's nearest edge to b's nearest edge,
 * control points pulled perpendicular.
 */
export function connectPath(a: Rect, b: Rect): string {
  const edges = (r: Rect) => [
    { p: { x: r.x + r.w / 2, y: r.y }, n: { x: 0, y: -1 } }, // top
    { p: { x: r.x + r.w / 2, y: r.y + r.h }, n: { x: 0, y: 1 } }, // bottom
    { p: { x: r.x, y: r.y + r.h / 2 }, n: { x: -1, y: 0 } }, // left
    { p: { x: r.x + r.w, y: r.y + r.h / 2 }, n: { x: 1, y: 0 } }, // right
  ];

  let bestDist = Infinity;
  let pA = { x: a.x + a.w, y: a.y + a.h / 2 };
  let nA = { x: 1, y: 0 };
  let pB = { x: b.x, y: b.y + b.h / 2 };
  let nB = { x: -1, y: 0 };

  for (const ea of edges(a)) {
    for (const eb of edges(b)) {
      const d = Math.hypot(ea.p.x - eb.p.x, ea.p.y - eb.p.y);
      if (d < bestDist) {
        bestDist = d;
        pA = ea.p;
        nA = ea.n;
        pB = eb.p;
        nB = eb.n;
      }
    }
  }

  const pull = Math.max(20, bestDist * 0.4);
  const cp1 = { x: pA.x + nA.x * pull, y: pA.y + nA.y * pull };
  const cp2 = { x: pB.x + nB.x * pull, y: pB.y + nB.y * pull };

  return `M ${round(pA.x)} ${round(pA.y)} C ${round(cp1.x)} ${round(cp1.y)}, ${round(cp2.x)} ${round(cp2.y)}, ${round(pB.x)} ${round(pB.y)}`;
}

/** One SVG path with fill-rule: evenodd (full viewport rectangle + rounded-rect hole). */
export function spotlightPath(viewport: Size, t: Rect, radius = 8): string {
  const vw = Math.max(0, viewport.w);
  const vh = Math.max(0, viewport.h);
  const r = Math.max(0, Math.min(radius, t.w / 2, t.h / 2));
  const x0 = round(t.x);
  const y0 = round(t.y);
  const w = round(t.w);
  const h = round(t.h);

  const outer = `M 0 0 H ${round(vw)} V ${round(vh)} H 0 Z`;
  const hole = `M ${round(x0 + r)} ${y0} ` +
    `H ${round(x0 + w - r)} ` +
    `A ${r} ${r} 0 0 1 ${round(x0 + w)} ${round(y0 + r)} ` +
    `V ${round(y0 + h - r)} ` +
    `A ${r} ${r} 0 0 1 ${round(x0 + w - r)} ${round(y0 + h)} ` +
    `H ${round(x0 + r)} ` +
    `A ${r} ${r} 0 0 1 ${x0} ${round(y0 + h - r)} ` +
    `V ${round(y0 + r)} ` +
    `A ${r} ${r} 0 0 1 ${round(x0 + r)} ${y0} Z`;

  return `${outer} ${hole}`;
}

export interface LensGeometry {
  cx: number;
  cy: number;
  r: number;
  radius: number;
  size: number;
  connectorStart: Point;
  connectorEnd: Point;
  connector: { from: Point; to: Point };
}

/**
 * Bubble centre placed on the side of the target with the most free space,
 * fully inside the viewport with a 16 px margin, plus the connector points.
 */
export function lensGeometry(t: Rect, viewport: Size, size = 170): LensGeometry {
  const margin = 16;
  const radius = size / 2;
  const gap = 24;

  const freeSpace = [
    { side: "right", space: viewport.w - (t.x + t.w) },
    { side: "left", space: t.x },
    { side: "below", space: viewport.h - (t.y + t.h) },
    { side: "above", space: t.y },
  ];
  freeSpace.sort((a, b) => b.space - a.space);
  const bestSide = freeSpace[0].side;

  const tcx = t.x + t.w / 2;
  const tcy = t.y + t.h / 2;

  let rawCx = tcx;
  let rawCy = tcy;

  if (bestSide === "right") {
    rawCx = t.x + t.w + gap + radius;
  } else if (bestSide === "left") {
    rawCx = t.x - gap - radius;
  } else if (bestSide === "below") {
    rawCy = t.y + t.h + gap + radius;
  } else {
    rawCy = t.y - gap - radius;
  }

  // Fully inside the viewport with 16 px margin
  const minX = margin + radius;
  const maxX = Math.max(minX, viewport.w - margin - radius);
  const minY = margin + radius;
  const maxY = Math.max(minY, viewport.h - margin - radius);

  const cx = round(Math.max(minX, Math.min(maxX, rawCx)));
  const cy = round(Math.max(minY, Math.min(maxY, rawCy)));

  const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(hi, v));
  const connectorStart: Point = {
    x: round(clamp(cx, t.x, t.x + t.w)),
    y: round(clamp(cy, t.y, t.y + t.h)),
  };

  const dx = connectorStart.x - cx;
  const dy = connectorStart.y - cy;
  const dist = Math.hypot(dx, dy) || 1;
  const connectorEnd: Point = {
    x: round(cx + (dx / dist) * radius),
    y: round(cy + (dy / dist) * radius),
  };

  return {
    cx,
    cy,
    r: radius,
    radius,
    size,
    connectorStart,
    connectorEnd,
    connector: { from: connectorStart, to: connectorEnd },
  };
}
