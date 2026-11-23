import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { SpeechPill } from "./SpeechPill";

describe("SpeechPill component", () => {
  it("renders null initially when rect is not loaded", () => {
    const html = renderToStaticMarkup(<SpeechPill />);
    expect(html).toBe("");
  });
});

describe("SpeechPill live captions", () => {
  // A partial with no final after it (cancelled turn, dead stream) used to leave the pill up for ever (2026-11-24 audit).
  it("hides a partial that is never followed by a final", async () => {
    const { readFileSync } = await import("node:fs");
    const src = readFileSync(new URL("./SpeechPill.tsx", import.meta.url), "utf8");
    const partial = src.split('listen<unknown>("stt:partial"')[1].split('listen<unknown>("stt:transcript"')[0];
    expect(partial).toContain("setTimeout");
    expect(partial).toContain("PARTIAL_STALE_MS");
    expect(src).toMatch(/PARTIAL_STALE_MS = \d+/);
  });

  it("no longer streams from the browser recorder (the native capture feeds the stream in Rust)", async () => {
    const { readFileSync } = await import("node:fs");
    const rec = readFileSync(new URL("../audio/recorder.ts", import.meta.url), "utf8");
    expect(rec).not.toContain("stt_stream_");
  });
});
