import { useEffect, useRef, useState } from "react";
import { Avatar } from "../avatar/Avatar";
import { useAssistant } from "../store/assistant";
import { initOrchestratorListener, hideOrbAfterSpeech } from "../net/orchestrator";
import { initOrbRuntime } from "./orbRuntime";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

async function tauriInvoke(cmd: string, args?: Record<string, unknown>): Promise<any> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(cmd, args);
}

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * OrbFrame — the voice orb's home inside the always-on `stage` overlay
 * (Single-Stage Shell step 2). Replaces the retired `main` OS window: the
 * orb's screen position comes from Rust's `stage:orb_rect` event (physical
 * px, same convention as `ghost:ring`/`stage:spatial_annotations` —
 * divide by devicePixelRatio for CSS px) instead of a native window move,
 * and visibility is a plain conditional render instead of `show()`/`hide()`.
 *
 * No-slide entrance (plan §1.3): there is deliberately no CSS transform
 * here for showing/hiding — the orb's own particle assemble()/disperse()
 * (inside VoiceOrb/Avatar) IS the entrance/exit now, for every wake, not
 * just ghost sessions.
 */
export function OrbFrame() {
  const assistantVisible = useAssistant((s) => s.visible);
  const state = useAssistant((s) => s.state);
  const loadingVisible = useAssistant((s) => s.loadingVisible);
  const ghostActive = useAssistant((s) => s.ghostActive);
  const orbPosition = useAssistant((s) => s.orbPosition || "top");

  const [rect, setRect] = useState<OrbRect | null>(null);
  // Calibration can force the orb hidden (Loading target active) or shown
  // (Wakeup/Waves target active) independent of the normal voice `visible`
  // flag — see calibration.rs's apply_main/calibration_set_target. `null`
  // means "no override — follow assistantVisible".
  const [stageOverride, setStageOverride] = useState<boolean | null>(null);
  const visible = stageOverride ?? assistantVisible;
  const isShown = Boolean(visible || ghostActive);

  // One-time bootstrap: voice runtime + orchestrator/tts-activity listeners
  // (previously wired in main.tsx / App.tsx's mount effect).
  useEffect(() => {
    initOrbRuntime();
    void initOrchestratorListener();
    void import("../audio/ttsActivity").then(({ initTtsActivityListener }) =>
      initTtsActivityListener(),
    );
  }, []);

  // Orb rect (position/size), physical px from Rust → CSS px here. Pulls
  // the pending value on mount (race-free: an emit before this effect
  // subscribes would otherwise be lost — same pattern as the codebase's
  // other dynamically-created-window pending-content commands).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<OrbRect>("stage:orb_rect", (event) => {
        if (event.payload) setRect(event.payload);
      });
      try {
        const pending = await tauriInvoke("get_pending_orb_rect");
        if (pending) setRect(pending as OrbRect);
      } catch {
        /* non-Tauri (tests) — no-op */
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Calibration state → visibility override + target/size mirror (moved
  // from App.tsx's "calibration:state" listener).
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{
        active?: boolean;
        target?: "wakeup" | "waves" | "loading";
        size?: number;
      }>("calibration:state", (event) => {
        const p = event.payload ?? {};
        const st = useAssistant.getState();
        if (p.active && p.target && p.target !== "loading") {
          if (st.calibrationTarget !== p.target) st.bumpCalibrationPulse();
          st.setCalibration(p.target, p.size ?? null);
          setStageOverride(true);
          st.setState("idle");
        } else if (p.active && p.target === "loading") {
          st.setCalibration(p.target, p.size ?? null);
          setStageOverride(false);
        } else if (p.active === false) {
          st.setCalibration(null, null);
          setStageOverride(null);
        }
      });
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Explicit show/hide from Rust (wake, Tier-3 command detection,
  // calibration preview) — see window_manager.rs's show_orb_interactive/
  // wake_orb and calibration.rs's apply_main. Clears any stale calibration
  // override so a normal wake always wins.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<boolean>("stage:orb_visible", (event) => {
        if (useAssistant.getState().calibrationTarget) {
          setStageOverride(event.payload);
        } else {
          setStageOverride(null);
          if (event.payload) useAssistant.getState().setVisible(true);
        }
      });
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Stage watchdog notices (spoken alerts outside the orchestrator
  // handshake — see stage.rs's inform_via_orb).
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ text?: string }>("stage:notice", (event) => {
        const text = event.payload?.text;
        if (!text) return;
        const s = useAssistant.getState();
        s.setVisible(true);
        s.setState("speaking");
        s.addAssistantMessage(text);
        import("../audio/ttsPlayer")
          .then(({ speak }) =>
            speak(text, () => {
              useAssistant.getState().setVisible(true);
              setTimeout(() => { void import("../net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
            }),
          )
          .catch(() => {
            void import("../net/ghostHotMic")
              .then((m) => m.endGhostTurn())
              .catch(() => useAssistant.getState().reset());
          });
      }).then((fn) => { unlisten = fn; }),
    );
    return () => unlisten?.();
  }, []);

  // Ghost session event → orb ghost mode (waves). See App.tsx's original
  // comment: session-scoped, fires on enter/exit/stand-down only.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void import("@tauri-apps/api/event").then(({ listen }) =>
      listen<{ active?: boolean }>("ghost:session", (event) => {
        const s = useAssistant.getState();
        const v = event.payload?.active === true;
        if (s.ghostActive !== v) {
          s.setGhostActive(v);
          s.setVisible(true);
          if (!v) s.setState("idle");
          console.log(`[ORB] ghost:session active=${v} → visible=true state=${v ? s.state : "idle"}`);
          // Stall watchdog lifecycle: re-arms capture when a ghost turn
          // never completes (hung backend, lost result event). Started
          // on enter, stopped on exit — never runs outside sessions.
          void import("../net/ghostHotMic").then((m) => {
            if (v) m.startGhostWatchdog();
            else m.stopGhostWatchdog();
          }).catch(() => {});
          if (!v) hideOrbAfterSpeech(3000);
        }
      }).then((fn) => { unlisten = fn; }),
    );
    // Mount-time sync: check if a ghost session was already armed before mount
    void tauriInvoke("get_pending_ghost_session")
      .then((active) => {
        if (active === true) {
          const s = useAssistant.getState();
          s.setGhostActive(true);
          s.setVisible(true);
          console.log("[ORB] get_pending_ghost_session active=true → visible=true");
          void import("../net/ghostHotMic").then((m) => {
            m.startGhostWatchdog();
          }).catch(() => {});
        }
      })
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  // Sentinel alerts (tracking-first, no speech/orb changes here).
  useEffect(() => {
    if (!isTauri()) return;
    void import("../store/sentinel").then(({ initSentinelAlertListener }) =>
      initSentinelAlertListener().catch(() => {}),
    );
  }, []);

  // Observability: every visible transition, one line.
  useEffect(() => {
    const s = useAssistant.getState();
    console.log(`[ORB] visible=${s.visible} state=${s.state} ghost=${s.ghostActive}`);
  }, [assistantVisible]);

  // No-input timeout: 8s of `listening` with no voice detected and the
  // orb hides itself (Rust owns the turn end if voice IS underway).
  useEffect(() => {
    if (!visible || state !== "listening" || ghostActive) return;
    const t = setTimeout(() => {
      void import("@tauri-apps/api/core").then(async ({ invoke }) => {
        try {
          const hadSpeech = await invoke<boolean>("stt_capture_had_speech");
          if (hadSpeech) {
            console.log("[NEXUS] no-input timeout: voice underway, Rust owns the turn end");
            return;
          }
        } catch {
          /* IPC failed — fall through to the legacy local abort below */
        }
        import("../audio/vad").then(({ stopVad }) => stopVad()).catch(() => {});
        import("../audio/recorder").then(({ abortCapture }) => {
          void abortCapture().catch(() => {});
        }).catch(() => {});
        useAssistant.getState().setVisible(false);
        setTimeout(() => useAssistant.getState().reset(), 300);
      }).catch(() => {});
    }, 8000);
    return () => clearTimeout(t);
  }, [visible, state, ghostActive]);

  // Speaking failsafe: force a reset if 60s pass in `speaking` with no
  // audio actually playing (genuinely long replies re-arm instead).
  useEffect(() => {
    if (state !== "speaking") return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const arm = () => {
      timer = setTimeout(() => {
        timer = null;
        if (cancelled || useAssistant.getState().state !== "speaking") return;
        import("../audio/ttsPlayer")
          .then(({ isRustTtsPlaying }) => {
            if (cancelled || useAssistant.getState().state !== "speaking") return;
            if (isRustTtsPlaying()) {
              arm();
              return;
            }
            console.warn("[NEXUS] speaking failsafe: silent 60s, forcing reset");
            const s = useAssistant.getState();
            s.setLoadingVisible(false);
            s.setVisible(true);
            setTimeout(() => { void import("../net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
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

  // Orb interactivity: when the state is active (not idle), the orb's
  // rect becomes a stage hitbox so pointer events reach it (replaces the
  // old `set_click_through` call on the `main` window).
  useEffect(() => {
    if (state === "idle" && !useAssistant.getState().calibrationTarget) {
      tauriInvoke("set_orb_interactive", { interactive: false }).catch(() => {});
      return;
    }
    tauriInvoke("set_orb_interactive", { interactive: true }).catch(() => {});
  }, [state]);

  // Loading indicator visibility (separate stage-hosted component —
  // LoadingIndicator.tsx listens for the same events independently; this
  // just drives the Rust-side rect/visibility emit).
  useEffect(() => {
    if (loadingVisible && !ghostActive) {
      tauriInvoke("orchestrator_show_loading").catch(() => {});
    } else {
      tauriInvoke("orchestrator_hide_loading").catch(() => {});
    }
  }, [loadingVisible, ghostActive]);

  // Particle entrance/exit (no-slide, plan §1.3): assemble() on every
  // false→true, disperse() on every true→false. Ghost-mode's own exit
  // stays sphere-to-sphere (handled inside VoiceOrb/Avatar's `entered`
  // prop wiring — this just tracks the transition edge).
  const wasVisibleRef = useRef(false);
  const [entered, setEntered] = useState(false);
  const [dispersing, setDispersing] = useState(false);
  useEffect(() => {
    const was = wasVisibleRef.current;
    wasVisibleRef.current = visible;
    if (!was && visible) {
      setDispersing(false);
      setEntered(true);
      // Reset the one-shot trigger next tick so a later show re-fires it.
      const t = setTimeout(() => setEntered(false), 50);
      return () => clearTimeout(t);
    }
    if (was && !visible && !ghostActive) {
      setDispersing(true);
      const t = setTimeout(() => setDispersing(false), 950);
      return () => clearTimeout(t);
    }
  }, [visible, ghostActive]);

  // Positioning is applied via DIRECT DOM mutation (ref callback + effect),
  // never React's `style` prop: the stage window's CSP injects a per-load
  // nonce into style-src for Tauri's own IPC init script, which per the
  // CSP spec makes 'unsafe-inline' inert for the WHOLE directive — silently
  // dropping every React-rendered inline style and collapsing this div to
  // zero size (live bug, 2026-10-04: orb invisible, captions still showed
  // since text has non-zero intrinsic height even unpositioned). The
  // existing ghost-ring/ghost-pointer elements elsewhere in this same file
  // already use this exact ref + `el.style.x = ...` pattern and are
  // unaffected — direct CSSOM property assignment isn't inline-style
  // text parsing, so it isn't subject to this restriction.
  const hasPositionedRef = useRef(false);
  const applyRect = (el: HTMLDivElement | null, r: OrbRect | null, shown: boolean, pos: string) => {
    if (!el || !r) return;
    const dpr = window.devicePixelRatio || 1;
    const wCss = r.w / dpr;
    const hCss = r.h / dpr;
    const xCss = r.x / dpr;
    const yCss = r.y / dpr;

    el.style.position = "fixed";
    el.style.left = `${xCss}px`;
    el.style.top = "0";
    el.style.width = `${wCss}px`;
    el.style.height = `${hCss}px`;

    const targetY =
      pos === "top"
        ? (shown ? yCss : -hCss - 24)
        : (shown ? yCss : (typeof window !== "undefined" ? window.innerHeight : 1080) + 24);

    if (!hasPositionedRef.current) {
      // First mount: instant snap to target without flying from (0, 0)
      el.style.transition = "none";
      el.style.transform = `translate3d(0, ${targetY}px, 0)`;
      el.style.opacity = shown ? "1" : "0";
      el.style.pointerEvents = shown ? "auto" : "none";
      hasPositionedRef.current = true;
      requestAnimationFrame(() => {
        if (el) {
          el.style.transition = "transform 0.72s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.5s ease";
        }
      });
      return;
    }

    el.style.transition = "transform 0.72s cubic-bezier(0.22, 1, 0.36, 1), opacity 0.5s ease";
    el.style.transform = `translate3d(0, ${targetY}px, 0)`;
    el.style.opacity = shown ? "1" : "0";
    el.style.pointerEvents = shown ? "auto" : "none";
  };
  const frameRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    console.log(`[ORB-FRAME] state: isShown=${isShown} pos=${orbPosition} (assistantVisible=${assistantVisible}, ghost=${ghostActive}, stageOverride=${stageOverride})`);
    applyRect(frameRef.current, rect, isShown, orbPosition);
  }, [rect, isShown, orbPosition, assistantVisible, ghostActive, stageOverride]);

  if (!rect) return null;

  return (
    <div
      id="orb-frame"
      data-interactive={isShown ? "true" : undefined}
      className={`orb-slider orb-slider--${orbPosition}`}
      ref={(el) => {
        frameRef.current = el;
        applyRect(el, rect, isShown, orbPosition);
      }}
    >
      <div className="orb-capsule">
        <div className="orb-sphere-wrapper">
          <Avatar entered={entered} dispersing={dispersing} />
        </div>
      </div>
    </div>
  );
}
