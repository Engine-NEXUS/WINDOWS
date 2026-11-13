// Small local "what I switched" log (per viewer convenience; storage may be unavailable, so every access is guarded).
export interface LogEntry { at: number; text: string }
const KEY = "nexus.features.log";
const MAX = 50;

export function loadLog(): LogEntry[] {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(raw) ? raw.filter((e) => e && typeof e.at === "number" && typeof e.text === "string") : [];
  } catch {
    return [];
  }
}

export function appendLog(text: string, now: () => number = Date.now): LogEntry[] {
  const next = [{ at: now(), text }, ...loadLog()].slice(0, MAX);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    /* ignore */
  }
  return next;
}
