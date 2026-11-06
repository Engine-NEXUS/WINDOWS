import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SpeechPill } from "./SpeechPill";

describe("SpeechPill component", () => {
  it("renders null initially when rect is not loaded", () => {
    const html = renderToStaticMarkup(<SpeechPill />);
    expect(html).toBe("");
  });
});
