/**
 * Does a spoken reply ask the user something? Only then does the orb go back to the circle and listen for the answer.
 * A statement or an explanation ("The moon is 384,400 km away.") is spoken and the orb leaves.
 *
 * Decided on the LAST sentence of the reply: a question at the end is what waits for an answer; a question earlier in
 * the text that the reply goes on to answer does not.
 */
const ASKING_PHRASES = [
  /\bshall i\b/i,
  /\bshould i\b/i,
  /\bwould you like\b/i,
  /\bdo you want\b/i,
  /\bwant me to\b/i,
  /\bwhich (one|of)\b/i,
  /\byes or no\b/i,
  /\bsay (yes|ok|okay|the word|go ahead)\b/i,
  /\bconfirm\b.*\b(yes|say|reply)\b/i,
];

/** The last sentence of `text`, trimmed of closing quotes and spaces. */
export function lastSentence(text: string): string {
  const t = text.trim().replace(/["'”’)\]]+$/u, "").trim();
  const parts = t.split(/(?<=[.!?…]["'”’)\]]*)\s+/u);
  return (parts[parts.length - 1] ?? "").trim();
}

export function replyNeedsInput(text?: string | null): boolean {
  if (!text) return false;
  const last = lastSentence(text);
  if (!last) return false;
  if (/\?$/u.test(last)) return true;
  return ASKING_PHRASES.some((re) => re.test(last));
}
