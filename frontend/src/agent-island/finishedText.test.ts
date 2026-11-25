import { describe, expect, it } from "vitest";
import { FINISH_W, finishedText, islandSize, pillWidth, type AgentState, type SessionView, type Snapshot } from "./agentIslandModel";

function session(agent: string, label: string, state: AgentState, failed = false): SessionView {
  return { key: `${agent}:1`, agent, label, folder: null, state, tool: null, steps: [], edit: null, startedMs: 0, updatedMs: 0, failed };
}
const snap = (...sessions: SessionView[]): Snapshot => ({ running: sessions.filter((s) => s.state !== "done").length, atMs: 0, sessions, approvals: [] });

describe("the pill says an agent finished (the bar is the notice, no toast)", () => {
  it("names the one agent that finished", () => {
    expect(finishedText(snap(session("claude", "Claude Code", "done")))).toBe("Claude Code finished");
    expect(finishedText(snap(session("codex", "Codex", "done")))).toBe("Codex finished");
    expect(finishedText(snap(session("opencode", "OpenCode", "done")))).toBe("OpenCode finished");
  });

  it("says failed for a turn that ended with an error", () => {
    expect(finishedText(snap(session("antigravity", "Antigravity", "done", true)))).toBe("Antigravity failed");
  });

  it("counts agents when several finished together", () => {
    expect(finishedText(snap(session("claude", "Claude Code", "done"), session("codex", "Codex", "done")))).toBe("2 agents finished");
  });

  it("says nothing while any agent is still thinking, working or waiting for you", () => {
    expect(finishedText(snap(session("claude", "Claude Code", "done"), session("codex", "Codex", "working")))).toBeNull();
    expect(finishedText(snap(session("claude", "Claude Code", "waiting")))).toBeNull();
    expect(finishedText(snap(session("claude", "Claude Code", "thinking")))).toBeNull();
    expect(finishedText(snap())).toBeNull();
  });

  it("widens the pill by the text width, and only then", () => {
    expect(pillWidth(1, true)).toBe(pillWidth(1) + FINISH_W);
    expect(islandSize("collapsed", 2, true).w).toBe(pillWidth(2) + FINISH_W);
    expect(islandSize("collapsed", 2).w).toBe(pillWidth(2));
    expect(islandSize("expanded", 2, true).w).toBe(islandSize("expanded", 2).w);
  });
});
