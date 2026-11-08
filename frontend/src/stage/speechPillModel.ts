// Pure model rules for the Speech Pill (window, trim, visibility decisions, injected clock).

export const PARTIAL_WINDOW_WORDS = 5;
export const FINAL_CAP_WORDS = 12;
export const FINAL_LINGER_MS = 4000;

export interface SpeechPillState {
  text: string;
  visible: boolean;
  phase: "idle" | "partial" | "final";
  finalTimestamp: number | null;
}

export const INITIAL_SPEECH_PILL_STATE: SpeechPillState = {
  text: "",
  visible: false,
  phase: "idle",
  finalTimestamp: null,
};

/**
 * Format partial text:
 * Reuses LiveCaption 5-word sliding FIFO window logic for clean display during speech.
 */
export function formatPartialText(raw: unknown): string {
  let str = "";
  if (typeof raw === "string") {
    str = raw;
  } else if (raw && typeof raw === "object" && "text" in raw && typeof (raw as { text?: unknown }).text === "string") {
    str = (raw as { text: string }).text;
  }
  const words = str.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "";
  return words.length > PARTIAL_WINDOW_WORDS ? words.slice(-PARTIAL_WINDOW_WORDS).join(" ") : words.join(" ");
}

/**
 * Format final text:
 * Extracts transcript text from string or `{ session, text, raw_text, ... }` object.
 * Caps at ~12 words with a leading ellipsis ("… ") if longer. Empty/whitespace returns "".
 */
export function formatFinalText(raw: unknown): string {
  let str = "";
  if (typeof raw === "string") {
    str = raw;
  } else if (raw && typeof raw === "object" && "text" in raw && typeof (raw as { text?: unknown }).text === "string") {
    str = (raw as { text: string }).text;
  }
  const words = str.trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "";
  if (words.length > FINAL_CAP_WORDS) {
    return `… ${words.slice(-FINAL_CAP_WORDS).join(" ")}`;
  }
  return words.join(" ");
}

/** Handle partial update event */
export function handlePartial(state: SpeechPillState, payload: unknown): SpeechPillState {
  const text = formatPartialText(payload);
  if (!text) {
    return { ...state, text: "", visible: false, phase: "idle", finalTimestamp: null };
  }
  return { ...state, text, visible: true, phase: "partial", finalTimestamp: null };
}

/** Handle final transcript update event */
export function handleFinal(
  state: SpeechPillState,
  payload: unknown,
  now: number = Date.now(),
): SpeechPillState {
  const text = formatFinalText(payload);
  if (!text) {
    // Empty text (e.g. phantom turns, silence) hides immediately
    return { ...state, text: "", visible: false, phase: "idle", finalTimestamp: null };
  }
  return { ...state, text, visible: true, phase: "final", finalTimestamp: now };
}

/** Handle assistant reply / response caption / TTS start */
export function handleReplyStart(state: SpeechPillState): SpeechPillState {
  return { ...state, visible: false, phase: "idle", finalTimestamp: null };
}

/** Handle cancel (e.g. orb:cancel or Escape) */
export function handleCancel(state: SpeechPillState): SpeechPillState {
  return { ...state, text: "", visible: false, phase: "idle", finalTimestamp: null };
}

/** Check if a final transcript linger duration has expired */
export function isFinalExpired(state: SpeechPillState, now: number = Date.now()): boolean {
  if (state.phase !== "final" || !state.visible || state.finalTimestamp === null) {
    return false;
  }
  return now - state.finalTimestamp >= FINAL_LINGER_MS;
}

/** Advance clock tick and auto-expire final pill if linger duration elapsed */
export function handleTick(state: SpeechPillState, now: number = Date.now()): SpeechPillState {
  if (isFinalExpired(state, now)) {
    return { ...state, visible: false, phase: "idle", finalTimestamp: null };
  }
  return state;
}

/**
 * Determine whether the Speech Pill should be rendered:
 * - Hidden if reply / response caption / TTS is active (response caption wins the shared slot)
 * - Hidden if visible flag is false or text is blank
 * - Hidden if final timestamp has exceeded the 4s dwell window
 */
export function shouldShowPill(
  state: SpeechPillState,
  replyActive: boolean,
  now: number = Date.now(),
): boolean {
  if (replyActive) return false;
  if (!state.visible) return false;
  if (!state.text.trim()) return false;
  if (isFinalExpired(state, now)) return false;
  return true;
}

/**
 * Stateful model wrapper supporting an injected clock function.
 */
export class SpeechPillModel {
  private state: SpeechPillState;
  private clock: () => number;

  constructor(
    clock: () => number = () => Date.now(),
    initialState: SpeechPillState = INITIAL_SPEECH_PILL_STATE,
  ) {
    this.clock = clock;
    this.state = { ...initialState };
  }

  getState(): SpeechPillState {
    return { ...this.state };
  }

  isVisible(replyActive: boolean = false): boolean {
    return shouldShowPill(this.state, replyActive, this.clock());
  }

  getText(): string {
    return this.state.text;
  }

  onPartial(payload: unknown): SpeechPillState {
    this.state = handlePartial(this.state, payload);
    return this.getState();
  }

  onFinal(payload: unknown): SpeechPillState {
    this.state = handleFinal(this.state, payload, this.clock());
    return this.getState();
  }

  onReplyStart(): SpeechPillState {
    this.state = handleReplyStart(this.state);
    return this.getState();
  }

  onCancel(): SpeechPillState {
    this.state = handleCancel(this.state);
    return this.getState();
  }

  tick(): SpeechPillState {
    this.state = handleTick(this.state, this.clock());
    return this.getState();
  }
}

// ─── Letter-by-letter typing of the pill text ───────────────────────────────────────────────

/** Typing speed (characters per second) for short texts. */
export const TYPE_CPS = 55;
/** A text never takes longer than this to finish typing, however long it is. */
export const TYPE_MAX_MS = 1200;

/**
 * Characters typed (fractional) `elapsedMs` after typing started from `base` characters towards `total`.
 * Short texts type at TYPE_CPS; long ones speed up so they still finish within TYPE_MAX_MS.
 */
export function typingProgress(base: number, total: number, elapsedMs: number, cps: number = TYPE_CPS, maxMs: number = TYPE_MAX_MS): number {
  if (total <= 0) return 0;
  const from = Math.min(Math.max(0, base), total);
  const remaining = total - from;
  if (remaining <= 0) return total;
  const duration = Math.max(1, Math.min(maxMs, (remaining / cps) * 1000));
  const p = Math.min(1, Math.max(0, elapsedMs / duration));
  return from + remaining * p;
}

/** The visible slice for a fractional typed length: at least the first letter, never a trailing space. */
export function typedSlice(text: string, typed: number): string {
  if (!text) return "";
  const n = Math.min(text.length, Math.max(1, Math.floor(typed)));
  return text.slice(0, n).trimEnd();
}
