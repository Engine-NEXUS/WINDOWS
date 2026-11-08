import { useEffect, useRef, useState } from "react";
import { typedSlice, typingProgress } from "./speechPillModel";

/**
 * Types `target` letter by letter while `active`. A longer target (the next partial, the final text) continues from the
 * letters already shown instead of restarting; going inactive clears it, so the next text starts fresh.
 * `reduced` shows everything at once (reduced motion).
 */
export function useTypedText(target: string, active: boolean, reduced = false): string {
  const [shown, setShown] = useState("");
  const st = useRef({ base: 0, startedAt: 0, typed: 0, target: "" });

  useEffect(() => {
    if (!active || !target) {
      st.current = { base: 0, startedAt: 0, typed: 0, target: "" };
      setShown("");
      return;
    }
    if (reduced) {
      st.current.typed = target.length;
      st.current.target = target;
      setShown(target);
      return;
    }
    const s = st.current;
    // Continue from what is on screen; a target that is shorter than the typed count just shows in full.
    s.base = Math.min(s.typed, target.length);
    s.startedAt = performance.now();
    s.target = target;
    let raf = 0;
    const tick = () => {
      const typed = typingProgress(s.base, target.length, performance.now() - s.startedAt);
      s.typed = typed;
      setShown(typedSlice(target, typed));
      if (typed < target.length) raf = requestAnimationFrame(tick);
    };
    tick();
    return () => cancelAnimationFrame(raf);
  }, [target, active, reduced]);

  return shown;
}
