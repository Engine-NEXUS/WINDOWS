import React, { useState } from "react";
import {
  SEVERITY_ORDER,
  agentPrompt,
  filterFindings,
  gateLine,
  groupBySeverity,
  kindLabel,
  lineRange,
  severityCounts,
  sourceLabel,
  usageLine,
  type Report,
  type Sev,
} from "./reportModel";

const SEV_LABEL: Record<Sev, string> = { critical: "Critical", major: "Major", minor: "Minor", trivial: "Trivial", info: "Info", unknown: "Unrated" };

/** CodeRabbit's analysis of a PR: where it came from, the free-review budget, the gate, then findings grouped by
 *  CodeRabbit's own severity with its type labels. Anything NEXUS had to infer says so; nothing is invented. */
export const CodeRabbitReport: React.FC<{ report: Report }> = ({ report }) => {
  const [filter, setFilter] = useState<Sev | "all">("all");
  const [copied, setCopied] = useState(false);
  const counts = severityCounts(report.findings);
  const groups = groupBySeverity(filterFindings(report.findings, filter));
  const usage = usageLine(report.usage);
  const gate = gateLine(report.gate);
  const prompt = agentPrompt(report);

  return (
    <>
      <div className="crp-row">
        <span className={`crp-pill crp-pill--${report.source === "none" ? "not_reviewed" : "comment"}`}>{sourceLabel(report.source)}</span>
        {gate && <span className="crp-meta">{gate}</span>}
        {report.reused && <span className="crp-meta">reused earlier review</span>}
      </div>
      {usage && <div className="crp-meta">{usage}</div>}
      {report.summary && <pre className="crp-summary">{report.summary}</pre>}
      {report.notes.map((n) => (
        <div key={n} className="crp-note">
          {n}
        </div>
      ))}
      {report.unreviewedFiles > 0 && <div className="crp-note">{report.unreviewedFiles} file(s) were not reviewed.</div>}

      {report.findings.length > 0 && (
        <div className="crp-chips" role="group" aria-label="Filter by severity">
          <button type="button" className={`crp-chip${filter === "all" ? " crp-chip--on" : ""}`} onClick={() => setFilter("all")}>
            All {report.findings.length}
          </button>
          {SEVERITY_ORDER.filter((s) => counts[s] > 0).map((s) => (
            <button key={s} type="button" className={`crp-chip crp-sev--${s}${filter === s ? " crp-chip--on" : ""}`} onClick={() => setFilter(s)}>
              {SEV_LABEL[s]} {counts[s]}
            </button>
          ))}
        </div>
      )}

      {groups.map((g) => (
        <section key={g.severity} className="crp-group">
          <div className="crp-section">
            {SEV_LABEL[g.severity]} · {g.items.length}
          </div>
          {g.items.map((f) => {
            const range = lineRange(f);
            const kind = kindLabel(f.kind, f.kindInferred);
            return (
              <article key={f.id} className="crp-card">
                <div className="crp-row">
                  <span className={`crp-sev crp-sev--${f.severity}`}>{SEV_LABEL[f.severity]}</span>
                  {kind && <span className="crp-meta">{kind}</span>}
                  {f.file && (
                    <span className="crp-file">
                      {f.file}
                      {range ? `:${range}` : ""}
                    </span>
                  )}
                </div>
                <h2 className="crp-card-title">{f.title}</h2>
                {f.comment && <pre className="crp-body">{f.comment}</pre>}
                {f.agentInstructions && <pre className="crp-body crp-agent">{f.agentInstructions}</pre>}
                {f.suggestions.map((s) => (
                  <pre key={s} className="crp-code" title="Click to copy" onClick={() => navigator.clipboard?.writeText(s)}>
                    {s}
                  </pre>
                ))}
              </article>
            );
          })}
        </section>
      ))}

      {prompt && (
        <button
          type="button"
          className="crp-link"
          onClick={() => {
            void navigator.clipboard?.writeText(prompt);
            setCopied(true);
            window.setTimeout(() => setCopied(false), 2000);
          }}
        >
          {copied ? "Copied" : "Copy prompt for an AI agent"}
        </button>
      )}
    </>
  );
};
