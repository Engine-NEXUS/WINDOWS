import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTyping, nextDock } from "./dockKeys";
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { ReviewControls } from "./ReviewControls";

export interface CodeRabbitFinding {
  id: string;
  file: string;
  lineRange: string;
  severity: "critical" | "major" | "minor" | "trivial" | "info" | "unknown";
  kind?: "potentialIssue" | "refactor" | "nitpick" | "unknown";
  title: string;
  description: string;
  originalCode?: string;
  suggestedCode?: string;
}

export interface CodeRabbitReviewData {
  prNumber: number;
  repo: string;
  title: string;
  headBranch: string;
  baseBranch: string;
  headSha?: string;
  verdict: "APPROVE" | "REQUEST_CHANGES" | "COMMENT" | "NOT_REVIEWED";
  summary: string;
  walkthrough: { title: string; details: string }[];
  findings: CodeRabbitFinding[];
  stats: { filesChanged: number; additions: number; deletions: number };
}

export interface CodeRabbitPrSummary {
  prNumber: number;
  title: string;
  author: string;
  headBranch: string;
  baseBranch: string;
  verdict: CodeRabbitReviewData["verdict"];
  issuesSummary: string;
  updatedAt: string;
}

type PanelData =
  | CodeRabbitReviewData
  | { mode?: string; repo?: string | null; activePrs?: CodeRabbitPrSummary[] };

export { nextDock };

export function isReview(d: PanelData): d is CodeRabbitReviewData {
  return Array.isArray((d as CodeRabbitReviewData).findings);
}

const VERDICT_LABEL: Record<string, string> = {
  APPROVE: "Approved",
  REQUEST_CHANGES: "Changes requested",
  COMMENT: "Reviewed",
  NOT_REVIEWED: "Not reviewed yet",
};

const LOCKED = ["Triage", "Analytics"];
const APP = "https://app.coderabbit.ai";

function go(url: string) {
  openExternal(url).catch(() => window.open(url, "_blank"));
}

/** The CodeRabbit review of one pull request: title, branches, verdict, summary and every finding. */
export const ReviewBody: React.FC<{ review: CodeRabbitReviewData }> = ({ review }) => (
  <>
    <h1 className="crp-title">{review.title || `PR #${review.prNumber}`}</h1>
    <div className="crp-meta">
      {review.repo} · #{review.prNumber} · {review.headBranch} → {review.baseBranch}
    </div>
    <div className="crp-row">
      <span className={`crp-pill crp-pill--${review.verdict.toLowerCase()}`}>{VERDICT_LABEL[review.verdict] ?? review.verdict}</span>
      <span className="crp-meta">
        {review.stats.filesChanged} files · +{review.stats.additions} −{review.stats.deletions}
      </span>
    </div>
    <ReviewControls repo={review.repo} prNumber={review.prNumber} />
    {review.summary && <pre className="crp-summary">{review.summary}</pre>}
    {review.verdict === "NOT_REVIEWED" && (
      <button type="button" className="crp-link" onClick={() => go(APP)}>
        Add this repository in CodeRabbit
      </button>
    )}
    {review.findings.map((f) => (
      <article key={f.id} className="crp-card">
        <div className="crp-row">
          <span className={`crp-sev crp-sev--${f.severity}`}>{f.severity}</span>
          <span className="crp-file">
            {f.file}:{f.lineRange}
          </span>
        </div>
        <h2 className="crp-card-title">{f.title}</h2>
        <pre className="crp-body">{f.description}</pre>
        {f.suggestedCode && (
          <pre className="crp-code" onClick={() => navigator.clipboard?.writeText(f.suggestedCode ?? "")}>
            {f.suggestedCode}
          </pre>
        )}
      </article>
    ))}
  </>
);

export const CodeRabbitPanel: React.FC<{ data: PanelData }> = ({ data }) => {
  const [review, setReview] = useState<CodeRabbitReviewData | null>(isReview(data) ? data : null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const search = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isReview(data)) setReview(data);
  }, [data]);

  const prs = useMemo(() => {
    const all = (!isReview(data) && data.activePrs) || [];
    const q = query.trim().toLowerCase();
    return q ? all.filter((p) => `${p.title} #${p.prNumber} ${p.author}`.toLowerCase().includes(q)) : all;
  }, [data, query]);
  const repo = isReview(data) ? data.repo : (data.repo ?? "");

  // Ctrl+K focuses the search box. Arrow-key docking is handled once for the whole sidebar (SidebarApp).
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k" && !isTyping(e.target)) {
        e.preventDefault();
        search.current?.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const openPr = useCallback(
    async (n: number) => {
      if (!repo) return;
      setBusy(n);
      setError(null);
      try {
        setReview(await invoke<CodeRabbitReviewData>("coderabbit_review_pr", { prNumber: n, repo }));
      } catch (e) {
        setError(String(e));
      } finally {
        setBusy(null);
      }
    },
    [repo],
  );

  if (review) {
    return (
      <div className="crp">
        <button type="button" className="crp-back" onClick={() => setReview(null)}>
          ← Pull requests
        </button>
        <ReviewBody review={review} />
      </div>
    );
  }

  return (
    <div className="crp">
      <div className="crp-search">
        <input
          ref={search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search"
          aria-label="Search pull requests"
        />
        <kbd>Ctrl K</kbd>
      </div>
      <nav className="crp-nav">
        {LOCKED.map((n) => (
          <div key={n} className="crp-nav-row crp-nav-row--locked" aria-disabled="true">
            {n}
          </div>
        ))}
        <button type="button" className="crp-nav-row" onClick={() => go(`${APP}`)}>
          New coding task
        </button>
        <button type="button" className="crp-nav-row" onClick={() => go(`${APP}`)}>
          Run deep scan
        </button>
      </nav>
      <div className="crp-section">Recents{repo ? ` · ${repo}` : ""}</div>
      {error && <div className="crp-error">{error}</div>}
      {prs.length === 0 && !error && <div className="crp-empty">No open pull requests found.</div>}
      {prs.map((p) => (
        <button key={p.prNumber} type="button" className="crp-pr" onClick={() => openPr(p.prNumber)}>
          <span className="crp-pr-title">{p.title}</span>
          <span className="crp-meta">
            #{p.prNumber} · {p.author} · {busy === p.prNumber ? "Loading…" : p.issuesSummary}
          </span>
        </button>
      ))}
      <div className="crp-section">Quick access</div>
      <button type="button" className="crp-nav-row" onClick={() => go(APP)}>
        Open CodeRabbit
      </button>
      <button type="button" className="crp-nav-row" onClick={() => go("https://docs.coderabbit.ai")}>
        Documentation
      </button>
    </div>
  );
};
