import { useAssistant } from "../store/assistant";

// Response caption scheduler (plan Phase 3) — word-by-word reveal driven
// by Rust's `tts:caption` event, anchored to performance.now() the moment
// the first chunk of an utterance arrives. A streamed multi-chunk reply
// emits several `tts:caption` events (one per chunk); each chunk's word
// `start_ms` values already carry every prior chunk's cumulative audio
// duration baked in (see tts.rs's `play_audio_streamed`), so this module
// just keeps scheduling against the SAME anchor across chunks — it never
// needs to know chunking happened.

export interface CaptionWord {
  text: string;
  start_ms: number;
  duration_ms: number;
}

export interface CaptionTrack {
  words: CaptionWord[];
  total_ms: number;
  estimated: boolean;
  /** TTS amplitude envelope (sub-phase C2) — real voice loudness per
   *  `frame_ms`-spaced frame, 0..1 normalized to the loudest frame in the
   *  track. Empty when Rust couldn't compute one (degrade silently —
   *  the orb's speaking "beat" falls back to the mic-proxy level). */
  envelope: number[];
  frame_ms: number;
  /** Same cumulative-offset convention as each word's `start_ms` — rides
   *  the SAME absolute utterance timeline across streamed chunks. */
  envelope_start_ms: number;
}

export interface CaptionLineEvent {
  text: string;
  previousText?: string;
  phase: "active" | "fading" | "cleared";
}

export type LineListener = (line: CaptionLineEvent, done: boolean) => void;

interface EnvelopeSegment {
  startMs: number;
  frameMs: number;
  values: number[];
}

type Listener = (revealed: string[], done: boolean) => void;

let anchorMs: number | null = null;
let timers: ReturnType<typeof setTimeout>[] = [];
let doneTimer: ReturnType<typeof setTimeout> | null = null;
let revealed: string[] = [];
let segments: EnvelopeSegment[] = [];
let suppressNext = false;
let latestScheduledEndMs = 0;
let currentLineSeq = 0;
let activeLineSeq = 0;
const listeners = new Set<Listener>();
const lineListeners = new Set<LineListener>();

export function suppressNextCaption(): void {
  suppressNext = true;
}

function notify(done: boolean): void {
  const snapshot = revealed.slice();
  listeners.forEach((l) => l(snapshot, done));
}

function notifyLine(line: CaptionLineEvent, done: boolean): void {
  lineListeners.forEach((l) => l(line, done));
}

/** Subscribe to word reveal updates. Returns an unsubscribe function. */
export function onCaptionUpdate(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Subscribe to line-by-line replacement updates. Returns an unsubscribe function. */
export function onCaptionLineUpdate(fn: LineListener): () => void {
  lineListeners.add(fn);
  return () => lineListeners.delete(fn);
}

/** Barge-in: cancel every pending reveal and clear the caption immediately. */
export function clearCaptionSchedule(): void {
  timers.forEach(clearTimeout);
  timers = [];
  if (doneTimer) {
    clearTimeout(doneTimer);
    doneTimer = null;
  }
  anchorMs = null;
  revealed = [];
  segments = [];
  suppressNext = false;
  latestScheduledEndMs = 0;
  currentLineSeq = 0;
  activeLineSeq = 0;
  useAssistant.getState().setCaptionActive(false);
  notify(true);
  notifyLine({ text: "", phase: "cleared" }, true);
}

/**
 * Current real TTS voice amplitude, 0..1, interpolated between the two
 * nearest envelope frames — or `null` when no envelope has arrived yet
 * (no utterance speaking, or this chunk's engine couldn't produce one).
 * Callers (VoiceOrb.tsx) fall back to the mic-proxy level on `null`.
 */
export function getEnvelopeLevel(): number | null {
  if (anchorMs === null || !segments.length) return null;
  const elapsed = performance.now() - anchorMs;
  for (const seg of segments) {
    if (!seg.values.length) continue;
    const segEnd = seg.startMs + seg.values.length * seg.frameMs;
    if (elapsed >= seg.startMs && elapsed < segEnd) {
      const idxF = (elapsed - seg.startMs) / seg.frameMs;
      const i0 = Math.max(0, Math.floor(idxF));
      const i1 = Math.min(seg.values.length - 1, i0 + 1);
      const frac = idxF - i0;
      return seg.values[i0] + (seg.values[i1] - seg.values[i0]) * frac;
    }
  }
  // Elapsed is past every known segment (more audio still synthesizing,
  // or this was the final chunk and we're in its tail) — hold the last
  // known value instead of snapping to 0 (a flat silent gap would look
  // like the beat died rather than just outrunning known data).
  const last = segments[segments.length - 1];
  return last.values.length ? last.values[last.values.length - 1] : null;
}

function unescapeXml(str: string): string {
  return str
    .replace(/&apos;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">");
}

export function partitionIntoLines(words: CaptionWord[]): { text: string; start_ms: number; end_ms: number }[] {
  if (!words.length) return [];
  const lines: { text: string; start_ms: number; end_ms: number }[] = [];
  let current: CaptionWord[] = [];

  for (let i = 0; i < words.length; i++) {
    const w = words[i];
    current.push(w);

    const isLast = i === words.length - 1;
    const clean = unescapeXml(w.text).trim();
    const isSentenceBreak = /[.?!]["']?$/.test(clean);
    const nextGap = !isLast && (words[i + 1].start_ms - (w.start_ms + w.duration_ms) > 650);
    const isWordCap = current.length >= 10;

    if (isLast || isSentenceBreak || nextGap || isWordCap) {
      lines.push({
        text: current.map((c) => unescapeXml(c.text)).join(" "),
        start_ms: current[0].start_ms,
        end_ms: current[current.length - 1].start_ms + current[current.length - 1].duration_ms,
      });
      current = [];
    }
  }
  return lines;
}

function scheduleChunk(track: CaptionTrack): void {
  const isFirstChunk = anchorMs === null;
  if (isFirstChunk) {
    anchorMs = performance.now();
    revealed = [];
    segments = [];
    latestScheduledEndMs = 0;
  }
  if (track.envelope?.length && track.frame_ms > 0) {
    segments.push({ startMs: track.envelope_start_ms ?? 0, frameMs: track.frame_ms, values: track.envelope });
  }
  if (suppressNext) {
    suppressNext = false;
    return;
  }
  if (!track.words.length) return;
  const anchor = anchorMs as number;

  // Legacy word-by-word reveal (for any word listeners)
  for (const w of track.words) {
    const delay = Math.max(0, w.start_ms - (performance.now() - anchor));
    timers.push(
      setTimeout(() => {
        revealed.push(w.text);
        notify(false);
      }, delay),
    );
  }

  // Modern Line-by-line replacement schedule (BBC/cognitive reading dwell + monotonic seq)
  const lines = partitionIntoLines(track.words);
  for (const l of lines) {
    if (l.end_ms > latestScheduledEndMs) {
      latestScheduledEndMs = l.end_ms;
    }
  }

  for (let li = 0; li < lines.length; li++) {
    const line = lines[li];
    const prevLineText = li > 0 ? lines[li - 1].text : undefined;
    const isLastInChunk = li === lines.length - 1;
    const lineSeq = ++currentLineSeq;
    const now = performance.now();
    const startDelay = Math.max(0, line.start_ms - (now - anchor));

    const wordCount = line.text.trim().split(/\s+/).length;
    const spokenDuration = Math.max(300, line.end_ms - line.start_ms);
    // Hard cognitive dwell floor: 2500ms minimum for intermediate lines, 3500ms for final line!
    const minDwell = isLastInChunk ? Math.max(3500, wordCount * 360) : Math.max(2500, wordCount * 320);
    const linger = isLastInChunk ? 2000 : 1200;
    const displayDuration = Math.max(minDwell, spokenDuration + linger);

    const fadeDelay = startDelay + displayDuration;
    const clearDelay = fadeDelay + 450;

    timers.push(
      setTimeout(() => {
        activeLineSeq = lineSeq;
        useAssistant.getState().setCaptionActive(true);
        console.log(`[CAPTION] Line ${li + 1}/${lines.length} (seq=${lineSeq}) ACTIVE: "${line.text}" (prev="${prevLineText || ''}")`);
        notifyLine({ text: line.text, previousText: prevLineText, phase: "active" }, false);
      }, startDelay)
    );

    timers.push(
      setTimeout(() => {
        if (activeLineSeq === lineSeq) {
          console.log(`[CAPTION] Line ${li + 1}/${lines.length} (seq=${lineSeq}) FADING: "${line.text}"`);
          notifyLine({ text: line.text, previousText: prevLineText, phase: "fading" }, false);
        }
      }, fadeDelay)
    );

    timers.push(
      setTimeout(() => {
        if (activeLineSeq === lineSeq) {
          const isUtteranceEnd = line.end_ms >= latestScheduledEndMs;
          console.log(`[CAPTION] Line ${li + 1}/${lines.length} (seq=${lineSeq}) CLEARED (utteranceEnd=${isUtteranceEnd})`);
          notifyLine({ text: "", previousText: "", phase: "cleared" }, isUtteranceEnd);
          if (isUtteranceEnd) {
            useAssistant.getState().setCaptionActive(false);
            console.log("[CAPTION] All caption lines cleared, captionActive=false");
          }
        }
      }, clearDelay)
    );
  }

  // Re-arm legacy doneTimer
  const last = track.words[track.words.length - 1];
  const endMs = last.start_ms + last.duration_ms;
  if (doneTimer) clearTimeout(doneTimer);
  const delay = Math.max(0, endMs - (performance.now() - anchor)) + 50;
  doneTimer = setTimeout(() => notify(true), delay);
}

let initialized = false;

/** Call once (idempotent) to start listening for Rust's `tts:caption` event. */
export function initCaptionListener(): void {
  if (initialized) return;
  initialized = true;
  void import("@tauri-apps/api/event")
    .then(({ listen }) =>
      listen<CaptionTrack>("tts:caption", (event) => {
        if (event.payload) scheduleChunk(event.payload);
      }),
    )
    .catch(() => {
      // Outside Tauri (tests) — no-op.
    });
}
