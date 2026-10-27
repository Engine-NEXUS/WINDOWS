import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  AGENT_APPROVALS,
  TOOLS,
  auditSummary,
  clampPolicy,
  effectLabel,
  fmtWhen,
  purgeInDays,
  ruleLabel,
  trashLabel,
  validPattern,
  validRoot,
  type AuditRow,
  type AgentApproval,
  type Effect,
  type Policy,
  type PolicyView,
  type TrashRow,
} from "./actionsModel";

/**
 * Actions & Permissions: what NEXUS may do to files by voice.
 * Rules apply immediately to the kernel without restarting.
 * Protected system directories are enforced in the Rust kernel and cannot be unlocked.
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
        {error ? <div className="hx-error">{error}</div> : <div className="hx-note">Loading policy matrix…</div>}
      </div>
    );
  }
  const p = view.policy;

  return (
    <div className="hx-page">
      {error && <div className="hx-error" onClick={() => setError(null)}>{error}</div>}
      {notice && <div className="hx-note">{notice}</div>}

      {/* ── Master Voice Action Toggle & Safety Tiers ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
          <div className="hx-switch-title">File Operations by Voice</div>
          <label className="hx-toggle">
            <input
              type="checkbox"
              checked={p.enabled}
              disabled={busy}
              onChange={(e) => save({ ...p, enabled: e.target.checked })}
            />
            <span className="hx-toggle-slider" />
          </label>
        </div>

        {/* 3-Tier Safety Level Bento Grid */}
        <div className="hx-safety-grid">
          <div
            className={`hx-safety-card ${p.mode === "ask" ? "is-selected" : ""}`}
            onClick={() => !busy && save({ ...p, mode: "ask" })}
          >
            <span className="hx-safety-icon">✋</span>
            <span className="hx-safety-title">Ask First</span>
            <span className="hx-safety-desc">Confirm writes & deletes</span>
          </div>

          <div
            className={`hx-safety-card ${p.mode === "smart" ? "is-selected" : ""}`}
            onClick={() => !busy && save({ ...p, mode: "smart" })}
          >
            <span className="hx-safety-icon">⚡</span>
            <span className="hx-safety-title">Smart</span>
            <span className="hx-safety-desc">Auto-read, ask delete</span>
          </div>

          <div
            className={`hx-safety-card ${p.mode === "never_destructive" ? "is-selected" : ""}`}
            onClick={() => !busy && save({ ...p, mode: "never_destructive" })}
          >
            <span className="hx-safety-icon">🔒</span>
            <span className="hx-safety-title">Strict</span>
            <span className="hx-safety-desc">Refuse all deletes</span>
          </div>
        </div>
      </div>

      {/* ── Allowed Workspace Folders ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Workspace Roots</div>

        {/* Standard User Folders Switch */}
        <div className="hx-switch-card" style={{ marginTop: 10, background: "rgba(0, 0, 0, 0.2)" }}>
          <div className="hx-switch-meta">
            <div className="hx-switch-title">Default User Folders</div>
            <div className="hx-switch-sub">Allow access to Downloads, Desktop, and Documents.</div>
          </div>
          <label className="hx-toggle">
            <input
              type="checkbox"
              checked={p.use_default_roots}
              disabled={busy}
              onChange={(e) => save({ ...p, use_default_roots: e.target.checked })}
            />
            <span className="hx-toggle-slider" />
          </label>
        </div>

        {/* Active Roots List */}
        <div className="hx-root-chips">
          {view.roots.map((r) => (
            <div className="hx-root-chip" key={r}>
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <span>📁</span>
                <span>{r}</span>
              </div>
              {p.extra_roots.includes(r) && (
                <button
                  className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                  disabled={busy}
                  onClick={() => save({ ...p, extra_roots: p.extra_roots.filter((x) => x !== r) })}
                  title="Remove folder boundary"
                >
                  Remove
                </button>
              )}
            </div>
          ))}
          {view.roots.length === 0 && (
            <div style={{ padding: 12, textAlign: "center", color: "rgba(255, 255, 255, 0.45)", fontSize: 12 }}>
              No folder roots configured. Voice file commands will be refused until a boundary is added.
            </div>
          )}
        </div>

        {/* Add Extra Root Input */}
        <div style={{ display: "flex", gap: 8, marginTop: 10 }}>
          <input
            className="hx-input"
            placeholder="Add a folder path, e.g. D:\PROJECTS\ULTRON"
            value={newRoot}
            onChange={(e) => setNewRoot(e.target.value)}
            style={{ flex: 1 }}
          />
          <button
            className="hx-btn hx-btn--primary"
            disabled={busy || !validRoot(newRoot)}
            onClick={async () => {
              await save({ ...p, extra_roots: [...p.extra_roots, newRoot.trim()] });
              setNewRoot("");
            }}
          >
            + Add Folder
          </button>
        </div>
      </div>

      {/* ── Coding Agent Authority ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Coding Agent Authority</div>
        <select
          className="hx-input"
          value={p.agent_approval}
          disabled={busy}
          onChange={(e) => save({ ...p, agent_approval: e.target.value as AgentApproval })}
          style={{ marginTop: 8, width: "100%" }}
        >
          {AGENT_APPROVALS.map((a) => (
            <option key={a} value={a}>
              {a === "edit"
                ? "Edit files freely in allowed roots (commands ask first)"
                : a === "readonly"
                  ? "Read-only inspection (cannot modify code)"
                  : a === "standard"
                    ? "Strict ask-first on every file edit"
                    : "Unrestricted workspace autonomy"}
            </option>
          ))}
        </select>
      </div>

      {/* ── Voice Undo & Quarantine Vault ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div>
            <div className="hx-switch-title">Quarantine Vault ({trash.length})</div>
            <div className="hx-switch-sub">Kept for {p.quarantine_days} days</div>
          </div>
          <button
            className="hx-btn hx-btn--accent"
            disabled={busy}
            onClick={() => run(() => invoke<string>("actions_undo"))}
          >
            ↺ Undo Last Action
          </button>
        </div>

        {trash.length === 0 ? (
          <div style={{ padding: "16px 0", textAlign: "center", color: "rgba(255, 255, 255, 0.45)", fontSize: 12 }}>
            Quarantine vault is empty. No files have been deleted by voice recently.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 10 }}>
            {trash.map((t) => (
              <div
                key={t.id}
                style={{
                  padding: "10px 12px",
                  background: "rgba(0, 0, 0, 0.25)",
                  border: "1px solid rgba(255, 255, 255, 0.06)",
                  borderRadius: 8,
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                }}
              >
                <div>
                  <div style={{ fontSize: 13, fontWeight: 600, color: "#fff" }}>{trashLabel(t)}</div>
                  <div className="hx-fact-meta">
                    Quarantine remaining: {purgeInDays(t, now, p.quarantine_days)} days
                  </div>
                </div>
                <button
                  className="hx-btn hx-btn--tiny hx-btn--primary"
                  disabled={busy}
                  onClick={() => run(() => invoke<string>("actions_restore", { id: t.id }))}
                >
                  Restore File
                </button>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* ── Custom Pattern Rules ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Permission Overrides ({p.rules.length})</div>

        {p.rules.length > 0 && (
          <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 10 }}>
            {p.rules.map((r, i) => (
              <div
                key={`${r.effect}${r.tool}${r.pattern}${i}`}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  padding: "8px 12px",
                  background: "rgba(0, 0, 0, 0.2)",
                  borderRadius: 8,
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span
                    className="hx-pill"
                    style={{
                      background:
                        r.effect === "allow"
                          ? "rgba(16, 185, 129, 0.15)"
                          : r.effect === "ask"
                            ? "rgba(242, 184, 89, 0.15)"
                            : "rgba(239, 68, 68, 0.15)",
                      color: r.effect === "allow" ? "#34d399" : r.effect === "ask" ? "#f2b859" : "#ff5a52",
                    }}
                  >
                    {r.effect.toUpperCase()}
                  </span>
                  <span style={{ fontSize: 12.5, color: "#fff" }}>{ruleLabel(r)}</span>
                </div>
                <button
                  className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                  disabled={busy}
                  onClick={() => save({ ...p, rules: p.rules.filter((_, j) => j !== i) })}
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        {/* Rule Builder Controls */}
        <div style={{ display: "flex", gap: 6, marginTop: 10, flexWrap: "wrap" }}>
          <select
            className="hx-input"
            value={ruleEffect}
            onChange={(e) => setRuleEffect(e.target.value as Effect)}
            style={{ width: "110px" }}
          >
            {(["deny", "ask", "allow"] as Effect[]).map((e) => (
              <option key={e} value={e}>{effectLabel(e)}</option>
            ))}
          </select>

          <select
            className="hx-input"
            value={ruleTool}
            onChange={(e) => setRuleTool(e.target.value)}
            style={{ width: "120px" }}
          >
            {TOOLS.map((t) => (
              <option key={t} value={t}>{t === "*" ? "any tool" : t.replace("_", " ")}</option>
            ))}
          </select>

          <input
            className="hx-input"
            placeholder="e.g. *.env or *.key"
            value={rulePattern}
            onChange={(e) => setRulePattern(e.target.value)}
            style={{ flex: 1, minWidth: "120px" }}
          />

          <button
            className="hx-btn hx-btn--primary"
            disabled={busy || !validPattern(rulePattern)}
            onClick={async () => {
              await save({ ...p, rules: [...p.rules, { effect: ruleEffect, tool: ruleTool, pattern: rulePattern.trim() }] });
              setRulePattern("");
            }}
          >
            + Add Rule
          </button>
        </div>
      </div>

      {/* ── Recent Activity Audit Trail ── */}
      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">Recent Activity ({audit.length})</div>

        {audit.length === 0 ? (
          <div style={{ padding: "16px 0", textAlign: "center", color: "rgba(255, 255, 255, 0.45)", fontSize: 12 }}>
            No recent file operations recorded.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 10, maxHeight: "240px", overflowY: "auto" }}>
            {audit.map((a, i) => (
              <div
                key={`${a.ts}${i}`}
                style={{
                  padding: "8px 10px",
                  background: "rgba(0, 0, 0, 0.2)",
                  borderRadius: 6,
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  fontSize: 12,
                }}
              >
                <div>
                  <span style={{ color: "#fff", fontWeight: 500 }}>{auditSummary(a)}</span>
                  {a.origin === "derived" && <span style={{ color: "rgba(255, 255, 255, 0.4)", marginLeft: 6 }}>(text trigger)</span>}
                </div>
                <span className="hx-fact-meta">{fmtWhen(a.ts)}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
