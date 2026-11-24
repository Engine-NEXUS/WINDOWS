import { describe, expect, it } from "vitest";
import { clockTime, dayLabel, groupMessages, incomingCount, initials, isWaPayload } from "./model";

// 2026-11-22 12:00:00 UTC
const NOW = Date.UTC(2026, 10, 22, 12, 0, 0) / 1000;
const at = (d: number, h: number, m = 0) => Date.UTC(2026, 10, d, h, m, 0) / 1000;

describe("whatsapp view model", () => {
  it("recognises its payload and nothing else", () => {
    expect(isWaPayload({ kind: "whatsapp", name: "Asha", messages: [] })).toBe(true);
    expect(isWaPayload({ kind: "mail", name: "Asha", messages: [] })).toBe(false);
    expect(isWaPayload({ kind: "whatsapp", messages: [] })).toBe(false);
    expect(isWaPayload(null)).toBe(false);
  });

  it("labels days Today, Yesterday, then weekday and date", () => {
    expect(dayLabel(at(22, 9), NOW, 0)).toBe("Today");
    expect(dayLabel(at(21, 23), NOW, 0)).toBe("Yesterday");
    expect(dayLabel(at(18, 8), NOW, 0)).toBe("Wed 18 Nov");
    expect(dayLabel(0, NOW, 0)).toBe("Earlier");
    expect(dayLabel(Date.UTC(2025, 5, 3) / 1000, NOW, 0)).toBe("Tue 3 Jun 2025");
  });

  it("respects the local zone when deciding the day", () => {
    // 23:30 UTC on the 21st is 05:00 on the 22nd at +5:30
    expect(dayLabel(at(21, 23, 30), NOW, 330)).toBe("Today");
    expect(clockTime(at(21, 23, 30), 330)).toBe("05:00");
    expect(clockTime(0, 0)).toBe("");
  });

  it("makes initials from one or two words and survives empty names", () => {
    expect(initials("Asha Rao")).toBe("AR");
    expect(initials("mummy")).toBe("M");
    expect(initials("  Raj Kumar Singh ")).toBe("RS");
    expect(initials("")).toBe("?");
  });

  it("groups oldest first by day and marks runs from the same sender", () => {
    const groups = groupMessages(
      [
        { id: "3", ts: at(22, 9, 5), text: "ok", fromMe: true },
        { id: "1", ts: at(21, 20), text: "hi", fromMe: false },
        { id: "2", ts: at(21, 20, 1), text: "are you free?", fromMe: false },
        { id: "4", ts: at(22, 9, 6), text: "call you later", fromMe: true },
        { id: "5", ts: at(22, 9, 20), text: "sure", fromMe: false },
      ],
      NOW,
      0,
    );
    expect(groups.map((g) => g.label)).toEqual(["Yesterday", "Today"]);
    expect(groups[0].rows.map((r) => r.id)).toEqual(["1", "2"]);
    expect(groups[0].rows.map((r) => [r.first, r.last])).toEqual([[true, false], [false, true]]);
    expect(groups[1].rows.map((r) => [r.id, r.first, r.last])).toEqual([["3", true, false], ["4", false, true], ["5", true, true]]);
    expect(groups[1].rows[0].time).toBe("09:05");
  });

  it("keeps messages with an unknown time together at the start", () => {
    const groups = groupMessages([{ id: "a", ts: 0, text: "old", fromMe: false }, { id: "b", ts: at(22, 8), text: "new", fromMe: false }], NOW, 0);
    expect(groups[0].label).toBe("Earlier");
    expect(groups[0].rows[0].time).toBe("");
    expect(groups[1].label).toBe("Today");
  });

  it("counts the messages from the other person", () => {
    expect(incomingCount([{ id: "1", ts: 1, text: "a", fromMe: false }, { id: "2", ts: 2, text: "b", fromMe: true }, { id: "3", ts: 3, text: "c", fromMe: false }])).toBe(2);
    expect(incomingCount([])).toBe(0);
  });
});
