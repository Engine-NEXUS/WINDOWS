import React, { useState, useRef, useEffect, useCallback } from "react";
import {
  zoomAt,
  clampPan,
  type ViewTransform,
  type Dimensions,
} from "./lensMath";
import "./lens.css";

export interface ZoomViewerProps {
  src: string;
  alt: string;
}

/**
 * ZoomViewer — Pan-and-zoom interactive viewer for lightbox.
 * Supports:
 * - Smooth wheel zoom about the pointer (`1.0015 ** -deltaY`)
 * - Pointer drag pan with `setPointerCapture`
 * - Double-click toggle (scale 1 ↔ 3 at click anchor)
 * - Accessible keyboard zoom (`+`, `-`, `0` when focused)
 * - Float toolbar with `+`, `−`, `Reset`, and live `NNN %` readout
 * - Clamped pan ensuring at least 25% content stays visible
 */
export function ZoomViewer({ src, alt }: ZoomViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);

  const [naturalDim, setNaturalDim] = useState<Dimensions>({ w: 0, h: 0 });
  const [frameDim, setFrameDim] = useState<Dimensions>({ w: 0, h: 0 });
  const [contentDim, setContentDim] = useState<Dimensions>({ w: 0, h: 0 });
  const [view, setView] = useState<ViewTransform>({ scale: 1, tx: 0, ty: 0 });
  const [isDragging, setIsDragging] = useState(false);

  // Keep refs up-to-date for event listeners and callbacks
  const frameRef = useRef(frameDim);
  frameRef.current = frameDim;
  const contentRef = useRef(contentDim);
  contentRef.current = contentDim;
  const viewRef = useRef(view);
  viewRef.current = view;

  const isDraggingRef = useRef(false);
  const dragStartRef = useRef<{ x: number; y: number; tx: number; ty: number }>({
    x: 0,
    y: 0,
    tx: 0,
    ty: 0,
  });

  // Calculate fitted content dimensions and center inside frame at scale 1
  const updateLayout = useCallback(
    (frame: Dimensions, natural: Dimensions) => {
      if (frame.w <= 0 || frame.h <= 0) return;
      const nw = natural.w > 0 ? natural.w : frame.w;
      const nh = natural.h > 0 ? natural.h : frame.h;

      // Fit within 96% of frame space to leave clean border margins
      const maxW = Math.max(10, frame.w * 0.96);
      const maxH = Math.max(10, frame.h * 0.96);
      const fitScale = Math.min(maxW / nw, maxH / nh, 1);

      const baseW = Math.max(1, Math.round(nw * fitScale));
      const baseH = Math.max(1, Math.round(nh * fitScale));
      const content = { w: baseW, h: baseH };

      setContentDim(content);
      contentRef.current = content;

      // Reset / clamp view to scale 1 centered
      setView((cur) => {
        if (cur.scale <= 1) {
          return clampPan({ scale: 1, tx: 0, ty: 0 }, content, frame);
        }
        return clampPan(cur, content, frame);
      });
    },
    []
  );

  // ResizeObserver for frame dimensions
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const measure = () => {
      const rect = container.getBoundingClientRect();
      const frame = { w: Math.round(rect.width), h: Math.round(rect.height) };
      setFrameDim(frame);
      frameRef.current = frame;
      updateLayout(frame, naturalDim);
    };

    measure();

    let ro: ResizeObserver | null = null;
    if (typeof ResizeObserver !== "undefined") {
      ro = new ResizeObserver(() => measure());
      ro.observe(container);
    }

    return () => {
      ro?.disconnect();
    };
  }, [naturalDim, updateLayout]);

  // Image load handler to get intrinsic resolution
  const handleImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
    const img = e.currentTarget;
    const natural = {
      w: img.naturalWidth || img.width || 800,
      h: img.naturalHeight || img.height || 600,
    };
    setNaturalDim(natural);
    updateLayout(frameRef.current, natural);
  };

  // Non-passive wheel zoom effect about pointer anchor
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = container.getBoundingClientRect();
      const anchor = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };

      const factor = Math.pow(1.0015, -e.deltaY);

      setView((cur) => {
        const next = zoomAt(cur, factor, anchor, { min: 1, max: 8 });
        return clampPan(next, contentRef.current, frameRef.current);
      });
    };

    container.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      container.removeEventListener("wheel", onWheel);
    };
  }, []);

  // Pointer drag panning
  const handlePointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {}
    isDraggingRef.current = true;
    setIsDragging(true);
    dragStartRef.current = {
      x: e.clientX,
      y: e.clientY,
      tx: viewRef.current.tx,
      ty: viewRef.current.ty,
    };
  };

  const handlePointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    if (!isDraggingRef.current) return;
    const dx = e.clientX - dragStartRef.current.x;
    const dy = e.clientY - dragStartRef.current.y;

    const nextView = {
      scale: viewRef.current.scale,
      tx: dragStartRef.current.tx + dx,
      ty: dragStartRef.current.ty + dy,
    };
    setView(clampPan(nextView, contentRef.current, frameRef.current));
  };

  const handlePointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    if (isDraggingRef.current) {
      isDraggingRef.current = false;
      setIsDragging(false);
      try {
        e.currentTarget.releasePointerCapture(e.pointerId);
      } catch {}
    }
  };

  // Double-click toggle (1 ↔ 3)
  const handleDoubleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    const anchor = {
      x: e.clientX - rect.left,
      y: e.clientY - rect.top,
    };

    setView((cur) => {
      const targetScale = cur.scale > 1.05 ? 1 : 3;
      const factor = targetScale / cur.scale;
      const next = zoomAt(cur, factor, anchor, { min: 1, max: 8 });
      return clampPan(next, contentRef.current, frameRef.current);
    });
  };

  // Zoom by step factor about the frame center
  const zoomByStep = (factor: number) => {
    const frame = frameRef.current;
    const centerAnchor = { x: frame.w / 2, y: frame.h / 2 };
    setView((cur) => {
      const next = zoomAt(cur, factor, centerAnchor, { min: 1, max: 8 });
      return clampPan(next, contentRef.current, frame);
    });
  };

  // Reset zoom to scale 1
  const resetZoom = () => {
    setView(clampPan({ scale: 1, tx: 0, ty: 0 }, contentRef.current, frameRef.current));
  };

  // Keyboard navigation (+, -, 0)
  const handleKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key === "+" || e.key === "=") {
      e.preventDefault();
      zoomByStep(1.25);
    } else if (e.key === "-" || e.key === "_") {
      e.preventDefault();
      zoomByStep(1 / 1.25);
    } else if (e.key === "0") {
      e.preventDefault();
      resetZoom();
    }
  };

  const cursorStyle = view.scale > 1 ? (isDragging ? "grabbing" : "grab") : "default";

  return (
    <div
      ref={containerRef}
      className="nexus-zoom-viewer"
      tabIndex={0}
      role="region"
      aria-label={`Interactive zoom viewer for ${alt}`}
      style={{ cursor: cursorStyle }}
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onPointerCancel={handlePointerUp}
      onDoubleClick={handleDoubleClick}
      onKeyDown={handleKeyDown}
    >
      <img
        ref={imgRef}
        src={src}
        alt={alt}
        className="nexus-zoom-img"
        onLoad={handleImageLoad}
        draggable={false}
        style={{
          width: contentDim.w > 0 ? `${contentDim.w}px` : "auto",
          height: contentDim.h > 0 ? `${contentDim.h}px` : "auto",
          transform: `translate(${view.tx}px, ${view.ty}px) scale(${view.scale})`,
        }}
      />

      <div
        className="nexus-zoom-toolbar"
        role="toolbar"
        aria-label="Image zoom controls"
        onClick={(e) => e.stopPropagation()}
        onPointerDown={(e) => e.stopPropagation()}
      >
        <button
          type="button"
          className="nexus-zoom-btn"
          onClick={() => zoomByStep(1.25)}
          title="Zoom in (+)"
          aria-label="Zoom in"
        >
          +
        </button>
        <button
          type="button"
          className="nexus-zoom-btn"
          onClick={() => zoomByStep(1 / 1.25)}
          title="Zoom out (−)"
          aria-label="Zoom out"
        >
          −
        </button>
        <button
          type="button"
          className="nexus-zoom-btn"
          onClick={resetZoom}
          title="Reset zoom (0)"
          aria-label="Reset zoom"
        >
          Reset
        </button>
        <span className="nexus-zoom-readout" aria-live="polite">
          {`${Math.round(view.scale * 100)} %`}
        </span>
      </div>
    </div>
  );
}
