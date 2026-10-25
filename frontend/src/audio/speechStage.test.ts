import { describe, expect, it, vi } from "vitest";
import { acceptSpeechFrame, captionAtPosition, clearSpeechStage, onSpeechStageLine, speechStageActive, type SpeechFrame } from "./speechStage";

const edge = [
  {
    text: "Hello world",
    start_ms: 0,
    end_ms: 500,
    estimated: false,
    words: [
      { text: "Hello", start_ms: 0, duration_ms: 200 },
      { text: "world", start_ms: 300, duration_ms: 200 },
    ],
  },
  {
    text: "Again",
    start_ms: 700,
    end_ms: 900,
    estimated: false,
    words: [{ text: "Again", start_ms: 700, duration_ms: 200 }],
  },
];

describe("playback-driven caption projection", () => {
  it("never reveals a later word before its playback position", () => {
    expect(captionAtPosition(edge, 250)).toBe("Hello");
    expect(captionAtPosition(edge, 300)).toBe("Hello w");
  });

  it("keeps the last known clause through a streaming gap", () => {
    expect(captionAtPosition(edge, 650)).toBe("Hello world");
    expect(captionAtPosition(edge, 900)).toBe("Again");
  });

  it("uses a clause for estimated local timing", () => {
    expect(captionAtPosition([{ ...edge[0], estimated: true }], 0)).toBe("Hello world");
  });

  it("shows the final complete line at playback completion", () => {
    expect(captionAtPosition(edge, 900, true)).toBe("Again");
  });
});

it("rejects duplicate, late, and cancelled playback frames before showing a successor", () => {
  const paints: FrameRequestCallback[] = [];
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    paints.push(callback);
    return paints.length;
  });
  vi.stubGlobal("cancelAnimationFrame", () => {});
  const shown: Array<string | null> = [];
  const unsubscribe = onSpeechStageLine((line) => shown.push(line?.text ?? null));
  const track = {
    words: edge[0].words,
    total_ms: 500,
    estimated: false,
    envelope: [0.5],
    frame_ms: 500,
    envelope_start_ms: 0,
  };
  const frame = (utterance_id: number, seq: number, phase: SpeechFrame["phase"], position_ms: number, caption: SpeechFrame["caption"] = null): SpeechFrame => ({
    utterance_id, seq, phase, position_ms, caption,
    emitted_at_unix_ms: Date.now(), request_id: "r1", source_center: "main", kind: "result",
  });
  const paint = () => paints.shift()?.(performance.now());
  try {
    acceptSpeechFrame(frame(999, 1, "prepared", 0, track));
    paint();
    acceptSpeechFrame(frame(999, 2, "playing", 300));
    paint();
    expect(shown.at(-1)).toBe("Hello w");
    acceptSpeechFrame(frame(999, 2, "playing", 500));
    paint();
    expect(shown.at(-1)).toBe("Hello w");
    acceptSpeechFrame(frame(999, 3, "cancelled", 300));
    paint();
    expect(shown.at(-1)).toBeNull();
    acceptSpeechFrame(frame(999, 4, "playing", 500));
    expect(shown.at(-1)).toBeNull();
    acceptSpeechFrame(frame(1000, 5, "prepared", 0, track));
    paint();
    expect(shown.at(-1)).toBe("…");
  } finally {
    clearSpeechStage();
    unsubscribe();
    vi.unstubAllGlobals();
  }
});

it("does not revive an old finished caption when the stage restarts", () => {
  const restored: SpeechFrame = {
    utterance_id: 1001, seq: 6, phase: "finished", position_ms: 500,
    emitted_at_unix_ms: Date.now() - 5000, request_id: "r2",
    source_center: "main", kind: "result", caption: null,
  };
  acceptSpeechFrame(restored, [{
    words: edge[0].words, total_ms: 500, estimated: false,
    envelope: [], frame_ms: 20, envelope_start_ms: 0,
  }]);
  expect(speechStageActive()).toBe(false);
});
