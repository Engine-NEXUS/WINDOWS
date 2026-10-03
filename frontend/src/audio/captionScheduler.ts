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
}

type Listener = (revealed: string[], done: boolean) => void;

let anchorMs: number | null = null;
let timers: ReturnType<typeof setTimeout>[] = [];
let doneTimer: ReturnType<typeof setTimeout> | null = null;
let revealed: string[] = [];
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
  notify(true);
}

function scheduleChunk(track: CaptionTrack): void {
  if (!track.words.length) return;
  const isFirstChunk = anchorMs === null;
  if (isFirstChunk) {
    anchorMs = performance.now();
    revealed = [];
  }
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
