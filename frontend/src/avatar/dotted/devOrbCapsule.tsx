// Dev-only: the real <Avatar> inside the real capsule styles, driven through every activity by the store.
import { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "../../theme/tokens.css";
import "../../styles.css";
import { Avatar } from "../Avatar";
import { useAssistant } from "../../store/assistant";
import { NO_SIGNALS, ORB_ACTIVITIES, deriveOrbActivity, type OrbActivity } from "../orbActivity";

type Preset = Partial<ReturnType<typeof useAssistant.getState>> & { signals?: Partial<typeof NO_SIGNALS> };

const PRESETS: Record<OrbActivity, Preset> = {
  idle: { state: "idle" },
  listening: { state: "listening" },
  thinking: { state: "thinking" },
  speaking: { state: "speaking" },
  waiting: { state: "speaking", awaitingInput: true },
  analysing: { state: "thinking", signals: { tourFetching: true } },
  ghost: { state: "idle", ghostActive: true },
  hand: { state: "idle", signals: { handActive: true } },
  dictation: { state: "idle", signals: { dictationActive: true } },
  agent: { state: "idle", signals: { agentWatching: true } },
  background: { state: "idle", signals: { backgroundTasks: 2 } },
};

function Demo() {
  const [picked, setPicked] = useState<OrbActivity>("idle");
  const [light, setLight] = useState(false);
  const [sim, setSim] = useState(true);

  useEffect(() => {
    const p = PRESETS[picked];
    useAssistant.setState({
      visible: true,
      ttsActive: p.state === "speaking",
      state: p.state ?? "idle",
      ghostActive: p.ghostActive ?? false,
      awaitingInput: p.awaitingInput ?? false,
      orbSignals: { ...NO_SIGNALS, ...(p.signals ?? {}) },
    });
  }, [picked]);

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", light ? "light" : "dark");
  }, [light]);

  useEffect(() => {
    if (!sim) {
      useAssistant.setState({ micLevel: 0, audioVolume: 0 });
      return;
    }
    const t = setInterval(() => {
      const v = 0.5 + 0.5 * Math.sin(performance.now() / 180) * Math.sin(performance.now() / 700);
      useAssistant.setState({ micLevel: v, audioVolume: v });
    }, 60);
    return () => clearInterval(t);
  }, [sim]);

  const s = useAssistant(); // subscribe, so the label follows the store
  const derived = deriveOrbActivity({ ...s.orbSignals, state: s.state, ghostActive: s.ghostActive, awaitingInput: s.awaitingInput });

  return (
    <div>
      <div className="row">
        {ORB_ACTIVITIES.map((a) => (
          <button key={a.id} className={picked === a.id ? "on" : ""} onClick={() => setPicked(a.id)}>
            {a.label}
          </button>
        ))}
      </div>
      <div className="row">
        <button onClick={() => setLight(!light)}>{light ? "light theme" : "dark theme"}</button>
        <button onClick={() => setSim(!sim)}>{sim ? "voice sim on" : "voice sim off"}</button>
        <span>derived: {derived}</span>
      </div>
      <div style={{ width: 301, height: 105, position: "relative" }}>
        <div className="orb-capsule" style={{ width: 301, height: 105, position: "relative", display: "flex", alignItems: "center", justifyContent: "center" }}>
          <div className="orb-cradle-wrapper" style={{ position: "relative" }}>
            <div className="orb-sphere-wrapper">
              <Avatar entered shown />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<Demo />);
