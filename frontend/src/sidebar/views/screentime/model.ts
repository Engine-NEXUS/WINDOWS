/**
 * Screen time view ("Ledger"): today's active time, the last days as bars, top apps and sites.
 * Payload from Rust (`run_screen_time`, `analysis.kind === "screentime"`). The data is the encrypted local Activity Manager history; nothing is shared.
 */
export interface StDay {
  /** "YYYY-MM-DD" */
  day: string;
  minutes: number;
  apps: { app: string; minutes: number }[];
}

export interface StPayload {
  kind: "screentime";
  /** The Activity Manager is switched on and supported. When false the view explains how to turn it on. */
  enabled: boolean;
  days: StDay[];
  /** Today's sites by minutes (empty when site tracking is off). */
  sites: { domain: string; minutes: number }[];
  sitesEnabled?: boolean;
}

export function isStPayload(v: unknown): v is StPayload {
  if (!v || typeof v !== "object") return false;
  const p = v as Partial<StPayload>;
  return p.kind === "screentime" && typeof p.enabled === "boolean" && Array.isArray(p.days) && Array.isArray(p.sites);
}

/** 135 -> "2h 15m", 45 -> "45m", 0 -> "0m". */
export function formatMinutes(min: number): string {
  const m = Math.max(0, Math.round(min));
  const h = Math.floor(m / 60);
  const r = m % 60;
  if (h === 0) return `${r}m`;
  return r === 0 ? `${h}h` : `${h}h ${r}m`;
}

const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** "2026-11-22" -> "Sun" (computed from the date parts, not the machine's zone). */
export function weekdayShort(day: string): string {
  const m = day.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  if (!m) return day;
  return WEEKDAYS[new Date(Date.UTC(+m[1], +m[2] - 1, +m[3])).getUTCDay()];
}

export interface StBar {
  day: string;
  label: string;
  minutes: number;
  /** 0..100, relative to the busiest day shown. */
  pct: number;
  today: boolean;
}

/** Oldest first, the last `count` days; the last entry of `days` is today. */
export function weekBars(days: StDay[], count = 7): StBar[] {
  const sorted = [...days].sort((a, b) => a.day.localeCompare(b.day)).slice(-count);
  const max = Math.max(1, ...sorted.map((d) => d.minutes));
  return sorted.map((d, i) => ({
    day: d.day,
    label: weekdayShort(d.day),
    minutes: d.minutes,
    pct: Math.round((d.minutes / max) * 100),
    today: i === sorted.length - 1,
  }));
}

export function today(days: StDay[]): StDay | null {
  if (days.length === 0) return null;
  return [...days].sort((a, b) => a.day.localeCompare(b.day))[days.length - 1];
}

export interface StRow {
  name: string;
  minutes: number;
  /** Share of the largest row, 0..100. */
  pct: number;
}

/** The top `limit` entries by minutes, ties broken by name; zero-minute rows are dropped. */
export function topRows(rows: { name: string; minutes: number }[], limit = 6): StRow[] {
  const sorted = rows.filter((r) => r.minutes > 0).sort((a, b) => b.minutes - a.minutes || a.name.localeCompare(b.name)).slice(0, limit);
  const max = Math.max(1, ...sorted.map((r) => r.minutes));
  return sorted.map((r) => ({ ...r, pct: Math.round((r.minutes / max) * 100) }));
}

/** Average of the days before today, rounded; null when there is no earlier day. */
export function averageBefore(days: StDay[]): number | null {
  const sorted = [...days].sort((a, b) => a.day.localeCompare(b.day));
  const before = sorted.slice(0, -1);
  if (before.length === 0) return null;
  return Math.round(before.reduce((s, d) => s + d.minutes, 0) / before.length);
}

/** "12m more than your average" / "20m less than your average" / "same as your average" / "". */
export function versusAverage(todayMin: number, avg: number | null): string {
  if (avg === null) return "";
  const diff = Math.round(todayMin - avg);
  if (Math.abs(diff) < 5) return "About your usual";
  return `${formatMinutes(Math.abs(diff))} ${diff > 0 ? "more" : "less"} than your usual`;
}
