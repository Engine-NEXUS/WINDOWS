/** The wake-sound candidates (synthesized in Rust, `src-tauri/src/wake_cue.rs`). Keys must match `Style::key`. */
export interface WakeSound {
  key: "rise" | "glass" | "pluck" | "triplet" | "sweep";
  label: string;
  blurb: string;
}

export const WAKE_SOUNDS: WakeSound[] = [
  { key: "rise", label: "Rise", blurb: "Two soft bells going up - 'ready'" },
  { key: "glass", label: "Glass", blurb: "One clean ping, the gentlest" },
  { key: "pluck", label: "Pluck", blurb: "Short warm note, good in a quiet room" },
  { key: "triplet", label: "Triplet", blurb: "Three quick notes, easiest to notice from across the room" },
  { key: "sweep", label: "Sweep", blurb: "Rising glide into a bell, sci-fi" },
];

export const DEFAULT_WAKE_SOUND = "rise";
export const DEFAULT_WAKE_VOLUME = 70;

/** A stored value that is not one of the five falls back to the default, like the Rust side. */
export function normalizeWakeSound(value: unknown): WakeSound["key"] {
  const found = WAKE_SOUNDS.find((s) => s.key === String(value ?? "").trim().toLowerCase());
  return found ? found.key : DEFAULT_WAKE_SOUND;
}

export function clampWakeVolume(value: unknown): number {
  const n = Number(value);
  if (!Number.isFinite(n)) return DEFAULT_WAKE_VOLUME;
  return Math.max(0, Math.min(100, Math.round(n)));
}
