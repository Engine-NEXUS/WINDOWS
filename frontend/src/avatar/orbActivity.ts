// What NEXUS is doing, as one value, and which orb design shows it.
// Pure: no store, no events, no timers (the hook in useOrbActivity.ts wires those).

import type { AssistantState } from "../store/assistant";
import { ORB_DESIGNS, type OrbDesign } from "./dotted/resolveOrb";

export type OrbActivity =
  | "idle"
  | "listening"
  | "thinking"
  | "speaking"
  | "waiting"
  | "analysing"
  | "ghost"
  | "hand"
  | "dictation"
  | "agent"
  | "background";

export const ORB_ACTIVITIES: { id: OrbActivity; label: string }[] = [
  { id: "idle", label: "Idle" },
  { id: "listening", label: "Listening to you" },
  { id: "thinking", label: "Thinking" },
  { id: "speaking", label: "Speaking" },
  { id: "background", label: "Doing tasks in the background" },
  { id: "analysing", label: "Analysing the screen" },
  { id: "ghost", label: "Ghost mode" },
  { id: "hand", label: "Hand mode" },
  { id: "dictation", label: "Dictation / typing for you" },
  { id: "agent", label: "Watching a coding agent" },
  { id: "waiting", label: "Waiting for your answer" },
];

/** The owner's mapping (dev-orbs.html, 2026-11-07; idle = plain circle, 2026-11-08; listening = the wobbling ring that only moves with the mic; speaking stays the composing sash). */
export const DEFAULT_ORB_DESIGNS: Record<OrbActivity, OrbDesign> = {
  idle: "circle",
  listening: "breathing",
  thinking: "working",
  speaking: "composing",
  background: "weaving",
  analysing: "searching",
  ghost: "connecting",
  hand: "shaping",
  dictation: "shaping",
  agent: "solving",
  waiting: "breathing",
};

/** Signals the store does not carry on its own (filled by orbSignals.ts). */
export interface OrbSignals {
  /** Screen tour is capturing + running vision. */
  tourFetching: boolean;
  handActive: boolean;
  dictationActive: boolean;
  agentWatching: boolean;
  backgroundTasks: number;
}

export const NO_SIGNALS: OrbSignals = {
  tourFetching: false,
  handActive: false,
  dictationActive: false,
  agentWatching: false,
  backgroundTasks: 0,
};

export interface OrbActivityInputs extends OrbSignals {
  /** The turn state (already ghost-pinned by Avatar). */
  state: AssistantState;
  ghostActive: boolean;
  awaitingInput: boolean;
}

/**
 * Priority: the turn itself first (the orb must always answer "is it hearing / thinking / talking to me"),
 * then long-running modes while the turn is idle.
 *   waiting > analysing > speaking > thinking > listening > ghost > hand > dictation > agent > background > idle
 * "analysing" outranks thinking/speaking because the screen tour is what the turn is doing at that moment.
 */
export function deriveOrbActivity(i: OrbActivityInputs): OrbActivity {
  if (i.state === "speaking" && i.awaitingInput) return "waiting";
  if (i.tourFetching) return "analysing";
  if (i.state === "speaking") return "speaking";
  if (i.state === "thinking") return "thinking";
  if (i.state === "listening") return "listening";
  if (i.ghostActive) return "ghost";
  if (i.handActive) return "hand";
  if (i.dictationActive) return "dictation";
  if (i.agentWatching) return "agent";
  if (i.backgroundTasks > 0) return "background";
  return "idle";
}

const KNOWN_DESIGNS = new Set<string>(ORB_DESIGNS);

/** Design for an activity from a (possibly partial, possibly bad) saved mapping. Unknown ids fall back to the default. */
export function designFor(activity: OrbActivity, mapping?: Record<string, string> | null): OrbDesign {
  const chosen = mapping?.[activity];
  if (chosen && KNOWN_DESIGNS.has(chosen)) return chosen as OrbDesign;
  return DEFAULT_ORB_DESIGNS[activity];
}

/** Sanitise a saved mapping: keep only known activities with known designs. */
export function normalizeOrbDesigns(raw: unknown): Record<string, string> {
  const out: Record<string, string> = {};
  if (!raw || typeof raw !== "object") return out;
  for (const { id } of ORB_ACTIVITIES) {
    const v = (raw as Record<string, unknown>)[id];
    if (typeof v === "string" && KNOWN_DESIGNS.has(v)) out[id] = v;
  }
  return out;
}

/** Shortest time a shown activity stays up before another may replace it (stops flicker on rapid flips). */
export const MIN_DWELL_MS = 350;

export interface Settled {
  activity: OrbActivity;
  /** When > 0, re-run after this many ms (the target is waiting out the dwell). */
  waitMs: number;
}

/**
 * Hysteresis: show `target`, but never swap away from `shown` sooner than `dwellMs` after it appeared.
 * The first activity (nothing shown yet) and returning to the same value are immediate.
 */
export function settleActivity(
  shown: OrbActivity | null,
  target: OrbActivity,
  shownSinceMs: number,
  nowMs: number,
  dwellMs: number = MIN_DWELL_MS,
): Settled {
  if (shown === null || shown === target) return { activity: target, waitMs: 0 };
  const left = dwellMs - (nowMs - shownSinceMs);
  if (left <= 0) return { activity: target, waitMs: 0 };
  return { activity: shown, waitMs: left };
}
