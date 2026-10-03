import { useEffect, useRef, useState } from "react";
import { initCaptionListener, onCaptionUpdate } from "../audio/captionScheduler";

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * ResponseCaption — the spoken reply's text, growing word-by-word above
 * the orb (plan Phase 3). Anchored a fixed gap above `stage:orb_rect` —
 * the SAME rect OrbFrame listens to (single source of truth for "above
 * the orb"), listened to independently here rather than threaded through
 * props, matching this file's existing pattern of self-contained stage
 * components (SpatialAnnotationLayer/AnnotationCanvas each own their
 * listeners too).
 */
export function ResponseCaption() {
  const [rect, setRect] = useState<OrbRect | null>(null);
  const [words, setWords] = useState<string[]>([]);
  const [visible, setVisible] = useState(false);
  const hideTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    initCaptionListener();
    const unsub = onCaptionUpdate((revealed, done) => {
      if (hideTimer.current) {
        clearTimeout(hideTimer.current);
        hideTimer.current = null;
      }
      if (revealed.length === 0) {
        setVisible(false);
        return;
      }
      setWords(revealed);
      setVisible(true);
      if (done) {
        // Linger briefly after the last word finishes so it's readable,
        // then fade — barge-in (clearCaptionSchedule) hides it immediately
        // via the revealed.length===0 branch above instead of this timer.
        hideTimer.current = setTimeout(() => setVisible(false), 1400);
      }
    });
    return () => {
      unsub();
      if (hideTimer.current) clearTimeout(hideTimer.current);
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

  if (!rect || words.length === 0) return null;

  const dpr = window.devicePixelRatio || 1;
  const centerX = rect.x / dpr + rect.w / dpr / 2;
  const topY = rect.y / dpr;
  const gap = 28;

  return (
    <div
      className="response-caption"
      style={{
        position: "fixed",
        left: 0,
        top: 0,
        width: 520,
        transform: `translate(${centerX - 260}px, ${topY - gap}px) translateY(-100%)`,
        opacity: visible ? 1 : 0,
      }}
    >
      {words.map((w, i) => (
        <span className="response-caption-word" key={i}>
          {w}
          {i < words.length - 1 ? " " : ""}
        </span>
      ))}
    </div>
  );
}
