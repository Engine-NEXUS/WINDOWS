import { describe, expect, it } from "vitest";
import {
  formatFinalText,
  formatPartialText,
  handleCancel,
  handleFinal,
  handlePartial,
  handleReplyStart,
  handleTick,
  isFinalExpired,
  shouldShowPill,
  SpeechPillModel,
  FINAL_LINGER_MS,
  INITIAL_SPEECH_PILL_STATE,
} from "./speechPillModel";

describe("speechPillModel", () => {
  describe("partial window table tests", () => {
    const table: Array<[string, unknown, string]> = [
      ["single word string", "hello", "hello"],
      ["3 words string", "hello world there", "hello world there"],
      ["exactly 5 words string", "one two three four five", "one two three four five"],
      ["6 words slides window to last 5", "one two three four five six", "two three four five six"],
      [
        "9 words slides window to last 5",
        "the quick brown fox jumps over the lazy dog",
        "jumps over the lazy dog",
      ],
      ["object payload with text property", { text: "alpha beta gamma delta epsilon zeta" }, "beta gamma delta epsilon zeta"],
      ["whitespace padding trimmed", "   word1   word2 word3 word4 word5 word6   ", "word2 word3 word4 word5 word6"],
      ["empty string returns empty", "", ""],
      ["whitespace only returns empty", "    ", ""],
      ["null payload returns empty", null, ""],
    ];

    it.each(table)("%s", (_name, input, expected) => {
      expect(formatPartialText(input)).toBe(expected);
      const state = handlePartial(INITIAL_SPEECH_PILL_STATE, input);
      if (expected) {
        expect(state.visible).toBe(true);
        expect(state.phase).toBe("partial");
        expect(state.text).toBe(expected);
      } else {
        expect(state.visible).toBe(false);
        expect(state.phase).toBe("idle");
        expect(state.text).toBe("");
      }
    });
  });

  describe("final text table tests", () => {
    const table: Array<[string, unknown, string]> = [
      ["short command string", "open chrome", "open chrome"],
      ["object with text", { text: "what is the time" }, "what is the time"],
      [
        "object with session, text and raw_text",
        { session: "s_123", text: "remind me to call mom", raw_text: "remind me to call mom." },
        "remind me to call mom",
      ],
      ["multiple spaces collapsed", { text: "turn   on   the    lights" }, "turn on the lights"],
    ];

    it.each(table)("%s", (_name, input, expected) => {
      expect(formatFinalText(input)).toBe(expected);
      const state = handleFinal(INITIAL_SPEECH_PILL_STATE, input, 1000);
      expect(state.visible).toBe(true);
      expect(state.phase).toBe("final");
      expect(state.finalTimestamp).toBe(1000);
      expect(state.text).toBe(expected);
    });
  });

  describe("empty hides table tests", () => {
    const emptyInputs: Array<[string, unknown]> = [
      ["empty string", ""],
      ["whitespace only", "   \t\n  "],
      ["object with empty text", { text: "" }],
      ["object with whitespace text", { text: "   " }],
      ["object without text", { session: "phantom_turn" }],
      ["null", null],
      ["undefined", undefined],
      ["number", 12345],
    ];

    it.each(emptyInputs)("empty transcript hides immediately: %s", (_name, input) => {
      expect(formatFinalText(input)).toBe("");

      // Given an already visible state (e.g. from prior partial speech)
      const priorState = {
        text: "partial words",
        visible: true,
        phase: "partial" as const,
        finalTimestamp: null,
      };

      const next = handleFinal(priorState, input, 1000);
      expect(next.visible).toBe(false);
      expect(next.phase).toBe("idle");
      expect(next.text).toBe("");
      expect(shouldShowPill(next, false, 1000)).toBe(false);
    });
  });

  describe("long text trimmed table tests", () => {
    const table: Array<[string, string, string]> = [
      [
        "exactly 12 words kept without ellipsis",
        "one two three four five six seven eight nine ten eleven twelve",
        "one two three four five six seven eight nine ten eleven twelve",
      ],
      [
        "13 words trimmed to 12 with leading ellipsis",
        "zero one two three four five six seven eight nine ten eleven twelve",
        "… one two three four five six seven eight nine ten eleven twelve",
      ],
      [
        "16 words trimmed to 12 with leading ellipsis",
        "alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa",
        "… echo foxtrot golf hotel india juliet kilo lima mike november oscar papa",
      ],
      [
        "sentence with punctuation trimmed properly",
        "hey nexus please search for the latest updates on the space station launch tomorrow",
        "… please search for the latest updates on the space station launch tomorrow",
      ],
    ];

    it.each(table)("%s", (_name, input, expected) => {
      expect(formatFinalText(input)).toBe(expected);
      const state = handleFinal(INITIAL_SPEECH_PILL_STATE, input, 500);
      expect(state.text).toBe(expected);
    });
  });

  describe("reply start hides", () => {
    it("hides when assistant starts reply while partial text was showing", () => {
      const partialState = handlePartial(INITIAL_SPEECH_PILL_STATE, "turn on the lights");
      expect(partialState.visible).toBe(true);

      const afterReply = handleReplyStart(partialState);
      expect(afterReply.visible).toBe(false);
      expect(afterReply.phase).toBe("idle");
      expect(shouldShowPill(afterReply, false, 1000)).toBe(false);
    });

    it("hides when assistant starts reply while final text was lingering", () => {
      const finalState = handleFinal(INITIAL_SPEECH_PILL_STATE, "turn on the lights", 1000);
      expect(finalState.visible).toBe(true);

      const afterReply = handleReplyStart(finalState);
      expect(afterReply.visible).toBe(false);
      expect(afterReply.phase).toBe("idle");
      expect(shouldShowPill(afterReply, false, 2000)).toBe(false);
    });

    it("shouldShowPill yields to response caption even if pill state is technically visible", () => {
      const finalState = handleFinal(INITIAL_SPEECH_PILL_STATE, "good morning", 1000);
      expect(shouldShowPill(finalState, false, 1500)).toBe(true);
      // Response caption is active: response caption wins
      expect(shouldShowPill(finalState, true, 1500)).toBe(false);
    });

    it("hides on cancel", () => {
      const state = handleFinal(INITIAL_SPEECH_PILL_STATE, "hello", 1000);
      expect(state.visible).toBe(true);
      const cancelled = handleCancel(state);
      expect(cancelled.visible).toBe(false);
      expect(cancelled.text).toBe("");
      expect(shouldShowPill(cancelled, false, 1000)).toBe(false);
    });
  });

  describe("4 s expiry with injected clock", () => {
    it("keeps final text visible before 4000ms and hides at/after 4000ms", () => {
      let currentTime = 10_000;
      const model = new SpeechPillModel(() => currentTime);

      // User completes utterance at t = 10,000
      model.onFinal({ text: "what is the speed of light" });

      expect(model.getText()).toBe("what is the speed of light");
      expect(model.isVisible(false)).toBe(true);

      // t = 11,000 (1s elapsed) -> visible
      currentTime = 11_000;
      expect(model.isVisible(false)).toBe(true);
      expect(isFinalExpired(model.getState(), currentTime)).toBe(false);

      // t = 13,999 (3.999s elapsed) -> visible
      currentTime = 13_999;
      expect(model.isVisible(false)).toBe(true);
      expect(isFinalExpired(model.getState(), currentTime)).toBe(false);

      // t = 14,000 (exactly 4s elapsed) -> expired
      currentTime = 14_000;
      expect(isFinalExpired(model.getState(), currentTime)).toBe(true);
      expect(model.isVisible(false)).toBe(false);

      // Calling tick() transitions state to hidden
      const ticked = model.tick();
      expect(ticked.visible).toBe(false);
      expect(ticked.phase).toBe("idle");
      expect(model.isVisible(false)).toBe(false);
    });

    it("table test: final linger expiry boundaries", () => {
      const baseTime = 5000;
      const state = handleFinal(INITIAL_SPEECH_PILL_STATE, "check calendar", baseTime);

      const table: Array<[number, boolean]> = [
        [baseTime + 0, true],
        [baseTime + 1000, true],
        [baseTime + 3999, true],
        [baseTime + FINAL_LINGER_MS, false],
        [baseTime + FINAL_LINGER_MS + 100, false],
      ];

      for (const [t, expectedVisible] of table) {
        expect(shouldShowPill(state, false, t)).toBe(expectedVisible);
        expect(isFinalExpired(state, t)).toBe(!expectedVisible);
        const afterTick = handleTick(state, t);
        expect(afterTick.visible).toBe(expectedVisible);
      }
    });

    it("new partial cancels lingering timer state and starts fresh", () => {
      let now = 1000;
      const model = new SpeechPillModel(() => now);

      model.onFinal("turn off bedroom lights");
      expect(model.isVisible(false)).toBe(true);

      now = 2000;
      model.onPartial("and also");
      expect(model.getText()).toBe("and also");
      expect(model.getState().phase).toBe("partial");
      expect(model.getState().finalTimestamp).toBeNull();

      // Advancing past 4s of the old final timestamp (e.g. now = 7000) does not expire partial
      now = 7000;
      expect(model.isVisible(false)).toBe(true);
      expect(model.tick().visible).toBe(true);
    });
  });
});
