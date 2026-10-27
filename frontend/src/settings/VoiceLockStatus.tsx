import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { VoiceLockState } from "./voiceLockState";

export function useVoiceLockState() {
  const [model] = useState(() => new VoiceLockState((command, args) => invoke(command, args)));
  const [, render] = useState(0);
  useEffect(() => {
    model.onChange = () => render(n => n + 1);
    const refresh = () => { if (document.visibilityState !== "hidden") void model.refresh(); };
    refresh();
    window.addEventListener("focus", refresh);
    document.addEventListener("visibilitychange", refresh);
    return () => { model.onChange = null; window.removeEventListener("focus", refresh); document.removeEventListener("visibilitychange", refresh); };
  }, [model]);
  return model;
}

export function VoiceLockStatus({ model, draft }: { model: VoiceLockState; draft: boolean }) {
  const { effective, checking, saving, profileReady, error } = model.state;
  return <div style={{ fontSize: 13, margin: "8px 0" }} aria-live="polite">
    <div><strong>Effective Voice Lock: {checking ? "Checking…" : effective === null ? "Unknown" : effective ? "On" : "Off"}</strong></div>
    <div>Draft: {draft ? "On" : "Off"}. {saving ? "Saving and verifying…" : effective === null || draft !== effective ? "Click Save to apply, then verify the effective state." : "Draft matches the last native readback."}</div>
    {effective && !profileReady && <div role="alert">Voice Lock is on but a valid profile is unavailable. Voice actions remain blocked; use settings or keyboard recovery.</div>}
    {error && <div role="alert">{error}</div>}
    <button type="button" disabled={saving || checking} onClick={() => void model.refresh()}>Refresh native status</button>
  </div>;
}

/** An opacity-hidden message must not announce an unverified Save. */
export function VerifiedSaveStatus({ model, draft, saved }: { model: VoiceLockState; draft: boolean; saved: boolean }) {
  const { effective, error, saving, checking, saveVerified } = model.state;
  const verified = saved && saveVerified && !error && !saving && !checking && effective !== null && effective === draft;
  return <span className={`hx-saved ${verified ? "hx-saved--on" : ""}`} role="status" aria-hidden={!verified}>
    {verified ? "✓ Saved" : null}
  </span>;
}
