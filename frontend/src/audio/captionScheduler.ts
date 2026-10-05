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
const listeners = new Set<Listener>();

function notify(done: boolean): void {
  const snapshot = revealed.slice();
  listeners.forEach((l) => l(snapshot, done));
}

/** Subscribe to reveal updates. Returns an unsubscribe function. */
export function onCaptionUpdate(fn: Listener): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
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
  notify(true);
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

function scheduleChunk(track: CaptionTrack): void {
  const isFirstChunk = anchorMs === null;
  if (isFirstChunk) {
    anchorMs = performance.now();
    revealed = [];
    segments = [];
  }
  if (track.envelope?.length && track.frame_ms > 0) {
    segments.push({ startMs: track.envelope_start_ms ?? 0, frameMs: track.frame_ms, values: track.envelope });
  }
  if (!track.words.length) return;
  const anchor = anchorMs as number;
  for (const w of track.words) {
    const delay = Math.max(0, w.start_ms - (performance.now() - anchor));
    timers.push(
      setTimeout(() => {
        revealed.push(w.text);
        notify(false);
      }, delay),
    );
  }
  // Re-arm the "done" signal to fire just after the LATEST word scheduled
  // so far finishes. Earlier chunks' doneTimers are superseded (cleared)
  // by each new chunk's arrival — only the true last chunk's timer survives.
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
