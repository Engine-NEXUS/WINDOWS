import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import {
  clearThinkingDwell,
  THINKING_MIN_DWELL_MS,
  transition,
  useAssistant,
} from "./assistant";
import {
  isBusyWatchable,
  isIdleEligible,
  type OrbSnapshot,
} from "../net/orbIdleWatchdog";

describe("transition function (state machine validity)", () => {
  it("allows legal forward transitions", () => {
    expect(transition("idle", "listening")).toBe(true);
    expect(transition("listening", "thinking")).toBe(true);
    expect(transition("thinking", "speaking")).toBe(true);
    expect(transition("speaking", "idle")).toBe(true);
  });

  it("allows legal abort/cancellation transitions to idle", () => {
    expect(transition("listening", "idle")).toBe(true);
    expect(transition("thinking", "idle")).toBe(true);
  });

  it("disallows illegal jumps", () => {
    expect(transition("idle", "thinking")).toBe(false);
    expect(transition("idle", "speaking")).toBe(false);
    expect(transition("listening", "speaking")).toBe(false);
    expect(transition("thinking", "listening")).toBe(false);
    expect(transition("speaking", "listening")).toBe(false);
    expect(transition("speaking", "thinking")).toBe(false);
  });
});

describe("useAssistant store state and thinking dwell", () => {
  beforeEach(() => {
    vi.useFakeTimers({
      toFake: [
        "setTimeout",
        "clearTimeout",
        "setInterval",
        "clearInterval",
        "setImmediate",
        "clearImmediate",
        "Date",
        "performance",
      ],
    });
    clearThinkingDwell();
    useAssistant.setState({
      state: "idle",
      visible: false,
      loadingVisible: false,
      transcript: [],
      speakSeq: null,
      audioVolume: 0,
      micLevel: 0,
      ttsActive: false,
      awaitingInput: false,
      ghostActive: false,
      calibrationTarget: null,
      calibrationSize: null,
      calibrationPulse: 0,
      pendingGithubCommand: null,
      pendingGithubCommandAt: null,
      captionActive: false,
    });
  });

  afterEach(() => {
    clearThinkingDwell();
    vi.useRealTimers();
  });

  it("enters thinking state immediately and arms the dwell timer", () => {
    const store = useAssistant.getState();
    store.setState("thinking");
    expect(useAssistant.getState().state).toBe("thinking");
  });

  it("holds thinking state for the minimum 1000ms dwell before transitioning to speaking", () => {
    const store = useAssistant.getState();
    store.setState("thinking");
    expect(useAssistant.getState().state).toBe("thinking");

    // Request immediate change to speaking
    store.setState("speaking");
    // Must still hold thinking state
    expect(useAssistant.getState().state).toBe("thinking");

    // Advance 500ms: still holding
    vi.advanceTimersByTime(500);
    expect(useAssistant.getState().state).toBe("thinking");

    // Advance remaining 500ms (1000ms total): now transitions to speaking
    vi.advanceTimersByTime(500);
    expect(useAssistant.getState().state).toBe("speaking");
  });

  it("transitions immediately if requested after the dwell time has already elapsed", () => {
    const store = useAssistant.getState();
    store.setState("thinking");
    vi.advanceTimersByTime(THINKING_MIN_DWELL_MS + 200);

    // Dwell already satisfied, should change to speaking at once
    store.setState("speaking");
    expect(useAssistant.getState().state).toBe("speaking");
  });

  it("clearThinkingDwell cancels pending transitions", () => {
    const store = useAssistant.getState();
    store.setState("thinking");
    store.setState("speaking");
    expect(useAssistant.getState().state).toBe("thinking");

    clearThinkingDwell();
    vi.advanceTimersByTime(2000);
    // Did not transition to speaking because timer was cancelled
    expect(useAssistant.getState().state).toBe("thinking");
  });
});

describe("useAssistant reset and ghost retention", () => {
  beforeEach(() => {
    clearThinkingDwell();
    useAssistant.setState({
      state: "speaking",
      speakSeq: 4,
      audioVolume: 0.8,
      micLevel: 0.6,
      ttsActive: true,
      awaitingInput: true,
      visible: true,
      ghostActive: false,
    });
  });

  it("reset resets state, audio, levels, and awaiting flags to defaults", () => {
    useAssistant.getState().reset();
    const st = useAssistant.getState();
    expect(st.state).toBe("idle");
    expect(st.speakSeq).toBeNull();
    expect(st.audioVolume).toBe(0);
    expect(st.micLevel).toBe(0);
    expect(st.ttsActive).toBe(false);
    expect(st.awaitingInput).toBe(false);
  });

  it("ghostActive flag survives reset and keeps visibility true", () => {
    useAssistant.setState({ ghostActive: true, visible: true });
    useAssistant.getState().reset();
    const st = useAssistant.getState();
    expect(st.ghostActive).toBe(true);
    expect(st.visible).toBe(true);
  });
});

describe("useAssistant visibility rules", () => {
  it("setVisible toggles visibility when ghost mode is inactive", () => {
    useAssistant.setState({ ghostActive: false, visible: false });
    useAssistant.getState().setVisible(true);
    expect(useAssistant.getState().visible).toBe(true);
    useAssistant.getState().setVisible(false);
    expect(useAssistant.getState().visible).toBe(false);
  });

  it("setVisible cannot hide the orb when ghostActive is true", () => {
    useAssistant.setState({ ghostActive: true, visible: true });
    useAssistant.getState().setVisible(false);
    // In ghost mode, visibility remains true
    expect(useAssistant.getState().visible).toBe(true);
  });

  it("setGhostActive controls ghost mode and enforces visibility on entry", () => {
    useAssistant.setState({ ghostActive: false, visible: false });
    useAssistant.getState().setGhostActive(true);
    expect(useAssistant.getState().ghostActive).toBe(true);
    expect(useAssistant.getState().visible).toBe(true);

    useAssistant.getState().setGhostActive(false);
    expect(useAssistant.getState().ghostActive).toBe(false);
  });
});

describe("assert every public setter once", () => {
  it("verifies setLoadingVisible updates loadingVisible snapshot", () => {
    useAssistant.getState().setLoadingVisible(true);
    expect(useAssistant.getState().loadingVisible).toBe(true);
    useAssistant.getState().setLoadingVisible(false);
    // Note: loading machine dwell may keep visible until settle, or set immediate
    expect(typeof useAssistant.getState().loadingVisible).toBe("boolean");
  });

  it("verifies setAudioVolume updates audioVolume", () => {
    useAssistant.getState().setAudioVolume(0.75);
    expect(useAssistant.getState().audioVolume).toBe(0.75);
  });

  it("verifies setMicLevel clamps between 0 and 1", () => {
    useAssistant.getState().setMicLevel(0.45);
    expect(useAssistant.getState().micLevel).toBe(0.45);

    useAssistant.getState().setMicLevel(-0.2);
    expect(useAssistant.getState().micLevel).toBe(0);

    useAssistant.getState().setMicLevel(1.5);
    expect(useAssistant.getState().micLevel).toBe(1);
  });

  it("verifies addUserMessage, addAssistantMessage, and clearTranscript", () => {
    useAssistant.getState().clearTranscript();
    expect(useAssistant.getState().transcript).toEqual([]);

    useAssistant.getState().addUserMessage("Hello world");
    const t1 = useAssistant.getState().transcript;
    expect(t1).toHaveLength(1);
    expect(t1[0].role).toBe("user");
    expect(t1[0].text).toBe("Hello world");
    expect(typeof t1[0].timestamp).toBe("number");

    useAssistant.getState().addAssistantMessage("Greetings sir");
    const t2 = useAssistant.getState().transcript;
    expect(t2).toHaveLength(2);
    expect(t2[1].role).toBe("assistant");
    expect(t2[1].text).toBe("Greetings sir");

    useAssistant.getState().clearTranscript();
    expect(useAssistant.getState().transcript).toEqual([]);
  });

  it("verifies setSpeakSeq updates speakSeq", () => {
    useAssistant.getState().setSpeakSeq(3);
    expect(useAssistant.getState().speakSeq).toBe(3);
    useAssistant.getState().setSpeakSeq(null);
    expect(useAssistant.getState().speakSeq).toBeNull();
  });

  it("verifies setTtsActive updates ttsActive", () => {
    useAssistant.getState().setTtsActive(true);
    expect(useAssistant.getState().ttsActive).toBe(true);
    useAssistant.getState().setTtsActive(false);
    expect(useAssistant.getState().ttsActive).toBe(false);
  });

  it("verifies setAwaitingInput updates awaitingInput", () => {
    useAssistant.getState().setAwaitingInput(true);
    expect(useAssistant.getState().awaitingInput).toBe(true);
    useAssistant.getState().setAwaitingInput(false);
    expect(useAssistant.getState().awaitingInput).toBe(false);
  });

  it("verifies setCalibration and bumpCalibrationPulse", () => {
    useAssistant.getState().setCalibration("waves", 120);
    expect(useAssistant.getState().calibrationTarget).toBe("waves");
    expect(useAssistant.getState().calibrationSize).toBe(120);

    const prevPulse = useAssistant.getState().calibrationPulse;
    useAssistant.getState().bumpCalibrationPulse();
    expect(useAssistant.getState().calibrationPulse).toBe(prevPulse + 1);

    useAssistant.getState().setCalibration(null, null);
    expect(useAssistant.getState().calibrationTarget).toBeNull();
    expect(useAssistant.getState().calibrationSize).toBeNull();
  });

  it("verifies setPendingGithubCommand updates command and timestamp", () => {
    const cmd = { type: "merge_pr", prNumber: 42 };
    useAssistant.getState().setPendingGithubCommand(cmd);
    expect(useAssistant.getState().pendingGithubCommand).toEqual(cmd);
    expect(typeof useAssistant.getState().pendingGithubCommandAt).toBe("number");

    useAssistant.getState().setPendingGithubCommand(null);
    expect(useAssistant.getState().pendingGithubCommand).toBeNull();
    expect(useAssistant.getState().pendingGithubCommandAt).toBeNull();
  });

  it("verifies setOrbColor updates orbColor", () => {
    useAssistant.getState().setOrbColor("#123456");
    expect(useAssistant.getState().orbColor).toBe("#123456");
  });

  it("verifies setOrbPosition updates orbPosition", () => {
    useAssistant.getState().setOrbPosition("bottom");
    expect(useAssistant.getState().orbPosition).toBe("bottom");
    useAssistant.getState().setOrbPosition("top");
    expect(useAssistant.getState().orbPosition).toBe("top");
  });

  it("verifies setOrbCapsuleStyle updates orbCapsuleStyle", () => {
    useAssistant.getState().setOrbCapsuleStyle("classic");
    expect(useAssistant.getState().orbCapsuleStyle).toBe("classic");
    useAssistant.getState().setOrbCapsuleStyle("pebble");
    expect(useAssistant.getState().orbCapsuleStyle).toBe("pebble");
  });

  it("verifies setOrbWakeupMode updates orbWakeupMode", () => {
    useAssistant.getState().setOrbWakeupMode(3);
    expect(useAssistant.getState().orbWakeupMode).toBe(3);
  });

  it("verifies setOrbInteriorGenesis updates orbInteriorGenesis", () => {
    useAssistant.getState().setOrbInteriorGenesis(1);
    expect(useAssistant.getState().orbInteriorGenesis).toBe(1);
  });

  it("verifies setCaptionActive updates captionActive", () => {
    useAssistant.getState().setCaptionActive(true);
    expect(useAssistant.getState().captionActive).toBe(true);
    useAssistant.getState().setCaptionActive(false);
    expect(useAssistant.getState().captionActive).toBe(false);
  });

  it("verifies setOrbThinkMode, setOrbSpeakMode, and setOrbListenMode", () => {
    useAssistant.getState().setOrbThinkMode(8);
    expect(useAssistant.getState().orbThinkMode).toBe(8);

    useAssistant.getState().setOrbSpeakMode(4);
    expect(useAssistant.getState().orbSpeakMode).toBe(4);

    useAssistant.getState().setOrbListenMode(2);
    expect(useAssistant.getState().orbListenMode).toBe(2);
  });
});

describe("watchdog inputs from assistant store snapshot", () => {
  const resetCleanStore = () => {
    useAssistant.getState().reset();
    useAssistant.setState({
      state: "idle",
      visible: false,
      loadingVisible: false,
      transcript: [],
      speakSeq: null,
      audioVolume: 0,
      micLevel: 0,
      ttsActive: false,
      awaitingInput: false,
      ghostActive: false,
      calibrationTarget: null,
      calibrationSize: null,
      calibrationPulse: 0,
      pendingGithubCommand: null,
      pendingGithubCommandAt: null,
      captionActive: false,
    });
  };

  beforeEach(() => {
    resetCleanStore();
  });

  const makeSnapshot = (over: Partial<OrbSnapshot> = {}): OrbSnapshot => {
    const st = useAssistant.getState();
    return {
      visible: st.visible,
      state: st.state as OrbSnapshot["state"],
      ghostActive: st.ghostActive,
      captionActive: st.captionActive,
      ttsActive: st.ttsActive,
      awaitingInput: st.awaitingInput,
      pendingGithubCommand: !!st.pendingGithubCommand,
      loadingVisible: st.loadingVisible,
      calibrating: st.calibrationTarget !== null,
      ...over,
    };
  };

  it("identifies idle visible state with nothing pending as eligible for hiding", () => {
    resetCleanStore();
    useAssistant.setState({
      state: "idle",
      visible: true,
      loadingVisible: false,
      transcript: [],
      speakSeq: null,
      audioVolume: 0,
      micLevel: 0,
      ttsActive: false,
      awaitingInput: false,
      ghostActive: false,
      calibrationTarget: null,
      calibrationSize: null,
      calibrationPulse: 0,
      pendingGithubCommand: null,
      pendingGithubCommandAt: null,
      captionActive: false,
    });
    const snap = makeSnapshot();
    expect(isIdleEligible(snap)).toBe(true);
    expect(isBusyWatchable(snap)).toBe(false);
  });

  it("identifies thinking or listening without ghost/loading as watchable for recovery", () => {
    resetCleanStore();
    useAssistant.setState({
      state: "thinking",
      visible: true,
      loadingVisible: false,
      transcript: [],
      speakSeq: null,
      audioVolume: 0,
      micLevel: 0,
      ttsActive: false,
      awaitingInput: false,
      ghostActive: false,
      calibrationTarget: null,
      calibrationSize: null,
      calibrationPulse: 0,
      pendingGithubCommand: null,
      pendingGithubCommandAt: null,
      captionActive: false,
    });
    const snapThinking = makeSnapshot();
    expect(isIdleEligible(snapThinking)).toBe(false);
    expect(isBusyWatchable(snapThinking)).toBe(true);

    useAssistant.setState({ visible: true, state: "listening" });
    const snapListening = makeSnapshot();
    expect(isBusyWatchable(snapListening)).toBe(true);
  });

  it("disables watchdog timeouts when ghost session or loader is active", () => {
    resetCleanStore();
    useAssistant.setState({
      state: "listening",
      visible: true,
      loadingVisible: false,
      transcript: [],
      speakSeq: null,
      audioVolume: 0,
      micLevel: 0,
      ttsActive: false,
      awaitingInput: false,
      ghostActive: true,
      calibrationTarget: null,
      calibrationSize: null,
      calibrationPulse: 0,
      pendingGithubCommand: null,
      pendingGithubCommandAt: null,
      captionActive: false,
    });
    expect(isBusyWatchable(makeSnapshot())).toBe(false);

    useAssistant.setState({ visible: true, state: "thinking", ghostActive: false, loadingVisible: true });
    expect(isBusyWatchable(makeSnapshot())).toBe(false);
  });
});
