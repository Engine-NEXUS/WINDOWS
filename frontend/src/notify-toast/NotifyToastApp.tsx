import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TICK_MS, asPayload, moreLabel, progress, remainingAfter, statusLabel, type ToastPayload } from "./notifyToastModel";
import "./notify-toast.css";

/** The five-second heads-up. Rust shows this window without activating it, sizes it and places it top-right; the page
 *  runs the timer (paused on hover / keyboard focus), and a click sends only the opaque event id back: Rust builds the
 *  link from the typed target. */
export function NotifyToastApp() {
  const [p, setP] = useState<ToastPayload | null>(null);
  const [left, setLeft] = useState(0);
  const paused = useRef(false);
  const closing = useRef(false);

  const show = useCallback((raw: unknown) => {
    const next = asPayload(raw);
    if (!next) return;
    closing.current = false;
    setP(next);
    setLeft(next.durationMs);
  }, []);

  useEffect(() => {
    invoke<unknown>("get_pending_toast").then(show).catch(() => {});
    const un = listen<unknown>("toast:show", (e) => show(e.payload));
    return () => {
      un.then((f) => f());
    };
  }, [show]);

  const close = useCallback(() => {
    if (closing.current) return;
    closing.current = true;
    invoke("notify_toast_close").catch(() => {});
  }, []);

  const open = useCallback(() => {
    if (!p || closing.current) return;
    closing.current = true;
    invoke("notify_toast_click", { eventId: p.eventId }).catch(() => {});
  }, [p]);

  useEffect(() => {
    if (!p) return;
    const id = window.setInterval(() => setLeft((l) => remainingAfter(l, TICK_MS, paused.current)), TICK_MS);
    return () => window.clearInterval(id);
  }, [p]);

  useEffect(() => {
    if (p && left <= 0) close();
  }, [p, left, close]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [close]);

  if (!p) return null;
  const more = moreLabel(p.count);
  return (
    <div
      className={`nt-card nt-card--${p.status}`}
      role="status"
      aria-live="polite"
      aria-label={`${p.provider}: ${p.title}`}
      onMouseEnter={() => (paused.current = true)}
      onMouseLeave={() => (paused.current = false)}
      onFocus={() => (paused.current = true)}
      onBlur={() => (paused.current = false)}
    >
      <div className="nt-head">
        <span className="nt-source">{p.provider}</span>
        <span className={`nt-status nt-status--${p.status}`}>{statusLabel(p.status)}</span>
        {more && <span className="nt-more">{more}</span>}
        <button className="nt-close" aria-label="Dismiss" onClick={close}>
          ×
        </button>
      </div>
      <div className="nt-body" onClick={open}>
        <div className="nt-title">{p.title}</div>
        {p.preview && <div className="nt-preview">{p.preview}</div>}
      </div>
      <div className="nt-foot">
        <button className="nt-action" onClick={open}>
          {p.action}
        </button>
      </div>
      <div className="nt-bar" aria-hidden="true">
        <div className="nt-bar-fill" style={{ width: `${progress(left, p.durationMs) * 100}%` }} />
      </div>
    </div>
  );
}
