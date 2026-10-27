import { describe, expect, it, vi } from "vitest";
import { ENROLLMENT_PHRASES, enrollmentDelay, withEnrollmentMicrophone } from "./enrollmentSession";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("enrollment microphone ownership", () => {
  it("waits for native pause, keeps it through all clips, and releases browser tracks before resume", async () => {
    const events: string[] = [];
    const pause = deferred();
    const result = withEnrollmentMicrophone(new AbortController().signal,
      async () => { events.push("pause"); await pause.promise; },
      async () => { for (let i = 0; i < 10; i++) events.push(`record:${i}`); events.push("save"); return 10; },
      async () => { events.push("stop-tracks"); },
      async () => { events.push("resume"); },
    );
    expect(events).toEqual(["pause"]);
    await Promise.resolve(); await Promise.resolve();
    expect(events).toEqual(["pause"]);
    pause.resolve();
    await expect(result).resolves.toBe(10);
    expect(events).toEqual(["pause", ...Array.from({ length: 10 }, (_, i) => `record:${i}`), "save", "stop-tracks", "resume"]);
  });

  it("never records if native pause fails, and attempts recovery", async () => {
    const record = vi.fn(); const stop = vi.fn(); const resume = vi.fn();
    await expect(withEnrollmentMicrophone(new AbortController().signal,
      async () => { throw new Error("device pause failed"); }, record, stop, resume,
    )).rejects.toThrow("device pause failed");
    expect(record).not.toHaveBeenCalled();
    expect(stop).toHaveBeenCalledOnce(); expect(resume).toHaveBeenCalledOnce();
  });

  it("restores native listening when microphone permission or recording fails", async () => {
    const events: string[] = [];
    await expect(withEnrollmentMicrophone(new AbortController().signal,
      async () => { events.push("pause"); },
      async () => { throw new Error("permission denied"); },
      async () => { events.push("stop-tracks"); },
      async () => { events.push("resume"); },
    )).rejects.toThrow("permission denied");
    expect(events).toEqual(["pause", "stop-tracks", "resume"]);
  });

  it("cancels navigation during a countdown without recording and restores listening", async () => {
    const controller = new AbortController(); const record = vi.fn(); const resume = vi.fn();
    const started = deferred();
    const result = withEnrollmentMicrophone(controller.signal, async () => {}, async () => {
      started.resolve();
      await enrollmentDelay(60_000, controller.signal); record();
    }, async () => {}, resume);
    await started.promise;
    controller.abort(new Error("navigation"));
    await expect(result).rejects.toThrow("navigation");
    expect(record).not.toHaveBeenCalled(); expect(resume).toHaveBeenCalledOnce();
  });

  it("releases a pause which finishes after the component unmounts", async () => {
    const controller = new AbortController(); const pause = deferred();
    const record = vi.fn(); const resume = vi.fn();
    const result = withEnrollmentMicrophone(controller.signal, () => pause.promise, record, async () => {}, resume);
    controller.abort(new Error("unmounted")); pause.resolve();
    await expect(result).rejects.toThrow("unmounted");
    expect(record).not.toHaveBeenCalled(); expect(resume).toHaveBeenCalledOnce();
  });

  it("restores listening on navigation while microphone permission is still pending", async () => {
    const controller = new AbortController();
    const permission = deferred(); const started = deferred();
    const record = vi.fn(); const resume = vi.fn();
    const result = withEnrollmentMicrophone(controller.signal, async () => {}, async () => {
      started.resolve();
      await permission.promise;
      controller.signal.throwIfAborted();
      record();
    }, async () => {}, resume);
    await started.promise;
    controller.abort(new Error("navigation"));
    await expect(result).rejects.toThrow("navigation");
    expect(resume).toHaveBeenCalledOnce();
    permission.resolve();
    await Promise.resolve(); await Promise.resolve();
    expect(record).not.toHaveBeenCalled();
    expect(resume).toHaveBeenCalledOnce();
  }, 1000);

  it("attempts resume even when browser cleanup fails", async () => {
    const resume = vi.fn();
    await expect(withEnrollmentMicrophone(new AbortController().signal, async () => {}, async () => {},
      async () => { throw new Error("cleanup failed"); }, resume,
    )).rejects.toThrow("cleanup failed");
    expect(resume).toHaveBeenCalledOnce();
  });

  it("reports native resume failure instead of showing enrollment success", async () => {
    await expect(withEnrollmentMicrophone(new AbortController().signal, async () => {}, async () => {}, async () => {},
      async () => { throw new Error("resume failed"); },
    )).rejects.toThrow("resume failed");
  });

  it("prompts all three phrases in both enrollment and held-out calibration", () => {
    expect(ENROLLMENT_PHRASES).toHaveLength(10);
    for (const half of [ENROLLMENT_PHRASES.slice(0, 5), ENROLLMENT_PHRASES.slice(5)]) {
      expect(new Set(half)).toEqual(new Set(["Nexus", "Hey Nexus", "OK Nexus"]));
    }
  });
});
