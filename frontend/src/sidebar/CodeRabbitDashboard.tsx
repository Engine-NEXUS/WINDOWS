import React, { useState, useMemo, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface CodeRabbitFinding {
  id: string;
  file: string;
  lineRange: string;
  severity: "critical" | "warning" | "suggestion" | "nitpick" | "praise";
  category?: string;
  title: string;
  description: string;
  originalCode?: string;
  suggestedCode?: string;
}

export interface CodeRabbitWalkthroughItem {
  title: string;
  details: string;
}

export interface CodeRabbitStats {
  filesChanged: number;
  additions: number;
  deletions: number;
}

export interface CodeRabbitPrSummary {
  prNumber: number;
  title: string;
  author: string;
  headBranch: string;
  baseBranch: string;
  verdict: "APPROVE" | "REQUEST_CHANGES" | "COMMENT";
  filesChanged: number;
  additions: number;
  deletions: number;
  issuesSummary: string;
  updatedAt: string;
}

export interface CodeRabbitReviewData {
  prNumber: number;
  repo: string;
  title: string;
  headBranch: string;
  baseBranch: string;
  verdict: "APPROVE" | "REQUEST_CHANGES" | "COMMENT";
  summary: string;
  riskScore?: number;
  riskLevel?: string;
  poem?: string;
  walkthrough: CodeRabbitWalkthroughItem[];
  findings: CodeRabbitFinding[];
  stats: CodeRabbitStats;
  activePrs?: CodeRabbitPrSummary[];
  mode?: "pr_list" | "pr_review";
}

interface CodeRabbitDashboardProps {
  data: CodeRabbitReviewData | { mode?: string; activePrs?: CodeRabbitPrSummary[]; [key: string]: any };
}

type SubPage = "active_prs" | "review" | "local" | "rules";
type SeverityFilter = "all" | "critical" | "warning" | "suggestion" | "nitpick" | "praise";
type DiffMode = "split" | "unified";

export const SAMPLE_ACTIVE_PRS: CodeRabbitPrSummary[] = [
  {
    prNumber: 24,
    title: "feat(db-pool): Optimize database connection pooling & query result caching",
    author: "Lakshya",
    headBranch: "feat/db-pool-opt",
    baseBranch: "main",
    verdict: "REQUEST_CHANGES",
    filesChanged: 4,
    additions: 184,
    deletions: 46,
    issuesSummary: "2 critical issues, 1 warning detected",
    updatedAt: "10 mins ago",
  },
  {
    prNumber: 25,
    title: "fix(auth): Prevent race condition in JWT token expiration renewal",
    author: "Prem",
    headBranch: "fix/jwt-renewal-race",
    baseBranch: "main",
    verdict: "APPROVE",
    filesChanged: 2,
    additions: 32,
    deletions: 8,
    issuesSummary: "Zero issues detected · Safe to merge",
    updatedAt: "1 hour ago",
  },
  {
    prNumber: 26,
    title: "refactor(tts): Unified zero-emotion neural engine & pitch locking",
    author: "Chitkul",
    headBranch: "refactor/zero-emotion-tts",
    baseBranch: "main",
    verdict: "COMMENT",
    filesChanged: 3,
    additions: 75,
    deletions: 15,
    issuesSummary: "1 suggestion, 1 praise detected",
    updatedAt: "3 hours ago",
  },
  {
    prNumber: 27,
    title: "chore(deps): Bump crate dependencies and update build configuration",
    author: "Automated",
    headBranch: "chore/deps-bump",
    baseBranch: "main",
    verdict: "APPROVE",
    filesChanged: 1,
    additions: 12,
    deletions: 4,
    issuesSummary: "Build passing · Zero regressions",
    updatedAt: "5 hours ago",
  },
];

export const SAMPLE_CODERABBIT_REVIEW: CodeRabbitReviewData = {
  prNumber: 24,
  repo: "ultron",
  title: "feat(db-pool): Optimize database connection pooling & query result caching",
  headBranch: "feat/db-pool-opt",
  baseBranch: "main",
  verdict: "REQUEST_CHANGES",
  riskScore: 78,
  riskLevel: "Moderate Risk",
  summary:
    "This pull request refactors the database connection acquisition pipeline to use semaphore-bounded leasing, integrates an in-memory LRU query cache, and introduces structured error mapping. While performance is significantly improved, there is a critical resource leak on transient errors and thread contention in the cache invalidation path.",
  poem:
    "A connection leased with high intent,\nDropped on transient errors, unspent.\nNow caught with care, recycled fast,\nThe pool shall breathe, and queries last.",
  walkthrough: [
    {
      title: "Connection Pool Lifecycle",
      details:
        "Replaces unconstrained connection spawning with a semaphore-bounded pool of max 24 connections, reducing database socket overhead by 45%.",
    },
    {
      title: "LRU Query Result Cache",
      details:
        "Implements an in-memory cache with eviction policies and 60-second TTL to accelerate repetitive reads in the agent loop.",
    },
    {
      title: "Error Handling & Metrics",
      details:
        "Maps low-level driver connection drops into typed ApplicationError variants and emits Prometheus counter metrics.",
    },
  ],
  stats: {
    filesChanged: 4,
    additions: 184,
    deletions: 46,
  },
  findings: [
    {
      id: "cr-1",
      file: "src-tauri/src/database/pool.rs",
      lineRange: "42-58",
      severity: "critical",
      category: "Resource Leak",
      title: "Potential Connection Leak in Error Handler",
      description:
        "When query execution fails with a transient driver error, the connection lease is dropped without explicitly invoking pool.recycle(conn). In high-frequency query bursts, this permanently starves the available connection slots.",
      originalCode: `let conn = pool.acquire().await?;\nlet res = conn.execute(query).await;\nif res.is_err() {\n    return Err(AppError::QueryFailed);\n}`,
      suggestedCode: `let conn = pool.acquire().await?;\nlet res = match conn.execute(query).await {\n    Ok(val) => val,\n    Err(e) => {\n        pool.recycle(conn).await;\n        return Err(AppError::from(e));\n    }\n};`,
    },
    {
      id: "cr-2",
      file: "src-tauri/src/cache/lru.rs",
      lineRange: "104-118",
      severity: "warning",
      category: "Lock Contention",
      title: "Unbounded Lock Contention in Cache Invalidation",
      description:
        "Acquiring a synchronous std::sync::RwLock write guard inside an asynchronous streaming pipeline blocks the Tokio worker thread pool during cache flushes. Use tokio::sync::RwLock instead.",
      originalCode: `pub fn invalidate_keys(&self, prefix: &str) {\n    let mut guard = self.cache.write().unwrap();\n    guard.retain(|k, _| !k.starts_with(prefix));\n}`,
      suggestedCode: `pub async fn invalidate_keys(&self, prefix: &str) {\n    let mut guard = self.cache.write().await;\n    guard.retain(|k, _| !k.starts_with(prefix));\n}`,
    },
    {
      id: "cr-3",
      file: "src-tauri/src/database/metrics.rs",
      lineRange: "27-35",
      severity: "suggestion",
      category: "Metric Explosion",
      title: "High Metric Cardinality from Raw SQL Queries",
      description:
        "Embedding raw SQL query text directly into Prometheus label values causes metric cardinality explosion in production time-series databases. Use a SHA-256 query hash or statement archetype instead.",
      originalCode: `metrics::counter!("db_query_duration_ms", "query" => query_str).increment(duration);`,
      suggestedCode: `let query_hash = hash_query_archetype(query_str);\nmetrics::counter!("db_query_duration_ms", "archetype" => query_hash).increment(duration);`,
    },
    {
      id: "cr-4",
      file: "src-tauri/src/config/pool_config.rs",
      lineRange: "15-18",
      severity: "nitpick",
      category: "Unit Safety",
      title: "Use Duration Type for Pool Timeout Configuration",
      description:
        "Prefer std::time::Duration over raw integer milliseconds (u64) in configuration structs for compile-time unit safety.",
      originalCode: `pub connection_timeout_ms: u64,`,
      suggestedCode: `pub connection_timeout: std::time::Duration,`,
    },
    {
      id: "cr-5",
      file: "src-tauri/src/database/health.rs",
      lineRange: "88-102",
      severity: "praise",
      category: "Efficiency",
      title: "Clean Zero-Allocation Heartbeat Probe",
      description:
        "Excellent use of static pre-allocated ping buffers to minimize memory allocations during high-frequency health probes.",
    },
  ],
};

export const CodeRabbitDashboard: React.FC<CodeRabbitDashboardProps> = ({ data }) => {
  // Determine starting view based on data payload
  const initialIsPrList = (data as any)?.mode === "pr_list" || Boolean((data as any)?.activePrs && !(data as any)?.findings);
  const [subpage, setSubpage] = useState<SubPage>(initialIsPrList ? "active_prs" : "review");
  const [activePrNumber, setActivePrNumber] = useState<number>((data as any)?.prNumber || 24);
  const [reviewData, setReviewData] = useState<CodeRabbitReviewData>(() => {
    if ((data as any)?.findings) {
      return data as CodeRabbitReviewData;
    }
    return SAMPLE_CODERABBIT_REVIEW;
  });

  const [loadingPr, setLoadingPr] = useState<number | null>(null);
  const [filter, setFilter] = useState<SeverityFilter>("all");
  const [diffMode, setDiffMode] = useState<DiffMode>("unified");
  const [selectedFile, setSelectedFile] = useState<string>("src-tauri/src/database/pool.rs");
  const [viewedFiles, setViewedFiles] = useState<Set<string>>(new Set(["src-tauri/src/database/health.rs"]));
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [acceptedFixes, setAcceptedFixes] = useState<Set<string>>(new Set());

  // Right Overlay Drawer state (overlay over center diff)
  const [overlayOpen, setOverlayOpen] = useState(false);
  const [overlayTab, setOverlayTab] = useState<"summary" | "chat">("summary");
  const [chatInput, setChatInput] = useState("");
  const [chatMessages, setChatMessages] = useState<Array<{ sender: "user" | "bot"; text: string }>>([
    {
      sender: "bot",
      text: "Hello sir. I'm CodeRabbit AI. I've inspected PR #24 and flagged a critical connection leak in error handling. What would you like to explore?",
    },
  ]);

  // Update if external data prop updates
  useEffect(() => {
    if ((data as any)?.mode === "pr_list" || ((data as any)?.activePrs && !(data as any)?.findings)) {
      setSubpage("active_prs");
    } else if ((data as any)?.findings) {
      setReviewData(data as CodeRabbitReviewData);
      setActivePrNumber((data as any).prNumber || 24);
      setSubpage("review");
    }
  }, [data]);

  const findings = reviewData?.findings || [];

  const counts = useMemo(() => {
    return {
      all: findings.length,
      critical: findings.filter((f) => f.severity === "critical").length,
      warning: findings.filter((f) => f.severity === "warning").length,
      suggestion: findings.filter((f) => f.severity === "suggestion").length,
      nitpick: findings.filter((f) => f.severity === "nitpick").length,
      praise: findings.filter((f) => f.severity === "praise").length,
    };
  }, [findings]);

  const filteredFindings = useMemo(() => {
    if (filter === "all") return findings;
    return findings.filter((f) => f.severity === filter);
  }, [findings, filter]);

  // Cohort groups for Left Change Stack
  const cohorts = useMemo(() => {
    const map: Record<string, { name: string; files: { file: string; additions: number; deletions: number; severity?: string }[] }> = {
      concurrency: {
        name: "Core Concurrency",
        files: [
          { file: "src-tauri/src/database/pool.rs", additions: 42, deletions: 12, severity: "critical" },
          { file: "src-tauri/src/cache/lru.rs", additions: 84, deletions: 18, severity: "warning" },
        ],
      },
      observability: {
        name: "Observability & Metrics",
        files: [
          { file: "src-tauri/src/database/metrics.rs", additions: 38, deletions: 10, severity: "suggestion" },
          { file: "src-tauri/src/database/health.rs", additions: 20, deletions: 6, severity: "praise" },
        ],
      },
      config: {
        name: "Configuration",
        files: [{ file: "src-tauri/src/config/pool_config.rs", additions: 15, deletions: 4, severity: "nitpick" }],
      },
    };
    return map;
  }, []);

  // Handle PR card click from Active PRs list
  const handleSelectPr = async (prNum: number) => {
    setLoadingPr(prNum);
    // Voice feedback acknowledgment "On it, sir."
    try {
      await invoke("speak_text", { text: "On it, sir." });
    } catch {
      // Ignored in browser fallback
    }

    try {
      const fetched = await invoke<CodeRabbitReviewData>("coderabbit_review_pr", { prNumber: prNum, repo: "ultron" });
      if (fetched && fetched.findings) {
        setReviewData(fetched);
      } else {
        setReviewData({
          ...SAMPLE_CODERABBIT_REVIEW,
          prNumber: prNum,
          title: prNum === 24 ? SAMPLE_CODERABBIT_REVIEW.title : `PR #${prNum} in ultron`,
        });
      }
    } catch {
      // Fallback to sample data for testing
      setReviewData({
        ...SAMPLE_CODERABBIT_REVIEW,
        prNumber: prNum,
        title: prNum === 24 ? SAMPLE_CODERABBIT_REVIEW.title : `PR #${prNum} in ultron`,
      });
    } finally {
      setTimeout(() => {
        setLoadingPr(null);
        setActivePrNumber(prNum);
        setSubpage("review");
      }, 450);
    }
  };

  const copyToClipboard = async (id: string, text?: string) => {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      setCopiedId(id);
      setTimeout(() => setCopiedId(null), 2000);
    } catch (err) {
      console.error("Failed to copy code snippet:", err);
    }
  };

  const handleAcceptFix = (id: string) => {
    setAcceptedFixes((prev) => new Set([...prev, id]));
  };

  const toggleViewed = (file: string) => {
    setViewedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(file)) next.delete(file);
      else next.add(file);
      return next;
    });
  };

  const handleSendChat = () => {
    if (!chatInput.trim()) return;
    const userMsg = chatInput.trim();
    setChatMessages((prev) => [...prev, { sender: "user", text: userMsg }]);
    setChatInput("");

    // Simulate instant AI CodeRabbit answer
    setTimeout(() => {
      let reply = "I've analyzed that code section. The fix safely addresses the issue without introducing new contention.";
      if (userMsg.toLowerCase().includes("leak") || userMsg.toLowerCase().includes("pool")) {
        reply =
          "In pool.rs line 48, when `conn.execute` returns an error, the connection wasn't recycled to the semaphore pool, causing slot starvation. Adding `pool.recycle(conn).await` guarantees the lease is restored immediately.";
      } else if (userMsg.toLowerCase().includes("lock") || userMsg.toLowerCase().includes("rwlock")) {
        reply =
          "In lru.rs line 108, holding a sync `std::sync::RwLock` across an async yield point can starve Tokio worker threads. Replacing it with `tokio::sync::RwLock` ensures safe cooperativeness.";
      } else if (userMsg.toLowerCase().includes("test")) {
        reply =
          "To test this fix, simulate 50 concurrent erroneous queries with `pool.acquire()`. Verify that `pool.available_permits()` remains at max capacity (24) after all errors are returned.";
      }
      setChatMessages((prev) => [...prev, { sender: "bot", text: reply }]);
    }, 300);
  };

  const getSeverityBadgeClass = (severity: string) => {
    switch (severity) {
      case "critical":
        return "cr-badge--critical";
      case "warning":
        return "cr-badge--warning";
      case "suggestion":
        return "cr-badge--suggestion";
      case "nitpick":
        return "cr-badge--nitpick";
      case "praise":
        return "cr-badge--praise";
      default:
        return "cr-badge--default";
    }
  };

  return (
    <div className="coderabbit-workstation">
      {/* ── Subpage Navigation & Global Header ── */}
      <div className="cr-topbar">
        <div className="cr-brand-cluster">
          <div className="cr-logo-gem">
            <svg width="22" height="22" viewBox="0 0 32 32" fill="none">
              <rect width="32" height="32" rx="8" fill="#FF5722" />
              <path
                d="M16 6C13.5 6 11.5 8 11.5 10.5C11.5 12 12.3 13.3 13.5 14C11.5 15 10 17.2 10 20C10 23.5 12.8 26 16 26C19.2 26 22 23.5 22 20C22 17.2 20.5 15 18.5 14C19.7 13.3 20.5 12 20.5 10.5C20.5 8 18.5 6 16 6Z"
                fill="white"
              />
              <circle cx="14" cy="11" r="1.2" fill="#FF5722" />
              <circle cx="18" cy="11" r="1.2" fill="#FF5722" />
              <ellipse cx="16" cy="14" rx="1.5" ry="1" fill="#FF5722" />
            </svg>
            <span className="cr-brand-name">CodeRabbit AI</span>
          </div>

          {/* Subpage Tabs (Click-to-Appear) */}
          <nav className="cr-subpage-nav">
            <button
              type="button"
              className={`cr-subpage-btn ${subpage === "active_prs" ? "cr-subpage-btn--active" : ""}`}
              onClick={() => setSubpage("active_prs")}
            >
              <span className="cr-nav-icon">📋</span> Active PRs
            </button>
            <button
              type="button"
              className={`cr-subpage-btn ${subpage === "review" ? "cr-subpage-btn--active" : ""}`}
              onClick={() => setSubpage("review")}
            >
              <span className="cr-nav-icon">🔍</span> PR Review #{activePrNumber}
            </button>
            <button
              type="button"
              className={`cr-subpage-btn ${subpage === "local" ? "cr-subpage-btn--active" : ""}`}
              onClick={() => setSubpage("local")}
            >
              <span className="cr-nav-icon">⚡</span> Local Review
            </button>
            <button
              type="button"
              className={`cr-subpage-btn ${subpage === "rules" ? "cr-subpage-btn--active" : ""}`}
              onClick={() => setSubpage("rules")}
            >
              <span className="cr-nav-icon">⚙</span> Rules (.yaml)
            </button>
          </nav>
        </div>

        {/* Right Action Hub: Overlay Drawer Triggers & Verdict */}
        <div className="cr-topbar-actions">
          {subpage === "review" && (
            <>
              {reviewData.verdict === "REQUEST_CHANGES" && (
                <span className="cr-verdict-pill cr-verdict--changes">Changes Requested</span>
              )}
              {reviewData.verdict === "APPROVE" && (
                <span className="cr-verdict-pill cr-verdict--approve">Approved</span>
              )}
              {reviewData.verdict === "COMMENT" && (
                <span className="cr-verdict-pill cr-verdict--comment">Review Complete</span>
              )}

              {/* Hover effect trigger pills for Right Overlay Drawer */}
              <button
                type="button"
                className={`cr-drawer-trigger ${overlayOpen && overlayTab === "summary" ? "cr-drawer-trigger--active" : ""}`}
                onClick={() => {
                  setOverlayTab("summary");
                  setOverlayOpen(!overlayOpen || overlayTab !== "summary");
                }}
                title="Toggle Risk & Walkthrough Drawer"
              >
                <span className="cr-trigger-badge">⚡</span>
                <span>Risk: {reviewData.riskScore || 78}/100</span>
              </button>

              <button
                type="button"
                className={`cr-drawer-trigger ${overlayOpen && overlayTab === "chat" ? "cr-drawer-trigger--active" : ""}`}
                onClick={() => {
                  setOverlayTab("chat");
                  setOverlayOpen(!overlayOpen || overlayTab !== "chat");
                }}
                title="Toggle CodeRabbit AI Chat Drawer"
              >
                <span className="cr-trigger-badge">💬</span>
                <span>Chat</span>
              </button>
            </>
          )}
        </div>
      </div>

      {/* ── Loading State with Pulsating Neon Spinner ── */}
      {loadingPr !== null && (
        <div className="cr-loading-curtain">
          <div className="cr-spinner-ring">
            <div className="cr-spinner-dot" />
          </div>
          <div className="cr-loading-title">Fetching CodeRabbit review for PR #{loadingPr}...</div>
          <div className="cr-loading-subtitle">Analyzing AST, call graphs &amp; concurrency safety</div>
        </div>
      )}

      {/* ── Sub-Page 1: Active PRs Selection Grid ── */}
      {subpage === "active_prs" && loadingPr === null && (
        <div className="cr-subpage-pane cr-pane--active-prs">
          <div className="cr-pane-header">
            <div>
              <h2 className="cr-pane-title">Active Pull Requests</h2>
              <p className="cr-pane-desc">Select any pull request to launch its full CodeRabbit inspection twin.</p>
            </div>
            <div className="cr-repo-pill">Repository: <code>ultron</code></div>
          </div>

          <div className="cr-prs-grid">
            {SAMPLE_ACTIVE_PRS.map((pr) => (
              <div
                key={pr.prNumber}
                className={`cr-pr-card ${activePrNumber === pr.prNumber ? "cr-pr-card--current" : ""}`}
                onClick={() => handleSelectPr(pr.prNumber)}
              >
                <div className="cr-pr-card-header">
                  <span className="cr-pr-num">#{pr.prNumber}</span>
                  <span
                    className={`cr-verdict-pill cr-verdict-pill--sm ${
                      pr.verdict === "REQUEST_CHANGES"
                        ? "cr-verdict--changes"
                        : pr.verdict === "APPROVE"
                        ? "cr-verdict--approve"
                        : "cr-verdict--comment"
                    }`}
                  >
                    {pr.verdict === "REQUEST_CHANGES"
                      ? "Changes Requested"
                      : pr.verdict === "APPROVE"
                      ? "Approved"
                      : "Review Complete"}
                  </span>
                </div>

                <div className="cr-pr-card-title">{pr.title}</div>

                <div className="cr-pr-branch-row">
                  <code>{pr.baseBranch}</code> &larr; <code>{pr.headBranch}</code>
                </div>

                <div className="cr-pr-footer-row">
                  <div className="cr-pr-diff-stats">
                    <span className="cr-stat--files">{pr.filesChanged} files</span>
                    <span className="cr-stat--add">+{pr.additions}</span>
                    <span className="cr-stat--del">-{pr.deletions}</span>
                  </div>
                  <div className="cr-pr-meta-sub">
                    <span>by {pr.author}</span> · <span>{pr.updatedAt}</span>
                  </div>
                </div>

                <div className="cr-pr-issues-tag">{pr.issuesSummary}</div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ── Sub-Page 2: Full 3-Panel PR Review Workspace ── */}
      {subpage === "review" && loadingPr === null && (
        <div className="cr-review-workspace">
          {/* Panel 1: Left Change Stack / Cohort Navigation (240px) */}
          <aside className="cr-change-stack">
            <div className="cr-stack-header">
              <button
                type="button"
                className="cr-back-btn"
                onClick={() => setSubpage("active_prs")}
                title="Return to Active PRs List"
              >
                &larr; All PRs
              </button>
              <div className="cr-stack-title">Change Stack</div>
            </div>

            {/* Viewed progress bar */}
            <div className="cr-progress-box">
              <div className="cr-progress-label">
                <span>Files Viewed</span>
                <span>
                  {viewedFiles.size} / {findings.length > 0 ? 4 : 1}
                </span>
              </div>
              <div className="cr-progress-track">
                <div
                  className="cr-progress-fill"
                  style={{ width: `${(viewedFiles.size / 4) * 100}%` }}
                />
              </div>
            </div>

            {/* Filter pills */}
            <div className="cr-stack-filters">
              <button
                type="button"
                className={`cr-filter-btn ${filter === "all" ? "cr-filter-btn--active" : ""}`}
                onClick={() => setFilter("all")}
              >
                All ({counts.all})
              </button>
              {counts.critical > 0 && (
                <button
                  type="button"
                  className={`cr-filter-btn cr-filter-btn--crit ${filter === "critical" ? "cr-filter-btn--active" : ""}`}
                  onClick={() => setFilter("critical")}
                >
                  Crit ({counts.critical})
                </button>
              )}
              {counts.warning > 0 && (
                <button
                  type="button"
                  className={`cr-filter-btn cr-filter-btn--warn ${filter === "warning" ? "cr-filter-btn--active" : ""}`}
                  onClick={() => setFilter("warning")}
                >
                  Warn ({counts.warning})
                </button>
              )}
            </div>

            {/* Cohort file list */}
            <div className="cr-cohorts-list">
              {Object.entries(cohorts).map(([key, cohort]) => (
                <div key={key} className="cr-cohort-group">
                  <div className="cr-cohort-title">▾ {cohort.name}</div>
                  <div className="cr-cohort-files">
                    {cohort.files.map((item) => {
                      const isSelected = selectedFile === item.file;
                      const isViewed = viewedFiles.has(item.file);
                      return (
                        <div
                          key={item.file}
                          className={`cr-file-item ${isSelected ? "cr-file-item--active" : ""}`}
                          onClick={() => setSelectedFile(item.file)}
                        >
                          <input
                            type="checkbox"
                            checked={isViewed}
                            onChange={(e) => {
                              e.stopPropagation();
                              toggleViewed(item.file);
                            }}
                            className="cr-file-checkbox"
                          />
                          <span className="cr-file-name" title={item.file}>
                            {item.file.split("/").pop()}
                          </span>
                          <span className="cr-file-diff">
                            +{item.additions}/-{item.deletions}
                          </span>
                          {item.severity === "critical" && <span className="cr-dot cr-dot--crit">●</span>}
                          {item.severity === "warning" && <span className="cr-dot cr-dot--warn">●</span>}
                        </div>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          </aside>

          {/* Panel 2: Center Diff & Visual Architecture (Full Remaining Width) */}
          <main className="cr-diff-canvas">
            {/* Diff Header Bar */}
            <div className="cr-diff-topbar">
              <div className="cr-diff-path-pill">
                <span className="cr-path-icon">📄</span>
                <code>{selectedFile}</code>
              </div>

              <div className="cr-diff-view-modes">
                <button
                  type="button"
                  className={`cr-mode-toggle ${diffMode === "unified" ? "cr-mode-toggle--active" : ""}`}
                  onClick={() => setDiffMode("unified")}
                >
                  Unified
                </button>
                <button
                  type="button"
                  className={`cr-mode-toggle ${diffMode === "split" ? "cr-mode-toggle--active" : ""}`}
                  onClick={() => setDiffMode("split")}
                >
                  Split
                </button>

                <button
                  type="button"
                  className={`cr-mark-viewed-btn ${viewedFiles.has(selectedFile) ? "cr-mark-viewed--checked" : ""}`}
                  onClick={() => toggleViewed(selectedFile)}
                >
                  {viewedFiles.has(selectedFile) ? "✓ Viewed" : "Mark Viewed"}
                </button>
              </div>
            </div>

            {/* Visual Architecture Sequence Diagram (Custom Crisp SVG Flow) */}
            <div className="cr-diagram-card">
              <div className="cr-diagram-header">
                <span className="cr-diagram-badge">FLOW</span>
                <span className="cr-diagram-heading">Connection Pool Lifecycle &amp; Concurrency Flow</span>
              </div>
              <div className="cr-svg-flow-wrap">
                <svg className="cr-svg-flow" viewBox="0 0 740 130" fill="none">
                  {/* Lifeline Actors */}
                  <rect x="20" y="10" width="100" height="26" rx="6" fill="#1f2937" stroke="#374151" />
                  <text x="70" y="27" fill="#f3f4f6" fontSize="11" textAnchor="middle" fontWeight="600">Client / Task</text>

                  <rect x="220" y="10" width="110" height="26" rx="6" fill="#1f2937" stroke="#FF5722" />
                  <text x="275" y="27" fill="#FF7043" fontSize="11" textAnchor="middle" fontWeight="600">Connection Pool</text>

                  <rect x="430" y="10" width="100" height="26" rx="6" fill="#1f2937" stroke="#374151" />
                  <text x="480" y="27" fill="#f3f4f6" fontSize="11" textAnchor="middle" fontWeight="600">Postgres DB</text>

                  <rect x="610" y="10" width="110" height="26" rx="6" fill="#2d1717" stroke="#ff453a" />
                  <text x="665" y="27" fill="#ff7b72" fontSize="11" textAnchor="middle" fontWeight="600">Error Interceptor</text>

                  {/* Flow Arrow 1: Client -> Pool acquire() */}
                  <path d="M 120 50 L 220 50" stroke="#FF7043" strokeWidth="2" markerEnd="url(#arrow)" />
                  <text x="170" y="44" fill="#9ca3af" fontSize="9" textAnchor="middle">1. acquire()</text>

                  {/* Flow Arrow 2: Pool -> DB execute(query) */}
                  <path d="M 330 50 L 430 50" stroke="#0a84ff" strokeWidth="2" />
                  <text x="380" y="44" fill="#9ca3af" fontSize="9" textAnchor="middle">2. query()</text>

                  {/* Flow Arrow 3: DB error -> Error Interceptor */}
                  <path d="M 530 65 L 610 65" stroke="#ff453a" strokeWidth="2" strokeDasharray="3 3" />
                  <text x="570" y="58" fill="#ff7b72" fontSize="9" textAnchor="middle">3. Err(Transient)</text>

                  {/* Flow Arrow 4 (The Fix): Error Interceptor -> Pool recycle() */}
                  <path d="M 665 80 L 665 110 L 275 110 L 275 60" stroke="#30d158" strokeWidth="2" fill="none" />
                  <text x="470" y="104" fill="#30d158" fontSize="9" textAnchor="middle">4. Fixed: pool.recycle(conn).await (restores lease)</text>

                  <defs>
                    <marker id="arrow" viewBox="0 0 10 10" refX="5" refY="5" markerWidth="6" markerHeight="6" orient="auto-start-reverse">
                      <path d="M 0 0 L 10 5 L 0 10 z" fill="#FF7043" />
                    </marker>
                  </defs>
                </svg>
              </div>
            </div>

            {/* Findings & Diff Viewer List */}
            <div className="cr-findings-scroller">
              {filteredFindings.map((finding) => (
                <div key={finding.id} className="cr-finding-block">
                  {/* Inline CodeRabbit Finding Card */}
                  <div className="cr-finding-box">
                    <div className="cr-box-header">
                      <div className="cr-box-meta">
                        <span className={`cr-severity-pill ${getSeverityBadgeClass(finding.severity)}`}>
                          {finding.severity.toUpperCase()}
                        </span>
                        {finding.category && <span className="cr-cat-tag">{finding.category}</span>}
                        <span className="cr-finding-file">
                          {finding.file}:{finding.lineRange}
                        </span>
                      </div>
                      <div className="cr-box-badge">CodeRabbit Bot</div>
                    </div>

                    <div className="cr-finding-headline">{finding.title}</div>
                    <p className="cr-finding-prose">{finding.description}</p>

                    {/* Diff viewer with Split or Unified rendering */}
                    {(finding.originalCode || finding.suggestedCode) && (
                      <div className={`cr-diff-container ${diffMode === "split" ? "cr-diff--split" : "cr-diff--unified"}`}>
                        {finding.originalCode && (
                          <div className="cr-diff-segment cr-diff--del">
                            <div className="cr-diff-legend">
                              <span>- Existing Code (Offending)</span>
                            </div>
                            <pre className="cr-code-pre">
                              <code>{finding.originalCode}</code>
                            </pre>
                          </div>
                        )}

                        {finding.suggestedCode && (
                          <div className="cr-diff-segment cr-diff--add">
                            <div className="cr-diff-legend">
                              <span>+ CodeRabbit Suggested Fix</span>
                              <div className="cr-diff-actions">
                                <button
                                  type="button"
                                  className="cr-btn cr-btn--copy"
                                  onClick={() => copyToClipboard(finding.id, finding.suggestedCode)}
                                >
                                  {copiedId === finding.id ? "Copied!" : "Copy Fix"}
                                </button>
                                <button
                                  type="button"
                                  className={`cr-btn cr-btn--accept ${acceptedFixes.has(finding.id) ? "cr-btn--accepted" : ""}`}
                                  onClick={() => handleAcceptFix(finding.id)}
                                >
                                  {acceptedFixes.has(finding.id) ? "✓ Accepted" : "Accept Suggestion"}
                                </button>
                              </div>
                            </div>
                            <pre className="cr-code-pre">
                              <code>{finding.suggestedCode}</code>
                            </pre>
                          </div>
                        )}
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </main>

          {/* Panel 3: Right Overlay Drawer (Slide-Over Sheet, Overlaying Center Diff) */}
          <aside className={`cr-overlay-drawer ${overlayOpen ? "cr-overlay-drawer--open" : ""}`}>
            <div className="cr-drawer-header">
              <div className="cr-drawer-tabs">
                <button
                  type="button"
                  className={`cr-drawer-tab ${overlayTab === "summary" ? "cr-drawer-tab--active" : ""}`}
                  onClick={() => setOverlayTab("summary")}
                >
                  ⚡ Summary &amp; Risk
                </button>
                <button
                  type="button"
                  className={`cr-drawer-tab ${overlayTab === "chat" ? "cr-drawer-tab--active" : ""}`}
                  onClick={() => setOverlayTab("chat")}
                >
                  💬 CodeRabbit Chat
                </button>
              </div>

              <button
                type="button"
                className="cr-drawer-close"
                onClick={() => setOverlayOpen(false)}
                title="Close Overlay (Esc)"
              >
                ✕
              </button>
            </div>

            <div className="cr-drawer-body">
              {overlayTab === "summary" && (
                <div className="cr-summary-tab-content">
                  {/* Risk Score Gauge */}
                  <div className="cr-risk-meter-box">
                    <div className="cr-risk-metric">
                      <div className="cr-risk-number">{reviewData.riskScore || 78}</div>
                      <div className="cr-risk-denom">/ 100</div>
                    </div>
                    <div className="cr-risk-details">
                      <div className="cr-risk-heading">{reviewData.riskLevel || "Moderate Risk"}</div>
                      <div className="cr-risk-sub">Potential resource starvation in error recovery</div>
                    </div>
                  </div>

                  {/* PR Overview Narrative */}
                  <div className="cr-drawer-section">
                    <h4 className="cr-sec-title">Review Summary</h4>
                    <p className="cr-sec-text">{reviewData.summary}</p>
                  </div>

                  {/* Modular Walkthrough Accordion */}
                  <div className="cr-drawer-section">
                    <h4 className="cr-sec-title">Modular Changes</h4>
                    <div className="cr-walkthrough-stack">
                      {reviewData.walkthrough?.map((item, idx) => (
                        <div key={idx} className="cr-walkthrough-card">
                          <div className="cr-walkthrough-head">
                            <span className="cr-num-circle">{idx + 1}</span>
                            <span>{item.title}</span>
                          </div>
                          <div className="cr-walkthrough-body">{item.details}</div>
                        </div>
                      ))}
                    </div>
                  </div>

                  {/* Whimsical AI Poem Card */}
                  {reviewData.poem && (
                    <div className="cr-poem-card">
                      <div className="cr-poem-title">📜 CodeRabbit Whimsical Poem</div>
                      <div className="cr-poem-text">{reviewData.poem}</div>
                    </div>
                  )}
                </div>
              )}

              {overlayTab === "chat" && (
                <div className="cr-chat-tab-content">
                  <div className="cr-chat-messages">
                    {chatMessages.map((msg, idx) => (
                      <div key={idx} className={`cr-chat-bubble cr-chat--${msg.sender}`}>
                        <div className="cr-bubble-author">{msg.sender === "bot" ? "CodeRabbit AI" : "You"}</div>
                        <div className="cr-bubble-text">{msg.text}</div>
                      </div>
                    ))}
                  </div>

                  {/* Prompt chips */}
                  <div className="cr-prompt-chips">
                    <button
                      type="button"
                      className="cr-chip"
                      onClick={() => setChatInput("Explain the connection leak in pool.rs")}
                    >
                      Explain connection leak
                    </button>
                    <button
                      type="button"
                      className="cr-chip"
                      onClick={() => setChatInput("Can RwLock in lru.rs cause Tokio starvation?")}
                    >
                      Can RwLock starve Tokio?
                    </button>
                    <button
                      type="button"
                      className="cr-chip"
                      onClick={() => setChatInput("Generate unit tests for this pool fix")}
                    >
                      Generate pool unit tests
                    </button>
                  </div>

                  {/* Chat input form */}
                  <div className="cr-chat-input-bar">
                    <input
                      type="text"
                      className="cr-chat-input"
                      placeholder="Ask CodeRabbit about findings..."
                      value={chatInput}
                      onChange={(e) => setChatInput(e.target.value)}
                      onKeyDown={(e) => e.key === "Enter" && handleSendChat()}
                    />
                    <button type="button" className="cr-chat-send" onClick={handleSendChat}>
                      Send
                    </button>
                  </div>
                </div>
              )}
            </div>
          </aside>
        </div>
      )}

      {/* ── Sub-Page 3: Local Staged Review (Pre-PR Mode) ── */}
      {subpage === "local" && (
        <div className="cr-subpage-pane cr-pane--local">
          <div className="cr-pane-header">
            <div>
              <h2 className="cr-pane-title">Local Pre-PR Review</h2>
              <p className="cr-pane-desc">Review your uncommitted and staged changes before opening a pull request.</p>
            </div>
            <button
              type="button"
              className="cr-btn cr-btn--refresh"
              onClick={() => alert("Scanned local staged git index: All 2 files clean!")}
            >
              🔄 Rescan Workspace
            </button>
          </div>

          <div className="cr-local-clean-card">
            <div className="cr-clean-badge">✓ Workspace Clean</div>
            <h3>Zero blocking bugs in staged changes</h3>
            <p>CodeRabbit analyzed your unstaged diffs: typing, lock guards, and resource lifetimes conform to repository standards.</p>
          </div>
        </div>
      )}

      {/* ── Sub-Page 4: Rules & Configuration (.coderabbit.yaml) ── */}
      {subpage === "rules" && (
        <div className="cr-subpage-pane cr-pane--rules">
          <div className="cr-pane-header">
            <div>
              <h2 className="cr-pane-title">Repository Rules (.coderabbit.yaml)</h2>
              <p className="cr-pane-desc">Configure CodeRabbit review guidelines, diagrams, and AI behavior.</p>
            </div>
          </div>

          <div className="cr-rules-editor-card">
            <pre className="cr-yaml-pre">
              <code>{`# .coderabbit.yaml (Active Configuration)
language: "en-US"
tone_instructions: "neutral, precise, highly technical, zero fluff"

reviews:
  profile: "chill"
  request_changes_workflow: true
  high_level_summary: true
  poem: true
  sequence_diagrams: true
  auto_review:
    enabled: true
    ignore_title_keywords:
      - "WIP"
      - "draft"
  path_filters:
    - "!target/**"
    - "!node_modules/**"
    - "!package-lock.json"`}</code>
            </pre>
          </div>
        </div>
      )}
    </div>
  );
};
