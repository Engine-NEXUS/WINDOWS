import { describe, expect, it } from "vitest";
import { applySetupKeys } from "./setupKeys";

describe("applySetupKeys", () => {
  const base = { ttsVolume: 75, localSttOnly: true, groqApiKey: "", geminiApiKey: "" };

  it("changes nothing when both fields are empty", () => {
    expect(applySetupKeys(base, { groq: "", gemini: "  " })).toEqual(base);
  });

  it("never overwrites an already-saved key with an empty field", () => {
    const saved = { ...base, groqApiKey: "gsk_old", geminiApiKey: "AIza_old" };
    expect(applySetupKeys(saved, { groq: "", gemini: "" })).toEqual(saved);
  });

  it("trims keys and switches cloud speech on when a Groq key is given", () => {
    const out = applySetupKeys(base, { groq: "  gsk_new \n", gemini: "" });
    expect(out.groqApiKey).toBe("gsk_new");
    expect(out.localSttOnly).toBe(false);
    expect(out.geminiApiKey).toBe("");
  });

  it("a Gemini key alone keeps speech local", () => {
    const out = applySetupKeys(base, { groq: "", gemini: " AIzaX " });
    expect(out.geminiApiKey).toBe("AIzaX");
    expect(out.localSttOnly).toBe(true);
  });

  it("does not mutate its input", () => {
    const copy = { ...base };
    applySetupKeys(base, { groq: "gsk", gemini: "AIza" });
    expect(base).toEqual(copy);
  });
});
