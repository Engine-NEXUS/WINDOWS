/** Own the mic baton for the entire enrollment, including countdowns. */
export async function withEnrollmentMicrophone<T>(
  signal: AbortSignal,
  pauseWake: () => Promise<unknown>,
  run: () => Promise<T>,
  stopMicrophone: () => Promise<void>,
  resumeWake: () => Promise<unknown>,
): Promise<T> {
  let abortRun: (() => void) | undefined;
  try {
    await pauseWake(); // Never start recording before native pause succeeds.
    signal.throwIfAborted();
    // getUserMedia permission prompts cannot be cancelled. Release the native
    // baton on navigation without waiting for that prompt; getStream stops any
    // late-granted browser stream after checking the same AbortSignal.
    return await new Promise<T>((resolve, reject) => {
      abortRun = () => reject(signal.reason ?? new Error("Enrollment cancelled"));
      signal.addEventListener("abort", abortRun, { once: true });
      Promise.resolve().then(() => {
        signal.throwIfAborted();
        return run();
      }).then(resolve, reject);
    });
  } finally {
    if (abortRun) signal.removeEventListener("abort", abortRun);
    // Release browser capture before native code reacquires the microphone.
    try { await stopMicrophone(); }
    finally { await resumeWake(); }
  }
}

export function enrollmentDelay(ms: number, signal: AbortSignal): Promise<void> {
  signal.throwIfAborted();
  return new Promise((resolve, reject) => {
    const finish = () => { signal.removeEventListener("abort", abort); resolve(); };
    const timer = setTimeout(finish, ms);
    const abort = () => {
      clearTimeout(timer);
      signal.removeEventListener("abort", abort);
      reject(signal.reason ?? new Error("Enrollment cancelled"));
    };
    signal.addEventListener("abort", abort, { once: true });
  });
}

export const ENROLLMENT_PHRASES = [
  "Nexus", "Hey Nexus", "OK Nexus", "Nexus", "Hey Nexus",
  "OK Nexus", "Nexus", "Hey Nexus", "OK Nexus", "Nexus",
] as const;
