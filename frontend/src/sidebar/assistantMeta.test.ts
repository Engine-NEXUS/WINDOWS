import { describe, expect, it } from "vitest";
import { assistantHeader, assistantKind } from "./assistantMeta";

const none = { conflict: false, confirmation: false, analysis: null, codeRabbit: null, research: false };

describe("assistant view kind", () => {
  it("picks the kind in the priority the view always had", () => {
    expect(assistantKind({ ...none, conflict: true, confirmation: true })).toBe("conflict");
    expect(assistantKind({ ...none, confirmation: true, analysis: { findings: [] } })).toBe("confirmation");
    expect(assistantKind({ ...none, analysis: { mode: "event_detail" } })).toBe("event");
    expect(assistantKind({ ...none, codeRabbit: { prNumber: 3 } })).toBe("pr-review");
    expect(assistantKind({ ...none, analysis: { findings: [] } })).toBe("pr-review");
    expect(assistantKind({ ...none, analysis: { activePrs: [] } })).toBe("pr-list");
    expect(assistantKind({ ...none, analysis: { mode: "pr_list" } })).toBe("pr-list");
    expect(assistantKind({ ...none, analysis: { stars: 3 } })).toBe("repo");
    expect(assistantKind({ ...none, research: true })).toBe("research");
    expect(assistantKind(none)).toBe("answer");
  });

  it("gives every kind a header with an icon and a title", () => {
    for (const k of ["conflict", "confirmation", "event", "pr-review", "pr-list", "repo", "research", "answer"] as const) {
      const h = assistantHeader(k, {});
      expect(h.icon.length).toBeGreaterThan(0);
      expect(h.title.length).toBeGreaterThan(0);
    }
  });

  it("names the PR, the repository and the topic when it knows them", () => {
    expect(assistantHeader("pr-review", { prNumber: 34, repo: "Engine-NEXUS/zync" })).toEqual({ icon: "pr", title: "PR #34", subtitle: "Engine-NEXUS/zync" });
    expect(assistantHeader("repo", { repo: "Engine-NEXUS/zync" }).title).toBe("zync");
    expect(assistantHeader("research", { topic: "click-through windows" }).subtitle).toBe("click-through windows");
    expect(assistantHeader("confirmation", { app: "WhatsApp" }).subtitle).toBe("WhatsApp");
  });
});
