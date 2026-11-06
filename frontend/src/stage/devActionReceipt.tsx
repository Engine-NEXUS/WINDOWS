import { useState, useEffect } from "react";
import { createRoot } from "react-dom/client";
import "./ghost.css";
import "../theme/tokens.css";
import {
  ActionReceiptModel,
  type ActionReceipt as ActionReceiptType,
} from "./actionReceiptModel";
import { ActionReceiptCard } from "./ActionReceipt";

export function DevActionReceipt() {
  const [model] = useState(() => new ActionReceiptModel());
  const [receipt, setReceipt] = useState<ActionReceiptType | null>(null);
  const [visible, setVisible] = useState(false);

  const handleYouTube = () => {
    const state = model.onReceipt({
      kind: "search",
      headline: "Opened",
      subject: "YouTube",
      title: "lofi beats",
      detail: "Search results. Nothing is playing yet.",
      ok: true,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleNotepad = () => {
    const state = model.onReceipt({
      kind: "open_app",
      headline: "Opened",
      subject: "Notepad",
      title: null,
      detail: null,
      ok: true,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleVSCode = () => {
    const state = model.onReceipt({
      kind: "open_app",
      headline: "Opened",
      subject: "Visual Studio Code",
      title: "src/stage/ActionReceipt.tsx",
      detail: null,
      ok: true,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleFooTool = () => {
    const state = model.onReceipt({
      kind: "open_app",
      headline: "Launched",
      subject: "Foo Tool",
      title: null,
      detail: "Unknown app monogram fallback",
      ok: true,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleGithub = () => {
    const state = model.onReceipt({
      kind: "open_site",
      headline: "Opened",
      subject: "github.com",
      title: "Engine-NEXUS / ULTRON",
      detail: null,
      ok: true,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleFail = () => {
    const state = model.onReceipt({
      kind: "open_app",
      headline: "Tried to open",
      subject: "Notepad",
      title: null,
      detail: "I could not confirm it came to the front.",
      ok: false,
    });
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  const handleCancel = () => {
    const state = model.onCancel();
    setReceipt(state.receipt);
    setVisible(state.visible);
  };

  useEffect(() => {
    const interval = setInterval(() => {
      const state = model.tick();
      if (!state.visible && visible) {
        setVisible(false);
      }
    }, 200);
    return () => clearInterval(interval);
  }, [model, visible]);

  return (
    <div>
      <h2>Action Receipt Preview (dev only)</h2>
      <div className="controls">
        <button id="btn-youtube" onClick={handleYouTube}>
          YouTube
        </button>
        <button id="btn-notepad" onClick={handleNotepad}>
          Notepad
        </button>
        <button id="btn-vscode" onClick={handleVSCode}>
          Visual Studio Code
        </button>
        <button id="btn-foo-tool" onClick={handleFooTool}>
          Foo Tool
        </button>
        <button id="btn-github" onClick={handleGithub}>
          github.com
        </button>
        <button id="btn-fail" onClick={handleFail}>
          tried to open (fail)
        </button>
        <button id="btn-cancel" onClick={handleCancel}>
          cancel
        </button>
      </div>

      <div className="stage-preview">
        {visible && receipt ? (
          <ActionReceiptCard receipt={receipt} />
        ) : (
          <span style={{ color: "var(--stage-text-muted)" }}>
            [Action receipt hidden]
          </span>
        )}
      </div>
    </div>
  );
}

const rootEl = document.getElementById("root");
if (rootEl) {
  createRoot(rootEl).render(<DevActionReceipt />);
}
