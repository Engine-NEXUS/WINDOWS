import { useEffect, useState, useRef, useMemo, useCallback } from "react";
import { useSidebar } from "./sidebarStore";
import type {
  StackPayload,
  StackProgress,
  StackFocusPayload,
  StackLogoPatch,
} from "./stackTypes";
import {
  confidenceLabel,
  securityScore,
  formatBytes,
  formatCategoryLabel,
  SECURITY_HEADER_KEYS,
  donutSlicesByCategory,
  donutSlicesByConfidence,
  findTopTechnology,
  filterDetections,
} from "./stackModel";
import {
  AppBar,
  LogoTile,
  IconButton,
  DockIcon,
  Card,
  Chip,
  DropdownChip,
  Donut,
  MiniBar,
  BarChart,
} from "./kit";
import "./stack.css";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

function extractHost(urlStr?: string | null): string {
  if (!urlStr) return "";
  try {
    const raw = urlStr.trim();
    const withProto = /^https?:\/\//i.test(raw) ? raw : `https://${raw}`;
    const parsed = new URL(withProto);
    return parsed.hostname.replace(/^www\./, "");
  } catch {
    return (urlStr || "").replace(/^https?:\/\//, "").split("/")[0].replace(/^www\./, "");
  }
}

async function copyToClipboard(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    /* fall through to execCommand */
  }
  try {
    const textarea = document.createElement("textarea");
    textarea.value = text;
    textarea.style.position = "fixed";
    textarea.style.left = "-9999px";
    textarea.style.top = "-9999px";
    textarea.setAttribute("readonly", "");
    document.body.appendChild(textarea);
    textarea.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(textarea);
    return ok;
  } catch {
    return false;
  }
}

function downloadMarkdown(markdown: string, host: string) {
  const safeHost = (host || "site").replace(/[^a-zA-Z0-9.-]/g, "_");
  const blob = new Blob([markdown || ""], { type: "text/markdown;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = `stack-${safeHost}.md`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

export function StackView({
  onDock = () => {},
  payload,
  progress,
  focus,
  initialSearch = "",
  initialConfidence = "All",
  initialExpanded = {},
}: {
  onDock?: (d: string) => void;
  payload?: StackPayload | null;
  progress?: StackProgress | null;
  focus?: string[];
  initialSearch?: string;
  initialConfidence?: string;
  initialExpanded?: Record<string, boolean>;
}) {
  const storeData = useSidebar((s) => s.stackData);
  const storeProgress = useSidebar((s) => s.stackProgress);
  const storeFocus = useSidebar((s) => s.stackFocus);

  const stackData = payload !== undefined ? payload : storeData;
  const stackProgress = progress !== undefined ? progress : storeProgress;
  const stackFocus = focus !== undefined ? focus : (storeFocus || []);

  const cardRefs = useRef<Map<string, HTMLDivElement>>(new Map());
  const [expandedEvidence, setExpandedEvidence] = useState<Record<string, boolean>>(initialExpanded);
  const [showAllHosts, setShowAllHosts] = useState(false);
  const [copied, setCopied] = useState(false);
  const [mixMode, setMixMode] = useState<"By category" | "By confidence">("By category");
  const [searchQuery, setSearchQuery] = useState(initialSearch);
  const [filterConfidence, setFilterConfidence] = useState(initialConfidence);

  // Fresh-window race path: fetch pending payload on mount
  useEffect(() => {
    if (!isTauri()) return;
    void (async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const pending = await invoke<StackPayload | null>("get_pending_stack").catch(() => null);
      if (pending) {
        useSidebar.getState().showStack(pending);
      }
    })();
  }, []);

  // Fast path: live event listeners
  useEffect(() => {
    if (!isTauri()) return;
    let unlistenStack: (() => void) | null = null;
    let unlistenProgress: (() => void) | null = null;
    let unlistenFocus: (() => void) | null = null;
    let unlistenLogos: (() => void) | null = null;

    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");

      unlistenStack = await listen<StackPayload>("sidebar:show_stack", (ev) => {
        if (ev.payload) {
          useSidebar.getState().showStack(ev.payload);
        }
      });

      unlistenProgress = await listen<StackProgress>("reverse:progress", (ev) => {
        if (ev.payload) {
          useSidebar.getState().setStackProgress(ev.payload);
        }
      });

      unlistenFocus = await listen<StackFocusPayload>("reverse:focus_card", (ev) => {
        useSidebar.getState().setStackFocus(ev.payload?.ids || []);
      });

      unlistenLogos = await listen<StackLogoPatch>("sidebar:stack_logos", (ev) => {
        if (ev.payload) {
          useSidebar.getState().mergeStackLogos(ev.payload);
        }
      });
    })().catch(() => {});

    return () => {
      unlistenStack?.();
      unlistenProgress?.();
      unlistenFocus?.();
      unlistenLogos?.();
    };
  }, []);

  // Scroll focused card into view smoothly
  useEffect(() => {
    if (stackFocus && stackFocus.length > 0) {
      for (const id of stackFocus) {
        const el = cardRefs.current.get(id);
        if (el) {
          el.scrollIntoView({ block: "nearest", behavior: "smooth" });
          break;
        }
      }
    }
  }, [stackFocus]);

  const toggleEvidence = useCallback((id: string) => {
    setExpandedEvidence((prev) => ({ ...prev, [id]: !prev[id] }));
  }, []);

  const handleCopyReport = useCallback(async () => {
    if (!stackData?.markdown) return;
    const ok = await copyToClipboard(stackData.markdown);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  }, [stackData?.markdown]);

  const handleSaveMd = useCallback(() => {
    if (!stackData) return;
    const host = extractHost(stackData.final_url || stackData.url);
    downloadMarkdown(stackData.markdown || "", host);
  }, [stackData]);

  const techNameMap = useMemo(() => {
    const map = new Map<string, string>();
    for (const d of stackData?.detections || []) {
      if (d && d.id) {
        map.set(d.id, d.name || d.id);
      }
    }
    return map;
  }, [stackData?.detections]);

  const secScore = useMemo(() => {
    return securityScore(stackData?.security);
  }, [stackData?.security]);

  const host = useMemo(() => {
    return extractHost(stackData?.final_url || stackData?.url);
  }, [stackData?.final_url, stackData?.url]);

  const topTech = useMemo(() => {
    return findTopTechnology(stackData?.detections);
  }, [stackData?.detections]);

  const mixSlices = useMemo(() => {
    if (mixMode === "By confidence") {
      return donutSlicesByConfidence(stackData);
    }
    return donutSlicesByCategory(stackData);
  }, [stackData, mixMode]);

  const filteredDetections = useMemo(() => {
    return filterDetections(stackData?.detections, searchQuery, filterConfidence);
  }, [stackData?.detections, searchQuery, filterConfidence]);

  const hasLogos = useMemo(() => {
    return (stackData?.detections || []).some((d) => Boolean(d?.logo));
  }, [stackData?.detections]);

  // Empty state
  if (!stackData) {
    return (
      <div className="stack-view stack-view--empty">
        <AppBar
          logo={<LogoTile name="Nexus" size={36} rounded="lg" />}
          title="Tech Stack"
          subtitle="Web Reverse Engineer"
          actions={
            <>
              <IconButton className="sidebar-dock-btn" label="Dock Left" onClick={() => onDock("left")}>
                <DockIcon side="left" />
              </IconButton>
              <IconButton className="sidebar-dock-btn" label="Dock Right" onClick={() => onDock("right")}>
                <DockIcon side="right" />
              </IconButton>
            </>
          }
        />
        <div className="stack-empty-content">
          <div className="stack-empty-icon">🌐</div>
          Say: Nexus, reverse engineer this website
        </div>
      </div>
    );
  }

  const isScanning = Boolean(stackData.scanning);
  const thirdPartyHosts = stackData.stats?.third_party_hosts || [];
  const visibleHosts = showAllHosts ? thirdPartyHosts : thirdPartyHosts.slice(0, 8);
  const sections = Array.isArray(stackData.sections) ? stackData.sections : [];

  const weightBars = [
    { label: "HTML", value: stackData.stats?.html_bytes ?? 0, colorIndex: 0 },
    { label: "JavaScript", value: stackData.stats?.js_bytes_fetched ?? 0, colorIndex: 1 },
    { label: "CSS", value: stackData.stats?.css_bytes_fetched ?? 0, colorIndex: 2 },
  ];

  return (
    <div className="stack-view">
      {/* ── 1. AppBar ──────────────────────────────────────────────── */}
      <AppBar
        logo={
          <LogoTile
            src={stackData.site_logo}
            name={host || stackData.title || "Site"}
            size={36}
            rounded="lg"
          />
        }
        title={stackData.title || host || "Website Tech Stack"}
        subtitle={`${host || "site"} · Tech stack`}
        status={isScanning ? "scanning" : "done"}
        actions={
          <>
            <IconButton
              label="Copy Report"
              onClick={handleCopyReport}
              active={copied}
              disabled={!stackData.markdown}
            >
              {copied ? "✓" : "📋"}
            </IconButton>
            <IconButton className="sidebar-dock-btn" label="Dock Left" onClick={() => onDock("left")}>
              <DockIcon side="left" />
            </IconButton>
            <IconButton className="sidebar-dock-btn" label="Dock Right" onClick={() => onDock("right")}>
              <DockIcon side="right" />
            </IconButton>
          </>
        }
      />

      <div className="stack-scroll-body">
        <div className="stack-phone-col">
          {/* ── 2. Hero Card ───────────────────────────────────────── */}
          <Card className="stack-hero-card">
            {topTech && (
              <div className="stack-hero-top-badge">
                <LogoTile src={topTech.logo} name={topTech.name} size={22} rounded="lg" />
                <span className="stack-hero-top-name">{topTech.name}</span>
                <Chip
                  tone={
                    topTech.confidence >= 0.8
                      ? "good"
                      : topTech.confidence >= 0.5
                      ? "warn"
                      : "neutral"
                  }
                >
                  {confidenceLabel(topTech.confidence)}
                </Chip>
              </div>
            )}

            {stackData.summary ? (
              <h2 className="stack-hero-headline">{stackData.summary}</h2>
            ) : isScanning ? (
              <div className="stack-skeleton-block" style={{ width: "90%", height: 22 }} />
            ) : (
              <h2 className="stack-hero-headline">{stackData.title || host}</h2>
            )}

            <p className="stack-hero-sub">
              {`${stackData.detections?.length ?? 0} technologies · ${
                stackData.stats?.script_count ?? 0
              } scripts · ${thirdPartyHosts.length} third-party hosts`}
            </p>

            <button
              type="button"
              className="stack-hero-copy-link"
              onClick={handleCopyReport}
              disabled={!stackData.markdown}
            >
              {copied ? "Copied! ✓" : "Copy report →"}
            </button>

            {/* Progress line inside hero card while scanning */}
            {stackProgress && (
              <div className="stack-progress-section">
                {typeof stackProgress.total === "number" && stackProgress.total > 0 ? (
                  <div className="stack-progress-segments">
                    {Array.from({ length: stackProgress.total }).map((_, i) => (
                      <div
                        key={i}
                        className={`stack-progress-segment ${
                          i < (stackProgress.step ?? 0) ? "stack-progress-segment--active" : ""
                        }`}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="stack-progress-bar-wrap">
                    <div
                      className="stack-progress-bar-fill"
                      style={{
                        width:
                          stackProgress.phase === "done"
                            ? "100%"
                            : stackProgress.phase === "starting"
                            ? "15%"
                            : "60%",
                      }}
                    />
                  </div>
                )}
                {stackProgress.text && <div className="stack-progress-text">{stackProgress.text}</div>}
              </div>
            )}
          </Card>

          {/* ── 3. Stack Mix Card ──────────────────────────────────── */}
          <Card
            title="Stack mix"
            trailing={
              <DropdownChip
                value={mixMode}
                options={["By category", "By confidence"]}
                onChange={(v) => setMixMode(v as any)}
                ariaLabel="Group stack mix by"
              />
            }
          >
            <Donut
              slices={mixSlices}
              centerTop={String(stackData.detections?.length ?? 0)}
              centerBottom="technologies"
              size={140}
            />
            {mixSlices.length > 0 && (
              <div className="stack-mix-legend">
                {mixSlices.map((s, idx) => (
                  <Chip
                    key={idx}
                    dot={`var(--kit-c${s.colorIndex % 8})`}
                    tone="neutral"
                  >
                    {`${s.label} (${s.value})`}
                  </Chip>
                ))}
              </div>
            )}
          </Card>

          {/* ── 4. Security Headers Card ───────────────────────────── */}
          <Card
            title="Security headers"
            trailing={
              <Chip
                tone={
                  secScore.passed === secScore.total
                    ? "good"
                    : secScore.passed >= 3
                    ? "warn"
                    : "bad"
                }
              >
                {`${secScore.passed}/${secScore.total}`}
              </Chip>
            }
          >
            <div className="stack-sec-grid">
              {SECURITY_HEADER_KEYS.map(({ key, label }) => {
                const pass = stackData.security ? Boolean(stackData.security[key]) : false;
                return <MiniBar key={key} label={label} pass={pass} />;
              })}
            </div>
          </Card>

          {/* ── 5. Technologies Card ───────────────────────────────── */}
          <Card title="Technologies">
            <div className="stack-tech-controls">
              <input
                type="text"
                placeholder="Search technologies..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="stack-search-input"
                aria-label="Search technologies"
              />
              <DropdownChip
                value={filterConfidence}
                options={["All", "Confirmed", "Likely", "Possible"]}
                onChange={setFilterConfidence}
                ariaLabel="Filter by confidence"
              />
            </div>

            <div className="stack-tech-list">
              {filteredDetections.length === 0 ? (
                <div className="stack-search-empty">
                  {searchQuery
                    ? `No technologies found matching "${searchQuery}"`
                    : "No technologies found"}
                </div>
              ) : (
                filteredDetections.map((detection) => {
                  const isFocused = stackFocus.includes(detection.id);
                  const isExpanded = Boolean(expandedEvidence[detection.id]);
                  const confTag = confidenceLabel(detection.confidence);
                  const confLower = confTag.toLowerCase();
                  const evidenceList = Array.isArray(detection.evidence) ? detection.evidence : [];

                  return (
                    <div
                      key={detection.id}
                      ref={(el) => {
                        if (el) cardRefs.current.set(detection.id, el);
                        else cardRefs.current.delete(detection.id);
                      }}
                      className={`stack-tech-row ${isFocused ? "stack-tech-row--focus" : ""}`}
                      data-tech-id={detection.id}
                      onClick={() => toggleEvidence(detection.id)}
                    >
                      <div className="stack-tech-row-main">
                        <LogoTile
                          src={detection.logo}
                          name={detection.name}
                          size={32}
                          rounded="lg"
                        />
                        <div className="stack-tech-info">
                          <span className="stack-tech-name">{detection.name}</span>
                          <span className="stack-tech-cat">
                            {formatCategoryLabel(detection.category)}
                          </span>
                        </div>
                        <span className={`stack-conf-pill stack-conf-pill--${confLower}`}>
                          {confTag}
                        </span>
                      </div>

                      {/* Evidence toggle on row tap */}
                      {isExpanded && evidenceList.length > 0 && (
                        <ul className="stack-evidence-list">
                          {evidenceList.map((ev, idx) => (
                            <li key={idx}>{ev}</li>
                          ))}
                        </ul>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </Card>

          {/* ── 6. Page Weight Card ────────────────────────────────── */}
          <Card title="Page weight">
            <div className="stack-weight-big-figures">
              <div className="stack-weight-figure-card">
                <span className="stack-weight-figure-val">
                  {formatBytes(stackData.stats?.js_bytes_fetched)}
                </span>
                <Chip tone="neutral">{`${stackData.stats?.script_count ?? 0} scripts`}</Chip>
              </div>
              <div className="stack-weight-figure-card">
                <span className="stack-weight-figure-val">
                  {formatBytes(
                    (stackData.stats?.html_bytes ?? 0) +
                      (stackData.stats?.css_bytes_fetched ?? 0),
                  )}
                </span>
                <Chip tone="neutral">{`${stackData.stats?.stylesheet_count ?? 0} stylesheets`}</Chip>
              </div>
            </div>

            <BarChart bars={weightBars} formatValue={formatBytes} />

            <div className="stack-weight-img-count">
              {`${stackData.stats?.image_count ?? 0} images detected`}
            </div>

            {thirdPartyHosts.length > 0 && (
              <div className="stack-hosts-wrap">
                <span className="stack-hosts-title">
                  {`Third-party hosts (${thirdPartyHosts.length})`}
                </span>
                <div className="stack-hosts-chips">
                  {visibleHosts.map((h, i) => (
                    <span key={i} className="stack-host-chip">
                      {h}
                    </span>
                  ))}
                </div>
                {thirdPartyHosts.length > 8 && (
                  <button
                    type="button"
                    className="stack-hosts-toggle-btn"
                    onClick={() => setShowAllHosts((prev) => !prev)}
                  >
                    {showAllHosts
                      ? "Show fewer"
                      : `+ ${thirdPartyHosts.length - 8} more hosts`}
                  </button>
                )}
              </div>
            )}
          </Card>

          {/* ── 7. On This Page Card (only when sections.length > 0) ─── */}
          {sections.length > 0 && (
            <Card title="On this page">
              <div className="stack-sections-list">
                {sections.map((sec) => (
                  <div key={sec.idx} className="stack-section-item">
                    <div className="stack-section-num-title">
                      <span className="stack-section-num">{sec.idx}</span>
                      <span>{sec.title}</span>
                    </div>
                    {sec.summary && (
                      <p className="stack-section-summary">{sec.summary}</p>
                    )}
                    {Array.isArray(sec.stack_refs) && sec.stack_refs.length > 0 && (
                      <div className="stack-section-chips">
                        {sec.stack_refs.map((refId) => {
                          const name = techNameMap.get(refId) || refId;
                          return (
                            <button
                              key={refId}
                              type="button"
                              className="stack-section-tech-btn"
                              onClick={() => {
                                useSidebar.getState().setStackFocus([refId]);
                                const el = cardRefs.current.get(refId);
                                if (el) {
                                  el.scrollIntoView({
                                    block: "nearest",
                                    behavior: "smooth",
                                  });
                                }
                              }}
                              title={`Focus ${name} row`}
                            >
                              {name}
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                ))}
              </div>
            </Card>
          )}

          {/* ── 8. Footer ──────────────────────────────────────────── */}
          <footer className="stack-footer">
            <p className="stack-footer-disclaimer">
              Not detectable from outside: server code, databases, private APIs
            </p>
            <div className="stack-footer-actions">
              <button
                type="button"
                className="stack-btn stack-btn--secondary"
                onClick={handleSaveMd}
                disabled={!stackData.markdown}
              >
                Save as .md
              </button>
            </div>
            {hasLogos && (
              <span className="stack-logo-attribution">Logos by Logo.dev</span>
            )}
          </footer>
        </div>
      </div>
    </div>
  );
}
