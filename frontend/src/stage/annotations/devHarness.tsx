// Dev-only visual harness for the tour annotation kit. Served by the Vite dev server at
// /dev-annotations.html; it is NOT a build input, so it never ships. Each tile draws one style
// over a fake page so the look, the draw-on animation and light/dark can be checked by eye.
//
// Second section: "Full overlay (mocked Tauri events)" tests the complete TourOverlay component
// driven by simulated screen tour IPC events.

import "./mockTauriBridge";
import { emit } from "./mockTauriBridge";
import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import "../tour.css";
import { Annotation } from "./Annotation";
import { DRAW_STYLES, DrawStyle } from "./drawKit";
import { TourOverlay } from "../TourOverlay";

const W = 560;
const H = 320;
const target = { x: 150, y: 120, w: 190, h: 90 };
const link = { x: 380, y: 200, w: 130, h: 70 };
// 1x1 grey-blue gradient-ish jpeg-free stand-in for the lens crop.
const thumb =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="190" height="90"><rect width="190" height="90" fill="#2b3a55"/><text x="12" y="52" font-size="28" fill="#fff" font-family="sans-serif">$4.99 / mo</text></svg>`,
  );

const CATEGORIES = [
  "ui_element",
  "text_block",
  "code",
  "chart",
  "diagram",
  "person",
  "product",
  "food",
  "object",
  "other",
] as const;

const INTENTS = [
  "explain",
  "find",
  "compare",
  "summarize",
  "decide",
  "debug",
] as const;

const CALLOUT_STEPS = [
  {
    title: "Starter plan",
    text: "Entry tier at $4.99 a month with one project.",
    x: 180,
    y: 160,
    width: 220,
    height: 110,
  },
  {
    title: "Pro plan",
    text: "Unlimited collaboration, priority queue, and advanced reasoning models.",
    x: 520,
    y: 280,
    width: 180,
    height: 90,
  },
  {
    title: "Analytics & Usage",
    text: "Track token usage, cost ceilings and latency across all local sessions.",
    x: 240,
    y: 480,
    width: 380,
    height: 80,
  },
  {
    title: "Header navigation",
    text: "Quickly switch workspaces and search recent command history.",
    x: 740,
    y: 160,
    width: 280,
    height: 140,
  },
];

function FakePage({ fullscreen }: { fullscreen?: boolean }) {
  return (
    <div className={`dev-page${fullscreen ? " dev-page--fullscreen" : ""}`}>
      <div className="dev-nav" />
      <div className="dev-hero">Pricing that scales with you</div>
      <div className="dev-card dev-card--a">Starter — $4.99</div>
      <div className="dev-card dev-card--b">Pro — $19</div>
      <div className="dev-line" />
      <div className="dev-line dev-line--short" />
    </div>
  );
}

function Tile({ style, replay }: { style: DrawStyle; replay: number }) {
  return (
    <figure className="dev-tile">
      <figcaption>{style}</figcaption>
      <div className="dev-stage" key={`${style}-${replay}`}>
        <FakePage />
        <svg className="tour-svg in dev-svg" width={W} height={H}>
          <Annotation
            style={style}
            target={target}
            link={style === "connect" ? link : null}
            thumb={style === "lens" ? thumb : null}
            viewport={{ w: W, h: H }}
            live
            idx={2}
          />
        </svg>
      </div>
    </figure>
  );
}

function FullOverlayPreview({
  theme,
  onToggleTheme,
  onToggleView,
}: {
  theme: "dark" | "light";
  onToggleTheme: () => void;
  onToggleView: () => void;
}) {
  const [style, setStyle] = useState<DrawStyle>("ring");
  const [category, setCategory] = useState<string>("ui_element");
  const [intent, setIntent] = useState<string>("explain");
  const [calloutIdx, setCalloutIdx] = useState(0);

  const handleStartTour = () => {
    setCalloutIdx(0);
    emit("screen:tour_start", {
      request_id: "dev",
      screen_w: 1280,
      screen_h: 800,
    });
  };

  const handleNextCallout = () => {
    const curIdx = calloutIdx % CALLOUT_STEPS.length;
    const step = CALLOUT_STEPS[curIdx];
    emit("screen:callout", {
      request_id: "dev",
      idx: curIdx,
      total: 4,
      title: step.title,
      text: step.text,
      category,
      draw: style,
      intent,
      x: step.x,
      y: step.y,
      width: step.width,
      height: step.height,
      has_target: true,
      link: style === "connect" ? { x: 520, y: 280, width: 180, height: 90 } : null,
      thumb: style === "lens" ? thumb : null,
    });
    setCalloutIdx((prev) => prev + 1);
  };

  const handleClear = () => {
    emit("screen:callout_clear", { request_id: "dev" });
  };

  const handleEndTour = () => {
    emit("screen:tour_end", { request_id: "dev" });
  };

  return (
    <section className="dev-full-overlay-section" aria-label="Full overlay (mocked Tauri events)">
      <div className="dev-full-controls">
        <span className="dev-controls-title">Full overlay (mocked Tauri events)</span>
        <button onClick={onToggleView}>Show tile grid</button>
        <button onClick={onToggleTheme}>Theme: {theme}</button>
        <label>
          Style:
          <select value={style} onChange={(e) => setStyle(e.target.value as DrawStyle)}>
            {DRAW_STYLES.map((s) => (
              <option key={s} value={s}>
                {s}
              </option>
            ))}
          </select>
        </label>
        <label>
          Category:
          <select value={category} onChange={(e) => setCategory(e.target.value)}>
            {CATEGORIES.map((c) => (
              <option key={c} value={c}>
                {c}
              </option>
            ))}
          </select>
        </label>
        <label>
          Intent:
          <select value={intent} onChange={(e) => setIntent(e.target.value)}>
            {INTENTS.map((i) => (
              <option key={i} value={i}>
                {i}
              </option>
            ))}
          </select>
        </label>
        <button onClick={handleStartTour}>Start tour</button>
        <button onClick={handleNextCallout}>Next callout</button>
        <button onClick={handleClear}>Clear</button>
        <button onClick={handleEndTour}>End tour</button>
      </div>
      <div className="dev-fake-screen-wrap">
        <div className="dev-fake-screen">
          <FakePage fullscreen />
          <TourOverlay />
        </div>
      </div>
    </section>
  );
}

function App() {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [replay, setReplay] = useState(0);
  const [showFullOverlay, setShowFullOverlay] = useState(false);

  document.documentElement.setAttribute("data-theme", theme);

  return (
    <main className="dev-root">
      {showFullOverlay ? (
        <FullOverlayPreview
          theme={theme}
          onToggleTheme={() => setTheme(theme === "dark" ? "light" : "dark")}
          onToggleView={() => setShowFullOverlay(false)}
        />
      ) : (
        <>
          <header>
            <h1>Annotation kit</h1>
            <button onClick={() => setShowFullOverlay(true)}>
              Show full-overlay preview
            </button>
            <button onClick={() => setTheme(theme === "dark" ? "light" : "dark")}>
              Theme: {theme}
            </button>
            <button onClick={() => setReplay(replay + 1)}>Replay</button>
          </header>
          <section className="dev-grid">
            {DRAW_STYLES.map((s) => (
              <Tile key={s} style={s} replay={replay} />
            ))}
          </section>
        </>
      )}
    </main>
  );
}

const css = `
  body { margin: 0; background: #0b0d12; color: #e6e9f0; font-family: system-ui, sans-serif; }
  [data-theme="light"] body { background: #eef1f6; color: #111827; }
  .dev-root { padding: 16px; }
  header { display: flex; gap: 12px; align-items: center; margin-bottom: 12px; }
  header h1 { font-size: 16px; margin: 0 12px 0 0; }
  button { padding: 6px 12px; border-radius: 8px; border: 1px solid #556; background: #1b2030; color: inherit; cursor: pointer; }
  button:hover { background: #272f45; }
  [data-theme="light"] button { background: #e2e8f0; border-color: #cbd5e1; color: #111827; }
  [data-theme="light"] button:hover { background: #cbd5e1; }
  .dev-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(${W}px, 1fr)); gap: 16px; }
  .dev-tile { margin: 0; }
  .dev-tile figcaption { font: 600 12px/1 monospace; margin: 0 0 6px; opacity: .8; }
  .dev-stage { position: relative; width: ${W}px; height: ${H}px; overflow: hidden; border-radius: 10px; background: #f4f5f7; }
  .dev-svg.tour-svg { position: absolute; inset: 0; }
  .dev-page { position: absolute; inset: 0; padding: 14px; color: #223; }
  .dev-nav { height: 18px; background: #d6dae3; border-radius: 4px; }
  .dev-hero { font: 700 22px/1.2 system-ui; margin: 14px 0; }
  .dev-card { position: absolute; background: #fff; border: 1px solid #cfd4de; border-radius: 10px; padding: 12px; font: 600 15px system-ui; }
  .dev-card--a { left: 150px; top: 120px; width: 166px; height: 66px; }
  .dev-card--b { left: 380px; top: 200px; width: 106px; height: 46px; }
  .dev-line { height: 10px; margin-top: 150px; background: #d6dae3; border-radius: 4px; }
  .dev-line--short { width: 40%; margin-top: 8px; }

  /* Full overlay preview styling */
  .dev-full-overlay-section {
    position: fixed;
    inset: 0;
    overflow: hidden;
    z-index: 1000;
  }
  .dev-full-controls {
    position: fixed;
    top: 14px;
    left: 14px;
    z-index: 100000;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    background: rgba(16, 20, 30, 0.92);
    backdrop-filter: blur(8px);
    padding: 8px 14px;
    border-radius: 10px;
    border: 1px solid rgba(255, 255, 255, 0.2);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.4);
    font: 500 13px/1.2 system-ui, sans-serif;
    color: #fff;
  }
  [data-theme="light"] .dev-full-controls {
    background: rgba(255, 255, 255, 0.94);
    border-color: rgba(0, 0, 0, 0.15);
    color: #111827;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.1);
  }
  .dev-controls-title {
    font-weight: 700;
    margin-right: 6px;
    opacity: 0.9;
  }
  .dev-full-controls label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    font-weight: 600;
  }
  .dev-full-controls select {
    padding: 5px 8px;
    border-radius: 6px;
    border: 1px solid #556;
    background: #1b2030;
    color: inherit;
    font: inherit;
  }
  [data-theme="light"] .dev-full-controls select {
    background: #f1f3f7;
    border-color: #cbd5e1;
    color: #111827;
  }
  .dev-fake-screen-wrap {
    position: fixed;
    inset: 0;
    transform: translateZ(0);
    overflow: hidden;
  }
  .dev-fake-screen {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }
  .dev-page--fullscreen {
    position: fixed;
    inset: 0;
    padding: 32px 48px;
    background: #f4f5f7;
    color: #111827;
    box-sizing: border-box;
    z-index: 0;
  }
  [data-theme="dark"] .dev-page--fullscreen {
    background: #0f131c;
    color: #e2e8f0;
  }
  .dev-page--fullscreen .dev-card {
    position: absolute;
    border-radius: 12px;
    padding: 16px;
    font: 600 16px/1.4 system-ui;
    box-sizing: border-box;
  }
  .dev-page--fullscreen .dev-card--a {
    left: calc(180 / 1280 * 100vw);
    top: calc(160 / 800 * 100vh);
    width: calc(220 / 1280 * 100vw);
    height: calc(110 / 800 * 100vh);
  }
  .dev-page--fullscreen .dev-card--b {
    left: calc(520 / 1280 * 100vw);
    top: calc(280 / 800 * 100vh);
    width: calc(180 / 1280 * 100vw);
    height: calc(90 / 800 * 100vh);
  }
`;
const styleEl = document.createElement("style");
styleEl.textContent = css;
document.head.appendChild(styleEl);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
