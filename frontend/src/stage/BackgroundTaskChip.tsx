import { useEffect, useState } from "react";
import "./background-task.css";
import { elapsedLabel, parseTasks, visibleTasks, type BackgroundTask } from "./backgroundTaskModel";

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

  useEffect(() => {
    if (!isTauri()) return;
    let off: (() => void) | undefined;
    let live = true;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      off = await listen<unknown>("task:update", (e) => live && setTasks(parseTasks(e.payload)));
      // A task that started before this stage mounted.
      invoke("background_tasks_list")
        .then((l) => live && setTasks(parseTasks(l)))
        .catch(() => {});
    })();
    return () => {
      live = false;
      off?.();
    };
  }, []);

  useEffect(() => {
    if (tasks.length === 0) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [tasks.length]);

  if (tasks.length === 0) return null;
  const { shown, extra } = visibleTasks(tasks);
  return (
    <div className="nx-task-stack" role="status" aria-live="polite">
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
