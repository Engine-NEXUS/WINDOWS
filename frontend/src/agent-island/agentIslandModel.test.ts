import { describe, expect, it } from "vitest";
import {
  answersJson,
  asMode,
  asSnapshot,
  defaultTab,
  elapsedLabel,
  EMPTY,
  focusSession,
  pillLogos,
  questionsOf,
  secondsLeft,
  stateWord,
  type Snapshot,
} from "./agentIslandModel";

const session = (agent: string, state: string, extra: object = {}) => ({
  key: `${agent}:${Math.random()}`,
  agent,
  label: agent.toUpperCase(),
  folder: "zync",
  state,
  tool: null,
  startedMs: 1,
  updatedMs: 2,
  ...extra,
});

describe("asSnapshot", () => {
  it("drops garbage instead of rendering it", () => {
    expect(asSnapshot(null)).toEqual(EMPTY);
    expect(asSnapshot("x")).toEqual(EMPTY);
    const s = asSnapshot({
      running: 1,
      atMs: 5,
      sessions: [session("claude", "working"), session("codex", "exploding"), { nope: 1 }, null],
      approvals: [{ id: 1, agent: "claude", label: "Claude Code", tool: "Bash", summary: "ls", question: null }, { id: "x" }],
    });
    expect(s.sessions.map((x) => x.agent)).toEqual(["claude"]);
    expect(s.approvals).toHaveLength(1);
    expect(s.approvals[0].tool).toBe("Bash");
  });
});

describe("asMode", () => {
  it("accepts the three modes and defaults to hidden", () => {
    expect(asMode({ mode: "expanded", byApproval: true })).toEqual({ mode: "expanded", byApproval: true, scale: 0.75, hot: false });
    expect(asMode({ mode: "collapsed" })).toEqual({ mode: "collapsed", byApproval: false, scale: 0.75, hot: false });
    expect(asMode({ mode: "weird" }).mode).toBe("hidden");
    expect(asMode(undefined).mode).toBe("hidden");
  });
});

describe("pillLogos", () => {
  it("shows one logo per agent type with a count and its most urgent state", () => {
    const s = asSnapshot({
      sessions: [session("claude", "thinking"), session("claude", "waiting"), session("codex", "working")],
    });
    const logos = pillLogos(s);
    expect(logos).toHaveLength(2);
    const claude = logos.find((l) => l.agent === "claude");
    expect(claude).toMatchObject({ count: 2, state: "waiting" });
    expect(logos.find((l) => l.agent === "codex")).toMatchObject({ count: 1, state: "working" });
  });
  it("is empty with nothing running", () => {
    expect(pillLogos(EMPTY)).toEqual([]);
  });
});

describe("defaultTab", () => {
  it("opens approvals when a request is waiting", () => {
    const withApproval: Snapshot = { ...EMPTY, approvals: [{ id: 1, agent: "claude", label: "C", tool: "Bash", summary: "", question: null, requestedMs: 0, deadlineMs: 0 }] };
    expect(defaultTab(withApproval, false)).toBe("approvals");
    expect(defaultTab(EMPTY, true)).toBe("approvals");
    expect(defaultTab(EMPTY, false)).toBe("tracking");
  });
});

describe("labels", () => {
  it("formats elapsed time", () => {
    expect(elapsedLabel(0)).toBe("0s");
    expect(elapsedLabel(42_900)).toBe("42s");
    expect(elapsedLabel(185_000)).toBe("3m 05s");
    expect(elapsedLabel(3_720_000)).toBe("1h 02m");
    expect(elapsedLabel(-5)).toBe("0s");
  });
  it("counts the seconds left, never below zero", () => {
    expect(secondsLeft(10_000, 1_000)).toBe(9);
    expect(secondsLeft(10_000, 10_000)).toBe(0);
    expect(secondsLeft(10_000, 20_000)).toBe(0);
  });
  it("names the state", () => {
    expect(stateWord("working", "Bash")).toBe("Running Bash");
    expect(stateWord("working", null)).toBe("Working");
    expect(stateWord("waiting", null)).toBe("Waiting for you");
    expect(stateWord("done", null)).toBe("Finished");
  });
});

describe("questions", () => {
  const raw = {
    questions: [
      { question: "Which one?", options: [{ label: "A" }, { label: "B" }] },
      { question: "Extras?", multiSelect: true, options: [{ label: "Tests" }, { label: "Docs" }] },
    ],
  };
  it("reads the questions of an AskUserQuestion input", () => {
    const qs = questionsOf(raw);
    expect(qs).toHaveLength(2);
    expect(qs[1].multi).toBe(true);
    expect(questionsOf(null)).toEqual([]);
    expect(questionsOf({ questions: [{ question: "no options", options: [] }] })).toEqual([]);
  });
  it("builds the answers only when every question is answered with a real option", () => {
    const qs = questionsOf(raw);
    expect(answersJson(qs, { "Which one?": ["A"] })).toBeNull();
    expect(answersJson(qs, { "Which one?": ["Z"], "Extras?": ["Tests"] })).toBeNull();
    const ok = answersJson(qs, { "Which one?": ["B"], "Extras?": ["Tests", "Docs", "Tests"] });
    expect(JSON.parse(ok!)).toEqual({ answers: { "Which one?": "B", "Extras?": ["Tests", "Docs"] } });
    expect(answersJson([], {})).toBeNull();
  });
});

describe("steps, edits and focus", () => {
  const withSteps = asSnapshot({
    sessions: [
      {
        key: "claude:a",
        agent: "claude",
        label: "Claude Code",
        folder: "korus",
        state: "working",
        tool: "Edit",
        steps: [
          { tool: "Read", detail: "invoice.ts", state: "done" },
          { tool: "Edit", detail: "invoice.ts", state: "running" },
          { nope: 1 },
        ],
        edit: { path: "src/invoice.ts", kind: "edit", pairs: [{ old: "a", new: "b" }], tooLarge: false },
        startedMs: 1,
        updatedMs: 5,
      },
      { key: "codex:b", agent: "codex", label: "Codex", folder: null, state: "waiting", tool: null, startedMs: 1, updatedMs: 4 },
    ],
  });

  it("keeps good steps and edits and drops malformed ones", () => {
    const claude = withSteps.sessions[0];
    expect(claude.steps).toEqual([
      { tool: "Read", detail: "invoice.ts", state: "done" },
      { tool: "Edit", detail: "invoice.ts", state: "running" },
    ]);
    expect(claude.edit?.path).toBe("src/invoice.ts");
    expect(withSteps.sessions[1].steps).toEqual([]);
    expect(withSteps.sessions[1].edit).toBeNull();
    expect(asSnapshot({ sessions: [{ ...session("claude", "working"), edit: { path: "" } }] }).sessions[0].edit).toBeNull();
  });

  it("focuses the asked-for session, else the most urgent", () => {
    expect(focusSession(withSteps, "claude:a")?.agent).toBe("claude");
    expect(focusSession(withSteps, null)?.agent).toBe("codex");
    expect(focusSession(withSteps, "gone")?.agent).toBe("codex");
    expect(focusSession(EMPTY, null)).toBeNull();
  });
});
