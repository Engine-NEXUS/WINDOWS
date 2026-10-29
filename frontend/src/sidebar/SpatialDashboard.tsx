import { useEffect, useState, useCallback } from "react";
import { useSidebar, type SpatialData, type SpatialItemCard } from "./sidebarStore";
import { Lens } from "./Lens";
import { AppBar, Card, Chip, DockIcon, IconButton, LogoTile } from "./kit";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

/**
 * Feature 86 — SpatialDashboard: card-by-card deep dive for the spatial
 * screen analysis. Pins [1]..[n] here match the glowing boxes floating
 * over the desktop stage. Bi-directional sync:
 *   card hover      → stage:highlight_item   (desktop box pulses cyan)
 *   stage pin click → sidebar:focus_spatial_card → this card activates
 *                     + scrolls into view.
 */
export function SpatialView({
  onDock = () => {},
  data,
}: {
  onDock?: (d: string) => void;
  data?: SpatialData | null;
}) {
  const storeData = useSidebar((s) => s.spatialData);
  const spatialData = data !== undefined ? data : storeData;

  // Fresh-window race path: fetch the payload the orchestrator stored
  // before this window existed. Event listener is the fast path.
  useEffect(() => {
    if (!isTauri()) return;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const pending = await invoke<SpatialData | null>("get_pending_spatial").catch(() => null);
      if (pending && (pending.items?.length || pending.review)) {
        useSidebar.getState().showSpatial(pending);
      } else {
        const saved = await invoke<{ spatial: SpatialData; meta: NonNullable<SpatialData["review"]> } | null>(
          "get_last_foreground_review",
        ).catch(() => null);
        if (saved && !useSidebar.getState().spatialData) {
          useSidebar.getState().showSpatial({ ...saved.spatial, review: saved.meta });
        }
      }
    })();
  }, []);

  // Fast path: Rust emits the full payload on analysis completion.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<SpatialData>("sidebar:show_spatial", (ev) => {
        if (ev.payload && (ev.payload.items?.length || ev.payload.review)) {
          useSidebar.getState().showSpatial(ev.payload);
        }
      });
    })().catch(() => {});
    return () => { unlisten?.(); };
  }, []);

  if (!spatialData) {
    return (
      <div className="sidebar-view-col">
        <AppBar
          className="sidebar-header-row"
          logo={<LogoTile name="Nexus" size={36} rounded="lg" />}
          title="Screen Analysis"
          subtitle="Spatial screen analysis"
          actions={
            <>
              <IconButton
                className="sidebar-dock-btn"
                label="Dock Left"
                onClick={() => onDock("left")}
              >
                <DockIcon side="left" />
              </IconButton>
              <IconButton
                className="sidebar-dock-btn"
                label="Dock Right"
                onClick={() => onDock("right")}
              >
                <DockIcon side="right" />
              </IconButton>
            </>
          }
        />
        <div className="spatial-dashboard spatial-dashboard--empty">
          No spatial analysis yet, sir. Say "Analyse the screen".
        </div>
      </div>
    );
  }

  const itemCount = spatialData.items?.length ?? 0;

  return (
    <div className="sidebar-view-col">
      <AppBar
        className="sidebar-header-row"
        logo={<LogoTile name={spatialData.title || "Screen"} size={36} rounded="lg" />}
        title={spatialData.title}
        subtitle={`Screen analysis · ${itemCount} items`}
        actions={
          <>
            <IconButton
              className="sidebar-dock-btn"
              label="Dock Left"
              onClick={() => onDock("left")}
            >
              <DockIcon side="left" />
            </IconButton>
            <IconButton
              className="sidebar-dock-btn"
              label="Dock Right"
              onClick={() => onDock("right")}
            >
              <DockIcon side="right" />
            </IconButton>
          </>
        }
      />
      <SpatialDashboard data={spatialData} onDock={onDock} />
    </div>
  );
}

export function SpatialDashboard({
  data,
}: {
  data: SpatialData;
  onDock?: (d: string) => void;
}) {
  const openImage = useSidebar((s) => s.openImage);
  const [activeCardId, setActiveCardId] = useState<number | null>(null);
  const [folder, setFolder] = useState("");
  const [recreationStatus, setRecreationStatus] = useState("");
  const [starting, setStarting] = useState(false);
  const [deletedReviewId, setDeletedReviewId] = useState<string | null>(null);

  const startRecreation = useCallback(async () => {
    if (!data.review || !folder.trim() || starting) return;
    setStarting(true);
    setRecreationStatus("Starting coding agent…");
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const watchId = await invoke<string>("recreate_review_in_folder", {
        reviewId: data.review.id,
        folder: folder.trim(),
      });
      setRecreationStatus(`Coding task started (${watchId}). NEXUS will report its actual result.`);
    } catch (error) {
      setRecreationStatus(`Could not start: ${String(error)}`);
    } finally {
      setStarting(false);
    }
  }, [data.review, folder, starting]);

  const deleteReview = useCallback(async () => {
    if (!data.review) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("delete_foreground_review", { id: data.review.id });
      setDeletedReviewId(data.review.id);
      setRecreationStatus("Saved reference deleted. Analyse a window again to recreate it.");
    } catch (error) {
      setRecreationStatus(`Could not delete: ${String(error)}`);
    }
  }, [data.review]);

  // Stage pin click → activate + reveal the matching card.
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | null = null;
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ id?: number | null }>("sidebar:focus_spatial_card", (ev) => {
        const id = ev.payload?.id;
        if (typeof id !== "number") return;
        setActiveCardId(id);
        document
          .getElementById(`spatial-card-${id}`)
          ?.scrollIntoView({ behavior: "smooth", block: "nearest" });
      });
    })().catch(() => {});
    return () => { unlisten?.(); };
  }, []);

  const hoverCard = useCallback((id: number | null) => {
    // A lost highlight = dead hover-sync with no other signal.
    void import("../ipc").then(({ emitLogged }) =>
      emitLogged("stage:highlight_item", { id }).catch(() => {}),
    );
  }, []);

  const items = data.items || [];

  return (
    <div className="spatial-dashboard">
      <div className="spatial-scroll-body">
        <div className="spatial-phone-col">
          {/* ── 2. Hero Card ─────────────────────────────────────── */}
          <Card className="spatial-hero-card">
            {data.screen_image_data_url?.startsWith("data:image/jpeg;base64,") && (
              <Lens
                src={data.screen_image_data_url}
                alt="The screen captured for this analysis"
                onOpen={() => openImage(data.screen_image_data_url!, "The screen captured for this analysis")}
              >
                <img
                  className="spatial-screen-image"
                  src={data.screen_image_data_url}
                  alt="The screen captured for this analysis"
                />
              </Lens>
            )}
            {data.overview && <div className="spatial-overview">{data.overview}</div>}
            {data.provider_used && (
              <div className="spatial-hero-provider">
                <Chip tone="neutral">{data.provider_used}</Chip>
              </div>
            )}
          </Card>

          {/* ── Foreground Review Card (if exists) ────────────────── */}
          {data.review && (
            <Card title="Foreground review" className="foreground-review-card">
              <section className="foreground-review" aria-label="Foreground review">
                <div className="foreground-review__meta">
                  <strong>{data.review.target_title || data.review.target_app}</strong>
                  <span>Reference {data.review.id.slice(0, 8)} · {data.review.completeness} · {data.review.persistence}</span>
                </div>
                {data.review.target_url && <div className="foreground-review__source">Foreground URL: {data.review.target_url}</div>}
                <h3>Technology evidence</h3>
                {data.review.technologies.length ? data.review.technologies.map((claim, index) =>
                  <div className="foreground-review__claim" key={`${claim.name}-${index}`}>
                    <strong>{claim.name}</strong> <span>{claim.status}</span>
                    <p>{claim.evidence}</p>
                    <small><a href={claim.source_url} target="_blank" rel="noreferrer">Source: {claim.source_url}</a></small>
                  </div>
                ) : <p>Stack unknown from this evidence. No framework has been assumed.</p>}
                <h3>Main public pages</h3>
                {data.review.pages.length ? data.review.pages.map((page) =>
                  <article className="foreground-review__page" key={page.url}>
                    <strong>{page.title || page.url}</strong>
                    <small>{page.url} · {page.completeness} · {page.method} · {page.captured_at}</small>
                    {page.image_data_url?.startsWith("data:image/png;base64,") && (
                      <Lens
                        src={page.image_data_url}
                        alt={`Captured public page: ${page.title || page.url}`}
                        onOpen={() => openImage(page.image_data_url!, `Captured public page: ${page.title || page.url}`)}
                      >
                        <img src={page.image_data_url} alt={`Captured public page: ${page.title || page.url}`} loading="lazy" />
                      </Lens>
                    )}
                    {page.rea_summary && <p>{page.rea_summary}</p>}
                  </article>
                ) : <p>No public pages inspected. This review uses the foreground screenshot above.</p>}
                {data.review.warnings.length > 0 && <div className="foreground-review__warnings">
                  {data.review.warnings.map((warning, index) => <p key={index}>{warning}</p>)}
                </div>}
                <h3>Recreate this review</h3>
                <p>Say “recreate it” with a coding project active, or choose an existing destination folder here.</p>
                <label htmlFor="foreground-review-folder">Destination folder</label>
                <input id="foreground-review-folder" type="text" value={folder}
                  onChange={(event) => setFolder(event.target.value)}
                  placeholder="D:\\Projects\\MyNewApp" autoComplete="off" />
                <div className="foreground-review__actions">
                  <button type="button" onClick={startRecreation}
                    disabled={starting || !folder.trim() || deletedReviewId === data.review.id || data.review.completeness === "capturing"}>
                    {starting ? "Starting…" : "Start recreation"}
                  </button>
                  <button type="button" onClick={deleteReview} disabled={deletedReviewId === data.review.id}>Delete saved review</button>
                </div>
                {recreationStatus && <p role="status">{recreationStatus}</p>}
              </section>
            </Card>
          )}

          {/* ── 3. Item List Card ─────────────────────────────────── */}
          <Card
            title="Parts of the screen"
            trailing={<Chip tone="neutral">{items.length}</Chip>}
            className="spatial-items-card"
          >
            <div className="spatial-cards">
              {items.map((item) => (
                <SpatialCard
                  key={item.id}
                  item={item}
                  active={activeCardId === item.id}
                  onHover={hoverCard}
                />
              ))}
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
}

export function SpatialCard({
  item,
  active,
  onHover,
}: {
  item: SpatialItemCard;
  active: boolean;
  onHover: (id: number | null) => void;
}) {
  const openImage = useSidebar((s) => s.openImage);
  return (
    <div
      id={`spatial-card-${item.id}`}
      className={`spatial-card ${active ? "spatial-card--active" : ""}`}
      onMouseEnter={() => onHover(item.id)}
      onMouseLeave={() => onHover(null)}
    >
      <div className="spatial-card-header">
        <span className="spatial-badge">[{item.id}]</span>
        <LogoTile
          src={item.logo_data_url}
          name={item.label}
          size={24}
          rounded="lg"
        />
        <span className="spatial-label">{item.label}</span>
        <Chip tone="neutral" className="spatial-category">
          {item.category}
        </Chip>
      </div>
      {item.summary && <div className="spatial-summary">{item.summary}</div>}
      {Boolean(item.image_data_url) && (
        <Lens
          src={item.image_data_url!}
          alt={`Captured view of ${item.label}`}
          onOpen={() => openImage(item.image_data_url!, `Captured view of ${item.label}`)}
        >
          <img
            className="spatial-card-image"
            src={item.image_data_url}
            alt={`Captured view of ${item.label}`}
            loading="lazy"
          />
        </Lens>
      )}
      {item.details && item.details.length > 0 && (
        <dl className="spatial-detail-grid">
          {item.details.map((row, i) => (
            <div className="spatial-detail-row" key={i}>
              <dt className="spatial-detail-title">{row.title}</dt>
              <dd className="spatial-detail-value">{row.value}</dd>
            </div>
          ))}
        </dl>
      )}
    </div>
  );
}
