import { describe, expect, it } from "vitest";
import { canDownload, parseSpeechModelStatus, speechModelLine } from "./speechModelModel";

describe("speech model status", () => {
  it("reads what the backend sends and refuses anything else", () => {
    expect(parseSpeechModelStatus({ state: "ready", downloaded_mb: 244, expected_mb: 245 })).toEqual({ state: "ready", downloaded_mb: 244, expected_mb: 245 });
    expect(parseSpeechModelStatus({ state: "weird" })).toBeNull();
    expect(parseSpeechModelStatus(null)).toBeNull();
    expect(parseSpeechModelStatus("ready")).toBeNull();
    expect(parseSpeechModelStatus({ state: "missing", downloaded_mb: -4, expected_mb: 0 })).toEqual({ state: "missing", downloaded_mb: 0, expected_mb: 245 });
  });
  it("says each state in plain words, with a progress figure that never reaches 100 before it is ready", () => {
    expect(speechModelLine(null)).toMatch(/Checking/);
    expect(speechModelLine({ state: "ready", downloaded_mb: 245, expected_mb: 245 })).toMatch(/Downloaded/);
    expect(speechModelLine({ state: "missing", downloaded_mb: 0, expected_mb: 245 })).toMatch(/Not downloaded/);
    const d = speechModelLine({ state: "downloading", downloaded_mb: 140, expected_mb: 245 });
    expect(d).toMatch(/140 of about 245 MB \(57%\)/);
    expect(speechModelLine({ state: "downloading", downloaded_mb: 245, expected_mb: 245 })).toMatch(/\(99%\)/);
    expect(speechModelLine({ state: "unknown", downloaded_mb: 0, expected_mb: 245 })).toMatch(/cannot check/);
  });
  it("offers the download only when the model is missing", () => {
    expect(canDownload({ state: "missing", downloaded_mb: 0, expected_mb: 245 })).toBe(true);
    for (const state of ["ready", "downloading", "unknown"] as const) expect(canDownload({ state, downloaded_mb: 0, expected_mb: 245 })).toBe(false);
    expect(canDownload(null)).toBe(false);
  });
});
