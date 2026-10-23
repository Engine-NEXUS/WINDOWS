import { describe, expect, it } from "vitest";
import {
  activitySummary,
  ageLabel,
  categoryLabel,
  describeCorrection,
  describePending,
  fmtDays,
  fmtSlotTime,
  fmtTime,
  groupRows,
  keyLabel,
  leaveBySummary,
  mailSummary,
  routineUse,
  selftestSummary,
  sourceLabel,
  stepsLine,
  whatsappSummary,
  type CorrectionRow,
  type MemoryRow,
  type PendingRow,
  type RoutineRow,
  type WhatsappSelfTest,
} from "./memoryModel";

describe("leaveBySummary boundaries", () => {
  it("returns Off message when leave_by is false", () => {
    expect(leaveBySummary({ leave_by: false }, true)).toContain("Off");
  });

  it("lists missing dependencies when leave_by is enabled", () => {
    // Missing both google and routes key
    const msgBoth = leaveBySummary({ leave_by: true, google_connected: false }, false);
    expect(msgBoth).toContain("connected Google account");
    expect(msgBoth).toContain("Google Maps & Routes API key");
    expect(msgBoth).toContain("home address");

    // Has Google, missing routes key
    const msgRoutes = leaveBySummary({ leave_by: true, google_connected: true }, false);
    expect(msgRoutes).not.toContain("connected Google account");
    expect(msgRoutes).toContain("Google Maps & Routes API key");

    // Has both, only mentions home address
    const msgReady = leaveBySummary({ leave_by: true, google_connected: true }, true);
    expect(msgReady).not.toContain("connected Google account");
    expect(msgReady).not.toContain("Google Maps & Routes API key");
    expect(msgReady).toContain("your home address");
  });
});

describe("whatsappSummary boundaries", () => {
  it("formats state when off", () => {
    expect(whatsappSummary({ whatsapp: false })).toContain("Off — NEXUS is not reading WhatsApp");
  });

  it("handles 0, 1 (singular), and multiple (plural) learned people", () => {
    expect(whatsappSummary({ whatsapp: true, people: 0 })).toContain("still learning");
    expect(whatsappSummary({ whatsapp: true, people: 1 })).toContain("1 person learned");
    expect(whatsappSummary({ whatsapp: true, people: 5 })).toContain("5 people learned");
  });
});

describe("selftestSummary boundaries", () => {
  const baseTest: WhatsappSelfTest = {
    reachable: true,
    paired: true,
    tools: 5,
    has_read_tools: true,
    guarded_tools_present: [],
    chats_parsed: 10,
    format_ok: true,
    note: "All systems green",
  };

  it("returns note when unreachable or unpaired", () => {
    expect(selftestSummary({ ...baseTest, reachable: false, note: "Server down" })).toBe("Server down");
    expect(selftestSummary({ ...baseTest, paired: false, note: "Need QR scan" })).toBe("Need QR scan");
  });

  it("warns when read tools are missing", () => {
    expect(selftestSummary({ ...baseTest, has_read_tools: false })).toContain("does not offer the chat-reading tools");
  });

  it("handles format failure with custom note", () => {
    expect(selftestSummary({ ...baseTest, format_ok: false, note: "Bad JSON" })).toContain("could not understand the chat list: Bad JSON");
  });

  it("formats singular vs plural guarded tools refusal", () => {
    const single = selftestSummary({ ...baseTest, guarded_tools_present: ["receipt"] });
    expect(single).toContain("1 receipt/presence tool;");
    expect(single).toContain("refuses to call it.");

    const plural = selftestSummary({ ...baseTest, guarded_tools_present: ["receipt", "presence"] });
    expect(plural).toContain("2 receipt/presence tools;");
    expect(plural).toContain("refuses to call them.");
  });
});

describe("mailSummary boundaries", () => {
  it("handles disabled state", () => {
    expect(mailSummary({ mail: false, google_connected: true, mail_items: 2 })).toContain("Off");
  });

  it("prioritizes sign-in required error", () => {
    expect(mailSummary({ mail: true, google_connected: true, mail_items: 2, google_needs_signin: true })).toContain("Paused — Google needs you to sign in again");
  });

  it("warns if Google is not connected", () => {
    expect(mailSummary({ mail: true, google_connected: false, mail_items: 0 })).toContain("no Google account is connected");
  });

  it("handles 0, 1 (singular), and multiple (plural) emails", () => {
    expect(mailSummary({ mail: true, google_connected: true, mail_items: 0 })).toContain("nothing important");
    expect(mailSummary({ mail: true, google_connected: true, mail_items: 1 })).toContain("1 important email filed");
    expect(mailSummary({ mail: true, google_connected: true, mail_items: 8 })).toContain("8 important emails filed");
  });
});

describe("categoryLabel boundaries", () => {
  it("maps known categories and passes unknown through unmodified", () => {
    expect(categoryLabel("exam")).toBe("Exam");
    expect(categoryLabel("hackathon")).toBe("Hackathon");
    expect(categoryLabel("github")).toBe("GitHub");
    expect(categoryLabel("database")).toBe("Database");
    expect(categoryLabel("deadline")).toBe("Deadline");
    expect(categoryLabel("internship")).toBe("Internship");
    expect(categoryLabel("custom_category")).toBe("custom_category");
  });
});

describe("fmtTime and fmtDays boundaries", () => {
  it("formats 24-hour minutes into 12-hour AM/PM with padding", () => {
    expect(fmtTime(0)).toBe("12 AM");
    expect(fmtTime(5)).toBe("12:05 AM");
    expect(fmtTime(60)).toBe("1 AM");
    expect(fmtTime(720)).toBe("12 PM");
    expect(fmtTime(750)).toBe("12:30 PM");
    expect(fmtTime(1439)).toBe("11:59 PM");
  });

  it("formats day collections including every day, weekdays, weekends, and subsets", () => {
    expect(fmtDays([0, 1, 2, 3, 4, 5, 6])).toBe("every day");
    expect(fmtDays([0, 1, 2, 3, 4])).toBe("weekdays");
    expect(fmtDays([5, 6])).toBe("weekends");
    expect(fmtDays([4, 0, 2])).toBe("Mon, Wed, Fri"); // Sorts correctly
  });

  it("formats slot times with and without end time", () => {
    expect(fmtSlotTime({ start_min: 600, end_min: 660 })).toBe("10 AM – 11 AM");
    expect(fmtSlotTime({ start_min: 600, end_min: null })).toBe("10 AM");
    expect(fmtSlotTime({ start_min: 600 })).toBe("10 AM");
  });
});

describe("activitySummary and ageLabel boundaries", () => {
  it("formats activity summary states", () => {
    expect(activitySummary({ activity: false, resume_points: 5 })).toContain("Off");
    expect(activitySummary({ activity: true, resume_points: 0 })).toContain("nothing recorded");
    expect(activitySummary({ activity: true, resume_points: 1 })).toContain("1 point kept");
    expect(activitySummary({ activity: true, resume_points: 4 })).toContain("4 points kept");
  });

  it("computes relative age strings", () => {
    const now = 1_000_000;
    expect(ageLabel(now, now)).toBe("today");
    expect(ageLabel(now, now - 3600)).toBe("today");
    expect(ageLabel(now, now - 86400)).toBe("yesterday");
    expect(ageLabel(now, now - 86400 * 3)).toBe("3d ago");
    expect(ageLabel(now, now + 1000)).toBe("today"); // Future timestamp clamped
  });
});

describe("groupRows, sourceLabel, and keyLabel boundaries", () => {
  it("partitions memory rows into told, learned, and episodes", () => {
    const rows: MemoryRow[] = [
      { id: 1, tier: "fact", key: "k1", value: "v1", source: "user:voice", trust: "user_said", created: 0, last_seen: 0, pinned: true, uses: 1 },
      { id: 2, tier: "fact", key: "k2", value: "v2", source: "miner:chat", trust: "derived", created: 0, last_seen: 0, pinned: false, uses: 1 },
      { id: 3, tier: "episode", key: "k3", value: "v3", source: "turn", trust: "untrusted", created: 0, last_seen: 0, pinned: false, uses: 1 },
    ];
    const grouped = groupRows(rows);
    expect(grouped.told.map((r) => r.id)).toEqual([1]);
    expect(grouped.learned.map((r) => r.id)).toEqual([2]);
    expect(grouped.episodes.map((r) => r.id)).toEqual([3]);
  });

  it("maps source identifiers to readable text", () => {
    expect(sourceLabel("user:manual")).toBe("you told me");
    expect(sourceLabel("miner:voice")).toBe("picked up from a conversation");
    expect(sourceLabel("legacy:v1")).toBe("imported from earlier memory");
    expect(sourceLabel("turn")).toBe("conversation");
    expect(sourceLabel("unknown_source")).toBe("unknown_source");
  });

  it("formats key labels cleanly replacing underscores and hiding episode keys", () => {
    const fact: MemoryRow = { id: 1, tier: "fact", key: "favorite_coffee", value: "latte", source: "user", trust: "user_said", created: 0, last_seen: 0, pinned: true, uses: 1 };
    expect(keyLabel(fact)).toBe("favorite coffee");

    const episode: MemoryRow = { ...fact, tier: "episode", key: "raw_turn_hash" };
    expect(keyLabel(episode)).toBe("conversation");
  });
});

describe("describeCorrection, describePending, stepsLine, and routineUse", () => {
  it("describes mishearing corrections based on promotion status and count", () => {
    const c1: CorrectionRow = { heard: "turn off", meant: "turn on", count: 3, last_seen: 0, promoted: true };
    expect(describeCorrection(c1)).toBe("“turn off” → “turn on” · applied, seen 3×");

    const c2: CorrectionRow = { heard: "mute", meant: "play", count: 2, last_seen: 0, promoted: false };
    expect(describeCorrection(c2)).toBe("“mute” → “play” · not applied: two answers tie");

    const c3: CorrectionRow = { heard: "mute", meant: "pause", count: 1, last_seen: 0, promoted: false };
    expect(describeCorrection(c3)).toBe("“mute” → “pause” · seen once, waiting for a second time");
  });

  it("describes pending memory changes (add, patch, remove)", () => {
    const add: PendingRow = { id: 1, op: "add", key: "pet_name", value: "Rover", old_value: "", reason: "user said", created: 0 };
    expect(describePending(add)).toBe("Remember: pet name is Rover");

    const patch: PendingRow = { id: 2, op: "patch", key: "job_title", value: "Lead", old_value: "Senior", reason: "update", created: 0 };
    expect(describePending(patch)).toBe("Change job title from Senior to Lead");

    const remove: PendingRow = { id: 3, op: "remove", key: "temp_code", value: "", old_value: "1234", reason: "expired", created: 0 };
    expect(describePending(remove)).toBe("Forget: temp code (1234)");
  });

  it("formats routine steps and usage strings", () => {
    expect(stepsLine(["open code", "open browser", "play music"])).toBe("open code → open browser → play music");
    expect(stepsLine([])).toBe("");

    const r0: RoutineRow = { name: "morning", steps: [], created: 0, last_run: 0, runs: 0 };
    expect(routineUse(r0)).toBe("not used yet");

    const r1: RoutineRow = { ...r0, runs: 1 };
    expect(routineUse(r1)).toBe("used 1 time");

    const r5: RoutineRow = { ...r0, runs: 5 };
    expect(routineUse(r5)).toBe("used 5 times");
  });
});
