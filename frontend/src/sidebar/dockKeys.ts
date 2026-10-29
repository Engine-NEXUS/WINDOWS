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
export function useArrowDock(initial: Dock, onDock: (d: Dock) => void) {
  const dock = useRef<Dock>(initial);
  useEffect(() => {
    dock.current = initial;
  }, [initial]);
  useEffect(() => {
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
  }, [onDock]);
}
