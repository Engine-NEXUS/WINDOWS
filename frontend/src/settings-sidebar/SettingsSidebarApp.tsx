import { useState, useEffect, useCallback, useRef, Component, type ErrorInfo } from "react";
import { HandModeSection } from "./HandModeSection";
import { AgentWatchSection } from "./AgentWatchSection";
import { invoke } from "@tauri-apps/api/core";
import { useVoiceLockState, VoiceLockStatus, VerifiedSaveStatus } from "../settings/VoiceLockStatus";
import { applyNativeLockToDraft, clearSubmittedFields, type VoiceLockState } from "../settings/voiceLockState";
import { isPlausibleGeminiKey } from "./keyFormat";
import { groqKeyHint, GROQ_KEY_URL } from "./keyHints";
import { identityBanner } from "../setup/identityBanner";
import { VoiceEnrollment } from "../setup/VoiceEnrollment";
import "./voiceEnrollmentHub.css";
import {
  setSidecarBaseUrl,
  getOAuthStatus,
  connectOAuth,
  disconnectOAuth,
  type OAuthStatus,
} from "../setup/oauth";
import { useAssistant } from "../store/assistant";
import { DockButtons, DockIcon } from "../sidebar/kit/DockButtons";
import { applyThemePreference, normalizePreference, resolveThemeMode, type ThemePreference } from "../sidebar/theme";
import "./hub/hub.css";
import { HubHome, type HubPage } from "./hub/HubHome";
import { ApiKeysPage } from "./hub/ApiKeysPage";
import { MemoryPage } from "./hub/MemoryPage";
import { WAKE_SOUNDS, clampWakeVolume, normalizeWakeSound } from "./wakeSounds";
import { ActionsPage } from "./hub/ActionsPage";
import { DeviceLinksPage } from "./hub/DeviceLinksPage";
import { ActivityManagerPage } from "./hub/ActivityManagerPage";
import { useAccounts, useInsights } from "./hub/useHubData";
import { McpServersPanel } from "./McpServersPanel";
import { ApiKeyIcon } from "./ApiKeyIcons";
import { OptionCard, SwitchCard } from "../theme/OptionCard";
import { AppearancePage, initWindowBorderColor } from "./hub/AppearancePage";

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
  sttDiagnostics?: boolean;
  serverUrl: string;
  userId: string;
  deviceId: string;
  ttsVoice: string;
  speechRate: number;
  ttsVolume: number;
  wakeSound?: boolean;
  wakeSoundStyle?: string;
  wakeSoundVolume?: number;
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
  // Moonshine STT model (small_streaming=7.84% WER ≈300MB default; medium_streaming=6.65% ≈400MB)
  moonshineModel?: string;
  // TTS voice emotion (auto = heuristics per reply; prosody via rate/pitch)
  ttsEmotion?: string;
  // Personality tone (butler = formal "sir"; friend = name, casual, honest counsel)
  personaMode?: string;
  // Memory Core: hide known people's names from cloud models (Memory page toggles it)
  memcoreRedactNames?: boolean;
  // Memory Core: record the working window (where-you-left-off) / speak the boot briefing
  memcoreActivity?: boolean;
  memcoreBriefing?: boolean;
  memcoreProfile?: boolean;
  memcoreAccountSeed?: boolean;
  memcoreRoutines?: boolean;
  memcoreReview?: boolean;
  memcoreTimetable?: boolean;
  memcoreMail?: boolean;
  memcoreMailPlain?: boolean;
  memcoreGithubWatch?: boolean;
  memcoreDeployWatch?: boolean;
  memcoreWhatsapp?: boolean;
  memcoreWhatsappSpeak?: boolean;
  memcoreLeaveBy?: boolean;
  memcoreSession?: boolean;
  memcoreHealth?: boolean;
  powerCommands?: boolean;
  // Command Hub theme (opaque "dark" default / "light"; no transparency)
  themeMode?: string;
  // Ghost vision provider order (auto = Groq → Gemini fallback)
  visionProvider?: string;
  // Ghost vision strategy (speed = race both providers in parallel)
  visionRace?: string;
  // Telegram owner chat id (remote bridge; token lives in vault)
  telegramChatId?: string;
  // Ghost FIFO queue: speak "Queued, sir." once per session at depth 2+ (D5/D6)
  ghostDepthAck?: boolean;
  sttConfidenceGate?: boolean;
  sttCommandHints?: boolean;
  wakeSeedOffsetMs?: number;
  wakeDspProfile?: string;
  wakeVerifyMode?: string;
  wakeVerifyHigh?: number;
  // Ghost FIFO queue: per-step watchdog timeout, ms (D7)
  ghostStepTimeoutMs?: number;
  // Ghost FIFO queue: inter-command gap τ, ms (D4)
  ghostTurnGapMs?: number;
  ghostEngagementSecs?: number;
  directedGate?: boolean;
  handMode?: boolean;
  handMousePreview?: boolean;
  handShowCamera?: boolean;
  handFrameSource?: string;
  handTrace?: boolean;
  handWithGhost?: boolean;
  handSnap?: boolean;
  handGestures?: boolean;
  handGesturesDesktop?: boolean;
  handFocusMode?: boolean;
  handFocusLowerApps?: boolean;
  handSensitivity?: number;
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
  // Orb background capsule style ("classic" | "pebble")
  orbCapsuleStyle?: "classic" | "pebble";
  // Orb wakeup animation mode (1..5) & genesis mode (0..3)
  orbWakeupMode?: number;
  orbInteriorGenesis?: number;
  // Orb morphology choices
  orbThinkMode?: number;
  orbSpeakMode?: number;
  orbListenMode?: number;
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
  localSttOnly: false,
  serverUrl: "",
  userId: "",
  deviceId: "",
  ttsVoice: "en-US-AvaNeural",
  speechRate: 1.15,
  ttsVolume: 75,
  wakeSound: true,
  wakeSoundStyle: "rise",
  wakeSoundVolume: 70,
  ttsProvider: "edge",
  groqApiKey: "",
  edgeTtsVoice: "en-US-AvaNeural",
  orbHorizontalPct: 0.5,
  orbVerticalPct: 0.0,
  orbPosition: "top",
  orbCapsuleStyle: "pebble",
  orbWakeupMode: 1,
  orbInteriorGenesis: 2,
  orbSize: 200,
  orbThinkMode: 6,
  orbSpeakMode: 2,
  orbListenMode: 1,
  geminiApiKey: "",
  cerebrasApiKey: "",
  moonshineModel: "small_streaming",
  ttsEmotion: "auto",
  personaMode: "butler",
  themeMode: "dark",
  visionProvider: "auto",
  visionRace: "sequential",
  telegramChatId: "",
  ghostDepthAck: true,
  sttConfidenceGate: false,
  sttCommandHints: false,
  wakeSeedOffsetMs: 0,
  wakeDspProfile: "legacy",
  wakeVerifyMode: "",
  wakeVerifyHigh: 0.97,
  ghostStepTimeoutMs: 15000,
  ghostTurnGapMs: 1000,
  ghostEngagementSecs: 0,
  directedGate: true,
  handMode: true,
  handMousePreview: false,
  handShowCamera: false,
  handFrameSource: "auto",
  handTrace: false,
  handWithGhost: false,
  handSnap: true,
  handGestures: true,
  handGesturesDesktop: false,
  handFocusMode: true,
  handFocusLowerApps: true,
  handSensitivity: 0,
  wavesHorizontalPct: 0.5,
  wavesVerticalPct: 1.0,
  wavesSize: 200,
  loadingHorizontalPct: 0.95,
  loadingVerticalPct: 0.05,
  loadingSize: 80,
  orbColor: "#f2b859",
  calibrationDevPersist: false,
};

const PAGE_TITLES: Record<HubPage, string> = {
  home: "Command Hub",
  keys: "API Keys",
  memory: "Memory",
  actions: "Actions & Permissions",
  "device-links": "Device Links",
  activity: "Activity Manager",
  audio: "Audio",
  display: "Display",
  connections: "MCP",
  advanced: "Advanced",
  appearance: "Appearance",
};

interface SubpageErrorBoundaryProps {
  children: React.ReactNode;
  page: string;
  onReset: () => void;
}

interface SubpageErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class SubpageErrorBoundary extends Component<SubpageErrorBoundaryProps, SubpageErrorBoundaryState> {
  constructor(props: SubpageErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error) {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error(`[Command Hub Subpage Error on "${this.props.page}"]:`, error, errorInfo);
  }

  componentDidUpdate(prevProps: SubpageErrorBoundaryProps) {
    if (prevProps.page !== this.props.page && this.state.hasError) {
      this.setState({ hasError: false, error: null });
    }
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="hx-page" style={{ padding: "16px 14px" }}>
          <div className="hx-card hx-key" style={{ padding: "16px", border: "1px solid rgba(255, 90, 82, 0.3)" }}>
            <div className="hx-key-title" style={{ color: "var(--hx-danger, #ff5a52)", marginBottom: 8, fontSize: 14 }}>
              ⚠️ Unable to display {this.props.page} subpage
            </div>
            <div className="hx-key-sub" style={{ marginBottom: 16, fontFamily: "monospace", fontSize: 11, wordBreak: "break-all" }}>
              {this.state.error?.message || "An unexpected error occurred while rendering this subpage."}
            </div>
            <button type="button" className="hx-btn hx-btn--primary" onClick={this.props.onReset}>
              ‹ Return to Command Hub
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export function SettingsSidebarApp({ onDock = () => {} }: { onDock?: (d: string) => void }) {
  const dirtyFields = useRef<Set<keyof Settings>>(new Set());
  const [page, setPage] = useState<HubPage>("home");
  const [insightsKey, setInsightsKey] = useState(0);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [saved, setSaved] = useState(false);
  const voiceLock = useVoiceLockState();
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  useEffect(() => {
    setSettings(previous => {
      const next = applyNativeLockToDraft(previous, dirtyFields.current, voiceLock.state.effective);
      settingsRef.current = next;
      return next;
    });
  }, [voiceLock.state.effective]);

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
    initWindowBorderColor();
    invoke<Partial<Settings>>("get_settings").then((s) => {
      // API keys are managed on the API Keys page (keychain); the form never
      // holds them, so a Save can neither resurrect nor wipe a key.
      if (s) setSettings(prev => {
        const loaded = { ...DEFAULT_SETTINGS, ...s, groqApiKey: "", geminiApiKey: "", cerebrasApiKey: "" };
        for (const key of dirtyFields.current) Object.assign(loaded, { [key]: prev[key] });
        const reconciled = applyNativeLockToDraft(loaded, dirtyFields.current, voiceLock.state.effective);
        settingsRef.current = reconciled;
        return reconciled;
      });
    }).catch(() => {});
  }, []);

  // Listen for live sidebar:backdrop events (listener retained for lifecycle compatibility)
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>("sidebar:backdrop", () => {});
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
    dirtyFields.current.add(key);
    settingsRef.current = { ...settingsRef.current, [key]: value };
    setSettings(settingsRef.current);
    setSaved(false);
  }, []);

  const handleSave = async () => {
    if (voiceLock.state.saving) return;
    setSaved(false);
    const snapshot = { ...settingsRef.current };
    const changed = [...dirtyFields.current];
    try {
      const patch = Object.fromEntries(changed.map(key => [key, snapshot[key]]));
      await voiceLock.save(patch);
      // Edits made while IPC was pending still need their own Save.
      const submitted = Object.fromEntries(changed.map(key => [key, snapshot[key]])) as Partial<Settings>;
      clearSubmittedFields<Partial<Settings>>(dirtyFields.current, submitted, settingsRef.current);
      setSaved(dirtyFields.current.size === 0);
      setTimeout(() => setSaved(false), 2000);
    } catch (error) {
      setToast(String(error));
    }
  };

  const handleReset = () => {
    dirtyFields.current = new Set(Object.keys(DEFAULT_SETTINGS) as (keyof Settings)[]);
    settingsRef.current = DEFAULT_SETTINGS;
    setSettings(DEFAULT_SETTINGS);
    setSaved(false);
  };

  const acc = useAccounts(settings.serverUrl, settings.userId);
  const insights = useInsights(insightsKey);
  const themePreference = normalizePreference(settings.themeMode);
  // What is on screen right now: "system" resolves against Windows.
  const themeMode: "light" | "dark" = resolveThemeMode(settings.themeMode);

  const handleTheme = (m: ThemePreference) => {
    update("themeMode", m);
    applyThemePreference(m);
    // Persist immediately: the Command Hub home has no Save button. Patch only the theme: re-saving the whole
    // (possibly stale) form here used to revert anything another window or IPC had changed.
    invoke("settings_patch", { patch: { themeMode: m } }).catch(() => {});
  };

  const isLegacyPage = page === "audio" || page === "display" || page === "advanced";

  return (
    <div className="hx">
      <header className="hx-header" data-tauri-drag-region>
        <div className="hx-header-left" data-tauri-drag-region>
          {page === "home" ? (
            <button type="button" className="hx-header-btn" onClick={() => invoke("hide_settings_sidebar").catch(() => {})}>
              Close
            </button>
          ) : (
            <button type="button" className="hx-header-btn" onClick={() => setPage("home")}>
              ‹ Back
            </button>
          )}
        </div>
        <div className="hx-title" data-tauri-drag-region>{PAGE_TITLES[page]}</div>
        <div className="hx-header-dock" data-tauri-drag-region>
          <DockButtons onDock={onDock} />
        </div>
      </header>

      {toast && <div className="hx-toast">{toast}</div>}

      <div className="hx-scroll">
        <SubpageErrorBoundary page={page} onReset={() => setPage("home")}>
          {page === "home" && (
            <HubHome acc={acc} insights={insights} themeMode={themeMode} onTheme={handleTheme} onOpen={setPage} />
          )}
          {page === "keys" && <ApiKeysPage onChanged={() => setInsightsKey((k) => k + 1)} />}
          {page === "memory" && <MemoryPage onFlagChanged={(flag, on) => update(flag, on)} />}
          {page === "actions" && <ActionsPage />}
          {page === "device-links" && <DeviceLinksPage onOpenActivity={() => setPage("activity")} />}
          {page === "activity" && <ActivityManagerPage />}
          {page === "connections" && <McpServersPanel />}
          {page === "appearance" && <AppearancePage themeMode={themePreference} onTheme={handleTheme} />}
          {isLegacyPage && (
            <div className="hx-legacy">
              {page === "display" && <DisplayTab settings={settings} update={update} />}
              {page === "audio" && <AudioTab settings={settings} update={update} voiceLock={voiceLock} />}
              {page === "advanced" && <><AgentWatchSection /><AdvancedSystemStatus /><AuthTab settings={settings} update={update} /></>}
            </div>
          )}
        </SubpageErrorBoundary>
      </div>

      {isLegacyPage && (
        <div className="hx-footer">
          <VerifiedSaveStatus model={voiceLock} draft={settings.speakerVerification} saved={saved} />
          <div className="hx-footer-actions">
            <button className="hx-btn" onClick={handleReset}>Reset</button>
            <button className="hx-btn hx-btn--primary" onClick={handleSave} disabled={voiceLock.state.saving}>{voiceLock.state.saving ? "Saving…" : "Save"}</button>
          </div>
        </div>
      )}
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
  const currentCapsuleStyle = settings.orbCapsuleStyle || "classic";
  const currentWakeupMode = settings.orbWakeupMode ?? 1;
  const currentInteriorGenesis = settings.orbInteriorGenesis ?? 2;

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

  const onCapsuleStyleSelect = (style: "classic" | "pebble", mode?: number, genesis?: number) => {
    update("orbCapsuleStyle", style);
    const m = mode ?? currentWakeupMode;
    const g = genesis ?? currentInteriorGenesis;
    if (mode !== undefined) update("orbWakeupMode", m);
    if (genesis !== undefined) update("orbInteriorGenesis", g);
    try {
      localStorage.setItem("nexus:orb_capsule_style", style);
      localStorage.setItem("nexus:orb_wakeup_mode", String(m));
      localStorage.setItem("nexus:orb_interior_genesis", String(g));
    } catch {}
    useAssistant.getState().setOrbCapsuleStyle(style);
    useAssistant.getState().setOrbWakeupMode(m);
    useAssistant.getState().setOrbInteriorGenesis(g);
    import("@tauri-apps/api/core").then(({ invoke }) => {
      invoke("set_orb_capsule_style", { style, mode: m, genesis: g }).catch(() => {});
    }).catch(() => {});
  };

  return (
    <>
      <div className="settings-section">
        <div className="settings-section-title">Orb Aura</div>
        <div className="setting-desc" style={{ marginBottom: 10 }}>
          Accent color for active and idle states.
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
            <div className="setting-label">Hex Color</div>
            <div className="setting-desc">Custom hex code</div>
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
        <div className="settings-section-title">Screen Anchor</div>
        <div className="setting-desc" style={{ marginBottom: 12 }}>
          Desktop docking edge.
        </div>
        <div className="nx-options-grid">
          <OptionCard
            icon="⬒"
            title="Top Anchor"
            desc="Screen top bezel"
            selected={currentPos === "top"}
            onClick={() => onPositionSelect("top")}
          />
          <OptionCard
            icon="⬓"
            title="Bottom Anchor"
            desc="Taskbar edge"
            selected={currentPos === "bottom"}
            onClick={() => onPositionSelect("bottom")}
          />
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Capsule Geometry</div>
        <div className="setting-desc" style={{ marginBottom: 12 }}>
          Bezel container style.
        </div>
        <div className="nx-options-grid">
          <OptionCard
            icon="⬝"
            title="Classic Capsule"
            desc="Pitch-black capsule"
            selected={currentCapsuleStyle === "classic"}
            onClick={() => onCapsuleStyleSelect("classic")}
          />
          <OptionCard
            icon="◗"
            title="Fluid Scoop"
            desc="Catenary curve"
            selected={currentCapsuleStyle === "pebble"}
            onClick={() => onCapsuleStyleSelect("pebble")}
          />
        </div>

        {currentCapsuleStyle === "pebble" && (
          <div style={{ marginTop: 14, display: "flex", flexDirection: "column", gap: 10 }}>
            <div style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--hx-dim)" }}>
              Wakeup Animation
            </div>
            <div>
              {[
                { id: 1, icon: "🧵", name: "Elastic Stretch & Snap", desc: "Harmonic damping" },
                { id: 2, icon: "🏹", name: "Bowstring Pluck", desc: "Sharp overshoot" },
                { id: 3, icon: "💧", name: "Liquid Extrusion", desc: "Fluid mercury droplet" },
                { id: 4, icon: "🌅", name: "Horizon Split", desc: "Wing specular gleam" },
                { id: 5, icon: "〰️", name: "Acoustic Ripple", desc: "Harmonic ripples" },
              ].map((m) => (
                <OptionCard
                  key={m.id}
                  icon={m.icon}
                  title={m.name}
                  desc={m.desc}
                  selected={currentWakeupMode === m.id}
                  onClick={() => onCapsuleStyleSelect("pebble", m.id, currentInteriorGenesis)}
                />
              ))}
            </div>

            <div style={{ fontSize: 11, fontWeight: 700, textTransform: "uppercase", letterSpacing: 0.5, color: "var(--hx-dim)", marginTop: 4 }}>
              Interior Genesis
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}>
              {[
                { id: 2, name: "Spark Burst" },
                { id: 0, name: "Core Bloom" },
                { id: 1, name: "Droplet Roll" },
                { id: 3, name: "Helix Weave" },
              ].map((g) => (
                <button
                  key={g.id}
                  type="button"
                  className={`settings-btn ${currentInteriorGenesis === g.id ? "settings-btn--primary" : "settings-btn--glass"}`}
                  style={{
                    padding: "7px 10px",
                    textAlign: "center",
                    borderRadius: 8,
                    fontSize: 11.5,
                    fontWeight: currentInteriorGenesis === g.id ? 700 : 500,
                    cursor: "pointer",
                    border: currentInteriorGenesis === g.id ? "1.5px solid rgba(255,255,255,0.8)" : "1px solid rgba(255,255,255,0.1)",
                    background: currentInteriorGenesis === g.id ? "rgba(255,255,255,0.18)" : "rgba(255,255,255,0.04)",
                  }}
                  onClick={() => onCapsuleStyleSelect("pebble", currentWakeupMode, g.id)}
                >
                  {g.name}
                </button>
              ))}
            </div>

            <button
              type="button"
              className="settings-btn settings-btn--glass"
              style={{
                marginTop: 6,
                padding: "10px 14px",
                borderRadius: 8,
                fontSize: 12,
                fontWeight: 600,
                color: "#38bdf8",
                borderColor: "rgba(56, 189, 248, 0.4)",
                background: "rgba(56, 189, 248, 0.08)",
                cursor: "pointer",
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                gap: 6,
              }}
              onClick={() => {
                import("@tauri-apps/api/core").then(({ invoke }) => {
                  invoke("test_wake_orb").catch(() => {});
                }).catch(() => {});
              }}
            >
              <span>✦ Test Wakeup on Desktop</span>
            </button>
          </div>
        )}
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Sidebar Placement</div>
        <div className="setting-desc" style={{ marginBottom: 8 }}>
          Screen position and docking.
        </div>
        <div style={{ display: "flex", gap: 6 }}>
          <button
            className="settings-btn settings-btn--glass"
            style={{ flex: 1 }}
            onClick={() => {
              invoke("set_sidebar_dock", { dock: "left" }).catch(() => {});
            }}
          >
            <DockIcon side="left" /> Dock Left
          </button>
          <button
            className="settings-btn settings-btn--glass"
            style={{ flex: 1 }}
            onClick={() => {
              invoke("set_sidebar_dock", { dock: "right" }).catch(() => {});
            }}
          >
            <DockIcon side="right" /> Dock Right
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

      <div className="settings-section">
        <div className="settings-section-title">3D Morphology Studio</div>
        <div className="setting-desc" style={{ marginBottom: 10 }}>
          Real-time particle dynamics.
        </div>
        <button
          type="button"
          className="settings-btn settings-btn--glass"
          style={{
            width: "100%",
            padding: "10px 14px",
            borderRadius: 10,
            color: "#ffffff",
            fontWeight: 600,
            fontSize: 13,
            cursor: "pointer",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            gap: 8,
          }}
          onClick={() => {
            invoke("open_orb_studio_window").catch(() => {});
          }}
        >
          ✦ Open Orb Customization Studio ›
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

function AudioTab({ settings, update, voiceLock }: {
  voiceLock: VoiceLockState;
  settings: Settings;
  update: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}) {
  const volume = settings.ttsVolume ?? 75;
  const [profileReady, setProfileReady] = useState(false);
  const handleProfileStatus = useCallback((ready: boolean) => { setProfileReady(ready); void voiceLock.refresh(); }, [voiceLock]);
  useEffect(() => { void voiceLock.refresh(); }, [voiceLock]);
  const [personas, setPersonas] = useState<VoicePersona[]>([]);
  const [previewing, setPreviewing] = useState<string | null>(null);
  const [wakePreviewError, setWakePreviewError] = useState<string | null>(null);
  const [equipping, setEquipping] = useState<string | null>(null);
  const [sync, setSync] = useState<Record<string, { state: "ready" | "cloud_only" | "downloading"; progress?: number; phase?: string }>>({});
  const [transport, setTransport] = useState<"cloud" | "local">("cloud");
  const [diagnostics, setDiagnostics] = useState<Array<{
    session: number; captured_at: number; raw_text: string; corrected_text: string;
    intent: string; decision: string; stats: Record<string, unknown>;
  }> | null>(null);

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
        update("ttsVoice", res.cloudId);
        update("ttsProvider", "edge");
        update("selectedVoice", res.voiceKey);
        update("offlineVoiceModel", p.kokoroVoice);
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
          Equip instant cloud &amp; offline voices.
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
        <div className="settings-section-title">Orb Visual Morphology</div>
        <div className="setting-row" style={{ alignItems: "center" }}>
          <div>
            <div className="setting-label">Orb Animation</div>
            <div className="setting-desc">3D WebGL particle morphologies</div>
          </div>
          <div className="setting-control">
            <button
              className="settings-btn settings-btn--glass"
              style={{
                background: "rgba(168, 85, 247, 0.15)",
                borderColor: "rgba(168, 85, 247, 0.4)",
                color: "#c084fc",
                fontWeight: 600,
                display: "inline-flex",
                alignItems: "center",
                gap: "6px"
              }}
              onClick={() => invoke("open_orb_studio_window").catch(() => {})}
            >
              ✦ Customize Orb
            </button>
          </div>
        </div>
      </div>

      <HandModeSection settings={settings} update={update} />

      <div className="settings-section">
        <div className="settings-section-title">Personality & Tone</div>
        <div className="setting-desc" style={{ marginBottom: 12 }}>
          Greeting style and conversational demeanor.
        </div>
        <div className="nx-options-grid">
          <OptionCard
            icon="🎩"
            title="Formal Butler"
            desc="Formal address ('sir', polite counsel)"
            selected={(settings.personaMode ?? "butler") === "butler"}
            onClick={() => update("personaMode", "butler")}
          />
          <OptionCard
            icon="🤝"
            title="Casual Partner"
            desc="Casual address (first name, honest counsel)"
            selected={(settings.personaMode ?? "butler") === "friend"}
            onClick={() => update("personaMode", "friend")}
          />
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Wake Sound</div>
        <SwitchCard
          icon="🔔"
          title="Earcon Audio Cue"
          desc="Play an immediate chime cue when NEXUS wakes up."
          checked={settings.wakeSound !== false}
          onChange={(checked) => update("wakeSound", checked)}
        />
        {settings.wakeSound !== false && (
          <div style={{ marginTop: 10 }}>
            {WAKE_SOUNDS.map((w) => {
              const chosen = normalizeWakeSound(settings.wakeSoundStyle) === w.key;
              return (
                <OptionCard
                  key={w.key}
                  icon="🎵"
                  title={w.label}
                  desc={w.blurb}
                  selected={chosen}
                  onClick={() => update("wakeSoundStyle", w.key)}
                />
              );
            })}
            <button
              type="button"
              className="settings-btn"
              onClick={() => {
                setWakePreviewError(null);
                void invoke("preview_wake_sound", {
                  style: normalizeWakeSound(settings.wakeSoundStyle),
                  volume: clampWakeVolume(settings.wakeSoundVolume),
                }).catch((error) => setWakePreviewError(String(error)));
              }}
            >
              Play selected sound
            </button>
            {wakePreviewError && <div role="alert" className="setting-desc">{wakePreviewError}</div>}
          </div>
        )}
        <div className="setting-row">
          <div>
            <div className="setting-label">Sound volume</div>
            <div className="setting-desc">Chime output level</div>
          </div>
          <div className="setting-control">
            <input
              type="range"
              className="settings-slider"
              min={0}
              max={100}
              value={clampWakeVolume(settings.wakeSoundVolume)}
              onChange={(e) => update("wakeSoundVolume", parseInt(e.target.value))}
            />
            <span className="slider-value">{clampWakeVolume(settings.wakeSoundVolume)}%</span>
          </div>
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Speech Volume</div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Output volume</div>
            <div className="setting-desc">NEXUS speech output level</div>
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
            <div className="setting-desc">Contextual inflection</div>
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
        <SwitchCard
          icon="🎙️"
          title="Temporary STT diagnostics"
          desc="Store encrypted microphone clips and turn evidence locally for up to 24 hours. Off by default."
          checked={settings.sttDiagnostics === true}
          onChange={(checked) => update("sttDiagnostics", checked)}
        />
        <button className="settings-input" type="button" onClick={() => {
          void invoke<typeof diagnostics>("stt_diagnostics_list")
            .then((items) => setDiagnostics(items ?? []))
            .catch(() => setDiagnostics([]));
        }}>View recent STT turn evidence</button>
        {diagnostics && (
          <div style={{ maxHeight: 260, overflowY: "auto", fontSize: 12, marginTop: 8 }}>
            {diagnostics.length === 0 ? "No retained diagnostic turns." : diagnostics.slice(-20).reverse().map((item) => (
              <div key={`${item.captured_at}-${item.session}`} style={{ marginBottom: 10, padding: 8, border: "1px solid currentColor", borderRadius: 6 }}>
                <strong>{new Date(item.captured_at * 1000).toLocaleString()} · {item.decision} · {item.intent}</strong>
                <div>Raw: {item.raw_text || "(none)"}</div>
                <div>Corrected: {item.corrected_text || "(none)"}</div>
                <div>Capture: {JSON.stringify(item.stats)}</div>
              </div>
            ))}
          </div>
        )}
        <button className="settings-input" type="button" onClick={() => {
          void invoke<number>("stt_diagnostics_delete").then((count) => {
            setDiagnostics([]);
            window.alert(`Deleted ${count} encrypted STT diagnostic clip(s).`);
          }).catch(() => window.alert("Could not delete STT diagnostics."));
        }}>Delete STT diagnostics now</button>
        <div className="setting-row">
          <div>
            <div className="setting-label">Local STT only</div>
            <div className="setting-desc">On-device processing</div>
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
            <div className="setting-label">Wake word speech check</div>
            <div className="setting-desc">Always: every wake is double-checked by speech recognition (fewest false wakes, can miss real ones). Tiered: skip the check when the wake model is very sure. Off: wake on the model alone (fastest). Pick with your voice scorecard.</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.wakeVerifyMode || ((settings as { verifyWake?: boolean }).verifyWake === false ? "off" : "always")}
              onChange={(e) => update("wakeVerifyMode", e.target.value)}
            >
              <option value="always">Always</option>
              <option value="tiered">Tiered</option>
              <option value="off">Off</option>
            </select>
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Command hints for speech</div>
            <div className="setting-desc">Experimental. Tells Groq the commands NEXUS understands. Can help "open Chrome"-style phrases; can also make it hear commands in noise. Compare with your voice scorecard.</div>
          </div>
          <div className="setting-control">
            <div
              className={`settings-toggle ${settings.sttCommandHints ? "settings-toggle--on" : ""}`}
              onClick={() => update("sttCommandHints", !settings.sttCommandHints)}
            />
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Reject low-confidence speech</div>
            <div className="setting-desc">Experimental. Drops Groq transcripts the engine itself is unsure about. Turn on only after your voice scorecard shows it helps.</div>
          </div>
          <div className="setting-control">
            <div
              className={`settings-toggle ${settings.sttConfidenceGate ? "settings-toggle--on" : ""}`}
              onClick={() => update("sttConfidenceGate", !settings.sttConfidenceGate)}
            />
          </div>
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Speaker verification</div>
            <div className="setting-desc">Enroll a profile below before turning on Voice Lock.</div>
          </div>
          <div className="setting-control">
            <div
              className={`settings-toggle ${settings.speakerVerification ? "settings-toggle--on" : ""}`}
              role="switch"
              aria-label="Draft Voice Lock"
              aria-checked={settings.speakerVerification}
              aria-disabled={voiceLock.state.saving || (!settings.speakerVerification && !profileReady)}
              tabIndex={0}
              onClick={() => {
                if (!voiceLock.state.saving && (settings.speakerVerification || profileReady)) update("speakerVerification", !settings.speakerVerification);
              }}
              onKeyDown={(event) => {
                if (!voiceLock.state.saving && (event.key === "Enter" || event.key === " ") && (settings.speakerVerification || profileReady)) {
                  event.preventDefault();
                  update("speakerVerification", !settings.speakerVerification);
                }
              }}
            />
          </div>
        </div>
        <div className="hub-voice-enrollment">
          <VoiceLockStatus model={voiceLock} draft={settings.speakerVerification} />
          <VoiceEnrollment onStatusChange={handleProfileStatus} />
        </div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Moonshine model</div>
            <div className="setting-desc">Accuracy vs RAM tradeoff</div>
          </div>
          <div className="setting-control">
            <select
              className="settings-input"
              value={settings.moonshineModel ?? "small_streaming"}
              onChange={(e) => update("moonshineModel", e.target.value)}
            >
              <option value="medium_streaming">Medium v2 (245M, ~400MB)</option>
              <option value="small_streaming">Small v2 (123M, ~300MB)</option>
              <option value="tiny_streaming">Tiny (34M, ~100MB)</option>
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
  phone?: string | null;
  address?: string | null;
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
  const [credStatus, setCredStatus] = useState<{ configured: boolean; has_client_id: boolean; has_secret: boolean; message?: string | null } | null>(null);
  const [credSaved, setCredSaved] = useState(false);

  const loadCredStatus = useCallback(async () => {
    try {
      const st = await invoke<{ configured: boolean; has_client_id: boolean; has_secret: boolean; message?: string | null }>("google_credentials_status");
      setCredStatus(st);
      // Not set up yet: open the drawer so the missing step is in front of the user.
      if (!st.configured) setShowCustomCreds(true);
    } catch {
      /* status is advisory */
    }
  }, []);
  useEffect(() => {
    void loadCredStatus();
  }, [loadCredStatus]);

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

  // Progressive re-consent (optional phone + address scopes). Absence is
  // normal — the button simply reappears if nothing was granted/saved.
  const [extendingGoogle, setExtendingGoogle] = useState(false);
  const handleExtendGoogle = async () => {
    setError(null);
    setExtendingGoogle(true);
    try {
      await invoke("google_connect_extended");
      await loadGoogleAccounts();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setExtendingGoogle(false);
    }
  };

  const handleSaveCustomCreds = async () => {
    setError(null);
    setCredSaved(false);
    try {
      await invoke("google_save_custom_credentials", {
        clientId,
        clientSecret: clientSecret || null,
      });
      setClientSecret("");
      setCredSaved(true);
      await loadCredStatus();
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

  // Phone-width column (carbon copy): the whole Accounts view reads as
  // a phone-sized card centered in the wider sidebar window.
  return (
    <div className="hub-phone-col">
      {error && (
        <div className="auth-card" style={{ borderColor: "rgba(255,80,80,0.3)" }}>
          <span style={{ fontSize: 12, color: "rgba(255,150,150,0.9)" }}>{error}</span>
        </div>
      )}

      {/* Profile header card (carbon copy): primary/first account, Add Account link */}
      <div className="hub-card hub-profile">
        {(() => {
          const primary = googleAccounts.find((a) => a.is_primary) ?? googleAccounts[0];
          if (!primary) {
            return (
              <div className="hub-profile--empty">
                <div className="hub-profile-name">No account yet</div>
                <div className="hub-profile-email">Gmail, Calendar &amp; Photos sync.</div>
              </div>
            );
          }
          return (
            <>
              {primary.picture ? (
                <img src={primary.picture} alt="" className="hub-avatar hub-avatar--lg" />
              ) : (
                <div className="hub-avatar hub-avatar--lg hub-avatar--fallback" aria-hidden="true">
                  {(primary.name || primary.email).charAt(0).toUpperCase()}
                </div>
              )}
              <div className="hub-profile-name">{primary.name}</div>
              <div className="hub-profile-email">{primary.email}</div>
            </>
          );
        })()}
        <button
          className="hub-link"
          disabled={connectingGoogle}
          onClick={handleConnectGoogleNative}
        >
          {connectingGoogle ? "Signing in..." : "+ Add Account"}
        </button>
      </div>

      {/* Per-account rows: one row per account (photo + name + email). */
      /* Row tap promotes to primary; Remove stays an explicit control. */}
      {googleAccounts.length > 0 && (
        <div className="hub-card hub-accounts" role="list" aria-label="Connected accounts">
          {googleAccounts.map((acc) => (
            <div
              key={acc.email}
              role="listitem"
              className={`hub-account-row ${acc.is_primary ? "hub-account-row--primary" : ""}`}
              onClick={() => { if (!acc.is_primary) void handleSetPrimaryGoogle(acc.email); }}
              title={acc.is_primary ? "Primary account" : "Tap to make primary"}
            >
              {acc.picture ? (
                <img src={acc.picture} alt="" className="hub-avatar" />
              ) : (
                <div className="hub-avatar hub-avatar--fallback" aria-hidden="true">
                  {(acc.name || acc.email).charAt(0).toUpperCase()}
                </div>
              )}
              <div className="hub-account-meta">
                <div className="hub-account-name">
                  {acc.name}
                  {acc.is_primary && <span className="hub-primary-badge">Primary</span>}
                </div>
                <div className="hub-account-email">{acc.email}</div>
                {(acc.phone || acc.address) ? (
                  <div className="hub-account-contact">
                    {[acc.phone, acc.address].filter(Boolean).join(" · ")}
                  </div>
                ) : (
                  <button
                    className="hub-link hub-link--sm"
                    title="Phone & address scopes"
                    onClick={(e) => { e.stopPropagation(); void handleExtendGoogle(); }}
                  >
                    {extendingGoogle ? "Opening Google…" : "Add phone & address"}
                  </button>
                )}
              </div>
              <span className="hub-chevron" aria-hidden="true">›</span>
              <button
                className="hub-remove"
                title={`Remove ${acc.email}`}
                onClick={(e) => { e.stopPropagation(); void handleDisconnectGoogle(acc.email); }}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      )}

        <div style={{ marginTop: 8 }}>
          <button
            className="settings-btn settings-btn--glass"
            style={{ fontSize: 10, opacity: 0.7 }}
            onClick={() => setShowCustomCreds(!showCustomCreds)}
          >
            {showCustomCreds ? "Hide Developer Credentials" : "Custom Developer Credentials"}
          </button>
          {credStatus && !credStatus.configured && (
            <div className="hx-error" style={{ marginTop: 8, fontSize: 11 }}>
              {credStatus.message}
            </div>
          )}
          {credStatus?.configured && (
            <div className="hx-note" style={{ marginTop: 8, fontSize: 11 }}>
              Google sign-in is set up. Click “Add Google Account”.
            </div>
          )}
          {showCustomCreds && (
            <div className="auth-card" style={{ marginTop: 8, padding: 12 }}>
              <div style={{ fontSize: 11, color: "rgba(255,255,255,0.7)", marginBottom: 8 }}>
                Google Cloud Desktop OAuth credentials:
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
                placeholder="Client Secret (required)"
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
              {credSaved && (
                <div className="hx-note" style={{ marginTop: 6, fontSize: 11 }}>
                  Saved. Now click “Add Google Account”.
                </div>
              )}
            </div>
          )}
        </div>

      <div className="settings-section">
        <div className="settings-section-title">GitHub Account</div>
        <div className="auth-card">
          <div className="auth-card-header">
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <ApiKeyIcon service="github" size={24} />
              <span className="auth-card-title">GitHub</span>
            </div>
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

        <div className="auth-card">
          <div className="auth-card-header">
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <ApiKeyIcon service="gemini" size={24} />
              <span className="auth-card-title">Gemini API Key</span>
            </div>
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
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <ApiKeyIcon service="groq" size={24} />
              <span className="auth-card-title">Groq API Key</span>
            </div>
          </div>
          <input
            type="password"
            className="settings-input"
            placeholder="gsk_..."
            value={settings.groqApiKey}
            onChange={(e) => update("groqApiKey", e.target.value)}
          />
          <GroqKeyHint
            hint={groqKeyHint(settings.groqApiKey, settings.localSttOnly)}
            onEnableCloud={() => update("localSttOnly", false)}
          />
        </div>

        <div className="auth-card">
          <div className="auth-card-header">
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <ApiKeyIcon service="cerebras" size={24} />
              <span className="auth-card-title">Cerebras API Key</span>
            </div>
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
          Keys stay on this device (OS keychain + your local settings file) — never uploaded, never in Cloudflare.
        </div>
      </div>

      <div className="settings-section">
        <div className="settings-section-title">Ghost Vision</div>
        <div className="setting-row">
          <div>
            <div className="setting-label">Vision provider</div>
            <div className="setting-desc">Primary vision engine</div>
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
            <div className="setting-desc">Execution mode</div>
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
    </div>
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

function openExternal(url: string) {
  import("@tauri-apps/plugin-shell")
    .then(({ open }) => open(url).catch(() => {}))
    .catch(() => {
      window.open(url, "_blank");
    });
}

/** Guidance under the Groq key: where to get one, and the local-only trap. */
function GroqKeyHint({
  hint,
  onEnableCloud,
}: {
  hint: ReturnType<typeof groqKeyHint>;
  onEnableCloud: () => void;
}) {
  if (hint === "get-key") {
    return (
      <div className="setting-desc" style={{ marginTop: 4 }}>
        Free cloud speech-to-text + fast chat. Without a key NEXUS still works: speech runs locally.{" "}
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            openExternal(GROQ_KEY_URL);
          }}
        >
          Get a free key
        </a>
      </div>
    );
  }
  if (hint === "local-only-blocks") {
    return (
      <div className="setting-desc" style={{ marginTop: 4, color: "#ffd479" }}>
        “Local STT only” is on, so this key isn&apos;t used for speech yet — your voice stays on this PC.{" "}
        <a
          href="#"
          onClick={(e) => {
            e.preventDefault();
            onEnableCloud();
          }}
        >
          Turn it off to use cloud speech
        </a>{" "}
        (then press Save).
      </div>
    );
  }
  return (
    <div className="setting-desc" style={{ marginTop: 4 }}>
      Cloud speech-to-text is on. Falls back to local speech if Groq is unreachable.
    </div>
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
        className="settings-btn settings-btn--glass"
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

function AdvancedSystemStatus() {
  const [identity, setIdentity] = useState<{ state: string; profileId?: string; deviceName?: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    try {
      const res = await invoke<{ state: string; profileId?: string; deviceName?: string }>("get_identity_status");
      setIdentity(res);
    } catch {
      /* ignore */
    }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  const banner = identity ? identityBanner(identity) : null;
  return <div className="settings-section">
    <div className="settings-section-title">Device & system</div>
    <div className="auth-card">
      <div className="auth-card-header">
        <span className="auth-card-title">{banner?.title ?? "Checking device…"}</span>
        <button className="settings-btn" onClick={() => void refresh()}>Refresh</button>
      </div>
      {banner && <div className="setting-desc">{banner.subtitle}</div>}
      {identity?.deviceName && <div className="setting-desc">{identity.deviceName}</div>}
      {identity?.state === "approved" && <button className="settings-btn settings-btn--danger" onClick={async () => {
        if (!window.confirm("Disconnect this device from the NEXUS cloud?")) return;
        try { await invoke("disconnect_device"); await refresh(); }
        catch (e) { setError(e instanceof Error ? e.message : String(e)); }
      }}>Disconnect this device</button>}
    </div>
    {error && <div className="hx-error" role="alert">{error}</div>}
  </div>;
}
