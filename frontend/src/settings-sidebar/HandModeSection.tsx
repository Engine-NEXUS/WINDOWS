import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SwitchCard } from "../theme/OptionCard";

type HandKey =
  | "handMode"
  | "handMousePreview"
  | "handWithGhost"
  | "handSnap"
  | "handShowCamera"
  | "handGestures"
  | "handGesturesDesktop"
  | "handFocusMode"
  | "handFocusLowerApps";

interface Props {
  settings: Partial<Record<HandKey, boolean>>;
  update: (key: HandKey, value: boolean) => void;
}

const ROWS: {
  key: HandKey;
  icon: string;
  label: string;
  desc: string;
  fallback: boolean;
}[] = [
  {
    key: "handMode",
    icon: "✋",
    label: "Allow Hand Tracking",
    desc: "Use camera-based hand tracking for spatial gesture control.",
    fallback: true,
  },
  {
    key: "handMousePreview",
    icon: "🎯",
    label: "Pointer Preview Only",
    desc: "Display tracking crosshair without registering clicks.",
    fallback: false,
  },
  {
    key: "handSnap",
    icon: "🧲",
    label: "Magnetic Snap",
    desc: "Smart snapping to nearest buttons and interactive controls.",
    fallback: true,
  },
  {
    key: "handGestures",
    icon: "📑",
    label: "Tab & Window Gestures",
    desc: "Three-finger pinch and swipe for app and tab switching.",
    fallback: true,
  },
  {
    key: "handGesturesDesktop",
    icon: "🖥️",
    label: "Virtual Desktop Gestures",
    desc: "Four-finger horizontal swipe across virtual workspaces.",
    fallback: false,
  },
  {
    key: "handShowCamera",
    icon: "📷",
    label: "Camera Skeleton Overlay",
    desc: "Show real-time visual landmarks and 21-point hand skeleton.",
    fallback: false,
  },
  {
    key: "handWithGhost",
    icon: "👻",
    label: "Auto-Start with Ghost Mode",
    desc: "Automatically activate hand control when entering Ghost mode.",
    fallback: false,
  },
  {
    key: "handFocusMode",
    icon: "⚡",
    label: "Focus mode (pause background work while Hand Mode is on)",
    desc: "Pause unneeded background tasks to keep cursor motion glitch-free.",
    fallback: true,
  },
  {
    key: "handFocusLowerApps",
    icon: "📉",
    label: "Lower other heavy apps while Hand Mode is on (restored exactly afterwards)",
    desc: "Reduce CPU priority and efficiency class of heavy apps so the camera pointer stays smooth.",
    fallback: true,
  },
];

/** Modern boxed option cards for hand mode (camera pointer). */
export function HandModeSection({ settings, update }: Props) {
  const [active, setActive] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    invoke<{ active: boolean }>("hand_status")
      .then((s) => setActive(s.active))
      .catch(() => {});
  }, []);

  const toggleMaster = async (nextVal: boolean) => {
    setBusy(true);
    try {
      if (!nextVal) {
        await invoke("hand_stop");
        setActive(false);
      } else {
        await invoke("hand_start");
        setActive(true);
      }
    } catch (e) {
      console.error("hand mode toggle failed:", e);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-section" style={{ border: "none", padding: 0 }}>
      <div
        className="settings-section-title"
        style={{
          fontSize: "12px",
          fontWeight: 700,
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          color: "var(--hx-dim)",
          marginBottom: "10px",
          display: "flex",
          alignItems: "center",
          gap: "6px",
        }}
      >
        <span>✋</span> Hand & Gesture Control
      </div>

      {/* Master Active Status Switch Card */}
      <SwitchCard
        icon={active ? "🟢" : "⚪"}
        title={active ? "Hand Tracking Active" : "Hand Tracking Inactive"}
        desc="Real-time spatial pointer and gesture recognition engine."
        badge={active ? "LIVE" : "OFF"}
        checked={active}
        disabled={busy || (!active && settings.handMode === false)}
        onChange={toggleMaster}
        className={active ? "is-selected" : ""}
      />

      {/* Individual Feature Switch Cards */}
      <div style={{ marginTop: "10px" }}>
        {ROWS.map((r) => {
          const on = settings[r.key] ?? r.fallback;
          return (
            <SwitchCard
              key={r.key}
              icon={r.icon}
              title={r.label}
              desc={r.desc}
              checked={on}
              onChange={(next) => update(r.key, next)}
            />
          );
        })}
      </div>
    </div>
  );
}
