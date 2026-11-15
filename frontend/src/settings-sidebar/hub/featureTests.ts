// Test buttons for the Features page. A row has a test only when a real command exists for it; no test = the row says so
// (never a fake green). Each runner returns a one-line detail; a thrown error is a FAIL.
import type { Invoke } from "./featureWrite";

export type TestStatus = "OK" | "WARN" | "FAIL";
export interface TestResult { status: TestStatus; detail: string; at: number }
export interface FeatureTest { label: string; run: (invoke: Invoke) => Promise<{ status: TestStatus; detail: string }> }

const ok = (detail: string) => ({ status: "OK" as const, detail });

export const FEATURE_TESTS: Record<string, FeatureTest> = {
  "wake-sound": {
    label: "Play",
    run: async (invoke) => {
      const s = (await invoke("get_settings")) as { wakeSoundStyle?: string; wakeSoundVolume?: number };
      await invoke("preview_wake_sound", { style: s.wakeSoundStyle ?? "rise", volume: s.wakeSoundVolume ?? 60 });
      return ok("Played the wake-up sound. Did you hear it?");
    },
  },
  "agent-monitor": {
    label: "Preview",
    run: async (invoke) => {
      await invoke("agent_island_preview");
      return ok("Showed the Agent Island preview at the top of the screen.");
    },
  },
  "mic-keepalive": {
    label: "Mic check",
    run: async (invoke) => {
      const r = (await invoke("mic_self_test")) as Record<string, unknown>;
      return ok(`Microphone test finished: ${JSON.stringify(r).slice(0, 160)}`);
    },
  },
  "wa-watch": {
    label: "Check bridge",
    run: async (invoke) => {
      const r = (await invoke("whatsapp_selftest")) as Record<string, unknown>;
      const failed = Object.values(r).some((v) => v === false);
      return { status: failed ? "WARN" : "OK", detail: JSON.stringify(r).slice(0, 200) };
    },
  },
  "screen-tour": {
    label: "Test key",
    run: async (invoke) => {
      const r = await invoke("vision_test_key");
      return ok(`Vision key check: ${JSON.stringify(r).slice(0, 160)}`);
    },
  },
  "notify-native": {
    label: "Send test alert",
    run: async (invoke) => ok(String(await invoke("notify_test", { scenario: "pr-failed" }))),
  },
  "github-watch": {
    label: "Send test alert",
    run: async (invoke) => ok(String(await invoke("notify_test", { scenario: "build-failed" }))),
  },
  "notify-toast": {
    label: "Send test alert",
    run: async (invoke) => ok(String(await invoke("notify_test", { scenario: "pr-failed" }))),
  },
};

export interface AuditRow { layer: number; name: string; status: TestStatus; detail: string }
export interface AuditReport { timestamp: string; items: AuditRow[]; allHealthy: boolean }

export async function runFullCheck(invoke: Invoke): Promise<AuditReport> {
  return (await invoke("run_system_audit")) as AuditReport;
}

export async function runTest(invoke: Invoke, id: string, now: () => number = Date.now): Promise<TestResult> {
  const t = FEATURE_TESTS[id];
  if (!t) return { status: "WARN", detail: "No test for this feature yet.", at: now() };
  try {
    const r = await t.run(invoke);
    return { ...r, at: now() };
  } catch (e) {
    return { status: "FAIL", detail: String(e instanceof Error ? e.message : e), at: now() };
  }
}
