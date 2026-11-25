// The local speech model (about 250 MB, fetched once): what the Command Hub says about it.

export type SpeechModelState = "ready" | "downloading" | "missing" | "unknown";

export interface SpeechModelStatus {
  state: SpeechModelState;
  downloaded_mb: number;
  expected_mb: number;
}

export function parseSpeechModelStatus(raw: unknown): SpeechModelStatus | null {
  if (!raw || typeof raw !== "object") return null;
  const r = raw as Record<string, unknown>;
  const state = r.state;
  if (state !== "ready" && state !== "downloading" && state !== "missing" && state !== "unknown") return null;
  const num = (v: unknown) => (typeof v === "number" && Number.isFinite(v) && v >= 0 ? v : 0);
  return { state, downloaded_mb: num(r.downloaded_mb), expected_mb: Math.max(1, num(r.expected_mb) || 245) };
}

export function speechModelLine(s: SpeechModelStatus | null): string {
  if (!s) return "Checking the local speech model…";
  switch (s.state) {
    case "ready":
      return "Downloaded. Live captions can use it whenever the local speech server is running (no key, no quota, audio stays here).";
    case "downloading": {
      const pct = Math.min(99, Math.round((s.downloaded_mb / s.expected_mb) * 100));
      return `Downloading in the background: ${s.downloaded_mb} of about ${s.expected_mb} MB (${pct}%). Speech and captions use the cloud meanwhile.`;
    }
    case "missing":
      return `Not downloaded yet (about ${s.expected_mb} MB, once). Without it, the first local command waits for the whole download.`;
    default:
      return "This system keeps the model somewhere NEXUS cannot check.";
  }
}

/** Whether the "Download now" button is offered. */
export function canDownload(s: SpeechModelStatus | null): boolean {
  return s?.state === "missing";
}
