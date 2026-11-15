import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type HTMLAttributes } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { diffOf, fileName, type DiffLine } from "./diff";
import { languageOf, tokenize } from "./syntax";
import {
  EMPTY,
  answersJson,
  asMode,
  agentLook,
  asSnapshot,
  defaultTab,
  elapsedLabel,
  focusSession,
  islandSize,
  pillLogos,
  questionsOf,
  SCALE_DEFAULT,
  secondsLeft,
  stateWord,
  type ApprovalView,
  type ModePayload,
  type SessionView,
  type Snapshot,
  type Tab,
} from "./agentIslandModel";

/** The Agent Island. Rust owns the window (a fixed, panel-sized, transparent window that ignores the mouse outside the
 *  drawn island; the hover and auto-open for a permission request are decided there); this page only draws what it is
 *  told. ONE element morphs between the one-row pill of the agents' logos and the full island (Tracking tab: the agent,
 *  its tool steps and a live diff; Approvals tab), so a hover never resizes a window or swaps components: nothing jumps.
 *  The snapshot is already without the agents the user is looking at. */
export function AgentIslandApp() {
  const [snap, setSnap] = useState<Snapshot>(EMPTY);
  const [mode, setMode] = useState<ModePayload>({ mode: "hidden", byApproval: false, scale: SCALE_DEFAULT, hot: false });
  const [tab, setTab] = useState<Tab>("tracking");
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    invoke<unknown>("agent_island_snapshot").then((r) => setSnap(asSnapshot(r))).catch(() => {});
    invoke<unknown>("agent_island_mode").then((r) => setMode(asMode(r))).catch(() => {});
    const a = listen<unknown>("agent_island:snapshot", (e) => setSnap(asSnapshot(e.payload)));
    const b = listen<unknown>("agent_island:mode", (e) => setMode(asMode(e.payload)));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  // a request opens on its tab; a plain hover opens on Tracking
  useEffect(() => {
    if (mode.mode === "expanded") setTab(defaultTab(snap, mode.byApproval));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [mode.mode, mode.byApproval]);

  useEffect(() => {
    if (mode.mode !== "expanded") return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [mode.mode]);

  const decide = useCallback((id: number, decision: string) => {
    invoke("agent_approval_decide", { id, decision }).catch(() => {});
  }, []);

  if (mode.mode === "hidden") return null;
  return <Island snap={snap} mode={mode.mode} tab={tab} setTab={setTab} now={now} decide={decide} scale={mode.scale} hot={mode.hot} />;
}

/** How long the panel stays mounted after the island folds, so its content fades out instead of vanishing. */
const PANEL_LINGER_MS = 340;

export function Island(p: {
  snap: Snapshot;
  mode: "collapsed" | "expanded";
  tab: Tab;
  setTab: (t: Tab) => void;
  now: number;
  decide: (id: number, d: string) => void;
  scale?: number;
  /** The cursor is in the hot zone of the pill (set by Rust). */
  hot?: boolean;
  /** Mouse handlers for the dev preview (the real island is hovered through Rust). */
  host?: HTMLAttributes<HTMLDivElement>;
}) {
  const { snap, mode, tab, setTab, now, decide, scale = 1, hot = false, host } = p;
  const ref = useRef<HTMLDivElement | null>(null);
  const logos = pillLogos(snap).length;
  const [keepPanel, setKeepPanel] = useState(mode === "expanded");

  // sizes go in through CSS custom properties (the CSSOM is allowed under the window's CSP; the transition does the rest)
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { w, h } = islandSize(mode, logos);
    el.style.setProperty("--ai-w", `${w}px`);
    el.style.setProperty("--ai-h", `${h}px`);
    el.style.setProperty("--ai-scale", String(scale));
  }, [mode, logos, scale]);

  useEffect(() => {
    if (mode === "expanded") {
      setKeepPanel(true);
      return;
    }
    const id = window.setTimeout(() => setKeepPanel(false), PANEL_LINGER_MS);
    return () => window.clearTimeout(id);
  }, [mode]);

  return (
    <div className={`ai-island ai-island--${mode}${hot && mode === "collapsed" ? " ai-island--hot" : ""}`} ref={ref} {...host}>
      <div className="ai-layer ai-layer--pill" aria-hidden={mode === "expanded"}>
        <Pill snap={snap} />
      </div>
      <div className="ai-layer ai-layer--panel" aria-hidden={mode === "collapsed"}>
        {keepPanel && <Panel snap={snap} tab={tab} setTab={setTab} now={now} decide={decide} />}
      </div>
    </div>
  );
}

export function Pill({ snap }: { snap: Snapshot }) {
  const logos = useMemo(() => pillLogos(snap), [snap]);
  return (
    <div className="ai-pill" role="status" aria-label={`${logos.length} agent${logos.length === 1 ? "" : "s"} running`}>
      {logos.map((l) => (
        <div key={l.agent} className={`ai-chip ai-chip--${l.state}`} title={`${l.label}: ${stateWord(l.state, null)}`}>
          <AgentLogo agent={l.agent} label={l.label} className="" />
          {l.count > 1 && <span className="ai-count">{l.count}</span>}
        </div>
      ))}
    </div>
  );
}

/** An agent's logo on its tile (see agentLook: the tile is chosen so the mark is always visible). */
function AgentLogo({ agent, label, className }: { agent: string; label: string; className: string }) {
  const look = agentLook(agent);
  return (
    <div className={`ai-logo ${className}`.trim()} style={{ background: look?.tile ?? "#f4f4f6" }} title={label}>
      {look ? <img src={look.src} alt={label} className="ai-logo-img" /> : <span className="ai-logo-mono">{(label || "?").slice(0, 1)}</span>}
    </div>
  );
}

const ICON = {
  home: "M3 11.5 12 4l9 7.5M5.5 10v9.5h13V10",
  shield: "M12 3 5 6v5.5c0 4.2 2.8 7.4 7 9 4.2-1.6 7-4.8 7-9V6l-7-3Zm-3 9 2.2 2.2L15.5 10",
  check: "M5 12.5 10 17.5 19 7",
};

function Icon({ d, size = 16 }: { d: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d={d} />
    </svg>
  );
}

export function Panel(p: { snap: Snapshot; tab: Tab; setTab: (t: Tab) => void; now: number; decide: (id: number, d: string) => void }) {
  const { snap, tab, setTab, now, decide } = p;
  const waiting = snap.approvals.length;
  return (
    <div className="ai-panel">
      <div className="ai-bar">
        <div className="ai-tabs" role="tablist">
          <button role="tab" aria-selected={tab === "tracking"} aria-label="Tracking" title="Tracking" className="ai-tab" onClick={() => setTab("tracking")}>
            <Icon d={ICON.home} />
          </button>
          <button role="tab" aria-selected={tab === "approvals"} aria-label="Approvals" title="Approvals" className="ai-tab" onClick={() => setTab("approvals")}>
            <Icon d={ICON.shield} />
            {waiting > 0 && <span className="ai-badge">{waiting}</span>}
          </button>
        </div>
        <span className="ai-title">{snap.running === 1 ? "1 agent running" : `${snap.running} agents running`}</span>
      </div>
      <div className="ai-body">
        {tab === "tracking" ? <Tracking snap={snap} now={now} /> : <Approvals approvals={snap.approvals} now={now} decide={decide} />}
      </div>
    </div>
  );
}

function Tracking({ snap, now }: { snap: Snapshot; now: number }) {
  const [picked, setPicked] = useState<string | null>(null);
  const s = focusSession(snap, picked);
  if (!s) return <p className="ai-empty">No agent is running.</p>;
  return (
    <div className="ai-track">
      <Rail snap={snap} s={s} now={now} onPick={setPicked} />
      <CodePane s={s} />
    </div>
  );
}

function Rail({ snap, s, now, onPick }: { snap: Snapshot; s: SessionView; now: number; onPick: (key: string) => void }) {
  return (
    <aside className="ai-rail">
      {snap.sessions.length > 1 && (
        <div className="ai-switch" role="tablist" aria-label="Agents">
          {snap.sessions.map((x) => (
            <button key={x.key} role="tab" aria-selected={x.key === s.key} title={`${x.label}${x.folder ? ` · ${x.folder}` : ""}`} className="ai-switch-btn" onClick={() => onPick(x.key)}>
              <AgentLogo agent={x.agent} label={x.label} className="ai-logo--small" />
            </button>
          ))}
        </div>
      )}
      <div className={`ai-hero ai-hero--${s.state}`}>
        <AgentLogo agent={s.agent} label={s.label} className="ai-logo--hero" />
        <StateBubble state={s.state} />
      </div>
      <div className="ai-who">
        <div className="ai-who-name">{s.folder ?? s.label}</div>
        <div className="ai-who-agent">{s.label}</div>
        <div className="ai-who-time">{s.state === "done" ? "finished" : elapsedLabel(now - s.startedMs)}</div>
      </div>
      <ol className="ai-steps" aria-label="Steps">
        {s.steps.length === 0 && <li className="ai-step ai-step--idle">{stateWord(s.state, s.tool)}</li>}
        {s.steps.map((st, i) => (
          <li key={`${i}-${st.tool}`} className={`ai-step ai-step--${st.state}`} title={st.detail ?? undefined}>
            <span className="ai-step-icon" aria-hidden="true">
              {st.state === "running" ? <span className="ai-spin" /> : <Icon d={ICON.check} size={13} />}
            </span>
            <span className="ai-step-tool">{st.tool}</span>
            {st.detail && <span className="ai-step-detail">{st.detail}</span>}
          </li>
        ))}
      </ol>
    </aside>
  );
}

function StateBubble({ state }: { state: SessionView["state"] }) {
  return (
    <span className={`ai-bubble ai-bubble--${state}`} aria-hidden="true">
      {state === "thinking" && (
        <>
          <i />
          <i />
          <i />
        </>
      )}
      {state === "working" && <span className="ai-spin ai-spin--small" />}
      {state === "waiting" && "!"}
      {state === "done" && <Icon d={ICON.check} size={11} />}
    </span>
  );
}

function CodePane({ s }: { s: SessionView }) {
  const diff = useMemo(() => (s.edit ? diffOf(s.edit) : null), [s.edit]);
  const running = s.steps.some((x) => x.state === "running");
  if (!s.edit || !diff) {
    const last = [...s.steps].reverse().find((x) => x.state === "running") ?? s.steps[s.steps.length - 1];
    return (
      <section className="ai-code ai-code--empty">
        <div className="ai-code-note">{last ? `${last.tool}${last.detail ? ` · ${last.detail}` : ""}` : stateWord(s.state, s.tool)}</div>
        <div className="ai-code-sub">The code it changes appears here.</div>
      </section>
    );
  }
  const lang = languageOf(diff.path);
  const rows: Array<DiffLine | "gap"> = [];
  diff.hunks.forEach((h, i) => {
    if (i > 0) rows.push("gap");
    rows.push(...h.lines);
  });
  const lastAdded = (() => {
    for (let i = rows.length - 1; i >= 0; i--) {
      const r = rows[i];
      if (r !== "gap" && r.kind === "added") return i;
    }
    return -1;
  })();
  return (
    <section className="ai-code">
      <header className="ai-code-head">
        <span className="ai-file">
          <span className="ai-lang">{lang.label}</span>
          <span className="ai-file-name">{fileName(diff.path)}</span>
          <span className={`ai-file-dot${running ? " ai-file-dot--live" : ""}`} aria-hidden="true" />
        </span>
        <span className="ai-path" title={diff.path}>{diff.path}</span>
      </header>
      <div className="ai-lines">
        {diff.tooLarge ? (
          <div className="ai-code-note">Diff too large to show{diff.added || diff.removed ? ` (+${diff.added} −${diff.removed})` : ""}</div>
        ) : (
          rows.map((r, i) =>
            r === "gap" ? (
              <div key={`g${i}`} className="ai-gap" aria-hidden="true">⋯</div>
            ) : (
              <div key={i} className={`ai-line ai-line--${r.kind}`}>
                <span className="ai-num">{r.newLine > 0 ? r.newLine : r.origLine}</span>
                <span className="ai-sign">{r.kind === "added" ? "+" : r.kind === "removed" ? "−" : ""}</span>
                <span className="ai-text">
                  {tokenize(r.text, lang).map((t, k) => (
                    <span key={k} className={`syn-${t.cls}`}>{t.text}</span>
                  ))}
                  {running && i === lastAdded && <span className="ai-caret" aria-hidden="true" />}
                </span>
              </div>
            ),
          )
        )}
      </div>
    </section>
  );
}

function Approvals({ approvals, now, decide }: { approvals: ApprovalView[]; now: number; decide: (id: number, d: string) => void }) {
  if (approvals.length === 0) return <p className="ai-empty">Nothing is waiting for you.</p>;
  return (
    <ul className="ai-list">
      {approvals.map((a) => (
        <ApprovalCard key={a.id} a={a} now={now} decide={decide} />
      ))}
    </ul>
  );
}

function ApprovalCard({ a, now, decide }: { a: ApprovalView; now: number; decide: (id: number, d: string) => void }) {
  const questions = useMemo(() => questionsOf(a.question), [a.question]);
  const [picks, setPicks] = useState<Record<string, string[]>>({});
  const answer = answersJson(questions, picks);
  const left = secondsLeft(a.deadlineMs, now);
  const pick = (q: string, option: string, multi: boolean) =>
    setPicks((cur) => {
      const have = cur[q] ?? [];
      const next = multi ? (have.includes(option) ? have.filter((x) => x !== option) : [...have, option]) : [option];
      return { ...cur, [q]: next };
    });
  return (
    <li className="ai-card">
      <div className="ai-card-head">
        <AgentLogo agent={a.agent} label={a.label} className="ai-logo--small" />
        <span className="ai-name">{a.label}</span>
        <span className="ai-tool">{questions.length > 0 ? "Question" : a.tool === "ExitPlanMode" ? "Plan" : a.tool || "Tool"}</span>
        <span className="ai-left" title="After this the agent asks in its own terminal">{left}s</span>
      </div>
      {questions.length === 0 ? (
        <>
          <pre className={a.tool === "ExitPlanMode" ? "ai-summary ai-summary--plan" : "ai-summary"}>{a.summary || "(no details)"}</pre>
          <div className="ai-actions">
            <button className="ai-btn ai-btn--allow" onClick={() => decide(a.id, "allow")}>
              Allow
            </button>
            <button className="ai-btn ai-btn--deny" onClick={() => decide(a.id, "deny")}>
              Deny
            </button>
          </div>
        </>
      ) : (
        <>
          {questions.map((q) => (
            <fieldset key={q.question} className="ai-q">
              <legend>{q.question}</legend>
              {q.options.map((o) => (
                <label key={o} className="ai-opt">
                  <input
                    type={q.multi ? "checkbox" : "radio"}
                    name={`q-${a.id}-${q.question}`}
                    checked={(picks[q.question] ?? []).includes(o)}
                    onChange={() => pick(q.question, o, q.multi)}
                  />
                  <span>{o}</span>
                </label>
              ))}
            </fieldset>
          ))}
          <div className="ai-actions">
            <button className="ai-btn ai-btn--allow" disabled={!answer} onClick={() => answer && decide(a.id, answer)}>
              Send answer
            </button>
          </div>
        </>
      )}
    </li>
  );
}
