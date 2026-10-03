/**
 * NEXUS Central Orchestrator — frontend event listener.
 *
 * This module listens to the "orchestrator:event" channel from Rust and
 * translates events into frontend state changes (Zustand store updates,
 * TTS playback, sidebar display, etc).
 *
 * This REPLACES the scattered "assistant:server" event handling in
 * wsBridge.ts and recorder.ts. The central orchestrator in Rust now owns:
 *   - When to show/hide the loading indicator
 *   - When to speak the ack
 *   - When to speak the result
 *   - When to show the sidebar with the response
 *   - Request lifecycle (cancel, done, error)
 *
 * The frontend just reacts to orchestrator events — it no longer makes
 * independent decisions about loading state or ack timing.
 */

import { useAssistant } from "../store/assistant";
import { speak, stopTts } from "../audio/ttsPlayer";
import { useSidebar } from "../sidebar/sidebarStore";
import { clearLongRunningInFlight, isLocalAckGiven } from "./wsBridge";
import { invoke } from "@tauri-apps/api/core";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/** Orchestrator event shape (mirrors Rust OrchestratorEvent enum). */
interface OrchestratorEvent {
  type:
  | "state"
  | "loading"
  | "ack"
  | "result"
  | "clarify"
  | "done"
    | "error"
    | "confirm"
    | "conflict_report"
    | "github_result";
  request_id: string;
  // state
  state?: "idle" | "listening" | "thinking" | "speaking";
  // loading
  visible?: boolean;
  // ack
  text?: string;
  // clarify
  expected_slot?: string;
  timeout_ms?: number;
  // result
  analysis?: unknown;
  dialog_state?: unknown;
  // error
  message?: string;
  // confirm (GitHub destructive operation)
  prompt?: string;
  command?: unknown; // Serialized GitHubCommand
  // conflict_report (GitHub merge conflict)
  pr_number?: number;
  repo?: string;
  conflict_files?: ConflictFile[];
  // github_result
  result?: GitHubResultPayload;
}

/** A file with merge conflicts (mirrors Rust ConflictFile). */
interface ConflictFile {
  filename: string;
  conflict_count: number;
  blocks: ConflictBlock[];
}

/** A single conflict block (mirrors Rust ConflictBlock). */
interface ConflictBlock {
  start_line: number;
  head_content: string;
  branch_content: string;
}

/** GitHub result payload (mirrors Rust GitHubResult enum). */
interface GitHubResultPayload {
  type: "text" | "needs_confirmation" | "merge_conflict" | "error";
  text?: string;
  prompt?: string;
  command?: unknown;
  pr_number?: number;
  repo?: string;
  conflict_files?: ConflictFile[];
  message?: string;
  status?: number;
  is_auth_error?: boolean;
}

let initialized = false;
let currentRequestId: string | null = null;
let confirmListeningTimer: ReturnType<typeof setTimeout> | null = null;
let clarificationRequestId: string | null = null;
let clarificationTimer: ReturnType<typeof setTimeout> | null = null;

function clearConfirmListeningTimer(): void {
  if (confirmListeningTimer) {
    clearTimeout(confirmListeningTimer);
    confirmListeningTimer = null;
  }
}

export function clearClarificationRequest(): void {
  clarificationRequestId = null;
  if (clarificationTimer) {
    clearTimeout(clarificationTimer);
    clarificationTimer = null;
  }
}

/**
 * One bounded automatic relisten after a clarification prompt. Reuses the
 * hotkey wake path and meeting suppression, but unlike ghost mode it closes
 * after a single window so background audio cannot hold the mic open.
 */
async function openClarificationCapture(requestId: string, timeoutMs: number): Promise<void> {
  const store = useAssistant.getState();
  if (store.ghostActive) return;
  try {
    const meeting = await invoke<boolean>("meeting_active").catch(() => false);
    if (meeting) {
      clearClarificationRequest();
      store.setAwaitingInput(false);
      await cancelOrchestrator();
      return;
    }
  } catch {
    // A failed meeting probe must not wedge clarification; fall through.
  }
  store.setVisible(true);
  store.setState("listening");
  store.setAwaitingInput(true);
  try {
    await invoke("start_stt_capture");
  } catch (err) {
    console.warn("[NEXUS] orchestrator: clarification capture failed:", err);
    clearClarificationRequest();
    return;
  }
  if (clarificationTimer) clearTimeout(clarificationTimer);
  clarificationTimer = setTimeout(() => {
    clarificationTimer = null;
    if (clarificationRequestId !== requestId) return;
    clearClarificationRequest();
    void (async () => {
      try {
        const hadSpeech = await invoke<boolean>("stt_capture_had_speech").catch(() => false);
        if (hadSpeech) return;
      } catch {
        // Fall through to the legacy close path.
      }
      await cancelOrchestrator();
      const latest = useAssistant.getState();
      latest.setAwaitingInput(false);
      latest.setVisible(false);
      setTimeout(() => useAssistant.getState().reset(), 550);
    })();
  }, timeoutMs);
}

/** Open the 5-second voice approval listening window for pending confirmations. */
export function openConfirmVoiceWindow(): void {
  const curStore = useAssistant.getState();
  if (curStore.pendingGithubCommand) {
    console.log("[NEXUS] orchestrator: prompt spoken — opening 5s voice approval window");
    curStore.setState("listening");
    import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("start_stt_capture"))
      .catch(() => {});

    clearConfirmListeningTimer();
    confirmListeningTimer = setTimeout(() => {
      confirmListeningTimer = null;
      const latest = useAssistant.getState();
      if (latest.state === "listening" && latest.pendingGithubCommand) {
        console.log("[NEXUS] orchestrator: 5s voice window timed out — mic back to idle, sidebar remains interactive");
        latest.setState("idle");
      }
    }, 5000);
  }
}

/** Current request ID (for debugging / diagnostics). */
export function getCurrentRequestId(): string | null {
  return currentRequestId;
}

/** Test hook: set the in-flight request id (vitest only). */
export function __testSetCurrentRequestId(id: string | null): void {
  currentRequestId = id;
}

/**
 * Complete a spoken result turn. Called when result TTS finishes (or fails).
 * Returns true when this turn was still current and the orb was reset;
 * false when a barge-in already moved on (stale onEnd must never touch the
 * new turn's state — the cancel flow owns that path).
 */
export function finishSpokenResult(spokenFor: string): boolean {
  if (currentRequestId !== spokenFor) return false;
  currentRequestId = null;
  clearLongRunningInFlight();
  clearConfirmListeningTimer();
  const store = useAssistant.getState();
  store.setLoadingVisible(false);
  store.setVisible(true); // brief beat, mirrors the `done` handler
  // Phase 4 cadence: ghost turns reopen the mic at 250ms (the hot-mic
  // loop owns the gap from here); normal turns keep the 550ms beat.
  const beat = store.ghostActive ? 250 : 550;
  setTimeout(() => {
    if (currentRequestId === null) {
      // Ghost hot-mic: reopen the mic if the session is still live
      // BEFORE resetting (the relisten gate reads ghostActive; keep
      // this order even though reset() preserves it today).
      // No-op everywhere outside ghost mode.
      void import("./ghostHotMic").then((m) => m.maybeGhostRelisten());
      useAssistant.getState().reset();
    }
  }, beat);
  void signalOrchestratorDone(spokenFor);
  return true;
}

/**
 * Close a non-ghost turn when its closing speech ends. Ghost turns stay open
 * for the hot-mic loop (the legacy timers / endGhostTurn own those paths) —
 * this helper returns false there and touches nothing. Pure contract,
 * unit-tested; used by the result/error/conflict_report handlers so no
 * request can park the orb in `speaking` forever.
 */
export function closeTurnOnSpeechEnd(requestId: string): boolean {
  if (useAssistant.getState().ghostActive) return false;
  return finishSpokenResult(requestId);
}

/**
 * Main Center UI-director rule (doc 74 P2 / feature 75 P-B): never hide
 * the orb while it is speaking (the zoom must play fully), never hide
 * mid-ghost-session, and never steal a new turn's orb. Hides only once
 * the turn is fully done (idle) — re-arming once per second while TTS is
 * still playing instead of cutting it off at a fixed delay.
 *
 * Replaces the old fixed 600/1500ms hide timers, which fired mid-speech
 * (the zoom played on a sliding-down orb) and mid-ghost-session.
 *
 * Stuck-speaking watchdog (voice-STT plan §5): genuine replies keep TTS
 * playing and re-arm forever (unchanged). Ten consecutive re-arms with NO
 * audio playing means onEnd died mid-turn — force the done-handshake so
 * the zoom doesn't park on a dead orb until the 60s failsafe. Visuals
 * thus follow audio truth, never just the state flag.
 */
let stuckSpeakingTicks = 0;
const STUCK_SPEAKING_LIMIT = 10;

/** Test hooks for the stuck-speaking watchdog. */
export function __testStuckTicks(): number {
  return stuckSpeakingTicks;
}
export function __testSetStuckTicks(n: number): void {
  stuckSpeakingTicks = n;
}

export function hideOrbAfterSpeech(firstDelayMs: number): void {
  setTimeout(() => {
    const s = useAssistant.getState();
    if (s.ghostActive) {
      stuckSpeakingTicks = 0;
      return;
    }
    if (s.state === "speaking") {
      void import("../audio/ttsPlayer").then(({ isRustTtsPlaying }) => {
        if (useAssistant.getState().state !== "speaking") {
          stuckSpeakingTicks = 0;
          return;
        }
        if (isRustTtsPlaying()) {
          stuckSpeakingTicks = 0;
          hideOrbAfterSpeech(1000);
          return;
        }
        stuckSpeakingTicks += 1;
        if (stuckSpeakingTicks >= STUCK_SPEAKING_LIMIT) {
          stuckSpeakingTicks = 0;
          console.warn(
            "[NEXUS] stuck-speaking watchdog: silent 10s, forcing turn close"
          );
          finishSpokenResult(getCurrentRequestId() ?? "");
          return;
        }
        hideOrbAfterSpeech(1000);
      }).catch(() => {
        hideOrbAfterSpeech(1000);
      });
      return;
    }
    stuckSpeakingTicks = 0;
    if (s.state !== "idle") return;
    s.setVisible(false);
  }, firstDelayMs);
}

/**
 * Initialize the orchestrator event listener.
 * Call this once at app startup (from App.tsx or main.tsx).
 *
 * This listens to the "orchestrator:event" channel and dispatches to:
 *   - useAssistant store (state, visible, loadingVisible, transcript)
 *   - TTS player (speak ack, speak result, stop on cancel)
 *   - Sidebar display (show result)
 */
export async function initOrchestratorListener(): Promise<void> {
  if (initialized || !isTauri()) return;
  initialized = true;

  const { listen } = await import("@tauri-apps/api/event");

  console.log("[NEXUS] orchestrator: initializing event listener");

  await listen<OrchestratorEvent>("orchestrator:event", async (event) => {
    const ev = event.payload;
    const store = useAssistant.getState();

    console.log(`[NEXUS] orchestrator: ${ev.type} (req=${ev.request_id})`, ev);

    switch (ev.type) {
      case "state": {
        if (ev.state) {
          store.setState(ev.state as any);
        }
        break;
      }

      case "loading": {
        // The Rust side already shows/hides the loading window directly.
        // We just update the store for UI consistency (e.g. if the frontend
        // needs to know the loading state for rendering decisions).
        if (ev.visible !== undefined) {
          const isGhost = useAssistant.getState().ghostActive;
          store.setLoadingVisible(isGhost ? false : ev.visible);
          if (ev.visible && !isGhost) {
            // Hide the orb shortly after the loading indicator appears.
            // This keeps the orb visible while "On it sir" is playing,
            // then transitions to the loading indicator once it's ready.
            // State-aware (never mid-speech, never mid-ghost).
            hideOrbAfterSpeech(600);
          }
        }
        break;
      }

      case "ack": {
        // Speak the acknowledgement ("On it sir")
        // Skip if we've already given a local ack (ackLongRunningQuery in
        // recorder.ts) to avoid double-speak ("On it sir" said twice).
        if (isLocalAckGiven()) {
          console.log("[NEXUS] orchestrator ack suppressed — local ack already given");
          // Still hide the orb after TTS finishes (if not in ghost mode) — the
          // loading indicator will take over. State-aware, never mid-speech.
          hideOrbAfterSpeech(1500);
          break;
        }
        if (ev.text) {
          store.setState("speaking");
          store.addAssistantMessage(ev.text);
          void speak(ev.text);
          // Hide the orb after TTS actually finishes (if not in ghost
          // mode) — the zoom plays fully instead of sliding down mid-speech.
          hideOrbAfterSpeech(1500);
        }
        break;
      }

      case "clarify": {
        // A clarification prompt keeps the turn open for one bounded reply.
        // Unlike result turns, this must not call finishSpokenResult: closing
        // here would require another wake word for the user's repeat.
        currentRequestId = ev.request_id;
        clearClarificationRequest();
        clarificationRequestId = ev.request_id;
        store.setLoadingVisible(false);
        store.setVisible(true);
        store.setState("speaking");
        store.setAwaitingInput(true);
        if (ev.prompt) {
          store.addAssistantMessage(ev.prompt);
          const captureAfterSpeech = ev.request_id;
          const captureTimeout = ev.timeout_ms ?? 8000;
          const beginReplyCapture = () => {
            if (clarificationRequestId !== captureAfterSpeech) return;
            if (useAssistant.getState().ghostActive) {
              void import("./ghostHotMic").then((m) => m.maybeGhostRelisten());
              return;
            }
            void openClarificationCapture(captureAfterSpeech, captureTimeout);
          };
          void speak(ev.prompt, beginReplyCapture).catch(beginReplyCapture);
        } else {
          void openClarificationCapture(ev.request_id, ev.timeout_ms ?? 8000);
        }
        break;
      }

      case "result": {
        // Final result from the subsystem
        currentRequestId = ev.request_id;

        // Clear the long-running in-flight flag so subsequent voice
        // commands aren't incorrectly deduped/queued.
        clearLongRunningInFlight();

        // Hide loading (Rust already does this, but update store too)
        store.setLoadingVisible(false);

        // Show the orb again for speaking the result
        store.setVisible(true);
        store.setState("speaking");
        store.setAwaitingInput(false);

        // Add to transcript
        if (ev.text) {
          store.addAssistantMessage(ev.text);
        }

        // Speak the result, then close the handshake: the backend
        // withholds `done` on success (emitting it would cancel TTS), so
        // the frontend must signal completion itself. Without this the orb
        // parks in `speaking` forever after long replies.
        // Guarded by request id: a barged-in turn must never reset the new
        // turn's state (barge-in abort skips onEnd; the cancel flow owns it).
        if (ev.text) {
          const spokenFor = ev.request_id;
          speak(ev.text, () => {
            finishSpokenResult(spokenFor);
          }).catch((err) => {
            console.warn("[NEXUS] orchestrator: result TTS failed:", err);
            finishSpokenResult(spokenFor);
          });
        } else {
          // Empty result text: no TTS to gate on — close the handshake now
          // or the orb parks in `speaking` forever (the backend withholds
          // `done` on success, so nobody else will close it).
          finishSpokenResult(ev.request_id);
        }

        // If there's analysis data, we could show it in the sidebar
        // (the existing sidebar logic handles this via the old channel)
        if (ev.analysis) {
          console.log("[NEXUS] orchestrator: result has analysis data", ev.analysis);
        }
        if (ev.dialog_state) {
          console.log("[NEXUS] orchestrator: result has dialog state", ev.dialog_state);
        }
        break;
      }

      case "done": {
        // Request is fully complete (TTS finished speaking)
        currentRequestId = null;
        clearLongRunningInFlight();
        store.setLoadingVisible(false);
        store.setAwaitingInput(false);
        const curStore = useAssistant.getState();
        if (curStore.pendingGithubCommand) {
          console.log("[NEXUS] orchestrator: done event with pending confirmation — opening voice approval window");
          openConfirmVoiceWindow();
        } else {
          store.setVisible(true); // Show orb briefly before reset
          // Phase 4 cadence: ghost beat 250ms, normal beat 550ms.
          const doneBeat = useAssistant.getState().ghostActive ? 250 : 550;
          setTimeout(() => {
            // Ghost hot-mic first — the gate reads ghostActive, so
            // relisten before reset (reset() preserves it today, but
            // this order must not depend on that).
            void import("./ghostHotMic").then((m) => m.maybeGhostRelisten());
            store.reset();
          }, doneBeat);
        }
        break;
      }

      case "error": {
        console.error("[NEXUS] orchestrator: error:", ev.message);
        clearLongRunningInFlight();
        currentRequestId = ev.request_id;
        store.setLoadingVisible(false);
        store.setVisible(true);
        store.setState("speaking");
        store.setAwaitingInput(false);
        const errMsg = ev.message || "Something went wrong sir.";
        store.addAssistantMessage(`Error: ${errMsg}`);
        // Close the turn when the error speech ends (normal mode only —
        // ghost turns stay open for the hot-mic loop via the timer below).
        // Without this the orb parks in `speaking` with dead air.
        void speak(errMsg, () => {
          closeTurnOnSpeechEnd(ev.request_id);
        }).catch(() => {
          closeTurnOnSpeechEnd(ev.request_id);
        });
        // After speaking the error, end the turn via the ghost-aware
        // path: a backend error mid-session must not deafen the loop.
        // Phase 4 cadence: ghost beat 250ms, normal beat 550ms.
        const errBeat = useAssistant.getState().ghostActive ? 250 : 550;
        setTimeout(() => {
          currentRequestId = null;
          setTimeout(() => { void import("./ghostHotMic").then((m) => m.endGhostTurn()); }, errBeat);
        }, 3000);
        break;
      }

      case "confirm": {
        // Operation needs confirmation.
        // Store the pending command so when the user says "yes" / "approved" / "proceed",
        // processViaOrchestrator can re-invoke with confirmed=true.
        clearLongRunningInFlight();
        clearConfirmListeningTimer();
        store.setLoadingVisible(false);
        store.setVisible(true);
        store.setState("speaking");
        // The prompt speech ends but the turn stays open waiting for
        // "yes" — mark it so the orb holds + glows instead of looping
        // over silence (or going dead).
        store.setAwaitingInput(true);
        store.setPendingGithubCommand(ev.command ?? null);
        console.log("[NEXUS] orchestrator: confirm needed for command", ev.command);

        if (ev.prompt) {
          store.addAssistantMessage(ev.prompt);
          void speak(ev.prompt, () => {
            openConfirmVoiceWindow();
          }).catch(() => {
            openConfirmVoiceWindow();
          });
        }
        break;
      }

      case "conflict_report": {
        // GitHub merge conflict detected.
        // Speak the conflict summary and display the conflict panel
        // in the sidebar with copy-paste options.
        clearLongRunningInFlight();
        currentRequestId = ev.request_id;
        store.setLoadingVisible(false);
        store.setVisible(true);
        store.setState("speaking");

        const prNum = ev.pr_number ?? 0;
        const repo = ev.repo || "";
        const files = ev.conflict_files || [];
        const fileCount = files.length;

        const summary = ev.message || `PR #${prNum} in ${repo} has merge conflicts.`;
        const spoken = `${summary} ${fileCount} file${fileCount !== 1 ? "s" : ""} have conflicts. Please fix the conflicts and push, then try merging again.`;

        store.addAssistantMessage(spoken);
        // Close the turn when the summary ends (normal mode only) — the
        // user fixes conflicts in a fresh turn; nothing else closes this one.
        void speak(spoken, () => {
          closeTurnOnSpeechEnd(ev.request_id);
        }).catch(() => {
          closeTurnOnSpeechEnd(ev.request_id);
        });

        console.log("[NEXUS] orchestrator: merge conflict", {
          pr_number: prNum,
          repo,
          files,
        });

        // Show the conflict panel in the sidebar with copy-paste options
        useSidebar.getState().showConflict({
          prNumber: prNum,
          repo,
          conflictFiles: files,
          message: summary,
        });

        break;
      }

      case "github_result": {
        // Raw GitHub result — used for structured UI display.
        // The text/conflict/error cases are already handled by the
        // result/conflict_report/error events above. This event provides
        // the raw structured data for advanced UI rendering.
        console.log("[NEXUS] orchestrator: github_result", ev.result);
        break;
      }
    }
  });

  console.log("[NEXUS] orchestrator: event listener ready");
}

/**
 * Process a transcript through the central orchestrator.
 *
 * This is the frontend entry point — call this after STT produces a transcript.
 * It invokes the Rust `orchestrator_process` command which:
 *   1. Parses intent (deterministic, <1ms)
 *   2. Routes to the correct subsystem
 *   3. Emits ack + loading events
 *   4. Dispatches to the subsystem
 *   5. Emits result + done
 *
 * The caller does NOT need to manage loading state, ack timing, or TTS —
 * the orchestrator handles all of that.
 */
export interface TurnProvenance {
  ownership?: "verified" | "uncertain" | "rejected" | "unenrolled";
  ownerScore?: number;
  decoderBias?: string;
  language?: string;
  initiation?: "explicit" | "ghost";
}

export async function processViaOrchestrator(
  transcript: string,
  dialogContext?: unknown,
  turn?: TurnProvenance,
): Promise<{ request_id: string; subsystem: string; handled_locally: boolean } | null> {
  if (!isTauri()) return null;

  // ─── Confirmation flow ───
  // If there's a pending command awaiting confirmation, check if
  // the user said "yes"/"approved"/"proceed" (confirm) or "no"/"cancel" (abort).
  const store = useAssistant.getState();
  const pendingCmd = store.pendingGithubCommand;
  if (turn?.ownership === "rejected" || (turn?.ownership === "uncertain" && pendingCmd)) {
    console.log("[NEXUS] orchestrator: unowned turn cannot approve a pending command — ambient drop");
    return {
      request_id: currentRequestId ?? "ambient-drop",
      subsystem: "ambient",
      handled_locally: true,
    };
  }
  if (pendingCmd) {
    clearConfirmListeningTimer();
    const lower = transcript.trim().toLowerCase();
    const isYes = /^(yes|yeah|yep|yup|confirm|ok|okay|sure|go ahead|do it|proceed|approved?|agreed?|approve)\b/i.test(lower);
    const isNo = /^(no|nope|cancel|abort|stop|don't|dont|never|disapproved?|disapprove)\b/i.test(lower);

    if (isYes) {
      // Clear the pending command first, then re-execute with confirmed=true
      store.setPendingGithubCommand(null);
      useAssistant.getState().addUserMessage(transcript);

      // MCP confirmations carry kind:"mcp" + {server, tool, params} —
      // route them to orchestrator_mcp_confirm instead of github_execute.
      const isMcp = (pendingCmd as any)?.kind === "mcp";
      if (isMcp) {
        console.log("[NEXUS] orchestrator: confirming pending MCP call", pendingCmd);
        try {
          const result = await invoke<unknown>("orchestrator_mcp_confirm", {
            requestId: currentRequestId ?? "mcp-confirm",
            confirmed: true,
            pending: pendingCmd,
          });
          console.log("[NEXUS] orchestrator: mcp_confirm result", result);
          await invoke("hide_sidebar").catch(() => {});
          return {
            request_id: "mcp-confirmed",
            subsystem: "mcp",
            handled_locally: false,
          };
        } catch (err) {
          console.error("[NEXUS] orchestrator: mcp_confirm failed:", err);
          return null;
        }
      }

      console.log("[NEXUS] orchestrator: confirming pending GitHub command", pendingCmd);
      try {
        const result = await invoke<unknown>("orchestrator_github_execute", {
          command: pendingCmd,
          confirmed: true,
        });
        console.log("[NEXUS] orchestrator: github_execute confirmed result", result);
        await invoke("hide_sidebar").catch(() => {});
        // The result events are emitted by Rust on the orchestrator:event channel
        // and handled by the listener above.
        return {
          request_id: String((result as any)?.request_id ?? "github-confirmed"),
          subsystem: "github",
          handled_locally: false,
        };
      } catch (err) {
        console.error("[NEXUS] orchestrator: github_execute confirmed failed:", err);
        return null;
      }
    } else if (isNo) {
      // User declined — clear the pending command
      const wasMcp = (pendingCmd as any)?.kind === "mcp";
      store.setPendingGithubCommand(null);
      useAssistant.getState().addUserMessage(transcript);
      if (wasMcp) {
        try {
          await invoke<unknown>("orchestrator_mcp_confirm", {
            requestId: currentRequestId ?? "mcp-cancel",
            confirmed: false,
            pending: pendingCmd,
          });
        } catch (err) {
          console.warn("[NEXUS] orchestrator: mcp cancel failed:", err);
        }
      }
      await invoke("hide_sidebar").catch(() => {});
      const abortMsg = "Okay, I've cancelled that operation, sir.";
      useAssistant.getState().addAssistantMessage(abortMsg);
      void speak(abortMsg);
      // Ghost-aware turn end: declining a confirm mid-session must not
      // deafen the loop (endGhostTurn no-ops outside ghost mode).
      setTimeout(() => { void import("./ghostHotMic").then((m) => m.endGhostTurn()); }, 2000);
      return {
        request_id: wasMcp ? "mcp-aborted" : "github-aborted",
        subsystem: wasMcp ? "mcp" : "github",
        handled_locally: true,
      };
    }
    // If it's neither yes nor no, fall through to normal processing
    // (the user may have said a completely different command)
    store.setPendingGithubCommand(null);
    void invoke("hide_sidebar").catch(() => {});
  }

  try {
    const result = await invoke<{
      request_id: string;
      subsystem: string;
      handled_locally: boolean;
    }>("orchestrator_process", {
      transcript,
      dialogContext: dialogContext ?? null,
      turnContext: turn ?? null,
    });

    currentRequestId = result.request_id;
    console.log("[NEXUS] orchestrator: process result", result);
    return result;
  } catch (err) {
    console.error("[NEXUS] orchestrator: process failed:", err);
    return null;
  }
}

/** Cancel the active orchestrator request (barge-in / new wake). */
export async function cancelOrchestrator(): Promise<void> {
  clearClarificationRequest();
  if (!isTauri()) return;
  try {
    await invoke("orchestrator_cancel");
    stopTts();
    currentRequestId = null;
  } catch (err) {
    console.warn("[NEXUS] orchestrator: cancel failed:", err);
  }
}

/** Signal that a request is done (called after TTS finishes). */
export async function signalOrchestratorDone(requestId: string): Promise<void> {
  if (!isTauri()) return;
  try {
    await invoke("orchestrator_done", { requestId });
  } catch (err) {
    console.warn("[NEXUS] orchestrator: done signal failed:", err);
  }
}
