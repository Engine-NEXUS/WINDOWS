import React, { useRef, useCallback } from "react";
import { lensBackground } from "./lensMath";
import "./lens.css";

export interface LensProps {
  src: string;
  alt: string;
  zoom?: number; // default 2.5
  lensSize?: number; // default 140
  onOpen?: () => void;
  className?: string;
  children?: React.ReactNode;
}

/**
 * Lens — hover magnifier bubble component for Response Sidebar.
 * Displays a magnified circular lens bubble tracking mouse/pen pointers.
 * Clicking or pressing Enter/Space calls `onOpen` to launch the full lightbox.
 * Touch / coarse pointers suppress the hover bubble; taps trigger `onOpen`.
 */
export function Lens({
  src,
  alt,
  zoom = 2.5,
  lensSize = 140,
  onOpen,
  className,
  children,
}: LensProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const bubbleRef = useRef<HTMLDivElement>(null);

  const updateBubble = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // Coarse / touch pointers: no hover bubble
      if (e.pointerType === "touch") return;
      if (typeof window !== "undefined" && window.matchMedia?.("(pointer: coarse)").matches) {
        return;
      }

      const container = containerRef.current;
      const bubble = bubbleRef.current;
      if (!container || !bubble) return;

      const rect = container.getBoundingClientRect();
      if (rect.width <= 0 || rect.height <= 0) return;

      const pointer = {
        x: e.clientX - rect.left,
        y: e.clientY - rect.top,
      };
      const box = {
        w: rect.width,
        h: rect.height,
      };

      const bg = lensBackground(pointer, box, zoom, lensSize);

      bubble.style.setProperty("--lens-size", `${lensSize}px`);
      bubble.style.setProperty("--lens-left", `${bg.left}px`);
      bubble.style.setProperty("--lens-top", `${bg.top}px`);
      bubble.style.setProperty("--lens-bg-x", `${bg.bgX}px`);
      bubble.style.setProperty("--lens-bg-y", `${bg.bgY}px`);
      bubble.style.setProperty("--lens-bg-w", `${bg.bgW}px`);
      bubble.style.setProperty("--lens-bg-h", `${bg.bgH}px`);
      bubble.style.backgroundImage = `url("${src}")`;
      bubble.style.display = "block";
    },
    [src, zoom, lensSize]
  );

  const hideBubble = useCallback(() => {
    if (bubbleRef.current) {
      bubbleRef.current.style.display = "none";
    }
  }, []);

  const handleClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      e.preventDefault();
      onOpen?.();
    },
    [onOpen]
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        onOpen?.();
      }
    },
    [onOpen]
  );

  return (
    <div
      ref={containerRef}
      className={`lens-container ${className || ""}`.trim()}
      role="button"
      tabIndex={0}
      aria-label={`Zoom ${alt}`}
      onClick={handleClick}
      onKeyDown={handleKeyDown}
      onPointerMove={updateBubble}
      onPointerEnter={updateBubble}
      onPointerLeave={hideBubble}
      onPointerCancel={hideBubble}
    >
      {children ?? <img src={src} alt={alt} />}
      <div
        ref={bubbleRef}
        className="lens-bubble"
        style={{ display: "none" }}
        aria-hidden="true"
      />
    </div>
  );
}
