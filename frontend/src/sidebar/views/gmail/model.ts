/**
 * Gmail review view ("Inbox briefing"): important mail the watcher filed, grouped by what it is about, deadlines first.
 * Payload from Rust (`run_mail_digest`, `analysis.kind === "mail"`). Nothing here sends, archives or opens mail.
 */
export interface MailRow {
  sender: string;
  subject: string;
  deadline?: string | null;
  category?: string | null;
  explain?: string | null;
}

export interface MailPayload {
  kind: "mail";
  items: MailRow[];
}

export function isMailPayload(v: unknown): v is MailPayload {
  if (!v || typeof v !== "object") return false;
  const p = v as Partial<MailPayload>;
  return p.kind === "mail" && Array.isArray(p.items);
}

/** Display order of the groups: the ones that cost the most if missed come first. */
export const CATEGORY_ORDER = ["exam", "deadline", "database", "github", "hackathon", "internship", "other"] as const;

const LABELS: Record<string, string> = {
  exam: "Exams",
  deadline: "Deadlines",
  database: "Database alerts",
  github: "GitHub",
  hackathon: "Hackathons",
  internship: "Internships",
  other: "Other",
};

export function categoryKey(c?: string | null): string {
  const k = (c ?? "").trim().toLowerCase();
  return (CATEGORY_ORDER as readonly string[]).includes(k) ? k : "other";
}

export function categoryLabel(c?: string | null): string {
  return LABELS[categoryKey(c)];
}

export interface MailGroup {
  key: string;
  label: string;
  items: MailRow[];
}

/** Groups in CATEGORY_ORDER (empty ones left out); inside a group, mail with a deadline first, original order otherwise. */
export function groupMail(items: MailRow[]): MailGroup[] {
  const buckets = new Map<string, MailRow[]>();
  for (const it of items) {
    const k = categoryKey(it.category);
    const list = buckets.get(k) ?? [];
    list.push(it);
    buckets.set(k, list);
  }
  const out: MailGroup[] = [];
  for (const key of CATEGORY_ORDER) {
    const list = buckets.get(key);
    if (!list?.length) continue;
    const withDeadline = list.filter((m) => hasDeadline(m));
    const without = list.filter((m) => !hasDeadline(m));
    out.push({ key, label: LABELS[key], items: [...withDeadline, ...without] });
  }
  return out;
}

export function hasDeadline(m: MailRow): boolean {
  return !!m.deadline && m.deadline.trim().length > 0;
}

export function deadlineCount(items: MailRow[]): number {
  return items.filter(hasDeadline).length;
}

/** "Name <a@b.com>" -> "Name"; a bare address stays as it is. */
export function senderName(sender: string): string {
  const s = sender.trim();
  const m = s.match(/^"?([^"<]+?)"?\s*<[^>]+>$/);
  return (m ? m[1] : s).trim() || s;
}

/** What to show under the subject: the stored one-sentence explanation, else nothing. */
export function summaryLine(m: MailRow): string {
  return (m.explain ?? "").trim();
}
