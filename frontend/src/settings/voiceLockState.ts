/** Native readback is observation only: it never grants voice authority. */
export type NativeCall = (command: string, args?: { patch: Record<string, unknown> }) => Promise<unknown>;
export interface LockStatus { effective: boolean | null; profileReady: boolean; checking: boolean; saving: boolean; saveVerified: boolean; error: string | null }
export class VoiceLockState {
  state: LockStatus = { effective: null, profileReady: false, checking: false, saving: false, saveVerified: false, error: null };
  onChange: (() => void) | null = null;
  private epoch = 0;
  constructor(private call: NativeCall) {}
  private publish(patch: Partial<LockStatus>) { this.state = { ...this.state, ...patch }; this.onChange?.(); }
  private async read() {
    const [effective, settings, profile] = await Promise.all([
      this.call("voice_lock_enabled"), this.call("get_settings"), this.call("get_voice_profile_status"),
    ]);
    if (typeof effective !== "boolean") throw new Error("Native Voice Lock status is unavailable.");
    return { effective, profileReady: (profile as { enrolled?: boolean } | null)?.enrolled === true,
      agrees: (settings as { speakerVerification?: boolean } | null)?.speakerVerification === effective };
  }
  async refresh() {
    if (this.state.saving) return;
    const epoch = ++this.epoch;
    this.publish({ checking: true, effective: null, profileReady: false, saveVerified: false, error: null });
    try {
      const status = await this.read();
      if (epoch !== this.epoch) return;
      this.publish({ effective: status.effective, profileReady: status.profileReady,
        error: status.agrees ? null : "Native routing and saved Voice Lock settings disagree. Refresh before testing." });
    } catch (error) { if (epoch === this.epoch) this.publish({ effective: null, profileReady: false, error: String(error) }); }
    finally { if (epoch === this.epoch) this.publish({ checking: false }); }
  }
  async save(patch: Record<string, unknown>) {
    if (this.state.saving) throw new Error("Save already in progress.");
    ++this.epoch; // invalidate any earlier focus/reopen read
    this.publish({ saving: true, checking: false, effective: null, saveVerified: false, error: null });
    try {
      if (patch.speakerVerification === true) {
        const profile = await this.call("get_voice_profile_status") as { enrolled?: boolean } | null;
        if (profile?.enrolled !== true) throw new Error("Enroll a valid voice profile before enabling Voice Lock.");
      }
      if (Object.keys(patch).length) await this.call("settings_patch", { patch });
      const status = await this.read();
      this.publish({ effective: status.effective, profileReady: status.profileReady });
      if (patch.speakerVerification === true && !status.profileReady) throw new Error("Voice profile became unavailable. Save was not verified.");
      if (!status.agrees) throw new Error("Native routing and saved Voice Lock settings disagree. Save was not verified.");
      if (typeof patch.speakerVerification === "boolean" && patch.speakerVerification !== status.effective)
        throw new Error("Native Voice Lock does not match the requested setting. Save was not verified.");
      this.publish({ saveVerified: true });
    } catch (error) { this.publish({ error: String(error) }); throw error; }
    finally { this.publish({ saving: false }); }
  }
}

/** Clear only acknowledged values; edits made during a slow save remain dirty. */
export function clearSubmittedFields<T extends object>(dirty: Set<keyof T>, submitted: T, current: T) {
  for (const key of [...dirty]) {
    if (Object.prototype.hasOwnProperty.call(submitted, key) && Object.is(current[key], submitted[key])) dirty.delete(key);
  }
}

/** Refresh untouched lock drafts without discarding an explicit local edit. */
export function applyNativeLockToDraft<T extends { speakerVerification: boolean }>(current: T, dirty: ReadonlySet<keyof T>, effective: boolean | null): T {
  if (effective === null || dirty.has("speakerVerification") || current.speakerVerification === effective) return current;
  return { ...current, speakerVerification: effective };
}
