/**
 * Where the orb goes when a barge-in capture turns out not to be for NEXUS (echo or noise, `barge:resume`).
 *
 * The duck put the orb in "listening", and when the capture stopped to be transcribed the recorder moved it to
 * "thinking" - so by the time `barge:resume` arrived the state was "thinking", the old handler only looked at
 * "listening", and the orb stayed in "thinking" after the reply ended. Both mean "the barge was ours alone": go back to
 * speaking if NEXUS is still talking, else idle. Any other state (a real turn started meanwhile) is left alone.
 */
export function stateAfterBargeResume(current: string, ttsPlaying: boolean): "speaking" | "idle" | null {
  if (current !== "listening" && current !== "thinking") return null;
  return ttsPlaying ? "speaking" : "idle";
}
