import { useEffect, useRef, useState } from "react";
import "./background-task.css";
import { elapsedLabel, parseTasks, visibleTasks, type BackgroundTask } from "./backgroundTaskModel";
import { taskStackSlot, type Rect } from "./layoutSlots";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/**
 * One small chip per task that is still running after its turn ended (a CodeRabbit review, an AI analysis).
 * The turn spinner cannot show these: it belongs to a request and goes with it. Rust owns the list
 * (`background_tasks.rs`); this only draws it. Never interactive, so it needs no hitbox.
 */
export function BackgroundTaskChip() {
  const [tasks, setTasks] = useState<BackgroundTask[]>([]);
  const [now, setNow] = useState(Date.now());
  // The spinner's rect (CSS px): the chips hang under it, wherever the owner placed it and at whatever size.
  const [spinner, setSpinner] = useState<Rect | null>(null);
  const stackRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let off: (() => void) | undefined;
    let offRect: (() => void) | undefined;
    let live = true;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      off = await listen<unknown>("task:update", (e) => live && setTasks(parseTasks(e.payload)));
      const toCss = (r: Rect): Rect => {
        const d = window.devicePixelRatio || 1;
        return { x: r.x / d, y: r.y / d, w: r.w / d, h: r.h / d };
      };
      offRect = await listen<Rect>("stage:loading_rect", (e) => live && e.payload && setSpinner(toCss(e.payload)));
      invoke<Rect | null>("get_pending_loading_rect")
        .then((r) => live && r && setSpinner(toCss(r)))
        .catch(() => {});
      // A task that started before this stage mounted.
      invoke("background_tasks_list")
        .then((l) => live && setTasks(parseTasks(l)))
        .catch(() => {});
    })();
    return () => {
      live = false;
      off?.();
      offRect?.();
    };
  }, []);

  // Placement is applied through the CSSOM (the stage CSP drops React inline styles); the CSS default is the fallback.
  useEffect(() => {
    const el = stackRef.current;
    if (!el) return;
    const slot = taskStackSlot(spinner, { w: window.innerWidth, h: window.innerHeight });
    el.style.top = `${slot.top}px`;
    el.style.right = `${slot.right}px`;
  }, [spinner, tasks.length]);

  useEffect(() => {
    if (tasks.length === 0) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [tasks.length]);

  if (tasks.length === 0) return null;
  const { shown, extra } = visibleTasks(tasks);
  return (
    <div className="nx-task-stack" role="status" aria-live="polite" ref={stackRef}>
      {shown.map((t) => (
        <div key={t.id} className="nx-task-chip">
          <span className="nx-task-spinner" aria-hidden="true" />
          <span className="nx-task-label">{t.label}</span>
          <span className="nx-task-time">{elapsedLabel(t.startedMs, now)}</span>
        </div>
      ))}
      {extra > 0 && <div className="nx-task-chip nx-task-more">+{extra} more</div>}
    </div>
  );
}
