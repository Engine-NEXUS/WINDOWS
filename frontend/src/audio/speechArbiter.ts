import type { SpeechIntent } from "./ttsPlayer";

export type SpeechDecision = "play" | "queue" | "drop";

/** Pure main-center policy. All subcenters use the same speech gateway. */
export function decideSpeech(active: SpeechIntent | null, incoming: SpeechIntent): SpeechDecision {
  if (!active) return "play";
  const next = incoming.kind ?? "result";
  const current = active.kind ?? "result";
  if (next === "ack") return "drop";
  if (next === "alert" || next === "narration") return "queue";
  if (next === "preview") return "queue";
  if (current === "prompt" && next === "result" && incoming.requestId === active.requestId) return "queue";
  return "play";
}
