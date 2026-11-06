/**
 * Command Hub: Alerts Page Model & Helpers
 * Pure data transformations for alerts list, grouping, relative time, and privacy settings.
 */

export type AlertStatus = "failed" | "recovered" | "info";
export type NotifyPrivacy = "normal" | "private" | "hidden";

export interface AlertItem {
  eventId: string;
  provider: string;
  icon: string;
  status: AlertStatus;
  title: string;
  preview: string;
  updatedAtMs: number;
  read: boolean;
  spoken: boolean;
  count: number;
}

export type AlertSettingKey = "notifyToast" | "notifyNative" | "notifyUrgent" | "notifyPrivacy";

export interface AlertSettings {
  notifyToast: boolean;
  notifyNative: boolean;
  notifyUrgent: boolean;
  notifyPrivacy: NotifyPrivacy;
}

export const DEFAULT_ALERT_SETTINGS: AlertSettings = {
  notifyToast: true,
  notifyNative: true,
  notifyUrgent: true,
  notifyPrivacy: "private",
};

export interface DayGroup {
  day: string;
  items: AlertItem[];
}

/**
 * Validates and normalizes an unknown object into an AlertItem.
 * Rejects malformed objects (returns null). Never throws under any input.
 */
export function normalizeAlert(raw: unknown): AlertItem | null {
  try {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) {
      return null;
    }
    const o = raw as Record<string, unknown>;

    // eventId must be a non-empty string
    if (typeof o.eventId !== "string" || !o.eventId.trim()) {
      return null;
    }
    const eventId = o.eventId.trim();

    // title must be a string
    if (typeof o.title !== "string") {
      return null;
    }
    const title = o.title.trim();

    // updatedAtMs must be a positive finite number
    if (typeof o.updatedAtMs !== "number" || !Number.isFinite(o.updatedAtMs) || o.updatedAtMs <= 0) {
      return null;
    }
    const updatedAtMs = o.updatedAtMs;

    const provider = typeof o.provider === "string" && o.provider.trim() ? o.provider.trim() : "NEXUS";
    const icon = typeof o.icon === "string" ? o.icon.trim() : "";
    const status: AlertStatus =
      o.status === "failed" || o.status === "recovered" || o.status === "info" ? o.status : "info";
    const preview = typeof o.preview === "string" ? o.preview : "";
    const read = Boolean(o.read);
    const spoken = Boolean(o.spoken);
    const count =
      typeof o.count === "number" && Number.isFinite(o.count) && o.count >= 1 ? Math.floor(o.count) : 1;

    return {
      eventId,
      provider,
      icon,
      status,
      title,
      preview,
      updatedAtMs,
      read,
      spoken,
      count,
    };
  } catch {
    return null;
  }
}

/**
 * Returns user-facing relative time string like "just now", "2 min ago", "1 hour ago", "1 day ago".
 * Handles clock skew / future timestamps gracefully.
 */
export function relativeTime(updatedAtMs: number, nowMs: number = Date.now()): string {
  if (!Number.isFinite(updatedAtMs) || updatedAtMs <= 0) {
    return "just now";
  }

  const diffMs = nowMs - updatedAtMs;
  if (diffMs < 45_000) {
    return "just now";
  }
  if (diffMs < 90_000) {
    return "1 min ago";
  }
  if (diffMs < 55 * 60_000) {
    const mins = Math.round(diffMs / 60_000);
    return `${mins} min ago`;
  }
  if (diffMs < 90 * 60_000) {
    return "1 hour ago";
  }
  if (diffMs < 23 * 3_600_000) {
    const hours = Math.round(diffMs / 3_600_000);
    return `${hours} hours ago`;
  }
  if (diffMs < 36 * 3_600_000) {
    return "1 day ago";
  }
  const days = Math.floor(diffMs / 86_400_000);
  return `${days} days ago`;
}

/**
 * Groups alerts into calendar days ("Today", "Yesterday", or formatted date).
 * Preserves the order of arrival within each day group.
 */
export function groupByDay(items: AlertItem[], now: Date | number = Date.now()): DayGroup[] {
  if (!Array.isArray(items) || items.length === 0) {
    return [];
  }

  const nowDate = typeof now === "number" ? new Date(now) : now;
  const todayMidnight = new Date(nowDate.getFullYear(), nowDate.getMonth(), nowDate.getDate()).getTime();
  const yesterdayMidnight = todayMidnight - 86_400_000;

  const groupsMap = new Map<string, AlertItem[]>();

  for (const item of items) {
    const itemDate = new Date(item.updatedAtMs);
    const itemMidnight = new Date(itemDate.getFullYear(), itemDate.getMonth(), itemDate.getDate()).getTime();

    let dayLabel: string;
    if (itemMidnight === todayMidnight) {
      dayLabel = "Today";
    } else if (itemMidnight === yesterdayMidnight) {
      dayLabel = "Yesterday";
    } else if (itemDate.getFullYear() === nowDate.getFullYear()) {
      dayLabel = itemDate.toLocaleDateString("en-US", { month: "short", day: "numeric" });
    } else {
      dayLabel = itemDate.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
    }

    const existing = groupsMap.get(dayLabel);
    if (existing) {
      existing.push(item);
    } else {
      groupsMap.set(dayLabel, [item]);
    }
  }

  const result: DayGroup[] = [];
  for (const [day, groupItems] of groupsMap.entries()) {
    result.push({ day, items: groupItems });
  }
  return result;
}

/** Human label for privacy mode segmented choices */
export function privacyLabel(mode: NotifyPrivacy): string {
  switch (mode) {
    case "normal":
      return "Normal";
    case "private":
      return "Private";
    case "hidden":
      return "Hidden";
    default:
      return "Private";
  }
}

/** One-line explanation for each privacy setting mode */
export function privacyExplanation(mode: NotifyPrivacy): string {
  switch (mode) {
    case "normal":
      return "Full alert titles and previews are displayed.";
    case "private":
      return "Alert source and short title are shown; preview text is hidden.";
    case "hidden":
      return "All details masked; shows generic provider notice only.";
    default:
      return "Alert source and short title are shown; preview text is hidden.";
  }
}

/** Normalizes unknown privacy value safely */
export function normalizePrivacy(raw: unknown): NotifyPrivacy {
  if (raw === "normal" || raw === "hidden" || raw === "private") {
    return raw;
  }
  return "private";
}
