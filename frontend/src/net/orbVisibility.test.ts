import { describe, expect, it } from "vitest";
import { decideAfterSpeech, HAND_OVER_TO_SPINNER, type AfterSpeechInputs } from "./orbVisibility";

const base: AfterSpeechInputs = {
  ghostActive: false,
  state: "idle",
  loadingVisible: false,
  audioPlaying: false,
  captionActive: false,
  awaitingInput: false,
  pendingGithubCommand: false,
};
const at = (o: Partial<AfterSpeechInputs>) => decideAfterSpeech({ ...base, ...o });

describe("orb after a spoken line", () => {
  it("never touches the orb in a ghost session", () => {
    expect(at({ ghostActive: true, state: "speaking" })).toBe("keep");
    expect(at({ ghostActive: true })).toBe("keep");
  });

  it("waits while audio is still playing", () => {
    expect(at({ state: "speaking", audioPlaying: true })).toBe("wait");
    expect(at({ state: "speaking", audioPlaying: true, loadingVisible: true })).toBe("wait");
  });

  it("after an acknowledgement with work pending, the orb stays and returns to the thinking pose", () => {
    expect(HAND_OVER_TO_SPINNER).toBe(false);
    expect(at({ state: "speaking", loadingVisible: true })).toBe("settle_thinking");
  });

  it("a silent speaking orb with nothing pending counts toward the stuck limit", () => {
    expect(at({ state: "speaking" })).toBe("stuck_tick");
  });

  it("an idle orb with nothing on screen hides", () => {
    expect(at({})).toBe("hide");
  });

  it("an idle orb with a caption still showing waits", () => {
    expect(at({ captionActive: true })).toBe("wait");
  });

  it("an orb waiting for the owner's answer or a confirmation is not hidden", () => {
    expect(at({ awaitingInput: true })).toBe("keep");
    expect(at({ pendingGithubCommand: true })).toBe("keep");
  });

  it("thinking and listening are left to their own watchdogs", () => {
    expect(at({ state: "thinking" })).toBe("keep");
    expect(at({ state: "listening" })).toBe("keep");
  });
});
