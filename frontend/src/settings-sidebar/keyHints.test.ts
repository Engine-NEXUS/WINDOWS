import { describe, expect, it } from "vitest";
import { groqKeyHint } from "./keyHints";

describe("groqKeyHint", () => {
  it("asks for a key when there is none (or only whitespace)", () => {
    expect(groqKeyHint("", true)).toBe("get-key");
    expect(groqKeyHint("   ", false)).toBe("get-key");
    expect(groqKeyHint(undefined, undefined)).toBe("get-key");
  });

  it("warns that a saved key is idle while Local STT only is on", () => {
    expect(groqKeyHint("gsk_abc", true)).toBe("local-only-blocks");
  });

  it("confirms cloud speech once local-only is off", () => {
    expect(groqKeyHint("gsk_abc", false)).toBe("cloud-active");
    expect(groqKeyHint(" gsk_abc ", false)).toBe("cloud-active");
  });
});
