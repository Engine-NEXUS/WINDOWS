// Arrow-key docking for the sidebar window: Left/Right step along left - centre - right (no buttons on screen).
import { useEffect, useRef } from "react";

export const DOCKS = ["left", "center", "right"] as const;
export type Dock = (typeof DOCKS)[number];

/** Left/Right arrow: one step along left - centre - right, never past either end. */
export function nextDock(current: Dock, key: string): Dock {
  const i = DOCKS.indexOf(current);
  if (key === "ArrowLeft") return DOCKS[Math.max(0, i - 1)];
  if (key === "ArrowRight") return DOCKS[Math.min(DOCKS.length - 1, i + 1)];
  return current;
}

/** True when the key press belongs to a text field and must not move the window. */
export function isTyping(target: EventTarget | null): boolean {
  const t = target as HTMLElement | null;
  return !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.tagName === "SELECT" || t.isContentEditable);
}

/** Window-level arrow handling. `initial` is where the window currently sits; it is re-read whenever it changes. */
export function useArrowDock(initial: Dock, onDock: (d: Dock) => void, enabled = true) {
  const dock = useRef<Dock>(initial);
  useEffect(() => {
    dock.current = initial;
  }, [initial]);
  useEffect(() => {
    if (!enabled) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
      if (e.ctrlKey || e.metaKey || e.altKey || e.shiftKey || isTyping(e.target)) return;
      const next = nextDock(dock.current, e.key);
      if (next === dock.current) return;
      e.preventDefault();
      dock.current = next;
      onDock(next);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDock, enabled]);
}

/** Shift + Left/Right inside a pinned panel: how many logical px to change its width (0 = not a resize key). */
export const PANEL_RESIZE_STEP = 60;
export function panelResizeDelta(e: KeyboardEvent): number {
  if (!e.shiftKey || e.ctrlKey || e.metaKey || e.altKey || isTyping(e.target)) return 0;
  if (e.key === "ArrowRight") return PANEL_RESIZE_STEP;
  if (e.key === "ArrowLeft") return -PANEL_RESIZE_STEP;
  return 0;
}

/** Window-level Shift + arrow resizing for a pinned panel (Rust clamps and re-lays out all panels). */
export function usePanelResize(label: string | null) {
  useEffect(() => {
    if (!label) return;
    const onKey = (e: KeyboardEvent) => {
      const delta = panelResizeDelta(e);
      if (!delta) return;
      e.preventDefault();
      void import("@tauri-apps/api/core").then(({ invoke }) => invoke("panel_resize", { label, delta })).catch(() => {});
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [label]);
}
