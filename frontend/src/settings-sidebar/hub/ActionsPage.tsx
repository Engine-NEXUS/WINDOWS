import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AGENT_APPROVALS,
  TOOLS,
  agentApprovalLabel,
  auditSummary,
  clampPolicy,
  effectLabel,
  fmtWhen,
  modeLabel,
  purgeInDays,
  ruleLabel,
  trashLabel,
  validPattern,
  validRoot,
  type AuditRow,
  type AgentApproval,
  type Effect,
  type Mode,
  type Policy,
  type PolicyView,
  type TrashRow,
} from "./actionsModel";

/**
 * Actions & Permissions: what NEXUS may do to files by voice. The rules the page edits are the same
 * file the kernel reads on every command, so changes apply at once, without a restart. Places that
 * are never touched (Windows, other profiles, .git, .ssh, NEXUS's own data) are fixed in code and
 * are not listed here because no setting can unlock them.
 */
export function ActionsPage() {
  const [view, setView] = useState<PolicyView | null>(null);
  const [trash, setTrash] = useState<TrashRow[]>([]);
  const [audit, setAudit] = useState<AuditRow[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [newRoot, setNewRoot] = useState("");
  const [ruleEffect, setRuleEffect] = useState<Effect>("deny");
  const [ruleTool, setRuleTool] = useState<string>("delete");
  const [rulePattern, setRulePattern] = useState("");
  const now = Math.floor(Date.now() / 1000);

  const load = useCallback(async () => {
    try {
      const [v, t, a] = await Promise.all([
        invoke<PolicyView>("actions_get_policy"),
        invoke<TrashRow[]>("actions_trash_list"),
        invoke<AuditRow[]>("actions_audit", { limit: 40 }),
      ]);
      setView(v);
      setTrash(t);
      setAudit([...a].reverse());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const save = async (next: Policy) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("actions_set_policy", { policy: clampPolicy(next) });
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const run = async (call: () => Promise<string>) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const said = await call();
      setNotice(said);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  if (!view) {
    return (
      <div className="hx-page">
        {error ? <div className="hx-error">{error}</div> : <div className="hx-note">Loading…</div>}
      </div>
    );
  }
  const p = view.policy;

  return (
    <div className="hx-page">
      <div className="hx-note">
        Say “delete the PDFs in Downloads” and I read back exactly what I found, then wait for your yes. Nothing is
        permanently deleted: it goes to a hidden trash folder inside the same folder, and “undo that” brings it back.
        Windows, other users' profiles, .git, .ssh and my own data are never touched, whatever you set here.
      </div>
      {error && <div className="hx-error">{error}</div>}
      {notice && <div className="hx-note">{notice}</div>}

      <div className="hx-card hx-key">
        <div className="hx-key-head">
          <div>
            <div className="hx-key-title">File commands by voice</div>
            <div className="hx-key-sub">{p.enabled ? "On" : "Off: I will say they are switched off"}</div>
          </div>
          <input type="checkbox" checked={p.enabled} disabled={busy} onChange={(e) => save({ ...p, enabled: e.target.checked })} />
        </div>
        <div className="hx-seg">
          {(["smart", "ask", "never_destructive"] as Mode[]).map((m) => (
            <button
              key={m}
              type="button"
              className={`hx-seg-btn ${p.mode === m ? "hx-seg-btn--on" : ""}`}
              disabled={busy}
              onClick={() => save({ ...p, mode: m })}
            >
              {m === "smart" ? "Smart" : m === "ask" ? "Ask first" : "Never delete"}
            </button>
          ))}
        </div>
        <div className="hx-key-sub">{modeLabel(p.mode)}</div>
      </div>

      <div className="hx-card hx-key">
        <div className="hx-key-title">Folders I may work in</div>
        <label className="hx-row-actions" style={{ justifyContent: "space-between" }}>
          <span className="hx-key-sub">Downloads, Desktop and Documents</span>
          <input
            type="checkbox"
            checked={p.use_default_roots}
            disabled={busy}
            onChange={(e) => save({ ...p, use_default_roots: e.target.checked })}
          />
        </label>
        {view.roots.map((r) => (
          <div className="hx-key-mask" key={r}>{r}</div>
        ))}
        {view.roots.length === 0 && <div className="hx-key-sub">No folders yet, so every file command is refused.</div>}
        {p.extra_roots.map((r) => (
          <div className="hx-row-actions" key={r} style={{ justifyContent: "space-between" }}>
            <span className="hx-key-sub">{r}</span>
            <button className="hx-btn hx-btn--danger-ghost" disabled={busy} onClick={() => save({ ...p, extra_roots: p.extra_roots.filter((x) => x !== r) })}>
              Remove
            </button>
          </div>
        ))}
        <div className="hx-row-actions">
          <input
            className="hx-input"
            placeholder="Add a folder, like D:\projects"
            value={newRoot}
            onChange={(e) => setNewRoot(e.target.value)}
          />
          <button
            className="hx-btn hx-btn--primary"
            disabled={busy || !validRoot(newRoot)}
            onClick={async () => {
              await save({ ...p, extra_roots: [...p.extra_roots, newRoot.trim()] });
              setNewRoot("");
            }}
          >
            Add
          </button>
        </div>
        <div className="hx-key-sub">Say “in the projects folder” after adding D:\projects. A folder must exist, and drives or network shares are refused.</div>
      </div>

      <div className="hx-card hx-key">
        <div className="hx-key-title">Rules</div>
        <div className="hx-key-sub">Checked in this order: Never, then Always ask, then Allow. The first match wins.</div>
        {p.rules.map((r, i) => (
          <div className="hx-row-actions" key={`${r.effect}${r.tool}${r.pattern}${i}`} style={{ justifyContent: "space-between" }}>
            <span className="hx-key-sub">{ruleLabel(r)}</span>
            <button className="hx-btn hx-btn--danger-ghost" disabled={busy} onClick={() => save({ ...p, rules: p.rules.filter((_, j) => j !== i) })}>
              Remove
            </button>
          </div>
        ))}
        <div className="hx-row-actions">
          <select className="hx-input" value={ruleEffect} onChange={(e) => setRuleEffect(e.target.value as Effect)}>
            {(["deny", "ask", "allow"] as Effect[]).map((e) => (
              <option key={e} value={e}>{effectLabel(e)}</option>
            ))}
          </select>
          <select className="hx-input" value={ruleTool} onChange={(e) => setRuleTool(e.target.value)}>
            {TOOLS.map((t) => (
              <option key={t} value={t}>{t === "*" ? "any action" : t.replace("_", " ")}</option>
            ))}
          </select>
          <input className="hx-input" placeholder="*.pdf" value={rulePattern} onChange={(e) => setRulePattern(e.target.value)} />
          <button
            className="hx-btn hx-btn--primary"
            disabled={busy || !validPattern(rulePattern)}
            onClick={async () => {
              await save({ ...p, rules: [...p.rules, { effect: ruleEffect, tool: ruleTool, pattern: rulePattern.trim() }] });
              setRulePattern("");
            }}
          >
            Add
          </button>
        </div>
      </div>

      <div className="hx-card hx-key">
        <div className="hx-key-title">Coding agents I start for you</div>
        <select
          className="hx-input"
          value={p.agent_approval}
          disabled={busy}
          onChange={(e) => save({ ...p, agent_approval: e.target.value as AgentApproval })}
        >
          {AGENT_APPROVALS.map((a) => (
            <option key={a} value={a}>{a === "edit" ? "Edit files only" : a === "readonly" ? "Read only" : a === "standard" ? "Ask first" : "Edit freely, ask before commands"}</option>
          ))}
        </select>
        <div className="hx-key-sub">{agentApprovalLabel(p.agent_approval)}. There is no option that turns the agent's permission checks off.</div>
      </div>

      <div className="hx-card hx-key">
        <div className="hx-key-head">
          <div>
            <div className="hx-key-title">Trash and undo</div>
            <div className="hx-key-sub">Kept {p.quarantine_days} days, then removed for good (every removal is logged).</div>
          </div>
          <button className="hx-btn" disabled={busy} onClick={() => run(() => invoke<string>("actions_undo"))}>
            Undo last action
          </button>
        </div>
        {trash.length === 0 && <div className="hx-key-sub">The trash is empty.</div>}
        {trash.map((t) => (
          <div className="hx-row-actions" key={t.id} style={{ justifyContent: "space-between" }}>
            <span className="hx-key-sub">
              {trashLabel(t)} · {purgeInDays(t, now, p.quarantine_days)} days left
            </span>
            <button className="hx-btn" disabled={busy} onClick={() => run(() => invoke<string>("actions_restore", { id: t.id }))}>
              Restore
            </button>
          </div>
        ))}
      </div>

      <div className="hx-card hx-key">
        <div className="hx-key-title">Recent activity</div>
        <div className="hx-key-sub">Kept on this PC for {p.audit_days} days. Phrases never leave the device.</div>
        {audit.length === 0 && <div className="hx-key-sub">Nothing yet.</div>}
        {audit.map((a, i) => (
          <div className="hx-key-sub" key={`${a.ts}${i}`}>
            {fmtWhen(a.ts)} · {auditSummary(a)}
            {a.origin === "derived" ? " (text, not your voice)" : ""}
          </div>
        ))}
      </div>
    </div>
  );
}
