import type { SvIconName } from "./kit/system";

/**
 * Which kind of content the assistant view is showing, and the header that goes with it (icon, title, subtitle).
 * Pure, so the choice is tested; SidebarApp feeds it what it already has.
 */
export type AssistantKind = "conflict" | "confirmation" | "event" | "pr-review" | "pr-list" | "repo" | "research" | "answer" | "whatsapp" | "mail" | "screentime";

/** Kinds that render their own full view (with their own header) from `analysis.kind`. */
export const OWN_VIEW_KINDS: readonly AssistantKind[] = ["whatsapp", "mail", "screentime"];

export interface AssistantInput {
  conflict: boolean;
  confirmation: boolean;
  /** The analysis payload, whatever it is (or null). */
  analysis: unknown;
  /** The CodeRabbit review payload (or null). */
  codeRabbit: unknown;
  /** True when the request reads as research / analyse / explain / search and has a topic. */
  research: boolean;
}

function asRecord(v: unknown): Record<string, unknown> | null {
  return v && typeof v === "object" ? (v as Record<string, unknown>) : null;
}

export function assistantKind(i: AssistantInput): AssistantKind {
  if (i.conflict) return "conflict";
  if (i.confirmation) return "confirmation";
  const a = asRecord(i.analysis);
  if (a?.kind === "whatsapp" || a?.kind === "mail" || a?.kind === "screentime") return a.kind;
  if (a?.mode === "event_detail") return "event";
  if (asRecord(i.codeRabbit)) return "pr-review";
  if (a && (a.findings || a.activePrs || a.mode === "pr_list")) {
    return Array.isArray(a.findings) ? "pr-review" : "pr-list";
  }
  if (a) return "repo";
  return i.research ? "research" : "answer";
}

export interface AssistantHeader {
  icon: SvIconName;
  title: string;
  subtitle?: string;
}

export function assistantHeader(kind: AssistantKind, extra: { topic?: string; repo?: string; prNumber?: number; app?: string } = {}): AssistantHeader {
  switch (kind) {
    case "conflict":
      return { icon: "alert", title: "Merge conflict", subtitle: extra.repo };
    case "confirmation":
      return { icon: "shield", title: "Approval needed", subtitle: extra.app };
    case "event":
      return { icon: "info", title: "Alert", subtitle: extra.topic };
    case "pr-review":
      return { icon: "pr", title: extra.prNumber ? `PR #${extra.prNumber}` : "PR analysis", subtitle: extra.repo };
    case "pr-list":
      return { icon: "pr", title: "Pull requests", subtitle: extra.repo };
    case "repo":
      return { icon: "repo", title: extra.repo ? extra.repo.split("/").pop() || "Repository" : "Repository", subtitle: extra.repo };
    case "research":
      return { icon: "globe", title: "Research", subtitle: extra.topic };
    case "whatsapp":
      return { icon: "chat", title: "WhatsApp", subtitle: extra.topic };
    case "mail":
      return { icon: "file", title: "Gmail", subtitle: "Important mail" };
    case "screentime":
      return { icon: "monitor", title: "Screen time", subtitle: "This laptop" };
    default:
      return { icon: "sparkle", title: "Answer", subtitle: "Nexus" };
  }
}
