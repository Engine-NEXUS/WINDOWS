// Feeds the store's `orbSignals` from the events Rust already emits (see the table in
// docs/changes for the orb): screen tour, hand mode, background tasks, agent watches, plus a light poll for
// dictation (Rust keeps that flag in a static and emits nothing). Started once by the orb; safe to call twice.

import { parseTasks } from "../stage/backgroundTaskModel";
import { useAssistant } from "../store/assistant";

/** A hand window that stops reporting counts as stopped after this long (covers a crash with no `hand:stopped`). */
export const HAND_STALE_MS = 5000;
/** How often the dictation flag is read. */
export const DICTATION_POLL_MS = 2000;

/** Tour phases that mean "capturing and running vision". Anything else ends the analysing state. */
export function tourFetchingFor(event: string, payload: unknown): boolean | null {
  if (event === "screen:tour_phase") {
    const phase = (payload as { phase?: string } | null)?.phase;
    if (phase === "fetching") return true;
    if (phase === "fallback") return false;
    return null;
  }
  if (event === "screen:tour_start" || event === "screen:tour_end") return false;
  return null;
}

/** Track which agent watches are running. `progress` adds, `done|error|stopped` remove, `login_required` keeps it. */
export function trackAgentWatches(ids: ReadonlySet<string>, event: string, payload: unknown): Set<string> {
  const next = new Set(ids);
  const p = (payload ?? {}) as { id?: unknown; watch_id?: unknown; watchId?: unknown };
  const raw = p.id ?? p.watch_id ?? p.watchId;
  const id = typeof raw === "string" || typeof raw === "number" ? String(raw) : "unknown";
  switch (event) {
    case "agent_watch:progress":
    case "agent_watch:login_required":
      next.add(id);
      break;
    case "agent_watch:done":
    case "agent_watch:error":
    case "agent_watch:stopped":
      next.delete(id);
      // A payload without an id (older sidecar) ends the anonymous watch too.
      if (id === "unknown") next.clear();
      break;
    default:
      break;
  }
  return next;
}

function isTauri(): boolean {
  return typeof window !== "undefined" && typeof (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ !== "undefined";
}

let started = false;

export function initOrbSignals(): void {
  if (started || !isTauri()) return;
  started = true;
  const set = useAssistant.getState().setOrbSignals;

  void (async () => {
    const { listen } = await import("@tauri-apps/api/event");
    const { invoke } = await import("@tauri-apps/api/core");

    for (const ev of ["screen:tour_phase", "screen:tour_start", "screen:tour_end"]) {
      await listen<unknown>(ev, (e) => {
        const v = tourFetchingFor(ev, e.payload);
        if (v !== null) set({ tourFetching: v });
      });
    }

    let handTimer: ReturnType<typeof setTimeout> | null = null;
    await listen<unknown>("hand:state", () => {
      set({ handActive: true });
      if (handTimer) clearTimeout(handTimer);
      handTimer = setTimeout(() => set({ handActive: false }), HAND_STALE_MS);
    });
    await listen<unknown>("hand:stopped", () => {
      if (handTimer) clearTimeout(handTimer);
      handTimer = null;
      set({ handActive: false });
    });

    await listen<unknown>("task:update", (e) => set({ backgroundTasks: parseTasks(e.payload).length }));
    void invoke("background_tasks_list")
      .then((l) => set({ backgroundTasks: parseTasks(l).length }))
      .catch(() => {});

    let agents: Set<string> = new Set();
    for (const ev of ["agent_watch:progress", "agent_watch:login_required", "agent_watch:done", "agent_watch:error", "agent_watch:stopped"]) {
      await listen<unknown>(ev, (e) => {
        agents = trackAgentWatches(agents, ev, e.payload);
        set({ agentWatching: agents.size > 0 });
      });
    }

    const poll = () => {
      invoke<boolean>("dictation_active")
        .then((v) => set({ dictationActive: Boolean(v) }))
        .catch(() => {});
    };
    poll();
    setInterval(poll, DICTATION_POLL_MS);
  })();
}
