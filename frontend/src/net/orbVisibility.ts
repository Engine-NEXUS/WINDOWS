// What the orb does when a spoken line (an acknowledgement or an answer) is over.
//
// One pure function so the answer is written down once. It was spread over the hide chain in
// orchestrator.ts, the idle watchdog and three comments that disagreed with the code (2026-10-29
// loading-state research): the comments said "the orb hides when the loading indicator takes over",
// the code never did, and on a plain acknowledgement the orb sat in a silent "speaking" pose beside
// the spinner for the whole wait.
//
// POLICY (decided with the owner's report that the orb "disappears at On it sir"): while work for the
// turn is still pending the orb STAYS, in its thinking pose, next to the spinner. It is hidden only
// after the turn is over. Flip HAND_OVER_TO_SPINNER to bring back the old "spinner carries the wait" look.

export const HAND_OVER_TO_SPINNER = false;

export interface AfterSpeechInputs {
  ghostActive: boolean;
  state: "idle" | "listening" | "thinking" | "speaking";
  loadingVisible: boolean;
  /** Rust is playing audio right now (the audio player's own truth, not the store flag). */
  audioPlaying: boolean;
  captionActive: boolean;
  awaitingInput: boolean;
  pendingGithubCommand: boolean;
}

export type AfterSpeechDecision =
  /** Leave everything alone and stop re-checking. */
  | "keep"
  /** Audio or a caption is still going: check again shortly. */
  | "wait"
  /** The line ended and the turn's work continues: back to the thinking pose, orb stays. */
  | "settle_thinking"
  /** The line ended and the orb is still "speaking" with nothing playing: count toward the stuck limit. */
  | "stuck_tick"
  /** Nothing left to show: hide the orb. */
  | "hide";

export function decideAfterSpeech(i: AfterSpeechInputs): AfterSpeechDecision {
  if (i.ghostActive) return "keep";
  if (i.state === "speaking") {
    if (i.audioPlaying) return "wait";
    if (i.loadingVisible) return HAND_OVER_TO_SPINNER ? "hide" : "settle_thinking";
    return "stuck_tick";
  }
  if (i.state !== "idle") return "keep";
  if (i.awaitingInput || i.pendingGithubCommand) return "keep";
  if (i.captionActive) return "wait";
  return "hide";
}
