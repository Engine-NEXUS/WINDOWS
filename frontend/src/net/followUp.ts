import { useAssistant } from "../store/assistant";
import { isBargedIn } from "./wsBridge";

/**
 * Follow-up listening window after NEXUS finishes speaking a reply (normal mode).
 * Keeps the mic open for a short window with the orb in listening form so the
 * user can say "ok", "do it", "also open the calendar" without "Nexus".
 * Directed speech gate in Rust filters echo/TV/side-talk.
 */

let open = false;
let openedAtMs = 0;
let windowMs = 0;

/** A noise-only capture inside the window must not end it; below this much time left it is not worth re-arming. */
export const REARM_MIN_LEFT_MS = 1500;

/** Pure: how much of the follow-up window is left, never negative. */
export function followUpTimeLeftMs(openedAt: number, windowLengthMs: number, now: number): number {
  return Math.max(0, openedAt + windowLengthMs - now);
}

/** Pure: should an empty capture inside the window re-arm the mic (true) or close the window (false)? */
export function shouldRearmFollowUp(timeLeftMs: number): boolean {
  return timeLeftMs >= REARM_MIN_LEFT_MS;
}

/** Time left in the open window (0 when closed). */
export function followUpTimeLeft(now: number = Date.now()): number {
  return open ? followUpTimeLeftMs(openedAtMs, windowMs, now) : 0;
}

/** Check if the follow-up window is currently active. */
export function isFollowUpOpen(): boolean {
  return open;
}

/** Mark the follow-up window closed. */
export function markFollowUpClosed(): void {
  open = false;
}

/** Close the follow-up window and log the outcome. */
export function closeFollowUp(reason: "silence" | "rejected" | "accepted"): void {
  if (open) {
    console.log(`[TURN] followup closed reason=${reason}`);
    markFollowUpClosed();
  }
}

/** Test hook: override the open state. */
export function __testSetFollowUpOpen(val: boolean): void {
  open = val;
}

export interface ShouldOpenFollowUpOptions {
  secs: number;
  ghostActive: boolean;
  meetingActive: boolean;
  offerWindowOpened: boolean;
  bargedIn: boolean;
  replyWasError: boolean;
}

/** Pure decision helper for whether to open the follow-up window. */
export function shouldOpenFollowUp(opts: ShouldOpenFollowUpOptions): boolean {
  if (opts.secs <= 0) return false;
  if (opts.ghostActive) return false;
  if (opts.meetingActive) return false;
  if (opts.offerWindowOpened) return false;
  if (opts.bargedIn) return false;
  if (opts.replyWasError) return false;
  return true;
}

export interface OpenFollowUpWindowOptions {
  offerWindowOpened?: boolean;
  replyWasError?: boolean;
  bargedIn?: boolean;
}

/**
 * Open the follow-up listening window after NEXUS finishes speaking.
 * Returns true if the window was opened; false otherwise.
 */
export async function openFollowUpWindow(
  opts: OpenFollowUpWindowOptions = {},
): Promise<boolean> {
  const ghostActive = useAssistant.getState().ghostActive;
  const bargedIn = opts.bargedIn ?? isBargedIn();
  const replyWasError = opts.replyWasError ?? false;
  const offerWindowOpened = opts.offerWindowOpened ?? false;

  let secs = 6;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const val = await invoke<number>("follow_up_window_secs");
    if (typeof val === "number" && !isNaN(val)) {
      secs = val;
    }
  } catch {
    secs = 6;
  }

  let meetingActive = false;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    meetingActive = await invoke<boolean>("meeting_active").catch(() => false);
  } catch {
    meetingActive = false;
  }

  if (
    !shouldOpenFollowUp({
      secs,
      ghostActive,
      meetingActive,
      offerWindowOpened,
      bargedIn,
      replyWasError,
    })
  ) {
    return false;
  }

  open = true;
  openedAtMs = Date.now();
  windowMs = secs * 1000;
  console.log(`[TURN] followup open secs=${secs}`);

  try {
    const { triggerFollowupListen } = await import("../stage/orbRuntime");
    triggerFollowupListen(true, secs * 1000);
    return true;
  } catch (err) {
    console.warn("[NEXUS] followup listen failed:", err);
    markFollowUpClosed();
    return false;
  }
}
