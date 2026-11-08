// Dev-only preview of the two caption changes:
//  1. NEXUS's reply caption: OLD (letters only inside each word's own time slot, so words pop in) next to NEW
//     (one steady typing rate across the whole line), both driven by the same fake word-timing track.
//  2. Your mic words in the speech pill, typed letter by letter after you stop speaking.
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import "./ghost.css";
import { typedText, type TimedWord } from "../audio/typedCaption";
import { typedSlice, typingProgress } from "./speechPillModel";

// A believable Edge-voice track: short words are quick, long ones slower, small gaps between words.
const SENTENCE = ["Opening", "YouTube", "for", "you,", "sir.", "Searching", "lofi", "beats", "now."];
const DURATIONS = [420, 380, 120, 160, 240, 440, 200, 260, 300];
const WORDS: TimedWord[] = (() => {
  let at = 250;
  return SENTENCE.map((text, i) => {
    const w = { text, start_ms: at, duration_ms: DURATIONS[i] };
    at += DURATIONS[i] + 70;
    return w;
  });
})();
// The local voice has no real word timing: words are spread evenly over the audio.
const LOCAL_WORDS: TimedWord[] = SENTENCE.map((text, i) => ({ text, start_ms: 250 + i * 400, duration_ms: 400 }));
const END_MS = WORDS[WORDS.length - 1].start_ms + WORDS[WORDS.length - 1].duration_ms + 600;

/** The previous behaviour, kept only for this comparison: letters appear inside each word's own slot. */
function oldCaptionAt(words: TimedWord[], pos: number): string {
  let text = "";
  for (const word of words) {
    if (word.start_ms > pos) break;
    const fraction = Math.min(1, Math.max(0, pos - word.start_ms) / Math.max(80, word.duration_ms));
    const count = Math.max(1, Math.ceil(word.text.length * fraction));
    text += `${text ? " " : ""}${word.text.slice(0, count)}`;
  }
  return text;
}

function Caption({ text }: { text: string }) {
  return (
    <div style={{ minHeight: 64, display: "flex", alignItems: "center", justifyContent: "center" }}>
      {text ? (
        <div className="caption-black-capsule caption-black-capsule--active" style={{ position: "static" }}>
          <span className="caption-black-text">{text}</span>
        </div>
      ) : null}
    </div>
  );
}

const MIC_SENTENCE = "open youtube and search lofi beats";

function Demo() {
  const [pos, setPos] = useState(-400);
  const [playing, setPlaying] = useState(false);
  const startRef = useRef(0);
  const [slow, setSlow] = useState(false);
  const [local, setLocal] = useState(false);

  useEffect(() => {
    if (!playing) return;
    let raf = 0;
    startRef.current = performance.now() - (pos + 400) * (slow ? 3 : 1);
    const tick = () => {
      const p = (performance.now() - startRef.current) / (slow ? 3 : 1) - 400;
      setPos(p);
      if (p < END_MS) raf = requestAnimationFrame(tick);
      else setPlaying(false);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing, slow]);

  // Mic pill typing demo
  const [micAt, setMicAt] = useState<number | null>(null);
  const [micNow, setMicNow] = useState(0);
  useEffect(() => {
    if (micAt === null) return;
    let raf = 0;
    const tick = () => {
      const t = performance.now() - micAt;
      setMicNow(t);
      if (typingProgress(0, MIC_SENTENCE.length, t) < MIC_SENTENCE.length) raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [micAt]);
  const micText = micAt === null ? "" : typedSlice(MIC_SENTENCE, typingProgress(0, MIC_SENTENCE.length, micNow));

  const btn = {
    padding: "6px 12px",
    borderRadius: 6,
    border: "1px solid #3a3a42",
    background: "#1e1e24",
    color: "#fff",
    cursor: "pointer",
    fontSize: 13,
  } as const;
  const panel = { background: "#16161b", border: "1px solid #25252c", borderRadius: 12, padding: 14, flex: 1, minWidth: 320 } as const;

  return (
    <div>
      <h2 style={{ margin: "0 0 4px", fontSize: 18 }}>Captions: letter by letter</h2>
      <p style={{ margin: "0 0 14px", color: "#cdd3de", fontSize: 13 }}>
        Same spoken line, same timing. Left: what you have now (words pop in). Right: one steady typing rate.
      </p>
      <div style={{ display: "flex", gap: 8, marginBottom: 14 }}>
        <button
          style={btn}
          onClick={() => {
            setPos(-400);
            setPlaying(true);
          }}
        >
          play reply
        </button>
        <button style={btn} onClick={() => setLocal(!local)}>
          {local ? "voice: local (estimated timing)" : "voice: Edge (real timing)"}
        </button>
        <button style={btn} onClick={() => setSlow(!slow)}>
          {slow ? "slow motion on (x3)" : "slow motion off"}
        </button>
        <span style={{ alignSelf: "center", fontSize: 13 }}>{Math.max(0, Math.round(pos))} ms</span>
      </div>
      <div style={{ display: "flex", gap: 10, alignItems: "center", marginBottom: 14, fontSize: 13 }}>
        scrub
        <input
          aria-label="scrub the reply"
          type="range"
          min={0}
          max={END_MS}
          step={5}
          value={Math.max(0, Math.min(END_MS, Math.round(pos)))}
          style={{ width: 420 }}
          onChange={(e) => {
            setPlaying(false);
            setPos(Number(e.target.value));
          }}
        />
      </div>
      <div style={{ display: "flex", gap: 16, flexWrap: "wrap", marginBottom: 26 }}>
        <div style={panel}>
          <div style={{ fontWeight: 700, marginBottom: 8 }}>Before (word bursts)</div>
          <Caption text={pos >= 0 ? (local ? SENTENCE.join(" ") : oldCaptionAt(WORDS, pos)) : ""} />
        </div>
        <div style={panel}>
          <div style={{ fontWeight: 700, marginBottom: 8 }}>After (letter by letter)</div>
          <Caption text={pos >= 0 ? typedText(local ? LOCAL_WORDS : WORDS, pos) : ""} />
        </div>
      </div>

      <h2 style={{ margin: "0 0 4px", fontSize: 18 }}>Your mic words</h2>
      <p style={{ margin: "0 0 14px", color: "#cdd3de", fontSize: 13 }}>
        After you stop speaking the pill types what you said, letter by letter (about 55 letters a second, never more
        than 1.2 s).
      </p>
      <button style={btn} onClick={() => setMicAt(performance.now())}>
        simulate: you said &quot;{MIC_SENTENCE}&quot;
      </button>
      <div style={{ marginTop: 16, minHeight: 56, display: "flex", justifyContent: "center" }}>
        {micText ? (
          <div className="speech-pill" role="status">
            <svg className="speech-pill-mic" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
              <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
              <line x1="12" y1="19" x2="12" y2="22" />
            </svg>
            <span className="speech-pill-text">{micText}</span>
          </div>
        ) : null}
      </div>
    </div>
  );
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<Demo />);
