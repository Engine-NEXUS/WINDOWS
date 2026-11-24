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
  TYPE_MAX_MS,
  typedSlice,
  typingProgress,
  typingStart,
  partialStableChars,
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

describe("letter-by-letter typing", () => {
  it("typingProgress starts at the base, ends at the total and never goes backwards", () => {
    expect(typingProgress(0, 20, 0)).toBe(0);
    expect(typingProgress(0, 20, 100000)).toBe(20);
    expect(typingProgress(5, 20, 0)).toBe(5);
    expect(typingProgress(0, 0, 500)).toBe(0);
    let prev = 0;
    for (let t = 0; t <= 2000; t += 16) {
      const v = typingProgress(0, 40, t);
      expect(v).toBeGreaterThanOrEqual(prev);
      prev = v;
    }
  });

  it("short text types at about 55 characters a second", () => {
    // 22 chars take 400 ms; at 200 ms half of them are out
    expect(typingProgress(0, 22, 200)).toBeCloseTo(11, 5);
    expect(typingProgress(0, 22, 400)).toBeCloseTo(22, 5);
  });

  it("a long text finishes within the maximum time", () => {
    expect(typingProgress(0, 500, TYPE_MAX_MS)).toBe(500);
    expect(typingProgress(0, 500, TYPE_MAX_MS / 2)).toBeCloseTo(250, 5);
  });

  it("an extended text continues from the base instead of restarting", () => {
    expect(typingProgress(10, 30, 0)).toBe(10);
    expect(typingProgress(10, 30, 100000)).toBe(30);
    expect(typingProgress(40, 30, 0)).toBe(30); // target shorter than what was typed: show in full
  });

  it("typedSlice shows at least the first letter, no trailing space, and the whole text at the end", () => {
    expect(typedSlice("hello world", 0)).toBe("h");
    expect(typedSlice("hello world", 5.9)).toBe("hello");
    expect(typedSlice("hello world", 6)).toBe("hello");
    expect(typedSlice("hello world", 7)).toBe("hello w");
    expect(typedSlice("hello world", 99)).toBe("hello world");
    expect(typedSlice("", 3)).toBe("");
  });

  it("types one letter at a time at frame rate (no word bursts)", () => {
    const text = "turn on the kitchen ceiling lights";
    let prev = 0;
    for (let t = 0; t <= 1200; t += 16) {
      const visible = typedSlice(text, typingProgress(0, text.length, t)).replace(/\s/g, "").length;
      expect(visible - prev).toBeLessThanOrEqual(2);
      prev = visible;
    }
  });
});

describe("typingStart: what stays on screen when the caption text changes", () => {
  it("a text that grows keeps everything already shown", () => {
    expect(typingStart("open what", "open whatsapp and")).toBe("open what".length); // a true start of the new text stays
    expect(typingStart("open whatsapp ", "open whatsapp and")).toBe("open whatsapp ".length);
    expect(typingStart("open", "open whatsapp")).toBe(4);
  });
  it("the sliding window keeps the words it still shares (no sideways jump)", () => {
    expect(typingStart("open whatsapp and send a", "and send a message to")).toBe("and send a".length);
    expect(typingStart("a b c d e", "d e f g h")).toBe("d e".length);
  });
  it("a revised tail keeps the start and types the revision", () => {
    expect(typingStart("open cervix", "open servx")).toBe("open ".length);
    expect(typingStart("search for pizza", "search for peace")).toBe("search for ".length);
  });
  it("nothing shared, or nothing on screen, starts from zero", () => {
    expect(typingStart("", "hello there")).toBe(0);
    expect(typingStart("hello", "")).toBe(0);
    expect(typingStart("alpha beta", "gamma delta")).toBe(0);
  });
  it("never more than the new text, and a shorter new text that is a start of the old one shows in full", () => {
    expect(typingStart("open whatsapp and send", "open whatsapp")).toBe("open whatsapp".length);
    for (const [a, b] of [["x y z", "x"], ["x y z", "y z q"], ["one two", "one two three four"]]) {
      expect(typingStart(a, b)).toBeLessThanOrEqual(b.length);
    }
  });
});

describe("partialStableChars: how much of the shown caption is confirmed", () => {
  it("a payload without the field, or with every word confirmed, is fully confirmed", () => {
    expect(partialStableChars("open the hub")).toBe("open the hub".length);
    expect(partialStableChars({ text: "open the hub" })).toBe("open the hub".length);
    expect(partialStableChars({ text: "open the hub", stable_words: 3 })).toBe("open the hub".length);
    expect(partialStableChars({ text: "open the hub", stable_words: 9 })).toBe("open the hub".length);
  });
  it("the words after the confirmed ones are the unconfirmed tail", () => {
    expect(partialStableChars({ text: "search for the best pizza", stable_words: 3 })).toBe("search for the ".length);
    expect(partialStableChars({ text: "search for the best pizza", stable_words: 0 })).toBe(0);
    expect(partialStableChars({ text: "search for the best pizza", stable_words: 4 })).toBe("search for the best ".length);
  });
  it("counts from the start of the whole text even though only the last five words are shown", () => {
    // 8 words, 6 confirmed: the window shows words 4..8 (5 words); word 4 and 5 are confirmed in it, 6 is the first guess...
    const text = "one two three four five six seven eight";
    // window "four five six seven eight"; confirmed = 6 of 8, so 3 of the 5 shown (four five six)
    expect(partialStableChars({ text, stable_words: 6 })).toBe("four five six ".length);
    // everything shown is confirmed once the confirmed count reaches the end of the text
    expect(partialStableChars({ text, stable_words: 8 })).toBe("four five six seven eight".length);
    // the confirmed part is entirely off the left edge: all five shown words are guesses
    expect(partialStableChars({ text, stable_words: 2 })).toBe(0);
  });
  it("never goes past the displayed text", () => {
    for (const sw of [-3, 0, 1, 2, 5, 7, 8, 100]) {
      const n = partialStableChars({ text: "a b c d e f g", stable_words: sw });
      expect(n).toBeGreaterThanOrEqual(0);
      expect(n).toBeLessThanOrEqual("c d e f g".length);
    }
  });
});
