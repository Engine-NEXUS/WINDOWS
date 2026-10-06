import { useEffect, useRef, useState } from "react";
import { initCaptionListener, onCaptionLineUpdate, CaptionLineEvent } from "../audio/captionScheduler";

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * ResponseCaption — the spoken reply's text, appearing line-by-line / clause-by-clause
 * outside the black capsule (top dock: below the capsule; bottom dock: above the capsule).
 * Synchronized with the Voice Orb: holds the orb alive until the final line clears.
 */
export function ResponseCaption() {
  const [rect, setRect] = useState<OrbRect | null>(null);
  const [lineEvent, setLineEvent] = useState<CaptionLineEvent | null>(null);

  useEffect(() => {
    initCaptionListener();
    const unsub = onCaptionLineUpdate((event, done) => {
      setLineEvent(event);
      if (done && event.phase === "cleared") {
        setLineEvent(null);
      }
    });
    return () => {
      unsub();
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<OrbRect>("stage:orb_rect", (event) => {
        if (event.payload) setRect(event.payload);
      });
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const pending = await invoke<OrbRect | null>("get_pending_orb_rect");
        if (pending) setRect(pending);
      } catch {
        /* non-Tauri (tests) — no-op */
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Position/opacity applied via direct DOM mutation, not React's `style`
  // prop — the stage window's CSP makes inline styles a silent no-op.
  const applyPosition = (el: HTMLDivElement | null) => {
    if (!el || !rect) return;
    const dpr = window.devicePixelRatio || 1;
    const centerX = typeof window !== "undefined" ? window.innerWidth / 2 : rect.x / dpr + rect.w / dpr / 2;
    const topY = rect.y / dpr;
    const bottomY = (rect.y + rect.h) / dpr;
    const gap = 24;
    const isTop = topY < (typeof window !== "undefined" ? window.innerHeight / 2 : 540);

    if (isTop) {
      el.style.transform = `translate(${centerX - 260}px, ${bottomY + gap}px)`;
    } else {
      el.style.transform = `translate(${centerX - 260}px, ${topY - gap}px) translateY(-100%)`;
    }
    el.style.opacity = lineEvent && lineEvent.phase !== "cleared" ? "1" : "0";
  };

  const captionRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    applyPosition(captionRef.current);
  }, [rect, lineEvent]);

  if (!rect || !lineEvent || !lineEvent.text.trim() || lineEvent.phase === "cleared") return null;

  const phaseClass =
    lineEvent.phase === "active"
      ? "caption-line--active"
      : lineEvent.phase === "fading"
      ? "caption-line--fading"
      : "";

  return (
    <div
      className="response-caption"
      ref={(el) => {
        captionRef.current = el;
        applyPosition(el);
      }}
    >
      {lineEvent.previousText && (
        <span className="caption-line caption-line--prev">{lineEvent.previousText}</span>
      )}
      <span className={`caption-line ${phaseClass}`}>{lineEvent.text}</span>
    </div>
  );
}

