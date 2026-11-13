import { FEATURES, effectiveValue, type Feature, type SettingsMap } from "./featureRegistry";

export type Invoke = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export interface WriteResult {
  ok: boolean;
  /** What the backend reports after the write (or the last known good value when the write failed). */
  settings: SettingsMap | null;
  /** Plain-language problem, when ok is false. */
  error?: string;
}

/** Titles of the features in `patch` whose effective value is not what the patch asked for. */
export function notTaken(patch: SettingsMap, after: SettingsMap, all: Feature[] = FEATURES): string[] {
  return all.filter((f) => f.key in patch && effectiveValue(after, f) !== patch[f.key]).map((f) => f.title);
}

/**
 * Write only the changed keys with `settings_patch`, then read the settings back and confirm the backend really uses the
 * new values. A switch must never look changed because we hoped it was (the Voice Lock "Saved but still off" bug).
 * Autostart also goes through its own command, which registers or removes the Windows start-up entry.
 */
export async function writeAndVerify(invoke: Invoke, patch: SettingsMap): Promise<WriteResult> {
  try {
    if ("autostart" in patch) await invoke("set_autostart", { enabled: patch.autostart === true });
    await invoke("settings_patch", { patch });
  } catch (e) {
    const settings = await invoke<SettingsMap>("get_settings").catch(() => null);
    return { ok: false, settings, error: `Could not save: ${String(e)}` };
  }
  try {
    const after = await invoke<SettingsMap>("get_settings");
    const wrong = notTaken(patch, after);
    if (wrong.length > 0) return { ok: false, settings: after, error: `Saved, but NEXUS still reports the old value for: ${wrong.join(", ")}.` };
    return { ok: true, settings: after };
  } catch (e) {
    return { ok: false, settings: null, error: `Saved, but could not read the settings back to confirm: ${String(e)}` };
  }
}
