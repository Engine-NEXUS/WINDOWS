import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  isFollowUpOpen,
  markFollowUpClosed,
  closeFollowUp,
  shouldOpenFollowUp,
  openFollowUpWindow,
  __testSetFollowUpOpen,
} from "./followUp";
import { useAssistant } from "../store/assistant";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const triggerFollowupListenMock = vi.fn();
vi.mock("../stage/orbRuntime", () => ({
  triggerFollowupListen: (...args: unknown[]) => triggerFollowupListenMock(...args),
}));

describe("shouldOpenFollowUp (pure decision table)", () => {
  const base = {
    secs: 6,
    ghostActive: false,
    meetingActive: false,
    offerWindowOpened: false,
    bargedIn: false,
    replyWasError: false,
  };

  it("normal non-ghost reply opens follow-up window", () => {
    expect(shouldOpenFollowUp(base)).toBe(true);
  });

  it("ghost on => false", () => {
    expect(shouldOpenFollowUp({ ...base, ghostActive: true })).toBe(false);
  });

  it("secs 0 => false", () => {
    expect(shouldOpenFollowUp({ ...base, secs: 0 })).toBe(false);
    expect(shouldOpenFollowUp({ ...base, secs: -1 })).toBe(false);
  });

  it("meeting => false", () => {
    expect(shouldOpenFollowUp({ ...base, meetingActive: true })).toBe(false);
  });

  it("offer opened => false", () => {
    expect(shouldOpenFollowUp({ ...base, offerWindowOpened: true })).toBe(false);
  });

  it("barged in => false", () => {
    expect(shouldOpenFollowUp({ ...base, bargedIn: true })).toBe(false);
  });

  it("error reply => false", () => {
    expect(shouldOpenFollowUp({ ...base, replyWasError: true })).toBe(false);
  });
});

describe("followUp state and lifecycle", () => {
  beforeEach(() => {
    markFollowUpClosed();
    invokeMock.mockReset();
    triggerFollowupListenMock.mockReset();
    useAssistant.setState({ ghostActive: false });
  });

  it("tracks open and closed state", () => {
    expect(isFollowUpOpen()).toBe(false);
    __testSetFollowUpOpen(true);
    expect(isFollowUpOpen()).toBe(true);
    markFollowUpClosed();
    expect(isFollowUpOpen()).toBe(false);
  });

  it("closeFollowUp logs and closes if open", () => {
    const spy = vi.spyOn(console, "log").mockImplementation(() => {});
    __testSetFollowUpOpen(true);
    closeFollowUp("silence");
    expect(isFollowUpOpen()).toBe(false);
    expect(spy).toHaveBeenCalledWith("[TURN] followup closed reason=silence");
    spy.mockRestore();
  });

  it("openFollowUpWindow respects IPC settings and triggers follow-up listen", async () => {
    const logSpy = vi.spyOn(console, "log").mockImplementation(() => {});
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "follow_up_window_secs") return Promise.resolve(8);
      if (cmd === "meeting_active") return Promise.resolve(false);
      return Promise.resolve(undefined);
    });

    const opened = await openFollowUpWindow();
    expect(opened).toBe(true);
    expect(isFollowUpOpen()).toBe(true);
    expect(logSpy).toHaveBeenCalledWith("[TURN] followup open secs=8");
    expect(triggerFollowupListenMock).toHaveBeenCalledWith(true, 8000);
    logSpy.mockRestore();
  });

  it("openFollowUpWindow returns false when meeting is active", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "follow_up_window_secs") return Promise.resolve(6);
      if (cmd === "meeting_active") return Promise.resolve(true);
      return Promise.resolve(undefined);
    });

    const opened = await openFollowUpWindow();
    expect(opened).toBe(false);
    expect(isFollowUpOpen()).toBe(false);
    expect(triggerFollowupListenMock).not.toHaveBeenCalled();
  });
});
