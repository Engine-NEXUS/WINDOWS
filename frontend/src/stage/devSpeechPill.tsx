import { useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import "./ghost.css";
import "../theme/tokens.css";
import { SpeechPillModel } from "./speechPillModel";

export function DevSpeechPill() {
  const [model] = useState(() => new SpeechPillModel());
  const [text, setText] = useState("");
  const [visible, setVisible] = useState(false);
  const [partialIdx, setPartialIdx] = useState(0);

  const partialStreams = [
    "turn",
    "turn on",
    "turn on the",
    "turn on the kitchen",
    "turn on the kitchen ceiling",
    "turn on the kitchen ceiling lights",
    "turn on the kitchen ceiling lights please",
  ];

  const handlePartial = () => {
    const streamText = partialStreams[partialIdx % partialStreams.length];
    setPartialIdx((i) => i + 1);
    const state = model.onPartial(streamText);
    setText(state.text);
    setVisible(state.visible);
  };

  const handleFinal = () => {
    const state = model.onFinal({
      session: "dev-session",
      text: "turn on the kitchen ceiling lights and set brightness to fifty percent",
    });
    setText(state.text);
    setVisible(state.visible);
  };

  const handleEmpty = () => {
    const state = model.onFinal({ session: "dev-session", text: "" });
    setText(state.text);
    setVisible(state.visible);
  };

  useEffect(() => {
    const interval = setInterval(() => {
      const state = model.tick();
      if (!state.visible && visible) {
        setVisible(false);
        setText("");
      }
    }, 200);
    return () => clearInterval(interval);
  }, [model, visible]);

  return (
    <div>
      <h2>Speech Pill Preview (dev only)</h2>
      <div className="controls">
        <button id="btn-partial" onClick={handlePartial}>
          partial
        </button>
        <button id="btn-final" onClick={handleFinal}>
          final
        </button>
        <button id="btn-empty" onClick={handleEmpty}>
          empty
        </button>
      </div>

      <div className="stage-preview">
        {visible && text ? (
          <div className="speech-pill" role="status">
            <svg
              className="speech-pill-mic"
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
              <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
              <line x1="12" y1="19" x2="12" y2="22" />
            </svg>
            <span className="speech-pill-text">{text}</span>
          </div>
        ) : (
          <span style={{ color: "var(--stage-text-muted)" }}>[Speech pill hidden]</span>
        )}
      </div>
    </div>
  );
}

createRoot(document.getElementById("root")!).render(<DevSpeechPill />);
