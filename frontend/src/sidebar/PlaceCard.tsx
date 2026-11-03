import { useEffect, useRef, useState } from "react";
import { Icon, type IconName } from "./placeIcons";
import { categoryIcon, formatKm, formatMinutes, formatRupees, isPlaceCard, safeImageSrc, siteHost, type PlaceCardData } from "./placeCardModel";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

const FARE_ICON: Record<string, IconName> = { auto: "auto", bike: "bike", cab: "car" };

/** Placeholder shown while the place is being looked up. */
function Skeleton() {
  return (
    <div className="place-card" aria-busy="true" aria-label="Loading place">
      <div className="sidebar-drag-strip" data-tauri-drag-region />
      <div className="place-scroll">
        <div className="pc-skel pc-skel-photo" />
        <div className="pc-skel pc-skel-line pc-skel-w60" />
        <div className="pc-skel pc-skel-line pc-skel-w40" />
        <div className="pc-skel pc-skel-big" />
        <div className="pc-skel-chips">
          <div className="pc-skel pc-skel-chip" />
          <div className="pc-skel pc-skel-chip" />
        </div>
        <div className="pc-skel pc-skel-line" />
        <div className="pc-skel pc-skel-line pc-skel-w80" />
      </div>
    </div>
  );
}

/**
 * The place card: a 315 px NEXUS panel beside the map. Rust (`place_card::show_card`) hands it the data: the fast path
 * is the `place:show` event, the fresh-window race path is `get_pending_place` on mount. Placement and height are owned
 * by `panel_layout` in Rust; the middle scrolls, the header and the action bar stay put.
 */
export function PlaceCard() {
  const [card, setCard] = useState<PlaceCardData | null>(null);
  const [week, setWeek] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<number | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const { listen } = await import("@tauri-apps/api/event");
      const pending = await invoke<unknown>("get_pending_place").catch(() => null);
      if (isPlaceCard(pending)) setCard(pending);
      unlisten = await listen<unknown>("place:show", (e) => {
        if (isPlaceCard(e.payload)) {
          setCard(e.payload);
          setWeek(false);
        }
      });
    })().catch(() => {});
    return () => {
      unlisten?.();
      if (copyTimer.current) window.clearTimeout(copyTimer.current);
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
  const openSite = () => {
    if (!isTauri()) return;
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("place_card_open", { kind: "site" }).catch(() => {}));
  };
  const close = () => {
    if (!isTauri()) return;
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("close_place_card").catch(() => {}));
  };
  const copy = (what: string, text: string) => {
    void navigator.clipboard
      ?.writeText(text)
      .then(() => {
        setCopied(what);
        if (copyTimer.current) window.clearTimeout(copyTimer.current);
        copyTimer.current = window.setTimeout(() => setCopied(null), 1500);
      })
      .catch(() => {});
  };

  if (!card) return <Skeleton />;

  const photo = safeImageSrc(card.photo?.dataUri);
  const fares = card.fares ?? [];
  const host = siteHost(card.website);
  const hours = card.weekdayHours ?? [];
  return (
    <div className="place-card" role="region" aria-label={`Place: ${card.name}`}>
      <div className="sidebar-drag-strip" data-tauri-drag-region />
      <button className="place-close" onClick={close} aria-label="Close place card" title="Close (Esc)">
        <Icon name="x" size={16} />
      </button>
      <div className="place-scroll">
        {photo && (
          <figure className="place-photo">
            <img src={photo} alt={card.photo?.kind === "map" ? `Map of ${card.name}` : `Photo of ${card.name}`} />
            <figcaption>
              {card.photo?.kind === "map" && <b>Map</b>}
              {card.photo?.credit ?? ""}
            </figcaption>
          </figure>
        )}

        <header className="place-head">
          {card.category && (
            <span className="place-pill">
              <Icon name={categoryIcon(card.category) as IconName} size={13} />
              {card.category}
            </span>
          )}
          <h2 className="place-name">{card.name}</h2>
          <div className="place-sub">
            {typeof card.rating === "number" && (
              <span className="place-rating">
                <Icon name="star" size={13} />
                {card.rating.toFixed(1)}
              </span>
            )}
            {card.openNow != null && (
              <span className={card.openNow ? "place-status place-open" : "place-status place-closed"}>
                <i aria-hidden="true" />
                {card.openNow ? "Open now" : "Closed now"}
              </span>
            )}
          </div>
        </header>

        {card.distanceKm != null ? (
          <section className="place-trip" aria-live="polite">
            <div className="place-distance">
              {card.travelApprox ? "about " : ""}
              {formatKm(card.distanceKm)} <span>from you</span>
            </div>
            <div className="place-chips">
              {card.carMin != null && (
                <div className="place-chip">
                  <Icon name="car" size={18} />
                  <div>
                    <small>Car</small>
                    {formatMinutes(card.carMin)}
                  </div>
                </div>
              )}
              {card.bikeMin != null && (
                <div className="place-chip">
                  <Icon name="bike" size={18} />
                  <div>
                    <small>Bike</small>
                    {card.bikeApprox ? "~" : ""}
                    {formatMinutes(card.bikeMin)}
                  </div>
                </div>
              )}
            </div>
            {(card.travelApprox || card.bikeApprox) && (
              <div className="place-note">
                <Icon name="info" size={12} />
                {card.travelApprox ? "Estimate, no live traffic." : "Bike time is an estimate."}
              </div>
            )}
          </section>
        ) : (
          <div className="place-note place-pad">
            <Icon name="info" size={12} />
            Distance unavailable: your location is not known.
          </div>
        )}

        {fares.length > 0 && (
          <section className="place-row">
            <h3>Ride estimate</h3>
            <ul className="place-fares">
              {fares.map((f) => (
                <li key={f.kind}>
                  <Icon name={FARE_ICON[f.kind] ?? "car"} size={16} />
                  <span>{f.label}</span>
                  <b>{formatRupees(f.low, f.high)}</b>
                </li>
              ))}
            </ul>
            <div className="place-note">
              <Icon name="info" size={12} />
              Not live prices. Uber, Ola and Rapido do not share prices with us.
            </div>
          </section>
        )}

        <section className="place-rows">
          {card.address && (
            <div className="place-line">
              <Icon name="pin" />
              <p>{card.address}</p>
              <button onClick={() => copy("address", card.address ?? "")} aria-label="Copy address" title="Copy address">
                <Icon name="copy" size={14} />
              </button>
              {copied === "address" && <em role="status">Copied</em>}
            </div>
          )}
          {card.hoursToday && (
            <div className="place-line">
              <Icon name="clock" />
              <p>
                <span className="place-k">Today</span> {card.hoursToday}
              </p>
              {hours.length > 1 && (
                <button onClick={() => setWeek((w) => !w)} aria-expanded={week} aria-label="Show the week's hours" title="Show the week">
                  <span className={week ? "pc-flip" : ""}>
                    <Icon name="chevron" size={14} />
                  </span>
                </button>
              )}
            </div>
          )}
          {week && hours.length > 1 && (
            <ul className="place-week">
              {hours.map((h) => (
                <li key={h}>{h}</li>
              ))}
            </ul>
          )}
          {card.phone && (
            <div className="place-line">
              <Icon name="phone" />
              <p>{card.phone}</p>
              <button onClick={() => copy("phone", card.phone ?? "")} aria-label="Copy phone number" title="Copy number">
                <Icon name="copy" size={14} />
              </button>
              {copied === "phone" && <em role="status">Copied</em>}
            </div>
          )}
          {host && (
            <div className="place-line">
              <Icon name="globe" />
              <p>{host}</p>
              <button onClick={openSite} aria-label="Open the website" title="Open website">
                <Icon name="directions" size={14} />
              </button>
            </div>
          )}
        </section>
        {card.summary && <p className="place-summary">{card.summary}</p>}
      </div>
      <div className="place-actions">
        <button onClick={openMaps}>
          <Icon name="directions" size={15} />
          Directions
        </button>
        <button onClick={openUber}>
          <Icon name="car" size={15} />
          Uber
        </button>
      </div>
    </div>
  );
}
