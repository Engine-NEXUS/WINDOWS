import { describe, expect, it } from "vitest";
import {
  addOutcomeMessage,
  emptyOverview,
  isValidFactInput,
  normalizeOverview,
  totalProfileEntries,
} from "./memoryModel";

describe("normalizeOverview", () => {
  it("fills every field from an empty / missing payload", () => {
    expect(normalizeOverview(undefined)).toEqual(emptyOverview());
    expect(normalizeOverview({})).toEqual(emptyOverview());
    expect(normalizeOverview("garbage")).toEqual(emptyOverview());
  });

  it("maps the backend shape (snake_case conversation_turns, entries, facts, habits)", () => {
    const o = normalizeOverview({
      profile: {
        identity: [{ text: "I am a student", ts: 5, sensitive: false }],
        priorities: [{ text: "ship it", ts: 9, sensitive: true }],
      },
      facts: [{ key: "city", value: "Delhi", sensitive: false }],
      habits: {
        top: [{ name: "Chrome", count: 7 }],
        suggestions: [{ key: "app:chrome", text: "Make progress in Chrome", reason: "You've opened Chrome 7 times." }],
      },
      conversation_turns: 12,
    });
    expect(o.profile.identity[0]).toEqual({ text: "I am a student", ts: 5, sensitive: false });
    expect(o.profile.priorities[0].sensitive).toBe(true);
    expect(o.profile.needs).toEqual([]);
    expect(o.facts).toEqual([{ key: "city", value: "Delhi", sensitive: false }]);
    expect(o.habits.top[0]).toEqual({ name: "Chrome", count: 7 });
    expect(o.habits.suggestions[0].key).toBe("app:chrome");
    expect(o.conversationTurns).toBe(12);
    expect(totalProfileEntries(o)).toBe(2);
  });

  it("drops malformed rows instead of throwing", () => {
    const o = normalizeOverview({
      profile: { identity: [null, 3, { text: "" }, { text: "ok" }] },
      facts: [{ key: "" }, { key: "k", value: 1 }],
      habits: { top: [{ name: "", count: 2 }], suggestions: [{ key: "", text: "x" }] },
    });
    expect(o.profile.identity.map((e) => e.text)).toEqual(["ok"]);
    expect(o.facts).toEqual([{ key: "k", value: "", sensitive: false }]);
    expect(o.habits.top).toEqual([]);
    expect(o.habits.suggestions).toEqual([]);
  });
});

describe("copy + validation", () => {
  it("explains each add outcome, flagging private entries", () => {
    expect(addOutcomeMessage("added")).toBe("Saved.");
    expect(addOutcomeMessage("added_private")).toMatch(/never sent/);
    expect(addOutcomeMessage("duplicate")).toMatch(/Already/);
    expect(addOutcomeMessage("weird")).toMatch(/Couldn't/);
  });

  it("requires both a fact name and a value", () => {
    expect(isValidFactInput("city", "Delhi")).toBe(true);
    expect(isValidFactInput(" ", "Delhi")).toBe(false);
    expect(isValidFactInput("city", "  ")).toBe(false);
  });
});
