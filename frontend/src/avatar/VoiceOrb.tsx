import React, { useEffect, useRef } from "react";
import "./voice-orb.js";

export type VoiceOrbState = "idle" | "listening" | "thinking" | "speaking" | "text";

export interface VoiceOrbElement extends HTMLElement {
  state: VoiceOrbState;
  particles: number;
  setLevel: (vol: number) => void;
  play: () => void;
  pause: () => void;
  assemble: () => void;
  disperse: () => void;
  /** Particle-generated text: the SAME cloud particles converge into glyphs. */
  setText: (text: string, holdMs?: number) => boolean;
  connect: (source: MediaStream | AudioNode | HTMLMediaElement) => Promise<VoiceOrbElement>;
  disconnect: () => void;
  renderAt: (time: number, opts?: unknown) => void;
}

export interface VoiceOrbProps {
  state: VoiceOrbState;
  particles?: number;
  level?: number;
  visible?: boolean;
  /** Ghost enter: particles snap scattered then assemble into the circle. */
  entered?: boolean;
  /** Particle-generated text (overrides the state visual until it dissolves). */
  text?: string | null;
  className?: string;
  style?: React.CSSProperties;
}

// Augment JSX IntrinsicElements for the <voice-orb> Web Component
declare global {
  namespace JSX {
    interface IntrinsicElements {
      "voice-orb": React.DetailedHTMLProps<
        React.HTMLAttributes<HTMLElement> & {
          state?: string;
          particles?: number | string;
          recording?: boolean;
        },
        HTMLElement
      >;
    }
  }
}

/**
 * High-performance WebGL 3D Voice Orb Component (Ship Notes)
 *
 * ONE continuous particle simulation morphing across 5 states:
 * - 'idle': Calm, breathing lilac/purple sphere
 * - 'listening': Turquoise inward-pulling wave, voice-reactive
 * - 'thinking': Braided toroidal knot (contract → stretch → intertwine)
 * - 'speaking': Radiant pink/violet outward blast with audio reactivity
 * - 'text': The SAME particles detach, converge into readable glyphs,
 *   hold, then dissolve back into the previous state
 */
export const VoiceOrb: React.FC<VoiceOrbProps> = ({
  state,
  particles = 5000,
  level = 0,
  visible = true,
  entered = false,
  text,
  className,
  style,
}) => {
  const orbRef = useRef<VoiceOrbElement | null>(null);

  useEffect(() => {
    if (orbRef.current) {
      orbRef.current.state = state;
    }
  }, [state]);

  useEffect(() => {
    if (orbRef.current) {
      if (visible) {
        orbRef.current.play();
      } else {
        const t = setTimeout(() => {
          orbRef.current?.pause();
        }, 500);
        return () => clearTimeout(t);
      }
    }
  }, [visible]);

  useEffect(() => {
    if (entered && orbRef.current) {
      orbRef.current.assemble();
    }
  }, [entered]);

  useEffect(() => {
    if (orbRef.current && typeof level === "number" && level > 0) {
      orbRef.current.setLevel(level);
    }
  }, [level]);

  // Particle text via prop (static callers).
  useEffect(() => {
    if (orbRef.current && typeof text === "string" && text.trim()) {
      orbRef.current.setText(text);
    }
  }, [text]);

  // Particle text via event (live path): short spoken lines emitted at the
  // ttsPlayer choke point reach every window's orb.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<{ text?: string }>("orb:show_text", (ev) => {
          const t = ev.payload?.text;
          if (t && t.trim()) orbRef.current?.setText(t);
        });
      } catch {
        // Outside Tauri (tests) — no-op
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <voice-orb
      ref={orbRef as React.RefObject<HTMLElement>}
      state={state}
      particles={particles}
      className={className}
      style={{
        display: "block",
        width: "100%",
        height: "100%",
        minWidth: "120px",
        minHeight: "120px",
        aspectRatio: "1",
        contain: "layout paint",
        ...style,
      }}
    />
  );
};
