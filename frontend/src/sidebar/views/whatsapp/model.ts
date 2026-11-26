/**
 * WhatsApp messages view ("Chat"): the pure part. A conversation as bubbles, grouped by day, with runs of one sender kept together.
 * The payload comes from Rust (`run_whatsapp_read` -> `show_sidebar_with_analysis`, `analysis.kind === "whatsapp"`).
 * Reading never marks the chat as read; the view says so.
 */
export interface WaMessage {
  id: string;
  /** Unix seconds (0 = unknown). */
  ts: number;
  text: string;
  fromMe: boolean;
}

export interface WaPayload {
  kind: "whatsapp";
  name: string;
  messages: WaMessage[];
  vip?: boolean;
  muted?: boolean;
}

export function isWaPayload(v: unknown): v is WaPayload {
  if (!v || typeof v !== "object") return false;
  const p = v as Partial<WaPayload>;
  return p.kind === "whatsapp" && typeof p.name === "string" && Array.isArray(p.messages);
}

export interface WaRow {
  id: string;
  text: string;
  fromMe: boolean;
  /** "10:42", or "" when the time is unknown. */
  time: string;
  /** First and last bubble of a run from the same sender (the tail and the sender line show on the last / first). */
  first: boolean;
  last: boolean;
}

export interface WaGroup {
  key: string;
  label: string;
  rows: WaRow[];
}

const DAY = 86_400;
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/** Local offset in minutes east of UTC; injectable so tests do not depend on the machine's zone. */
export function localOffsetMinutes(): number {
  return -new Date().getTimezoneOffset();
}

function local(ts: number, offsetMin: number): Date {
  return new Date((ts + offsetMin * 60) * 1000);
}

/** The local day number (days since 1970) of a timestamp. */
export function dayNumber(ts: number, offsetMin: number): number {
  return Math.floor((ts + offsetMin * 60) / DAY);
}

export function dayLabel(ts: number, nowSecs: number, offsetMin: number): string {
  if (!ts) return "Earlier";
  const diff = dayNumber(nowSecs, offsetMin) - dayNumber(ts, offsetMin);
  if (diff === 0) return "Today";
  if (diff === 1) return "Yesterday";
  const d = local(ts, offsetMin);
  const base = `${WEEKDAYS[d.getUTCDay()]} ${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]}`;
  return diff < 0 || diff >= 330 ? `${base} ${d.getUTCFullYear()}` : base;
}

export function clockTime(ts: number, offsetMin: number): string {
  if (!ts) return "";
  const d = local(ts, offsetMin);
  return `${String(d.getUTCHours()).padStart(2, "0")}:${String(d.getUTCMinutes()).padStart(2, "0")}`;
}

/** Up to two initials: "Asha Rao" -> "AR", "mummy" -> "M", "" -> "?". */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  const pick = (w: string) => Array.from(w)[0]?.toUpperCase() ?? "";
  return parts.length === 1 ? pick(parts[0]) : pick(parts[0]) + pick(parts[parts.length - 1]);
}

/** Oldest first, grouped by local day, consecutive messages from the same sender marked as one run. */
export function groupMessages(msgs: WaMessage[], nowSecs: number, offsetMin = localOffsetMinutes()): WaGroup[] {
  const sorted = [...msgs].sort((a, b) => a.ts - b.ts);
  const groups: WaGroup[] = [];
  for (const m of sorted) {
    const key = m.ts ? String(dayNumber(m.ts, offsetMin)) : "unknown";
    let g = groups[groups.length - 1];
    if (!g || g.key !== key) {
      g = { key, label: dayLabel(m.ts, nowSecs, offsetMin), rows: [] };
      groups.push(g);
    }
    g.rows.push({ id: m.id, text: m.text, fromMe: m.fromMe, time: clockTime(m.ts, offsetMin), first: true, last: true });
  }
  for (const g of groups) {
    g.rows.forEach((r, i) => {
      const prev = g.rows[i - 1];
      const next = g.rows[i + 1];
      r.first = !prev || prev.fromMe !== r.fromMe;
      r.last = !next || next.fromMe !== r.fromMe;
    });
  }
  return groups;
}

/** How many of the messages are from the other person (the ones that would be unread). */
export function incomingCount(msgs: WaMessage[]): number {
  return msgs.filter((m) => !m.fromMe).length;
}
