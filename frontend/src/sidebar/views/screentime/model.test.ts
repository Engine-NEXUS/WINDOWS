import { describe, expect, it } from "vitest";
import { averageBefore, formatMinutes, isStPayload, today, topRows, versusAverage, weekBars, weekdayShort } from "./model";

const day = (d: string, minutes: number) => ({ day: d, minutes, apps: [] });

describe("screen time view model", () => {
  it("recognises its payload", () => {
    expect(isStPayload({ kind: "screentime", enabled: true, days: [], sites: [] })).toBe(true);
    expect(isStPayload({ kind: "screentime", enabled: true, days: [] })).toBe(false);
    expect(isStPayload({ kind: "mail", enabled: true, days: [], sites: [] })).toBe(false);
  });

  it("formats minutes as hours and minutes", () => {
    expect(formatMinutes(0)).toBe("0m");
    expect(formatMinutes(45)).toBe("45m");
    expect(formatMinutes(60)).toBe("1h");
    expect(formatMinutes(135)).toBe("2h 15m");
    expect(formatMinutes(-5)).toBe("0m");
  });

  it("names the weekday from the date parts", () => {
    expect(weekdayShort("2026-11-22")).toBe("Sun");
    expect(weekdayShort("2026-11-23")).toBe("Mon");
    expect(weekdayShort("garbage")).toBe("garbage");
  });

  it("builds bars oldest first, scaled to the busiest day, today last", () => {
    const bars = weekBars([day("2026-11-22", 60), day("2026-11-20", 120), day("2026-11-21", 30)]);
    expect(bars.map((b) => b.day)).toEqual(["2026-11-20", "2026-11-21", "2026-11-22"]);
    expect(bars.map((b) => b.pct)).toEqual([100, 25, 50]);
    expect(bars.map((b) => b.today)).toEqual([false, false, true]);
  });

  it("keeps only the last N days and survives all-zero days", () => {
    const many = Array.from({ length: 10 }, (_, i) => day(`2026-11-${String(10 + i).padStart(2, "0")}`, 0));
    const bars = weekBars(many, 7);
    expect(bars).toHaveLength(7);
    expect(bars.every((b) => b.pct === 0)).toBe(true);
  });

  it("finds today as the latest day", () => {
    expect(today([day("2026-11-21", 1), day("2026-11-22", 2)])?.minutes).toBe(2);
    expect(today([])).toBeNull();
  });

  it("ranks rows by minutes and drops zeros", () => {
    const rows = topRows([{ name: "b", minutes: 10 }, { name: "a", minutes: 10 }, { name: "z", minutes: 0 }, { name: "c", minutes: 40 }], 3);
    expect(rows.map((r) => r.name)).toEqual(["c", "a", "b"]);
    expect(rows.map((r) => r.pct)).toEqual([100, 25, 25]);
  });

  it("compares today with the average of earlier days", () => {
    expect(averageBefore([day("2026-11-20", 100), day("2026-11-21", 200), day("2026-11-22", 50)])).toBe(150);
    expect(averageBefore([day("2026-11-22", 50)])).toBeNull();
    expect(versusAverage(180, 150)).toBe("30m more than your usual");
    expect(versusAverage(120, 150)).toBe("30m less than your usual");
    expect(versusAverage(152, 150)).toBe("About your usual");
    expect(versusAverage(10, null)).toBe("");
  });
});
