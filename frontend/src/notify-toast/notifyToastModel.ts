/** Display payload of one alert (built in Rust: notify_toast::payload). It never carries a URL. */
export interface ToastPayload {
  seq: number;
  eventId: string;
  provider: string;
  status: "failed" | "recovered" | "info";
  kind: string;
  title: string;
  preview: string;
  count: number;
  action: string;
  durationMs: number;
}

export const TICK_MS = 100;

/** Pure countdown: time left after dt ms, paused while the pointer or keyboard focus is on the toast. */
export function remainingAfter(remaining: number, dt: number, paused: boolean): number {
  if (paused) return remaining;
  return Math.max(0, remaining - Math.max(0, dt));
}

/** Share of the time still left, 0..1 (the thin progress line). */
export function progress(remaining: number, total: number): number {
  if (!(total > 0)) return 0;
  return Math.min(1, Math.max(0, remaining / total));
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
    status,
    kind: str("kind"),
    title: str("title"),
    preview: str("preview"),
    count: Math.max(1, num("count", 1)),
    action: str("action") || "Open",
    durationMs: Math.min(300_000, Math.max(1_000, num("durationMs", 5_000))),
  };
}
