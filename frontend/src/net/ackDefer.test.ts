import { describe, expect, it, vi } from "vitest";
import { ACK_DELAY_MS, ACK_PLAY_MS, DeferredAck } from "./ackDefer";

describe("DeferredAck", () => {
  it("drops the ack when the answer is ready before the delay", () => {
    vi.useFakeTimers();
    const ack = new DeferredAck();
    const run = vi.fn();
    ack.schedule(run);
    vi.advanceTimersByTime(ACK_DELAY_MS - 100); // a 0.9 s answer arrives
    ack.cancel();
    vi.advanceTimersByTime(5000);
    expect(run).not.toHaveBeenCalled();
    expect(ack.waitMs()).toBe(0);
    vi.useRealTimers();
  });

  it("speaks the ack once when the answer is slow, and makes a late answer wait for it", () => {
    vi.useFakeTimers();
    let t = 0;
    const ack = new DeferredAck(() => t, (fn, ms) => setTimeout(() => { t += ms; fn(); }, ms));
    const run = vi.fn();
    ack.schedule(run);
    vi.advanceTimersByTime(ACK_DELAY_MS);
    expect(run).toHaveBeenCalledTimes(1);
    expect(ack.waitMs()).toBe(ACK_PLAY_MS);
    t += 400; // the answer arrives 0.4 s into the ack
    expect(ack.waitMs()).toBe(ACK_PLAY_MS - 400);
    t += ACK_PLAY_MS;
    expect(ack.waitMs()).toBe(0);
    vi.useRealTimers();
  });

  it("a new ack replaces a pending one", () => {
    vi.useFakeTimers();
    const ack = new DeferredAck();
    const first = vi.fn();
    const second = vi.fn();
    ack.schedule(first);
    ack.schedule(second);
    vi.advanceTimersByTime(ACK_DELAY_MS);
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});
