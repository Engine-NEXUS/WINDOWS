// Pure helpers for the "Actions & Permissions" Hub page (Action Kernel, docs/features/140).
// Everything the page shows or validates is decided here so it can be tested without a window.

export type Mode = "smart" | "ask" | "never_destructive";
export type Effect = "deny" | "ask" | "allow";
export type AgentApproval = "edit" | "readonly" | "standard" | "permissive";

export const AGENT_APPROVALS: AgentApproval[] = ["edit", "readonly", "standard", "permissive"];

export function agentApprovalLabel(a: AgentApproval): string {
  switch (a) {
    case "edit":
      return "Edit files only (default): reads and edits files in the project, cannot run commands or browse";
    case "readonly":
      return "Read only: can look at the project but not change it";
    case "standard":
      return "Ask first: stops and asks before editing or running anything";
    case "permissive":
      return "Edit freely, ask before running commands";
  }
}

export interface Rule {
  effect: Effect;
  tool: string;
  pattern: string;
}

export interface Policy {
  version: number;
  enabled: boolean;
  mode: Mode;
  use_default_roots: boolean;
  extra_roots: string[];
  protected_paths: string[];
  rules: Rule[];
  max_items: number;
  max_folder_files: number;
  quarantine_days: number;
  audit_days: number;
  /** What a coding agent started by voice may do (never autonomous). */
  agent_approval: AgentApproval;
}

export interface PolicyView {
  policy: Policy;
  /** Folders the voice layer may work in right now (canonical, existing). */
  roots: string[];
}

export interface AuditRow {
  ts: number;
  phrase: string;
  tool: string;
  risk: string;
  origin: string;
  decision: string;
  outcome: string;
  detail: string;
}

export interface TrashRow {
  id: string;
  original: string;
  stored: string;
  is_dir: boolean;
  bytes: number;
  items: number;
  at: number;
}

export const TOOLS = ["*", "delete", "move", "copy", "rename", "create_folder"] as const;

export function modeLabel(m: Mode): string {
  switch (m) {
    case "smart":
      return "Smart: instant for reading and undoable actions, asks before deleting";
    case "ask":
      return "Ask first: confirm everything except reading";
    case "never_destructive":
      return "Never delete: deleting is refused";
  }
}

export function effectLabel(e: Effect): string {
  return e === "deny" ? "Never" : e === "ask" ? "Always ask" : "Allow";
}

export function ruleLabel(r: Rule): string {
  const what = r.tool === "*" ? "any action on" : `${r.tool.replace("_", " ")} for`;
  return `${effectLabel(r.effect)}: ${what} ${r.pattern}`;
}

/** The pattern a user typed: at least a letter or digit, no control characters, sane length. */
export function validPattern(p: string): boolean {
  const t = p.trim();
  return t.length > 0 && t.length <= 200 && /[A-Za-z0-9]/.test(t) && !/[\u0000-\u001f]/.test(t);
}

/** A folder to add: a full Windows (X:\...) or POSIX path that is not a drive root. */
export function validRoot(p: string): boolean {
  const t = p.trim();
  if (/^[A-Za-z]:\\?$/.test(t) || t === "/" || t === "") return false;
  if (t.startsWith("\\\\")) return false; // network share
  return /^[A-Za-z]:\\.+/.test(t) || (t.startsWith("/") && t.length > 1);
}

export function humanBytes(b: number): string {
  if (b < 1024) return `${b} B`;
  const units = ["KB", "MB", "GB"];
  let v = b / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function trashLabel(t: TrashRow): string {
  const size = t.is_dir ? `${t.items} file${t.items === 1 ? "" : "s"}` : humanBytes(t.bytes);
  return `${baseName(t.original)}${t.is_dir ? "/" : ""} (${size})`;
}

export function purgeInDays(t: TrashRow, nowSecs: number, keepDays: number): number {
  const left = Math.ceil((t.at + keepDays * 86400 - nowSecs) / 86400);
  return Math.max(0, left);
}

export function auditSummary(a: AuditRow): string {
  const what = a.tool.replace("_", " ");
  if (a.decision === "deny") return `Refused ${what}${a.detail ? `: ${a.detail}` : ""}`;
  if (a.decision === "declined") return `You declined ${what}`;
  if (a.outcome === "verified") return `Did ${what}${a.detail ? `: ${a.detail}` : ""}`;
  if (a.outcome === "planned") return a.decision === "ask" ? `Asked before ${what}` : `Checked ${what}`;
  return `${what}: ${a.outcome}`;
}

export function fmtWhen(ts: number): string {
  const d = new Date(ts * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${two(d.getDate())}/${two(d.getMonth() + 1)} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

/** Edits are clamped to the same limits the Rust side enforces. */
export function clampPolicy(p: Policy): Policy {
  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, Math.round(Number.isFinite(v) ? v : lo)));
  return {
    ...p,
    max_items: clamp(p.max_items, 1, 200),
    max_folder_files: clamp(p.max_folder_files, 1, 100000),
    quarantine_days: clamp(p.quarantine_days, 1, 365),
    audit_days: clamp(p.audit_days, 7, 3650),
  };
}
