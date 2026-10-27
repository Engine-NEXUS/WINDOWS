/**
 * Guidance for the Groq key card.
 *
 * "Local STT only" is ON by default (privacy-first), and while it is on the
 * app never calls Groq for speech — so a user who pastes a Groq key sees no
 * change and concludes the key (or NEXUS) is broken. Say so, and offer the fix.
 */
export type GroqKeyHint =
  /** No key yet: point at the free key. */
  | "get-key"
  /** Key present but local-only is on: speech stays local, the key is idle. */
  | "local-only-blocks"
  /** Key present and cloud speech is enabled. */
  | "cloud-active";

export function groqKeyHint(groqApiKey: string | undefined, localSttOnly: boolean | undefined): GroqKeyHint {
  const hasKey = (groqApiKey ?? "").trim().length > 0;
  if (!hasKey) return "get-key";
  return localSttOnly ? "local-only-blocks" : "cloud-active";
}

export const GROQ_KEY_URL = "https://console.groq.com/keys";
export const GEMINI_KEY_URL = "https://aistudio.google.com/apikey";
