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

/** How often the hand-mode state is asked for as a backup to the events. */
export const HAND_POLL_MS = 3000;

/** The hand-mode orb signals from the `hand_status` command, or null for an answer that is not a status. */
export function handSignalsFromStatus(r: unknown): { handActive: boolean; handArmed: boolean; handPaused: boolean } | null {
  if (!r || typeof r !== "object") return null;
  const o = r as { active?: unknown; armed?: unknown; palm_paused?: unknown };
  if (typeof o.active !== "boolean") return null;
  const active = o.active;
  return { handActive: active, handArmed: active && o.armed === true, handPaused: active && o.armed !== true && o.palm_paused === true };
}

/** After hand mode ends the orb first goes back to the circle and stays that long, then leaves the screen. */
export const HAND_EXIT_MS = 1700;

export interface HandSignals {
  handActive: boolean;
  handArmed: boolean;
  handPaused: boolean;
}

const HAND_OFF: HandSignals = { handActive: false, handArmed: false, handPaused: false };
const HAND_CIRCLE: HandSignals = { handActive: true, handArmed: false, handPaused: false };

/**
 * Hand-mode orb signals for a new status (from the `hand:orb` event, the status poll or `hand:stopped`).
 * Ending hand mode (it was on, it is now off) first shows the circle for HAND_EXIT_MS, then nothing: the orb morphs back to
 * the circle and only then leaves. `endingUntil` is the time that circle stops (0 = not ending).
 */
export function settleHand(
  prevActive: boolean,
  status: HandSignals,
  now: number,
  endingUntil: number,
): { signals: HandSignals; endingUntil: number } {
  if (status.handActive) return { signals: status, endingUntil: 0 };
  if (endingUntil > 0 && now < endingUntil) return { signals: HAND_CIRCLE, endingUntil };
  if (endingUntil === 0 && prevActive) return { signals: HAND_CIRCLE, endingUntil: now + HAND_EXIT_MS };
  return { signals: HAND_OFF, endingUntil: 0 };
}

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
    let exitTimer: ReturnType<typeof setTimeout> | null = null;
    let endingUntil = 0;
    let handOn = false;
    let lastHandEvent = 0;
    const applyHand = (status: HandSignals) => {
      const r = settleHand(handOn, status, Date.now(), endingUntil);
      endingUntil = r.endingUntil;
      handOn = r.signals.handActive;
      set(r.signals);
      if (exitTimer) clearTimeout(exitTimer);
      exitTimer = null;
      if (endingUntil > 0) {
        // leave the screen when the circle has been shown
        exitTimer = setTimeout(() => applyHand(HAND_OFF), Math.max(50, endingUntil - Date.now() + 20));
      }
    };
    // `hand:orb` is the orb's own event (the overlay listens to `hand:state`; two listeners for one event name in one page
    // were not reliable). It arrives when hand mode's state changes and at least every 1.5 s while it runs.
    await listen<unknown>("hand:orb", (e) => {
      const next = handSignalsFromStatus(e.payload);
      if (!next) return;
      lastHandEvent = Date.now();
      applyHand(next);
      if (handTimer) clearTimeout(handTimer);
      handTimer = setTimeout(() => applyHand(HAND_OFF), HAND_STALE_MS);
    });
    console.log("[ORB] hand signals listening");
    // The events can be missed (a page that started before the app was ready lost its first listeners in testing), so
    // the same facts are also asked for every few seconds: hand mode on, pointer armed, pointer switched off by a palm.
    const askHand = () =>
      Date.now() - lastHandEvent < HAND_STALE_MS
        ? Promise.resolve()
        : invoke("hand_status")
            .then((r) => {
              const next = handSignalsFromStatus(r);
              if (next) applyHand(next);
            })
            .catch(() => {});
    void askHand();
    setInterval(askHand, HAND_POLL_MS);
    await listen<unknown>("hand:stopped", () => {
      if (handTimer) clearTimeout(handTimer);
      handTimer = null;
      lastHandEvent = 0;
      applyHand(HAND_OFF);
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
