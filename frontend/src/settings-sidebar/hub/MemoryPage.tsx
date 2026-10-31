import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  ageLabel,
  keyLabel,
  sourceLabel,
  type EgressEntry,
  categoryLabel,
  fmtDays,
  fmtSlotTime,
  selftestSummary,
  describeCorrection,
  routineUse,
  type MailRow,
  type PersonRow,
  type WhatsappSelfTest,
  type CorrectionRow,
  type PendingRow,
  type RoutinesView,
  type MemoryFlag,
  type MemoryRow,
  type MemoryStatus,
  type TimetableSlot,
} from "./memoryModel";
import { SwitchCard } from "../../theme/OptionCard";

type MemoryTab = "privacy" | "facts" | "people" | "egress";

export function MemoryPage({ onFlagChanged }: { onFlagChanged?: (flag: MemoryFlag, on: boolean) => void }) {
  const [activeTab, setActiveTab] = useState<MemoryTab>("privacy");
  const [status, setStatus] = useState<MemoryStatus | null>(null);
  const [rows, setRows] = useState<MemoryRow[]>([]);
  const [egress, setEgress] = useState<EgressEntry[]>([]);
  const [slots, setSlots] = useState<TimetableSlot[]>([]);
  const [corrections, setCorrections] = useState<CorrectionRow[]>([]);
  const [routinesView, setRoutinesView] = useState<RoutinesView>({ routines: [], suggestion: null, limit: 30 });
  const [routineName, setRoutineName] = useState("");
  const [pending, setPending] = useState<PendingRow[]>([]);
  const [mail, setMail] = useState<MailRow[]>([]);
  const [people, setPeople] = useState<PersonRow[]>([]);
  const [selftest, setSelftest] = useState<WhatsappSelfTest | null>(null);
  const [_hasRoutesKey, setHasRoutesKey] = useState(false);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [factFilter, setFactFilter] = useState<string>("all");
  const [searchQuery, setSearchQuery] = useState("");

  const load = useCallback(async () => {
    try {
      const [s, r, e, t, m, ppl, keys, fixes, rv, pend] = await Promise.all([
        invoke<MemoryStatus>("memcore_status"),
        invoke<MemoryRow[]>("memcore_list"),
        invoke<EgressEntry[]>("memcore_egress_log", { limit: 50 }),
        invoke<TimetableSlot[]>("memcore_timetable"),
        invoke<MailRow[]>("memcore_mail_list"),
        invoke<PersonRow[]>("memcore_people"),
        invoke<{ service: string; has_key: boolean }[]>("api_keys_list").catch(() => [] as { service: string; has_key: boolean }[]),
        invoke<CorrectionRow[]>("memcore_corrections").catch(() => [] as CorrectionRow[]),
        invoke<RoutinesView>("memcore_routines").catch(() => ({ routines: [], suggestion: null, limit: 30 }) as RoutinesView),
        invoke<PendingRow[]>("memcore_pending").catch(() => [] as PendingRow[]),
      ]);
      setStatus(s);
      setRows(r);
      setEgress(e);
      setSlots(t);
      setMail(m);
      setPeople(ppl);
      setCorrections(fixes);
      setRoutinesView(rv);
      setPending(pend);
      setHasRoutesKey(keys.some((k) => k.service === "google_maps" && k.has_key));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const now = Math.floor(Date.now() / 1000);

  const forget = async (row: MemoryRow) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memory_forget", { key: row.key });
      setConfirm(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const setFlag = async (flag: MemoryFlag, on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      const s = await invoke<Record<string, unknown>>("get_settings");
      await invoke("save_settings", {
        settings: { ...s, [flag]: on, groqApiKey: "", geminiApiKey: "", cerebrasApiKey: "" },
      });
      onFlagChanged?.(flag, on);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const muteSender = async (email: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_mail_mute", { email });
      setConfirm(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const flagPerson = async (jid: string, kind: "vip" | "mute", on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_person_flag", { jid, kind, on });
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runSelftest = async () => {
    setBusy(true);
    setError(null);
    try {
      setSelftest(await invoke<WhatsappSelfTest>("whatsapp_selftest"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const decide = async (id: number, approve: boolean) => {
    setBusy(true);
    setError(null);
    try {
      if (approve) await invoke("memcore_pending_approve", { id });
      else await invoke("memcore_pending_reject", { id });
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const saveRoutine = async () => {
    if (!routineName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_routine_save", { name: routineName.trim() });
      setRoutineName("");
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeRoutine = async (name: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_routine_delete", { name });
      setConfirm(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeCorrection = async (heard: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_correction_forget", { heard });
      setConfirm(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const removeSlot = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_timetable_delete", { id });
      setConfirm(null);
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const clearActivity = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("memcore_clear_activity");
      await load();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // Filtered facts list
  const filteredRows = useMemo(() => {
    return rows.filter((r) => {
      if (factFilter === "pinned" && !r.pinned) return false;
      if (factFilter === "fact" && r.tier !== "fact") return false;
      if (factFilter === "profile" && r.tier !== "profile") return false;
      if (searchQuery.trim()) {
        const q = searchQuery.toLowerCase();
        return (
          r.key.toLowerCase().includes(q) ||
          r.value.toLowerCase().includes(q) ||
          (r.source && r.source.toLowerCase().includes(q))
        );
      }
      return true;
    });
  }, [rows, factFilter, searchQuery]);

  return (
    <div className="hx-page">
      {error && <div className="hx-error" onClick={() => setError(null)}>{error}</div>}

      {/* ── KPI Metrics Bento ── */}
      <div className="hx-metrics-bento">
        <div className="hx-metric-card">
          <div className="hx-metric-head">
            <span className="hx-metric-lbl">Facts Stored</span>
            <svg className="hx-metric-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 2a7 7 0 0 0-7 7c0 2.38 1.19 4.47 3 5.74V17a2 2 0 0 0 2 2h4a2 2 0 0 0 2-2v-2.26c1.81-1.27 3-3.36 3-5.74a7 7 0 0 0-7-7M9 21h6" />
            </svg>
          </div>
          <div className="hx-metric-val">{status ? status.facts : "–"}</div>
        </div>

        <div className="hx-metric-card">
          <div className="hx-metric-head">
            <span className="hx-metric-lbl">Episodes</span>
            <svg className="hx-metric-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
          </div>
          <div className="hx-metric-val">{status ? status.episodes : "–"}</div>
        </div>

        <div className="hx-metric-card">
          <div className="hx-metric-head">
            <span className="hx-metric-lbl">Cloud Egress (7d)</span>
            <svg className="hx-metric-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z" />
            </svg>
          </div>
          <div className="hx-metric-val">{status ? status.egress_7d : "–"}</div>
        </div>

        <div className="hx-metric-card">
          <div className="hx-metric-head">
            <span className="hx-metric-lbl">Local Security</span>
            <svg className="hx-metric-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="11" width="18" height="11" rx="2" ry="2" />
              <path d="M7 11V7a5 5 0 0 1 10 0v4" />
            </svg>
          </div>
          <div className="hx-metric-val" style={{ fontSize: "14px", color: status?.encrypted ? "#34d399" : "#fbbf24" }}>
            {status?.encrypted ? "Encrypted" : "Local Plain"}
          </div>
        </div>
      </div>

      {/* ── Segmented Navigation ── */}
      <div className="hx-tab-nav">
        <button
          type="button"
          className={`hx-tab-btn ${activeTab === "privacy" ? "is-active" : ""}`}
          onClick={() => setActiveTab("privacy")}
          aria-pressed={activeTab === "privacy"}
        >
          🛡️ Privacy & AI
        </button>
        <button
          type="button"
          className={`hx-tab-btn ${activeTab === "facts" ? "is-active" : ""}`}
          onClick={() => setActiveTab("facts")}
          aria-pressed={activeTab === "facts"}
        >
          🧠 Facts Vault ({rows.length})
        </button>
        <button
          type="button"
          className={`hx-tab-btn ${activeTab === "people" ? "is-active" : ""}`}
          onClick={() => setActiveTab("people")}
          aria-pressed={activeTab === "people"}
        >
          👥 People & Agenda ({people.length + slots.length})
        </button>
        <button
          type="button"
          className={`hx-tab-btn ${activeTab === "egress" ? "is-active" : ""}`}
          onClick={() => setActiveTab("egress")}
          aria-pressed={activeTab === "egress"}
        >
          🌐 Cloud Egress ({egress.length})
        </button>
      </div>

      {/* ── TAB 1: PRIVACY & INTELLIGENCE ── */}
      {activeTab === "privacy" && status && (
        <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
          {/* Anonymize Names */}
          <SwitchCard
            icon="🛡️"
            title="Anonymize Names"
            desc="Replaces real names with tokens before cloud sends."
            checked={status.redact_names}
            disabled={busy}
            onChange={(c) => setFlag("memcoreRedactNames", c)}
          />

          {/* Context & Work Awareness */}
          <SwitchCard
            icon="💻"
            title="Workspace Context"
            desc="Resumes IDE windows where you left off."
            checked={status.activity}
            disabled={busy}
            onChange={(c) => setFlag("memcoreActivity", c)}
            extraAction={
              status.resume_points > 0 ? (
                <button
                  className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                  style={{ marginTop: 2 }}
                  disabled={busy}
                  onClick={clearActivity}
                >
                  Clear Points
                </button>
              ) : null
            }
          />

          {/* Morning Briefing */}
          <SwitchCard
            icon="🌅"
            title="Daily Morning Briefing"
            desc="Brief morning summary of tasks and commits."
            checked={status.briefing}
            disabled={busy}
            onChange={(c) => setFlag("memcoreBriefing", c)}
          />

          {/* Owner's name from the Google account */}
          <SwitchCard
            icon="🪪"
            title="Know Who I Am"
            desc="Learns your name from your Google account (name only). Never overwrites what you told me."
            checked={status.account_seed ?? true}
            disabled={busy}
            onChange={(c) => setFlag("memcoreAccountSeed", c)}
          />

          {/* Inbox Priority Guard */}
          <SwitchCard
            icon="📬"
            title="Priority Inbox Monitor"
            desc="Monitors Gmail for exams, PRs, and database alerts."
            checked={status.mail}
            disabled={busy}
            onChange={(c) => setFlag("memcoreMail", c)}
          />

          {/* Plain-word Email Alerts */}
          <SwitchCard
            icon="🗣️"
            title="Say Important Emails in Plain Words"
            desc="Off by default. When on, emails the on-device rules can't phrase are re-worded by a free cloud model (sender and subject only). GitHub, database, hackathon and exam notices are always done on your device."
            checked={status.mail_plain === true}
            disabled={busy}
            onChange={(c) => setFlag("memcoreMailPlain", c)}
          />

          {/* GitHub Actions Build Watcher */}
          <SwitchCard
            icon="🔨"
            title="Tell me when builds fail"
            desc="Watches the latest GitHub Actions runs of the repos you push to; says it once when a build fails and once when it is green again. Needs your GitHub sign-in."
            checked={status.github_watch !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreGithubWatch", c)}
          />

          {/* Deploy Watcher */}
          <SwitchCard
            icon="🚀"
            title="Tell me when deploys fail"
            desc="Watches the latest production deploy of your Vercel, Render and Netlify projects; says it once when one fails and once when it is live again. Needs the token you saved in Connections."
            checked={status.deploy_watch !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreDeployWatch", c)}
          />

          {/* WhatsApp Priority Voice Guard */}
          <SwitchCard
            icon="💬"
            title="WhatsApp Priority Guard"
            desc="Voice alerts for VIP contacts only."
            checked={!!status.whatsapp}
            disabled={busy}
            onChange={(c) => setFlag("memcoreWhatsapp", c)}
            extraAction={
              <div style={{ display: "flex", gap: "6px", alignItems: "center", marginTop: 4 }}>
                <button className="hx-btn hx-btn--tiny" disabled={busy} onClick={runSelftest}>
                  Test Link
                </button>
                {selftest && <span className="hx-fact-meta">{selftestSummary(selftest)}</span>}
              </div>
            }
          />

          {/* Live Traffic Event Departure */}
          <SwitchCard
            icon="🚗"
            title="Travel Departure Alerts"
            desc="Live traffic warnings for upcoming events."
            checked={status.leave_by !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreLeaveBy", c)}
          />

          {/* Timetable Reminders */}
          <SwitchCard
            icon="⏰"
            title="Timetable Reminders"
            desc="Starts scheduled focus blocks proactively."
            checked={status.timetable}
            disabled={busy}
            onChange={(c) => setFlag("memcoreTimetable", c)}
          />

          {/* Adaptive Macros */}
          <SwitchCard
            icon="⚡"
            title="Adaptive Voice Macros"
            desc="Auto-detects repeated multi-step commands."
            checked={status.routines !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreRoutines", c)}
          />

          {/* Session Restore */}
          <SwitchCard
            icon="🔄"
            title="Session Restore"
            desc="Restores workspace tabs and apps after restart."
            checked={status.session !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreSession", c)}
          />

          {/* System Health Probe */}
          <SwitchCard
            icon="🩺"
            title="System Health Check"
            desc="Self-tests audio and network ports on launch."
            checked={status.health !== false}
            disabled={busy}
            onChange={(c) => setFlag("memcoreHealth", c)}
          />
        </div>
      )}

      {/* ── TAB 2: FACTS VAULT ── */}
      {activeTab === "facts" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "10px" }}>
          {/* Search & Filter Bar */}
          <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
            <input
              type="text"
              className="hx-input"
              placeholder="Search memories, facts, preferences..."
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              style={{ flex: 1 }}
            />
            <div className="hx-category-tabs" style={{ margin: 0 }}>
              {(["all", "pinned", "fact", "profile"] as const).map((f) => (
                <button
                  key={f}
                  type="button"
                  className={`hx-cat-btn ${factFilter === f ? "is-active" : ""}`}
                  onClick={() => setFactFilter(f)}
                  aria-pressed={factFilter === f}
                >
                  {f === "all" ? "All" : f === "pinned" ? "Pinned" : f === "fact" ? "Facts" : "Profile"}
                </button>
              ))}
            </div>
          </div>

          {/* Pending Facts for Approval */}
          {pending.length > 0 && (
            <div className="hx-card" style={{ padding: 12, border: "1px solid #f2b859" }}>
              <div className="hx-switch-title" style={{ color: "#f2b859" }}>
                ⏳ Pending Review ({pending.length})
              </div>
              <div className="hx-switch-sub">Suggested from recent interactions. Choose whether to store or discard.</div>
              {pending.map((p) => (
                <div key={p.id} style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: 8 }}>
                  <div>
                    <div style={{ fontSize: 13, fontWeight: 600, color: "var(--ui-text-primary)" }}>{p.key}</div>
                    <div className="hx-switch-sub">{p.value}</div>
                  </div>
                  <div style={{ display: "flex", gap: 6 }}>
                    <button className="hx-btn hx-btn--tiny hx-btn--primary" onClick={() => decide(p.id, true)}>Approve</button>
                    <button className="hx-btn hx-btn--tiny hx-btn--danger-ghost" onClick={() => decide(p.id, false)}>Discard</button>
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Facts List */}
          {filteredRows.length === 0 && (
            <div className="hx-card" style={{ padding: 24, textAlign: "center", color: "var(--ui-text-tertiary)" }}>
              No facts match your query. NEXUS learns your habits and projects automatically as you speak.
            </div>
          )}

          {filteredRows.map((r) => (
            <div className="hx-fact-card" key={r.id}>
              <div className="hx-fact-head">
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <span className="hx-fact-key">{keyLabel(r)}</span>
                  {r.pinned && <span className="hx-pill hx-pill--on">Pinned</span>}
                  <span className="hx-badge" style={{ background: "var(--ui-surface-hover)" }}>{r.tier}</span>
                </div>
                {confirm === `${r.id}` ? (
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    <span style={{ fontSize: 11, color: "#ff5a52" }}>Erase?</span>
                    <button className="hx-btn hx-btn--tiny" onClick={() => setConfirm(null)}>Cancel</button>
                    <button className="hx-btn hx-btn--tiny hx-btn--danger" disabled={busy} onClick={() => forget(r)}>
                      Yes, Forget
                    </button>
                  </div>
                ) : (
                  <button
                    className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                    onClick={() => setConfirm(`${r.id}`)}
                    title="Remove this memory"
                  >
                    Forget
                  </button>
                )}
              </div>
              <div className="hx-fact-val">{r.value}</div>
              <div className="hx-fact-meta">
                <span>{sourceLabel(r.source)}</span>
                <span>•</span>
                <span>Added {ageLabel(now, r.created)}</span>
                {r.uses ? <span>• Used {r.uses} times</span> : null}
              </div>
            </div>
          ))}
        </div>
      )}

      {/* ── TAB 3: PEOPLE & TIMETABLE ── */}
      {activeTab === "people" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "16px" }}>
          {/* Priority People */}
          <div>
            <div className="hx-section-title">Priority People & Contacts ({people.length})</div>
            <div className="hx-section-desc">Contacts whose messages trigger voice alerts and executive summaries.</div>
            {people.length === 0 ? (
              <div className="hx-card" style={{ padding: 16, color: "var(--ui-text-tertiary)", fontSize: 12 }}>
                No priority people detected yet. People are indexed automatically when you frequently collaborate with them.
              </div>
            ) : (
              <div className="hx-people-grid" style={{ marginTop: 8 }}>
                {people.map((p) => (
                  <div className="hx-person-card" key={p.jid}>
                    <div className="hx-person-top">
                      <div className="hx-person-avatar">{p.name ? p.name.charAt(0).toUpperCase() : "P"}</div>
                      <div className="hx-person-info">
                        <div className="hx-person-name">{p.name || p.jid}</div>
                        <div className="hx-person-role">{p.why || "Collaborator"}</div>
                      </div>
                    </div>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 4 }}>
                      <span className="hx-fact-meta">Affinity: {p.score}%</span>
                      <div style={{ display: "flex", gap: 6 }}>
                        <button
                          className={`hx-btn hx-btn--tiny ${p.vip ? "hx-btn--primary" : ""}`}
                          onClick={() => flagPerson(p.jid, "vip", !p.vip)}
                        >
                          {p.vip ? "★ VIP" : "☆ Mark VIP"}
                        </button>
                        <button
                          className={`hx-btn hx-btn--tiny ${p.muted ? "hx-btn--danger" : ""}`}
                          onClick={() => flagPerson(p.jid, "mute", !p.muted)}
                        >
                          {p.muted ? "Unmute" : "Mute"}
                        </button>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Important Mail */}
          <div>
            <div className="hx-section-title">Important Emails ({mail.length})</div>
            <div className="hx-section-desc">Critical notifications from GitHub, databases, and deadlines filed for 7 days.</div>
            {mail.length === 0 ? (
              <div className="hx-card" style={{ padding: 16, color: "var(--ui-text-tertiary)", fontSize: 12 }}>
                No important emails filed. Senders and short snippets are analyzed only when new mail arrives.
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
                {mail.map((m) => (
                  <div className="hx-card" key={m.key} style={{ padding: "10px 14px" }}>
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                        <span style={{ fontSize: 13, fontWeight: 600, color: "var(--ui-text-primary)" }}>{m.subject || "(no subject)"}</span>
                        <span className="hx-pill hx-pill--on" style={{ fontSize: 9.5 }}>{categoryLabel(m.category)}</span>
                      </div>
                      <button
                        className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                        onClick={() => muteSender(m.email)}
                        title={`Mute notifications from ${m.email}`}
                      >
                        Mute Sender
                      </button>
                    </div>
                    <div className="hx-fact-meta" style={{ marginTop: 4 }}>
                      From: {m.sender} ({m.email}) • {ageLabel(now, Math.floor(m.ts_ms / 1000))}
                    </div>
                    <div style={{ fontSize: 12, color: "var(--ui-text-secondary)", marginTop: 4 }}>
                      {m.snippet}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Timetable Agenda */}
          <div>
            <div className="hx-section-title">Timetable Agenda Slots ({slots.length})</div>
            <div className="hx-section-desc">Recurring schedules NEXUS watches to proactively prepare workspaces.</div>
            {slots.length === 0 ? (
              <div className="hx-card" style={{ padding: 16, color: "var(--ui-text-tertiary)", fontSize: 12 }}>
                No agenda slots created. Say "remind me every day at 10 AM for standup" to create one.
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
                {slots.map((s) => (
                  <div className="hx-card" key={s.id} style={{ padding: "10px 14px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <div>
                      <div style={{ fontSize: 13, fontWeight: 600, color: "var(--ui-text-primary)" }}>{s.title}</div>
                      <div className="hx-fact-meta" style={{ marginTop: 2 }}>
                        ⏰ {fmtSlotTime(s)} • {fmtDays(s.days)}
                      </div>
                    </div>
                    <button
                      className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                      onClick={() => removeSlot(s.id)}
                    >
                      Delete
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Voice Routines */}
          <div>
            <div className="hx-section-title">Repeating Voice Macros ({routinesView.routines.length})</div>
            <div className="hx-section-desc">Chains of actions executed from a single spoken phrase.</div>
            <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
              <input
                className="hx-input"
                placeholder="New routine trigger phrase, e.g. Start Work"
                value={routineName}
                onChange={(e) => setRoutineName(e.target.value)}
                style={{ flex: 1 }}
              />
              <button
                className="hx-btn hx-btn--primary"
                disabled={busy || !routineName.trim()}
                onClick={saveRoutine}
              >
                Save Routine
              </button>
            </div>
            {routinesView.routines.length === 0 ? (
              <div className="hx-card" style={{ padding: 16, color: "var(--ui-text-tertiary)", fontSize: 12, marginTop: 8 }}>
                No custom voice macros saved yet.
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
                {routinesView.routines.map((r) => (
                  <div className="hx-card" key={r.name} style={{ padding: "10px 14px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <div>
                      <div style={{ fontSize: 13, fontWeight: 600, color: "var(--ui-text-primary)" }}>"{r.name}"</div>
                      <div className="hx-fact-meta">{routineUse(r)} • {r.steps.join(" → ")}</div>
                    </div>
                    <button
                      className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                      onClick={() => removeRoutine(r.name)}
                    >
                      Delete
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Voice Corrections */}
          {corrections.length > 0 && (
            <div>
              <div className="hx-section-title">Voice Hearing Corrections ({corrections.length})</div>
              <div className="hx-section-desc">Phonetic mishearings automatically adapted from your clarifications.</div>
              <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
                {corrections.map((c) => (
                  <div className="hx-card" key={c.heard} style={{ padding: "10px 14px", display: "flex", alignItems: "center", justifyContent: "space-between" }}>
                    <div>
                      <div style={{ fontSize: 13, fontWeight: 600, color: "var(--ui-text-primary)" }}>{describeCorrection(c)}</div>
                      <div className="hx-fact-meta">Last seen {ageLabel(now, Math.floor(c.last_seen / 1000))}</div>
                    </div>
                    <button
                      className="hx-btn hx-btn--tiny hx-btn--danger-ghost"
                      onClick={() => removeCorrection(c.heard)}
                    >
                      Forget
                    </button>
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      )}

      {/* ── TAB 4: CLOUD EGRESS TRANSPARENCY ── */}
      {activeTab === "egress" && (
        <div style={{ display: "flex", flexDirection: "column", gap: "8px" }}>

          {egress.length === 0 ? (
            <div className="hx-card" style={{ padding: 24, textAlign: "center", color: "var(--ui-text-tertiary)" }}>
              No cloud transmissions in the last 7 days. Everything was resolved entirely on this machine.
            </div>
          ) : (
            egress.map((e, idx) => (
              <div className="hx-card" key={idx} style={{ padding: "12px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
                <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <span className="hx-pill hx-pill--on" style={{ fontSize: 10 }}>{e.route}</span>
                  <span className="hx-fact-meta">{ageLabel(now, Math.floor(e.ts / 1000))}</span>
                </div>
                <div style={{ fontSize: 12.5, fontWeight: 600, color: "var(--ui-text-primary)" }}>
                  Request: <span style={{ fontWeight: 400, color: "var(--ui-text-primary)" }}>{e.request}</span>
                </div>
                {e.memory && (
                  <div style={{ fontSize: 11, color: "var(--ui-text-secondary)", background: "var(--ui-surface-sunken)", padding: "6px 8px", borderRadius: 6 }}>
                    Attached Memory: {e.memory}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
