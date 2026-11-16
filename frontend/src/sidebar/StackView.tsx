import { useEffect, useState, useRef, useMemo, useCallback } from "react";
import { useSidebar } from "./sidebarStore";
import type {
  StackDetection,
  StackPayload,
  StackProgress,
  StackFocusPayload,
  StackLogoPatch,
} from "./stackTypes";
import { techIconFor, techIconForHost } from "./kit/techIcon";
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
  Card,
  Chip,
  DropdownChip,
  Donut,
  MiniBar,
  BarChart,
} from "./kit";
import "./stack.css";
import "./stack-v2.css";

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

/** Groups detections by category, keeping the incoming (confidence) order inside a group and the first-seen group order. */
function groupByCategory(list: StackDetection[]) {
  const order: string[] = [];
  const map = new Map<string, StackDetection[]>();
  for (const d of list) {
    const c = d.category || "other";
    if (!map.has(c)) {
      map.set(c, []);
      order.push(c);
    }
    map.get(c)!.push(d);
  }
  return order.map((category, i) => ({
    category,
    label: formatCategoryLabel(category),
    colorIndex: i,
    items: map.get(category)!,
  }));
}

export function StackView({
  onDock: _onDock = () => {},
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

  const isApp = stackData?.kind === "app";
  const host = useMemo(() => {
    // Apps carry the exe file name, not a web address.
    if (stackData?.kind === "app") return stackData?.url || "";
    return extractHost(stackData?.final_url || stackData?.url);
  }, [stackData?.kind, stackData?.final_url, stackData?.url]);

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
        />
        <div className="stack-empty-content">
          <div className="stack-empty-icon">🌐</div>
          Say: Nexus, reverse engineer this website
        </div>
      </div>
    );
  }

  const isScanning = Boolean(stackData.scanning) || Boolean(stackData.rendering);
  const thin = Array.isArray(stackData.thin) ? stackData.thin : [];
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
            techId={isApp ? stackData.title : undefined}
            size={36}
            rounded="lg"
          />
        }
        title={stackData.title || host || (isApp ? "App Tech Stack" : "Website Tech Stack")}
        subtitle={`${host || (isApp ? "app" : "site")} · ${isApp ? "App stack" : "Tech stack"}`}
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
          </>
        }
      />

      <div className="stack-scroll-body">
        <div className="stack-phone-col">
          {/* ── 2. Hero Card ───────────────────────────────────────── */}
          <Card id="sv-overview" className="stack-hero-card">
            {(stackData.detections?.length ?? 0) > 0 && (
              <div className="stack-avatars" aria-hidden="true">
                {(stackData.detections ?? []).slice(0, 7).map((d) => (
                  <LogoTile
                    key={d.id}
                    src={d.logo}
                    name={d.name}
                    techId={d.id}
                    category={d.category}
                    size={34}
                    rounded="full"
                    className="stack-avatar"
                  />
                ))}
                {(stackData.detections?.length ?? 0) > 7 && (
                  <span className="stack-avatar stack-avatar--more">{`+${(stackData.detections?.length ?? 0) - 7}`}</span>
                )}
              </div>
            )}
            {topTech && (
              <div className="stack-hero-top-badge">
                <LogoTile src={topTech.logo} name={topTech.name} techId={topTech.id} category={topTech.category} size={22} rounded="lg" />
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

            <div className="stack-stat-tiles">
              <div className="stack-stat-tile" data-accent="blue">
                <span className="stack-stat-label">Technologies</span>
                <span className="stack-stat-num">{stackData.detections?.length ?? 0}</span>
                <span className="stack-stat-hint">{`${new Set((stackData.detections ?? []).map((d) => d.category)).size} layers`}</span>
              </div>
              {!isApp && (
                <div className="stack-stat-tile" data-accent="violet">
                  <span className="stack-stat-label">Scripts</span>
                  <span className="stack-stat-num">{stackData.stats?.script_count ?? 0}</span>
                  <span className="stack-stat-hint">{`${stackData.stats?.inline_script_count ?? 0} inline`}</span>
                </div>
              )}
              {!isApp && (
                <div className="stack-stat-tile" data-accent="amber">
                  <span className="stack-stat-label">Hosts</span>
                  <span className="stack-stat-num">{thirdPartyHosts.length}</span>
                  <span className="stack-stat-hint">third-party</span>
                </div>
              )}
              {!isApp && (
                <div className="stack-stat-tile" data-accent="green">
                  <span className="stack-stat-label">Security</span>
                  <span className="stack-stat-num">{`${secScore.passed}/${secScore.total}`}</span>
                  <span className="stack-stat-hint">headers set</span>
                </div>
              )}
            </div>

            {thin.length > 0 && (
              <ul className="stack-thin-list" aria-label="Why this scan is limited">
                {thin.map((t, i) => (
                  <li key={i}>{t}</li>
                ))}
              </ul>
            )}

            {stackData.rendering && (
              <p className="stack-rendering-note">Looking deeper in a hidden browser window…</p>
            )}

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
            className="stack-mix-card"
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
                {mixSlices.map((sl, idx) => {
                  const total = mixSlices.reduce((n, x) => n + x.value, 0) || 1;
                  return (
                    <div key={idx} className="stack-layer-row" style={{ ["--c" as any]: `var(--kit-c${sl.colorIndex % 8})` }}>
                      <span className="stack-layer-label">{sl.label}</span>
                      <span className="stack-layer-count">{`(${sl.value})`}</span>
                      <span className="stack-layer-bar"><i style={{ width: `${Math.max(6, Math.round((sl.value / total) * 100))}%` }} /></span>
                    </div>
                  );
                })}
              </div>
            )}
          </Card>

          {/* ── 4. Security Headers Card (websites only) ───────────── */}
          {!isApp && (
          <Card
            id="sv-security"
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
            <div className="stack-sec-ring-row">
              <div
                className="stack-sec-ring"
                style={{ ["--pct" as any]: `${Math.round((secScore.passed / Math.max(1, secScore.total)) * 100)}%` }}
                role="img"
                aria-label={`${secScore.passed} of ${secScore.total} security headers present`}
              >
                <span className="stack-sec-ring-num">{secScore.passed}</span>
                <span className="stack-sec-ring-of">{`of ${secScore.total}`}</span>
              </div>
              <p className="stack-sec-note">
                {secScore.passed === secScore.total
                  ? "Every common protection header is set."
                  : `${secScore.total - secScore.passed} common protection headers are missing.`}
              </p>
            </div>
            <div className="stack-sec-grid">
              {SECURITY_HEADER_KEYS.map(({ key, label }) => {
                const pass = stackData.security ? Boolean(stackData.security[key]) : false;
                return <MiniBar key={key} label={label} pass={pass} />;
              })}
            </div>
          </Card>
          )}

          {/* ── 5. Technologies Card ───────────────────────────────── */}
          <Card id="sv-stack" title="Technologies">
            <div className="stack-tech-controls">
              <input
                type="text"
                placeholder="Search technologies..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                className="stack-search-input"
                aria-label="Search technologies"
              />
              <div className="stack-seg" role="tablist" aria-label="Filter by confidence">
                {["All", "Confirmed", "Likely", "Possible"].map((opt) => (
                  <button
                    key={opt}
                    type="button"
                    role="tab"
                    aria-selected={filterConfidence === opt}
                    className={`stack-seg-btn ${filterConfidence === opt ? "stack-seg-btn--on" : ""}`}
                    onClick={() => setFilterConfidence(opt)}
                  >
                    {opt}
                  </button>
                ))}
              </div>
            </div>

            <div className="stack-tech-list">
              {filteredDetections.length === 0 ? (
                <div className="stack-search-empty">
                  {searchQuery
                    ? `No technologies found matching "${searchQuery}"`
                    : "No technologies found"}
                </div>
              ) : (
                groupByCategory(filteredDetections).map((group) => (
                  <div key={group.category} className="stack-tech-group">
                    <div className="stack-tech-group-head">
                      <span className="stack-tech-group-dot" style={{ background: `var(--kit-c${group.colorIndex % 8})` }} />
                      <span className="stack-tech-group-name">{group.label}</span>
                      <span className="stack-tech-group-count">{group.items.length}</span>
                    </div>
                    {group.items.map((detection) => {
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
                          techId={detection.id}
                          category={detection.category}
                          size={32}
                          rounded="lg"
                        />
                        <div className="stack-tech-info">
                          <span className="stack-tech-name">
                            {detection.name}
                            {detection.version ? (
                              <span className="stack-tech-version">{` ${detection.version}`}</span>
                            ) : null}
                          </span>
                          <span className="stack-tech-cat">
                            {evidenceList[0] ?? formatCategoryLabel(detection.category)}
                          </span>
                        </div>
                        <span className={`stack-conf-pill stack-conf-pill--${confLower}`} title={`${Math.round(detection.confidence * 100)}% sure`}>
                          <span className="stack-conf-label">{confTag}</span>
                          <span className="stack-conf-bar" aria-hidden="true">
                            <i style={{ width: `${Math.max(8, Math.round(detection.confidence * 100))}%` }} />
                          </span>
                          <span className="stack-conf-pct">{`${Math.round(detection.confidence * 100)}%`}</span>
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
                    })}
                  </div>
                ))
              )}
            </div>
          </Card>

          {/* ── 6. Page Weight Card (websites only) ────────────────── */}
          {!isApp && (
          <Card id="sv-weight" title="Page weight">
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
                      {(() => {
                        const ic = techIconForHost(h);
                        return ic ? <img className="stack-chip-icon" src={ic.src} alt="" data-tone={ic.tone} /> : null;
                      })()}
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
          )}

          {/* ── 7. On This Page Card (only when sections.length > 0) ─── */}
          {sections.length > 0 && (
            <Card title={isApp ? "In this window" : "On this page"}>
              <div className="stack-sections-list">
                {sections.map((sec) => (
                  <div key={sec.idx} className="stack-section-item">
                    <div className="stack-section-num-title">
                      <span className="stack-section-num">{(sec.idx ?? 0) + 1}</span>
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
                              {(() => {
                                const ic = techIconFor(refId, name);
                                return ic ? <img className="stack-chip-icon" src={ic.src} alt="" data-tone={ic.tone} /> : null;
                              })()}
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
              {isApp
                ? "Read from the install folder only; the app was never run."
                : "Not detectable from outside: server code, databases, private APIs"}
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
      <nav className="stack-tabbar" aria-label="Sections">
        {[
          { id: "sv-overview", label: "Overview", icon: "M4 11l8-7 8 7v8a1 1 0 0 1-1 1h-4v-6H9v6H5a1 1 0 0 1-1-1z" },
          { id: "sv-stack", label: "Stack", icon: "M12 3 3 8l9 5 9-5-9-5zm-9 9 9 5 9-5M3 16l9 5 9-5" },
          ...(!isApp ? [{ id: "sv-security", label: "Security", icon: "M12 3 5 6v5.5c0 4.4 2.9 7.9 7 9.5 4.1-1.6 7-5.1 7-9.5V6z" }] : []),
          ...(!isApp ? [{ id: "sv-weight", label: "Weight", icon: "M4 20V10M10 20V4M16 20v-7M22 20H2" }] : []),
        ].map((t) => (
          <button
            key={t.id}
            type="button"
            className="stack-tab"
            onClick={() => document.getElementById(t.id)?.scrollIntoView({ block: "start", behavior: "smooth" })}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d={t.icon} /></svg>
            <span>{t.label}</span>
          </button>
        ))}
      </nav>
    </div>
  );
}
