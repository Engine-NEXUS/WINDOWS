import { describe, it, expect, vi } from "vitest";
import { applyNativeLockToDraft, clearSubmittedFields, VoiceLockState } from "./voiceLockState";

function native(enabled = false, enrolled = true) {
  const call = vi.fn(async (command: string, args?: { patch: Record<string, unknown> }): Promise<unknown> => {
    if (command === "voice_lock_enabled") return enabled;
    if (command === "get_settings") return { speakerVerification: enabled };
    if (command === "get_voice_profile_status") return { enrolled };
    if (command === "settings_patch") { if (typeof args?.patch.speakerVerification === "boolean") enabled = args.patch.speakerVerification; return {}; }
    throw new Error(command);
  });
  return call;
}
function deferred<T>() { let resolve!: (v: T) => void; const promise = new Promise<T>(r => { resolve = r; }); return { promise, resolve }; }

describe("effective Voice Lock state", () => {
  it("starts unknown, never assumes defaults are effective", () => { expect(new VoiceLockState(native()).state.effective).toBe(null); });
  it("reads native routing state and profile on reopen", async () => { const m = new VoiceLockState(native(true)); await m.refresh(); expect(m.state).toMatchObject({ effective: true, profileReady: true, error: null }); });
  it("does not change effective state when a draft changes", async () => { const m = new VoiceLockState(native()); await m.refresh(); expect(m.state.effective).toBe(false); });
  it("reads back a successful save before confirming", async () => { const call = native(); const m = new VoiceLockState(call); await m.save({ speakerVerification: true }); expect(m.state.effective).toBe(true); expect(call.mock.calls.map(c => c[0])).toContain("voice_lock_enabled"); });
  it("refuses enable when the profile is unavailable", async () => { const call = native(false, false); const m = new VoiceLockState(call); await expect(m.save({ speakerVerification: true })).rejects.toThrow(/profile/i); expect(call.mock.calls.some(c => c[0] === "settings_patch")).toBe(false); });
  it("permits disabling a lock with no profile", async () => { const m = new VoiceLockState(native(true, false)); await m.save({ speakerVerification: false }); expect(m.state.effective).toBe(false); });
  it("rejects a patch acknowledged without the requested native state", async () => { const call = native(); const m = new VoiceLockState(async (c,a) => c === "settings_patch" ? {} : call(c,a)); await expect(m.save({ speakerVerification: true })).rejects.toThrow(/match/i); expect(m.state.effective).toBe(false); });
  it("shows native/settings disagreement instead of Saved", async () => { const call = native(); const m = new VoiceLockState(async (c,a) => c === "get_settings" ? { speakerVerification: true } : call(c,a)); await m.refresh(); expect(m.state.effective).toBe(false); expect(m.state.error).toMatch(/disagree/i); });
  it("failed readback never confirms a save", async () => { const m = new VoiceLockState(async () => { throw new Error("offline"); }); await expect(m.save({ ttsVolume: 60 })).rejects.toThrow("offline"); expect(m.state.effective).toBe(null); expect(m.state.error).toContain("offline"); });
  it("slow save remains busy and rejects duplicate saves", async () => { const d = deferred<unknown>(); const call = native(); const m = new VoiceLockState(async (c,a) => c === "settings_patch" ? d.promise : call(c,a)); const save = m.save({ ttsVolume: 60 }); expect(m.state.saving).toBe(true); const second = m.save({ ttsVolume: 70 }); d.resolve({}); await save; await expect(second).rejects.toThrow(/progress/i); expect(m.state.saving).toBe(false); });
  it("an older refresh cannot overwrite a newer one", async () => { const d = deferred<unknown>(); const call = native(true); let first = true; const m = new VoiceLockState(async (c,a) => { if (c === "voice_lock_enabled" && first) { first = false; return d.promise; } return call(c,a); }); const old = m.refresh(); await m.refresh(); d.resolve(false); await old; expect(m.state.effective).toBe(true); expect(m.state.error).toBe(null); });
  it("a stale pre-save refresh cannot overwrite the saved state", async () => { const d = deferred<unknown>(); const call = native(); let first = true; const m = new VoiceLockState(async (c,a) => { if (c === "voice_lock_enabled" && first) { first = false; return d.promise; } return call(c,a); }); const old = m.refresh(); await m.save({ speakerVerification: true }); d.resolve(false); await old; expect(m.state.effective).toBe(true); });
});

describe("in-flight settings drafts", () => {
  it("keeps a value edited during a save and clears the acknowledged value", () => {
    const dirty = new Set<"speakerVerification" | "ttsVolume">(["speakerVerification", "ttsVolume"]);
    clearSubmittedFields(dirty, { speakerVerification: true, ttsVolume: 50 }, { speakerVerification: false, ttsVolume: 50 });
    expect([...dirty]).toEqual(["speakerVerification"]);
  });
  it("keeps fields newly edited during a save", () => {
    const dirty = new Set<"speakerVerification" | "ttsVolume">(["speakerVerification", "ttsVolume"]);
    clearSubmittedFields<Partial<{speakerVerification: boolean; ttsVolume: number}>>(dirty, { ttsVolume: 50 }, { speakerVerification: true, ttsVolume: 50 });
    expect([...dirty]).toEqual(["speakerVerification"]);
  });
  it("rejects profile disappearance during enable", async () => {
    const call = native(); let patched = false;
    const m = new VoiceLockState(async (c,a) => {
      if (c === "settings_patch") patched = true;
      if (c === "get_voice_profile_status" && patched) return { enrolled: false };
      return call(c,a);
    });
    await expect(m.save({ speakerVerification: true })).rejects.toThrow(/profile/i);
  });
});

describe("save readback failures", () => {
  it("rejects native/settings disagreement after saving another field", async () => {
    const call = native();
    const m = new VoiceLockState(async (c,a) => c === "get_settings" ? { speakerVerification: true } : call(c,a));
    await expect(m.save({ ttsVolume: 60 })).rejects.toThrow(/disagree/i);
    expect(m.state.effective).toBe(false);
  });
  it("rejects readback failure after the patch has succeeded", async () => {
    let patched = false;
    const m = new VoiceLockState(async c => { if (c === "settings_patch") { patched = true; return {}; } throw new Error("read unavailable"); });
    await expect(m.save({ ttsVolume: 60 })).rejects.toThrow(/read unavailable/);
    expect(patched).toBe(true); expect(m.state.effective).toBe(null);
  });
  it("refresh sees another window changing the native state", async () => {
    const call = native(); const m = new VoiceLockState(call);
    await m.refresh(); expect(m.state.effective).toBe(false);
    await call("settings_patch", { patch: { speakerVerification: true } });
    await m.refresh(); expect(m.state.effective).toBe(true);
  });
  it("rejects malformed native status instead of coercing it", async () => {
    const call = native(); const m = new VoiceLockState(async (c,a) => c === "voice_lock_enabled" ? "false" : call(c,a));
    await m.refresh(); expect(m.state.effective).toBe(null); expect(m.state.error).toMatch(/unavailable/i);
  });
});

describe("reopened or stale settings drafts", () => {
  it("syncs an untouched draft to native state changed by another window", () => {
    const current = { speakerVerification: true, ttsVolume: 75 };
    expect(applyNativeLockToDraft(current, new Set<keyof typeof current>(), false)).toEqual({ speakerVerification: false, ttsVolume: 75 });
  });
  it("preserves an explicitly edited draft when another window changes state", () => {
    const current = { speakerVerification: false };
    expect(applyNativeLockToDraft(current, new Set<keyof typeof current>(["speakerVerification"]), true)).toBe(current);
  });
  it("does not invent a draft change from unknown native status", () => {
    const current = { speakerVerification: true };
    expect(applyNativeLockToDraft(current, new Set<keyof typeof current>(), null)).toBe(current);
  });
  it("invalidates old Save confirmation on refresh", async () => {
    const m = new VoiceLockState(native());
    await m.save({ speakerVerification: true }); expect(m.state.saveVerified).toBe(true);
    await m.refresh(); expect(m.state.saveVerified).toBe(false);
  });
});
