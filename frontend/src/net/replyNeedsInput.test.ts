import { describe, expect, it } from "vitest";
import { lastSentence, replyNeedsInput } from "./replyNeedsInput";

describe("replyNeedsInput", () => {
  it("a statement or explanation does not need input", () => {
    for (const t of [
      "Two plus two equals four.",
      "Lakshya, the Moon is roughly 384,400 kilometers from Earth and is the only other celestial body humans have set foot on.",
      "Claude Code has finished, sir.",
      "Opening the command hub, sir.",
      "Done.",
      "",
    ]) expect(replyNeedsInput(t), t).toBe(false);
    expect(replyNeedsInput(undefined)).toBe(false);
    expect(replyNeedsInput(null)).toBe(false);
  });

  it("a question at the end needs input", () => {
    for (const t of [
      "Bring back what you had open last night, sir?",
      "I found two Johns. Which one do you mean?",
      "Shall I start?",
      "Shall I open everything from earlier today?",
      "I can send it now. Do you want me to send it",
      "Would you like me to read it out.",
      "Say yes or no.",
    ]) expect(replyNeedsInput(t), t).toBe(true);
  });

  it("a question that the reply goes on to answer does not need input", () => {
    expect(replyNeedsInput("You asked how far the Moon is? It is about 384,400 kilometers.")).toBe(false);
    expect(replyNeedsInput("What is the capital of France? Paris.")).toBe(false);
  });

  it("looks at the last sentence, ignoring closing quotes and spaces", () => {
    expect(lastSentence('He said "Go now." Shall I go?"  ')).toBe("Shall I go?");
    expect(replyNeedsInput('Okay. Which one do you want?"  ')).toBe(true);
  });
});
