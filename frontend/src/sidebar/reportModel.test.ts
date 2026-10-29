import { describe, expect, it } from "vitest";
import { agentPrompt, filterFindings, gateLine, groupBySeverity, kindLabel, lineRange, severityCounts, sourceLabel, usageLine, allowanceLine, canReview, confirmLine, elapsedText, minutesText, type Report, type ReportFinding } from "./reportModel";

const f = (id: string, severity: ReportFinding["severity"], extra: Partial<ReportFinding> = {}): ReportFinding => ({
  id,
  severity,
  kind: "refactor",
  kindInferred: true,
  file: "src/a.rs",
  startLine: 10,
  endLine: 12,
  title: `t-${id}`,
  comment: `c-${id}`,
  suggestions: [],
  ...extra,
});

const report = (findings: ReportFinding[], extra: Partial<Report> = {}): Report => ({ source: "remoteReview", summary: "", findings, unreviewedFiles: 0, reused: false, notes: [], ...extra });

describe("report model", () => {
  it("counts and groups by CodeRabbit's severity order, most serious first, empty groups left out", () => {
    const list = [f("1", "minor"), f("2", "critical"), f("3", "minor"), f("4", "unknown")];
    expect(severityCounts(list)).toEqual({ critical: 1, major: 0, minor: 2, trivial: 0, info: 0, unknown: 1 });
    expect(groupBySeverity(list).map((g) => [g.severity, g.items.length])).toEqual([["critical", 1], ["minor", 2], ["unknown", 1]]);
    expect(filterFindings(list, "minor")).toHaveLength(2);
    expect(filterFindings(list, "all")).toHaveLength(4);
  });
  it("an unrecognised severity is counted as unknown, never as something official", () => {
    expect(severityCounts([f("1", "praise" as never)]).unknown).toBe(1);
  });
  it("labels the type and marks an inferred one", () => {
    expect(kindLabel("potentialIssue", false)).toBe("Potential issue");
    expect(kindLabel("nitpick", true)).toBe("Nitpick (inferred)");
    expect(kindLabel("unknown", true)).toBe("");
  });
  it("describes the source, usage and gate in plain words", () => {
    expect(sourceLabel("githubBot")).toMatch(/posted on GitHub/);
    expect(sourceLabel("remoteReview")).toMatch(/free review/);
    expect(sourceLabel("none")).toMatch(/Not reviewed/);
    expect(usageLine({ state: "available", limit: 10, remaining: 7, billingStatus: "inactive" })).toBe("7 of 10 free CodeRabbit reviews left this hour");
    expect(usageLine({ state: "exhausted", limit: 10, remaining: 0, billingStatus: "inactive" })).toBeNull();
    expect(usageLine(null)).toBeNull();
    expect(gateLine({ status: "blocked", threshold: "major", blocking: 2 })).toBe("Gate blocked at major (2 blocking)");
    expect(gateLine({ status: "passed" })).toBe("Gate passed");
    expect(gateLine(null)).toBeNull();
  });
  it("formats line ranges", () => {
    expect(lineRange({ startLine: 5, endLine: 5 })).toBe("5");
    expect(lineRange({ startLine: 5, endLine: 9 })).toBe("5-9");
    expect(lineRange({ startLine: null, endLine: null })).toBe("");
  });
  it("builds the agent prompt from CodeRabbit's own instructions, or its consolidated prompt", () => {
    const withInstr = report([f("1", "major", { agentInstructions: "Call recycle()" }), f("2", "minor")]);
    expect(agentPrompt(withInstr)).toBe("src/a.rs:10-12 - t-1\nCall recycle()\n\nsrc/a.rs:10-12 - t-2\nc-2");
    expect(agentPrompt(report([f("1", "major")], { promptForAgents: "  FIX ALL  " }))).toBe("FIX ALL");
    expect(agentPrompt(report([]))).toBe("");
  });
});

describe("review allowance", () => {
  const u = { state: "available", limit: 10, remaining: 7, billingStatus: "inactive", fullCapacityMs: 2_972_403 };
  it("shows the count and when it is back to full", () => {
    expect(allowanceLine(u)).toBe("7 of 10 free reviews left · back to 10 in ~50 min");
    expect(allowanceLine({ ...u, remaining: 10, fullCapacityMs: 0 })).toBe("10 of 10 free reviews left");
    expect(allowanceLine({ ...u, remaining: 0, state: "exhausted" })).toBe("0 of 10 free reviews left · back to 10 in ~50 min");
    expect(allowanceLine(null)).toBeNull();
    expect(minutesText(1)).toBe("1 min");
    expect(minutesText(0)).toBe("under a minute");
  });
  it("only offers a review when a free one is left, and asks with the count", () => {
    expect(canReview(u)).toBe(true);
    expect(canReview({ ...u, remaining: 0 })).toBe(false);
    expect(canReview({ ...u, state: "exhausted" })).toBe(false);
    expect(canReview(undefined)).toBe(false);
    expect(confirmLine(u)).toBe("Use 1 of your 7 free reviews?");
  });
  it("formats the elapsed time", () => {
    expect(elapsedText(125_000)).toBe("2:05");
    expect(elapsedText(-5)).toBe("0:00");
  });
});
