/**
 * Math utilities for Lens hover magnifier and ZoomViewer pan/zoom.
 * Pure functions, zero dependencies.
 */

export interface Point {
  x: number;
  y: number;
}

export interface Dimensions {
  w: number;
  h: number;
}

export interface LensBackgroundResult {
  bgX: number;
  bgY: number;
  bgW: number;
  bgH: number;
  left: number;
  top: number;
}

export interface ViewTransform {
  scale: number;
  tx: number;
  ty: number;
}

export interface ZoomLimits {
  min: number;
  max: number;
}

/**
 * Calculates background position/size and bubble offset for hover lens.
 * The bubble shows the area under the pointer magnified by `zoom`.
 * The bubble's (left, top) is centered on the pointer but clamped so the
 * bubble stays fully inside the box.
 */
export function lensBackground(
  pointer: Point,
  box: Dimensions,
  zoom: number,
  lensSize: number
): LensBackgroundResult {
  if (box.w <= 0 || box.h <= 0) {
    return {
      bgX: 0,
      bgY: 0,
      bgW: 0,
      bgH: 0,
      left: 0,
      top: 0,
    };
  }

  const maxLeft = Math.max(0, box.w - lensSize);
  const maxTop = Math.max(0, box.h - lensSize);

  const left = Math.max(0, Math.min(pointer.x - lensSize / 2, maxLeft));
  const top = Math.max(0, Math.min(pointer.y - lensSize / 2, maxTop));

  const bgW = box.w * zoom;
  const bgH = box.h * zoom;

  const bgX = lensSize / 2 - pointer.x * zoom;
  const bgY = lensSize / 2 - pointer.y * zoom;

  return { bgX, bgY, bgW, bgH, left, top };
}

/**
 * Zooms about the anchor point so that the content coordinate under
 * the anchor stays fixed before and after the zoom. Clamps scale to limits.
 */
export function zoomAt(
  view: ViewTransform,
  factor: number,
  anchor: Point,
  limits: ZoomLimits = { min: 1, max: 8 }
): ViewTransform {
  const min = limits?.min ?? 1;
  const max = limits?.max ?? 8;

  if (view.scale <= 0 || !Number.isFinite(view.scale)) {
    return { scale: min, tx: view.tx || 0, ty: view.ty || 0 };
  }

  const rawScale = view.scale * factor;
  const nextScale = Math.min(Math.max(rawScale, min), max);

  const effectiveFactor = nextScale / view.scale;
  const nextTx = anchor.x - effectiveFactor * (anchor.x - view.tx);
  const nextTy = anchor.y - effectiveFactor * (anchor.y - view.ty);

  return {
    scale: nextScale,
    tx: Number.isFinite(nextTx) ? nextTx : 0,
    ty: Number.isFinite(nextTy) ? nextTy : 0,
  };
}

/**
 * Clamps pan translation so the image cannot be dragged fully out of frame.
 * At least 25% of the scaled content must stay visible in the frame.
 * At scale <= 1 the pan is centred and locked.
 */
export function clampPan(
  view: ViewTransform,
  content: Dimensions,
  frame: Dimensions
): ViewTransform {
  if (content.w <= 0 || content.h <= 0 || frame.w <= 0 || frame.h <= 0) {
    return { scale: view.scale, tx: 0, ty: 0 };
  }

  // At scale <= 1 the pan is centred/locked
  if (view.scale <= 1) {
    const tx = (frame.w - content.w * view.scale) / 2;
    const ty = (frame.h - content.h * view.scale) / 2;
    return {
      scale: view.scale,
      tx: Number.isFinite(tx) ? tx : 0,
      ty: Number.isFinite(ty) ? ty : 0,
    };
  }

  const sw = content.w * view.scale;
  const sh = content.h * view.scale;

  // At least 25% of the scaled content must stay visible in the frame:
  // Leftmost drag: right edge of image is at least 0.25 * sw inside the frame
  //   tx + sw >= 0.25 * sw  =>  tx >= -0.75 * sw
  // Rightmost drag: left edge of image leaves at least 0.25 * sw inside the frame
  //   frame.w - tx >= 0.25 * sw  =>  tx <= frame.w - 0.25 * sw
  const minTx = -0.75 * sw;
  const maxTx = frame.w - 0.25 * sw;

  const minTy = -0.75 * sh;
  const maxTy = frame.h - 0.25 * sh;

  const clampedTx = Math.min(Math.max(view.tx, minTx), maxTx);
  const clampedTy = Math.min(Math.max(view.ty, minTy), maxTy);

  return {
    scale: view.scale,
    tx: Number.isFinite(clampedTx) ? clampedTx : 0,
    ty: Number.isFinite(clampedTy) ? clampedTy : 0,
  };
}
