import { useEffect, useState } from "react";
import "./a11y.css";
import { initCaptionListener, onCaptionLineUpdate } from "../audio/captionScheduler";
import { onSpeechStageLine, speechStageActive } from "../audio/speechStage";
import { buildLiveRegionText, extractCaptionText } from "./liveRegionModel";

/**
 * CaptionLiveRegion (WCAG 2.2 Phase 5):
 * Visually hidden region with `role="status" aria-live="polite" aria-atomic="true"`
 * that mirrors current response caption text and loading state ("Working"),
 * announcing spoken replies and task progress to screen readers.
 */
export function CaptionLiveRegion() {
  const [liveText, setLiveText] = useState("");
  const [captionText, setCaptionText] = useState("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    initCaptionListener();
    const unsub = onCaptionLineUpdate((event, done) => {
      if (speechStageActive()) return;
      if (done && event.phase === "cleared") {
        setCaptionText("");
      } else {
        setCaptionText(extractCaptionText(event));
      }
    });
    const unsubStage = onSpeechStageLine((event) => {
      setCaptionText(extractCaptionText(event));
    });
    return () => {
      unsub();
      unsubStage();
    };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    void (async () => {
      try {
        const { listen } = await import("@tauri-apps/api/event");
        unlisten = await listen<boolean>("stage:loading_visible", (event) => {
          setLoading(event.payload === true);
        });
      } catch {
        /* non-Tauri (tests / browsers) — no-op */
      }
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // The caption grows letter by letter while NEXUS speaks; announcing each step would make a screen reader stutter.
  // Publish the text only once it has stopped changing.
  const text = buildLiveRegionText({ captionText, loading });
  useEffect(() => {
    const id = window.setTimeout(() => setLiveText(text), text === "" ? 0 : 700);
    return () => window.clearTimeout(id);
  }, [text]);

  return (
    <div
      role="status"
      aria-live="polite"
      aria-atomic="true"
      className="sr-only"
    >
      {liveText}
    </div>
  );
}
