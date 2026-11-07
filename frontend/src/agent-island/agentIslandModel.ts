/** Pure model of the Agent Island page (agent_monitor.rs / agent_island.rs send the data). No DOM, no Tauri. */

import type { EditView } from "./diff";

export type AgentState = "thinking" | "working" | "waiting" | "done";
export type IslandMode = "hidden" | "collapsed" | "expanded";
export type Tab = "tracking" | "approvals";

export interface StepView {
  tool: string;
  detail: string | null;
  state: "running" | "done";
}

export interface SessionView {
  key: string;
  agent: string;
  label: string;
  folder: string | null;
  state: AgentState;
  tool: string | null;
  /** The tool calls of the current turn, oldest first; the last one may still be running. */
  steps: StepView[];
  /** What the latest Edit / MultiEdit / Write is changing, for the live diff. */
  edit: EditView | null;
  startedMs: number;
  updatedMs: number;
}

export interface ApprovalView {
  id: number;
  agent: string;
  label: string;
  tool: string;
  summary: string;
  question: unknown;
  requestedMs: number;
  deadlineMs: number;
}

export interface Snapshot {
  running: number;
  atMs: number;
  sessions: SessionView[];
  approvals: ApprovalView[];
}

export interface ModePayload {
  mode: IslandMode;
  byApproval: boolean;
}

/** How an agent's logo is drawn on the island: the SVG and the tile it sits on. A mark is drawn in the vendor's own
 *  colours (white for Antigravity and Windsurf, near-black for Cursor, `currentColor` = black for Codex, Devin and
 *  Kilo), so the tile is chosen per mark: a dark tile for white marks, a light one for dark or colourful marks.
 *  A white mark on a white tile (or a dark one on a dark tile) is invisible; logos.test.ts reads the real SVG files
 *  and fails if a mark and its tile do not contrast. */
export interface AgentLook {
  src: string;
  tile: string;
}

const LIGHT_TILE = "#f4f4f6";
const DARK_TILE = "#1c1c21";

const LOOKS: Record<string, AgentLook> = {
  claude: { src: "/activity-logos/claude.svg", tile: DARK_TILE },
  codex: { src: "/activity-logos/codex.svg", tile: LIGHT_TILE },
  antigravity: { src: "/activity-logos/antigravity.svg", tile: DARK_TILE },
  opencode: { src: "/activity-logos/opencode.svg", tile: LIGHT_TILE },
  gemini: { src: "/activity-logos/gemini.svg", tile: LIGHT_TILE },
  cursor: { src: "/activity-logos/cursor.svg", tile: LIGHT_TILE },
  aider: { src: "/activity-logos/aider.svg", tile: "#14b014" },
  copilot: { src: "/activity-logos/copilot.svg", tile: LIGHT_TILE },
  windsurf: { src: "/activity-logos/windsurf.svg", tile: LIGHT_TILE },
  trae: { src: "/activity-logos/trae.svg", tile: DARK_TILE },
  kilo: { src: "/activity-logos/kilo.svg", tile: LIGHT_TILE },
  devin: { src: "/activity-logos/devin.svg", tile: LIGHT_TILE },
};

/** The logo and tile for an agent name, or null for an agent with no mark (the page then draws a letter). */
export function agentLook(agent: string): AgentLook | null {
  return LOOKS[agent.trim().toLowerCase()] ?? null;
}

/** Every agent that has a mark (for tests). */
export const AGENTS_WITH_LOGOS: readonly string[] = Object.keys(LOOKS);

export const EMPTY: Snapshot = { running: 0, atMs: 0, sessions: [], approvals: [] };

const STATES: AgentState[] = ["thinking", "working", "waiting", "done"];
const isObj = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null;
const str = (v: unknown, d = ""): string => (typeof v === "string" ? v : d);
const num = (v: unknown, d = 0): number => (typeof v === "number" && Number.isFinite(v) ? v : d);

function stepsOf(raw: unknown): StepView[] {
  if (!Array.isArray(raw)) return [];
  return raw
    .filter(isObj)
    .filter((x) => typeof x.tool === "string" && x.tool)
    .map((x): StepView => ({
      tool: x.tool as string,
      detail: typeof x.detail === "string" ? x.detail : null,
      state: x.state === "running" ? "running" : "done",
    }));
}

function editOf(raw: unknown): EditView | null {
  if (!isObj(raw) || typeof raw.path !== "string" || !raw.path) return null;
  const kind = raw.kind === "write" || raw.kind === "multi" ? raw.kind : "edit";
  const pairs = (Array.isArray(raw.pairs) ? raw.pairs : [])
    .filter(isObj)
    .map((p) => ({ old: str(p.old), new: str(p.new) }));
  return { path: raw.path, kind, pairs, tooLarge: raw.tooLarge === true };
}

/** Validate whatever arrives over IPC; anything malformed is dropped rather than rendered. */
export function asSnapshot(raw: unknown): Snapshot {
  if (!isObj(raw)) return EMPTY;
  const sessions = (Array.isArray(raw.sessions) ? raw.sessions : [])
    .filter(isObj)
    .map((s): SessionView | null => {
      const state = STATES.find((x) => x === s.state);
      if (!state || !str(s.key)) return null;
      return {
        key: str(s.key),
        agent: str(s.agent),
        label: str(s.label, str(s.agent)),
        folder: typeof s.folder === "string" ? s.folder : null,
        state,
        tool: typeof s.tool === "string" ? s.tool : null,
        steps: stepsOf(s.steps),
        edit: editOf(s.edit),
        startedMs: num(s.startedMs),
        updatedMs: num(s.updatedMs),
      };
    })
    .filter((s): s is SessionView => s !== null);
  const approvals = (Array.isArray(raw.approvals) ? raw.approvals : [])
    .filter(isObj)
    .map((a): ApprovalView | null => {
      if (typeof a.id !== "number") return null;
      return {
        id: a.id,
        agent: str(a.agent),
        label: str(a.label, str(a.agent)),
        tool: str(a.tool),
        summary: str(a.summary),
        question: a.question ?? null,
        requestedMs: num(a.requestedMs),
        deadlineMs: num(a.deadlineMs),
      };
    })
    .filter((a): a is ApprovalView => a !== null);
  return { running: num(raw.running), atMs: num(raw.atMs), sessions, approvals };
}

export function asMode(raw: unknown): ModePayload {
  const mode = isObj(raw) ? (["hidden", "collapsed", "expanded"] as const).find((m) => m === raw.mode) : undefined;
  return { mode: mode ?? "hidden", byApproval: isObj(raw) && raw.byApproval === true };
}

/** One logo per agent type: how many sessions it has and its most urgent state. */
export interface PillLogo {
  agent: string;
  label: string;
  count: number;
  state: AgentState;
}

const URGENCY: Record<AgentState, number> = { waiting: 3, working: 2, thinking: 1, done: 0 };

export function pillLogos(s: Snapshot): PillLogo[] {
  const byAgent = new Map<string, PillLogo>();
  for (const x of s.sessions) {
    const cur = byAgent.get(x.agent);
    if (!cur) {
      byAgent.set(x.agent, { agent: x.agent, label: x.label, count: 1, state: x.state });
    } else {
      cur.count += 1;
      if (URGENCY[x.state] > URGENCY[cur.state]) cur.state = x.state;
    }
  }
  return [...byAgent.values()];
}

/** The session the island's left rail and code pane are about: the one asked for, else the most urgent, else the
 *  most recently active. */
export function focusSession(s: Snapshot, wanted: string | null): SessionView | null {
  if (s.sessions.length === 0) return null;
  const picked = wanted ? s.sessions.find((x) => x.key === wanted) : undefined;
  if (picked) return picked;
  return [...s.sessions].sort((a, b) => URGENCY[b.state] - URGENCY[a.state] || b.updatedMs - a.updatedMs)[0];
}

/** Which tab to show: a permission request wins; otherwise tracking. */
export function defaultTab(s: Snapshot, byApproval: boolean): Tab {
  return byApproval || s.approvals.length > 0 ? "approvals" : "tracking";
}

export function stateWord(state: AgentState, tool: string | null): string {
  switch (state) {
    case "thinking":
      return "Thinking";
    case "working":
      return tool ? `Running ${tool}` : "Working";
    case "waiting":
      return "Waiting for you";
    case "done":
      return "Finished";
  }
}

/** "42s", "3m 05s", "1h 02m". */
export function elapsedLabel(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  return `${Math.floor(m / 60)}h ${String(m % 60).padStart(2, "0")}m`;
}

/** Whole seconds left before the agent is left to ask in its own terminal. */
export function secondsLeft(deadlineMs: number, nowMs: number): number {
  return Math.max(0, Math.ceil((deadlineMs - nowMs) / 1000));
}

export interface Question {
  question: string;
  options: string[];
  multi: boolean;
}

/** The questions of an AskUserQuestion tool input (`{questions:[{question, options:[{label}], multiSelect}]}`). */
export function questionsOf(raw: unknown): Question[] {
  if (!isObj(raw) || !Array.isArray(raw.questions)) return [];
  return raw.questions
    .filter(isObj)
    .map((q) => ({
      question: str(q.question),
      options: (Array.isArray(q.options) ? q.options : []).filter(isObj).map((o) => str(o.label)).filter(Boolean),
      multi: q.multiSelect === true,
    }))
    .filter((q) => q.question && q.options.length > 0);
}

/** The decision string for Rust, or null while any question is unanswered. */
export function answersJson(questions: Question[], picks: Record<string, string[]>): string | null {
  if (questions.length === 0) return null;
  const answers: Record<string, string | string[]> = {};
  for (const q of questions) {
    const p = (picks[q.question] ?? []).filter((x) => q.options.includes(x));
    if (p.length === 0) return null;
    answers[q.question] = q.multi ? [...new Set(p)] : p[0];
  }
  return JSON.stringify({ answers });
}
