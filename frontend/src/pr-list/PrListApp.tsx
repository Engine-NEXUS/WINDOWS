import { useEffect, useMemo } from "react";
import { usePrList, PrSummary, GitHubFeedItem } from "./prListStore";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error("Not in Tauri");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

/** Format ISO timestamp to a relative time string. */
function timeAgo(iso: string): string {
  if (!iso) return "";
  const then = new Date(iso).getTime();
  if (isNaN(then)) return "";
  const secs = Math.floor((Date.now() - then) / 1000);
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

export function PrListApp({ onDock = () => {} }: { onDock?: (d: string) => void }) {
  const {
    visible,
    prs,
    feedItems,
    orgs,
    loading,
    actionInProgress,
    subpage,
    accountFilter,
    attentionFilter,
    theme,
    hide,
    setActionInProgress,
    removePr,
    setSubpage,
    setAccountFilter,
    setAttentionFilter,
    toggleTheme,
  } = usePrList();

  // Listen for orchestrator github_result events with PrList data
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | null = null;

    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<any>("orchestrator:event", (ev) => {
        const payload = ev.payload;
        if (!payload || payload.type !== "github_result") return;

        const result = payload.result;
        if (!result || typeof result !== "object") return;

        if (result.type !== "pr_list") return;

        usePrList.getState().showPrList(result.repo, result.state, result.prs);
      });
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  // Fetch pending PR list data on mount
  useEffect(() => {
    if (!isTauri()) return;

    (async () => {
      try {
        const pending = await tauriInvoke<any>("get_pending_pr_list");
        if (pending && pending.type === "pr_list") {
          console.log("[GitHub Dashboard] fetched pending PR list from Rust");
          usePrList.getState().showPrList(pending.repo, pending.state, pending.prs);
        }
      } catch (e) {
        console.warn("[GitHub Dashboard] failed to fetch pending data:", e);
      }
    })();
  }, []);

  // Listen for sidebar:backdrop events
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | null = null;

    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>("sidebar:backdrop", (ev) => {
        const dataUri = ev.payload;
        if (typeof dataUri === "string" && dataUri.startsWith("data:image/")) {
          document.documentElement.style.setProperty("--sidebar-backdrop-image", `url("${dataUri}")`);
        }
      });
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  // Handle PR actions
  const handleMerge = async (pr: PrSummary | GitHubFeedItem) => {
    setActionInProgress({ prNumber: pr.number, action: "merge" });
    try {
      await tauriInvoke("orchestrator_process", {
        transcript: `merge pr ${pr.number} in ${pr.repo}`,
        dialogContext: null,
      });
      removePr(pr.number, pr.repo);
    } catch (e) {
      console.error("[GitHub Dashboard] merge failed:", e);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleAnalyse = async (pr: PrSummary | GitHubFeedItem) => {
    setActionInProgress({ prNumber: pr.number, action: "analyse" });
    hide();
    tauriInvoke("hide_pr_list_sidebar").catch(() => {});
    try {
      await tauriInvoke("orchestrator_process", {
        transcript: `analyse pr ${pr.number} in ${pr.repo}`,
        dialogContext: null,
      });
    } catch (e) {
      console.error("[GitHub Dashboard] analyse failed:", e);
    } finally {
      setActionInProgress(null);
    }
  };

  // Filter feed items according to accountFilter and attentionFilter
  const filteredFeed = useMemo(() => {
    let list = feedItems;

    // Filter by account type
    if (accountFilter === "organization") {
      list = list.filter((i) => i.accountType === "organization");
    } else if (accountFilter === "personal") {
      list = list.filter((i) => i.accountType === "personal");
    }

    // Filter by attention tab
    if (attentionFilter === "pull_requests") {
      list = list.filter((i) => i.itemType === "pull_request");
    } else if (attentionFilter === "issues") {
      list = list.filter((i) => i.itemType === "issue");
    }

    return list;
  }, [feedItems, accountFilter, attentionFilter]);

  // Merge live PRs from Rust into the feed if available
  const allPullRequests = useMemo(() => {
    if (prs.length > 0) {
      return prs;
    }
    return feedItems
      .filter((i) => i.itemType === "pull_request")
      .map((i) => ({
        number: i.number,
        title: i.title,
        author: i.author,
        repo: i.repo,
        state: i.state,
        created_at: i.created_at,
        mergeable: i.mergeable,
        orgName: i.orgName,
        orgLogoUrl: i.orgLogoUrl,
        authorAvatarUrl: i.authorAvatarUrl,
        formattedTime: i.formattedTime,
      }));
  }, [prs, feedItems]);

  const accountCounts = useMemo(() => {
    return {
      all: feedItems.length,
      org: feedItems.filter((i) => i.accountType === "organization").length,
      personal: feedItems.filter((i) => i.accountType === "personal").length,
    };
  }, [feedItems]);

  if (!visible) return null;

  return (
    <div className={`gh-dashboard-shell theme--${theme}`}>
      {/* ── Top Header Bar (No Phone Status Bar, No Search Bar) ── */}
      <header className="gh-header-row" data-tauri-drag-region>
        <div className="gh-brand-cluster" data-tauri-drag-region>
          <svg className="gh-octocat-logo" height="24" viewBox="0 0 16 16" width="24" fill="currentColor">
            <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
          </svg>
          <div className="gh-title-block">
            <span className="gh-title-main">GitHub Dashboard</span>
            <span className="gh-user-pill">@lakshya</span>
          </div>
        </div>

        <div className="gh-header-actions">
          {/* Sun / Moon Theme Toggle */}
          <button
            type="button"
            className="gh-theme-btn"
            onClick={toggleTheme}
            title={`Switch to ${theme === "light" ? "Dark" : "Light"} Theme`}
          >
            {theme === "light" ? "🌙 Dark" : "☀️ Light"}
          </button>

          {/* Dock Controls */}
          <div className="sidebar-dock-controls">
            <button type="button" className="sidebar-dock-btn" onClick={() => onDock("left")} title="Dock Left">
              ◧
            </button>
            <button type="button" className="sidebar-dock-btn" onClick={() => onDock("right")} title="Dock Right">
              ◨
            </button>
          </div>
        </div>
      </header>

      {/* ── Organization vs Personal Account Filter Bar ── */}
      <div className="gh-account-bar">
        <button
          type="button"
          className={`gh-account-pill ${accountFilter === "all" ? "gh-account-pill--active" : ""}`}
          onClick={() => setAccountFilter("all")}
        >
          All ({accountCounts.all})
        </button>
        <button
          type="button"
          className={`gh-account-pill ${accountFilter === "organization" ? "gh-account-pill--active" : ""}`}
          onClick={() => setAccountFilter("organization")}
        >
          🏢 Organizations ({accountCounts.org})
        </button>
        <button
          type="button"
          className={`gh-account-pill ${accountFilter === "personal" ? "gh-account-pill--active" : ""}`}
          onClick={() => setAccountFilter("personal")}
        >
          👤 Personal ({accountCounts.personal})
        </button>
      </div>

      {/* ── Subpage Navigation Tabs ── */}
      <nav className="gh-subnav-bar">
        <button
          type="button"
          className={`gh-subnav-btn ${subpage === "feed" ? "gh-subnav-btn--active" : ""}`}
          onClick={() => setSubpage("feed")}
        >
          <span className="gh-tab-icon">📋</span> Feed
        </button>
        <button
          type="button"
          className={`gh-subnav-btn ${subpage === "pull_requests" ? "gh-subnav-btn--active" : ""}`}
          onClick={() => setSubpage("pull_requests")}
        >
          <span className="gh-tab-icon">🔀</span> Pull Requests ({allPullRequests.length})
        </button>
        <button
          type="button"
          className={`gh-subnav-btn ${subpage === "issues" ? "gh-subnav-btn--active" : ""}`}
          onClick={() => setSubpage("issues")}
        >
          <span className="gh-tab-icon">🐞</span> Issues (1)
        </button>
        <button
          type="button"
          className={`gh-subnav-btn ${subpage === "organizations" ? "gh-subnav-btn--active" : ""}`}
          onClick={() => setSubpage("organizations")}
        >
          <span className="gh-tab-icon">🏢</span> Organizations ({orgs.length})
        </button>
      </nav>

      {/* ── Main Scrollable Body ── */}
      <div className="gh-content-scroller">
        {loading && <div className="gh-loading-bar">Fetching latest GitHub activity...</div>}

        {/* ══ SUBPAGE 1: FEED (Home View) ══ */}
        {subpage === "feed" && (
          <div className="gh-feed-view">
            {/* Requires Attention Section */}
            <section className="gh-section gh-attention-section">
              <div className="gh-section-header">
                <h2 className="gh-section-title">Requires Attention</h2>
              </div>

              {/* Attention sub-tabs: All | Pull Requests | Issues */}
              <div className="gh-attention-tabs">
                <button
                  type="button"
                  className={`gh-att-tab ${attentionFilter === "all" ? "gh-att-tab--active" : ""}`}
                  onClick={() => setAttentionFilter("all")}
                >
                  All
                </button>
                <button
                  type="button"
                  className={`gh-att-tab ${attentionFilter === "pull_requests" ? "gh-att-tab--active" : ""}`}
                  onClick={() => setAttentionFilter("pull_requests")}
                >
                  Pull Requests
                </button>
                <button
                  type="button"
                  className={`gh-att-tab ${attentionFilter === "issues" ? "gh-att-tab--active" : ""}`}
                  onClick={() => setAttentionFilter("issues")}
                >
                  Issues
                </button>
              </div>

              {/* Feed Items (Ordered by Date & Time with Org & Author Attribution) */}
              <div className="gh-cards-stack">
                {filteredFeed.map((item) => (
                  <div key={item.id} className="gh-card">
                    {/* Item Badge & Icon Row */}
                    <div className="gh-card-topline">
                      <div className="gh-card-type-tag">
                        {item.itemType === "pull_request" ? (
                          <>
                            <svg className="gh-type-svg gh-svg--pr" width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                              <path d="M7.177 3.073L9.573.677A.25.25 0 0110 .854v4.792a.25.25 0 01-.427.177L7.177 3.427a.25.25 0 010-.354zM3.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122v5.256a2.251 2.251 0 11-1.5 0V5.372A2.25 2.25 0 011.5 3.25zM11 2.5h-1V4h1a1 1 0 011 1v5.628a2.251 2.251 0 101.5 0V5A2.5 2.5 0 0011 2.5zm1 10.25a.75.75 0 111.5 0 .75.75 0 01-1.5 0zM3.75 12a.75.75 0 100 1.5.75.75 0 000-1.5z" />
                            </svg>
                            <span className="gh-type-label">Pull Request</span>
                          </>
                        ) : (
                          <>
                            <svg className="gh-type-svg gh-svg--issue" width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                              <path d="M8 9.5a1.5 1.5 0 100-3 1.5 1.5 0 000 3z" />
                              <path fillRule="evenodd" d="M8 0a8 8 0 100 16A8 8 0 008 0zM1.5 8a6.5 6.5 0 1113 0 6.5 6.5 0 01-13 0z" />
                            </svg>
                            <span className="gh-type-label">Issue</span>
                          </>
                        )}
                      </div>
                      <span className="gh-card-timestamp">{item.formattedTime}</span>
                    </div>

                    {/* Headline Title */}
                    <div className="gh-card-title">{item.title}</div>

                    {/* Attribution Row: Org Logo + Target Repo + Author Avatar + Username */}
                    <div className="gh-attribution-row">
                      {/* Organization Logo badge */}
                      {item.accountType === "organization" && (
                        <div className="gh-org-badge" title={`Organization: ${item.orgName}`}>
                          {item.orgLogoUrl ? (
                            <img src={item.orgLogoUrl} alt={item.orgName} className="gh-org-img" />
                          ) : (
                            <span className="gh-org-initials">🏢</span>
                          )}
                        </div>
                      )}

                      <span className="gh-repo-tag">{item.repo}</span>
                      <span className="gh-meta-sep">·</span>
                      <span className="gh-opened-by">opened by</span>

                      {/* Small circular profile photo */}
                      {item.authorAvatarUrl ? (
                        <img
                          src={item.authorAvatarUrl}
                          alt={item.author}
                          className="gh-user-avatar-sm"
                        />
                      ) : (
                        <div className="gh-user-avatar-placeholder">
                          {item.author.charAt(0).toUpperCase()}
                        </div>
                      )}

                      <span className="gh-author-name">@{item.author}</span>
                    </div>

                    {/* Action buttons if PR */}
                    {item.itemType === "pull_request" && (
                      <div className="gh-card-actions">
                        <button
                          type="button"
                          className="gh-btn gh-btn--analyse"
                          disabled={actionInProgress !== null}
                          onClick={() => handleAnalyse(item)}
                        >
                          Analyse
                        </button>
                        <button
                          type="button"
                          className="gh-btn gh-btn--merge"
                          disabled={actionInProgress !== null}
                          onClick={() => handleMerge(item)}
                        >
                          Merge PR
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>

              {/* View All link */}
              <button
                type="button"
                className="gh-view-all-link"
                onClick={() => setSubpage("pull_requests")}
              >
                View All ( 55 ) &rarr;
              </button>
            </section>

            {/* ── Quick Access 4-Grid (Repos, PR's, Organization, Starred) ── */}
            <section className="gh-section gh-quick-section">
              <h2 className="gh-section-title">Quick Access</h2>
              <div className="gh-quick-grid">
                {/* 1. Repos */}
                <div
                  className="gh-quick-card gh-quick--repos"
                  onClick={() => alert("Viewing all 18 repositories in ultron.")}
                >
                  <div className="gh-quick-icon-wrap">
                    <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M2 2.5A2.5 2.5 0 0 1 4.5 0h8.75a.75.75 0 0 1 .75.75v12.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1 0-1.5h1.75v-2h-8a1 1 0 0 0-.714 1.7.75.75 0 1 1-1.072 1.05A2.495 2.495 0 0 1 2 11.5v-9zm10.5-1h-8a1 1 0 0 0-1 1v6.708A2.486 2.486 0 0 1 4.5 9h8V1.5z" />
                    </svg>
                  </div>
                  <span className="gh-quick-name">Repos</span>
                </div>

                {/* 2. PR's */}
                <div
                  className="gh-quick-card gh-quick--prs"
                  onClick={() => setSubpage("pull_requests")}
                >
                  <div className="gh-quick-icon-wrap">
                    <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M7.177 3.073L9.573.677A.25.25 0 0110 .854v4.792a.25.25 0 01-.427.177L7.177 3.427a.25.25 0 010-.354zM3.75 2.5a.75.75 0 100 1.5.75.75 0 000-1.5zm-2.25.75a2.25 2.25 0 113 2.122v5.256a2.251 2.251 0 11-1.5 0V5.372A2.25 2.25 0 011.5 3.25zM11 2.5h-1V4h1a1 1 0 011 1v5.628a2.251 2.251 0 101.5 0V5A2.5 2.5 0 0011 2.5zm1 10.25a.75.75 0 111.5 0 .75.75 0 01-1.5 0zM3.75 12a.75.75 0 100 1.5.75.75 0 000-1.5z" />
                    </svg>
                  </div>
                  <span className="gh-quick-name">PR's</span>
                </div>

                {/* 3. Organization */}
                <div
                  className="gh-quick-card gh-quick--orgs"
                  onClick={() => setSubpage("organizations")}
                >
                  <div className="gh-quick-icon-wrap">
                    <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M1.75 1.5a.25.25 0 0 0-.25.25v12.5c0 .138.112.25.25.25h2.5a.25.25 0 0 0 .25-.25v-2.5a.25.25 0 0 1 .25-.25h6.5a.25.25 0 0 1 .25.25v2.5a.25.25 0 0 0 .25.25h2.5a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H1.75zM0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v12.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V1.75zM3.75 3h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5A.75.75 0 0 1 3 5.25v-1.5A.75.75 0 0 1 3.75 3zM4.5 4.5h1V4h-1v.5zM3.75 8h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5A.75.75 0 0 1 3 10.25v-1.5A.75.75 0 0 1 3.75 8zm.75 1.5h1V9h-1v.5zM9.75 3h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75v-1.5A.75.75 0 0 1 9.75 3zm.75 1.5h1V4h-1v.5zM9.75 8h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75v-1.5a.75.75 0 0 1 .75-.75zm.75 1.5h1V9h-1v.5z" />
                    </svg>
                  </div>
                  <span className="gh-quick-name">Organization</span>
                </div>

                {/* 4. Starred */}
                <div
                  className="gh-quick-card gh-quick--starred"
                  onClick={() => alert("Showing 12 starred repositories.")}
                >
                  <div className="gh-quick-icon-wrap">
                    <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                      <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.751.751 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
                    </svg>
                  </div>
                  <span className="gh-quick-name">Starred</span>
                </div>
              </div>
            </section>

            {/* ── Organisations Section ── */}
            <section className="gh-section gh-orgs-section">
              <h2 className="gh-section-title">Organisations</h2>
              <div className="gh-orgs-grid">
                {orgs.map((org) => (
                  <div key={org.id} className="gh-org-card">
                    <div className="gh-org-card-head">
                      <div className="gh-org-avatar" style={{ backgroundColor: org.logoBg }}>
                        {org.logoText}
                      </div>
                      <div className="gh-org-info">
                        <div className="gh-org-name">{org.name}</div>
                        <div className="gh-org-div">{org.division}</div>
                      </div>
                    </div>
                    <div className="gh-org-badge-chip">{org.mentionsCount} Mentions</div>
                  </div>
                ))}
              </div>
            </section>

            {/* ── Favorites Section ── */}
            <section className="gh-section gh-favs-section">
              <h2 className="gh-section-title">Favorites</h2>
              <div className="gh-fav-prompt-card">
                <p className="gh-fav-desc">
                  Add favorite repositories for quick access at any time, without having to search
                </p>
                <button
                  type="button"
                  className="gh-btn-add-fav"
                  onClick={() => alert("Added current active repository to favorites!")}
                >
                  + ADD FAVORITES
                </button>
              </div>
            </section>
          </div>
        )}

        {/* ══ SUBPAGE 2: PULL REQUESTS ══ */}
        {subpage === "pull_requests" && (
          <div className="gh-subpage-view">
            <div className="gh-subpage-header">
              <h2 className="gh-subpage-heading">Pull Requests ({allPullRequests.length})</h2>
              <p className="gh-subpage-sub">Open code reviews and active branches requiring review or merge.</p>
            </div>

            <div className="gh-cards-stack">
              {allPullRequests.map((pr) => (
                <div key={`${pr.repo}#${pr.number}`} className="gh-card">
                  <div className="gh-card-topline">
                    <div className="gh-card-type-tag">
                      <span className="gh-pr-num">#{pr.number}</span>
                      <span className="gh-repo-tag">{pr.repo}</span>
                    </div>
                    <span className="gh-card-timestamp">{timeAgo(pr.created_at) || pr.formattedTime}</span>
                  </div>

                  <div className="gh-card-title">{pr.title}</div>

                  <div className="gh-attribution-row">
                    <span className="gh-opened-by">opened by</span>
                    <span className="gh-author-name">@{pr.author}</span>
                    {pr.mergeable !== undefined && pr.mergeable !== null && (
                      <span className={`gh-mergeable-tag ${pr.mergeable ? "gh-mergeable--clean" : "gh-mergeable--conflict"}`}>
                        {pr.mergeable ? "Clean to merge" : "Merge conflicts"}
                      </span>
                    )}
                  </div>

                  <div className="gh-card-actions">
                    <button
                      type="button"
                      className="gh-btn gh-btn--analyse"
                      disabled={actionInProgress !== null}
                      onClick={() => handleAnalyse(pr)}
                    >
                      Analyse
                    </button>
                    <button
                      type="button"
                      className="gh-btn gh-btn--merge"
                      disabled={actionInProgress !== null}
                      onClick={() => handleMerge(pr)}
                    >
                      Merge PR
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* ══ SUBPAGE 3: ISSUES ══ */}
        {subpage === "issues" && (
          <div className="gh-subpage-view">
            <div className="gh-subpage-header">
              <h2 className="gh-subpage-heading">Issues (1)</h2>
              <p className="gh-subpage-sub">Tracked bugs and enhancement requests across repositories.</p>
            </div>

            <div className="gh-cards-stack">
              {feedItems
                .filter((i) => i.itemType === "issue")
                .map((issue) => (
                  <div key={issue.id} className="gh-card">
                    <div className="gh-card-topline">
                      <div className="gh-card-type-tag">
                        <span className="gh-issue-num">#{issue.number}</span>
                        <span className="gh-repo-tag">{issue.repo}</span>
                      </div>
                      <span className="gh-card-timestamp">{issue.formattedTime}</span>
                    </div>
                    <div className="gh-card-title">{issue.title}</div>
                    <div className="gh-attribution-row">
                      <span className="gh-opened-by">reported by</span>
                      <span className="gh-author-name">@{issue.author}</span>
                    </div>
                  </div>
                ))}
            </div>
          </div>
        )}

        {/* ══ SUBPAGE 4: ORGANIZATIONS ══ */}
        {subpage === "organizations" && (
          <div className="gh-subpage-view">
            <div className="gh-subpage-header">
              <h2 className="gh-subpage-heading">Organizations ({orgs.length})</h2>
              <p className="gh-subpage-sub">Teams and organizations where your account has membership access.</p>
            </div>

            <div className="gh-orgs-grid">
              {orgs.map((org) => (
                <div key={org.id} className="gh-org-card">
                  <div className="gh-org-card-head">
                    <div className="gh-org-avatar" style={{ backgroundColor: org.logoBg }}>
                      {org.logoText}
                    </div>
                    <div className="gh-org-info">
                      <div className="gh-org-name">{org.name}</div>
                      <div className="gh-org-div">{org.division}</div>
                    </div>
                  </div>
                  <div className="gh-org-badge-chip">{org.mentionsCount} Mentions</div>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      {/* ── Footer ── */}
      <footer className="gh-footer-bar">
        <span>Esc to close</span>
        <span>·</span>
        <span>Ctrl+Space for Voice</span>
      </footer>
    </div>
  );
}
