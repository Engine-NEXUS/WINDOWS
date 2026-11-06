import { describe, expect, it } from "vitest";
import {
  DEFAULT_ALERT_SETTINGS,
  groupByDay,
  normalizeAlert,
  normalizePrivacy,
  privacyExplanation,
  privacyLabel,
  relativeTime,
  type AlertItem,
} from "./alertsModel";

describe("alertsModel", () => {
  describe("normalizeAlert", () => {
    it("accepts a well-formed alert payload", () => {
      const raw = {
        eventId: "ev_123",
        provider: "GitHub",
        icon: "github",
        status: "failed",
        title: "Build failed in engine",
        preview: "main branch run 42",
        updatedAtMs: 1730000000000,
        read: false,
        spoken: true,
        count: 2,
      };

      const result = normalizeAlert(raw);
      expect(result).toEqual({
        eventId: "ev_123",
        provider: "GitHub",
        icon: "github",
        status: "failed",
        title: "Build failed in engine",
        preview: "main branch run 42",
        updatedAtMs: 1730000000000,
        read: false,
        spoken: true,
        count: 2,
      });
    });

    it("applies sane defaults for missing optional fields", () => {
      const raw = {
        eventId: "ev_999",
        title: "Deploy succeeded",
        updatedAtMs: 1730000000000,
      };

      const result = normalizeAlert(raw);
      expect(result).not.toBeNull();
      expect(result?.provider).toBe("NEXUS");
      expect(result?.icon).toBe("");
      expect(result?.status).toBe("info");
      expect(result?.preview).toBe("");
      expect(result?.read).toBe(false);
      expect(result?.spoken).toBe(false);
      expect(result?.count).toBe(1);
    });

    it("clamps invalid count to 1", () => {
      const raw = {
        eventId: "ev_1",
        title: "Notice",
        updatedAtMs: 1730000000000,
        count: -5,
      };
      expect(normalizeAlert(raw)?.count).toBe(1);

      const raw2 = {
        eventId: "ev_1",
        title: "Notice",
        updatedAtMs: 1730000000000,
        count: 3.8,
      };
      expect(normalizeAlert(raw2)?.count).toBe(3);
    });

    it("rejects malformed items without throwing", () => {
      expect(normalizeAlert(null)).toBeNull();
      expect(normalizeAlert(undefined)).toBeNull();
      expect(normalizeAlert("")).toBeNull();
      expect(normalizeAlert(42)).toBeNull();
      expect(normalizeAlert([])).toBeNull();
      expect(normalizeAlert({})).toBeNull();

      // missing / empty eventId
      expect(normalizeAlert({ title: "Hi", updatedAtMs: 1000 })).toBeNull();
      expect(normalizeAlert({ eventId: "   ", title: "Hi", updatedAtMs: 1000 })).toBeNull();
      expect(normalizeAlert({ eventId: 123, title: "Hi", updatedAtMs: 1000 })).toBeNull();

      // missing / non-string title
      expect(normalizeAlert({ eventId: "ev_1", updatedAtMs: 1000 })).toBeNull();
      expect(normalizeAlert({ eventId: "ev_1", title: null, updatedAtMs: 1000 })).toBeNull();

      // invalid updatedAtMs
      expect(normalizeAlert({ eventId: "ev_1", title: "Hi", updatedAtMs: 0 })).toBeNull();
      expect(normalizeAlert({ eventId: "ev_1", title: "Hi", updatedAtMs: -500 })).toBeNull();
      expect(normalizeAlert({ eventId: "ev_1", title: "Hi", updatedAtMs: NaN })).toBeNull();
      expect(normalizeAlert({ eventId: "ev_1", title: "Hi", updatedAtMs: Infinity })).toBeNull();
      expect(normalizeAlert({ eventId: "ev_1", title: "Hi", updatedAtMs: "1730000000000" })).toBeNull();
    });
  });

  describe("relativeTime", () => {
    const baseNow = 1730000000000;

    it("formats recent intervals accurately", () => {
      expect(relativeTime(baseNow, baseNow)).toBe("just now");
      expect(relativeTime(baseNow - 10_000, baseNow)).toBe("just now");
      expect(relativeTime(baseNow - 60_000, baseNow)).toBe("1 min ago");
      expect(relativeTime(baseNow - 120_000, baseNow)).toBe("2 min ago");
      expect(relativeTime(baseNow - 15 * 60_000, baseNow)).toBe("15 min ago");
      expect(relativeTime(baseNow - 60 * 60_000, baseNow)).toBe("1 hour ago");
      expect(relativeTime(baseNow - 4 * 3_600_000, baseNow)).toBe("4 hours ago");
      expect(relativeTime(baseNow - 24 * 3_600_000, baseNow)).toBe("1 day ago");
      expect(relativeTime(baseNow - 72 * 3_600_000, baseNow)).toBe("3 days ago");
    });

    it("handles clock skew or future times gracefully", () => {
      expect(relativeTime(baseNow + 5000, baseNow)).toBe("just now");
      expect(relativeTime(0, baseNow)).toBe("just now");
      expect(relativeTime(-100, baseNow)).toBe("just now");
    });
  });

  describe("groupByDay", () => {
    it("returns empty array for empty or non-array input", () => {
      expect(groupByDay([])).toEqual([]);
      expect(groupByDay(null as any)).toEqual([]);
    });

    it("groups items into Today, Yesterday, and formatted calendar dates", () => {
      // 2026-11-05 14:00:00 UTC
      const now = new Date(2026, 10, 5, 14, 0, 0);

      const todayItem: AlertItem = {
        eventId: "ev_today",
        provider: "GitHub",
        icon: "github",
        status: "failed",
        title: "Failed 1",
        preview: "error",
        updatedAtMs: new Date(2026, 10, 5, 10, 0, 0).getTime(),
        read: false,
        spoken: false,
        count: 1,
      };

      const yesterdayItem: AlertItem = {
        eventId: "ev_yesterday",
        provider: "Vercel",
        icon: "vercel",
        status: "recovered",
        title: "Deploy 1",
        preview: "ready",
        updatedAtMs: new Date(2026, 10, 4, 18, 0, 0).getTime(),
        read: true,
        spoken: true,
        count: 1,
      };

      const olderItem: AlertItem = {
        eventId: "ev_older",
        provider: "Gmail",
        icon: "gmail",
        status: "info",
        title: "Mail 1",
        preview: "hello",
        updatedAtMs: new Date(2026, 9, 28, 12, 0, 0).getTime(),
        read: true,
        spoken: false,
        count: 1,
      };

      const groups = groupByDay([todayItem, yesterdayItem, olderItem], now);
      expect(groups).toHaveLength(3);
      expect(groups[0].day).toBe("Today");
      expect(groups[0].items).toEqual([todayItem]);

      expect(groups[1].day).toBe("Yesterday");
      expect(groups[1].items).toEqual([yesterdayItem]);

      expect(groups[2].day).toBe("Oct 28");
      expect(groups[2].items).toEqual([olderItem]);
    });
  });

  describe("privacyLabel & privacyExplanation", () => {
    it("returns correct labels", () => {
      expect(privacyLabel("normal")).toBe("Normal");
      expect(privacyLabel("private")).toBe("Private");
      expect(privacyLabel("hidden")).toBe("Hidden");
    });

    it("returns one-line explanations for each mode", () => {
      expect(privacyExplanation("normal")).toMatch(/full alert/i);
      expect(privacyExplanation("private")).toMatch(/short title/i);
      expect(privacyExplanation("hidden")).toMatch(/masked/i);
    });

    it("normalizes unknown privacy safely", () => {
      expect(normalizePrivacy("normal")).toBe("normal");
      expect(normalizePrivacy("hidden")).toBe("hidden");
      expect(normalizePrivacy("private")).toBe("private");
      expect(normalizePrivacy("unknown")).toBe("private");
      expect(normalizePrivacy(undefined)).toBe("private");
    });

    it("DEFAULT_ALERT_SETTINGS defaults match requirements", () => {
      expect(DEFAULT_ALERT_SETTINGS.notifyToast).toBe(true);
      expect(DEFAULT_ALERT_SETTINGS.notifyNative).toBe(true);
      expect(DEFAULT_ALERT_SETTINGS.notifyUrgent).toBe(true);
      expect(DEFAULT_ALERT_SETTINGS.notifyPrivacy).toBe("private");
    });
  });
});
