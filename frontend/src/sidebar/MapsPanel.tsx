import { useEffect, useState } from "react";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/** Only Google's Maps Embed endpoint may be framed here; anything else (a stale or forged payload) is ignored. */
export function isEmbedSrc(url: unknown): url is string {
  return typeof url === "string" && url.startsWith("https://www.google.com/maps?") && url.endsWith("&output=embed") && !/\s/.test(url);
}

/**
 * The Maps panel: a plain NEXUS window (no title bar, no dock buttons) that frames Google's Maps Embed API.
 * Rust (`maps_panel::show_map`) hands it the embed URL: the fast path is the `maps:show` event, the fresh-window
 * race path is `get_pending_maps` on mount. Placement, order and size are owned by `panel_layout` in Rust.
 */
export function MapsPanel() {
  const [shown, setShown] = useState<{ src: string; n: number } | null>(null);
  // Every request reloads the map, even for the same place (the user may have panned away from it).
  const setSrc = (src: string) => setShown((p) => ({ src, n: (p?.n ?? 0) + 1 }));

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      const pending = await invoke<string | null>("get_pending_maps").catch(() => null);
      if (isEmbedSrc(pending)) setSrc(pending);
      unlisten = await listen<string>("maps:show", (e) => {
        if (isEmbedSrc(e.payload)) setSrc(e.payload);
      });
    })().catch(() => {});
    return () => {
      unlisten?.();
    };
  }, []);

  return (
    <div className="maps-panel">
      <div className="sidebar-drag-strip" data-tauri-drag-region />
      {shown ? (
        <iframe
          className="maps-frame"
          title="Map"
          key={shown.n}
          src={shown.src}
          allowFullScreen
          referrerPolicy="strict-origin-when-cross-origin"
        />
      ) : (
        <div className="maps-empty">Loading map…</div>
      )}
    </div>
  );
}
