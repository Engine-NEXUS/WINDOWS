import { describe, expect, it, vi } from "vitest";
import { FEATURES } from "./featureRegistry";
import { FEATURE_TESTS, runFullCheck, runTest } from "./featureTests";
import readiness from "./featureReadiness.json";
import map from "./featureReadiness.map.json";

describe("feature tests", () => {
  it("every test and readiness id is a registry row", () => {
    const ids = new Set(FEATURES.map((f) => f.id));
    for (const id of [...Object.keys(FEATURE_TESTS), ...Object.keys(readiness), ...Object.keys(map)]) expect(ids.has(id)).toBe(true);
  });
  it("a feature without a test says so, never a fake OK", async () => {
    const r = await runTest(vi.fn(), "autostart", () => 5);
    expect(r).toEqual({ status: "WARN", detail: "No test for this feature yet.", at: 5 });
  });
  it("a failing command is a FAIL with its message", async () => {
    const r = await runTest(vi.fn().mockRejectedValue(new Error("no bridge")), "agent-monitor", () => 1);
    expect(r.status).toBe("FAIL");
    expect(r.detail).toBe("no bridge");
  });
  it("a notification test sends only a fixed scenario", async () => {
    const inv = vi.fn().mockResolvedValue("spoken line");
    const r = await runTest(inv, "notify-native", () => 1);
    expect(inv).toHaveBeenCalledWith("notify_test", { scenario: "pr-failed" });
    expect(r.status).toBe("OK");
  });
  it("full check calls the audit command", async () => {
    const inv = vi.fn().mockResolvedValue({ timestamp: "t", items: [], allHealthy: true });
    expect((await runFullCheck(inv)).allHealthy).toBe(true);
    expect(inv).toHaveBeenCalledWith("run_system_audit");
  });
});
