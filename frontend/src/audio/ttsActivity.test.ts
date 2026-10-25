import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const listeners: Record<string, () => void> = {};
const mockListen = vi.fn((event: string, handler: () => void) => {
  listeners[event] = handler;
  return Promise.resolve(() => {});
});

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

const mockSpeechStageRegistered = vi.fn(() => false);
vi.mock("./speechStage", () => ({
  speechStageRegistered: () => mockSpeechStageRegistered(),
}));

import { useAssistant } from "../store/assistant";
import { initTtsActivityListener } from "./ttsActivity";

describe("ttsActivity listener and 400ms grace window", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    mockSpeechStageRegistered.mockReturnValue(false);
    useAssistant.setState({ ttsActive: false });
    await initTtsActivityListener();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("subscribes to tts-started and tts-ended events", () => {
    expect(mockListen).toHaveBeenCalledWith("tts-started", expect.any(Function));
    expect(mockListen).toHaveBeenCalledWith("tts-ended", expect.any(Function));
    expect(listeners["tts-started"]).toBeDefined();
    expect(listeners["tts-ended"]).toBeDefined();
  });

  it("sets ttsActive to true immediately on tts-started", () => {
    expect(useAssistant.getState().ttsActive).toBe(false);
    listeners["tts-started"]();
    expect(useAssistant.getState().ttsActive).toBe(true);
  });

  it("keeps ttsActive true during the 400ms grace window after tts-ended", () => {
    listeners["tts-started"]();
    expect(useAssistant.getState().ttsActive).toBe(true);

    listeners["tts-ended"]();
    // Still active immediately
    expect(useAssistant.getState().ttsActive).toBe(true);

    // Advance 390ms: still active
    vi.advanceTimersByTime(390);
    expect(useAssistant.getState().ttsActive).toBe(true);

    // Advance remaining 10ms (400ms total): now inactive
    vi.advanceTimersByTime(10);
    expect(useAssistant.getState().ttsActive).toBe(false);
  });

  it("cancels grace expiration if tts-started arrives before the 400ms timer expires", () => {
    listeners["tts-started"]();
    listeners["tts-ended"]();
    vi.advanceTimersByTime(200);
    expect(useAssistant.getState().ttsActive).toBe(true);

    // New chunk starts playing within grace window
    listeners["tts-started"]();

    // Advance past the initial 400ms window
    vi.advanceTimersByTime(300);
    // Should still be active because grace timer was cancelled
    expect(useAssistant.getState().ttsActive).toBe(true);
  });

  it("ignores events when speech stage is registered (speech stage takes precedence)", () => {
    mockSpeechStageRegistered.mockReturnValue(true);
    useAssistant.setState({ ttsActive: false });

    listeners["tts-started"]();
    expect(useAssistant.getState().ttsActive).toBe(false);

    useAssistant.setState({ ttsActive: true });
    listeners["tts-ended"]();
    vi.advanceTimersByTime(500);
    // Should not have been changed by ttsActivity
    expect(useAssistant.getState().ttsActive).toBe(true);
  });

  it("does not re-register if initTtsActivityListener is invoked multiple times", async () => {
    const initialCallCount = mockListen.mock.calls.length;
    await initTtsActivityListener();
    expect(mockListen.mock.calls.length).toBe(initialCallCount);
  });
});
