import { describe, expect, it } from "vitest";
import { charSchedule, decodeEntities, lineSpan, lineText, typedLength, typedText, type TimedWord } from "./typedCaption";

const words: TimedWord[] = [
  { text: "Hello", start_ms: 1000, duration_ms: 200 },
  { text: "there", start_ms: 1300, duration_ms: 200 },
  { text: "friend", start_ms: 1900, duration_ms: 400 },
];

describe("lineText / lineSpan", () => {
  it("joins decoded words with single spaces", () => {
    expect(lineText(words)).toBe("Hello there friend");
    expect(lineText([{ text: "don&apos;t", start_ms: 0, duration_ms: 10 }, { text: "&amp;", start_ms: 10, duration_ms: 10 }])).toBe("don't &");
    expect(decodeEntities("&lt;b&gt; &quot;x&quot;")).toBe('<b> "x"');
  });
  it("spans first word start to last word end and is never zero length", () => {
    expect(lineSpan(words)).toEqual({ start: 1000, end: 2300 });
    expect(lineSpan([{ text: "a", start_ms: 5, duration_ms: 0 }]).end).toBeGreaterThan(5);
    expect(lineSpan([])).toEqual({ start: 0, end: 1 });
  });
});

describe("typedLength (steady rate)", () => {
  it("types one letter at the start, everything at the end, and clamps outside", () => {
    expect(typedLength(18, 1000, 2300, 1000)).toBe(1);
    expect(typedLength(18, 1000, 2300, 2300)).toBe(18);
    expect(typedLength(18, 1000, 2300, 5000)).toBe(18);
    expect(typedLength(18, 1000, 2300, -50)).toBe(1);
    expect(typedLength(0, 0, 100, 50)).toBe(0);
    expect(typedLength(1, 0, 100, 50)).toBe(1);
  });

  it("grows by about the same number of letters every equal time step (no word bursts)", () => {
    const total = 60;
    const steps: number[] = [];
    let prev = typedLength(total, 0, 3000, 0);
    for (let t = 100; t <= 3000; t += 100) {
      const now = typedLength(total, 0, 3000, t);
      steps.push(now - prev);
      prev = now;
    }
    expect(Math.max(...steps) - Math.min(...steps)).toBeLessThanOrEqual(1);
    expect(Math.max(...steps)).toBeLessThanOrEqual(3);
  });

  it("never goes backwards", () => {
    let prev = 0;
    for (let t = 0; t <= 3000; t += 7) {
      const now = typedLength(33, 100, 2900, t);
      expect(now).toBeGreaterThanOrEqual(prev);
      prev = now;
    }
  });
});

describe("typedText", () => {
  it("is a prefix of the line that only ever grows and never ends in a space", () => {
    const full = lineText(words);
    let prev = "";
    for (let t = 900; t <= 2400; t += 25) {
      const text = typedText(words, t);
      expect(full.startsWith(text)).toBe(true);
      expect(text.startsWith(prev)).toBe(true);
      expect(text.endsWith(" ")).toBe(false);
      prev = text;
    }
    expect(prev).toBe(full);
  });

  it("does not jump a whole word at a word boundary", () => {
    // 'there' starts at 1300; the typed text 10 ms before and after differs by at most 1 letter
    const before = typedText(words, 1290).length;
    const after = typedText(words, 1310).length;
    expect(after - before).toBeLessThanOrEqual(1);
  });

  it("works for evenly estimated local-engine timing too", () => {
    const est: TimedWord[] = [
      { text: "Hello", start_ms: 0, duration_ms: 250 },
      { text: "world", start_ms: 250, duration_ms: 250 },
    ];
    expect(typedText(est, 0)).toBe("H");
    expect(typedText(est, 250)).toBe("Hello");
    expect(typedText(est, 500)).toBe("Hello world");
  });

  it("is empty for an empty line", () => {
    expect(typedText([], 100)).toBe("");
  });
});

describe("charSchedule", () => {
  it("has one entry per character, strictly non-decreasing in time, from the line start to its end", () => {
    const s = charSchedule(words);
    expect(s).toHaveLength(lineText(words).length);
    expect(s[0].atMs).toBe(1000);
    expect(s[s.length - 1].atMs).toBe(2300);
    for (let i = 1; i < s.length; i++) expect(s[i].atMs).toBeGreaterThanOrEqual(s[i - 1].atMs);
  });

  it("agrees with typedText at every scheduled instant", () => {
    for (const { atMs, text } of charSchedule(words)) {
      expect(typedText(words, atMs).length).toBeGreaterThanOrEqual(text.length);
    }
  });

  it("is empty for no words and one entry for one letter", () => {
    expect(charSchedule([])).toEqual([]);
    expect(charSchedule([{ text: "a", start_ms: 7, duration_ms: 10 }])).toEqual([{ atMs: 7, text: "a" }]);
  });
});
