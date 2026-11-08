// Dev-only preview of the Agent Island (served by the Vite dev server; not part of the release bundle).
// The real Pill / Panel components are rendered with simulated agents that walk through a scripted turn (Read, Edit
// with a live diff, Bash, Done). The hover rules mirror agent_island.rs (120 ms to open, 450 ms to fold, a permission
// request opens it, the answer folds it back to the pill).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import "./agent-island.css";
import { Panel, Pill } from "./AgentIslandApp";
import { pillLogos, type AgentState, type ApprovalView, type SessionView, type Tab } from "./agentIslandModel";

const HOVER_DWELL_MS = 120;
const LEAVE_DWELL_MS = 450;
const PANEL = { w: 760, h: 430 };
const LABELS: Record<string, string> = { claude: "Claude Code", codex: "Codex", antigravity: "Antigravity", opencode: "OpenCode", gemini: "Gemini CLI", cursor: "Cursor" };

const SRC_OLD = "import { Item } from './types'\n\nconst TVA = 0.196\n\nexport function total(items: Item[]) {\n  const sum = items.reduce((s, i) => s + i.price, 0)\n  return sum * (1 + TVA)\n}";
const SRC_NEW = "import { Item } from './types'\n\nconst TVA = 0.2 // rate from 2026\n\nexport function total(items: Item[]) {\n  const sum = items.reduce((s, i) => s + i.price, 0)\n  return sum * (1 + TVA)\n}";

/** One scripted turn: what the agent does at each tick. */
const SCRIPT = [
  { tool: "Read", detail: "invoice.ts", edit: null },
  { tool: "Edit", detail: "invoice.ts", edit: { path: "D:\\work\\korus\\src\\invoice.ts", kind: "edit" as const, pairs: [{ old: SRC_OLD, new: SRC_NEW }], tooLarge: false } },
  { tool: "Bash", detail: "cargo test", edit: null },
];

let nextId = 1;

function Preview() {
  const [sessions, setSessions] = useState<SessionView[]>([]);
  const [approvals, setApprovals] = useState<ApprovalView[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [byApproval, setByApproval] = useState(false);
  const [tab, setTab] = useState<Tab>("tracking");
  const [now, setNow] = useState(() => Date.now());
  const timer = useRef<number | null>(null);
  const needLeave = useRef(false);
  const insideRef = useRef(false);
  const script = useRef<Record<string, number>>({});

  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 500);
    return () => window.clearInterval(id);
  }, []);

  // each running agent advances one scripted step every 3 s
  useEffect(() => {
    const id = window.setInterval(() => {
      setSessions((cur) =>
        cur.map((s) => {
          if (s.state === "done" || s.state === "waiting") return s;
          const at = (script.current[s.key] ?? -1) + 1;
          script.current[s.key] = at;
          if (at >= SCRIPT.length) {
            const steps = [...s.steps.map((x) => ({ ...x, state: "done" as const })), { tool: "Done", detail: null, state: "done" as const }];
            window.setTimeout(() => setSessions((c) => c.filter((x) => x.key !== s.key)), 2500);
            return { ...s, state: "done" as AgentState, tool: null, steps };
          }
          const step = SCRIPT[at];
          const steps = [...s.steps.map((x) => ({ ...x, state: "done" as const })), { tool: step.tool, detail: step.detail, state: "running" as const }];
          return { ...s, state: "working" as AgentState, tool: step.tool, steps, edit: step.edit ?? s.edit, updatedMs: Date.now() };
        }),
      );
    }, 3000);
    return () => window.clearInterval(id);
  }, []);

  const snap = useMemo(
    () => ({ running: sessions.filter((s) => s.state !== "done").length, atMs: now, sessions, approvals }),
    [sessions, approvals, now],
  );
  const logos = pillLogos(snap);

  const clear = () => {
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = null;
  };

  const onEnter = () => {
    insideRef.current = true;
    clear();
    if (expanded || needLeave.current) return;
    timer.current = window.setTimeout(() => {
      setExpanded(true);
      setByApproval(false);
      setTab("tracking");
    }, HOVER_DWELL_MS);
  };
  const onLeave = () => {
    insideRef.current = false;
    needLeave.current = false;
    clear();
    if (!expanded || approvals.length > 0) return;
    timer.current = window.setTimeout(() => setExpanded(false), LEAVE_DWELL_MS);
  };

  const start = useCallback((agent: string) => {
    const t = Date.now();
    const key = `${agent}:${nextId++}`;
    script.current[key] = 0;
    setSessions((cur) => [
      ...cur,
      {
        key,
        agent,
        label: LABELS[agent] ?? agent,
        folder: ["korus", "nexus", "api-backend"][cur.length % 3],
        state: "working" as AgentState,
        tool: "Read",
        steps: [{ tool: "Read", detail: "invoice.ts", state: "running" as const }],
        edit: null,
        startedMs: t,
        updatedMs: t,
      },
    ]);
  }, []);

  const finish = () => {
    setSessions((cur) => {
      const i = cur.findIndex((s) => s.state !== "done");
      if (i < 0) return cur;
      const key = cur[i].key;
      window.setTimeout(() => setSessions((c) => c.filter((s) => s.key !== key)), 1500);
      return cur.map((s, j) =>
        j === i ? { ...s, state: "done" as AgentState, tool: null, steps: [...s.steps.map((x) => ({ ...x, state: "done" as const })), { tool: "Done", detail: null, state: "done" as const }] } : s,
      );
    });
  };

  const request = (question: boolean) => {
    const t = Date.now();
    const agent = sessions.find((s) => s.state !== "done")?.agent ?? "claude";
    if (!sessions.some((s) => s.state !== "done")) start(agent);
    setApprovals((cur) => [
      ...cur,
      {
        id: nextId++,
        agent,
        label: LABELS[agent] ?? agent,
        tool: question ? "AskUserQuestion" : "Bash",
        summary: question ? "" : "git push --force origin main && rm -rf node_modules",
        question: question
          ? { questions: [{ question: "Which database should I use?", options: [{ label: "Postgres" }, { label: "SQLite" }] }, { question: "Extras?", multiSelect: true, options: [{ label: "Migrations" }, { label: "Seed data" }, { label: "Tests" }] }] }
          : null,
        requestedMs: t,
        deadlineMs: t + 108_000,
      },
    ]);
    setSessions((cur) => cur.map((s) => (s.agent === agent && s.state !== "done" ? { ...s, state: "waiting" as AgentState } : s)));
    setExpanded(true);
    setByApproval(true);
    setTab("approvals");
  };

  const decide = (id: number, d: string) => {
    console.info("decision sent to Rust:", id, d);
    setApprovals((cur) => {
      const left = cur.filter((a) => a.id !== id);
      if (left.length === 0) {
        // the answer folds it back to the one-row pill; hover must not re-open it until the cursor leaves
        setSessions((s) => s.map((x) => (x.state === "waiting" ? { ...x, state: "working" as AgentState } : x)));
        setExpanded(false);
        setByApproval(false);
        needLeave.current = insideRef.current;
      }
      return left;
    });
  };

  const mode = sessions.length === 0 && approvals.length === 0 ? "hidden" : expanded ? "expanded" : "collapsed";
  const pillW = Math.max(64, 28 + 32 * Math.min(8, Math.max(1, logos.length)));
  const frame = mode === "expanded" ? { width: PANEL.w, height: PANEL.h } : { width: pillW, height: 40 };

  return (
    <>
      <div className="dev-note">
        Agent Island preview. Add an agent and hover the pill: the island shows the agent, its steps and the file it edits as a live diff. A permission request opens it by itself; answering folds it back. A finished agent's logo leaves the row.
      </div>
      {mode !== "hidden" && (
        <div className="dev-frame" style={frame} onMouseEnter={onEnter} onMouseLeave={onLeave}>
          {mode === "collapsed" ? <Pill snap={snap} /> : <Panel snap={snap} tab={tab} setTab={setTab} now={now} decide={decide} />}
        </div>
      )}
      <div className="dev-bar">
        {Object.keys(LABELS).map((a) => (
          <button key={a} onClick={() => start(a)}>
            + {LABELS[a]}
          </button>
        ))}
        <button onClick={() => request(false)}>Permission request</button>
        <button onClick={() => request(true)}>Question</button>
        <button onClick={finish}>Finish one agent</button>
        <button onClick={() => { setSessions([]); setApprovals([]); setExpanded(false); }}>Clear</button>
        <span>mode: {mode}{byApproval ? " (by request)" : ""}</span>
      </div>
    </>
  );
}

createRoot(document.getElementById("root")!).render(<Preview />);
