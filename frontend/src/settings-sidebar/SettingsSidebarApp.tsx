import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { isPlausibleGeminiKey } from "./keyFormat";
import { identityBanner, type IdentityBanner } from "../setup/identityBanner";
import {
  setSidecarBaseUrl,
  getOAuthStatus,
  connectOAuth,
  disconnectOAuth,
  type OAuthStatus,
} from "../setup/oauth";
import { useAssistant } from "../store/assistant";

/**
 * NEXUS Settings Sidebar
 *
 * Liquid-glass sidebar (720x1000) that controls the entire application:
 *   - Display: orb position sliders, orb size slider, live preview
 *   - Audio: TTS volume slider, test button
 *   - Auth: Google/GitHub OAuth connect/disconnect, Gemini/Groq API keys
 *
 * Persists to settings.json via save_settings Tauri command.
 * Overwrites on each save (std::fs::write).
 */

type Tab = "display" | "audio" | "auth" | "connections";

interface Settings {
  autostart: boolean;
  hotkey: string;
  autoHideDelay: number;
  wakeWordEnabled: boolean;
  wakePhrase: string;
  wakeSensitivity: string;
  speakerVerification: boolean;
  meetingModeAuto: boolean;
  suppressTtsInMeetings: boolean;
  localSttOnly: boolean;
  serverUrl: string;
  userId: string;
  deviceId: string;
  ttsVoice: string;
  speechRate: number;
  ttsVolume: number;
  ttsProvider: string;
  groqApiKey: string;
  edgeTtsVoice: string;
  // Iconic voice persona key (Feature 83 catalog, e.g. "jarvis")
  selectedVoice?: string;
  // Local Kokoro voice name for the selected persona (e.g. "bm_george")
  offlineVoiceModel?: string;
  // Orb position + size (Phase 2)
  orbHorizontalPct?: number;
  orbVerticalPct?: number;
  orbSize?: number;
  // Gemini API key (Phase 3)
  geminiApiKey?: string;
  // Cerebras API key (9Router — fastest free provider, ~80ms)
  cerebrasApiKey?: string;
  // Moonshine STT model (medium_streaming=6.65% WER, small_streaming=7.84%)
  moonshineModel?: string;
  // TTS voice emotion (auto = heuristics per reply; prosody via rate/pitch)
  ttsEmotion?: string;
  // Ghost vision provider order (auto = Groq → Gemini fallback)
  visionProvider?: string;
  // Ghost vision strategy (speed = race both providers in parallel)
  visionRace?: string;
  // Telegram owner chat id (remote bridge; token lives in vault)
  telegramChatId?: string;
  // Ghost FIFO queue: speak "Queued, sir." once per session at depth 2+ (D5/D6)
  ghostDepthAck?: boolean;
  // Ghost FIFO queue: per-step watchdog timeout, ms (D7)
  ghostStepTimeoutMs?: number;
  // Ghost FIFO queue: inter-command gap τ, ms (D4)
  ghostTurnGapMs?: number;
  // Ghost waves placement (ghost sessions reposition the orb window to this rect)
  wavesHorizontalPct?: number;
  wavesVerticalPct?: number;
  wavesSize?: number;
  // Loading indicator placement (center-anchored fractions + logical px)
  loadingHorizontalPct?: number;
  loadingVerticalPct?: number;
  loadingSize?: number;
  // Orb color theme (e.g. "#f2b859")
  orbColor?: string;
  // Orb fixed position ("top" | "bottom")
  orbPosition?: "top" | "bottom";
  // TEMPORARY dev flag (remove before release): auto-open the calibrator
  // on boot for live cross-checks. Never part of the save flow.
  calibrationDevPersist?: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  autostart: true,
  hotkey: "Ctrl+Space",
  autoHideDelay: 8,
  wakeWordEnabled: true,
  wakePhrase: "NEXUS",
  wakeSensitivity: "medium",
  speakerVerification: false,
  meetingModeAuto: true,
  suppressTtsInMeetings: true,
  localSttOnly: true,
  serverUrl: "",
  userId: "",
  deviceId: "",
  ttsVoice: "af_sky",
  speechRate: 1.15,
  ttsVolume: 75,
  ttsProvider: "kokoro",
  groqApiKey: "",
  edgeTtsVoice: "en-US-AvaNeural",
  orbHorizontalPct: 0.5,
  orbVerticalPct: 0.0,
  orbPosition: "top",
  orbSize: 200,
  geminiApiKey: "",
  cerebrasApiKey: "",
  moonshineModel: "medium_streaming",
  ttsEmotion: "auto",
  visionProvider: "auto",
  visionRace: "sequential",
  telegramChatId: "",
  ghostDepthAck: true,
  ghostStepTimeoutMs: 15000,
  ghostTurnGapMs: 1000,
  wavesHorizontalPct: 0.5,
  wavesVerticalPct: 1.0,
  wavesSize: 200,
  loadingHorizontalPct: 0.95,
  loadingVerticalPct: 0.05,
  loadingSize: 80,
  orbColor: "#f2b859",
  calibrationDevPersist: false,
};

const TABS: { id: Tab; label: string }[] = [
  { id: "display", label: "Display" },
  { id: "audio", label: "Audio" },
  { id: "auth", label: "Accounts" },
  { id: "connections", label: "Connections" },
];

export function SettingsSidebarApp({ onDock = () => {} }: { onDock?: (d: string) => void }) {
  const [tab, setTab] = useState<Tab>("display");
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [saved, setSaved] = useState(false);
  const [toast, setToast] = useState<string | null>(null);

  // Toast from the calibration round-trip (save/cancel re-open the hub).
  useEffect(() => {
    let un: (() => void) | null = null;
    import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ message?: string }>("settings:toast", (ev) => {
        if (ev.payload?.message) setToast(ev.payload.message);
      })
    ).then((u) => {
      un = u;
    });
    return () => {
      un?.();
    };
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3500);
    return () => clearTimeout(t);
  }, [toast]);

  // Load settings from Rust on mount
  useEffect(() => {
    invoke<Partial<Settings>>("get_settings").then((s) => {
      if (s) setSettings({ ...DEFAULT_SETTINGS, ...s });
    }).catch(() => {});
  }, []);

  // Show the orb window when the Display tab is active so the user can
  // see it move live while dragging the position sliders.
  useEffect(() => {
    if (tab === "display") {
      invoke("show_overlay").catch(() => {});
    }
  }, [tab]);

  // Fetch pending backdrop on mount — handles the fresh-window case where
  // the backdrop was captured before the React app loaded (same pattern as
  // the response sidebar's get_pending_sidebar_content).
  useEffect(() => {
    invoke<string | null>("get_pending_settings_backdrop")
      .then((backdrop) => {
        if (backdrop && backdrop.startsWith("data:image/")) {
          document.documentElement.style.setProperty(
            "--sidebar-backdrop-image",
            `url("${backdrop}")`,
          );
        }
      })
      .catch(() => {});
  }, []);

  // Listen for live sidebar:backdrop events — Rust captures the desktop
  // behind the window every 1s, blurs it, and sends it as a data URI.
  // We set it as a CSS variable on <html> so the ::after layer renders it.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>("sidebar:backdrop", (ev) => {
        const dataUri = ev.payload;
        if (typeof dataUri === "string" && dataUri.startsWith("data:image/")) {
          document.documentElement.style.setProperty("--sidebar-backdrop-image", `url("${dataUri}")`);
        }
      });
    })();
    return () => { unlisten?.(); };
  }, []);

  // Escape closes the settings window. Ctrl+Space must NOT close it:
  // Ctrl+Space is the global wake hotkey (D3) — it wakes NEXUS even with
  // settings focused. Window-level listener only, no OS registration.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        invoke("hide_settings_sidebar").catch(() => {});
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  const update = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
    setSaved(false);
  }, []);

  const handleSave = async () => {
    try {
      await invoke("save_settings", { settings });
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error("save_settings failed:", e);
    }
  };

  const handleReset = () => {
    setSettings(DEFAULT_SETTINGS);
    setSaved(false);
  };

  return (
    <div className="settings-container">
      {/* Merged header row: tabs left, dock buttons right, full-width drag region */}
      <header className="settings-header-row" data-tauri-drag-region>
        {/* Tabs on the left */}
        <div className="settings-tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              className={`settings-tab ${tab === t.id ? "settings-tab--active" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Dock controls on the right */}
        <div className="sidebar-dock-controls">
          <button type="button" className="sidebar-dock-btn" onClick={() => onDock("left")} title="Dock Left">
            ◧
          </button>
          <button type="button" className="sidebar-dock-btn" onClick={() => onDock("right")} title="Dock Right">
            ◨
          </button>
        </div>
      </header>

      {/* Calibration round-trip toast (save/cancel re-open the hub) */}
      {toast && <div className="settings-toast">{toast}</div>}

      {/* Scrollable content */}
      <div className="settings-scroll">
        {tab === "display" && <DisplayTab settings={settings} update={update} />}
        {tab === "audio" && <AudioTab settings={settings} update={update} />}
        {tab === "auth" && <AuthTab settings={settings} update={update} />}
        {tab === "connections" && <ConnectionsTab settings={settings} update={update} />}
      </div>

      {/* Footer */}
      <div className="settings-footer">
        <span className={`settings-saved-indicator ${saved ? "settings-saved-indicator--visible" : ""}`}>
          ✓ Saved
        </span>
        <div className="settings-footer-actions">
          <button className="settings-btn" onClick={handleReset}>Reset</button>
          <button className="settings-btn settings-btn--primary" onClick={handleSave}>
            Save
          </button>
        </div>
      </div>
    </div>
  );
}

const COLOR_PRESETS = [
  { name: "Golden Amber (Default)", hex: "#f2b859" },
  { name: "Electric Cyan", hex: "#00e5ff" },
  { name: "Neon Emerald", hex: "#10b981" },
  { name: "Crimson Ruby", hex: "#ef4444" },
  { name: "Royal Azure", hex: "#3b82f6" },
  { name: "Amethyst Violet", hex: "#a855f7" },
  { name: "Silver Pearl", hex: "#e2e8f0" },
];

// ─── Display Tab (Phase 2 — orb position + size) ──────────────────────
function DisplayTab({ settings, update }: {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const currentColor = settings.orbColor || "#f2b859";

  const onColorSelect = (hex: string) => {
    update("orbColor", hex);
    try {
      localStorage.setItem("nexus:orb_color", hex);
    } catch {}
    import("@tauri-apps/api/event").then(({ emit }) => {
      emit("orb:color", { color: hex }).catch(() => {});
    }).catch(() => {});
  };

  const currentPos = settings.orbPosition || "top";

  const onPositionSelect = (pos: "top" | "bottom") => {
    update("orbPosition", pos);
    try {
      localStorage.setItem("nexus:orb_position", pos);
    } catch {}
    useAssistant.getState().setOrbPosition(pos);
    import("@tauri-apps/api/core").then(({ invoke }) => {
      const vPct = pos === "bottom" ? 1.0 : 0.0;
      invoke("set_orb_position", {
        horizontalPct: 0.5,
        verticalPct: vPct,
        size: settings.orbSize || 200,
      }).catch(() => {});
    }).catch(() => {});
  };

  return (
    <>
      <div className="settings-section">
        <div className="settings-section-title">Orb Color Theme</div>
        <div className="setting-desc" style={{ marginBottom: 10 }}>
          Customize your visual aura for listening and idle states. The thinking state remains its iconic electric purple starburst.
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(130px, 1fr))", gap: 8, marginBottom: 12 }}>
          {COLOR_PRESETS.map((p) => {
            const isSelected = currentColor.toLowerCase() === p.hex.toLowerCase();
            return (
              <button
                key={p.hex}
                type="button"
                className={`settings-btn ${isSelected ? "settings-btn--primary" : "settings-btn--glass"}`}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "8px 10px",
                  borderRadius: 8,
                  border: isSelected ? "1.5px solid rgba(255,255,255,0.8)" : "1px solid rgba(255,255,255,0.12)",
                  background: isSelected ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.05)",
                  cursor: "pointer",
                  color: "#fff",
                  fontSize: 12,
                  fontWeight: isSelected ? 600 : 400,
                  transition: "all 0.15s ease",
                }}
                onClick={() => onColorSelect(p.hex)}
                title={p.name}
              >
                <span
                  style={{
                    width: 14,
                    height: 14,
                    borderRadius: "50%",
                    background: p.hex,
                    boxShadow: `0 0 8px ${p.hex}99`,
                    flexShrink: 0,
                    display: "inline-block",
                  }}
                />
                <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {p.name.split(" ")[0]}
                </span>
              </button>
            );
          })}
        </div>
        <div className="setting-row" style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <div>
            <div className="setting-label">Custom Hex Color</div>
            <div className="setting-desc">Enter or pick any bespoke color value</div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="color"
              value={currentColor}
              onChange={(e) => onColorSelect(e.target.value)}
              style={{
                width: 32,
                height: 32,
                borderRadius: 6,
                border: "1px solid rgba(255,255,255,0.2)",
                cursor: "pointer",
                background: "transparent",
              }}
            />
            <input
              type="text"
              className="settings-input"
              style={{ width: 85, textAlign: "center", fontFamily: "monospace" }}
              value={currentColor}
              onChange={(e) => onColorSelect(e.target.value)}
            />
          </div>
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Orb Screen Position</div>
        <div className="setting-desc" style={{ marginBottom: 12 }}>
          Choose where NEXUS appears. The sleek black slider smoothly glides into view from your chosen screen edge.
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
          <button
            type="button"
            className={`settings-btn ${currentPos === "top" ? "settings-btn--primary" : "settings-btn--glass"}`}
            style={{
              padding: "12px 14px",
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 6,
              borderRadius: 10,
              cursor: "pointer",
              border: currentPos === "top" ? "1.5px solid rgba(255,255,255,0.8)" : "1px solid rgba(255,255,255,0.12)",
              background: currentPos === "top" ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.05)",
              color: "#fff",
              transition: "all 0.15s ease",
            }}
            onClick={() => onPositionSelect("top")}
          >
            <span style={{ fontSize: 18 }}>⬒</span>
            <span style={{ fontWeight: 600, fontSize: 13 }}>Top (Default)</span>
            <span style={{ fontSize: 11, opacity: 0.7 }}>Slides down from top edge</span>
          </button>
          <button
            type="button"
            className={`settings-btn ${currentPos === "bottom" ? "settings-btn--primary" : "settings-btn--glass"}`}
            style={{
              padding: "12px 14px",
              display: "flex",
              flexDirection: "column",
              alignItems: "center",
              gap: 6,
              borderRadius: 10,
              cursor: "pointer",
              border: currentPos === "bottom" ? "1.5px solid rgba(255,255,255,0.8)" : "1px solid rgba(255,255,255,0.12)",
              background: currentPos === "bottom" ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.05)",
              color: "#fff",
              transition: "all 0.15s ease",
            }}
            onClick={() => onPositionSelect("bottom")}
          >
            <span style={{ fontSize: 18 }}>⬓</span>
            <span style={{ fontWeight: 600, fontSize: 13 }}>Bottom</span>
            <span style={{ fontSize: 11, opacity: 0.7 }}>Slides up from taskbar</span>
          </button>
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Sidebar Docking</div>
        <div className="setting-desc" style={{ marginBottom: 8 }}>
          Choose where the NEXUS panel is pinned: right edge, left edge, or
          floating as a centered modal. Works from any tab, any view.
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            className="settings-btn settings-btn--glass"
            style={{ flex: 1 }}
            onClick={() => {
              invoke("set_sidebar_dock", { dock: "left" }).catch(() => {});
            }}
          >
            ◧ Dock Left
          </button>
          <button
            className="settings-btn settings-btn--glass"
            style={{ flex: 1 }}
            onClick={() => {
              invoke("set_sidebar_dock", { dock: "right" }).catch(() => {});
            }}
          >
            ◨ Dock Right
          </button>
        </div>
        <button
          className="settings-btn settings-btn--glass"
          style={{ width: "100%", marginTop: 6 }}
          onClick={() => {
            invoke("set_sidebar_dock", { dock: "center" }).catch(() => {});
          }}
        >
          ⧉ Center Floating
        </button>
      </div>
    </>
  );
}

// ─── Audio Tab — iconic voice personas (Feature 83) ─────────────────
// Persona catalog comes from Rust (list_voice_personas — single source
// of truth). Cards: preview demo (non-destructive) + click to equip
// (instant cloud switch + background offline-twin sync).
interface VoicePersona {
  key: string;
  name: string;
  persona: string;
  accentTag: string;
  avatar: string;
  cloudId: string;
  kokoroVoice: string;
  previewPhrase: string;
}

interface VoiceStatus {
  voice_key: string;
  cloud_id: string;
  twin: string;
  sync_state: "ready" | "cloud_only" | "downloading";
}

function AudioTab({ settings, update }: {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const volume = settings.ttsVolume ?? 75;
  const [personas, setPersonas] = useState<VoicePersona[]>([]);
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [equipping, setEquipping] = useState<string | null>(null);
  const [sync, setSync] = useState<Record<string, { state: "ready" | "cloud_only" | "downloading"; progress?: number; phase?: string }>>({});
  const [transport, setTransport] = useState<"cloud" | "local">("cloud");

  // Fetch persona catalog + current sync state + transport on mount.
  useEffect(() => {
    invoke<VoicePersona[]>("list_voice_personas")
      .then(setPersonas)
      .catch(() => {});
    invoke<VoiceStatus>("get_voice_status")
      .then((s) => {
        if (s?.voice_key) {
          setSync((m) => ({ ...m, [s.voice_key]: { state: s.sync_state } }));
        }
      })
      .catch(() => {});
    invoke<{ transport?: string }>("get_voice_transport")
      .then((t) => {
        if (t?.transport === "local" || t?.transport === "cloud") {
          setTransport(t.transport);
        }
      })
      .catch(() => {});
  }, []);

  // Live sync pill + transport updates from the swap worker /
  // connectivity watchdog. Any voice:status event also re-reads the
  // transport flag (cheap sync read) so the dot never lies.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const { invoke: inv } = await import("@tauri-apps/api/core");
      unlisten = await listen<{ status?: string; voice_key?: string; progress?: number; phase?: string }>(
        "voice:status",
        (ev) => {
          const key = ev.payload?.voice_key;
          const status = ev.payload?.status;
          if (status === "cloud-restored") {
            setTransport("cloud");
            return;
          }
          if (!key || (status !== "ready" && status !== "cloud_only" && status !== "downloading" && status !== "error")) return;
          const state = status === "error" ? "cloud_only" : status;
          setSync((m) => ({ ...m, [key]: { state, progress: ev.payload?.progress, phase: ev.payload?.phase } }));
          inv<{ transport?: string }>("get_voice_transport")
            .then((t) => {
              if (t?.transport === "local" || t?.transport === "cloud") {
                setTransport(t.transport);
              }
            })
            .catch(() => {});
        },
      );
    })().catch(() => {});
    return () => { unlisten?.(); };
  }, []);

  const previewVoice = (p: VoicePersona) => {
    if (previewing) return;
    setPreviewing(p.key);
    invoke("preview_voice", { voiceId: p.cloudId, text: p.previewPhrase })
      .catch(() => {})
      .finally(() => setTimeout(() => setPreviewing(null), 500));
  };

  const equipVoice = (p: VoicePersona) => {
    if (equipping) return;
    setEquipping(p.key);
    invoke<{ voiceKey: string; cloudId: string; syncState: string }>("set_voice_preference", {
      voiceKey: p.key,
    })
      .then((res) => {
        // Mirror into local settings state (persists on Save).
        update("edgeTtsVoice", res.cloudId);
        update("selectedVoice", res.voiceKey);
        const st = res.syncState === "ready" ? "ready" : "cloud_only";
        setSync((m) => ({ ...m, [res.voiceKey]: { state: st } }));
      })
      .catch(() => {})
      .finally(() => setEquipping(null));
  };

  const activeKey =
    personas.find((p) => p.cloudId === (settings.edgeTtsVoice || "en-US-AvaNeural"))?.key
    ?? "nexus";

  const syncPill = (key: string) => {
    const s = sync[key];
    if (!s) return null;
    if (s.state === "ready") return <span className="persona-sync persona-sync--ready">✓ Cloud + Offline Ready</span>;
    if (s.state === "downloading") {
      const pct = typeof s.progress === "number" ? ` ${s.progress}%` : "…";
      // phase "base" = the one-time shared offline engine (~88 MB); "voice" = the 0.5 MB voice file
      const label = s.phase === "base" ? "Downloading offline engine (one-time)" : "Syncing Offline Voice";
      return <span className="persona-sync persona-sync--busy">⬇ {label}{pct}</span>;
    }
    return <span className="persona-sync persona-sync--pending">⚡ Cloud Only</span>;
  };

  return (
    <>
      <div className="settings-section">
        <div className="settings-section-title">
          Voice Selection
          <span
            className={`transport-dot transport-dot--${transport}`}
            title={transport === "cloud" ? "Speaking via cloud" : "Speaking via local offline voice"}
          >
            ● {transport === "cloud" ? "Cloud" : "Local"}
          </span>
        </div>
        <div className="setting-desc" style={{ marginBottom: 8 }}>
          Tap ▶ for a demo. Tap a card to equip it — cloud switches instantly, the offline twin syncs in the background.
        </div>
        <div className="persona-grid">
          {personas.length === 0 && (
            <div className="voice-empty">Loading voices…</div>
          )}
          {personas.map((p) => (
            <div
              key={p.key}
              className={`persona-card ${activeKey === p.key ? "persona-card--active" : ""}`}
              onClick={() => equipVoice(p)}
              title={p.persona}
            >
              <div className="persona-card-top">
                <span className="persona-avatar" aria-hidden="true">{p.avatar}</span>
                {activeKey === p.key && <span className="persona-check">✓</span>}
              </div>
              <div className="persona-name">{p.name}</div>
              <div className="persona-accent">{p.accentTag}</div>
              <div className="persona-desc">{p.persona}</div>
              {activeKey === p.key && syncPill(p.key)}
              <button
                className="persona-play-btn"
                disabled={previewing !== null || equipping !== null}
                onClick={(e) => {
                  e.stopPropagation();
                  previewVoice(p);
                }}
              >
                {previewing === p.key ? "⏸ Pause" : "▶ Play Demo"}
              </button>
            </div>
          ))}
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Speech Volume</div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Default NEXUS volume</div>
            <div className="setting-desc">Sets system volume before speaking, restores after. 0 = disabled.</div>
          </div>
          <div className="setting-control">
            <input
              type="range"
              className="settings-slider"
              min={0}
              max={100}
              value={volume}
              onChange={(e) => update("ttsVolume", parseInt(e.target.value))}
            />
            <span className="slider-value">{volume}%</span>
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Voice emotion</div>
            <div className="setting-desc">Auto picks per reply (errors sad, success cheerful)</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.ttsEmotion ?? "auto"}
              onChange={(e) => update("ttsEmotion", e.target.value)}
            >
              <option value="auto">Auto</option>
              <option value="neutral">Neutral</option>
              <option value="cheerful">Cheerful</option>
              <option value="calm">Calm</option>
              <option value="sad">Sad</option>
              <option value="urgent">Urgent</option>
              <option value="whisper">Whisper</option>
            </select>
          </div>
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">STT Provider</div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Local STT only</div>
            <div className="setting-desc">Audio never leaves your device</div>
          </div>
          <div className="setting-control">
            <div
              className={`settings-toggle ${settings.localSttOnly ? "settings-toggle--on" : ""}`}
              onClick={() => update("localSttOnly", !settings.localSttOnly)}
            />
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Speaker verification</div>
            <div className="setting-desc">Only enrolled voice can wake NEXUS</div>
          </div>
          <div className="setting-control">
            <div
              className={`settings-toggle ${settings.speakerVerification ? "settings-toggle--on" : ""}`}
              onClick={() => update("speakerVerification", !settings.speakerVerification)}
            />
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Moonshine model</div>
            <div className="setting-desc">Local STT accuracy vs RAM tradeoff</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.moonshineModel ?? "medium_streaming"}
              onChange={(e) => update("moonshineModel", e.target.value)}
            >
              <option value="medium_streaming">Medium v2 (245M, 6.65% WER, ~400MB)</option>
              <option value="small_streaming">Small v2 (123M, 7.84% WER, ~300MB)</option>
              <option value="tiny_streaming">Tiny (34M, 12% WER, ~100MB)</option>
            </select>
          </div>
        </div>
      </div>
    </>
  );
}

// ─── Auth Tab (Multi-Email Google + Direct OAuth + API keys) ──────────
interface GoogleAccountProfile {
  email: string;
  name: string;
  picture?: string;
  is_primary: boolean;
  added_at_ms: number;
  scopes: string[];
}

function AuthTab({ settings, update }: {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const [oauthStatus, setOauthStatus] = useState<Record<string, OAuthStatus>>({});
  const [googleAccounts, setGoogleAccounts] = useState<GoogleAccountProfile[]>([]);
  const [connecting, setConnecting] = useState<string | null>(null);
  const [connectingGoogle, setConnectingGoogle] = useState(false);
  const [showCustomCreds, setShowCustomCreds] = useState(false);
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [error, setError] = useState<string | null>(null);

  const loadGoogleAccounts = useCallback(async () => {
    try {
      const accs = await invoke<GoogleAccountProfile[]>("google_get_accounts");
      setGoogleAccounts(accs);
    } catch (e) {
      console.warn("Failed to load google accounts", e);
    }
  }, []);

  useEffect(() => {
    loadGoogleAccounts();
    if (settings.serverUrl) {
      setSidecarBaseUrl(settings.serverUrl);
      if (settings.userId) {
        getOAuthStatus(settings.userId).then(setOauthStatus).catch(() => {});
      }
    }
  }, [loadGoogleAccounts, settings.serverUrl, settings.userId]);

  const handleConnectGoogleNative = async () => {
    setError(null);
    setConnectingGoogle(true);
    try {
      await invoke("google_connect_account");
      await loadGoogleAccounts();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnectingGoogle(false);
    }
  };

  const handleSetPrimaryGoogle = async (email: string) => {
    try {
      await invoke("google_set_primary_account", { email });
      await loadGoogleAccounts();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDisconnectGoogle = async (email: string) => {
    try {
      await invoke("google_disconnect_account", { email });
      await loadGoogleAccounts();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleSaveCustomCreds = async () => {
    try {
      await invoke("google_save_custom_credentials", {
        clientId,
        clientSecret: clientSecret || null,
      });
      setShowCustomCreds(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleConnectGithub = async () => {
    setError(null);
    setConnecting("github");
    try {
      if (!settings.serverUrl) throw new Error("Server URL not configured");
      setSidecarBaseUrl(settings.serverUrl);
      const success = await connectOAuth("github", settings.userId || "local-user");
      if (success) {
        const status = await getOAuthStatus(settings.userId || "local-user");
        setOauthStatus(status);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setConnecting(null);
    }
  };

  const handleDisconnectGithub = async () => {
    setError(null);
    try {
      await disconnectOAuth(settings.userId || "local-user", "github");
      const status = await getOAuthStatus(settings.userId || "local-user");
      setOauthStatus(status);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const githubConnected = oauthStatus.github?.connected ?? false;

  return (
    <>
      {error && (
        <div className="auth-card" style={{ borderColor: "rgba(255,80,80,0.3)" }}>
          <span style={{ fontSize: 12, color: "rgba(255,150,150,0.9)" }}>{error}</span>
        </div>
      )}

      <div className="settings-section">
        <div className="settings-section-title" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span>Google Accounts</span>
          <button
            className="settings-btn settings-btn--primary"
            style={{ fontSize: 11, padding: "4px 10px" }}
            disabled={connectingGoogle}
            onClick={handleConnectGoogleNative}
          >
            {connectingGoogle ? "Signing in..." : "+ Add Google Account"}
          </button>
        </div>

        {googleAccounts.length === 0 ? (
          <div className="auth-card" style={{ textAlign: "center", padding: "16px" }}>
            <span style={{ fontSize: 12, color: "rgba(255,255,255,0.6)" }}>
              No Google accounts connected yet. Connect an account to use Gmail engine, Calendar, Sentinel watch, & Photos.
            </span>
          </div>
        ) : (
          googleAccounts.map((acc) => (
            <div key={acc.email} className="auth-card" style={{ marginBottom: 10 }}>
              <div className="auth-card-header" style={{ alignItems: "center" }}>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  {acc.picture ? (
                    <img src={acc.picture} alt="" style={{ width: 32, height: 32, borderRadius: "50%" }} />
                  ) : (
                    <div style={{ width: 32, height: 32, borderRadius: "50%", background: "rgba(255,255,255,0.15)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 14 }}>
                      {acc.name.charAt(0).toUpperCase()}
                    </div>
                  )}
                  <div>
                    <div style={{ fontWeight: 600, fontSize: 13 }}>{acc.name}</div>
                    <div style={{ fontSize: 11, color: "rgba(255,255,255,0.5)" }}>{acc.email}</div>
                  </div>
                </div>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  {acc.is_primary ? (
                    <span className="status-badge status-badge--connected" style={{ fontSize: 10 }}>
                      Primary
                    </span>
                  ) : (
                    <button
                      className="settings-btn"
                      style={{ fontSize: 10, padding: "2px 6px" }}
                      onClick={() => handleSetPrimaryGoogle(acc.email)}
                    >
                      Make Primary
                    </button>
                  )}
                  <button
                    className="settings-btn settings-btn--danger"
                    style={{ fontSize: 10, padding: "2px 6px" }}
                    onClick={() => handleDisconnectGoogle(acc.email)}
                  >
                    Remove
                  </button>
                </div>
              </div>
            </div>
          ))
        )}

        <div style={{ marginTop: 8 }}>
          <button
            className="settings-btn"
            style={{ fontSize: 10, opacity: 0.7 }}
            onClick={() => setShowCustomCreds(!showCustomCreds)}
          >
            {showCustomCreds ? "Hide Developer Credentials" : "Custom Developer OAuth Credentials"}
          </button>
          {showCustomCreds && (
            <div className="auth-card" style={{ marginTop: 8, padding: 12 }}>
              <div style={{ fontSize: 11, color: "rgba(255,255,255,0.7)", marginBottom: 8 }}>
                Optionally supply your own Google OAuth Client ID & Secret:
              </div>
              <input
                type="text"
                className="settings-input"
                placeholder="Client ID (...apps.googleusercontent.com)"
                value={clientId}
                onChange={(e) => setClientId(e.target.value)}
                style={{ marginBottom: 8, fontSize: 11 }}
              />
              <input
                type="password"
                className="settings-input"
                placeholder="Client Secret (optional for PKCE)"
                value={clientSecret}
                onChange={(e) => setClientSecret(e.target.value)}
                style={{ marginBottom: 8, fontSize: 11 }}
              />
              <button
                className="settings-btn settings-btn--primary"
                style={{ fontSize: 11, width: "100%" }}
                onClick={handleSaveCustomCreds}
              >
                Save Credentials
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">GitHub Account</div>
        <div className="auth-card">
          <div className="auth-card-header">
            <span className="auth-card-title">GitHub</span>
            <span className={`status-badge ${githubConnected ? "status-badge--connected" : "status-badge--disconnected"}`}>
              {githubConnected ? "Connected" : "Not connected"}
            </span>
          </div>
          <div className="auth-card-actions">
            {githubConnected ? (
              <button className="settings-btn settings-btn--danger" onClick={() => handleDisconnectGithub()}>
                Disconnect
              </button>
            ) : (
              <button
                className="settings-btn settings-btn--primary"
                disabled={connecting === "github"}
                onClick={() => handleConnectGithub()}
              >
                {connecting === "github" ? "Waiting..." : "Connect GitHub"}
              </button>
            )}
          </div>
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">API Keys</div>

        <div className="auth-card">
          <div className="auth-card-header">
            <span className="auth-card-title">Gemini API Key</span>
            <GeminiKeyPill />
          </div>
          <input
            type="password"
            className="settings-input"
            placeholder="AIza..."
            value={settings.geminiApiKey ?? ""}
            onChange={(e) => update("geminiApiKey", e.target.value)}
          />
          {(settings.geminiApiKey ?? "").trim() !== "" &&
            !isPlausibleGeminiKey(settings.geminiApiKey ?? "") && (
              <div className="setting-desc" style={{ marginTop: 4, color: "#ffd479" }}>
                Doesn&apos;t look like a Gemini key — should start with AIza… (39 chars). Hit Test to verify.
              </div>
            )}
          <div className="setting-desc" style={{ marginTop: 4 }}>
            Powers spatial screen analysis + ghost vision grounding. Free tier covers ~500 vision calls/day.{" "}
            <a
              href="#"
              onClick={(e) => {
                e.preventDefault();
                import("@tauri-apps/plugin-shell").then(({ open }) => {
                  open("https://aistudio.google.com/apikey").catch(() => {});
                });
              }}
            >
              Get a free key
            </a>
          </div>
          <GeminiKeyTest />
        </div>

        <div className="auth-card">
          <div className="auth-card-header">
            <span className="auth-card-title">Groq API Key</span>
          </div>
          <input
            type="password"
            className="settings-input"
            placeholder="gsk_..."
            value={settings.groqApiKey}
            onChange={(e) => update("groqApiKey", e.target.value)}
          />
        </div>

        <div className="auth-card">
          <div className="auth-card-header">
            <span className="auth-card-title">Cerebras API Key</span>
          </div>
          <input
            type="password"
            className="settings-input"
            placeholder="csk-..."
            value={settings.cerebrasApiKey ?? ""}
            onChange={(e) => update("cerebrasApiKey", e.target.value)}
          />
        </div>
        <div className="setting-desc" style={{ marginTop: 4 }}>
          Keys stay on this device (OS keychain) — never uploaded, never in Cloudflare.
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Ghost Vision</div>
        <div className="setting-desc" style={{ marginBottom: 8 }}>
          Spatial screen analysis (“analyse the screen”) needs a Gemini key — add it under API Keys above.
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Vision provider</div>
            <div className="setting-desc">Who sees custom buttons UIA can't. Gemini is the sole vision engine (Groq retired vision).</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.visionProvider ?? "auto"}
              onChange={(e) => update("visionProvider", e.target.value)}
            >
              <option value="auto">Auto (Gemini)</option>
              <option value="gemini">Gemini only</option>
              <option value="groq">Groq only (unavailable)</option>
            </select>
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Vision speed</div>
            <div className="setting-desc">Single provider active — sequential only. Racing returns if a second vision engine appears.</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.visionRace ?? "sequential"}
              onChange={(e) => update("visionRace", e.target.value)}
            >
              <option value="sequential">Sequential (save quota)</option>
              <option value="speed" disabled>Race (needs 2 engines)</option>
            </select>
          </div>
        </div>
        <VisionQuota />
      </div>
    </>
  );
}

function GeminiKeyPill() {
  const [present, setPresent] = useState<boolean | null>(null);

  useEffect(() => {
    invoke<{ gemini_present?: boolean }>("vision_key_status")
      .then((s) => setPresent(!!s?.gemini_present))
      .catch(() => setPresent(null));
  }, []);

  if (present === null) return null;
  return present ? (
    <span className="status-badge status-badge--connected">✓ Saved</span>
  ) : (
    <span className="status-badge status-badge--disconnected">○ Missing</span>
  );
}

function GeminiKeyTest() {
  const [state, setState] = useState<"idle" | "testing" | "ok" | "fail">("idle");
  const [detail, setDetail] = useState<string>("");

  const runTest = () => {
    setState("testing");
    setDetail("");
    invoke<{ ok?: boolean; detail?: string }>("vision_test_key")
      .then((r) => {
        if (r?.ok) {
          setState("ok");
          setDetail("");
        } else {
          setState("fail");
          setDetail(
            r?.detail === "no-key"
              ? "Save a key first, then test."
              : r?.detail === "quota-exhausted"
                ? "Key valid but daily quota is spent."
                : r?.detail === "network-error"
                  ? "Network unreachable — check connection."
                  : "Key rejected — check for typos."
          );
        }
      })
      .catch(() => {
        setState("fail");
        setDetail("Test call failed.");
      });
  };

  return (
    <div style={{ marginTop: 6, display: "flex", alignItems: "center", gap: 8 }}>
      <button
        className="settings-btn"
        disabled={state === "testing"}
        onClick={runTest}
      >
        {state === "testing" ? "Testing…" : "Test Key"}
      </button>
      {state === "ok" && (
        <span className="status-badge status-badge--connected">✓ Valid</span>
      )}
      {state === "fail" && (
        <span className="status-badge status-badge--disconnected">
          ✗ {detail || "Invalid"}
        </span>
      )}
    </div>
  );
}

function VisionQuota() {
  const [quota, setQuota] = useState<{
    date: string;
    groq: { used: number; limit: number };
    gemini: { used: number; limit: number };
  } | null>(null);

  useEffect(() => {
    invoke<{
      date: string;
      groq: { used: number; limit: number };
      gemini: { used: number; limit: number };
    }>("vision_quota_status").then(setQuota).catch(() => {});
  }, []);

  if (!quota) {
    return (
      <div className="setting-row">
        <div>
          <div className="setting-label">Daily usage</div>
          <div className="setting-desc">Loading…</div>
        </div>
      </div>
    );
  }
  const pct = (u: number, l: number) => (l > 0 ? Math.min(100, Math.round((u / l) * 100)) : 0);
  return (
    <div className="setting-row">
      <div>
        <div className="setting-label">Daily usage (resets midnight PT)</div>
        <div className="setting-desc">
          Groq {quota.groq.used} / {quota.groq.limit} ({pct(quota.groq.used, quota.groq.limit)}%)
          {" • "}
          Gemini {quota.gemini.used} / {quota.gemini.limit} ({pct(quota.gemini.used, quota.gemini.limit)}%)
        </div>
      </div>
    </div>
  );
}

// ─── Connections Tab (MCP vault — one login per service group) ──────
// Each card shows vault status (live/expired/missing) with Reconnect +
// Delete. Google reconnects via OAuth (Accounts tab flow writes the token
// into the vault on next MCP call); token services paste a key directly.
// Session bridges (WhatsApp/LinkedIn) show bridge health + instructions.
const VAULT_META: Record<string, { title: string; hint: string; kind: "oauth" | "token" | "session"; tokenUrl?: string }> = {
  google: { title: "Google (Gmail, Calendar, Contacts, Drive, Sheets, Meet)", hint: "One OAuth login covers all six. Reconnect in Accounts tab, then Delete here only clears the local copy.", kind: "oauth" },
  telegram: { title: "Telegram remote (owner-only phone control)", hint: "Create a bot via @BotFather, paste its token below, then put your chat id (from @userinfobot) in the Owner chat id field and press Save + restart.", kind: "token" },
  swiggy: { title: "Swiggy (Food, Instamart, Dineout)", hint: "One OAuth login covers all three. Login opens your browser; NEXUS stores and refreshes the token. A pasted token works too.", kind: "token" },
  spotify: { title: "Spotify", hint: "1. Open the Spotify dashboard (button below). 2. Create a token with user-read scope. 3. Paste it here.", kind: "token", tokenUrl: "https://developer.spotify.com/dashboard" },
  vercel: { title: "Vercel", hint: "1. Open Vercel account settings (button below). 2. Create a token (read-only recommended). 3. Paste it here.", kind: "token", tokenUrl: "https://vercel.com/account/tokens" },
  render: { title: "Render", hint: "1. Open Render API keys (button below). 2. Create a key. 3. Paste it here. OAuth preferred — keys are broadly scoped.", kind: "token", tokenUrl: "https://dashboard.render.com/u/keys" },
};

function statusCell(ok: boolean): React.CSSProperties {
  return {
    display: "flex",
    justifyContent: "space-between",
    padding: "6px 10px",
    borderRadius: 4,
    background: ok ? "rgba(80,200,120,0.08)" : "rgba(255,120,120,0.08)",
    border: `1px solid ${ok ? "rgba(80,200,120,0.25)" : "rgba(255,120,120,0.25)"}`,
  };
}

function formatUptime(sec: number): string {
  if (sec < 60) return `${sec}s`;
  if (sec < 3600) return `${Math.floor(sec / 60)}m`;
  if (sec < 86400) return `${Math.floor(sec / 3600)}h ${Math.floor((sec % 3600) / 60)}m`;
  return `${Math.floor(sec / 86400)}d ${Math.floor((sec % 86400) / 3600)}h`;
}

function ConnectionsTab({ settings, update }: {
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const [vault, setVault] = useState<Record<string, string>>({});
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [swiggyConnecting, setSwiggyConnecting] = useState(false);
  const [health, setHealth] = useState<{
    memoryMb: number;
    uptimeSec: number;
    sttPort: boolean;
    nluPort: boolean;
    workerReachable: boolean;
    groqKey: boolean;
    geminiKey: boolean;
  } | null>(null);
  // Feature 88: canonical laptop identity state.
  const [identityState, setIdentityState] = useState<{
    state: string;
    profileId?: string;
    deviceName?: string;
  } | null>(null);

  useEffect(() => {
    invoke<{ state: string; profileId?: string; deviceName?: string }>("get_identity_status")
      .then((st) => setIdentityState(st))
      .catch(() => setIdentityState({ state: "unknown_profile" }));
  }, []);

  const refresh = useCallback(async () => {
    try {
      const entries = await invoke<{ service: string; status: string }[]>("vault_status");
      const map: Record<string, string> = {};
      for (const e of entries) map[e.service] = e.status;
      setVault(map);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const refreshHealth = useCallback(async () => {
    try {
      const h = await invoke<{
        memoryMb: number;
        uptimeSec: number;
        sttPort: boolean;
        nluPort: boolean;
        workerReachable: boolean;
        groqKey: boolean;
        geminiKey: boolean;
      }>("get_health_status");
      setHealth(h);
    } catch {
      // Health check failed — leave stale data
    }
  }, []);

  useEffect(() => { refresh().catch(() => {}); }, [refresh]);
  useEffect(() => { refreshHealth().catch(() => {}); }, [refreshHealth]);

  // Refresh health every 5s while tab is open
  useEffect(() => {
    const id = setInterval(() => { refreshHealth().catch(() => {}); }, 5000);
    return () => clearInterval(id);
  }, [refreshHealth]);

  // Live refresh when the idle vault monitor reports a credential change
  // (expiry/reconnect while the tab is open).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>("vault:changed", () => {
        refresh().catch(() => {});
      });
    })();
    return () => { unlisten?.(); };
  }, [refresh]);

  const handleSave = async (service: string) => {
    setError(null);
    try {
      await invoke("vault_set_token", { service, token: drafts[service] ?? "", expiresInSecs: 0 });
      setDrafts((d) => ({ ...d, [service]: "" }));
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async (service: string) => {
    setError(null);
    try {
      await invoke("vault_clear_token", { service });
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  // Swiggy OAuth: browser login → Worker stores + refreshes server-side;
  // the vault pulls a fresh token on next use (or the user pastes one).
  const handleSwiggyLogin = async () => {
    setError(null);
    setSwiggyConnecting(true);
    try {
      if (!settings.serverUrl) throw new Error("Server URL not configured");
      setSidecarBaseUrl(settings.serverUrl);
      await connectOAuth("swiggy", settings.userId || "local-user");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSwiggyConnecting(false);
    }
  };

  const badge = (status?: string) => (
    <span className={`status-badge ${status === "live" ? "status-badge--connected" : "status-badge--disconnected"}`}>
      {status === "live" ? "Live" : status === "expired" ? "Expired" : "Missing"}
    </span>
  );

  return (
    <>
      {error && (
        <div className="auth-card" style={{ borderColor: "rgba(255,80,80,0.3)" }}>
          <span style={{ fontSize: 12, color: "rgba(255,150,150,0.9)" }}>{error}</span>
        </div>
      )}

      <div className="settings-section">
        <div className="settings-section-title">Identity</div>
        {(() => {
          if (identityState == null) {
            return <div style={{ fontSize: 12, opacity: 0.5 }}>Loading…</div>;
          }
          const banner = identityBanner(identityState);
          const colors: Record<IdentityBanner["tone"], string> = {
            amber: "#fbbf24", green: "#22c55e", red: "#ef4444", dim: "#94a3b8",
          };
          return (
            <div className="auth-card">
              <div className="auth-card-header">
                <span className="auth-card-title">{banner.title}</span>
                <span style={{ fontSize: 11, color: colors[banner.tone] }}>{identityState.state}</span>
              </div>
              <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>{banner.subtitle}</div>
              {identityState.profileId ? (
                <div style={{ fontSize: 11, opacity: 0.45, marginBottom: 8 }}>
                  device: {identityState.deviceName || "unknown"} · profile …{identityState.profileId.slice(-6)}
                </div>
              ) : null}
              {identityState.state === "approved" && (
                <button
                  className="settings-btn"
                  onClick={async () => {
                    if (!window.confirm("Disconnect this device from the NEXUS cloud? Cloud features will be disabled until re-approval.")) return;
                    try {
                      await invoke("disconnect_device");
                      setIdentityState({ state: "provisional" });
                    } catch (err) {
                      console.warn("[NEXUS] disconnect failed:", err);
                    }
                  }}
                >
                  Disconnect this device
                </button>
              )}
            </div>
          );
        })()}
      </div>

      <div className="settings-section">
        <div className="settings-section-title">System Status</div>
        {health ? (
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "8px", fontSize: 12 }}>
            <div style={statusCell(health.memoryMb > 0)}>
              <span style={{ opacity: 0.6 }}>Memory</span>
              <span>{health.memoryMb} MB</span>
            </div>
            <div style={statusCell(true)}>
              <span style={{ opacity: 0.6 }}>Uptime</span>
              <span>{formatUptime(health.uptimeSec)}</span>
            </div>
            <div style={statusCell(health.sttPort)}>
              <span style={{ opacity: 0.6 }}>STT</span>
              <span>{health.sttPort ? "Running" : "Idle"}</span>
            </div>
            <div style={statusCell(health.nluPort)}>
              <span style={{ opacity: 0.6 }}>NLU</span>
              <span>{health.nluPort ? "Running" : "Idle"}</span>
            </div>
            <div style={statusCell(health.workerReachable)}>
              <span style={{ opacity: 0.6 }}>Worker</span>
              <span>{health.workerReachable ? "Online" : "Offline"}</span>
            </div>
            <div style={statusCell(true)}>
              <span style={{ opacity: 0.6 }}>Wake</span>
              <span>Active</span>
            </div>
            <div style={statusCell(health.groqKey || health.geminiKey)}>
              <span style={{ opacity: 0.6 }}>Keys</span>
              <span>Groq {health.groqKey ? "✓" : "✗"} · Gemini {health.geminiKey ? "✓" : "✗"}</span>
            </div>
          </div>
        ) : (
          <div style={{ fontSize: 12, opacity: 0.5 }}>Loading…</div>
        )}
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Vision keys</div>
        <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>
          {health == null
            ? "Checking…"
            : health.groqKey || health.geminiKey
              ? "A vision key is present — ghost clicks can see custom buttons."
              : "No Groq or Gemini key — add one in the Accounts tab for cloud STT + ghost vision."}
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">MCP Connections (vault)</div>

        {Object.entries(VAULT_META).map(([service, meta]) => (
          <div className="auth-card" key={service}>
            <div className="auth-card-header">
              <span className="auth-card-title">{meta.title}</span>
              {badge(vault[service])}
            </div>
            <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>{meta.hint}</div>
            {meta.kind === "token" && service !== "telegram" && (
              <input
                type="password"
                className="settings-input"
                placeholder="paste token…"
                value={drafts[service] ?? ""}
                onChange={(e) => setDrafts((d) => ({ ...d, [service]: e.target.value }))}
              />
            )}
            {service === "telegram" && (
              <>
                <input
                  type="password"
                  className="settings-input"
                  placeholder="bot token from @BotFather…"
                  value={drafts[service] ?? ""}
                  onChange={(e) => setDrafts((d) => ({ ...d, [service]: e.target.value }))}
                />
                <input
                  type="text"
                  className="settings-input"
                  placeholder="owner chat id (from @userinfobot)…"
                  value={settings.telegramChatId ?? ""}
                  onChange={(e) => update("telegramChatId", e.target.value)}
                />
                <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>
                  Save (footer) persists the chat id — restart NEXUS to start the bridge. Only this chat is ever answered.
                </div>
              </>
            )}
            {service === "swiggy" && (
              <button
                className="settings-btn settings-btn--primary"
                disabled={swiggyConnecting || !settings.serverUrl}
                onClick={handleSwiggyLogin}
              >
                {swiggyConnecting ? "Waiting for login…" : "Login with Swiggy"}
              </button>
            )}
            {meta.kind === "token" && meta.tokenUrl && (
              <button
                className="settings-btn"
                onClick={() => { import("@tauri-apps/plugin-shell").then(({ open }) => open(meta.tokenUrl!)).catch(() => window.open(meta.tokenUrl!, "_blank")); }}
              >
                Get token
              </button>
            )}
            <div className="auth-card-actions">
              {meta.kind === "token" && (
                <button
                  className="settings-btn settings-btn--primary"
                  disabled={!(drafts[service] ?? "").trim()}
                  onClick={() => handleSave(service)}
                >
                  Save token
                </button>
              )}
              <button className="settings-btn settings-btn--danger" onClick={() => handleDelete(service)}>
                Delete
              </button>
            </div>
          </div>
        ))}
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Session bridges (local)</div>

        <BridgeCards />
      </div>
    </>
  );
}

function BridgeCards() {
  const [bridges, setBridges] = useState<Record<string, { reachable: boolean; note: string }>>({});
  const [checking, setChecking] = useState(false);

  const refresh = useCallback(async () => {
    setChecking(true);
    try {
      const list = await invoke<{ server: string; url: string; reachable: boolean; latency_ms: number; note: string }[]>("mcp_status");
      const map: Record<string, { reachable: boolean; note: string }> = {};
      for (const b of list) map[b.server] = { reachable: b.reachable, note: b.note };
      setBridges(map);
    } catch {
      // probe failed entirely — leave cards in unknown state
    } finally {
      setChecking(false);
    }
  }, []);

  useEffect(() => { refresh().catch(() => {}); }, [refresh]);

  const card = (key: string, title: string, hint: string) => {
    const st = bridges[key];
    return (
      <div className="auth-card" key={key}>
        <div className="auth-card-header">
          <span className="auth-card-title">{title}</span>
          <span className={`status-badge ${st?.reachable ? "status-badge--connected" : "status-badge--disconnected"}`}>
            {st ? (st.reachable ? "Reachable" : "Down") : "…"}
          </span>
        </div>
        <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>{hint}</div>
        {st && (
          <div style={{ fontSize: 12, opacity: 0.65, margin: "0 0 8px" }}>{st.note}</div>
        )}
      </div>
    );
  };

  return (
    <>
      {card("whatsapp", "WhatsApp bridge (:8765)", "Run the bridge binary, then scan the QR once — the session persists. Reads never mark messages read.")}
      {card("amazon", "Amazon bridge (:8766)", "Run the product-search bridge locally. Read-only: search, details, reviews.")}
      <div className="auth-card">
        <div className="auth-card-header">
          <span className="auth-card-title">LinkedIn session</span>
        </div>
        <div style={{ fontSize: 12, opacity: 0.65, margin: "4px 0 8px" }}>
          Uses your logged-in browser session cookie. Re-login in the browser if writes start failing.
        </div>
      </div>
      <div className="auth-card-actions">
        <button className="settings-btn" disabled={checking} onClick={refresh}>
          {checking ? "Checking…" : "Recheck bridges"}
        </button>
      </div>
    </>
  );
}
