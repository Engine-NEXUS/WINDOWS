// Dev-only preview: the orb for every NEXUS activity, with the particle MORPH between them.
// Click an activity, or press "cycle all" to watch every transition in order; "slow motion" stretches the morph
// and the animation clock so each transition can be inspected frame by frame.
import { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { DottedOrb } from "./DottedOrb";
import { MORPH_MS } from "./morph";
import { DEFAULT_ORB_DESIGNS, ORB_ACTIVITIES, type OrbActivity } from "../orbActivity";

const CYCLE_MS = 2600;

function Capsule({ dark, children }: { dark: boolean; children: React.ReactNode }) {
  return (
    <div
      style={{
        width: 170,
        height: 130,
        borderRadius: 18,
        background: dark ? "#000" : "#fafafa",
        border: "1px solid " + (dark ? "#2b2b30" : "#d6d6dc"),
        boxShadow: "0 12px 34px rgba(0,0,0,.45)",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      {children}
    </div>
  );
}

function Demo() {
  const [active, setActive] = useState<OrbActivity>("idle");
  const [cycling, setCycling] = useState(false);
  const [slow, setSlow] = useState(false);
  const [voice, setVoice] = useState(true);
  const [mic, setMic] = useState(0);
  const micRef = useRef(0);
  micRef.current = mic;
  const levelRef = useRef(0);
  const idx = useRef(0);

  useEffect(() => {
    if (!cycling) return;
    const t = setInterval(() => {
      idx.current = (idx.current + 1) % ORB_ACTIVITIES.length;
      setActive(ORB_ACTIVITIES[idx.current].id);
    }, CYCLE_MS * (slow ? 2.4 : 1));
    return () => clearInterval(t);
  }, [cycling, slow]);

  useEffect(() => {
    let raf = 0;
    const tick = () => {
      const t = performance.now() / 1000;
      if (voice) {
        // talk for about 2.4 s, then 1.8 s of silence, so you can see the ring hold still when nothing is heard
        const phase = t % 4.2;
        const talking = phase < 2.4;
        levelRef.current = talking ? 0.45 + 0.4 * Math.abs(Math.sin(t * 7)) * (0.6 + 0.4 * Math.sin(t * 1.7)) : 0;
      } else {
        levelRef.current = micRef.current;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [voice]);

  const design = DEFAULT_ORB_DESIGNS[active];
  const getLevel = useMemo(() => () => levelRef.current, []);
  const btn = (on: boolean) => ({
    padding: "6px 12px",
    borderRadius: 6,
    border: "1px solid " + (on ? "#8ab4ff" : "#3a3a42"),
    background: on ? "#3a3a52" : "#1e1e24",
    color: "#fff",
    cursor: "pointer",
    fontSize: 13,
  });

  return (
    <div>
      <h2 style={{ margin: "0 0 4px", fontSize: 18 }}>Orb states and transitions</h2>
      <p style={{ margin: "0 0 14px", color: "#cdd3de", fontSize: 13 }}>
        Idle is a plain circle; Listening is the wobbling ring and only moves while the mic hears sound (use the mic slider or the talk / silence loop); speaking is the composing sash. Changing state morphs the particles into the next shape
        (about {MORPH_MS} ms), no flip.
      </p>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 12 }}>
        {ORB_ACTIVITIES.map((a) => (
          <button
            key={a.id}
            style={btn(active === a.id)}
            onClick={() => {
              setCycling(false);
              setActive(a.id);
            }}
          >
            {a.label}
          </button>
        ))}
      </div>
      <div style={{ display: "flex", flexWrap: "wrap", gap: 8, marginBottom: 18, alignItems: "center" }}>
        <button style={btn(cycling)} onClick={() => setCycling(!cycling)}>
          {cycling ? "stop cycling" : "cycle all"}
        </button>
        <button style={btn(slow)} onClick={() => setSlow(!slow)}>
          {slow ? "slow motion on" : "slow motion off"}
        </button>
        <button style={btn(voice)} onClick={() => setVoice(!voice)}>
          {voice ? "mic: talk / silence loop" : "mic: manual slider"}
        </button>
        <label style={{ fontSize: 13, display: "flex", gap: 6, alignItems: "center" }}>
          mic level {mic.toFixed(2)}
          <input type="range" min={0} max={1} step={0.01} value={mic} disabled={voice} onChange={(e) => setMic(Number(e.target.value))} />
        </label>
        <span style={{ fontSize: 14 }}>
          now: <b>{ORB_ACTIVITIES.find((a) => a.id === active)?.label}</b> &rarr; design <b>{design}</b>
        </span>
      </div>
      <div style={{ display: "flex", gap: 24, flexWrap: "wrap" }}>
        <Capsule dark>
          <DottedOrb design={design} size={100} dark speed={slow ? 0.35 : 1} morphMs={slow ? MORPH_MS * 4 : MORPH_MS} getLevel={getLevel} voiceDriven={active === "listening"} />
        </Capsule>
        <Capsule dark={false}>
          <DottedOrb design={design} size={100} dark={false} speed={slow ? 0.35 : 1} morphMs={slow ? MORPH_MS * 4 : MORPH_MS} getLevel={getLevel} voiceDriven={active === "listening"} />
        </Capsule>
      </div>
    </div>
  );
}

const root = document.getElementById("root");
if (root) createRoot(root).render(<Demo />);
