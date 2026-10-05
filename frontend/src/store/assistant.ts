import { create } from "zustand";
import {
  LOADING_HIDDEN,
  LoadingSnapshot,
  loadingHide,
  loadingSettle,
  loadingShow,
} from "./loadingMachine";

export type AssistantState = "idle" | "listening" | "thinking" | "speaking";

interface TranscriptEntry {
  role: "user" | "assistant";
  text: string;
  timestamp: number;
}

interface AssistantStore {
  state: AssistantState;
  visible: boolean;
  /** Whether the loading animation overlay (top-right corner) is showing.
   *  Set to true when "On it sir" is spoken (command validated as long-running).
   *  Set to false when the result arrives or the orb re-shows. */
  loadingVisible: boolean;
  /** Conversation transcript for display in the sidebar. */
  transcript: TranscriptEntry[];
  /** True while TTS audio is actually playing (event-derived, see
   *  audio/ttsActivity.ts). The orb animation gates on this — not on
   *  `state` — so the loading loop never runs over silence. */
  ttsActive: boolean;
  setTtsActive: (v: boolean) => void;
  /** True while the orb is waiting on the user with no audio (e.g. a
   *  confirm prompt after its speech ends). Renders the hold-frame +
   *  steady "waiting" glow instead of a frozen loop or a dead orb. */
  awaitingInput: boolean;
  setAwaitingInput: (v: boolean) => void;
  /** True while a ghost cursor session is live (mic hot, waves on). */
  ghostActive: boolean;
  setGhostActive: (v: boolean) => void;
  /** Animation-calibration preview target (null = not calibrating).
   *  When set, the orb window shows the selected target and accepts
   *  native drag + wheel-resize (task: drag & scroll calibration). */
  calibrationTarget: "wakeup" | "waves" | "loading" | null;
  /** Live calibration size badge value (px) — mirrored from the Rust
   *  `calibration:state` event so the pill under the element stays true. */
  calibrationSize: number | null;
  setCalibration: (
    target: "wakeup" | "waves" | "loading" | null,
    size: number | null
  ) => void;
  /** Target-switch pulse counter (calibration preview readability): bumped
   *  whenever the active target changes so the desktop preview plays a
   *  220ms highlight even when both rects are identical. Render-only. */
  calibrationPulse: number;
  bumpCalibrationPulse: () => void;
  /** Index of the TTS chunk currently playing (for avatar mouth animation). */
  speakSeq: number | null;
  /** Current microphone audio volume (RMS, 0.0 - ~1.0) for avatar reactivity. */
  audioVolume: number;
  /** Live mic level 0..1 from the Rust capture loop (`audio:level`,
   *  ~6 Hz during STT capture + final 0 on stop). Drives the
   *  speech-synced ghost waves. Unlike `audioVolume` (browser-VAD
   *  path, dormant under Rust capture) this is live in every mode
   *  that uses cpal capture. Reset to 0 by reset(). */
  micLevel: number;
  setMicLevel: (v: number) => void;
  setState: (s: AssistantState) => void;
  setVisible: (v: boolean) => void;
  setLoadingVisible: (v: boolean) => void;
  setAudioVolume: (v: number) => void;
  addUserMessage: (text: string) => void;
  addAssistantMessage: (text: string) => void;
  setSpeakSeq: (n: number | null) => void;
  /** Reset to idle and hide after a short delay (driven by an effect in App). */
  reset: () => void;
  /** Clear the transcript. */
  clearTranscript: () => void;
  /** Pending GitHub command awaiting user confirmation (destructive ops). */
  pendingGithubCommand: unknown | null;
  setPendingGithubCommand: (cmd: unknown | null) => void;
}

/** Single owner for the loading indicator (see loadingMachine.ts).
 * All ~20 writers funnel through here: minimum 800ms dwell (no flashes),
 * 120s failsafe (a lost hide can never wedge the spinner), transition log. */
let loadingSnap: LoadingSnapshot = LOADING_HIDDEN;
let loadingTimer: ReturnType<typeof setTimeout> | null = null;

export const useAssistant = create<AssistantStore>((set) => ({
  state: "idle",
  visible: false,
  loadingVisible: false,
  transcript: [],
  speakSeq: null,
  audioVolume: 0,
  micLevel: 0,
  ttsActive: false,
  awaitingInput: false,
  ghostActive: false,
  calibrationTarget: null,
  calibrationPulse: 0,
  calibrationSize: null,
  setState: (s) => {
      return set((st) => {
        console.log(`[ORB] state ${st.state} → ${s}`);
        return { state: s };
      });
    },
  setVisible: (v) => set((st) => {
      console.log(`[ORB] setVisible(${v}) from:${st.visible} ghostActive:${st.ghostActive} state:${st.state}`);
      return { visible: st.ghostActive ? true : v };
    }),
  setLoadingVisible: (v) => {
    const now = Date.now();
    const prev = loadingSnap;
    const next = v ? loadingShow(prev, now) : loadingHide(prev, now);
    if (next === prev) {
      return; // no-op (hide-when-hidden)
    }
    loadingSnap = next;
    if (loadingTimer) {
      clearTimeout(loadingTimer);
      loadingTimer = null;
    }
    if (next.pendingAt !== null) {
      const delay = Math.max(0, next.pendingAt - Date.now());
      loadingTimer = setTimeout(() => {
        loadingTimer = null;
        loadingSnap = loadingSettle(loadingSnap, Date.now());
        set({ loadingVisible: loadingSnap.visible });
        console.debug(
          `[NEXUS] loading: settled → ${loadingSnap.visible ? "shown" : "hidden"}`
        );
      }, delay);
    }
    set({ loadingVisible: next.visible });
    console.debug(
      `[NEXUS] loading: ${prev.visible} → ${next.visible} (req=${v})`
    );
  },
  setAudioVolume: (v) => set({ audioVolume: v }),
  setMicLevel: (v) => set({ micLevel: Math.max(0, Math.min(1, v)) }),
  addUserMessage: (text) =>
    set((st) => ({
      transcript: [...st.transcript, { role: "user", text, timestamp: Date.now() }],
    })),
  addAssistantMessage: (text) =>
    set((st) => ({
      transcript: [...st.transcript, { role: "assistant", text, timestamp: Date.now() }],
    })),
  setSpeakSeq: (n) => set({ speakSeq: n }),
  setTtsActive: (v) => set({ ttsActive: v }),
  setAwaitingInput: (v) => set({ awaitingInput: v }),
  setGhostActive: (v) => set((st) => {
      console.log(`[ORB] setGhostActive(${v}) from:${st.ghostActive} visible:${st.visible} state:${st.state}`);
      return { ghostActive: v, visible: v ? true : st.visible };
    }),
  setCalibration: (target, size) =>
    set({ calibrationTarget: target, calibrationSize: size }),
  bumpCalibrationPulse: () => set((st) => ({ calibrationPulse: st.calibrationPulse + 1 })),
  reset: () => set({ state: "idle", speakSeq: null, audioVolume: 0, micLevel: 0, ttsActive: false, awaitingInput: false }),
  clearTranscript: () => set({ transcript: [] }),
  pendingGithubCommand: null,
  setPendingGithubCommand: (cmd) => set({ pendingGithubCommand: cmd }),
}));

/**
 * Canonical state-machine transitions. Enforced everywhere we call setState.
 *   idle  -> listening   (wake / hotkey)
 *   listening -> thinking (VAD silence + local STT + transcript sent)
 *   thinking -> speaking (ack or result event — local TTS speaks)
 *   speaking -> idle      (done event)
 */
export function transition(from: AssistantState, to: AssistantState): boolean {
  const allowed: Record<AssistantState, AssistantState[]> = {
    idle: ["listening"],
    listening: ["thinking", "idle"],
    thinking: ["speaking", "idle"],
    speaking: ["idle"],
  };
  return allowed[from]?.includes(to) ?? false;
}
