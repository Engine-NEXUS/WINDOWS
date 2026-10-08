import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const triggerFollowupListenMock = vi.fn();
vi.mock("../stage/orbRuntime", () => ({
  triggerFollowupListen: (...args: unknown[]) => triggerFollowupListenMock(...args),
}));

import { useAssistant } from "../store/assistant";
import {
  GHOST_SILENT_CAP,
  GHOST_STALL_AFTER_MS,
  __testSetGhostTurnTimes,
  __testSilentMissCount,
  __testStopGhostWatchdog,
  endGhostTurn,
  maybeGhostRelisten,
  noteGhostTurnClosed,
  noteGhostTurnOpened,
  recordSilentMiss,
  resetSilentMisses,
  shouldGhostRelisten,
  shouldGhostRoute,
  startGhostWatchdog,
  stopGhostWatchdog,
} from "./ghostHotMic";

describe("ghostHotMic pure helpers", () => {
  beforeEach(() => {
    resetSilentMisses();
    useAssistant.setState({ ghostActive: false });
  });

  it("only relistens inside ghost mode", () => {
    expect(shouldGhostRelisten(true)).toBe(true);
    expect(shouldGhostRelisten(false)).toBe(false);
  });

  it("silence streak counts up and resets on speech", () => {
    expect(GHOST_SILENT_CAP).toBe(3);
    expect(recordSilentMiss()).toBe(1);
    expect(recordSilentMiss()).toBe(2);
    expect(__testSilentMissCount()).toBe(2);
    resetSilentMisses();
    expect(__testSilentMissCount()).toBe(0);
  });
});

describe("shouldGhostRoute", () => {
  it("routes open_app and whatsapp_chat to ghost runners in-session", () => {
    expect(shouldGhostRoute("open_app", true)).toBe(true);
    expect(shouldGhostRoute("whatsapp_chat", true)).toBe(true);
  });

  it("stays local outside ghost mode and for all other actions", () => {
    expect(shouldGhostRoute("open_app", false)).toBe(false);
    expect(shouldGhostRoute("whatsapp_chat", false)).toBe(false);
    expect(shouldGhostRoute("greeting", true)).toBe(false);
    expect(shouldGhostRoute("close_app", true)).toBe(false);
    expect(shouldGhostRoute("media_stop", true)).toBe(false);
    expect(shouldGhostRoute("search", true)).toBe(false);
  });
});

describe("maybeGhostRelisten", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockReset().mockResolvedValue(false);
    useAssistant.setState({ ghostActive: false });
  });

  it("no-ops outside ghost mode without touching IPC", async () => {
    expect(await maybeGhostRelisten()).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("skips the loop in meeting mode", async () => {
    useAssistant.setState({ ghostActive: true });
    invokeMock.mockResolvedValue(true); // meeting_active
    expect(await maybeGhostRelisten()).toBe(false);
  });
});

describe("endGhostTurn", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    invokeMock.mockReset().mockResolvedValue(false);
    triggerFollowupListenMock.mockReset();
    useAssistant.setState({ ghostActive: false, state: "speaking", visible: true });
  });

  it("outside ghost mode it is exactly reset() with no IPC", async () => {
    await endGhostTurn();
    expect(invokeMock).not.toHaveBeenCalled();
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
    expect(useAssistant.getState().state).toBe("idle");
  });

  it("inside ghost mode it relistens before resetting (loop stays alive)", async () => {
    useAssistant.setState({ ghostActive: true, state: "speaking", visible: true });
    await endGhostTurn();
    expect(invokeMock).toHaveBeenCalled(); // meeting_active gate consulted
    expect(triggerFollowupListenMock).toHaveBeenCalledTimes(1);
    expect(useAssistant.getState().state).toBe("idle");
    // Session survives the turn — reset() must never clear ghostActive.
    expect(useAssistant.getState().ghostActive).toBe(true);
  });
});

describe("ghost stall watchdog (hung turns re-arm the mic)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.stubGlobal("window", { __TAURI_INTERNALS__: {} });
    triggerFollowupListenMock.mockReset();
    __testStopGhostWatchdog();
    useAssistant.setState({ ghostActive: false });
  });

  afterEach(() => {
    __testStopGhostWatchdog();
    vi.useRealTimers();
  });

  it("re-arms once when a turn stays open past the stall budget", async () => {
    useAssistant.setState({ ghostActive: true });
    __testSetGhostTurnTimes(Date.now() - GHOST_STALL_AFTER_MS - 5000, 0);
    startGhostWatchdog();
    await vi.advanceTimersByTimeAsync(5000);
    expect(triggerFollowupListenMock).toHaveBeenCalledTimes(1);
    // Single re-arm per stall: further ticks stay quiet until a new turn.
    await vi.advanceTimersByTimeAsync(20000);
    expect(triggerFollowupListenMock).toHaveBeenCalledTimes(1);
  });

  it("stays quiet for fresh turns, closed turns, and outside ghost mode", async () => {
    startGhostWatchdog();
    // Fresh turn (just opened).
    useAssistant.setState({ ghostActive: true });
    noteGhostTurnOpened();
    await vi.advanceTimersByTimeAsync(20000);
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
    // Closed turn.
    noteGhostTurnClosed();
    __testSetGhostTurnTimes(Date.now() - 60000, Date.now() - 50000);
    await vi.advanceTimersByTimeAsync(20000);
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
    // Outside ghost mode entirely.
    useAssistant.setState({ ghostActive: false });
    __testSetGhostTurnTimes(Date.now() - 60000, 0);
    await vi.advanceTimersByTimeAsync(20000);
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
  });

  it("stop prevents all future firing", async () => {
    useAssistant.setState({ ghostActive: true });
    __testSetGhostTurnTimes(Date.now() - 60000, 0);
    startGhostWatchdog();
    stopGhostWatchdog();
    await vi.advanceTimersByTimeAsync(20000);
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
  });
});
