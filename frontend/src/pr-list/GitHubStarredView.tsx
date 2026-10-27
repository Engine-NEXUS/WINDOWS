import { useState, useMemo } from "react";
import { usePrList, GitHubStarredCard } from "./prListStore";

function isTauri(): boolean {
  return typeof (window as any).__TAURI_INTERNALS__ !== "undefined";
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) throw new Error("Not in Tauri");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

function timeAgo(iso: string | null | undefined): string {
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

export function GitHubStarredView({ onBack }: { onBack: () => void }) {
  const { starred, hide } = usePrList();
  const [searchQuery, setSearchQuery] = useState("");
  const [analyzingRepo, setAnalyzingRepo] = useState<string | null>(null);

  const filteredStarred = useMemo(() => {
    if (!searchQuery.trim()) return starred;
    const q = searchQuery.toLowerCase();
    return starred.filter(
      (r) =>
        r.name.toLowerCase().includes(q) ||
        r.full_name.toLowerCase().includes(q) ||
        (r.description && r.description.toLowerCase().includes(q)) ||
        (r.language && r.language.toLowerCase().includes(q))
    );
  }, [starred, searchQuery]);

  const handleAnalyse = async (repo: GitHubStarredCard) => {
    setAnalyzingRepo(repo.name);
    hide();
    if (isTauri()) {
      tauriInvoke("hide_pr_list_sidebar").catch(() => {});
      try {
        await tauriInvoke("orchestrator_process", {
          transcript: `analyse ${repo.name}`,
          dialogContext: null,
        });
      } catch (e) {
        console.error("[GitHub Dashboard] analyse repo failed:", e);
      } finally {
        setAnalyzingRepo(null);
      }
    } else {
      setTimeout(() => setAnalyzingRepo(null), 1000);
    }
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

  return (
    <div className="gh-subpage-view">
      {/* ── Subpage Header ── */}
      <div className="gh-subpage-header">
        <button type="button" className="gh-subpage-back" onClick={onBack}>
          ← Back to Feed
        </button>
        <div className="gh-subpage-title-row">
          <h2 className="gh-subpage-heading">Starred Repositories ({filteredStarred.length})</h2>
        </div>
        <p className="gh-subpage-sub">
          Projects and libraries you've bookmarked across GitHub.
        </p>

        {/* ── Search Bar ── */}
        <div className="gh-search-bar-wrap">
          <svg className="gh-search-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor">
            <path d="M10.68 11.74a6 6 0 0 1-7.922-8.982 6 6 0 0 1 8.982 7.922l3.04 3.04a.749.749 0 0 1-.326 1.275.749.749 0 0 1-.734-.215ZM11.5 7a4.499 4.499 0 1 0-8.997 0A4.499 4.499 0 0 0 11.5 7Z" />
          </svg>
          <input
            type="text"
            className="gh-search-input"
            placeholder="Search starred repositories..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
          />
          {searchQuery && (
            <button
              type="button"
              className="gh-search-clear"
              onClick={() => setSearchQuery("")}
              title="Clear search"
            >
              ✕
            </button>
          )}
        </div>
      </div>

      {/* ── Starred Stack ── */}
      <div className="gh-cards-stack">
        {filteredStarred.length === 0 ? (
          <div className="gh-empty-card">
            <p className="gh-empty-text">No starred repositories found.</p>
          </div>
        ) : (
          filteredStarred.map((item) => (
            <div key={item.id} className="gh-card gh-repo-card">
              <div className="gh-card-topline">
                <div className="gh-repo-owner-identity">
                  {item.owner_avatar_url ? (
                    <img
                      src={item.owner_avatar_url}
                      alt={item.owner_login}
                      className="gh-repo-owner-avatar"
                      loading="lazy"
                      onError={(e) => {
                        (e.currentTarget as HTMLElement).style.display = "none";
                      }}
                    />
                  ) : (
                    <div className="gh-repo-owner-avatar gh-repo-owner-avatar--fallback">
                      {(item.owner_login || item.name).charAt(0).toUpperCase()}
                    </div>
                  )}
                  <div className="gh-repo-identity-text">
                    <div className="gh-repo-name-cluster">
                      <span className="gh-repo-title">{item.full_name}</span>
                    </div>
                    <span className="gh-repo-owner-name">@{item.owner_login}</span>
                  </div>
                </div>
                <button
                  type="button"
                  className="gh-icon-ext-btn"
                  title="Open on GitHub"
                  onClick={() => handleOpenExternal(item.html_url)}
                >
                  <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M3.75 2h3.5a.75.75 0 0 1 0 1.5h-3.5a.25.25 0 0 0-.25.25v8.5c0 .138.112.25.25.25h8.5a.25.25 0 0 0 .25-.25v-3.5a.75.75 0 0 1 1.5 0v3.5A1.75 1.75 0 0 1 12.25 14h-8.5A1.75 1.75 0 0 1 2 12.25v-8.5C2 2.784 2.784 2 3.75 2Zm6.5 0a.75.75 0 0 1 .75-.75h3.5a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0V3.56l-4.72 4.72a.749.749 0 0 1-1.275-.326.749.749 0 0 1 .215-.734L12.44 2.5H10.25a.75.75 0 0 1-.75-.75Z" />
                  </svg>
                </button>
              </div>

              {item.description && <p className="gh-card-title gh-repo-desc">{item.description}</p>}

              <div className="gh-repo-meta-row">
                {item.language && (
                  <div className="gh-lang-pill">
                    <span
                      className="gh-lang-dot"
                      style={{ backgroundColor: item.language_color || "#8b949e" }}
                    />
                    <span className="gh-lang-name">{item.language}</span>
                  </div>
                )}

                <div className="gh-stat-pill" title="Stars">
                  <svg width="13" height="13" viewBox="0 0 16 16" fill="#e3b341">
                    <path d="M8 .25a.75.75 0 0 1 .673.418l1.882 3.815 4.21.612a.75.75 0 0 1 .416 1.279l-3.046 2.97.719 4.192a.751.751 0 0 1-1.088.791L8 12.347l-3.766 1.98a.75.75 0 0 1-1.088-.79l.72-4.194L.818 6.374a.75.75 0 0 1 .416-1.28l4.21-.611L7.327.668A.75.75 0 0 1 8 .25z" />
                  </svg>
                  <span>{item.stars_count}</span>
                </div>

                <div className="gh-stat-pill" title="Forks">
                  <svg width="13" height="13" viewBox="0 0 16 16" fill="currentColor">
                    <path d="M5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0Zm0 2.122a2.25 2.25 0 1 0-1.5 0v.878A2.25 2.25 0 0 0 5.75 8.5h4.5A2.25 2.25 0 0 0 12.5 6.25v-.878a2.25 2.25 0 1 0-1.5 0v.878a.75.75 0 0 1-.75.75h-4.5a.75.75 0 0 1-.75-.75v-.878ZM12.5 3.25a.75.75 0 1 1-1.5 0 .75.75 0 0 1 1.5 0ZM7.25 10.5a.75.75 0 0 1 .75-.75h.5a.75.75 0 0 1 .75.75v1.878a2.25 2.25 0 1 1-1.5 0V10.5a.75.75 0 0 1-.5-.5Zm1.25 3.75a.75.75 0 1 0 0-1.5.75.75 0 0 0 0 1.5Z" />
                  </svg>
                  <span>{item.forks_count}</span>
                </div>

                <span className="gh-card-timestamp">Updated {timeAgo(item.updated_at)}</span>
              </div>

              <div className="gh-card-actions">
                <button
                  type="button"
                  className="gh-btn gh-btn--analyse"
                  disabled={analyzingRepo !== null}
                  onClick={() => handleAnalyse(item)}
                >
                  {analyzingRepo === item.name ? "Analysing..." : "Analyse Repo"}
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
