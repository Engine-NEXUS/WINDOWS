import type { SpeechIntent } from "./ttsPlayer";

export type SpeechDecision = "play" | "queue" | "drop";

/** Background-initiated speech (boot hello, briefing, sentinel and notification alerts, reminder questions)
 *  reaches the frontend as an ordinary `Result` event. Its request id is the only thing that tells it apart
 *  from an answer to the user, and an answer is allowed to cut what is playing while an alert must wait. */
export function speechKindForResult(requestId: string | undefined): "result" | "alert" {
  return requestId !== undefined && (requestId.startsWith("sentinel_") || requestId.startsWith("boot_")) ? "alert" : "result";
}

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
