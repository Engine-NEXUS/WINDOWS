import { beforeEach, describe, expect, it, vi } from "vitest";

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
  __testSilentMissCount,
  endGhostTurn,
  maybeGhostRelisten,
  recordSilentMiss,
  resetSilentMisses,
  shouldGhostRelisten,
  shouldGhostRoute,
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
