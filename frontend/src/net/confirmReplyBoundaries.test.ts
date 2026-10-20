import { describe, expect, it } from "vitest";
import {
  classifyConfirmReply,
  isConfirmExpired,
  PENDING_CONFIRM_TTL_MS,
} from "./confirmReply";

describe("classifyConfirmReply boundaries", () => {
  it("strips leading 'nexus' wake word and classifies remaining words", () => {
    expect(classifyConfirmReply("nexus yes")).toBe("yes");
    expect(classifyConfirmReply("Nexus, no")).toBe("no");
    expect(classifyConfirmReply("NEXUS please confirm")).toBe("yes");
    expect(classifyConfirmReply("nexus")).toBe("other");
  });

  it("enforces MAX_WORDS length boundary (6 words max)", () => {
    // 6 words: "yes please do that right now" -> valid yes
    expect(classifyConfirmReply("yes please do that right now")).toBe("yes");

    // 7 words -> rejected as other
    expect(classifyConfirmReply("yes please do that right now sir")).toBe("other");
  });

  it("ensures refusal words strictly override affirmation words", () => {
    expect(classifyConfirmReply("yes no")).toBe("no");
    // "but" is not in the answer vocabulary; strict matching rejects any utterance with unknown words as "other"
    expect(classifyConfirmReply("sure but don't")).toBe("other");
    // When words are strictly within the answer vocabulary, refusal overrides affirmation
    expect(classifyConfirmReply("sure don't")).toBe("no");
    expect(classifyConfirmReply("okay cancel")).toBe("no");
    expect(classifyConfirmReply("proceed stop")).toBe("no");
  });

  it("rejects utterances that contain unknown non-filler words", () => {
    expect(classifyConfirmReply("yes apple")).toBe("other");
    expect(classifyConfirmReply("okay delete everything")).toBe("other");
    expect(classifyConfirmReply("sure open chrome")).toBe("other");
  });

  it("rejects purely polite fillers that contain neither yes nor no", () => {
    expect(classifyConfirmReply("please sir")).toBe("other");
    expect(classifyConfirmReply("thank you")).toBe("other");
    expect(classifyConfirmReply("nexus kindly")).toBe("other");
  });

  it("handles empty strings, punctuation, and foreign language words", () => {
    expect(classifyConfirmReply("")).toBe("other");
    expect(classifyConfirmReply("    \t\n   ")).toBe("other");
    expect(classifyConfirmReply("!@#$%^&*()")).toBe("other");
    expect(classifyConfirmReply("yes!!!")).toBe("yes");
    expect(classifyConfirmReply("no???")).toBe("no");
    // Non-English affirmations not in vocabulary
    expect(classifyConfirmReply("oui")).toBe("other");
    expect(classifyConfirmReply("si")).toBe("other");
    expect(classifyConfirmReply("ja")).toBe("other");
  });
});

describe("isConfirmExpired boundaries", () => {
  it("never expires when timestamp is null", () => {
    expect(isConfirmExpired(null, 999_999_999)).toBe(false);
  });

  it("evaluates exact TTL boundary (strictly greater than)", () => {
    const at = 10_000;
    // Exactly at TTL: not expired
    expect(isConfirmExpired(at, at + PENDING_CONFIRM_TTL_MS)).toBe(false);

    // 1ms after TTL: expired
    expect(isConfirmExpired(at, at + PENDING_CONFIRM_TTL_MS + 1)).toBe(true);
  });

  it("supports custom TTL thresholds", () => {
    const at = 5_000;
    const customTtl = 3_000;
    expect(isConfirmExpired(at, at + 2_999, customTtl)).toBe(false);
    expect(isConfirmExpired(at, at + 3_000, customTtl)).toBe(false);
    expect(isConfirmExpired(at, at + 3_001, customTtl)).toBe(true);
  });
});
