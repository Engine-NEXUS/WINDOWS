import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Mode = "auto" | "server" | "google" | "open";

interface Props {
  settings: { routingMode?: string; routingServerUrl?: string };
  update: (key: "routingMode" | "routingServerUrl", value: string) => void;
}

const MODES: { id: Mode; label: string; desc: string }[] = [
  { id: "auto", label: "Automatic (recommended)", desc: "Your server if you set one, then your Google key, then the free open service." },
  { id: "server", label: "My NEXUS server", desc: "Only your own routing server. Free and unlimited; normal-traffic times, real motorbike times." },
  { id: "google", label: "My Google key", desc: "Live traffic and two-wheeler times from Google. Uses your own free monthly allowance, then Google bills you." },
  { id: "open", label: "Free open service only", desc: "No setup, no key. Car time only, bike time is estimated." },
];

/** Where the place card gets distance and travel time (Command Hub, Connections). */
export function RoutingSection({ settings, update }: Props) {
  const mode = (MODES.some((m) => m.id === settings.routingMode) ? settings.routingMode : "auto") as Mode;
  const [token, setToken] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const saveToken = async () => {
    if (!token.trim()) return;
    setBusy(true);
    try {
      await invoke("api_key_set", { service: "routing_server", key: token.trim() });
      setToken("");
      setMsg({ ok: true, text: "Token saved in the system keychain." });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const test = async () => {
    setBusy(true);
    setMsg(null);
    try {
      const text = await invoke<string>("routing_server_test", { url: settings.routingServerUrl ?? "" });
      setMsg({ ok: true, text });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-section">
      <div className="settings-section-title">Distance and travel time</div>
      <div className="setting-desc" style={{ marginBottom: 10 }}>
        Where the place card gets the distance and the car and bike times.
      </div>
      <div role="radiogroup" aria-label="Routing source" style={{ display: "grid", gap: 8 }}>
        {MODES.map((m) => (
          <label key={m.id} className={`option-card${mode === m.id ? " option-card--active" : ""}`} style={{ display: "flex", gap: 10, alignItems: "flex-start", cursor: "pointer" }}>
            <input type="radio" name="routing-mode" checked={mode === m.id} onChange={() => update("routingMode", m.id)} style={{ marginTop: 3 }} />
            <span>
              <b>{m.label}</b>
              <span className="setting-desc" style={{ display: "block" }}>
                {m.desc}
              </span>
            </span>
          </label>
        ))}
      </div>

      <div className="setting-row" style={{ marginTop: 12 }}>
        <label htmlFor="routing-url">Server address</label>
        <input id="routing-url" type="url" inputMode="url" placeholder="https://route.example.com" value={settings.routingServerUrl ?? ""} onChange={(e) => update("routingServerUrl", e.target.value)} style={{ width: "100%" }} />
        <div className="setting-desc">https, or http only for a server on this computer or your home network. Remember to Save.</div>
      </div>
      <div className="setting-row">
        <label htmlFor="routing-token">Server token</label>
        <div style={{ display: "flex", gap: 8 }}>
          <input id="routing-token" type="password" autoComplete="off" placeholder="paste the token, then Save token" value={token} onChange={(e) => setToken(e.target.value)} style={{ flex: 1, minWidth: 0 }} />
          <button type="button" className="settings-btn" disabled={busy || !token.trim()} onClick={saveToken}>
            Save token
          </button>
        </div>
      </div>
      <button type="button" className="settings-btn" disabled={busy} onClick={test} style={{ marginTop: 6 }}>
        Test server
      </button>
      {msg && (
        <div role="status" className="setting-desc" style={{ marginTop: 8, color: msg.ok ? "var(--ui-success-text)" : "var(--ui-danger-text)" }}>
          {msg.text}
        </div>
      )}
    </div>
  );
}
