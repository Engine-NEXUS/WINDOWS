// Dev-only preview of the CodeRabbit panel with SAMPLE data (never shipped: not a Tauri window).
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { CodeRabbitPanel } from "./CodeRabbitPanel";
import "./coderabbit.css";

const PRS = [
  { prNumber: 36, title: "Feature programme: focus mode, notifications", author: "sample-user", headBranch: "feat/x", baseBranch: "main", verdict: "NOT_REVIEWED", issuesSummary: "Not reviewed by CodeRabbit yet", updatedAt: "" },
  { prNumber: 37, title: "Fix pool leak", author: "sample-user", headBranch: "fix/pool", baseBranch: "main", verdict: "REQUEST_CHANGES", issuesSummary: "Reviewed by CodeRabbit", updatedAt: "" },
];
const REVIEW = {
  prNumber: 37, repo: "owner/repo", title: "Fix pool leak", headBranch: "fix/pool", baseBranch: "main",
  verdict: "REQUEST_CHANGES", summary: "## Walkthrough\nSample text: the pool is now bounded.", walkthrough: [],
  stats: { filesChanged: 3, additions: 10, deletions: 2 },
  findings: [{ id: "cr-1", file: "src/pool.rs", lineRange: "40-42", severity: "critical", title: "Connection leak on error.",
    description: "Sample finding. The lease is dropped without recycling.", suggestedCode: "pool.recycle(conn);" }],
};

let cb = 0;
(window as any).__TAURI_INTERNALS__ = {
  transformCallback: () => ++cb,
  invoke: (cmd: string) => Promise.resolve(cmd === "coderabbit_review_pr" ? REVIEW : undefined),
};

createRoot(document.getElementById("root")!).render(
  <StrictMode><CodeRabbitPanel data={{ mode: "pr_list", repo: "owner/repo", activePrs: PRS as never }} /></StrictMode>,
);
