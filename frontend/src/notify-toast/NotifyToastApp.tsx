import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TICK_MS, asPayload, isFading, remainingAfter, type ToastPayload } from "./notifyToastModel";
import { NotifyToastView } from "./NotifyToastView";

/** The five-second heads-up. Rust shows this window without activating it, sizes it to the content and places it
 *  top-right; the page runs the timer (paused on hover / keyboard focus), and a click sends only the opaque event id
 *  back: Rust opens the sidebar detail (and from there the real item). */
export function NotifyToastApp() {
  const [p, setP] = useState<ToastPayload | null>(null);
  const [left, setLeft] = useState(0);
  const paused = useRef(false);
  const closing = useRef(false);
  const cardRef = useRef<HTMLDivElement>(null);

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

  // Tell Rust how tall the content really is so the window fits it exactly (its first size is an estimate).
  useLayoutEffect(() => {
    const h = cardRef.current?.offsetHeight;
    if (p && h) invoke("notify_toast_fit", { height: h }).catch(() => {});
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
  return (
    <NotifyToastView
      ref={cardRef}
      payload={p}
      fading={isFading(left)}
      onOpen={open}
      onClose={close}
      onPause={(v) => (paused.current = v)}
    />
  );
}
