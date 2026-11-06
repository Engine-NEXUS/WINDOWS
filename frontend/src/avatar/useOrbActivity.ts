import { useEffect, useRef, useState } from "react";
import { getEnvelopeLevel } from "../audio/captionScheduler";
import { getSpeechStageEnvelopeLevel, speechStageActive } from "../audio/speechStage";
import { useAssistant, type AssistantState } from "../store/assistant";
import { deriveOrbActivity, designFor, settleActivity, type OrbActivity } from "./orbActivity";
import type { OrbDesign } from "./dotted/resolveOrb";

export interface OrbLevelInputs {
  ttsActive: boolean;
  audioVolume: number;
  micLevel: number;
  /** TTS envelope, or null when none is available (local/system voice). */
  envelope: number | null;
}

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);

/**
 * Voice level (0..1) that swells the dots: your voice while listening, NEXUS's own voice while speaking.
 * Every other activity is a steady animation with no voice coupling.
 */
export function orbLevel(activity: OrbActivity, i: OrbLevelInputs): number {
  if (activity === "listening") return clamp01(Math.max(i.micLevel, i.audioVolume));
  if (activity === "speaking") {
    if (i.envelope !== null) return clamp01(i.envelope);
    return i.ttsActive ? clamp01(Math.max(0.4, i.audioVolume)) : 0;
  }
  return 0;
}

/** Snapshot read inside the animation loop (never re-renders React). */
export function readOrbLevel(activity: OrbActivity): number {
  const s = useAssistant.getState();
  const envelope = speechStageActive() ? getSpeechStageEnvelopeLevel() ?? 0 : getEnvelopeLevel();
  return orbLevel(activity, { ttsActive: s.ttsActive, audioVolume: s.audioVolume, micLevel: s.micLevel, envelope });
}

/**
 * The activity the orb shows and the design for it. `state` is the turn state as Avatar wants it shown
 * (already ghost-pinned). A changed activity waits out MIN_DWELL_MS so rapid flips never flicker.
 */
export function useOrbActivity(state: AssistantState): { activity: OrbActivity; design: OrbDesign } {
  const ghostActive = useAssistant((s) => s.ghostActive);
  const awaitingInput = useAssistant((s) => s.awaitingInput);
  const signals = useAssistant((s) => s.orbSignals);
  const designs = useAssistant((s) => s.orbDesigns);

  const target = deriveOrbActivity({ ...signals, state, ghostActive, awaitingInput });

  const [shown, setShown] = useState<OrbActivity>(target);
  const sinceRef = useRef(performance.now());
  const shownRef = useRef(shown);
  shownRef.current = shown;

  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    const apply = () => {
      const r = settleActivity(shownRef.current, target, sinceRef.current, performance.now());
      if (r.activity !== shownRef.current) {
        shownRef.current = r.activity;
        sinceRef.current = performance.now();
        setShown(r.activity);
      }
      if (r.waitMs > 0) timer = setTimeout(apply, r.waitMs);
    };
    apply();
    return () => {
      if (timer) clearTimeout(timer);
    };
  }, [target]);

  return { activity: shown, design: designFor(shown, designs) };
}
