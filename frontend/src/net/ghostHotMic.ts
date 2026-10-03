import { useAssistant } from "../store/assistant";

/**
 * Ghost hot-mic loop — keeps the command mic open across turns while a
 * ghost session is live, so no wake word is needed between commands.
 *
 * Design constraints (each load-bearing, each verified in code):
 * - Reuses the follow-up auto-reopen path (`triggerFollowupListen` →
 *   `__NEXUS_WAKE__` → `startListening`), which bypasses wake-word
 *   suppression entirely — the suppressor only gates *detection*, and
 *   direct listen calls never consult it.
 * - Echo guard: polls `isRustTtsPlaying` (up to 3s) before reopening,
 *   so the mic never captures our own tail audio.
 * - Meeting mode suppresses the loop (checked live via `meeting_active`,
 *   not cached — meetings start mid-session).
 * - Pure decision + counter helpers below are unit-tested; the async
 *   orchestrator (`maybeGhostRelisten`) is wired but thin.
 */

/** Consecutive silent hot-mic turns tolerated before parking. */
export const GHOST_SILENT_CAP = 3;

let silentMisses = 0;

/** Should this turn-end reopen the mic? Pure (unit-tested). */
export function shouldGhostRelisten(ghostActive: boolean): boolean {
  return ghostActive;
}

/** Record one silent hot-mic turn; returns the running count. */
export function recordSilentMiss(): number {
  silentMisses += 1;
  return silentMisses;
}

/** Any heard speech resets the silence streak. */
export function resetSilentMisses(): void {
  silentMisses = 0;
}

/** Test hook: read the streak without touching it. */
export function __testSilentMissCount(): number {
  return silentMisses;
}

function isTauri(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof (window as any).__TAURI_INTERNALS__ !== "undefined"
  );
}

/**
 * Echo guard: wait until Rust TTS actually goes quiet before reopening
 * the mic, so playback can't leak into the next capture. Capped at 1000ms
 * (Phase 4 cadence): the 300ms post-TTS DAC-drain mute in startListening
 * already absorbs the hardware tail — a 3s poll here only stacked dead
 * air onto every ghost turn. Skipped entirely by the caller for silent
 * steps (no TTS → nothing to wait for).
 */
async function waitForAudioIdle(timeoutMs = 1000): Promise<void> {
  const { isRustTtsPlaying } = await import("../audio/ttsPlayer");
  const start = Date.now();
  while (isRustTtsPlaying() && Date.now() - start < timeoutMs) {
    await new Promise((r) => setTimeout(r, 100));
  }
}

/**
 * Reopen the mic if — and only if — a ghost session is still live, no
 * meeting is active, and TTS has actually gone quiet. Returns true when
 * a relisten was triggered. Safe to call after every turn end; no-ops
 * everywhere outside ghost mode.
 */
export async function maybeGhostRelisten(): Promise<boolean> {
  if (!shouldGhostRelisten(useAssistant.getState().ghostActive)) return false;
  if (!isTauri()) return false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const meeting = await invoke<boolean>("meeting_active").catch(() => false);
    if (meeting) return false;
  } catch {
    return false;
  }
  try {
    await waitForAudioIdle();
  } catch {
    // Echo guard is best-effort; a failed poll must not wedge the loop.
  }
  if (!useAssistant.getState().ghostActive) return false;
  try {
    const { triggerFollowupListen } = await import("../main");
    triggerFollowupListen();
    return true;
  } catch {
    return false;
  }
}

/**
 * End-of-turn reset that keeps the ghost hot-mic loop alive.
 *
 * Relisten BEFORE reset (the gate reads ghostActive — today reset()
 * preserves it, but this order must not depend on that). Outside ghost
 * mode this is exactly reset(): maybeGhostRelisten no-ops without
 * touching IPC. NEVER call from abort / barge-in / idle-timeout paths —
 * an explicit user cancel must stay cancelled.
 */
export async function endGhostTurn(): Promise<void> {
  await maybeGhostRelisten();
  useAssistant.getState().reset();
}

/**
 * Intents a live ghost session owns. With a session active these must
 * bypass the frontend local-execute path and go to the orchestrator,
 * whose ghost runners narrate, focus-verify, and keep the session open
 * for follow-ups (plain local execute opens silently and starves the
 * drill pipeline). Everything else stays local. Pure (unit-tested).
 */
const GHOST_ROUTED_ACTIONS = ["open_app", "whatsapp_chat"];

export function shouldGhostRoute(intentAction: string, ghostActive: boolean): boolean {
  return ghostActive && GHOST_ROUTED_ACTIONS.includes(intentAction);
}
