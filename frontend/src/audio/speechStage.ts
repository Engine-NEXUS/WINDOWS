import { Channel, invoke } from "@tauri-apps/api/core";
import { useAssistant } from "../store/assistant";
import { CaptionLineEvent, CaptionTrack, partitionIntoLines, setPlaybackTimelineEnabled } from "./captionScheduler";

export interface SpeechFrame {
  utterance_id: number;
  seq: number;
  emitted_at_unix_ms: number;
  phase: "prepared" | "chunk" | "playing" | "finished" | "cancelled" | "failed";
  position_ms: number;
  caption?: CaptionTrack | null;
  request_id?: string | null;
  source_center: string;
  kind: string;
}

interface SpeechSnapshot {
  frame: SpeechFrame | null;
  tracks: CaptionTrack[];
}

type Line = ReturnType<typeof partitionIntoLines>[number] & { estimated: boolean };
const listeners = new Set<(line: CaptionLineEvent | null) => void>();
let initialized = false;
let registering = false;
let registrationAttempts = 0;
let activeId = 0;
let blockedThrough = 0;
let lastSeq = 0;
let phase: SpeechFrame["phase"] | null = null;
let positionMs = 0;
let positionAt = 0;
let lines: Line[] = [];
let tracks: CaptionTrack[] = [];
let finishedAt = 0;
let raf = 0;
let lastText = "";
let currentLine: CaptionLineEvent | null = null;
let orbReady = false;
let captionReady = false;
let ackPending = 0;
let lagSamples: number[] = [];
let droppedUpdates = 0;

function reportCompletion(frame: SpeechFrame) {
  const sorted = [...lagSamples].sort((a, b) => a - b);
  const p95 = sorted.length ? sorted[Math.ceil(sorted.length * 0.95) - 1] : 0;
  const displayedClause = lines.findIndex((line, index) =>
    line.start_ms <= positionMs && (index === lines.length - 1 || lines[index + 1].start_ms > positionMs));
  console.info("[SPEECH] playback receipt", {
    utteranceId: frame.utterance_id,
    requestId: frame.request_id,
    sourceCenter: frame.source_center,
    kind: frame.kind,
    completion: frame.phase,
    positionMs: frame.position_ms,
    displayedClause,
    orbVisible: useAssistant.getState().visible,
    eventLagP95Ms: p95,
    droppedUpdates,
  });
}

function maybeAcknowledge() {
  if (!ackPending || !orbReady || !captionReady) return;
  const id = ackPending;
  ackPending = 0;
  requestAnimationFrame(() => requestAnimationFrame(() => {
    if (activeId === id && phase === "prepared" && orbReady && captionReady) {
      void invoke("speech_stage_ack", { utteranceId: id });
    }
  }));
}

export function setSpeechOrbReady(ready: boolean) {
  orbReady = ready;
  maybeAcknowledge();
}

export function setSpeechCaptionReady(ready: boolean) {
  captionReady = ready;
  maybeAcknowledge();
}

export function speechStageActive(): boolean {
  return activeId > 0 && phase !== null;
}

export function speechStageRegistered(): boolean {
  return initialized;
}

export function onSpeechStageLine(fn: (line: CaptionLineEvent | null) => void): () => void {
  listeners.add(fn);
  fn(currentLine);
  return () => listeners.delete(fn);
}

function publishLine(line: CaptionLineEvent | null) {
  if (line?.text === lastText && line?.phase === currentLine?.phase) return;
  lastText = line?.text ?? "";
  currentLine = line;
  listeners.forEach((fn) => fn(line));
}

function rebuildLines() {
  lines = tracks.flatMap((track) =>
    partitionIntoLines(track.words).map((line) => ({ ...line, estimated: track.estimated })),
  ).sort((a, b) => a.start_ms - b.start_ms);
}

function decodeWord(text: string): string {
  return text.replace(/&apos;/g, "'").replace(/&quot;/g, '"')
    .replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">");
}

/** Pure caption projection from the measured playback cursor. */
export function captionAtPosition(all: Line[], pos: number, finished = false): string {
  if (!all.length) return "";
  let low = 0;
  let high = all.length - 1;
  let selected = -1;
  while (low <= high) {
    const mid = (low + high) >> 1;
    if (all[mid].start_ms <= pos) { selected = mid; low = mid + 1; }
    else high = mid - 1;
  }
  if (finished) selected = all.length - 1;
  if (selected < 0) return "";
  const line = all[selected];
  if (line.estimated || finished) return line.text;
  let text = "";
  for (const word of line.words) {
    if (word.start_ms > pos) break;
    const clean = decodeWord(word.text);
    const fraction = Math.min(1, Math.max(0, pos - word.start_ms) / Math.max(80, word.duration_ms));
    const count = Math.max(1, Math.ceil(clean.length * fraction));
    text += `${text ? " " : ""}${clean.slice(0, count)}`;
  }
  return text;
}

function cursor(): number {
  if (phase !== "playing") return positionMs;
  // Extrapolate at most one display frame; the next measured rodio cursor
  // corrects this. A blocked WebView cannot run far ahead of audible audio.
  return positionMs + Math.min(50, Math.max(0, performance.now() - positionAt));
}

export function getSpeechStageEnvelopeLevel(): number | null {
  if (phase !== "playing") return null;
  const pos = cursor();
  for (const track of tracks) {
    const start = track.envelope_start_ms ?? 0;
    const end = start + (track.envelope?.length ?? 0) * track.frame_ms;
    if (track.frame_ms > 0 && pos >= start && pos < end) {
      const index = Math.min(track.envelope.length - 1, Math.floor((pos - start) / track.frame_ms));
      return track.envelope[index] ?? null;
    }
  }
  return 0;
}

function paint() {
  raf = 0;
  if (!phase) return;
  if (phase === "prepared") {
    publishLine({ text: "…", phase: "active" });
    raf = requestAnimationFrame(paint);
    return;
  }
  const pos = cursor();
  const text = captionAtPosition(lines, pos, phase === "finished");
  if (text) publishLine({ text, phase: "active" });
  if (phase === "finished" && performance.now() - finishedAt >= 2000) {
    useAssistant.getState().setCaptionActive(false);
    publishLine(null);
    phase = null;
    return;
  }
  if (phase === "cancelled" || phase === "failed") {
    useAssistant.getState().setCaptionActive(false);
    publishLine(null);
    phase = null;
    return;
  }
  raf = requestAnimationFrame(paint);
}

function ensurePaint() {
  if (!raf) raf = requestAnimationFrame(paint);
}

export function acceptSpeechFrame(frame: SpeechFrame, restoreTracks?: CaptionTrack[]) {
  const restoredAgeMs = restoreTracks && frame.emitted_at_unix_ms > 0
    ? Math.max(0, Date.now() - frame.emitted_at_unix_ms)
    : 0;
  if (restoreTracks && (frame.phase === "cancelled" || frame.phase === "failed"
      || (frame.phase === "finished" && restoredAgeMs >= 2000))) {
    blockedThrough = Math.max(blockedThrough, frame.utterance_id);
    useAssistant.getState().setTtsActive(false);
    useAssistant.getState().setCaptionActive(false);
    publishLine(null);
    return;
  }
  if (frame.utterance_id <= blockedThrough) return;
  if (frame.utterance_id < activeId || (frame.utterance_id === activeId && frame.seq <= lastSeq)) return;
  if (frame.utterance_id === activeId
      && (phase === "finished" || phase === "cancelled" || phase === "failed")
      && (frame.phase === "prepared" || frame.phase === "chunk" || frame.phase === "playing")) return;
  const fresh = frame.utterance_id !== activeId;
  if (fresh) {
    lagSamples = [];
    droppedUpdates = 0;
    activeId = frame.utterance_id;
    positionMs = 0;
    positionAt = 0;
    tracks = [];
    lines = [];
    publishLine(null);
  } else if (frame.seq > lastSeq + 1) {
    droppedUpdates += frame.seq - lastSeq - 1;
  }
  if (frame.emitted_at_unix_ms > 0 && lagSamples.length < 512) {
    lagSamples.push(Math.max(0, Date.now() - frame.emitted_at_unix_ms));
  }
  lastSeq = frame.seq;
  phase = frame.phase;
  if (restoreTracks) tracks = restoreTracks;
  else if (frame.phase === "prepared") tracks = frame.caption ? [frame.caption] : [];
  else if (frame.phase === "chunk" && frame.caption) {
    if (tracks.length === 128) tracks.shift();
    tracks.push(frame.caption);
  }
  if (restoreTracks || frame.caption) rebuildLines();
  if (frame.phase === "prepared") {
    positionMs = 0;
    useAssistant.setState({ visible: true, state: "speaking", captionActive: true });
    // Both stage elements must have geometry before two paint frames can
    // acknowledge readiness to the Rust player.
    ackPending = frame.utterance_id;
    maybeAcknowledge();
  } else if (frame.phase === "playing") {
    positionMs = Math.max(positionMs, frame.position_ms);
    positionAt = performance.now();
    const assistant = useAssistant.getState();
    if (!assistant.visible || assistant.state !== "speaking" || !assistant.ttsActive || !assistant.captionActive) {
      useAssistant.setState({ visible: true, state: "speaking", ttsActive: true, captionActive: true });
    }
  } else if (frame.phase === "finished") {
    positionMs = frame.position_ms;
    finishedAt = performance.now() - restoredAgeMs;
    useAssistant.getState().setTtsActive(false);
    reportCompletion(frame);
  } else if (frame.phase === "cancelled" || frame.phase === "failed") {
    useAssistant.getState().setTtsActive(false);
    reportCompletion(frame);
  }
  ensurePaint();
}

export function clearSpeechStage() {
  blockedThrough = Math.max(blockedThrough, activeId);
  if (raf) cancelAnimationFrame(raf);
  raf = 0;
  phase = null;
  tracks = [];
  lines = [];
  positionMs = 0;
  useAssistant.getState().setTtsActive(false);
  useAssistant.getState().setCaptionActive(false);
  publishLine(null);
}

export async function initSpeechStage(): Promise<void> {
  if (initialized || registering) return;
  registering = true;
  registrationAttempts += 1;
  const channel = new Channel<SpeechFrame>();
  channel.onmessage = (frame) => acceptSpeechFrame(frame);
  try {
    const snapshot = await invoke<SpeechSnapshot>("speech_stage_register", { onFrame: channel });
    initialized = true;
    registrationAttempts = 0;
    setPlaybackTimelineEnabled(true);
    if (snapshot.frame) acceptSpeechFrame(snapshot.frame, snapshot.tracks);
    if (speechStageActive() && phase !== "cancelled" && phase !== "failed") {
      useAssistant.getState().setCaptionActive(true);
    }
  } catch (error) {
    console.warn("[SPEECH] Stage timeline registration failed", error);
    if (registrationAttempts < 6) {
      setTimeout(() => void initSpeechStage(), 1000);
    }
  } finally {
    registering = false;
  }
}
