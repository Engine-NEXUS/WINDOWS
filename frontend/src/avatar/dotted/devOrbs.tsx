// Dev-only preview: all nine thinking-orbs designs at the real orb size, dark and light, plus the
// "activity -> design" table the owner fills in before anything is wired into the stage.
import { useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { DottedOrb } from "./DottedOrb";
import { ORB_DESIGNS, type OrbDesign } from "./resolveOrb";

const DESCRIPTIONS: Record<OrbDesign, string> = {
  breathing: "a face-on ring slowly morphing",
  listening: "a waveform rolls through the rings",
  working: "particles on tilted orbits",
  searching: "a scan meridian sweeps a dotted globe",
  connecting: "a constellation wires itself",
  weaving: "three strands plait around the sphere",
  composing: "an undulating multi-band sash",
  solving: "bands scramble, then click back solved",
  shaping: "dotted outline: circle, triangle, square",
};

const ACTIVITIES: { id: string; label: string; suggestion: OrbDesign }[] = [
  { id: "idle", label: "Idle", suggestion: "breathing" },
  { id: "listening", label: "Listening to you", suggestion: "listening" },
  { id: "thinking", label: "Thinking", suggestion: "working" },
  { id: "speaking", label: "Speaking", suggestion: "composing" },
  { id: "background", label: "Doing tasks in the background", suggestion: "weaving" },
  { id: "analysing", label: "Analysing the screen", suggestion: "searching" },
  { id: "ghost", label: "Ghost mode", suggestion: "connecting" },
  { id: "hand", label: "Hand mode", suggestion: "shaping" },
  { id: "dictation", label: "Dictation / typing for you", suggestion: "shaping" },
  { id: "agent", label: "Watching a coding agent", suggestion: "solving" },
  { id: "waiting", label: "Waiting for your answer", suggestion: "breathing" },
];

function Pane({ dark, size, speed, getLevel, design }: { dark: boolean; size: number; speed: number; getLevel: () => number; design: OrbDesign }) {
  return (
    <div
      style={{
        background: dark ? "#000" : "#f2f2f4",
        border: "1px solid " + (dark ? "#2b2b30" : "#d6d6dc"),
        borderRadius: 14,
        width: size + 28,
        height: size + 28,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <DottedOrb design={design} size={size} dark={dark} speed={speed} getLevel={getLevel} />
    </div>
  );
}

function DevOrbs() {
  const [size, setSize] = useState(120);
  const [speed, setSpeed] = useState(1);
  const [level, setLevel] = useState(0);
  const [auto, setAuto] = useState(false);
  const levelRef = useRef(0);
  levelRef.current = level;
  const [mapping, setMapping] = useState<Record<string, OrbDesign>>(() =>
    Object.fromEntries(ACTIVITIES.map((a) => [a.id, a.suggestion])),
  );

  useEffect(() => {
    if (!auto) return;
    let raf = 0;
    const tick = () => {
      const t = performance.now() / 1000;
      levelRef.current = 0.5 + 0.5 * Math.sin(t * 5) * Math.sin(t * 1.3);
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [auto]);

  const getLevel = () => levelRef.current;
  const label = { display: "flex", gap: 8, alignItems: "center", fontSize: 13 } as const;

  return (
    <div>
      <h1>Orb designs (thinking-orbs engine, MIT)</h1>
      <p className="note">Pick a design for each activity in the table at the bottom. Dark = the black capsule, light = light theme.</p>
      <div style={{ display: "flex", gap: 24, flexWrap: "wrap", marginBottom: 20 }}>
        <label style={label}>
          size {size}px
          <input type="range" min={64} max={200} value={size} onChange={(e) => setSize(+e.target.value)} />
        </label>
        <label style={label}>
          speed {speed.toFixed(2)}x
          <input type="range" min={0.4} max={2.5} step={0.05} value={speed} onChange={(e) => setSpeed(+e.target.value)} />
        </label>
        <label style={label}>
          voice level {level.toFixed(2)}
          <input type="range" min={0} max={1} step={0.01} value={level} disabled={auto} onChange={(e) => setLevel(+e.target.value)} />
        </label>
        <label style={label}>
          <input type="checkbox" checked={auto} onChange={(e) => setAuto(e.target.checked)} /> simulate speech
        </label>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(" + (size * 2 + 90) + "px, 1fr))", gap: 18 }}>
        {ORB_DESIGNS.map((d) => (
          <div key={d} style={{ background: "#16161b", border: "1px solid #25252c", borderRadius: 12, padding: 12 }}>
            <div style={{ fontWeight: 700, fontSize: 14 }}>{d}</div>
            <div style={{ color: "#9aa0aa", fontSize: 12, marginBottom: 8 }}>{DESCRIPTIONS[d]}</div>
            <div style={{ display: "flex", gap: 10 }}>
              <Pane dark size={size} speed={speed} getLevel={getLevel} design={d} />
              <Pane dark={false} size={size} speed={speed} getLevel={getLevel} design={d} />
            </div>
          </div>
        ))}
      </div>

      <h1 style={{ marginTop: 32 }}>Activity to design</h1>
      <p className="note">Defaults are my suggestion. Change any row, then tell me the list (or paste the JSON below).</p>
      <div style={{ display: "grid", gap: 8, maxWidth: 560 }}>
        {ACTIVITIES.map((a) => (
          <div key={a.id} style={{ display: "flex", alignItems: "center", gap: 12, background: "#16161b", border: "1px solid #25252c", borderRadius: 10, padding: "6px 10px" }}>
            <div style={{ background: "#000", borderRadius: 10, width: 76, height: 76, display: "flex", alignItems: "center", justifyContent: "center" }}>
              <DottedOrb design={mapping[a.id]} size={64} dark speed={speed} getLevel={getLevel} />
            </div>
            <div style={{ flex: 1, fontSize: 14 }}>{a.label}</div>
            <select value={mapping[a.id]} onChange={(e) => setMapping({ ...mapping, [a.id]: e.target.value as OrbDesign })}>
              {ORB_DESIGNS.map((d) => (
                <option key={d} value={d}>
                  {d}
                </option>
              ))}
            </select>
          </div>
        ))}
      </div>
      <pre style={{ marginTop: 16, background: "#16161b", padding: 12, borderRadius: 8, fontSize: 12, maxWidth: 560, overflow: "auto" }}>
        {JSON.stringify(mapping, null, 2)}
      </pre>
    </div>
  );
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<DevOrbs />);
