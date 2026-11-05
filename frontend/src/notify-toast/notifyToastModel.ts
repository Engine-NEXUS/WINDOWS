/** One label/value line under the headline (built in Rust: notify_toast::headline_and_rows). */
export interface ToastRow {
  label: string;
  value: string;
}

/** Display payload of one alert (built in Rust: notify_toast::payload). It never carries a URL. */
export interface ToastPayload {
  seq: number;
  eventId: string;
  provider: string;
  icon: string;
  status: "failed" | "recovered" | "info";
  kind: string;
  /** The big line (the subject for mail, the sentence for a failed build). */
  headline: string;
  rows: ToastRow[];
  count: number;
  action: string;
  durationMs: number;
}

export const TICK_MS = 100;
/** The card fades out during this last stretch of its time. */
export const FADE_MS = 300;

/** Pure countdown: time left after dt ms, paused while the pointer or keyboard focus is on the toast. */
export function remainingAfter(remaining: number, dt: number, paused: boolean): number {
  if (paused) return remaining;
  return Math.max(0, remaining - Math.max(0, dt));
}

/** True while the card should be fading out. */
export function isFading(remaining: number): boolean {
  return remaining > 0 && remaining <= FADE_MS;
}

export function statusLabel(s: ToastPayload["status"]): string {
  switch (s) {
    case "failed":
      return "Failed";
    case "recovered":
      return "Recovered";
    default:
      return "New";
  }
}

/** "+2 more" when the same alert was grouped; empty for a single occurrence. */
export function moreLabel(count: number): string {
  return count > 1 ? `+${count - 1} more` : "";
}

function asRows(v: unknown): ToastRow[] {
  if (!Array.isArray(v)) return [];
  const out: ToastRow[] = [];
  for (const r of v) {
    if (!r || typeof r !== "object") continue;
    const o = r as Record<string, unknown>;
    if (typeof o.label === "string" && typeof o.value === "string" && o.value.trim()) {
      out.push({ label: o.label.slice(0, 20), value: o.value.slice(0, 120) });
    }
    if (out.length >= 4) break;
  }
  return out;
}

/** Narrow unknown event data to a payload, or null (the page never trusts what it did not shape). */
export function asPayload(v: unknown): ToastPayload | null {
  if (!v || typeof v !== "object") return null;
  const o = v as Record<string, unknown>;
  const str = (k: string) => (typeof o[k] === "string" ? (o[k] as string) : "");
  const num = (k: string, d: number) => (typeof o[k] === "number" && Number.isFinite(o[k] as number) ? (o[k] as number) : d);
  const status = o.status === "failed" || o.status === "recovered" ? (o.status as ToastPayload["status"]) : "info";
  const eventId = str("eventId");
  if (!eventId) return null;
  return {
    seq: num("seq", 0),
    eventId,
    provider: str("provider") || "NEXUS",
    icon: str("icon"),
    status,
    kind: str("kind"),
    headline: str("headline") || str("title"),
    rows: asRows(o.rows),
    count: Math.max(1, num("count", 1)),
    action: str("action") || "Open",
    durationMs: Math.min(300_000, Math.max(1_000, num("durationMs", 5_000))),
  };
}
