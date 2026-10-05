import React, { useEffect, useRef } from "react";
import "./voice-orb.js";
import { getEnvelopeLevel } from "../audio/captionScheduler";

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
 * - 'thinking': 3D rotating purple beaded starburst — 64 radial rays with
 *   dense white-magenta nucleus, concentric beaded steps, dual-axis 3D
 *   tumbling rotation, and dynamic outward energy waves
 * - 'speaking': Dense irregular potato/pebble blob, molded by NEXUS's
 *   real voice-amplitude beat (sub-phase C2), with particle sparks
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

  // Real TTS-audio beat sync (sub-phase C2): while speaking, pump NEXUS's
  // own voice-amplitude envelope (Rust's `tts:caption.envelope`, scheduled
  // by captionScheduler.ts) into setLevel() every frame — this takes
  // priority over the `level` prop's mic-volume-proxy effect above
  // whenever a real envelope value is available for the current instant.
  // Falls back to doing nothing (letting the `level` prop keep driving)
  // the moment no envelope is available, so speaking visuals never go
  // flat just because this signal is momentarily missing.
  useEffect(() => {
    if (state !== "speaking") return;
    let raf = 0;
    const pump = () => {
      const v = getEnvelopeLevel();
      if (v !== null && orbRef.current) orbRef.current.setLevel(v);
      raf = requestAnimationFrame(pump);
    };
    raf = requestAnimationFrame(pump);
    return () => {
      if (raf) cancelAnimationFrame(raf);
    };
  }, [state]);

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
      ref={(el: VoiceOrbElement | null) => {
        orbRef.current = el;
        // Base sizing lives in the `.voice-orb-el` CSS class (styles.css),
        // not here, for the same reason as Avatar.tsx's wrapper: the stage
        // window's CSP makes React's `style` prop a no-op (see OrbFrame.tsx's
        // note). Any caller-supplied `style` override (no current caller
        // passes one, but it's part of this component's public props) is
        // still honored, applied imperatively instead.
        if (el && style) {
          Object.assign(el.style, style);
        }
      }}
      state={state}
      particles={particles}
      className={`voice-orb-el${className ? ` ${className}` : ""}`}
    />
  );
};
