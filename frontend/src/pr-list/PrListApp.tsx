import { useEffect, useMemo } from "react";
import { usePrList, PrSummary, GitHubFeedItem, GitHubDashboardData } from "./prListStore";
import { applyThemeMode } from "../sidebar/theme";
import { GitHubContributionGraph } from "./GitHubContributionGraph";
import { GitHubActivityRadarGraph } from "./GitHubActivityRadarGraph";
import { GitHubSignInGate } from "./GitHubSignInGate";
import { GitHubReposView } from "./GitHubReposView";
import { GitHubStarredView } from "./GitHubStarredView";
import { DockButtons } from "../sidebar/kit/DockButtons";

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
    pinned,
    title,
    feedItems,
    orgs,
    repos,
    starred,
    activityStats,
    loading,
    actionInProgress,
    subpage,
    isAuthenticated,
    isAuthenticating,
    authChecked,
    authError,
    hide,
    clearPinned,
    setActionInProgress,
    removePr,
    setSubpage,
    setAuthenticated,
    setAuthenticating,
    setAuthChecked,
    setCurrentUser,
    setAuthError,
    loadDashboardData,
    applyDashboardData,
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

  // Listen for background sync updates to the dashboard data
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<GitHubDashboardData>("github:dashboard_updated", (ev) => {
        if (ev.payload) {
          applyDashboardData(ev.payload);
        }
      });
    })();

    return () => {
      unlisten?.();
    };
  }, [applyDashboardData]);

  // Check auth state on mount (or handle dev URL params) - prevents any login screen flash
  useEffect(() => {
    if (!isTauri()) {
      const params = new URLSearchParams(window.location.search);
      const authParam = params.get("auth");
      if (authParam === "1" || authParam === "true") {
        setAuthenticated(true);
      } else if (authParam === "0" || authParam === "false") {
        setAuthenticated(false);
      }
      setAuthChecked(true);
      return;
    }

    (async () => {
      try {
        const serverUrl = localStorage.getItem("nexus_server_url") || "";
        const profile = await tauriInvoke<any>("github_profile", { serverUrl, userId: "local-user" });
        if (profile && profile.login) {
          localStorage.setItem("nexus_github_authed", "true");
          localStorage.setItem("nexus_github_login", profile.login);
          setCurrentUser(profile.login);
          setAuthenticated(true);
        } else {
          const cachedAuth = localStorage.getItem("nexus_github_authed") === "true";
          setAuthenticated(cachedAuth);
        }
      } catch (e) {
        console.warn("[GitHub Dashboard] auth check fallback:", e);
        const cachedAuth = localStorage.getItem("nexus_github_authed") === "true";
        setAuthenticated(cachedAuth);
      } finally {
        setAuthChecked(true);
      }
    })();
  }, [setAuthenticated, setAuthChecked, setCurrentUser]);

  // When authenticated, trigger real GitHub data fetching (cached + background sync)
  useEffect(() => {
    if (isAuthenticated) {
      loadDashboardData();
    }
  }, [isAuthenticated, loadDashboardData]);

  // Re-check auth when this window regains focus after browser OAuth.
  useEffect(() => {
    if (!isTauri()) return;

    const handleFocus = async () => {
      if (usePrList.getState().isAuthenticated) return;
      try {
        const serverUrl = localStorage.getItem("nexus_server_url") || "";
        const profile = await tauriInvoke<any>("github_profile", { serverUrl, userId: "local-user" });
        if (profile && profile.login) {
          setAuthenticated(true);
        }
      } catch {}
    };

    window.addEventListener("focus", handleFocus);
    return () => {
      window.removeEventListener("focus", handleFocus);
    };
  }, [setAuthenticated]);

  // Fetch pending PR list data on mount (or auto-activate in browser dev mode)
  useEffect(() => {
    if (!isTauri()) {
      const params = new URLSearchParams(window.location.search);
      const subpageParam = (params.get("subpage") || "feed") as any;
      const requestedTheme = params.get("theme");
      if (requestedTheme === "dark" || requestedTheme === "light") {
        applyThemeMode(requestedTheme, false);
      }
      usePrList.getState().showPrList("", subpageParam, []);
      return;
    }

    (async () => {
      try {
        const pending = await tauriInvoke<any>("get_pending_pr_list");
        if (pending && pending.type === "pr_list") {
          console.log("[GitHub Dashboard] fetched pending PR list from Rust");
          usePrList.getState().showPrList(pending.repo, pending.state, pending.prs);
        } else {
          usePrList.getState().showFeedUnlessPinned();
        }
      } catch (e) {
        console.warn("[GitHub Dashboard] failed to fetch pending data:", e);
        usePrList.getState().showFeedUnlessPinned();
      }
    })();
  }, []);

  // Listen for sidebar:set_view to ensure pr-list is active and visible
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | null = null;
    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<{ view: string }>("sidebar:set_view", (ev) => {
        if (ev.payload?.view === "pr-list") {
          usePrList.getState().showFeedUnlessPinned();
        }
      });
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  // Handle Sign-in button click
  const handleSignIn = async (patToken?: string) => {
    setAuthenticating(true);
    setAuthError(null);
    if (!isTauri()) {
      // Standalone browser dev preview mode: simulate instant sign-in transition
      setTimeout(() => {
        setAuthenticating(false);
        setAuthenticated(true);
      }, 500);
      return;
    }

    try {
      if (patToken && patToken.trim()) {
        await tauriInvoke("vault_set_token", {
          service: "github",
          token: patToken.trim(),
          expiresInSecs: null,
        });
      }

      // Check profile using the unified resolver (which checks vault, gh CLI keyring, etc.)
      const serverUrl = localStorage.getItem("nexus_server_url") || "";
      const profile = await tauriInvoke<any>("github_profile", { serverUrl, userId: "local-user" });
      if (profile && profile.login) {
        localStorage.setItem("nexus_github_authed", "true");
        localStorage.setItem("nexus_github_login", profile.login);
        setCurrentUser(profile.login);
        setAuthenticated(true);
        setAuthChecked(true);
        loadDashboardData(true);
        return;
      }

      // If not resolved and no PAT provided, try OAuth if serverUrl exists
      if (serverUrl) {
        const { connectOAuth, setSidecarBaseUrl } = await import("../setup/oauth");
        setSidecarBaseUrl(serverUrl);
        const ok = await connectOAuth("github", "local-user");
        if (ok) {
          localStorage.setItem("nexus_github_authed", "true");
          setAuthenticated(true);
          setAuthChecked(true);
          loadDashboardData(true);
          return;
        }
      }
      setAuthError("No GitHub authentication found. Please sign in via 'gh auth login' or enter a Personal Access Token.");
    } catch (err) {
      console.error("[GitHub Dashboard] Sign in error:", err);
      setAuthError(err instanceof Error ? err.message : String(err));
    } finally {
      setAuthenticating(false);
    }
  };

  // The theme is not tracked here: the stylesheet follows `data-theme` on <html>, which sidebar/theme.ts keeps current.

  // Listen for sidebar:backdrop events
  useEffect(() => {
    if (!isTauri()) return;

    let unlisten: (() => void) | null = null;

    (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>("sidebar:backdrop", () => {});
    })();

    return () => {
      unlisten?.();
    };
  }, []);

  // Helper to determine the owner/repo identifier if owner is known
  const qualifiedRepo = (pr: PrSummary | GitHubFeedItem): string => {
    if (pr.repo.includes("/")) {
      return pr.repo;
    }
    const owner = (pr as PrSummary).repo_owner;
    if (owner && owner.trim()) {
      return `${owner.trim()}/${pr.repo}`;
    }
    return pr.repo;
  };

  // Handle PR actions
  const handleMerge = async (pr: PrSummary | GitHubFeedItem) => {
    setActionInProgress({ prNumber: pr.number, action: "merge" });
    const repoTarget = qualifiedRepo(pr);
    try {
      await tauriInvoke("orchestrator_process", {
        transcript: `merge pr ${pr.number} in ${repoTarget}`,
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
    const repoTarget = qualifiedRepo(pr);
    try {
      await tauriInvoke("orchestrator_process", {
        transcript: `analyse pr ${pr.number} in ${repoTarget}`,
        dialogContext: null,
      });
    } catch (e) {
      console.error("[GitHub Dashboard] analyse failed:", e);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleRemove = (pr: PrSummary | GitHubFeedItem) => {
    if (
      !window.confirm(
        `Dismiss PR #${pr.number} (${pr.repo}) from this list?\n\nIt stays open on GitHub — this only clears the local dashboard entry.`
      )
    ) {
      return;
    }
    removePr(pr.number, pr.repo);
  };

  const handleOpenExternal = (url: string) => {
    if (isTauri()) {
      import("@tauri-apps/plugin-shell")
        .then(({ open }) => open(url))
        .catch(() => window.open(url, "_blank"));
    } else {
      window.open(url, "_blank");
    }
  };

  // Merge live PRs from Rust into the feed if available; newest PR first (created_at desc)
  const allPullRequests: PrSummary[] = useMemo(() => {
    if (pinned) {
      return prs;
    }
    const list: PrSummary[] = prs.length > 0
      ? prs
      : feedItems
          .filter((i) => i.itemType === "pull_request")
          .map((i) => ({
            number: i.number,
            title: i.title,
            author: i.author,
            repo: i.repo,
            repo_owner: i.orgName || "",
            state: i.state,
            created_at: i.created_at,
            mergeable: i.mergeable,
            orgName: i.orgName,
            orgLogoUrl: i.orgLogoUrl,
            authorAvatarUrl: i.authorAvatarUrl,
            formattedTime: i.formattedTime,
          }));
    return list
      .slice()
      .sort((a, b) => {
        const ta = Date.parse(a.created_at || "") || 0;
        const tb = Date.parse(b.created_at || "") || 0;
        if (ta !== tb) return tb - ta;
        return b.number - a.number;
      });
  }, [pinned, prs, feedItems]);

  if (!visible) return null;

  return (
    <div className={`gh-dashboard-shell ${authChecked && !isAuthenticated ? "gh-shell--logged-out" : ""}`}>
      {/* ── Top Header Bar ── */}
      <header className={`gh-header-row ${authChecked && !isAuthenticated ? "gh-header--signin" : ""}`} data-tauri-drag-region>
        <div className="gh-brand-cluster" data-tauri-drag-region>
          <svg className="gh-octocat-logo" height="24" viewBox="0 0 16 16" width="24" fill="currentColor">
            <path d="M8 0c4.42 0 8 3.58 8 8a8.013 8.013 0 0 1-5.45 7.59c-.4.08-.55-.17-.55-.38 0-.27.01-1.13.01-2.2 0-.75-.25-1.23-.54-1.48 1.78-.2 3.65-.88 3.65-3.95 0-.88-.31-1.59-.82-2.15.08-.2.36-1.02-.08-2.12 0 0-.67-.22-2.2.82-.64-.18-1.32-.27-2-.27-.68 0-1.36.09-2 .27-1.53-1.03-2.2-.82-2.2-.82-.44 1.1-.16 1.92-.08 2.12-.51.56-.82 1.28-.82 2.15 0 3.06 1.86 3.75 3.64 3.95-.23.2-.44.55-.51 1.07-.46.21-1.61.55-2.33-.66-.15-.24-.6-.83-1.23-.82-.67.01-.27.38.01.53.34.19.73.9.82 1.13.16.45.68 1.31 2.69.94 0 .67.01 1.3.01 1.49 0 .21-.15.45-.55.38A7.995 7.995 0 0 1 0 8c0-4.42 3.58-8 8-8Z" />
          </svg>
          <div className="gh-title-block">
            <span className="gh-title-main">GitHub Dashboard</span>
          </div>
        </div>

        <div className="gh-header-actions">
          <DockButtons onDock={onDock} />
        </div>
      </header>

      {authChecked && !isAuthenticated ? (
        <GitHubSignInGate
          onSignIn={handleSignIn}
          isAuthenticating={isAuthenticating}
          error={authError}
        />
      ) : (
        <>
          {/* ── Main Scrollable Body ── */}
          <div className="gh-content-scroller">
            {(!authChecked || loading) && (
              <div className="gh-loading-bar">
                {!authChecked ? "Connecting to GitHub..." : "Fetching latest GitHub activity..."}
              </div>
            )}

            {/* ══ SUBPAGE 1: FEED (Home View) ══ */}
            {subpage === "feed" && (
              <div className="gh-feed-view">
                {/* ── GitHub Contribution Activity Graph ── */}
                <GitHubContributionGraph />

                {/* ── Graph 2: 4-Axis Activity Radar Graph ── */}
                <GitHubActivityRadarGraph
                  data={{
                    commitsPercent: activityStats.commits_pct,
                    pullRequestsPercent: activityStats.pull_requests_pct,
                    codeReviewPercent: activityStats.code_review_pct,
                    issuesPercent: activityStats.issues_pct,
                  }}
                />

                {/* ── Quick Access 4-Grid (Repos, PR's, Organization, Starred) ── */}
                <section className="gh-section gh-quick-section">
                  <h2 className="gh-section-title">Quick Access</h2>
                  <div className="gh-quick-grid">
                    {/* 1. Repos */}
                    <div
                      className="gh-quick-card gh-quick--repos"
                      onClick={() => setSubpage("repos")}
                    >
                      <span className="gh-quick-badge">{repos.length}</span>
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
                      <span className="gh-quick-badge">{allPullRequests.length}</span>
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
                      <span className="gh-quick-badge">{orgs.length}</span>
                      <div className="gh-quick-icon-wrap">
                        <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                          <path d="M1.75 1.5a.25.25 0 0 0-.25.25v12.5c0 .138.112.25.25.25h2.5a.25.25 0 0 0 .25-.25v-2.5a.25.25 0 0 1 .25-.25h6.5a.25.25 0 0 1 .25.25v2.5a.25.25 0 0 0 .25.25h2.5a.25.25 0 0 0 .25-.25V1.75a.25.25 0 0 0-.25-.25H1.75zM0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v12.5A1.75 1.75 0 0 1 14.25 16H1.75A1.75 1.75 0 0 1 0 14.25V1.75zM3.75 3h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5A.75.75 0 0 1 3 5.25v-1.5A.75.75 0 0 1 3.75 3zM4.5 4.5h1V4h-1v.5zM3.75 8h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5A.75.75 0 0 1 3 10.25v-1.5A.75.75 0 0 1 3.75 8zm.75 1.5h1V9h-1v.5zM9.75 3h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75v-1.5A.75.75 0 0 1 9.75 3zm.75 1.5h1V4h-1v.5zM9.75 8h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75v-1.5A.75.75 0 0 1 9.75 3zm.75 1.5h1V4h-1v.5zM9.75 8h2.5a.75.75 0 0 1 .75.75v1.5a.75.75 0 0 1-.75.75h-2.5a.75.75 0 0 1-.75-.75v-1.5a.75.75 0 0 1 .75-.75zm.75 1.5h1V9h-1v.5z" />
                        </svg>
                      </div>
                      <span className="gh-quick-name">Organization</span>
                    </div>

                    {/* 4. Starred */}
                    <div
                      className="gh-quick-card gh-quick--starred"
                      onClick={() => setSubpage("starred")}
                    >
                      <span className="gh-quick-badge">{starred.length}</span>
                      <div className="gh-quick-icon-wrap">
                        <svg width="22" height="22" viewBox="0 0 16 16" fill="currentColor">
                          <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.751.751 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.751.751 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
                        </svg>
                      </div>
                      <span className="gh-quick-name">Starred</span>
                    </div>
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
                      onClick={() => setSubpage("repos")}
                    >
                      + ADD FAVORITES
                    </button>
                  </div>
                </section>
              </div>
            )}

            {/* ══ SUBPAGE 2: REPOSITORIES ══ */}
            {subpage === "repos" && (
              <GitHubReposView onBack={() => setSubpage("feed")} />
            )}

            {/* ══ SUBPAGE 3: STARRED ══ */}
            {subpage === "starred" && (
              <GitHubStarredView onBack={() => setSubpage("feed")} />
            )}

            {/* ══ SUBPAGE 4: PULL REQUESTS ══ */}
            {subpage === "pull_requests" && (
              <div className="gh-subpage-view">
                <div className="gh-subpage-header">
                  <button
                    type="button"
                    className="gh-subpage-back"
                    onClick={() => setSubpage("feed")}
                  >
                    ← Back to Feed
                  </button>
                  {title ? (
                    <div className="gh-pr-list-title-row" style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginTop: "8px" }}>
                      <h2 className="gh-subpage-heading" style={{ margin: 0 }}>{title}</h2>
                      {pinned && (
                        <button
                          type="button"
                          className="gh-btn-show-all"
                          style={{ background: "none", border: "none", color: "var(--gh-accent, #0969da)", cursor: "pointer", fontSize: "12px", textDecoration: "underline" }}
                          onClick={() => clearPinned()}
                        >
                          Show all
                        </button>
                      )}
                    </div>
                  ) : null}
                </div>

                <div className="gh-cards-stack">
                  {allPullRequests.length === 0 ? (
                    <div className="gh-empty-card">
                      <p className="gh-empty-text">No open pull requests found.</p>
                    </div>
                  ) : (
                    allPullRequests.map((pr) => {
                      const repoAvatar =
                        pr.orgLogoUrl ||
                        repos.find(
                          (r) =>
                            r.name.toLowerCase() === pr.repo.toLowerCase() ||
                            r.full_name.toLowerCase().endsWith("/" + pr.repo.toLowerCase())
                        )?.owner_avatar_url ||
                        orgs.find(
                          (o) =>
                            (o.login && o.login.toLowerCase() === pr.repo_owner?.toLowerCase()) ||
                            (o.name && o.name.toLowerCase() === pr.repo_owner?.toLowerCase())
                        )?.avatar_url ||
                        (pr.repo_owner ? `https://github.com/${pr.repo_owner}.png` : undefined);

                      return (
                        <div key={`${pr.repo}#${pr.number}`} className="gh-card">
                          <div className="gh-card-topline">
                            <div className="gh-pr-repo-cluster">
                              {repoAvatar ? (
                                <img
                                  src={repoAvatar}
                                  alt={pr.repo_owner || pr.repo}
                                  className="gh-pr-repo-avatar"
                                  loading="lazy"
                                  onError={(e) => {
                                    (e.currentTarget as HTMLElement).style.display = "none";
                                  }}
                                />
                              ) : (
                                <div className="gh-pr-repo-avatar gh-pr-repo-avatar--fallback">
                                  {(pr.repo_owner || pr.repo || "R").charAt(0).toUpperCase()}
                                </div>
                              )}
                              <div className="gh-card-type-tag">
                                <span className="gh-pr-num">#{pr.number}</span>
                                <span className="gh-repo-tag">{pr.repo}</span>
                              </div>
                            </div>
                            <span className="gh-card-timestamp">{timeAgo(pr.created_at) || pr.formattedTime}</span>
                          </div>

                          <div className="gh-card-title">{pr.title}</div>

                          <div className="gh-attribution-row">
                            <span className="gh-opened-by">opened by</span>
                            {pr.authorAvatarUrl || pr.author ? (
                              <img
                                src={pr.authorAvatarUrl || `https://github.com/${pr.author}.png`}
                                alt={pr.author}
                                className="gh-pr-author-avatar"
                                loading="lazy"
                                onError={(e) => {
                                  (e.currentTarget as HTMLElement).style.display = "none";
                                }}
                              />
                            ) : null}
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
                              {actionInProgress?.prNumber === pr.number && actionInProgress.action === "analyse"
                                ? "Analysing..."
                                : "Analyse"}
                            </button>
                            <button
                              type="button"
                              className="gh-btn gh-btn--merge"
                              disabled={actionInProgress !== null || pr.mergeable === false}
                              onClick={() => handleMerge(pr)}
                            >
                              {actionInProgress?.prNumber === pr.number && actionInProgress.action === "merge"
                                ? "Merging..."
                                : "Merge PR"}
                            </button>
                            <button
                              type="button"
                              className="gh-btn gh-btn--delete"
                              disabled={actionInProgress !== null}
                              onClick={() => handleRemove(pr)}
                            >
                              Delete
                            </button>
                          </div>
                        </div>
                      );
                    })
                  )}
                </div>
              </div>
            )}

            {/* ══ SUBPAGE 5: ISSUES ══ */}
            {subpage === "issues" && (
              <div className="gh-subpage-view">
                <div className="gh-subpage-header">
                  <button
                    type="button"
                    className="gh-subpage-back"
                    onClick={() => setSubpage("feed")}
                  >
                    ← Back to Feed
                  </button>
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

            {/* ══ SUBPAGE 6: ORGANIZATIONS ══ */}
            {subpage === "organizations" && (
              <div className="gh-subpage-view">
                <div className="gh-subpage-header">
                  <button
                    type="button"
                    className="gh-subpage-back"
                    onClick={() => setSubpage("feed")}
                  >
                    ← Back to Feed
                  </button>
                </div>

                <div className="gh-orgs-grid">
                  {orgs.length === 0 ? (
                    <div className="gh-empty-card">
                      <p className="gh-empty-text">No organizations found for your account.</p>
                    </div>
                  ) : (
                    orgs.map((org) => (
                      <div
                        key={org.id}
                        className="gh-org-card"
                        style={{ cursor: "pointer" }}
                        onClick={() => handleOpenExternal(org.html_url || `https://github.com/${org.login || org.name}`)}
                        title="Open on GitHub"
                      >
                        <div className="gh-org-card-head">
                          {org.avatar_url ? (
                            <img src={org.avatar_url} alt={org.name} className="gh-org-avatar" style={{ objectFit: "cover" }} />
                          ) : (
                            <div className="gh-org-avatar" style={{ backgroundColor: "rgba(255, 255, 255, 0.12)", color: "#ffffff" }}>
                              {org.logoText || org.name.slice(0, 2).toUpperCase()}
                            </div>
                          )}
                          <div className="gh-org-info">
                            <div className="gh-org-name">{org.name}</div>
                            <div className="gh-org-div">{org.division}</div>
                          </div>
                        </div>
                        <div className="gh-org-badge-chip">{org.mentionsCount || 0} Mentions</div>
                      </div>
                    ))
                  )}
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
        </>
      )}
    </div>
  );
}
