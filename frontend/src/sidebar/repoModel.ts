export interface LanguageShare {
  name: string;
  bytes: number;
  percentage: number;
}

export interface LanguageRow {
  name: string;
  percent: number;
  tone: "ink" | "mid" | "low";
}

/**
 * The language bars of the repository view: the largest five by share, the rest folded into "Other", the first bar ink, the
 * second mid grey and the others light grey. Shares are rounded to whole percents and never negative or over 100.
 */
export function languageRows(languages: LanguageShare[] | undefined | null, keep = 5): LanguageRow[] {
  const list = (languages ?? [])
    .filter((l) => l && typeof l.name === "string" && Number.isFinite(l.percentage) && l.percentage > 0)
    .sort((a, b) => b.percentage - a.percentage);
  const head = list.slice(0, keep);
  const restSum = list.slice(keep).reduce((s, l) => s + l.percentage, 0);
  const rows = head.map((l) => ({ name: l.name, percent: l.percentage }));
  if (restSum >= 0.5) rows.push({ name: "Other", percent: restSum });
  return rows.map((r, i) => ({
    name: r.name,
    percent: Math.min(100, Math.max(0, Math.round(r.percent))),
    tone: i === 0 ? "ink" : i === 1 ? "mid" : "low",
  }));
}

/** 1200 -> "1.2k", 2801 -> "2.8k", 950 -> "950". Anything not a finite number shows a dash. */
export function compactCount(n: unknown): string {
  if (typeof n !== "number" || !Number.isFinite(n) || n < 0) return "–";
  if (n < 1000) return String(Math.round(n));
  if (n < 10_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, "")}k`;
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`;
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
}
