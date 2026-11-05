import { useAssistant } from "../store/assistant";
import { isBargedIn } from "./wsBridge";

/**
 * Follow-up listening window after NEXUS finishes speaking a reply (normal mode).
 * Keeps the mic open for a short window with the orb in listening form so the
 * user can say "ok", "do it", "also open the calendar" without "Nexus".
 * Directed speech gate in Rust filters echo/TV/side-talk.
 */

let open = false;

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
