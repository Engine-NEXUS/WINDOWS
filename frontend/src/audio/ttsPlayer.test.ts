import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async () => {}),
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(),
}));

import { isRustTtsPlaying, stopTts } from "./ttsPlayer";

describe("stopTts idempotency (Alexa-style barge-in)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("stops cleanly when nothing is playing (unconditional wake stop)", () => {
    expect(isRustTtsPlaying()).toBe(false);
    expect(() => {
      stopTts();
      stopTts();
    }).not.toThrow();
    expect(isRustTtsPlaying()).toBe(false);
  });

  it("stays stopped after repeated calls (no stuck-playing flag)", () => {
    stopTts();
    stopTts();
    stopTts();
    expect(isRustTtsPlaying()).toBe(false);
  });
});
