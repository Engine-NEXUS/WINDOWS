// Pure helpers for the CodeRabbit report shown in the alert view (no DOM, no Tauri).

export type Sev = "critical" | "major" | "minor" | "trivial" | "info" | "unknown";
export type Kind = "potentialIssue" | "refactor" | "nitpick" | "unknown";

export interface ReportFinding {
  id: string;
  severity: Sev;
  kind: Kind;
  kindInferred: boolean;
  file: string;
  startLine?: number | null;
  endLine?: number | null;
  title: string;
  comment: string;
  suggestions: string[];
  agentInstructions?: string | null;
}

export interface Usage {
  state: string;
  limit: number;
  remaining: number;
  billingStatus: string;
  resetsAt?: string | null;
  /** Milliseconds until the whole free allowance is back (CodeRabbit's own figure; 0 when full). */
  fullCapacityMs?: number;
}

export interface Report {
  source: "githubBot" | "remoteReview" | "none";
  summary: string;
  findings: ReportFinding[];
  promptForAgents?: string | null;
  gate?: { threshold?: string | null; status?: string | null; blocking?: number | null } | null;
  unreviewedFiles: number;
  reused: boolean;
  notes: string[];
  usage?: Usage | null;
}

/** CodeRabbit's own severity order, most serious first. `unknown` is shown last, never guessed. */
export const SEVERITY_ORDER: Sev[] = ["critical", "major", "minor", "trivial", "info", "unknown"];

export function severityCounts(findings: ReportFinding[]): Record<Sev, number> {
  const c: Record<Sev, number> = { critical: 0, major: 0, minor: 0, trivial: 0, info: 0, unknown: 0 };
  for (const f of findings) c[SEVERITY_ORDER.includes(f.severity) ? f.severity : "unknown"] += 1;
  return c;
}

export function filterFindings(findings: ReportFinding[], sev: Sev | "all"): ReportFinding[] {
  return sev === "all" ? findings : findings.filter((f) => f.severity === sev);
}

/** Findings grouped by severity in CodeRabbit order; empty groups are left out. */
export function groupBySeverity(findings: ReportFinding[]): { severity: Sev; items: ReportFinding[] }[] {
  return SEVERITY_ORDER.map((severity) => ({ severity, items: findings.filter((f) => f.severity === severity) })).filter((g) => g.items.length > 0);
}

const KIND_LABEL: Record<Kind, string> = {
  potentialIssue: "Potential issue",
  refactor: "Refactor suggestion",
  nitpick: "Nitpick",
  unknown: "",
};

/** CodeRabbit's type, with "(inferred)" when NEXUS derived it (the CLI states none); empty when unknown. */
export function kindLabel(kind: Kind, inferred: boolean): string {
  const base = KIND_LABEL[kind] ?? "";
  return base && inferred ? `${base} (inferred)` : base;
}

export function sourceLabel(source: Report["source"]): string {
  if (source === "githubBot") return "CodeRabbit review posted on GitHub";
  if (source === "remoteReview") return "CodeRabbit free review (run by NEXUS)";
  return "Not reviewed by CodeRabbit";
}

export function usageLine(u?: Usage | null): string | null {
  if (!u || u.state !== "available") return null;
  return `${u.remaining} of ${u.limit} free CodeRabbit reviews left this hour`;
}

export function gateLine(g?: Report["gate"]): string | null {
  if (!g || !g.status) return null;
  const at = g.threshold ? ` at ${g.threshold}` : "";
  const n = typeof g.blocking === "number" && g.blocking > 0 ? ` (${g.blocking} blocking)` : "";
  return `Gate ${g.status}${at}${n}`;
}

export function lineRange(f: Pick<ReportFinding, "startLine" | "endLine">): string {
  if (f.startLine == null) return "";
  return f.endLine != null && f.endLine !== f.startLine ? `${f.startLine}-${f.endLine}` : `${f.startLine}`;
}

/** Text to paste into an AI coding agent: CodeRabbit's consolidated prompt if present, else one block per finding
 *  using CodeRabbit's own fix instructions (or its comment). Empty when there is nothing to fix. */
export function agentPrompt(r: Report): string {
  if (r.promptForAgents && r.promptForAgents.trim()) return r.promptForAgents.trim();
  return r.findings
    .map((f) => {
      const loc = f.file ? `${f.file}${lineRange(f) ? `:${lineRange(f)}` : ""}` : "";
      const body = (f.agentInstructions && f.agentInstructions.trim()) || f.comment.trim();
      return `${loc ? `${loc} - ` : ""}${f.title}\n${body}`.trim();
    })
    .join("\n\n");
}

/** "50 min", "1 min", "under a minute" (never rounds a wait down to nothing). Mirrors the Rust wording. */
export function minutesText(ms: number): string {
  if (ms <= 0) return "under a minute";
  return `${Math.max(1, Math.ceil(ms / 60000))} min`;
}

/** "7 of 10 free reviews left" plus, while below the limit, when the allowance is back to full. Null when unknown. */
export function allowanceLine(u?: Usage | null): string | null {
  if (!u || (u.state !== "available" && u.state !== "exhausted")) return null;
  const back = u.remaining < u.limit && (u.fullCapacityMs ?? 0) > 0 ? ` · back to ${u.limit} in ~${minutesText(u.fullCapacityMs ?? 0)}` : "";
  return `${u.remaining} of ${u.limit} free reviews left${back}`;
}

/** The inline question before a free review is spent. */
export function confirmLine(u: Usage): string {
  return `Use 1 of your ${u.remaining} free reviews?`;
}

/** Only a free review that is available and has some left may be offered. */
export function canReview(u?: Usage | null): boolean {
  return !!u && u.state === "available" && u.remaining > 0;
}

/** "2:05" style elapsed time of a running review. */
export function elapsedText(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
