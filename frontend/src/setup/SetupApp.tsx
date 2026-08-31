import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

import {
  setSidecarBaseUrl,
  connectOAuth,
  getOAuthStatus,
  type OAuthStatus,
} from "./oauth";
import { VoiceEnrollment } from "./VoiceEnrollment";
import { CURATED_VOICES, previewVoice, stopTts, type VoiceOption } from "../audio/ttsPlayer";

type Step = 0 | 1 | 2;
const STEP_LABELS = ["Persona & Voice", "Preferences", "Accounts"];

export function SetupApp() {
  const [step, setStep] = useState<Step>(0);
  const [serverUrl, setServerUrl] = useState("");
  const [userId, setUserId] = useState("");
  const [selectedVoice, setSelectedVoice] = useState<string>("af_sky");
  const [playingVoice, setPlayingVoice] = useState<string | null>(null);

  // Settings
  const [hotkey, setHotkey] = useState("Ctrl+Shift+Space");
  const [wakeWordEnabled, setWakeWordEnabled] = useState(true);
  const [autostart, setAutostart] = useState(true);

  // Accounts
  const [oauthStatus, setOauthStatus] = useState<Record<string, OAuthStatus>>({});
  const [connecting, setConnecting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // Load current settings and server config
  useEffect(() => {
    invoke<{ serverUrl: string; userId: string; deviceId: string }>("get_server_config")
      .then((cfg) => {
        setServerUrl(cfg.serverUrl);
        setUserId(cfg.userId);
      })
      .catch(() => {});

    invoke<any>("get_settings")
      .then((s) => {
        if (s) {
          if (s.ttsVoice) setSelectedVoice(s.ttsVoice);
          if (s.hotkey) setHotkey(s.hotkey);
          if (typeof s.wakeWordEnabled === "boolean") setWakeWordEnabled(s.wakeWordEnabled);
          if (typeof s.autostart === "boolean") setAutostart(s.autostart);
        }
      })
      .catch(() => {});
  }, []);

  const handlePreview = async (voice: VoiceOption, e: React.MouseEvent) => {
    e.stopPropagation();
    if (playingVoice === voice.id) {
      stopTts();
      setPlayingVoice(null);
      return;
    }
    setPlayingVoice(voice.id);
    await previewVoice(voice, undefined, () => {
      setPlayingVoice(null);
    });
  };

  const checkServer = useCallback(async () => {
    if (!serverUrl || !userId) return;
    setSidecarBaseUrl(serverUrl);
    try {
      const status = await getOAuthStatus(userId);
      setOauthStatus(status);
      setError(null);
    } catch {
      // Server unreachable
    }
  }, [serverUrl, userId]);

  useEffect(() => {
    if (step === 2) checkServer();
  }, [step, checkServer]);

  const handleConnect = async (provider: "google" | "github") => {
    if (!serverUrl) {
      setError("Server not configured");
      return;
    }
    setConnecting(provider);
    setError(null);
    try {
      setSidecarBaseUrl(serverUrl);
      await connectOAuth(provider, userId);
      await checkServer();
    } catch (err) {
      setError(`${provider} connection failed: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setConnecting(null);
    }
  };

  const saveAllSettings = async () => {
    try {
      const current = (await invoke<any>("get_settings").catch(() => ({}))) || {};
      const updated = {
        ...current,
        ttsVoice: selectedVoice,
        hotkey,
        wakeWordEnabled,
        autostart,
      };
      await invoke("save_settings", { settings: updated });
    } catch (e) {
      console.warn("Failed to persist settings:", e);
    }
  };

  const handleFinish = async () => {
    try {
      await saveAllSettings();
      await invoke("close_setup_window");
      setSaved(true);
    } catch (err) {
      setError(`Failed to finish: ${err instanceof Error ? err.message : String(err)}`);
    }
  };

  return (
    <div className="setup-root">
      {error && <div className="setup-error">{error}</div>}

      <div style={{ flex: 1 }}>
        {/* ── Step 0: Voice & Persona ── */}
          {step === 0 && (
            <div>
              <div style={{ textAlign: "center", marginBottom: "var(--nx-space-5)" }}>
                <h1 style={{ fontSize: "var(--nx-text-xl)", fontWeight: "bold", color: "var(--nx-text-primary)" }}>
                  Choose Your Assistant Persona
                </h1>
                <p style={{ color: "var(--nx-text-secondary)", fontSize: "var(--nx-text-sm)", marginTop: "4px" }}>
                  Select the voice & tone for NEXUS. You can change this anytime.
                </p>
              </div>

              <div className="setup-voice-grid">
                {CURATED_VOICES.map((voice) => {
                  const isSelected = selectedVoice === voice.id;
                  const isPlaying = playingVoice === voice.id;
                  return (
                    <div
                      key={voice.id}
                      className={`setup-voice-card ${isSelected ? "setup-voice-card--active" : ""}`}
                      onClick={() => setSelectedVoice(voice.id)}
                    >
                      <div className="setup-voice-card-header">
                        <span className="setup-voice-name">{voice.name}</span>
                        <span className="setup-voice-accent">{voice.accent}</span>
                      </div>
                      <p className="setup-voice-desc">{voice.description}</p>
                      <button
                        type="button"
                        className="setup-voice-play-btn"
                        onClick={(e) => handlePreview(voice, e)}
                      >
                        {isPlaying ? "⏹ Stop" : "▶ Play Sample"}
                      </button>
                    </div>
                  );
                })}
              </div>


            </div>
          )}

          {/* ── Step 1: Preferences ── */}
          {step === 1 && (
            <>
              <StepHeader step={step} />
              <section className="setup-section">
                <h2>Interaction Controls</h2>
                <p style={{ marginBottom: "var(--nx-space-4)", color: "var(--nx-text-secondary)", fontSize: "var(--nx-text-sm)" }}>
                  Configure your primary wake triggers and startup settings.
                </p>

                <div style={{ display: "flex", flexDirection: "column", gap: "var(--nx-space-3)" }}>
                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "12px", border: "1px solid var(--nx-border)", borderRadius: "8px" }}>
                    <div>
                      <div style={{ fontWeight: 600, fontSize: "var(--nx-text-sm)" }}>Wake Word ("NEXUS")</div>
                      <div style={{ fontSize: "var(--nx-text-xs)", color: "var(--nx-text-secondary)" }}>Local neural keyword spotter (openWakeWord)</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={wakeWordEnabled}
                      onChange={(e) => setWakeWordEnabled(e.target.checked)}
                      style={{ width: "18px", height: "18px", accentColor: "var(--nx-accent-blue)" }}
                    />
                  </div>

                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "12px", border: "1px solid var(--nx-border)", borderRadius: "8px" }}>
                    <div>
                      <div style={{ fontWeight: 600, fontSize: "var(--nx-text-sm)" }}>Global Hotkey</div>
                      <div style={{ fontSize: "var(--nx-text-xs)", color: "var(--nx-text-secondary)" }}>Instantly wake/toggle assistant</div>
                    </div>
                    <input
                      type="text"
                      value={hotkey}
                      onChange={(e) => setHotkey(e.target.value)}
                      style={{ padding: "6px 10px", fontSize: "var(--nx-text-xs)", border: "1px solid var(--nx-border)", borderRadius: "6px", width: "140px", textAlign: "center" }}
                    />
                  </div>

                  <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "12px", border: "1px solid var(--nx-border)", borderRadius: "8px" }}>
                    <div>
                      <div style={{ fontWeight: 600, fontSize: "var(--nx-text-sm)" }}>Start at Login</div>
                      <div style={{ fontSize: "var(--nx-text-xs)", color: "var(--nx-text-secondary)" }}>Launch in system background on boot</div>
                    </div>
                    <input
                      type="checkbox"
                      checked={autostart}
                      onChange={(e) => setAutostart(e.target.checked)}
                      style={{ width: "18px", height: "18px", accentColor: "var(--nx-accent-blue)" }}
                    />
                  </div>
                </div>

                <div style={{ marginTop: "var(--nx-space-5)" }}>
                  <h3 style={{ fontSize: "var(--nx-text-sm)", marginBottom: "var(--nx-space-2)" }}>Voice Lock (Optional)</h3>
                  <VoiceEnrollment />
                </div>
              </section>
            </>
          )}

          {/* ── Step 2: Accounts ── */}
          {step === 2 && (
            <>
              <StepHeader step={step} />
              <section className="setup-section">
                <h2>Connect Integrations</h2>
                <p style={{ marginBottom: "var(--nx-space-4)", color: "var(--nx-text-secondary)", fontSize: "var(--nx-text-sm)" }}>
                  Connect Google and GitHub to let NEXUS manage your emails, calendar, and GitHub repos. (You can also skip and connect later).
                </p>

                {/* Google card */}
                <div className="setup-provider setup-provider--large">
                  <div className="setup-provider-icon setup-provider-icon--google">
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="none">
                      <path d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z" fill="#4285F4"/>
                      <path d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z" fill="#34A853"/>
                      <path d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z" fill="#FBBC05"/>
                      <path d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84C6.71 7.31 9.14 5.38 12 5.38z" fill="#EA4335"/>
                    </svg>
                  </div>
                  <div className="setup-provider-info">
                    <h3>Google</h3>
                    <p>Gmail · Calendar · Meet</p>
                  </div>
                  {oauthStatus.google?.connected ? (
                    <span className="setup-badge setup-badge--ok">Connected</span>
                  ) : (
                    <button className="setup-btn setup-btn--primary setup-btn--small" disabled={connecting !== null} onClick={() => handleConnect("google")}>
                      {connecting === "google" ? "Connecting..." : "Connect"}
                    </button>
                  )}
                </div>

                {/* GitHub card */}
                <div className="setup-provider setup-provider--large">
                  <div className="setup-provider-icon setup-provider-icon--github">
                    <svg width="24" height="24" viewBox="0 0 24 24" fill="currentColor">
                      <path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0024 12c0-6.63-5.37-12-12-12z"/>
                    </svg>
                  </div>
                  <div className="setup-provider-info">
                    <h3>GitHub</h3>
                    <p>Repos · Pull Requests</p>
                  </div>
                  {oauthStatus.github?.connected ? (
                    <span className="setup-badge setup-badge--ok">Connected</span>
                  ) : (
                    <button className="setup-btn setup-btn--primary setup-btn--small" disabled={connecting !== null} onClick={() => handleConnect("github")}>
                      {connecting === "github" ? "Connecting..." : "Connect"}
                    </button>
                  )}
                </div>
              </section>
            </>
          )}
        </div>

      {/* ── Footer navigation ── */}
      <div className="setup-footer">
        {step > 0 ? (
          <button className="setup-btn" onClick={() => setStep((step - 1) as Step)}>
            ← Back
          </button>
        ) : (
          <div />
        )}
        <div style={{ display: "flex", alignItems: "center", gap: "var(--nx-space-3)" }}>
          {saved && <span className="setup-saved">Ready!</span>}
          {step < 2 ? (
            <button
              className="setup-btn setup-btn--primary"
              onClick={async () => {
                await saveAllSettings();
                setStep((step + 1) as Step);
              }}
            >
              Continue →
            </button>
          ) : (
            <button className="setup-btn setup-btn--primary" style={{ padding: "10px 24px" }} onClick={handleFinish}>
              🚀 Launch Assistant
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function StepHeader({ step }: { step: Step }) {
  return (
    <div className="setup-step-header">
      <div className="setup-step-indicator">
        {STEP_LABELS.map((label, i) => (
          <div key={label} style={{ display: "flex", alignItems: "center", gap: "var(--nx-space-2)", flex: i < 2 ? 1 : undefined }}>
            <div className={`setup-step-dot ${i === step ? "setup-step-dot--active" : ""} ${i < step ? "setup-step-dot--completed" : ""}`} />
            {i < 2 && <div className={`setup-step-bar ${i < step ? "setup-step-bar--completed" : ""}`} />}
          </div>
        ))}
      </div>
      <div className="setup-step-label">Step {step + 1} of 3</div>
      <div className="setup-step-title">{STEP_LABELS[step]}</div>
    </div>
  );
}
