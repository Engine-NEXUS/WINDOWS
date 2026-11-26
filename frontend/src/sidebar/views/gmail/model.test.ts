import { describe, expect, it } from "vitest";
import { categoryKey, categoryLabel, deadlineCount, groupMail, isMailPayload, senderName, summaryLine } from "./model";

describe("gmail view model", () => {
  it("recognises its payload", () => {
    expect(isMailPayload({ kind: "mail", items: [] })).toBe(true);
    expect(isMailPayload({ kind: "whatsapp", items: [] })).toBe(false);
    expect(isMailPayload({ kind: "mail" })).toBe(false);
  });

  it("maps unknown or missing categories to other", () => {
    expect(categoryKey("EXAM")).toBe("exam");
    expect(categoryKey("zebra")).toBe("other");
    expect(categoryKey(null)).toBe("other");
    expect(categoryLabel("database")).toBe("Database alerts");
  });

  it("orders groups by importance and puts deadlines first inside a group", () => {
    const groups = groupMail([
      { sender: "a", subject: "gh 1", category: "github" },
      { sender: "b", subject: "exam notice", category: "exam" },
      { sender: "c", subject: "gh 2 due", category: "github", deadline: "Friday 5 PM" },
      { sender: "d", subject: "misc" },
    ]);
    expect(groups.map((g) => g.key)).toEqual(["exam", "github", "other"]);
    expect(groups[1].items.map((i) => i.subject)).toEqual(["gh 2 due", "gh 1"]);
  });

  it("leaves empty groups out and survives no mail", () => {
    expect(groupMail([])).toEqual([]);
  });

  it("counts mail with a real deadline only", () => {
    expect(deadlineCount([{ sender: "a", subject: "s", deadline: "  " }, { sender: "b", subject: "s", deadline: "Mon" }, { sender: "c", subject: "s" }])).toBe(1);
  });

  it("shows a friendly sender name", () => {
    expect(senderName('"GitHub" <noreply@github.com>')).toBe("GitHub");
    expect(senderName("Asha Rao <asha@x.com>")).toBe("Asha Rao");
    expect(senderName("plain@x.com")).toBe("plain@x.com");
  });

  it("uses only the stored explanation as the summary", () => {
    expect(summaryLine({ sender: "a", subject: "s", explain: " Pay by Friday. " })).toBe("Pay by Friday.");
    expect(summaryLine({ sender: "a", subject: "s" })).toBe("");
  });
});
