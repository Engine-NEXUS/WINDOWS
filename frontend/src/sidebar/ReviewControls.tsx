import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { allowanceLine, canReview, confirmLine, elapsedText, type Report, type Usage } from "./reportModel";

interface Job {
  state: "running" | "done" | "failed";
  startedMs: number;
  report?: Report | null;
}

interface AiFinding {
  file: string;
  startLine?: number | null;
  endLine?: number | null;
  severity: string;
  category: string;
  title: string;
  explanation: string;
  suggestion?: string | null;
}

interface AiJob {
  state: "running" | "done" | "failed";
  startedMs: number;
  error?: string | null;
  report?: {
    provider: string;
    model: string;
    summary: string;
    findings: AiFinding[];
    filesReviewed: string[];
    filesSkipped: [string, string][];
    notes: string[];
  } | null;
}

const USAGE_POLL_MS = 30_000;
const JOB_POLL_MS = 3_000;

/** Spinner + sweeping bar + elapsed time while a review runs (reviews take 2 to 15 minutes). */
export const ReviewingIndicator: React.FC<{ label: string; startedMs: number; hint: string }> = ({ label, startedMs, hint }) => {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return (
    <div className="crp-reviewing" role="status" aria-live="polite">
      <span className="crp-spinner" aria-hidden="true" />
      <div className="crp-reviewing-text">
        <div>
          {label} · {elapsedText(now - startedMs)}
        </div>
        <div className="crp-meta">{hint}</div>
        <div className="crp-sweep" aria-hidden="true" />
      </div>
    </div>
  );
};

/** The two on-request actions for one pull request: CodeRabbit's free review (asks first, shows the count) and an
 *  AI analysis with the owner's own API key (labelled as not CodeRabbit). Nothing is spent without a second click. */
export const ReviewControls: React.FC<{ repo: string; prNumber: number; onReport?: (r: Report) => void }> = ({ repo, prNumber, onReport }) => {
  const [usage, setUsage] = useState<Usage | null>(null);
  const [asking, setAsking] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [ai, setAi] = useState<AiJob | null>(null);
  const [aiError, setAiError] = useState<string | null>(null);
  const reported = useRef(false);

  // The PR this view shows is "this PR" for spoken commands.
  useEffect(() => {
    void invoke("coderabbit_set_context", { repo, prNumber }).catch(() => {});
    reported.current = false;
    setJob(null);
    setAi(null);
    setAsking(false);
    setError(null);
    setAiError(null);
  }, [repo, prNumber]);

  const refreshUsage = useCallback(() => {
    invoke<Usage | null>("coderabbit_usage")
      .then((u) => setUsage(u ?? null))
      .catch(() => {});
  }, []);

  // Live allowance: re-read every 30 s and once when the CLI says it is back to full.
  useEffect(() => {
    refreshUsage();
    const t = setInterval(refreshUsage, USAGE_POLL_MS);
    return () => clearInterval(t);
  }, [refreshUsage]);
  useEffect(() => {
    if (!usage?.fullCapacityMs) return;
    const t = setTimeout(refreshUsage, usage.fullCapacityMs + 1000);
    return () => clearTimeout(t);
  }, [usage?.fullCapacityMs, refreshUsage]);

  // A spoken "CodeRabbit review of this" shows the same question here.
  useEffect(() => {
    let off: (() => void) | undefined;
    void listen<{ repo: string; prNumber: number }>("coderabbit:consent", (e) => {
      if (e.payload.repo.toLowerCase() === repo.toLowerCase() && e.payload.prNumber === prNumber) {
        refreshUsage();
        setAsking(true);
      }
    }).then((u) => (off = u));
    return () => off?.();
  }, [repo, prNumber, refreshUsage]);

  // Poll the running review (also picks one started by voice).
  useEffect(() => {
    let live = true;
    const tick = () =>
      invoke<Job | null>("coderabbit_job", { repo, prNumber })
        .then((j) => {
          if (!live || !j) return;
          setJob(j);
          if (j.state !== "running") {
            setAsking(false);
            refreshUsage();
            if (j.report && !reported.current) {
              reported.current = true;
              onReport?.(j.report);
            }
          }
        })
        .catch(() => {});
    void tick();
    const t = setInterval(tick, JOB_POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [repo, prNumber, onReport, refreshUsage]);

  // Poll the AI analysis.
  useEffect(() => {
    if (ai?.state !== "running") return;
    let live = true;
    const t = setInterval(() => {
      invoke<AiJob | null>("ai_review_job", { repo, prNumber })
        .then((j) => live && j && setAi(j))
        .catch(() => {});
    }, JOB_POLL_MS);
    return () => {
      live = false;
      clearInterval(t);
    };
  }, [ai?.state, repo, prNumber]);

  const startReview = () => {
    setError(null);
    reported.current = false;
    invoke<Job>("coderabbit_review_start", { repo, prNumber })
      .then((j) => {
        setJob(j);
        setAsking(false);
      })
      .catch((e) => {
        setError(String(e));
        setAsking(false);
      })
      .finally(refreshUsage);
  };

  const startAi = () => {
    setAiError(null);
    invoke<AiJob>("ai_review_start", { repo, prNumber })
      .then(setAi)
      .catch((e) => setAiError(String(e)));
  };

  const running = job?.state === "running";
  const aiRunning = ai?.state === "running";
  const allowance = allowanceLine(usage);

  return (
    <div className="crp-controls">
      <div className="crp-row">
        {!asking && (
          <button type="button" className="crp-btn" disabled={running || !canReview(usage)} onClick={() => setAsking(true)}>
            CodeRabbit review
          </button>
        )}
        {asking && usage && (
          <span className="crp-confirm" role="group" aria-label="Confirm review">
            <span>{confirmLine(usage)}</span>
            <button type="button" className="crp-btn crp-btn--go" onClick={startReview}>
              Review
            </button>
            <button type="button" className="crp-btn" onClick={() => setAsking(false)}>
              Cancel
            </button>
          </span>
        )}
        <button type="button" className="crp-btn" disabled={aiRunning} onClick={startAi}>
          AI analysis
        </button>
      </div>
      {allowance && <div className="crp-meta">{allowance}</div>}
      {!allowance && usage === null && <div className="crp-meta">CodeRabbit usage unavailable (sign in with the CodeRabbit CLI).</div>}
      {error && <div className="crp-error">{error}</div>}
      {running && job && <ReviewingIndicator label="CodeRabbit is reviewing" startedMs={job.startedMs} hint="Usually 2 to 6 minutes, up to 15. I'll tell you when it finishes." />}
      {job?.state === "failed" && job.report && !running && <div className="crp-error">{job.report.notes.join(" ") || "The review did not finish."}</div>}

      {aiError && <div className="crp-error">{aiError}</div>}
      {aiRunning && ai && <ReviewingIndicator label="AI analysis" startedMs={ai.startedMs} hint="Reading the diff with your own AI key. Not CodeRabbit." />}
      {ai?.state === "failed" && <div className="crp-error">{ai.error ?? "The AI analysis did not finish."}</div>}
      {ai?.state === "done" && ai.report && (
        <section className="crp-ai">
          <div className="crp-section">AI analysis · not CodeRabbit</div>
          <div className="crp-meta">
            {ai.report.provider} · {ai.report.model} · {ai.report.filesReviewed.length} files read
            {ai.report.filesSkipped.length > 0 ? ` · ${ai.report.filesSkipped.length} skipped` : ""}
          </div>
          {ai.report.notes.map((n) => (
            <div key={n} className="crp-meta">
              {n}
            </div>
          ))}
          {ai.report.summary && <p className="crp-summary">{ai.report.summary}</p>}
          {ai.report.findings.length === 0 && <div className="crp-empty">Nothing to flag.</div>}
          {ai.report.findings.map((f, i) => (
            <article key={`${f.file}:${f.startLine}:${i}`} className="crp-card">
              <div className="crp-row">
                <span className={`crp-sev crp-sev--${f.severity}`}>{f.severity}</span>
                <span className="crp-file">
                  {f.file}
                  {f.startLine != null ? `:${f.startLine}${f.endLine != null && f.endLine !== f.startLine ? `-${f.endLine}` : ""}` : ""}
                </span>
              </div>
              <h2 className="crp-card-title">{f.title}</h2>
              <pre className="crp-body">{f.explanation}</pre>
              {f.suggestion && <pre className="crp-code">{f.suggestion}</pre>}
            </article>
          ))}
        </section>
      )}
    </div>
  );
};
