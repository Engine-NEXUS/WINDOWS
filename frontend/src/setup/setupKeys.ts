/**
 * First-run "free AI keys" card.
 *
 * NEXUS is BYOK: cloud features run on the user's OWN free Groq / Gemini keys.
 * Both are optional — with neither key, wake word, local speech-to-text, TTS
 * and local commands still work. This merges what the user typed into the
 * settings object that is about to be saved.
 *
 * Rules:
 *  - keys are trimmed; an empty field never overwrites a key already saved;
 *  - entering a Groq key means the user wants cloud speech-to-text, so
 *    `localSttOnly` is switched off (the UI states this next to the field —
 *    it is an explicit, visible choice, not a silent privacy flip).
 */
export interface SetupKeyInput {
  groq: string;
  gemini: string;
}

export function applySetupKeys<T extends Record<string, unknown>>(current: T, input: SetupKeyInput): T {
  const next: Record<string, unknown> = { ...current };
  const groq = input.groq.trim();
  const gemini = input.gemini.trim();
  if (groq) {
    next.groqApiKey = groq;
    next.localSttOnly = false;
  }
  if (gemini) {
    next.geminiApiKey = gemini;
  }
  return next as T;
}
