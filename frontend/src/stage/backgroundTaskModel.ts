// Pure rules for the chips that show work still running after its turn ended (see src-tauri/src/background_tasks.rs).

export interface BackgroundTask {
  id: string;
  kind: string;
  label: string;
  startedMs: number;
}

/** Keep the list the backend sent, newest last, and never more than a few chips (the rest are summed up). */
export const MAX_CHIPS = 3;

export function visibleTasks(list: BackgroundTask[]): { shown: BackgroundTask[]; extra: number } {
  const sorted = [...list].sort((a, b) => a.startedMs - b.startedMs);
  return { shown: sorted.slice(0, MAX_CHIPS), extra: Math.max(0, sorted.length - MAX_CHIPS) };
}

export function elapsedLabel(startedMs: number, nowMs: number): string {
  const s = Math.max(0, Math.floor((nowMs - startedMs) / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** Only well-formed entries are drawn; a malformed event must not blank the stage. */
export function parseTasks(payload: unknown): BackgroundTask[] {
  if (!Array.isArray(payload)) return [];
  return payload.filter(
    (t): t is BackgroundTask =>
      !!t && typeof t.id === "string" && typeof t.label === "string" && typeof t.kind === "string" && typeof t.startedMs === "number",
  );
}
