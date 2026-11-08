import { useEffect, useRef, useState } from "react";
import { onCaptionLineUpdate } from "../audio/captionScheduler";
import { onSpeechStageLine } from "../audio/speechStage";
import { useAssistant } from "../store/assistant";
import { speechPillSlot } from "./layoutSlots";
import {
  FINAL_LINGER_MS,
  formatFinalText,
  formatPartialText,
} from "./speechPillModel";

interface OrbRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/**
 * SpeechPill — rounded pill with a small mic glyph followed by the user's spoken words.
 *
 * Replaces LiveCaption:
 * - On `stt:partial`: 5-word sliding window while the user is speaking.
 * - On `stt:transcript`: full final text (capped at ~12 words with leading ellipsis)
 *   kept visible until the reply starts or 4 s pass. Empty text hides immediately.
 * - Hides when the response caption or TTS starts, and on `orb:cancel`.
 * - Shares the caption slot family (layoutSlots.ts), yielding to the response caption.
 */
export function SpeechPill() {
  const [rect, setRect] = useState<OrbRect | null>(null);
  const [text, setText] = useState("");
  const [visible, setVisible] = useState(false);
  const [replyActive, setReplyActive] = useState(false);

  const captionActive = useAssistant((s) => s.captionActive);
  const ttsActive = useAssistant((s) => s.ttsActive);
  const isReplying = replyActive || captionActive || ttsActive;

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pillRef = useRef<HTMLDivElement | null>(null);

  const clearTimer = () => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  };

  // Response caption line update listener (same signal ResponseCaption uses)
  useEffect(() => {
    const unsubCaption = onCaptionLineUpdate((event, done) => {
      if (event.phase !== "cleared" && event.text && event.text.trim()) {
        setReplyActive(true);
        clearTimer();
        setVisible(false);
      } else if (done && event.phase === "cleared") {
        setReplyActive(false);
      }
    });

    const unsubStage = onSpeechStageLine((event) => {
      if (event && event.phase !== "cleared" && event.text && event.text.trim()) {
        setReplyActive(true);
        clearTimer();
        setVisible(false);
      } else {
        setReplyActive(false);
      }
    });

    return () => {
      unsubCaption();
      unsubStage();
    };
  }, []);

  // When TTS or response caption becomes active, hide the speech pill immediately
  useEffect(() => {
    if (isReplying) {
      clearTimer();
      setVisible(false);
    }
  }, [isReplying]);

  // STT partial, transcript and orb:cancel listeners
  useEffect(() => {
    let unlistenPartial: (() => void) | null = null;
    let unlistenTranscript: (() => void) | null = null;
    let unlistenCancel: (() => void) | null = null;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");

      unlistenPartial = await listen<unknown>("stt:partial", (event) => {
        clearTimer();
        const formatted = formatPartialText(event.payload);
        if (formatted) {
          setText(formatted);
          setVisible(true);
        }
      });

      unlistenTranscript = await listen<unknown>("stt:transcript", (event) => {
        clearTimer();
        const formatted = formatFinalText(event.payload);
        if (!formatted) {
          // Empty or phantom turns hide immediately
          setText("");
          setVisible(false);
        } else {
          setText(formatted);
          setVisible(true);
          // Keep visible until reply starts or 4 s pass
          timerRef.current = setTimeout(() => {
            setVisible(false);
            timerRef.current = null;
          }, FINAL_LINGER_MS);
        }
      });

      unlistenCancel = await listen("orb:cancel", () => {
        clearTimer();
        setText("");
        setVisible(false);
      });
    })();

    return () => {
      unlistenPartial?.();
      unlistenTranscript?.();
      unlistenCancel?.();
      clearTimer();
    };
  }, []);

  // Stage orb rect tracking
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<OrbRect>("stage:orb_rect", (event) => {
        if (event.payload) setRect(event.payload);
      });
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const pending = await invoke<OrbRect | null>("get_pending_orb_rect");
        if (pending) setRect(pending);
      } catch {
        /* non-Tauri (tests) — no-op */
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // Direct DOM mutation for positioning (CSP-safe, inline style prop is dropped)
  const applyPosition = (el: HTMLDivElement | null) => {
    if (!el || !rect) return;
    const dpr = window.devicePixelRatio || 1;
    const topY = rect.y / dpr;
    const isTop = topY < (typeof window !== "undefined" ? window.innerHeight / 2 : 540);

    if (isTop) {
      const slot = speechPillSlot({ w: window.innerWidth, h: window.innerHeight });
      const centerX = slot.x + slot.w / 2;
      el.style.transform = `translate(${centerX}px, ${slot.y}px) translateX(-50%)`;
    } else {
      const centerX = typeof window !== "undefined" ? window.innerWidth / 2 : rect.x / dpr + rect.w / dpr / 2;
      const bottomY = (rect.y + rect.h) / dpr;
      el.style.transform = `translate(${centerX}px, ${bottomY + 20}px) translateX(-50%)`;
    }
  };

  useEffect(() => {
    applyPosition(pillRef.current);
  }, [rect, text, visible]);

  if (!rect || !visible || !text.trim() || isReplying) {
    return null;
  }

  return (
    <div
      className="speech-pill-container"
      ref={(el) => {
        pillRef.current = el;
        applyPosition(el);
      }}
    >
      <div className="speech-pill" role="status" aria-live="polite">
        <svg
          className="speech-pill-mic"
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
          <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
          <line x1="12" y1="19" x2="12" y2="22" />
        </svg>
        <span className="speech-pill-text">{text}</span>
      </div>
    </div>
  );
}
