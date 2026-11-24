// Pure model rules for the ActionReceipt card (validation, normalization,
// length caps 60/60/90, visibility decisions, 6 s auto-hide, replacement, injected clock).

export const RECEIPT_AUTO_HIDE_MS = 6000;
export const HEADLINE_MAX_LEN = 60;
export const SUBJECT_MAX_LEN = 60;
export const TITLE_MAX_LEN = 60;
export const DETAIL_MAX_LEN = 90;

export type ActionReceiptKind = "open_app" | "open_site" | "search" | "media";

export interface ActionReceipt {
  kind: ActionReceiptKind;
  headline: string;
  subject: string;
  title: string | null;
  detail: string | null;
  ok: boolean;
}

export interface ActionReceiptState {
  receipt: ActionReceipt | null;
  visible: boolean;
  timestamp: number | null;
}

export const INITIAL_ACTION_RECEIPT_STATE: ActionReceiptState = {
  receipt: null,
  visible: false,
  timestamp: null,
};

/**
 * Validate and normalize incoming `action:receipt` payload.
 *
 * Rules:
 * - Returns null (ignored) if raw is malformed or invalid.
 * - `kind` must be one of "open_app", "open_site", "search", "media".
 * - `headline` and `subject` must be non-empty strings (after trim), capped at 60 chars.
 * - `ok` must be a boolean.
 * - `title` (optional): string or null/undefined. If string, trimmed and capped at 60 chars. Empty string -> null.
 * - `detail` (optional): string or null/undefined. If string, trimmed and capped at 90 chars. Empty string -> null.
 */
export function normalizeReceiptPayload(raw: unknown): ActionReceipt | null {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
    return null;
  }
  const o = raw as Record<string, unknown>;

  if (
    typeof o.kind !== "string" ||
    (o.kind !== "open_app" && o.kind !== "open_site" && o.kind !== "search" && o.kind !== "media")
  ) {
    return null;
  }

  if (typeof o.headline !== "string") {
    return null;
  }
  const headline = o.headline.trim();
  if (!headline) {
    return null;
  }

  if (typeof o.subject !== "string") {
    return null;
  }
  const subject = o.subject.trim();
  if (!subject) {
    return null;
  }

  if (typeof o.ok !== "boolean") {
    return null;
  }

  let title: string | null = null;
  if (o.title !== null && o.title !== undefined) {
    if (typeof o.title !== "string") {
      return null;
    }
    const t = o.title.trim();
    if (t) {
      title = t.slice(0, TITLE_MAX_LEN);
    }
  }

  let detail: string | null = null;
  if (o.detail !== null && o.detail !== undefined) {
    if (typeof o.detail !== "string") {
      return null;
    }
    const d = o.detail.trim();
    if (d) {
      detail = d.slice(0, DETAIL_MAX_LEN);
    }
  }

  return {
    kind: o.kind as ActionReceiptKind,
    headline: headline.slice(0, HEADLINE_MAX_LEN),
    subject: subject.slice(0, SUBJECT_MAX_LEN),
    title,
    detail,
    ok: o.ok,
  };
}

/** Format first line: "<headline> - <subject>" */
export function formatReceiptHeadline(receipt: ActionReceipt): string {
  return `${receipt.headline} • ${receipt.subject}`;
}

/** Check if the 6 s dwell has elapsed */
export function isReceiptExpired(state: ActionReceiptState, now: number = Date.now()): boolean {
  if (!state.visible || state.timestamp === null) {
    return false;
  }
  return now - state.timestamp >= RECEIPT_AUTO_HIDE_MS;
}

/** Handle new incoming receipt: normalizes, replaces active receipt, restarts timer */
export function handleReceipt(
  state: ActionReceiptState,
  raw: unknown,
  now: number = Date.now(),
): ActionReceiptState {
  const receipt = normalizeReceiptPayload(raw);
  if (!receipt) {
    // Malformed payloads are ignored; current receipt and timer preserved
    return state;
  }
  return {
    receipt,
    visible: true,
    timestamp: now,
  };
}

/** Handle cancel (e.g. orb:cancel) — hides immediately */
export function handleCancel(state: ActionReceiptState): ActionReceiptState {
  return {
    ...state,
    visible: false,
    timestamp: null,
  };
}

/** Advance clock tick and auto-expire receipt if duration elapsed */
export function handleTick(state: ActionReceiptState, now: number = Date.now()): ActionReceiptState {
  if (isReceiptExpired(state, now)) {
    return {
      ...state,
      visible: false,
      timestamp: null,
    };
  }
  return state;
}

/** Determine whether receipt should be displayed */
export function shouldShowReceipt(state: ActionReceiptState, now: number = Date.now()): boolean {
  if (!state.visible || !state.receipt || state.timestamp === null) {
    return false;
  }
  return !isReceiptExpired(state, now);
}

/**
 * Stateful model wrapper supporting an injected clock function.
 */
export class ActionReceiptModel {
  private state: ActionReceiptState;
  private clock: () => number;

  constructor(
    clock: () => number = () => Date.now(),
    initialState: ActionReceiptState = INITIAL_ACTION_RECEIPT_STATE,
  ) {
    this.clock = clock;
    this.state = { ...initialState };
  }

  getState(): ActionReceiptState {
    return { ...this.state };
  }

  getReceipt(): ActionReceipt | null {
    return this.state.receipt;
  }

  isVisible(): boolean {
    return shouldShowReceipt(this.state, this.clock());
  }

  onReceipt(payload: unknown): ActionReceiptState {
    this.state = handleReceipt(this.state, payload, this.clock());
    return this.getState();
  }

  onCancel(): ActionReceiptState {
    this.state = handleCancel(this.state);
    return this.getState();
  }

  tick(): ActionReceiptState {
    this.state = handleTick(this.state, this.clock());
    return this.getState();
  }
}
