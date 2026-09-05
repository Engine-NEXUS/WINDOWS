import { useEffect } from "react";
import { usePrList, PrSummary } from "./prListStore";

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

/** A single PR card with Merge and Analyse buttons. */
function PrCard({ pr, disabled, onMerge, onAnalyse }: {
  pr: PrSummary;
  disabled: boolean;
  onMerge: () => void;
  onAnalyse: () => void;
}) {
  return (
    <div className="pr-card">
      <div className="pr-card-header">
        <span className="pr-number">#{pr.number}</span>
        <span className="pr-repo">{pr.repo}</span>
        <span className="pr-time">{timeAgo(pr.created_at)}</span>
      </div>
      <div className="pr-title">{pr.title}</div>
      <div className="pr-author">by {pr.author}</div>
      <div className="pr-actions">
        <button className="pr-btn pr-btn-merge" disabled={disabled} onClick={onMerge}>
          Merge PR
        </button>
        <button className="pr-btn pr-btn-analyse" disabled={disabled} onClick={onAnalyse}>
          Analyse
        </button>
      </div>
    </div>
  );
}

export function PrListApp() {
  const { visible, repo, state, prs, loading, actionInProgress, hide, setActionInProgress, removePr } = usePrList();

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

        // Check if it's a PrList variant: { "PrList": { ... } }
        const prListData = result.PrList;
        if (!prListData) return;

        usePrList.getState().showPrList(prListData.repo, prListData.state, prListData.prs);

        // Show the window via Tauri command
        tauriInvoke("show_pr_list_sidebar").catch((e) =>
          console.error("[PR List] failed to show sidebar:", e)
        );
      });
    })();

    return () => { unlisten?.(); };
  }, []);

  // Listen for Ctrl+Space to close
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.code === "Space" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        hide();
        tauriInvoke("hide_pr_list_sidebar").catch(() => {});
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [hide]);

  if (!visible) return null;

  const handleMerge = async (pr: PrSummary) => {
    setActionInProgress({ prNumber: pr.number, action: "merge" });
    try {
      // Emit a voice-like command through the orchestrator
      await tauriInvoke("orchestrator_process", {
        transcript: `merge pr ${pr.number} in ${pr.repo}`,
        dialogContext: null,
      });
      // Remove the PR from the list on success
      removePr(pr.number);
    } catch (e) {
      console.error("[PR List] merge failed:", e);
    } finally {
      setActionInProgress(null);
    }
  };

  const handleAnalyse = async (pr: PrSummary) => {
    setActionInProgress({ prNumber: pr.number, action: "analyse" });
    try {
      // Trigger the same analysis flow as "analyse pr N in repo"
      await tauriInvoke("orchestrator_process", {
        transcript: `analyse pr ${pr.number} in ${pr.repo}`,
        dialogContext: null,
      });
    } catch (e) {
      console.error("[PR List] analyse failed:", e);
    } finally {
      setActionInProgress(null);
    }
  };

  return (
    <div className="pr-list-container">
      <div className="pr-list-header">
        <div className="pr-list-title">
          {prs.length} {state} PR{prs.length === 1 ? "" : "s"} in {repo}
        </div>
        <button className="pr-list-close" onClick={() => { hide(); tauriInvoke("hide_pr_list_sidebar").catch(() => {}); }}>
          ✕
        </button>
      </div>
      {loading && <div className="pr-list-loading">Loading PRs...</div>}
      {!loading && prs.length === 0 && (
        <div className="pr-list-empty">No pull requests found.</div>
      )}
      <div className="pr-list-scroll">
        {prs.map((pr) => (
          <PrCard
            key={pr.number}
            pr={pr}
            disabled={actionInProgress !== null}
            onMerge={() => handleMerge(pr)}
            onAnalyse={() => handleAnalyse(pr)}
          />
        ))}
      </div>
      <div className="pr-list-footer">
        Ctrl+Space to close
      </div>
    </div>
  );
}
