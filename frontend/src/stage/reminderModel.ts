// Pure rules for the universal reminder animation (see src-tauri/src/reminder.rs and docs/changes/195-...).
// Nothing here knows what a reminder is for: `kind` is only a label. The stage draws whatever Rust sends.

export interface ReminderEvent {
  id: string;
  kind: string;
  title: string;
  /** The moment it fires, epoch ms. */
  atMs: number;
  spoken: string;
  alert: string;
  source: string;
  display?: { countdown?: boolean };
}

export interface FiredNotice {
  id: string;
  title: string;
  alert: string;
}

const KIND_LABELS: Record<string, string> = {
  trip: "Trip",
  reminder: "Reminder",
  calendar: "Calendar",
  timer: "Timer",
  study: "Study",
};

export function kindLabel(kind: string): string {
  return KIND_LABELS[kind] ?? "Reminder";
}

/** Only well-formed events are drawn; a malformed event must not blank the stage. */
export function parseReminder(v: unknown): ReminderEvent | null {
  if (!v || typeof v !== "object") return null;
  const e = v as Record<string, unknown>;
  if (typeof e.id !== "string" || !e.id || typeof e.title !== "string" || !e.title) return null;
  if (typeof e.atMs !== "number" || !Number.isFinite(e.atMs)) return null;
  return {
    id: e.id,
    kind: typeof e.kind === "string" ? e.kind : "reminder",
    title: e.title,
    atMs: e.atMs,
    spoken: typeof e.spoken === "string" ? e.spoken : "",
    alert: typeof e.alert === "string" ? e.alert : e.title,
    source: typeof e.source === "string" ? e.source : "",
    display: e.display && typeof e.display === "object" ? (e.display as { countdown?: boolean }) : undefined,
  };
}

export function parseReminders(v: unknown): ReminderEvent[] {
  if (!Array.isArray(v)) return [];
  return v.map(parseReminder).filter((e): e is ReminderEvent => e !== null);
}

export function parseFired(v: unknown): FiredNotice | null {
  if (!v || typeof v !== "object") return null;
  const e = v as Record<string, unknown>;
  if (typeof e.id !== "string" || !e.id) return null;
  return { id: e.id, title: typeof e.title === "string" ? e.title : "", alert: typeof e.alert === "string" && e.alert ? e.alert : "Reminder" };
}

/** The four clock digits and AM/PM for an instant (local time, 12-hour clock). */
export interface ClockDigits {
  h1: number;
  h2: number;
  m1: number;
  m2: number;
  ap: "AM" | "PM";
  text: string;
}

export function digitsFor(ms: number): ClockDigits {
  const d = new Date(ms);
  const h = d.getHours();
  const m = d.getMinutes();
  const h12 = h % 12 || 12;
  const ap = h >= 12 ? "PM" : "AM";
  return { h1: h12 >= 10 ? 1 : 0, h2: h12 % 10, m1: Math.floor(m / 10), m2: m % 10, ap, text: `${h12}:${String(m).padStart(2, "0")} ${ap}` };
}

/** Each odometer strip holds 40 digits (0 to 9 repeated). A roll never needs more than this many steps. */
export const STRIP_LENGTH = 40;

/**
 * How many strip steps column `col` (0 hour-tens, 1 hour-units, 2 minute-tens, 3 minute-units) moves from digit `cur` to
 * digit `want`: forward only, with one extra full turn for the hour-units and minute-tens columns and two for the
 * minute-units column, so the roll reads as a clock running forward. The hour-tens column only moves the short way.
 */
export function rollSteps(col: number, cur: number, want: number): number {
  const direct = (((want - cur) % 10) + 10) % 10;
  if (col === 0) return direct;
  return direct + 10 + (col === 3 ? 10 : 0);
}

/** The strip index to start from and the one to end on; the start is always 0 to 9 so the end stays inside the strip. */
export function rollRange(col: number, cur: number, want: number): { from: number; to: number } {
  const from = ((cur % 10) + 10) % 10;
  const to = from + rollSteps(col, from, want);
  return { from, to };
}

/** "in 2 hours 14 minutes" / "in less than a minute". */
export function untilWords(ms: number): string {
  const d = Math.round(ms / 60000);
  if (d < 1) return "in less than a minute";
  const h = Math.floor(d / 60);
  const m = d % 60;
  const out: string[] = [];
  if (h) out.push(`${h} ${h === 1 ? "hour" : "hours"}`);
  if (m) out.push(`${m} ${m === 1 ? "minute" : "minutes"}`);
  return `in ${out.join(" ")}`;
}

/** "2h 13m" over an hour, "09:58" under it, "now" at zero. */
export function countdownText(ms: number): string {
  if (ms <= 0) return "now";
  const t = Math.round(ms / 1000);
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  return h > 0 ? `${h}h ${String(m).padStart(2, "0")}m` : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** The ring around a chip fills from 0 to 1 as the time passes. */
export function ringFraction(startedMs: number, atMs: number, nowMs: number): number {
  const total = Math.max(1, atMs - startedMs);
  return Math.min(1, Math.max(0, 1 - (atMs - nowMs) / total));
}

/**
 * Whether a waiting reminder keeps a chip on screen. The USER's switch always wins: a reminder may ask for no chip
 * (`display.countdown === false`) but can never show one when the user turned the countdown off. (A reminder that has
 * fired is always shown, whatever the switch says: that is the alert itself.)
 */
export function wantsChip(e: ReminderEvent, userCountdown: boolean, firing: boolean): boolean {
  if (firing) return true;
  if (!userCountdown) return false;
  return e.display?.countdown !== false;
}

/** At most this many chips are drawn; the rest are summed up as "+N". */
export const MAX_CHIPS = 3;

export function visibleChips(list: ReminderEvent[]): { shown: ReminderEvent[]; extra: number } {
  const sorted = [...list].sort((a, b) => a.atMs - b.atMs);
  return { shown: sorted.slice(0, MAX_CHIPS), extra: Math.max(0, sorted.length - MAX_CHIPS) };
}

/** Timings of the roll, in milliseconds at normal speed. The stage collapses them under reduced motion. */
export const TIMING = {
  titleIn: 200,
  toLabel: 700,
  rollMs: 1500,
  stagger: 90,
  settle: 250,
  holdAfter: 2200,
  vanish: 1000,
  alertShown: 4500,
} as const;

/** Total time from the start of a roll until the clock has settled. */
export function rollDuration(): number {
  return TIMING.titleIn + TIMING.toLabel + TIMING.rollMs + TIMING.stagger * 3 + TIMING.settle;
}
