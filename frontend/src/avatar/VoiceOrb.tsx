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
  /** No-slide exit (plan §1.3): particles scatter outward instead of a CSS slide-away. */
  dispersing?: boolean;
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
 * - 'idle': Calm, breathing grey sphere
 * - 'listening': Grainy warm amber/brown sphere, calm and voice-reactive
 * - 'thinking': Open flowing multi-strand violet/blue wisps — curl-noise
 *   tendrils reaching outward from a dense core, tapering to a point
 *   (contract → extend on entry; never a closed loop)
 * - 'speaking': Dense bumpy magenta/white blob, irregular lobed silhouette,
 *   audio-reactive with particle sparks
 * - 'text': The SAME particles detach, converge into readable glyphs,
 *   hold, then dissolve back into the previous state
 * A brief glitch/tear burst plays whenever the dominant state changes.
 */
export const VoiceOrb: React.FC<VoiceOrbProps> = ({
  state,
  particles = 5000,
  level = 0,
  visible = true,
  entered = false,
  dispersing = false,
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
    if (dispersing && orbRef.current) {
      orbRef.current.disperse();
    }
  }, [dispersing]);

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
