import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn().mockResolvedValue(undefined);

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useAssistant } from "../store/assistant";
import {
  __testMissStreak,
  __testSetMissStreak,
  hideOrbIfIdle,
  isAdministratorControlText,
  nextMissAction,
  processTranscript,
} from "./recorder";

describe("Administrator capture routing", () => {
  it("sends explicit role commands to Rust without frontend text rewriting", () => {
    for (const phrase of [
      "Nexus, administrator, analyse PR 24 in owner/repo and send it to my laptop",
      "Start administrator session",
      "Am I in admin mode?",
      "send the review to my laptop",
    ]) {
      expect(isAdministratorControlText(phrase)).toBe(true);
    }
    expect(isAdministratorControlText("open browser")).toBe(false);
    expect(isAdministratorControlText("administer medicine")).toBe(false);
  });
});

describe("hideOrbIfIdle (Main Center UI-director rule, doc 74 P2)", () => {
  beforeEach(() => {
    useAssistant.setState({ ghostActive: false, visible: true });
  });

  it("hides the orb on turn end outside ghost mode", () => {
    useAssistant.setState({ visible: true, ghostActive: false });
    hideOrbIfIdle();
    expect(useAssistant.getState().visible).toBe(false);
  });

  it("keeps the orb visible for the WHOLE ghost session", () => {
    useAssistant.setState({ visible: true, ghostActive: true });
    hideOrbIfIdle();
    expect(useAssistant.getState().visible).toBe(true);
    // Repeated turn ends must not hide it either.
    hideOrbIfIdle();
    hideOrbIfIdle();
    expect(useAssistant.getState().visible).toBe(true);
  });
});

describe("processTranscript provenance gating", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    useAssistant.setState({
      ghostActive: false,
      visible: true,
      state: "listening",
      transcript: [],
    });
    __testSetMissStreak(0);
  });

  it("drops a rejected turn without relistening, nagging, learning, or remembering", async () => {
    await processTranscript("", {
      ownership: "rejected",
      ownerScore: 0.12,
      decoderBias: "neutral",
      language: "en",
      initiation: "ghost",
    });

    expect(useAssistant.getState().visible).toBe(false);
    expect(useAssistant.getState().transcript).toEqual([]);
    expect(__testMissStreak()).toBe(0);
    await vi.advanceTimersByTimeAsync(550);
    expect(useAssistant.getState().state).toBe("idle");
  });

  it("sends native raw and corrected text to Rust without local execution", async () => {
    const previous = (window as any).__TAURI_INTERNALS__;
    (window as any).__TAURI_INTERNALS__ = {};
    invokeMock.mockClear();
    invokeMock.mockImplementation((command: string) => command === "orchestrator_process"
      ? Promise.resolve({ request_id: "native-1", subsystem: "local_command", handled_locally: false })
      : Promise.resolve(undefined));
    try {
      await processTranscript("analyze my screen.", {
        session: 42, rawText: "analyze my screen.", ownership: "unenrolled",
        decoderBias: "neutral", language: "en",
      });
      expect(invokeMock).toHaveBeenCalledWith("orchestrator_process", expect.objectContaining({
        transcript: "analyse my screen.",
        turnContext: expect.objectContaining({
          session: 42, rawText: "analyze my screen.", correctedText: "analyse my screen.",
        }),
      }));
      expect(invokeMock.mock.calls.some(([command]) => command === "execute_command")).toBe(false);
    } finally {
      if (previous === undefined) delete (window as any).__TAURI_INTERNALS__;
      else (window as any).__TAURI_INTERNALS__ = previous;
      invokeMock.mockReset().mockResolvedValue(undefined);
    }
  });
});

describe("nextMissAction (approach E retry-with-escalation)", () => {
  beforeEach(() => {
    __testSetMissStreak(0);
  });

  it("1st consecutive miss relistens silently", () => {
    expect(nextMissAction(1)).toBe("relisten");
    expect(nextMissAction(0)).toBe("relisten");
  });

  it("2nd consecutive miss nags once", () => {
    expect(nextMissAction(2)).toBe("nag");
    expect(nextMissAction(5)).toBe("nag");
  });

  it("streak helpers round-trip", () => {
    expect(__testMissStreak()).toBe(0);
    __testSetMissStreak(1);
    expect(__testMissStreak()).toBe(1);
    expect(nextMissAction(__testMissStreak())).toBe("relisten");
    __testSetMissStreak(0);
  });
});
