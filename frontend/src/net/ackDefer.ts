// The stock ack ("Right away sir.") is only useful when the answer takes a while. A fast answer (9Router
// replies in ~0.9 s) used to cut the ack after about a second and the owner heard half a word.
// So the ack is held back briefly, dropped if the result arrives first, and a result that arrives while the
// ack is audible waits for it to finish instead of stopping it.

/** How long to hold the ack back before speaking it. */
export const ACK_DELAY_MS = 600;
/** How long a stock ack is audible once it starts ("Right away sir." ≈ 1 s). */
export const ACK_PLAY_MS = 1100;

type TimerId = ReturnType<typeof setTimeout>;

export class DeferredAck {
  private timer: TimerId | null = null;
  private playingUntil = 0;

  constructor(
    private readonly now: () => number = () => Date.now(),
    private readonly setT: (fn: () => void, ms: number) => TimerId = (fn, ms) => setTimeout(fn, ms),
    private readonly clearT: (id: TimerId) => void = (id) => clearTimeout(id),
  ) {}

  /** Speak the ack after `delayMs` unless `cancel()` runs first. */
  schedule(run: () => void, delayMs: number = ACK_DELAY_MS): void {
    this.cancel();
    this.timer = this.setT(() => {
      this.timer = null;
      this.playingUntil = this.now() + ACK_PLAY_MS;
      run();
    }, delayMs);
  }

  /** The answer is ready: an ack that has not started is dropped. A playing ack is left alone. */
  cancel(): void {
    if (this.timer !== null) {
      this.clearT(this.timer);
      this.timer = null;
    }
  }

  isPending(): boolean {
    return this.timer !== null;
  }

  /** Milliseconds the answer must wait so a playing ack is not cut (0 when none is playing). */
  waitMs(): number {
    return Math.max(0, this.playingUntil - this.now());
  }
}
