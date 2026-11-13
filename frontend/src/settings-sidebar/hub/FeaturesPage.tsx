import { useCallback, useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  APPLY_LABEL,
  FEATURES,
  GROUP_ORDER,
  RISK_LABEL,
  blockedBy,
  defaultsPatch,
  effectiveValue,
  isOn,
  matches,
  patchFor,
  safeModePatch,
  type Feature,
  type Filter,
  type SettingsMap,
} from "./featureRegistry";
import { writeAndVerify, type Invoke } from "./featureWrite";
import { FEATURE_TESTS, runFullCheck, runTest, type AuditReport, type TestResult } from "./featureTests";
import readiness from "./featureReadiness.json";
import { appendLog, loadLog, type LogEntry } from "./featureLog";

type Confirm = { kind: "off"; id: string } | { kind: "safe" } | { kind: "defaults" } | null;

const FILTERS: { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "on", label: "On" },
  { id: "off", label: "Off" },
  { id: "risky", label: "Needs care" },
];

/** One place to switch every feature on or off. Writes go through `settings_patch` (only the changed keys), and a switch
 *  only moves once `get_settings` confirms the backend really uses the new value. */
export function FeaturesPage() {
  const [settings, setSettings] = useState<SettingsMap | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [confirm, setConfirm] = useState<Confirm>(null);
  const [results, setResults] = useState<Record<string, TestResult>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [audit, setAudit] = useState<AuditReport | null>(null);
  const [log, setLog] = useState<LogEntry[]>(() => loadLog());

  const reload = useCallback(async (): Promise<SettingsMap> => {
    const s = await invoke<SettingsMap>("get_settings");
    setSettings(s);
    return s;
  }, []);

  useEffect(() => {
    reload().catch((e) => setError(`Could not read settings: ${String(e)}`));
  }, [reload]);

  /** Apply a patch, then verify against what the backend now reports. */
  const apply = useCallback(async (patch: SettingsMap, label: string): Promise<boolean> => {
    setBusy(label);
    setError(null);
    setNote(null);
    const r = await writeAndVerify(invoke as Invoke, patch);
    if (r.settings) setSettings(r.settings);
    else await reload().catch(() => {});
    if (!r.ok) setError(r.error ?? "Could not save.");
    else setLog(appendLog(Object.entries(patch).map(([k, v]) => `${k} = ${String(v)}`).join(", ")));
    setBusy(null);
    return r.ok;
  }, [reload]);

  const flip = (f: Feature, value: boolean | string) => {
    if (f.safety && !confirm && ((f.kind === "bool" && value === false) || (f.kind === "enum" && value === f.options?.[0]))) {
      setConfirm({ kind: "off", id: f.id });
      return;
    }
    void apply(patchFor(f, value), f.id);
  };

  const test = async (id: string) => {
    setTesting(id);
    const r = await runTest(invoke as Invoke, id);
    setResults((m) => ({ ...m, [id]: r }));
    setTesting(null);
  };

  const fullCheck = async () => {
    setTesting("audit");
    setError(null);
    try {
      setAudit(await runFullCheck(invoke as Invoke));
    } catch (e) {
      setError(`Full check failed: ${String(e)}`);
    }
    setTesting(null);
  };

  const list = useMemo(() => (settings ? FEATURES.filter((f) => matches(settings, f, filter, query)) : []), [settings, filter, query]);
  const onCount = settings ? FEATURES.filter((f) => isOn(settings, f)).length : 0;

  if (!settings) {
    return (
      <div className="hx-page">
        {error ? <div className="hx-error">{error}</div> : <div className="hx-empty">Reading settings…</div>}
      </div>
    );
  }

  const safePatch = safeModePatch(settings);
  const safeCount = Object.keys(safePatch).length;

  return (
    <div className="hx-page">
      {error && (
        <div className="hx-error" onClick={() => setError(null)} role="alert">
          {error}
        </div>
      )}
      {note && <div className="hx-note">{note}</div>}

      <div className="hx-card" style={{ padding: "12px 14px" }}>
        <div className="hx-switch-title">{onCount} of {FEATURES.length} features on</div>
        <div className="hx-switch-sub">Each switch takes effect as shown on its row. A switch only moves when NEXUS confirms the change.</div>
        <input className="hx-input" placeholder="Search features" value={query} onChange={(e) => setQuery(e.target.value)} aria-label="Search features" style={{ marginTop: 10 }} />
        <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
          {FILTERS.map((x) => (
            <button key={x.id} className={`hx-btn hx-btn--tiny ${filter === x.id ? "hx-btn--primary" : ""}`} onClick={() => setFilter(x.id)}>
              {x.label}
            </button>
          ))}
        </div>
        <div style={{ display: "flex", gap: 6, marginTop: 8, flexWrap: "wrap" }}>
          <button className="hx-btn hx-btn--tiny" disabled={busy !== null || safeCount === 0} onClick={() => setConfirm({ kind: "safe" })}>
            Safe mode{safeCount > 0 ? ` (turns off ${safeCount})` : ""}
          </button>
          <button className="hx-btn hx-btn--tiny" disabled={busy !== null} onClick={() => setConfirm({ kind: "defaults" })}>
            Restore defaults
          </button>
          <button className="hx-btn hx-btn--tiny" disabled={testing !== null} onClick={() => void fullCheck()}>
            {testing === "audit" ? "Checking…" : "Run full check"}
          </button>
        </div>
        {audit && (
          <div style={{ marginTop: 8 }}>
            <div className="hx-switch-sub">{audit.allHealthy ? "All checks passed." : "Some checks need attention."}</div>
            {audit.items.map((i, n) => (
              <div key={n} className="hx-switch-sub">{i.status} · {i.name}: {i.detail}</div>
            ))}
          </div>
        )}

        {confirm?.kind === "safe" && (
          <div className="hx-confirm-inline" style={{ marginTop: 8 }}>
            <span className="hx-switch-sub">Turns off {safeCount} features that listen, watch, act on the desktop or send data out. Voice Lock and local-only speech stay as they are.</span>
            <button className="hx-btn hx-btn--tiny" onClick={() => setConfirm(null)}>Keep</button>
            <button
              className="hx-btn hx-btn--tiny hx-btn--danger"
              onClick={() => {
                setConfirm(null);
                void apply(safePatch, "safe").then((ok) => ok && setNote(`Safe mode: ${safeCount} features turned off.`));
              }}
            >
              Turn them off
            </button>
          </div>
        )}
        {confirm?.kind === "defaults" && (
          <div className="hx-confirm-inline" style={{ marginTop: 8 }}>
            <span className="hx-switch-sub">Puts every switch back to its default, except Voice Lock, local-only speech and the wake check, which stay as you set them. Keys, accounts and other settings are not touched.</span>
            <button className="hx-btn hx-btn--tiny" onClick={() => setConfirm(null)}>Keep</button>
            <button
              className="hx-btn hx-btn--tiny hx-btn--danger"
              onClick={() => {
                setConfirm(null);
                void apply(defaultsPatch(), "defaults").then((ok) => ok && setNote("Every switch is back to its default."));
              }}
            >
              Restore
            </button>
          </div>
        )}
      </div>

      {GROUP_ORDER.map((g) => {
        const rows = list.filter((f) => f.group === g);
        if (rows.length === 0) return null;
        return (
          <div className="hx-card" key={g} style={{ padding: "6px 14px" }}>
            <div className="hx-switch-title" style={{ padding: "8px 0 4px" }}>{g}</div>
            {rows.map((f) => {
              const parent = blockedBy(settings, f);
              const on = isOn(settings, f);
              const value = effectiveValue(settings, f);
              const risk = RISK_LABEL[f.risk];
              return (
                <div key={f.id} style={{ padding: "8px 0", borderTop: "1px solid var(--ui-hairline, rgba(128,128,128,0.18))", opacity: parent ? 0.6 : 1 }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: 10 }}>
                    <div className="hx-switch-title">{f.title}</div>
                    {f.kind === "bool" ? (
                      <label className="hx-toggle">
                        <input type="checkbox" checked={on} disabled={busy !== null} aria-label={f.title} onChange={(e) => flip(f, e.target.checked)} />
                        <span className="hx-toggle-slider" />
                      </label>
                    ) : (
                      <select className="hx-input" style={{ width: "auto" }} value={String(value)} disabled={busy !== null} aria-label={f.title} onChange={(e) => flip(f, e.target.value)}>
                        {f.options?.map((o) => (
                          <option key={o} value={o}>
                            {o}
                          </option>
                        ))}
                      </select>
                    )}
                  </div>
                  <div className="hx-switch-sub">{f.what}</div>
                  <div className="hx-switch-sub" style={{ opacity: 0.8 }}>
                    {[APPLY_LABEL[f.apply], risk, f.safety ? "Protects you" : "", parent ? `Needs "${parent.title}" on` : "", (readiness as Record<string, { live: number }>)[f.id] ? `Live proof ${(readiness as Record<string, { live: number }>)[f.id].live}%` : "Live proof not tracked"]
                      .filter(Boolean)
                      .join(" · ")}
                  </div>
                  {FEATURE_TESTS[f.id] ? (
                    <div style={{ marginTop: 6 }}>
                      <button className="hx-btn hx-btn--tiny" disabled={testing !== null} onClick={() => void test(f.id)}>
                        {testing === f.id ? "Running…" : FEATURE_TESTS[f.id].label}
                      </button>
                      {results[f.id] && (
                        <span className="hx-switch-sub" style={{ marginLeft: 8 }}>
                          {results[f.id].status} · {results[f.id].detail} · {new Date(results[f.id].at).toLocaleTimeString()}
                        </span>
                      )}
                    </div>
                  ) : (
                    <div className="hx-switch-sub" style={{ opacity: 0.6 }}>No test button yet. Use the live checklist.</div>
                  )}
                  {confirm?.kind === "off" && confirm.id === f.id && (
                    <div className="hx-confirm-inline" style={{ marginTop: 6 }}>
                      <span className="hx-switch-sub">This turns off a protection. Continue?</span>
                      <button className="hx-btn hx-btn--tiny" onClick={() => setConfirm(null)}>Keep on</button>
                      <button
                        className="hx-btn hx-btn--tiny hx-btn--danger"
                        onClick={() => {
                          setConfirm(null);
                          void apply(patchFor(f, f.kind === "bool" ? false : (f.options?.[0] ?? "off")), f.id);
                        }}
                      >
                        Turn off
                      </button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        );
      })}
      {list.length === 0 && <div className="hx-empty">No feature matches.</div>}
      {log.length > 0 && (
        <div className="hx-card" style={{ padding: "8px 14px" }}>
          <div className="hx-switch-title">What I switched</div>
          {log.slice(0, 10).map((l, n) => (
            <div key={n} className="hx-switch-sub">{new Date(l.at).toLocaleString()} · {l.text}</div>
          ))}
        </div>
      )}
    </div>
  );
}
