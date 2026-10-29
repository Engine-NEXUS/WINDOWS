import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { HOLD_TIMEOUT_MS, LEAVE_MS, durationFor, expandStyle, type ExpandPayload, type ExpandPhase } from "./notifyExpandModel";
import "./notify-expand.css";

const reducedMotion = () => window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

/** Full-screen overlay animation for a clicked Windows notification: a card starts where the toast was (bottom-right),
 *  grows into the sidebar's rectangle, stays as a cover until the sidebar reports it has rendered (or a bounded
 *  timeout), then fades out. Purely visual: it never takes input or focus. */
export function NotifyExpand() {
  const [p, setP] = useState<ExpandPayload | null>(null);
  const [phase, setPhase] = useState<ExpandPhase>("start");
  const seq = useRef(0);

  useEffect(() => {
    const offs: Array<() => void> = [];
    listen<ExpandPayload>("notify:expand", (e) => {
      seq.current += 1; // a newer click replaces whatever was animating
      setPhase("start");
      setP(e.payload);
    }).then((u) => offs.push(u));
    listen("notify:ready", () => setPhase((cur) => (cur === "end" || cur === "start" ? "leaving" : cur))).then((u) => offs.push(u));
    return () => offs.forEach((u) => u());
  }, []);

  // start -> end on the next frames (so the browser paints the start rectangle first), bounded hold, then unmount.
  useEffect(() => {
    if (!p) return;
    const timers: number[] = [];
    let raf2 = 0;
    const raf1 = requestAnimationFrame(() => {
      raf2 = requestAnimationFrame(() => setPhase((cur) => (cur === "start" ? "end" : cur)));
    });
    timers.push(window.setTimeout(() => setPhase((cur) => (cur === "leaving" ? cur : "leaving")), HOLD_TIMEOUT_MS));
    return () => {
      cancelAnimationFrame(raf1);
      cancelAnimationFrame(raf2);
      timers.forEach((t) => window.clearTimeout(t));
    };
  }, [p]);

  useEffect(() => {
    if (phase !== "leaving" || !p) return;
    const t = window.setTimeout(() => setP(null), reducedMotion() ? 0 : LEAVE_MS);
    return () => window.clearTimeout(t);
  }, [phase, p]);

  if (!p) return null;
  const vp = { w: window.innerWidth, h: window.innerHeight };
  const ms = durationFor(p, reducedMotion());
  const style = {
    ...expandStyle(phase, p, vp),
    transition: phase === "leaving" ? `opacity ${reducedMotion() ? 0 : LEAVE_MS}ms ease` : `all ${ms}ms cubic-bezier(0.22, 1, 0.36, 1)`,
  };
  return (
    <div className={`nx-expand nx-expand--${p.status}`} style={style} aria-hidden="true">
      <span className="nx-expand-title">{p.title}</span>
    </div>
  );
}
