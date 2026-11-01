import { useEffect, useState } from "react";
import { formatKm, formatMinutes, formatRupees, isPlaceCard, safeImageSrc, type PlaceCardData } from "./placeCardModel";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/**
 * The place card: a 315 px NEXUS panel beside the map. Rust (`place_card::show_card`) hands it the data: the fast path
 * is the `place:show` event, the fresh-window race path is `get_pending_place` on mount. Placement and height are owned
 * by `panel_layout` in Rust; the body scrolls when the screen is short.
 */
export function PlaceCard() {
  const [card, setCard] = useState<PlaceCardData | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      const pending = await invoke<unknown>("get_pending_place").catch(() => null);
      if (isPlaceCard(pending)) setCard(pending);
      unlisten = await listen<unknown>("place:show", (e) => {
        if (isPlaceCard(e.payload)) setCard(e.payload);
      });
    })().catch(() => {});
    return () => {
      unlisten?.();
    };
  }, []);

  const openMaps = () => {
    if (!isTauri()) return;
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("place_card_open", { kind: "maps" }).catch(() => {}));
  };
  const openUber = () => {
    if (!isTauri()) return;
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("place_card_open", { kind: "uber" }).catch(() => {}));
  };
  const close = () => {
    if (!isTauri()) return;
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("close_place_card").catch(() => {}));
  };

  if (!card) {
    return (
      <div className="place-card">
        <div className="sidebar-drag-strip" data-tauri-drag-region />
        <div className="place-empty">Loading place…</div>
      </div>
    );
  }

  const photo = safeImageSrc(card.photo?.dataUri);
  const fares = card.fares ?? [];
  return (
    <div className="place-card" role="region" aria-label={`Place: ${card.name}`}>
      <div className="sidebar-drag-strip" data-tauri-drag-region />
      <button className="place-close" onClick={close} aria-label="Close place card" title="Close (Esc)">
        ×
      </button>
      <div className="place-scroll">
        {photo ? (
          <figure className="place-photo">
            <img src={photo} alt={card.photo?.kind === "map" ? `Map of ${card.name}` : `Photo of ${card.name}`} />
            <figcaption>
              {card.photo?.kind === "map" ? "Map image · " : ""}
              {card.photo?.credit ?? ""}
            </figcaption>
          </figure>
        ) : (
          <div className="place-photo place-photo--none" aria-hidden="true" />
        )}

        <h2 className="place-name">{card.name}</h2>
        <div className="place-sub">
          {card.category && <span>{card.category}</span>}
          {typeof card.rating === "number" && <span>★ {card.rating.toFixed(1)}</span>}
          {card.openNow != null && <span className={card.openNow ? "place-open" : "place-closed"}>{card.openNow ? "Open now" : "Closed now"}</span>}
        </div>

        {card.distanceKm != null ? (
          <div className="place-trip" aria-live="polite">
            <div className="place-distance">
              {card.travelApprox ? "about " : ""}
              {formatKm(card.distanceKm)} <span>from you</span>
            </div>
            <div className="place-chips">
              {card.carMin != null && (
                <div className="place-chip">
                  <b>Car</b>
                  {formatMinutes(card.carMin)}
                </div>
              )}
              {card.bikeMin != null && (
                <div className="place-chip">
                  <b>Bike</b>
                  {card.bikeApprox ? "about " : ""}
                  {formatMinutes(card.bikeMin)}
                </div>
              )}
            </div>
            {(card.travelApprox || card.bikeApprox) && <div className="place-note">{card.travelApprox ? "Estimate, no live traffic." : "Bike time is an estimate."}</div>}
          </div>
        ) : (
          <div className="place-note">Distance unavailable: your location is not known.</div>
        )}

        {card.address && (
          <section className="place-row">
            <h3>Address</h3>
            <p>{card.address}</p>
          </section>
        )}

        {fares.length > 0 && (
          <section className="place-row">
            <h3>Ride estimate</h3>
            <ul className="place-fares">
              {fares.map((f) => (
                <li key={f.kind}>
                  <span>{f.label}</span>
                  <b>{formatRupees(f.low, f.high)}</b>
                </li>
              ))}
            </ul>
            <div className="place-note">Estimate, not a live price. Uber, Ola and Rapido do not share prices with us.</div>
          </section>
        )}

        {(card.hoursToday || card.phone) && (
          <section className="place-row">
            {card.hoursToday && (
              <p>
                <span className="place-k">Today</span> {card.hoursToday}
              </p>
            )}
            {card.phone && (
              <p>
                <span className="place-k">Phone</span> {card.phone}
              </p>
            )}
          </section>
        )}
        {card.summary && <p className="place-summary">{card.summary}</p>}
      </div>
      <div className="place-actions">
        <button onClick={openMaps}>Directions</button>
        <button onClick={openUber}>Open in Uber</button>
      </div>
    </div>
  );
}
