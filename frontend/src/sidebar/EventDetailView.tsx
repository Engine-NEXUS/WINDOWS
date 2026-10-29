import React, { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import { CodeRabbitReport } from "./CodeRabbitReport";
import { ReviewControls } from "./ReviewControls";
import type { Report } from "./reportModel";

/** Payload built in Rust from validated ledger state (see notify_hub.rs); nothing here is parsed from text. */
export interface EventDetailData {
  mode: "event_detail";
  token: number;
  unavailable?: string;
  eventId?: string;
  provider?: string;
  status?: "failed" | "recovered" | "info";
  title?: string;
  preview?: string;
  target?: { kind: string; label: string }[];
  originalSourceRef?: string;
  sourceRef?: string;
  occurrences?: number;
  providerUrl?: string | null;
  restoreView?: string | null;
  /** Full `owner/repo` and PR number when the alert belongs to a pull request. */
  repo?: string | null;
  prNumber?: number | null;
}

const STATUS: Record<string, string> = { failed: "Failed", recovered: "Recovered", info: "Info" };

type Review = { state: "none" } | { state: "loading" } | { state: "error"; message: string } | { state: "ready"; report: Report };

/** A GitHub alert shown in the CodeRabbit interface: the alert (what failed, which run) on top, then CodeRabbit's own
 *  review of the pull request below it. Builds with no pull request say so instead of showing anything else. */
export const EventDetailView: React.FC<{ data: EventDetailData }> = ({ data }) => {
  const [review, setReview] = useState<Review>(data.repo && data.prNumber ? { state: "loading" } : { state: "none" });

  // Closing the sidebar or switching away ends this opening transaction (a stale token is ignored in Rust).
  useEffect(() => {
    const token = data.token;
    // Tell the overlay animation this opening has rendered (only the live token is accepted in Rust).
    void invoke("notify_event_ready", { token }).catch(() => {});
    return () => {
      void invoke("notify_close_event", { token }).catch(() => {});
    };
  }, [data.token]);

  // CodeRabbit's review of the PR, fetched for THIS alert only (a newer alert discards a late answer).
  useEffect(() => {
    if (!data.repo || !data.prNumber) {
      setReview({ state: "none" });
      return;
    }
    let live = true;
    setReview({ state: "loading" });
    invoke<Report>("coderabbit_report", { prNumber: data.prNumber, repo: data.repo })
      .then((r) => live && setReview({ state: "ready", report: r }))
      .catch((e) => live && setReview({ state: "error", message: String(e) }));
    return () => {
      live = false;
    };
  }, [data.token, data.repo, data.prNumber]);

  const back = () => {
    const view = data.restoreView;
    void invoke("notify_close_event", { token: data.token })
      .then(() => (view ? invoke("show_sidebar_view", { view, dock: null }) : invoke("hide_sidebar")))
      .catch(() => {});
  };

  if (data.unavailable) {
    return (
      <div className="crp">
        <h1 className="crp-title">Alert unavailable</h1>
        <p className="crp-meta">{data.unavailable}</p>
        <button type="button" className="crp-link" onClick={back}>
          {data.restoreView ? "Back" : "Close"}
        </button>
      </div>
    );
  }
  return (
    <div className="crp">
      <div className="crp-row">
        <span className={`crp-pill crp-pill--${data.status === "recovered" ? "approve" : "request_changes"}`}>
          {STATUS[data.status ?? "info"]}
        </span>
        <span className="crp-meta">{data.provider}</span>
      </div>
      <h1 className="crp-title">{data.title}</h1>
      {data.preview && <p className="crp-meta">{data.preview}</p>}
      <dl className="crp-facts">
        {(data.target ?? []).map((t) => (
          <React.Fragment key={t.kind}>
            <dt>{t.kind}</dt>
            <dd>{t.label}</dd>
          </React.Fragment>
        ))}
        {data.originalSourceRef && (
          <>
            <dt>First failure</dt>
            <dd>{data.originalSourceRef}</dd>
          </>
        )}
        {data.sourceRef && data.sourceRef !== data.originalSourceRef && (
          <>
            <dt>Latest</dt>
            <dd>{data.sourceRef}</dd>
          </>
        )}
        {(data.occurrences ?? 1) > 1 && (
          <>
            <dt>Failing runs</dt>
            <dd>{data.occurrences}</dd>
          </>
        )}
      </dl>
      <div className="crp-row">
        {data.providerUrl && (
          <button type="button" className="crp-link" onClick={() => openExternal(data.providerUrl as string).catch(() => {})}>
            Open on GitHub
          </button>
        )}
        {data.restoreView && (
          <button type="button" className="crp-link" onClick={back}>
            Back
          </button>
        )}
      </div>

      <div className="crp-section">CodeRabbit review</div>
      {data.repo && data.prNumber ? (
        <ReviewControls repo={data.repo} prNumber={data.prNumber} onReport={(r) => setReview({ state: "ready", report: r })} />
      ) : null}
      {review.state === "none" && <div className="crp-empty">This run is not attached to a pull request, so CodeRabbit has no review for it.</div>}
      {review.state === "loading" && <div className="crp-empty">Checking what CodeRabbit already posted on this pull request…</div>}
      {review.state === "error" && <div className="crp-error">{review.message}</div>}
      {review.state === "ready" && <CodeRabbitReport report={review.report} />}
    </div>
  );
};
