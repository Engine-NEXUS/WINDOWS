import { describe, expect, it } from "vitest";
import {
  countdownText,
  digitsFor,
  kindLabel,
  MAX_CHIPS,
  parseFired,
  parseReminder,
  parseReminders,
  ringFraction,
  rollDuration,
  rollRange,
  rollSteps,
  STRIP_LENGTH,
  untilWords,
  visibleChips,
  wantsChip,
  type ReminderEvent,
} from "./reminderModel";
import { captionSlot, intersects, reminderStackSlot, taskStackSlot, type Rect } from "./layoutSlots";

function ev(id: string, atMs: number, over: Partial<ReminderEvent> = {}): ReminderEvent {
  return { id, kind: "trip", title: "Leave for the airport", atMs, spoken: "s", alert: "a", source: "directions", ...over };
}

// A local time on a fixed day, so the tests do not depend on when they run.
function at(h: number, m: number): number {
  return new Date(2026, 10, 14, h, m, 0).getTime();
}

describe("clock digits", () => {
  it("reads a 12-hour clock for every edge of the day", () => {
    const cases: [number, number, [number, number, number, number, string]][] = [
      [0, 0, [1, 2, 0, 0, "AM"]],
      [0, 5, [1, 2, 0, 5, "AM"]],
      [1, 7, [0, 1, 0, 7, "AM"]],
      [9, 59, [0, 9, 5, 9, "AM"]],
      [10, 0, [1, 0, 0, 0, "AM"]],
      [11, 59, [1, 1, 5, 9, "AM"]],
      [12, 0, [1, 2, 0, 0, "PM"]],
      [12, 30, [1, 2, 3, 0, "PM"]],
      [13, 15, [0, 1, 1, 5, "PM"]],
      [17, 0, [0, 5, 0, 0, "PM"]],
      [23, 59, [1, 1, 5, 9, "PM"]],
    ];
    for (const [h, m, want] of cases) {
      const d = digitsFor(at(h, m));
      expect([d.h1, d.h2, d.m1, d.m2, d.ap], `${h}:${m}`).toEqual(want);
    }
  });

  it("has the text NEXUS would say", () => {
    expect(digitsFor(at(17, 0)).text).toBe("5:00 PM");
    expect(digitsFor(at(0, 7)).text).toBe("12:07 AM");
    expect(digitsFor(at(13, 45)).text).toBe("1:45 PM");
  });
});

describe("odometer roll", () => {
  it("only ever moves forward, and always lands on the wanted digit", () => {
    for (let col = 0; col < 4; col++) {
      for (let cur = 0; cur < 10; cur++) {
        for (let want = 0; want < 10; want++) {
          const { from, to } = rollRange(col, cur, want);
          expect(from).toBe(cur);
          expect(to, `col ${col} ${cur}->${want}`).toBeGreaterThanOrEqual(from);
          expect(to % 10, `col ${col} ${cur}->${want}`).toBe(want);
          expect(to, "inside the strip").toBeLessThan(STRIP_LENGTH);
        }
      }
    }
  });

  it("turns the units and minute columns further than the hour-tens column", () => {
    expect(rollSteps(0, 3, 3)).toBe(0);
    expect(rollSteps(0, 9, 1)).toBe(2);
    expect(rollSteps(1, 3, 3)).toBe(10);
    expect(rollSteps(2, 3, 3)).toBe(10);
    expect(rollSteps(3, 3, 3)).toBe(20);
    expect(rollSteps(3, 9, 0)).toBe(21);
  });

  it("takes the whole roll about two and a half seconds at normal speed", () => {
    expect(rollDuration()).toBeGreaterThan(2000);
    expect(rollDuration()).toBeLessThan(3500);
  });
});

describe("words and numbers", () => {
  it("says how long until a reminder", () => {
    expect(untilWords(20_000)).toBe("in less than a minute");
    expect(untilWords(60_000)).toBe("in 1 minute");
    expect(untilWords(45 * 60_000)).toBe("in 45 minutes");
    expect(untilWords(60 * 60_000)).toBe("in 1 hour");
    expect(untilWords(134 * 60_000)).toBe("in 2 hours 14 minutes");
  });

  it("counts down in hours, then minutes and seconds, then now", () => {
    expect(countdownText(2 * 3600_000 + 13 * 60_000 + 20_000)).toBe("2h 13m");
    expect(countdownText(9 * 60_000 + 58_000)).toBe("09:58");
    expect(countdownText(5_000)).toBe("00:05");
    expect(countdownText(0)).toBe("now");
    expect(countdownText(-5000)).toBe("now");
  });

  it("fills the ring from nothing to full", () => {
    expect(ringFraction(0, 1000, 0)).toBe(0);
    expect(ringFraction(0, 1000, 500)).toBeCloseTo(0.5);
    expect(ringFraction(0, 1000, 1000)).toBe(1);
    expect(ringFraction(0, 1000, 5000)).toBe(1);
    expect(ringFraction(1000, 1000, 1000)).toBeGreaterThanOrEqual(0);
  });

  it("labels the kinds and falls back for an unknown one", () => {
    expect(kindLabel("study")).toBe("Study");
    expect(kindLabel("calendar")).toBe("Calendar");
    expect(kindLabel("something-new")).toBe("Reminder");
  });
});

describe("the countdown switch", () => {
  it("lets the user's switch win in every combination", () => {
    const asks = ev("a", 1, { display: { countdown: true } });
    const refuses = ev("b", 1, { display: { countdown: false } });
    const silent = ev("c", 1);
    // user on: a reminder can ask for no chip
    expect(wantsChip(asks, true, false)).toBe(true);
    expect(wantsChip(refuses, true, false)).toBe(false);
    expect(wantsChip(silent, true, false)).toBe(true);
    // user off: no reminder can show one, whatever it asks
    expect(wantsChip(asks, false, false)).toBe(false);
    expect(wantsChip(refuses, false, false)).toBe(false);
    expect(wantsChip(silent, false, false)).toBe(false);
  });

  it("always shows the alert itself when a reminder fires", () => {
    expect(wantsChip(ev("a", 1), false, true)).toBe(true);
    expect(wantsChip(ev("a", 1, { display: { countdown: false } }), false, true)).toBe(true);
  });
});

describe("the chip stack", () => {
  it("shows the soonest first and sums up the rest", () => {
    const list = [ev("c", 300), ev("a", 100), ev("d", 400), ev("b", 200), ev("e", 500)];
    const { shown, extra } = visibleChips(list);
    expect(shown.map((e) => e.id)).toEqual(["a", "b", "c"]);
    expect(shown).toHaveLength(MAX_CHIPS);
    expect(extra).toBe(2);
    expect(visibleChips([]).extra).toBe(0);
  });

  it("does not change the list it is given", () => {
    const list = [ev("b", 2), ev("a", 1)];
    visibleChips(list);
    expect(list.map((e) => e.id)).toEqual(["b", "a"]);
  });
});

describe("what Rust sends", () => {
  it("accepts a good event and fills what is missing", () => {
    const e = parseReminder({ id: "x", title: "Tea", atMs: 5, kind: "timer" });
    expect(e).toMatchObject({ id: "x", title: "Tea", atMs: 5, kind: "timer", spoken: "", alert: "Tea" });
  });

  it("refuses anything that would blank the stage", () => {
    for (const bad of [null, undefined, 5, "x", {}, { id: "x" }, { id: "", title: "t", atMs: 1 }, { id: "x", title: "t", atMs: "soon" }, { id: "x", title: "t", atMs: NaN }]) {
      expect(parseReminder(bad), JSON.stringify(bad)).toBeNull();
    }
    expect(parseReminders("nope")).toEqual([]);
    expect(parseReminders([{ id: "x", title: "t", atMs: 1 }, 7, null])).toHaveLength(1);
  });

  it("reads the fire notice", () => {
    expect(parseFired({ id: "x", title: "Tea", alert: "Your tea is ready." })).toEqual({ id: "x", title: "Tea", alert: "Your tea is ready." });
    expect(parseFired({ id: "x" })).toEqual({ id: "x", title: "", alert: "Reminder" });
    expect(parseFired({})).toBeNull();
    expect(parseFired(null)).toBeNull();
  });
});

describe("where the chips hang", () => {
  const SCREENS: [number, number, number][] = [
    [1366, 768, 1],
    [1920, 1080, 1],
    [1920, 1080, 1.25],
    [2560, 1440, 1],
    [2560, 1440, 1.5],
  ];

  it("sits to the right of the orb capsule, level with it", () => {
    const orb: Rect = { x: 885, y: 0, w: 150, h: 30 };
    const s = reminderStackSlot(orb, { w: 1920, h: 1080 });
    expect(s.left).toBe(885 + 150 + 12);
    expect(s.top).toBe(0);
  });

  it("falls back to the centre until the orb is known, and stays on screen", () => {
    expect(reminderStackSlot(null, { w: 1920, h: 1080 }).left).toBe(960 + 75 + 12);
    const far = reminderStackSlot({ x: 1800, y: 0, w: 150, h: 30 }, { w: 1920, h: 1080 });
    expect(far.left).toBeLessThanOrEqual(1920 - 160);
    expect(far.maxWidth).toBeGreaterThanOrEqual(160);
  });

  it("never overlaps the caption or the task stack on the common screens", () => {
    for (const [pw, ph, scale] of SCREENS) {
      const vp = { w: Math.round(pw / scale), h: Math.round(ph / scale) };
      const orb: Rect = { x: vp.w / 2 - 75, y: 0, w: 150, h: 30 };
      const slot = reminderStackSlot(orb, vp);
      // three chips of the largest size, stacked with the 6 px gap
      const stack: Rect = { x: slot.left, y: slot.top, w: slot.maxWidth, h: 3 * 32 + 2 * 6 };
      expect(intersects(stack, captionSlot(vp)), `${pw}x${ph}@${scale} caption`).toBe(false);
      const spinner: Rect = { x: vp.w - 104, y: 24, w: 80, h: 80 };
      const t = taskStackSlot(spinner, vp);
      const tasks: Rect = { x: vp.w - t.right - 320, y: t.top, w: 320, h: 3 * 32 + 12 };
      expect(intersects(stack, tasks), `${pw}x${ph}@${scale} task stack`).toBe(false);
    }
  });
});
