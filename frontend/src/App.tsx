import { useEffect, useRef } from "react";
import { Avatar } from "./avatar/Avatar";
import { useAssistant } from "./store/assistant";
import { initOrchestratorListener, hideOrbAfterSpeech } from "./net/orchestrator";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<any> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(cmd, args);
}

export default function App() {
  const state = useAssistant((s) => s.state);
  const visible = useAssistant((s) => s.visible);
  const loadingVisible = useAssistant((s) => s.loadingVisible);
  const ghostActive = useAssistant((s) => s.ghostActive);

  // Initialize the central orchestrator event listener (once).
  // This listens to "orchestrator:event" from Rust and handles:
  //   - state transitions (thinking → speaking)
  //   - loading indicator visibility
  //   - ack TTS ("On it sir")
  //   - result TTS + sidebar display
  //   - error handling
  useEffect(() => {
    void initOrchestratorListener();
    void import("./audio/ttsActivity").then(({ initTtsActivityListener }) =>
      initTtsActivityListener(),
    );
    // Stage watchdog notices: spoken alerts that must never touch the
    // orchestrator handshake (no request id, no long-running flag), so a
    // notice arriving mid-turn can't orphan a real turn's `done` reset.
    if (isTauri()) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen<{ text?: string }>("stage:notice", (event) => {
          const text = event.payload?.text;
          if (!text) return;
          const s = useAssistant.getState();
          s.setVisible(true);
          s.setState("speaking");
          s.addAssistantMessage(text);
          import("./audio/ttsPlayer")
            .then(({ speak }) =>
              speak(text, () => {
                useAssistant.getState().setVisible(true);
                // Ghost-aware turn end (no-op outside ghost mode).
                setTimeout(() => { void import("./net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
              }),
            )
            .catch(() => {
              // Speak path failed — still end the turn ghost-aware, with a
              // plain reset as the last-resort fallback.
              void import("./net/ghostHotMic")
                .then((m) => m.endGhostTurn())
                .catch(() => useAssistant.getState().reset());
            });
        }),
      );
    }
    // Ghost session event → orb ghost mode (waves). Session-scoped —
    // fires on enter/exit/stand-down only, never per turn, so waves
    // survive turn resets and die exactly when the session ends.
    // Visibility is explicit both ways (the store no longer couples it):
    // enter shows the orb, exit lands on the VISIBLE normal-mode orb
    // (idle smile) — an exit must never end invisible.
    // Exit also arms the state-aware hide (the exit turn emits no `ack`,
    // so no other hide is ever armed for it — without this the wakeup
    // orb sits visible forever). hideOrbAfterSpeech re-arms while the
    // exit line is still speaking and no-ops if the user re-wakes, so
    // the delay here only sets the first check, never the hide itself.
    if (isTauri()) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen<{ active?: boolean }>("ghost:session", (event) => {
          const s = useAssistant.getState();
          const v = event.payload?.active === true;
          if (s.ghostActive !== v) {
            s.setGhostActive(v);
            s.setVisible(true);
            if (!v) s.setState("idle");
            console.log(`[ORB] ghost:session active=${v} → visible=true state=${v ? s.state : "idle"}`);
            if (!v) hideOrbAfterSpeech(3000);
          }
        }),
      );
    }
    // Animation calibration session — the Command Hub drives it from a
    // separate window; this (orb) window mirrors the active target and
    // size so the preview + px badge render live. Cleans up on end.
    if (isTauri()) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen<{
          active?: boolean;
          target?: "wakeup" | "waves" | "loading";
          size?: number;
        }>("calibration:state", (event) => {
          const p = event.payload ?? {};
          const st = useAssistant.getState();
          if (p.active && p.target && p.target !== "loading") {
            // Plan 04 §4: pulse on every target switch so identical rects
            // still read as switched (the window may not move at all).
            if (st.calibrationTarget !== p.target) st.bumpCalibrationPulse();
            st.setCalibration(p.target, p.size ?? null);
            st.setVisible(true);
            st.setState("idle");
          } else if (p.active === false) {
            st.setCalibration(null, null);
            st.setVisible(false);
          }
        }),
      );
    }
    // Ghost ring position (AI driving only) — display-only; the ring
    // element lives in the stage window. The orb no longer reads this.
    if (isTauri()) {
      void import("@tauri-apps/api/event").then(({ listen }) =>
        listen("ghost:ring", () => {}),
      );
    }
    // Sentinel alerts (tracking-first): the Gmail sentinel emits
    // `orchestrator:sentinel-alert` per detected change; the sentinel
    // store observes them for tracking/debugging. No speech, no orb
    // changes here — the landing animation is a later phase.
    if (isTauri()) {
      void import("./store/sentinel").then(({ initSentinelAlertListener }) =>
        initSentinelAlertListener().catch(() => {}),
      );
    }
  }, []);

  // Orb visibility transitions — single observability point for every
  // show/hide across all 30+ call sites (recorder turn ends, wake,
  // ghost session, auto-hide). If the orb vanishes unexpectedly, this
  // line names the exact state it vanished with.
  useEffect(() => {
    const s = useAssistant.getState();
    console.log(`[ORB] visible=${s.visible} state=${s.state} ghost=${s.ghostActive}`);
  }, [visible]);
  // Also cleans up VAD + recording + mic stream to avoid orphaned AudioContexts.
  // Delay reset() until after the slide-down completes so the Lottie doesn't
  // switch segments mid-slide (which would cause a visual glitch).
  // Single-owner endpoint (approach B): the Rust capture owns the turn end.
  // This 8s timer only hides the orb when NO voice is underway (checked
  // non-destructively — a destructive stop here would amputate slow
  // starters, whose late transcripts still succeed through the normal
  // flow today). Voice underway → hands off entirely; Rust endpoints the
  // turn itself (≤10s hard cap) and the transcript flow hides the orb.
  useEffect(() => {
    if (!visible || state !== "listening" || ghostActive) return;
    const t = setTimeout(() => {
      void import("@tauri-apps/api/core").then(async ({ invoke }) => {
        try {
          const hadSpeech = await invoke<boolean>("stt_capture_had_speech");
          if (hadSpeech) {
            console.log("[NEXUS] no-input timeout: voice underway, Rust owns the turn end");
            return; // Do NOT hide — the turn is live; normal flow continues.
          }
        } catch {
          // IPC failed — fall through to the legacy local abort below.
        }
        // Stop VAD + recording + mic stream before hiding.
        import("./audio/vad").then(({ stopVad }) => stopVad()).catch(() => {});
        import("./audio/recorder").then(({ abortCapture }) => {
          void abortCapture().catch(() => {});
        }).catch(() => {});
        useAssistant.getState().setVisible(false);
        // Delay state reset until the 0.5s slide-down finishes.
        // turn-end:keep-raw (no-input timeout — abortCapture already ran)
        setTimeout(() => useAssistant.getState().reset(), 550);
      }).catch(() => {});
    }, 8000);
    return () => clearTimeout(t);
  }, [visible, state]);

  // Speaking failsafe: if TTS ends (or dies) without firing onEnd, the
  // orb would park in `speaking` with the loading-loop animation forever.
  // If 60s pass in `speaking` with no audio actually playing, force the
  // same reset the `done` handler performs. Genuinely long replies (audio
  // still playing) re-arm instead of cutting.
  useEffect(() => {
    if (state !== "speaking") return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const arm = () => {
      timer = setTimeout(() => {
        timer = null;
        if (cancelled || useAssistant.getState().state !== "speaking") return;
        import("./audio/ttsPlayer")
          .then(({ isRustTtsPlaying }) => {
            if (cancelled || useAssistant.getState().state !== "speaking") return;
            if (isRustTtsPlaying()) {
              arm(); // long reply still playing — wait another 60s
              return;
            }
            console.warn("[NEXUS] speaking failsafe: silent 60s, forcing reset");
            const s = useAssistant.getState();
            s.setLoadingVisible(false);
            s.setVisible(true);
            // Ghost-aware: a wedged turn mid-session must not deafen the loop.
            setTimeout(() => { void import("./net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
          })
          .catch(() => {});
      }, 60000);
    };
    arm();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [state]);
  // Native orb window visibility — only depends on `visible` (the orb).
  // The loading animation is now in a SEPARATE Tauri window, so hiding the
  // orb window does NOT affect the loading window.
  const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (visible) {
      if (hideTimerRef.current) {
        clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
      tauriInvoke("show_overlay").catch(() => {});
    } else {
      hideTimerRef.current = setTimeout(() => {
        tauriInvoke("hide_overlay").catch(() => {});
        hideTimerRef.current = null;
      }, 600);
    }
    return () => {
      if (hideTimerRef.current) {
        clearTimeout(hideTimerRef.current);
        hideTimerRef.current = null;
      }
    };
  }, [visible]);

  // When state is active (not idle), ensure click-through is OFF.
  useEffect(() => {
    if (state === "idle") return;
    tauriInvoke("set_click_through", { ignore: false }).catch(() => {});
  }, [state]);

  // Loading window management — show/hide a separate Tauri window at the
  // top-right corner of the screen. This window contains the Lottie loading
  // animation and is completely independent from the orb window.
  useEffect(() => {
    if (loadingVisible && !ghostActive) {
      console.log("[NEXUS] loading: showing loading window at top-right corner");
      tauriInvoke("show_loading_indicator").catch((e) =>
        console.warn("[NEXUS] loading: show_loading_indicator failed:", e)
      );
    } else {
      console.log("[NEXUS] loading: hiding loading window");
      tauriInvoke("hide_loading_indicator").catch((e) =>
        console.warn("[NEXUS] loading: hide_loading_indicator failed:", e)
      );
    }
  }, [loadingVisible, ghostActive]);

  // Cleanup: destroy the loading window when the App unmounts
  useEffect(() => {
    return () => {
      tauriInvoke("hide_loading_indicator").catch(() => {});
    };
  }, []);

  return (
    <div id="app" className={`${visible ? "app--visible" : "app--hidden"}${ghostActive ? " app--ghost" : ""}`}>
      <div className="avatar-section" data-interactive>
        <Avatar />
      </div>
    </div>
  );
}
