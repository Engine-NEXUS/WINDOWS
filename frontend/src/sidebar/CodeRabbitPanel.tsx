import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { isTyping, nextDock } from "./dockKeys";
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { ReviewControls } from "./ReviewControls";
import { KpiGrid, KpiTile, Pill, SegPill, SeverityPill, SvButton, SvCard, SvChip, SvIcon, severityOf, type PillTone, type SvIconName } from "./kit/system";

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

/** The verdict as a pill: a word and a shape (solid, outline, dashed), never colour alone. */
export function verdictLook(v: string): { tone: PillTone; icon: SvIconName; label: string } {
  switch (v) {
    case "APPROVE":
      return { tone: "solid", icon: "check", label: VERDICT_LABEL.APPROVE };
    case "REQUEST_CHANGES":
      return { tone: "solid", icon: "alert", label: VERDICT_LABEL.REQUEST_CHANGES };
    case "COMMENT":
      return { tone: "outline", icon: "info", label: VERDICT_LABEL.COMMENT };
    default:
      return { tone: "dash", icon: "info", label: VERDICT_LABEL[v] ?? "Not reviewed yet" };
  }
}

/** The severity filter's buttons and how many findings each holds; the first button is "All". */
export function severityFilters(findings: Pick<CodeRabbitFinding, "severity">[]): { label: string; word: string | null; count: number }[] {
  const count = (w: string) => findings.filter((f) => severityOf(f.severity).word === w).length;
  return [
    { label: `All ${findings.length}`, word: null, count: findings.length },
    { label: `High ${count("High")}`, word: "High", count: count("High") },
    { label: `Medium ${count("Medium")}`, word: "Medium", count: count("Medium") },
    { label: `Low ${count("Low")}`, word: "Low", count: count("Low") },
  ];
}

/** The findings that pass the filter, worst first (stable inside a severity). */
export function visibleFindings<T extends Pick<CodeRabbitFinding, "severity">>(findings: T[], word: string | null): T[] {
  return findings
    .map((f, i) => ({ f, i }))
    .filter(({ f }) => word === null || severityOf(f.severity).word === word)
    .sort((a, b) => severityOf(a.f.severity).rank - severityOf(b.f.severity).rank || a.i - b.i)
    .map(({ f }) => f);
}

const APP = "https://app.coderabbit.ai";

function go(url: string) {
  openExternal(url).catch(() => window.open(url, "_blank"));
}

function CopyCode({ code }: { code: string }) {
  const [done, setDone] = useState(false);
  return (
    <pre className="sv-code">
      <button
        type="button"
        className="sv-copy"
        onClick={() => {
          void navigator.clipboard?.writeText(code);
          setDone(true);
          setTimeout(() => setDone(false), 1100);
        }}
      >
        {done ? "Copied" : "Copy"}
      </button>
      {code}
    </pre>
  );
}

/** The CodeRabbit review of one pull request: the verdict first, the numbers, the review actions, then every finding. */
export const ReviewBody: React.FC<{ review: CodeRabbitReviewData }> = ({ review }) => {
  const [filter, setFilter] = useState(0);
  const filters = useMemo(() => severityFilters(review.findings), [review.findings]);
  const shown = useMemo(() => visibleFindings(review.findings, filters[filter]?.word ?? null), [review.findings, filters, filter]);
  const verdict = verdictLook(review.verdict);
  const high = filters[1].count;
  return (
    <>
      <SvCard>
        <div className="sv-row" style={{ marginBottom: 10 }}>
          <Pill tone={verdict.tone} icon={verdict.icon}>{verdict.label}</Pill>
          <SvChip icon="pr">{review.headBranch} → {review.baseBranch}</SvChip>
        </div>
        <h2 className="sv-h">{review.title || `PR #${review.prNumber}`}</h2>
        <p className="sv-p" style={{ marginTop: 8 }}>{review.repo} · #{review.prNumber}</p>
      </SvCard>

      <KpiGrid>
        <KpiTile label="Files" value={review.stats.filesChanged} />
        <KpiTile label="Added" value={`+${review.stats.additions}`} sub="lines" />
        <KpiTile label="Removed" value={`−${review.stats.deletions}`} sub="lines" />
        <KpiTile label="Findings" value={review.findings.length} sub={high ? `${high} high` : "none high"} />
      </KpiGrid>

      <SvCard title="Review">
        <ReviewControls repo={review.repo} prNumber={review.prNumber} />
        {review.verdict === "NOT_REVIEWED" && (
          <div style={{ marginTop: 10 }}>
            <SvButton onClick={() => go(APP)}>Add this repository in CodeRabbit</SvButton>
          </div>
        )}
      </SvCard>

      {review.summary && (
        <SvCard title="Summary">
          <p className="sv-p" style={{ whiteSpace: "pre-wrap", color: "var(--sv-text)" }}>{review.summary}</p>
        </SvCard>
      )}

      {review.findings.length > 0 && (
        <SvCard title="Findings" right={<Pill tone="solid">{shown.length}</Pill>}>
          <div style={{ marginBottom: 12 }}>
            <SegPill label="Filter findings by severity" items={filters.map((f) => f.label)} active={filter} onChange={setFilter} />
          </div>
          <ul className="sv-list">
            {shown.map((f) => (
              <li className="sv-li" key={f.id} style={{ flexDirection: "column", alignItems: "stretch", gap: 8 }}>
                <div className="sv-row">
                  <SeverityPill severity={f.severity} />
                  <SvChip icon="file">{f.file}:{f.lineRange}</SvChip>
                </div>
                <b className="sv-li-t">{f.title}</b>
                <p className="sv-p" style={{ whiteSpace: "pre-wrap" }}>{f.description}</p>
                {f.suggestedCode ? <CopyCode code={f.suggestedCode} /> : null}
              </li>
            ))}
          </ul>
        </SvCard>
      )}
    </>
  );
};

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
      <>
        <div>
          <button type="button" className="sv-back" onClick={() => setReview(null)}>
            <SvIcon name="arrow" />
            Pull requests
          </button>
        </div>
        <ReviewBody review={review} />
      </>
    );
  }

  return (
    <>
      <label className="sv-search">
        <SvIcon name="link" />
        <input ref={search} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search pull requests" aria-label="Search pull requests" />
        <kbd className="sv-kbd">Ctrl K</kbd>
      </label>

      <SvCard title={`Recents${repo ? ` · ${repo}` : ""}`} right={<Pill tone="solid">{prs.length}</Pill>}>
        {error && <p className="sv-p" role="alert" style={{ color: "var(--sv-text)", fontWeight: 600 }}>{error}</p>}
        {prs.length === 0 && !error && <p className="sv-p">No open pull requests found.</p>}
        <ul className="sv-list">
          {prs.map((p) => (
            <li key={p.prNumber} style={{ listStyle: "none" }}>
              <button type="button" className="sv-pr" onClick={() => openPr(p.prNumber)}>
                <span className="sv-pr-t">{p.title}</span>
                <span className="sv-row">
                  <SvChip icon="pr">#{p.prNumber}</SvChip>
                  <Pill>{p.author}</Pill>
                  <Pill tone="dash">{busy === p.prNumber ? "Loading…" : p.issuesSummary}</Pill>
                </span>
              </button>
            </li>
          ))}
        </ul>
      </SvCard>

      <SvCard title="Quick access">
        <div className="sv-row">
          <SvButton onClick={() => go(APP)}>New coding task</SvButton>
          <SvButton onClick={() => go(APP)}>Run deep scan</SvButton>
          <SvButton onClick={() => go(APP)}>Open CodeRabbit</SvButton>
          <SvButton onClick={() => go("https://docs.coderabbit.ai")}>Documentation</SvButton>
        </div>
      </SvCard>
    </>
  );
};
