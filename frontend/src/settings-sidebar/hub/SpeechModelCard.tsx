import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { canDownload, parseSpeechModelStatus, speechModelLine, type SpeechModelStatus } from "./speechModelModel";

/** The local speech model: whether it is on this disk, and a button to fetch it in the background (no server is started). */
export function SpeechModelCard() {
  const [status, setStatus] = useState<SpeechModelStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(async () => {
    try {
      const s = parseSpeechModelStatus(await invoke("stt_model_status"));
      setStatus(s);
      return s;
    } catch {
      return null;
    }
  }, []);

  // check now, and every 3 seconds while a download runs
  useEffect(() => {
    let alive = true;
    const loop = async () => {
      const s = await refresh();
      if (!alive) return;
      timer.current = setTimeout(loop, s?.state === "downloading" ? 3000 : 15000);
    };
    void loop();
    return () => {
      alive = false;
      if (timer.current) clearTimeout(timer.current);
    };
  }, [refresh]);

  const download = async () => {
    setError(null);
    try {
      setStatus(parseSpeechModelStatus(await invoke("stt_model_prefetch")));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div style={{ padding: "8px 0", borderTop: "1px solid var(--ui-hairline, rgba(128,128,128,0.18))" }} data-testid="speech-model-card">
      <div className="hx-switch-title">Local speech model</div>
      <div className="hx-switch-sub">{speechModelLine(status)}</div>
      {error && <div className="hx-switch-sub" role="alert">{error}</div>}
      {canDownload(status) && (
        <div style={{ marginTop: 6 }}>
          <button className="hx-btn hx-btn--tiny" onClick={() => void download()}>Download now</button>
        </div>
      )}
    </div>
  );
}
