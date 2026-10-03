import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// ─── HOT MIC + PRE-INIT VAD (Approach A+B) ─────────────────────────────────
// At startup, we pre-acquire the mic stream and pre-initialize the Silero
// MicVAD instance. This eliminates the two biggest sources of wake-to-listen
// delay:
//   1. getUserMedia() — 50-200ms per wake → eliminated (mic stays warm)
//   2. MicVAD.new()   — 60-250ms per wake → eliminated (VAD stays paused, ready)
//
// On wake, we just resume the VAD + start recording in parallel (Approach C).
// Wake-to-listen drops from ~200-500ms to ~10-50ms.
//
// Privacy: Audio is ALWAYS processed locally. The mic stream stays open but
// audio is only captured when recording is active. VAD runs only during
// listening state. No audio leaves the device.

import { preloadSileroVad, preloadMicVad, stopVad } from "./audio/vad";
import { abortCapture, processTranscript } from "./audio/recorder";
import { stopTts, isRustTtsPlaying } from "./audio/ttsPlayer";
import { useAssistant } from "./store/assistant";
import { setBargedIn, clearBargedIn, clearDialogContext } from "./net/wsBridge";

/** Set to true when the wake is triggered automatically by a follow-up
 * question (not by the user pressing Ctrl+Space). When true, startListening
 * does NOT set the bargedIn flag — the follow-up response should be heard. */
let autoReopenFromFollowup = false;

/** Called by wsBridge when a follow-up question finishes speaking and
 *  the mic should auto-reopen without requiring the user to press Ctrl+Space.
 *  Also the ghost hot-mic loop's listen entry. Showing the orb is NOT
 *  enough: the Rust cpal capture must be started explicitly, or the orb
 *  shows "listening" while Rust captures nothing (live bug, doc 44 —
 *  the confirm window already did this; the shared path didn't). */
export function triggerFollowupListen(): void {
  autoReopenFromFollowup = true;
  const w = window as any;
  if (w.__NEXUS_WAKE__) {
    w.__NEXUS_WAKE__();
  }
  import("@tauri-apps/api/core")
    .then(({ invoke }) => invoke("start_stt_capture"))
    .catch((e) => {
      console.warn("[NEXUS] followup listen: Rust capture not started:", e);
    });
}

let micStream: MediaStream | null = null;

// ─── No-speech watchdog ───────────────────────────────────────────────────
// NOTE: The no-speech watchdog is now handled on the Rust side.
// The Rust STT capture has a built-in timeout (STT_NO_SPEECH_CHUNK_LIMIT = 100
// chunks = ~8s). If no speech is detected, it stops capturing and emits an
// empty transcript, which triggers the "didn't catch that" retry logic.

/**
 * Acquire the mic stream at startup and keep it warm.
 * This eliminates the 50-200ms getUserMedia() latency on every wake.
 *
 * If this fails (e.g. mic permission not yet granted), we fall back to
 * acquiring the mic on the first wake — the old behavior.
 */
async function warmMic(): Promise<void> {
  try {
    micStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
      },
    });
    console.log("[NEXUS] hot mic: stream acquired and warming");

    // Pre-initialize MicVAD with the warm stream.
    // This creates the AudioWorklet + loads the Silero model.
    // The VAD starts in paused state — it won't process audio until startVad().
    await preloadMicVad(micStream);
    console.log("[NEXUS] hot mic: MicVAD pre-initialized and ready");
  } catch (err) {
    console.warn("[NEXUS] hot mic: startup mic acquisition failed, will acquire on wake:", err);
    micStream = null;
  }
}

// Start the hot mic + VAD preload at app startup (non-blocking)
// NOTE: warmMic() is DISABLED at startup because it opens getUserMedia() via
// WebView2, which conflicts with the Rust cpal wake-word stream on some audio
// drivers (Intel Smart Sound Technology). The cpal stream gets silence when
// WebView2 is also capturing from the same mic.
// The frontend will acquire the mic on first wake via startListening().
preloadSileroVad()
  .then(() => {
    console.log("[NEXUS] Silero VAD model pre-loaded (mic NOT acquired — cpal has exclusive access)");
  })
  .catch(() => {
    console.warn("[NEXUS] Silero VAD pre-load failed at startup — will use RMS fallback");
  });

// warmMic is kept for future use but not called at startup to avoid mic conflict.
void warmMic;

/**
 * Wake-word / hotkey → mic capture → VAD → STT → intent → execute / backend.
 *
 * With hot mic + pre-init VAD, the wake-to-listen path is:
 *   1. Wake fires → show overlay, set state to "listening"
 *   2. Mic stream already warm → skip getUserMedia (saves 50-200ms)
 *   3. Start recording + start VAD in PARALLEL (saves 60-250ms)
 *   4. VAD detects silence → finishCapture() → STT → intent → execute
 *
 * Total wake-to-listen: ~10-50ms (down from ~200-500ms)
 */

async function startListening() {
  const s = useAssistant.getState();
  console.log("[NEXUS] wake →", s.state);

  // Second Ctrl+Space press while listening = cancel the call.
  // Ask Rust to abort the cpal capture: no voice yet → hide the orb
  // outright; speech underway → let the in-flight turn finish (never
  // kill mid-word) and drop any late transcript via the session guard.
  if (s.state === "listening") {
    console.log("[NEXUS] second press while listening → aborting capture");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const res = await invoke<{ had_speech: boolean; elapsed_ms: number }>(
        "stop_stt_capture"
      );
      if (!res.had_speech) {
        await abortCapture().catch(() => {});
        useAssistant.getState().reset();
        useAssistant.getState().setVisible(false);
      }
    } catch (err) {
      console.warn("[NEXUS] stop_stt_capture failed:", err);
    }
    return;
  }

  // Alexa-style barge-in: ALWAYS cut audio on wake, not only when the
  // frontend believes it is speaking. stopTts()/stopVad() are idempotent
  // (generation bump + stateless rodio stop), so stopping silence is
  // free — while a missed stop talks over the new turn (state drift via
  // early onEnd, failsafe reset, or generation skew). Turn teardown
  // below still branches on prior state (abortCapture vs session keep).
  stopTts();
  stopVad();

  // If NEXUS is speaking or thinking, cancel the current turn before
  // starting a new one. This prevents the TTS 'interrupted' error and
  // ensures clean state transitions.
  //
  // EXCEPTION: If a long-running query is in flight (PR analysis etc),
  // do NOT close the session — the HTTP request to the Worker is already
  // in progress and can't be cancelled. The dedup/queue logic in
  // recorder.ts handles the new command instead.
  const wasSpeaking = s.state === "speaking";
  const isAutoReopen = autoReopenFromFollowup;
  autoReopenFromFollowup = false; // consume the flag
  const { isLongRunningInFlight } = await import("./net/wsBridge");
  const longRunningActive = isLongRunningInFlight();
  if (wasSpeaking || s.state === "thinking") {
    // If this is an auto-reopen from a follow-up question, do NOT set bargedIn —
    // the follow-up response should be heard, not discarded.
    if (!isAutoReopen) {
      setBargedIn();
      // User barge-in: clear any pending dialog context — they're starting fresh
      clearDialogContext();
    } else {
      console.log("[NEXUS] auto-reopen from follow-up — NOT setting bargedIn");
      clearBargedIn();
    }
    if (longRunningActive) {
      console.log("[NEXUS] barge-in: long-running query in flight — NOT closing session (dedup/queue will handle)");
      stopTts();
      stopVad();
      // Don't call abortCapture — it would closeSession and break the in-flight request
    } else {
      console.log("[NEXUS] barge-in/turn-transition: cancelling current turn");
      stopTts();
      stopVad();
      await abortCapture().catch(() => {});
    }
    if (wasSpeaking && !isAutoReopen) {
      // Dual-Phase Post-TTS Mute Gate: 300ms delay to allow DAC audio buffers and room acoustics to clear
      await new Promise((r) => setTimeout(r, 300));
    }
  } else {
    // Fresh wake (no barge-in) — clear the flag for the new turn
    clearBargedIn();
  }

  // Hide the loading indicator — the orb is showing again, so the
  // loading animation at the top-right corner is no longer needed.
  // This covers the barge-in case where the user presses Ctrl+Space
  // while a long-running query is in flight and the loading indicator
  // is still visible.
  useAssistant.getState().setLoadingVisible(false);
  // New turn: never inherit the previous turn's "waiting" glow.
  useAssistant.getState().setAwaitingInput(false);
  s.setVisible(true);
  s.setState("listening");
  // Clear barge-in flag now that we're starting a fresh listening session
  clearBargedIn();

  // ─── Rust-side STT capture ────────────────────────────────────────
  // The Rust cpal stream (which detected the wake word) captures audio
  // directly — no getUserMedia, no baton pass, no frontend audio processing.
  // This fixes the Intel SST driver issue where getUserMedia returns silence
  // but the cpal stream is still working.
  //
  // The Rust side:
  //   1. Buffers 16kHz audio from the cpal callback
  //   2. RMS-based VAD detects speech + silence
  //   3. On silence, transcribes (Groq or local faster-whisper)
  //   4. Emits "stt:transcript" event with the transcript text
  //
  // The frontend just shows the orb and waits for the event.
  // No getUserMedia, no VAD, no ScriptProcessorNode, no baton pass.
  console.log("[NEXUS] Rust-side STT capture active — waiting for stt:transcript event");
}

/** Called from Rust on wake (hotkey, spoken "NEXUS", or tray click). */
(window as any).__NEXUS_WAKE__ = () => {
  console.log("[NEXUS] __NEXUS_WAKE__ invoked");
  void wakeWithGreeting();
};

/**
 * Called from Rust when the first-run setup wizard completes.
 * Speaks the first-run greeting: "NEXUS online, sir. Ready when you are."
 * Shows the orb briefly, then hides it. Does NOT transition to listening.
 *
 * After the greeting, warms up the mic stream so the first wake is fast.
 * This is safe because the setup wizard already verified mic permission.
 */
(window as any).__NEXUS_FIRST_RUN_GREETING__ = async () => {
  console.log("[NEXUS] __NEXUS_FIRST_RUN_GREETING__ invoked");
  const { useAssistant } = await import("./store/assistant");
  const { speak } = await import("./audio/ttsPlayer");

  const s = useAssistant.getState();
  s.setVisible(true);
  s.setState("speaking");
  s.addAssistantMessage("NEXUS online, sir. Ready when you are.");

  void speak("NEXUS online, sir. Ready when you are.").then(() => {
    console.log("[NEXUS] first-run greeting done — hiding orb");
    s.setState("idle");
    s.setVisible(false);
    // turn-end:keep-raw (first-run greeting — no session exists yet)
    setTimeout(() => s.reset(), 550);
  });

  // Warm up the mic now that permission has been granted during setup.
  // This is safe because the setup wizard's Permissions step verified
  // getUserMedia works. The mic stream will be reused on first wake.
  // NOTE: We wait 2s after the greeting starts so the TTS audio doesn't
  // interfere with the mic warm-up on Intel SST drivers.
  setTimeout(() => {
    warmMic().catch((e) => console.warn("[NEXUS] post-setup warmMic failed:", e));
  }, 2000);
};

// Tauri IPC wake events are NOT listened to here anymore.
// The Rust side calls window.__NEXUS_WAKE__() directly via eval(),
// which is more reliable than the event system for repeated rapid events.
// Listening to both caused wakeWithGreeting() to fire 2-3x, resulting in
// "on it sir" being spoken twice.

/**
 * Wake handler — goes straight to listening.
 *
 * The daily greeting has been removed. NEXUS simply shows the orb and
 * starts listening immediately when the hotkey is pressed.
 */
async function wakeWithGreeting() {
  void startListening();
}

/**
 * Tier 3: Direct command detection listener.
 *
 * When a command classifier fires in the OWW pipeline (e.g. "open youtube"),
 * Rust emits a `command-detected` Tauri event with the structured intent.
 * The frontend skips STT entirely and executes the intent directly —
 * no Whisper, no transcript, no 27-second delay.
 *
 * This is the fast path: ~200ms from speech to action.
 * The STT path remains as fallback for commands not covered by classifiers.
 */
async function setupCommandDetectionListener() {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<{ action: string; target: string; needs_param?: boolean }>("command-detected", async (event) => {
      const intent = event.payload;
      console.log(`[NEXUS] Tier 3 command detected: ${intent.action} (needs_param=${intent.needs_param ?? false})`);

      const { useAssistant } = await import("./store/assistant");
      const { speak } = await import("./audio/ttsPlayer");

      // Show the overlay and set state to speaking
      const s = useAssistant.getState();
      s.setVisible(true);
      s.setState("speaking");

      // ─── Type 2: Parameterized command ─────────────────────────────
      // The acoustic classifier detected the command PATTERN (e.g. "play ... in spotify").
      // Now we need to capture the PARAMETER (e.g. song name) via STT.
      // Flow: speak "On it sir" → record 3s → STT → execute with parameter
      if (intent.needs_param) {
        s.addUserMessage(`${intent.action.replace(/_/g, " ")}...`);
        s.addAssistantMessage("On it sir");
        void speak("On it sir");

        // Wait for TTS to finish before recording (so we don't capture TTS audio).
        // Primary TTS is Rust-side (rodio): speechSynthesis.speaking is only
        // ever true on the web-speech fallback, so the Rust flag is checked
        // first — checking speechSynthesis alone resolved immediately while
        // the prompt was still playing (audit H2).
        await new Promise<void>((resolve) => {
          const idle = () =>
            !isRustTtsPlaying() &&
            (typeof speechSynthesis === "undefined" || !speechSynthesis.speaking);
          if (idle()) {
            resolve();
            return;
          }
          const check = () => {
            if (idle()) {
              resolve();
              return;
            }
            setTimeout(check, 100);
          };
          setTimeout(check, 100);
        });

        // Record 3 seconds of audio for the parameter
        s.setState("listening");
        try {
          const { captureParameter } = await import("./audio/paramCapture");
          const pcm = await captureParameter(3000);
          if (pcm && pcm.length > 0) {
            s.setState("thinking");
            const { transcribeAudio } = await import("./audio/stt");
            const param = await transcribeAudio(pcm);
            if (param && param.trim().length > 0) {
              console.log(`[NEXUS] Tier 3 parameter: "${param}"`);
              s.addUserMessage(param);
              // Execute with the parameter as the query
              const { invoke } = await import("@tauri-apps/api/core");
              const result = await invoke<{ success: boolean; message: string }>(
                "execute_command",
                { intent: { action: intent.action, query: param } }
              );
              console.log(`[NEXUS] Tier 3 execute result:`, result);
              if (result.message) {
                s.addAssistantMessage(result.message);
                void speak(result.message.replace(/,/g, ""));
              }
            } else {
              console.warn("[NEXUS] Tier 3 parameter STT returned empty");
              s.addAssistantMessage("Didn't catch that sir");
              void speak("Didn't catch that sir");
            }
          }
        } catch (err) {
          console.error("[NEXUS] Tier 3 parameter capture failed:", err);
          s.addAssistantMessage("Didn't catch that sir");
          void speak("Didn't catch that sir");
        }

        setTimeout(() => {
          // Main Center UI-director rule (doc 74 P2): never hide mid-ghost-session.
          if (!useAssistant.getState().ghostActive) {
            useAssistant.getState().setVisible(false);
          }
          // Ghost-aware turn end (no-op outside ghost mode).
          setTimeout(() => { void import("./net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
        }, 800);
        return;
      }

      // ─── Type 1: Fixed command (no parameter) ──────────────────────
      // Execute directly — no STT needed.
      s.addUserMessage(`${intent.action.replace(/_/g, " ")} ${intent.target}`);
      s.addAssistantMessage("Ok sir.");
      void speak("Ok sir.");

      // Execute the command directly — no STT needed
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke<{ success: boolean; message: string }>(
          "execute_command",
          { intent }
        );
        console.log(`[NEXUS] Tier 3 execute result:`, result);
      } catch (err) {
        console.error("[NEXUS] Tier 3 command execution failed:", err);
      }

      // Hide after a short delay
      setTimeout(() => {
        // Main Center UI-director rule (doc 74 P2): never hide mid-ghost-session.
        if (!useAssistant.getState().ghostActive) {
          useAssistant.getState().setVisible(false);
        }
        // Ghost-aware turn end (no-op outside ghost mode).
        setTimeout(() => { void import("./net/ghostHotMic").then((m) => m.endGhostTurn()); }, 550);
      }, 800);
    });
    console.log("[NEXUS] Tier 3 command detection listener registered");
  } catch (err) {
    // Non-fatal — STT fallback handles all commands if this listener fails
    console.warn("[NEXUS] Failed to register Tier 3 command listener:", err);
  }
}

// Register the listener at startup (non-blocking, non-fatal)
void setupCommandDetectionListener();

export type SttTurnOwnership = "verified" | "uncertain" | "rejected" | "unenrolled";

export interface SttTranscriptTurn {
  session?: number;
  text?: string;
  ownership?: SttTurnOwnership;
  owner_score?: number;
  decoder_bias?: string;
  language?: string;
}

export type SttTranscriptPayload = string | SttTranscriptTurn;

// ─── Rust-side STT transcript listener ─────────────────────────────────
// The Rust cpal stream captures audio and transcribes it (Groq or local
// faster-whisper). When the transcript is ready, Rust emits "stt:transcript".
// The frontend processes it using the same logic as the old finishCapture().
async function setupSttTranscriptListener() {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<SttTranscriptPayload>("stt:transcript", async (event) => {
      const payload = event.payload;
      const transcript = typeof payload === "string" ? payload : payload.text ?? "";
      const turn =
        typeof payload === "string"
          ? undefined
          : {
              ownership: payload.ownership ?? "unenrolled",
              ownerScore: payload.owner_score ?? 1,
              decoderBias: payload.decoder_bias ?? "owner",
              language: payload.language ?? "en",
              initiation: useAssistant.getState().ghostActive ? ("ghost" as const) : ("explicit" as const),
            };
      console.log(`[NEXUS] stt:transcript event received: "${transcript}"`);
      // Process the transcript using the same logic as finishCapture()
      await processTranscript(transcript, turn);
    });
    console.log("[NEXUS] stt:transcript listener registered");
  } catch (err) {
    console.warn("[NEXUS] Failed to register stt:transcript listener:", err);
  }
}

// Register the listener at startup (non-blocking, non-fatal)
void setupSttTranscriptListener();

interface TurnStats {
  session: number;
  rms_max: number;
  rms_mean: number;
  voiced_chunks: number;
  total_chunks: number;
  endpoint: string;
  stt_path: string;
  filter: string;
  owner?: string;
  owner_score?: number;
  decoder_bias?: string;
  stt_language?: string;
}

// ─── Turn packet listener (approach A: instrument-first) ─────────────
// Every transcript — including every empty one — arrives with its cause
// attached (mic energy seen, endpoint branch, STT path, filter verdict).
// Logged to console + debug_trace so the next miss is data, not mystery.
// Read-only: never touches orb state or the transcript flow.
async function setupTurnStatsListener() {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<TurnStats>("stt:turn_stats", async (event) => {
      const p = event.payload;
      console.info(
        `[NEXUS] stt:turn_stats session=${p.session} ` +
          `rms_max=${Number(p.rms_max).toFixed(4)} rms_mean=${Number(p.rms_mean).toFixed(5)} ` +
          `voiced=${p.voiced_chunks}/${p.total_chunks} endpoint=${p.endpoint} ` +
          `path=${p.stt_path} filter=${p.filter} owner=${p.owner ?? "unknown"}`
      );
      const { invoke } = await import("@tauri-apps/api/core");
      void invoke("debug_trace", {
        msg:
          `turn session=${p.session} rms_max=${Number(p.rms_max).toFixed(4)} ` +
          `voiced=${p.voiced_chunks}/${p.total_chunks} endpoint=${p.endpoint} ` +
          `path=${p.stt_path} filter=${p.filter} owner=${p.owner ?? "unknown"}`,
      }).catch(() => {});
    });
    console.log("[NEXUS] stt:turn_stats listener registered");
  } catch (err) {
    console.warn("[NEXUS] Failed to register stt:turn_stats listener:", err);
  }
}

// Register at startup
void setupTurnStatsListener();

// ─── Rust ghost-watchdog relisten ──────────────────────────────────
// The P1 relisten watchdog (ghost.rs) emits `ghost:relisten` when a
// session is live but idle past the window (a dropped turn-end
// relisten). Answer with the GUARDED path — echo + meeting checks —
// never a blind capture.
async function setupGhostRelistenListener() {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen("ghost:relisten", () => {
      console.log("[NEXUS] ghost:relisten from watchdog — guarded relisten");
      import("./net/ghostHotMic").then((m) => m.maybeGhostRelisten()).catch(() => {});
    });
    console.log("[NEXUS] ghost:relisten listener registered");
  } catch (err) {
    console.warn("[NEXUS] Failed to register ghost:relisten listener:", err);
  }
}

// Register at startup
void setupGhostRelistenListener();

// ─── Rust capture level meter ──────────────────────────────────────
// The cpal loop emits `audio:level` {level: 0..1} ~6Hz during STT capture
// (+ final 0 on stop). Drives the speech-synced ghost waves. Cheap:
// one clamped number into the store, no re-render storm (Avatar reads it
// in a rAF loop via getState, not via subscription).
async function setupAudioLevelListener() {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    await listen<{ level?: unknown }>("audio:level", (event) => {
      const raw = event.payload?.level;
      const level = typeof raw === "number" && Number.isFinite(raw) ? raw : 0;
      useAssistant.getState().setMicLevel(level);
    });
    console.log("[NEXUS] audio:level listener registered");
  } catch (err) {
    console.warn("[NEXUS] Failed to register audio:level listener:", err);
  }
}

// Register at startup
void setupAudioLevelListener();

// Open the backend session at startup (fire-and-forget): the long-running
// command queue drains through sendTranscript(), which throws while no
// session is open. openSession() is config-only (no network), retries ×5
// internally, and degrades to local-only mode on failure — never blocks boot.
void (async () => {
  try {
    const { ensureSessionOpen } = await import("./net/wsBridge");
    void ensureSessionOpen();
  } catch (e) {
    console.warn("[NEXUS] session pre-open failed:", e);
  }
})();

// NOTE: __NEXUS_CANCEL__ was deleted (audit H4) — assigned but never
// called from Rust eval or frontend code. Cancellation flows through
// abortCapture() + the orchestrator's ACTIVE_REQUEST guard instead.

/** Called by finishCapture/abortCapture cleanup to release the mic stream.
 *  With hot mic, we DON'T release the stream — we keep it warm for the next wake.
 *  The stream tracks are disabled by VAD's pauseStream callback instead. */
// Holder id for the shared frontend stream (approach C audit). The release
// path below is the single funnel: paramCapture, recorder cleanup, and the
// 8s no-speech timer all land here.
let sharedMicHolder: string | null = null;
(window as any).__NEXUS_RELEASE_MIC__ = () => {
  // Hot mic: keep the stream alive, just disable the tracks
  if (micStream) {
    micStream.getTracks().forEach((t) => (t.enabled = false));
  }
  if (sharedMicHolder !== null) {
    void import("./audio/micHolders").then(({ micRelease }) => {
      if (sharedMicHolder !== null) micRelease(sharedMicHolder);
      sharedMicHolder = null;
    }).catch(() => {});
  }
  // THE BATON PASS: Tell Rust to resume wake-word detection now that
  // the frontend is done with the mic. Without this, the wake-word
  // engine stays deaf after the first voice command.
  import("@tauri-apps/api/core").then(({ invoke }) => {
    invoke("resume_wakeword").catch((e: unknown) => console.warn("resume_wakeword failed:", e));
    console.log("[NEXUS] baton pass: Rust wakeword resumed");
  }).catch(() => {});
};

/** Called by paramCapture to get the existing mic stream (or null if not active). */
(window as any).__NEXUS_GET_MIC_STREAM__ = async (): Promise<MediaStream> => {
  if (micStream) return micStream;
  // If no existing stream, get a new one
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: {
      channelCount: 1,
      echoCancellation: true,
      noiseSuppression: true,
    },
  });
  micStream = stream;
  // Approach C: record the opener. A second concurrent opener reuses this
  // stream with a warning instead of opening a rival one (Intel SST
  // starves cpal while ANY WebView2 stream is live).
  if (sharedMicHolder === null) {
    const { micAcquire } = await import("./audio/micHolders");
    sharedMicHolder = micAcquire("frontend-stream");
  } else {
    console.warn("[NEXUS] mic: second opener reused live stream (no double-open)");
  }
  return stream;
};

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
