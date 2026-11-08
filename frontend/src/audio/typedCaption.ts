// Letter-by-letter caption timing. One steady typing rate across a whole spoken line, instead of a burst of letters
// inside each word's own short time slot (which reads as words popping in one by one).
// Pure maths shared by the playback-driven stage (speechStage.ts) and the timer-driven scheduler (captionScheduler.ts).

export interface TimedWord {
  text: string;
  start_ms: number;
  duration_ms: number;
}

export function decodeEntities(text: string): string {
  return text
    .replace(/&apos;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

/** The line as it is displayed: decoded words joined by single spaces. */
export function lineText(words: readonly TimedWord[]): string {
  return words.map((w) => decodeEntities(w.text)).join(" ");
}

/** When the line starts and ends being spoken (first word start .. last word end), never zero-length. */
export function lineSpan(words: readonly TimedWord[]): { start: number; end: number } {
  if (!words.length) return { start: 0, end: 1 };
  const start = words[0].start_ms;
  const last = words[words.length - 1];
  const end = Math.max(start + 1, last.start_ms + Math.max(1, last.duration_ms));
  return { start, end };
}

/**
 * How many of `total` characters are typed at playback position `posMs`.
 * Steady rate from `startMs` to `endMs`: 1 character as soon as the line starts, all of them when it ends.
 */
export function typedLength(total: number, startMs: number, endMs: number, posMs: number): number {
  if (total <= 0) return 0;
  if (total === 1) return 1;
  const span = Math.max(1, endMs - startMs);
  const p = Math.min(1, Math.max(0, (posMs - startMs) / span));
  return Math.min(total, Math.max(1, 1 + Math.floor((total - 1) * p)));
}

/** The text typed so far at `posMs` (no trailing space, so the caption never ends in a gap). */
export function typedText(words: readonly TimedWord[], posMs: number): string {
  const full = lineText(words);
  if (!full) return "";
  const { start, end } = lineSpan(words);
  return full.slice(0, typedLength(full.length, start, end, posMs)).trimEnd();
}

/** For timer-driven reveals: when (ms on the track timeline) each character appears, and the text typed by then. */
export function charSchedule(words: readonly TimedWord[]): { atMs: number; text: string }[] {
  const full = lineText(words);
  const total = full.length;
  if (!total) return [];
  const { start, end } = lineSpan(words);
  const out: { atMs: number; text: string }[] = [];
  for (let c = 1; c <= total; c++) {
    const atMs = total === 1 ? start : start + ((c - 1) / (total - 1)) * (end - start);
    out.push({ atMs, text: full.slice(0, c).trimEnd() });
  }
  return out;
}
