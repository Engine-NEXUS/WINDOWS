import { Point, Rect, Size } from "../tourGeometry";

let lastPoint: Point | null = null;

export function getLastPoint(): Point | null {
  return lastPoint;
}

export function setLastPoint(p: Point | null): void {
  lastPoint = p;
}

export function resetPointerTrail(): void {
  lastPoint = null;
}

export interface PointerTrailData {
  start: Point;
  end: Point;
  dx: number;
  dy: number;
}

/**
 * Computes the pointer start position and delta to the new target centre,
 * updating the module-level lastPoint.
 * First callout: starts at orb centre if known, else viewport top-centre.
 */
export function computePointerTrail(
  target: Rect | null,
  viewport: Size,
  orb?: Rect | null,
): PointerTrailData | null {
  if (!target) return null;

  const targetTopLeft: Point = {
    x: Math.round(target.x),
    y: Math.round(target.y),
  };

  let start: Point;
  if (lastPoint) {
    start = lastPoint;
  } else if (orb) {
    start = {
      x: Math.round(orb.x + orb.w / 2),
      y: Math.round(orb.y + orb.h / 2),
    };
  } else {
    start = {
      x: Math.round(viewport.w / 2),
      y: 0,
    };
  }

  const dx = targetTopLeft.x - start.x;
  const dy = targetTopLeft.y - start.y;

  lastPoint = targetTopLeft;

  return { start, end: targetTopLeft, dx, dy };
}
