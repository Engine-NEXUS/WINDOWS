// Dev-only preview of every toast layout (served by the Vite dev server; not part of the release bundle).
import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import { NotifyToastView } from "./NotifyToastView";
import type { ToastPayload } from "./notifyToastModel";

const base = { seq: 1, kind: "", count: 1, durationMs: 5000 };

const samples: ToastPayload[] = [
  {
    ...base,
    eventId: "ev_github",
    provider: "GitHub",
    icon: "github",
    status: "failed",
    headline: "Build failed in zync on main",
    action: "View run",
    rows: [
      { label: "Repo", value: "alice/zync" },
      { label: "Branch", value: "main" },
      { label: "Run", value: "#1793813131" },
      { label: "Step", value: "cargo test" },
    ],
  },
  {
    ...base,
    eventId: "ev_gmail",
    provider: "Gmail",
    icon: "gmail",
    status: "info",
    headline: "Interview on Monday at 10:30 — please confirm attendance",
    action: "Open mail",
    rows: [
      { label: "From", value: "Asha Rao <asha@example.com>" },
      { label: "Subject", value: "NEXUS Engineering Interview" },
    ],
  },
  {
    ...base,
    eventId: "ev_vercel",
    provider: "Vercel",
    icon: "vercel",
    status: "recovered",
    headline: "Production deployment ready for zync-web",
    action: "Inspect",
    rows: [
      { label: "Project", value: "zync-web" },
      { label: "Deploy", value: "dpl_8fK2x19a" },
    ],
  },
  {
    ...base,
    eventId: "ev_render",
    provider: "Render",
    icon: "render",
    status: "failed",
    headline: "Web service crashed with out of memory",
    action: "View logs",
    rows: [
      { label: "Service", value: "api-backend" },
      { label: "Region", value: "oregon" },
    ],
  },
  {
    ...base,
    eventId: "ev_netlify",
    provider: "Netlify",
    icon: "netlify",
    status: "recovered",
    headline: "Site deploy succeeded: preview live",
    action: "Open site",
    rows: [
      { label: "Site", value: "nexus-docs" },
      { label: "Branch", value: "feat/logos" },
    ],
  },
  {
    ...base,
    eventId: "ev_supabase",
    provider: "Supabase",
    icon: "supabase",
    status: "info",
    headline: "Database migration completed in 2.4s",
    action: "Open dashboard",
    rows: [
      { label: "Project", value: "nexus-prod" },
      { label: "Migration", value: "20261105_toast_icons" },
    ],
  },
  {
    ...base,
    eventId: "ev_whatsapp",
    provider: "WhatsApp",
    icon: "whatsapp",
    status: "info",
    headline: "Mummy: Are you coming home for dinner?",
    action: "Reply",
    rows: [{ label: "Chat", value: "Family Group" }],
  },
  {
    ...base,
    eventId: "ev_calendar",
    provider: "Google Calendar",
    icon: "googlecalendar",
    status: "info",
    headline: "Dentist appointment at City Clinic",
    action: "Open event",
    rows: [{ label: "Reminder", value: "Leave in about 12 minutes" }],
  },
  {
    ...base,
    eventId: "ev_claude",
    provider: "Claude Code",
    icon: "claude",
    status: "info",
    headline: "Claude Code is waiting for your authorization",
    action: "Review tool",
    count: 3,
    rows: [
      { label: "Tool", value: "Bash" },
      { label: "Command", value: "pnpm test" },
    ],
  },
  {
    ...base,
    eventId: "ev_codex",
    provider: "Codex",
    icon: "codex",
    status: "failed",
    headline: "Execution halted: maximum tokens exceeded",
    action: "Restart",
    rows: [{ label: "Model", value: "codex-mini" }],
  },
  {
    ...base,
    eventId: "ev_antigravity",
    provider: "Antigravity",
    icon: "antigravity",
    status: "recovered",
    headline: "Workspace verification completed successfully",
    action: "View results",
    rows: [
      { label: "Suite", value: "notify-toast" },
      { label: "Checks", value: "15/15 passed" },
    ],
  },
  {
    ...base,
    eventId: "ev_opencode",
    provider: "OpenCode",
    icon: "opencode",
    status: "info",
    headline: "Code review finished with 0 warnings",
    action: "Open diff",
    rows: [{ label: "PR", value: "#142" }],
  },
  {
    ...base,
    eventId: "ev_cursor",
    provider: "Cursor",
    icon: "cursor",
    status: "failed",
    headline: "Composer background indexing timed out",
    action: "Retry",
    rows: [{ label: "Workspace", value: "ULTRON" }],
  },
  {
    ...base,
    eventId: "ev_gemini",
    provider: "Gemini",
    icon: "gemini",
    status: "recovered",
    headline: "Multimodal video analysis complete",
    action: "See response",
    rows: [{ label: "Tokens", value: "142,500" }],
  },
  {
    ...base,
    eventId: "ev_aider",
    provider: "Aider",
    icon: "aider",
    status: "info",
    headline: "Changes applied to 3 files and committed",
    action: "Show commit",
    rows: [
      { label: "Commit", value: "7a8bc92" },
      { label: "Model", value: "claude-3-5-sonnet" },
    ],
  },
  {
    ...base,
    eventId: "ev_agent",
    provider: "Unknown Agent",
    icon: "agent",
    status: "info",
    headline: "Autonomous agent completed scheduled task",
    action: "View summary",
    rows: [{ label: "Task", value: "nightly-backup" }],
  },
];

function Slot({ p }: { p: ToastPayload }) {
  return (
    <div className="slot">
      <NotifyToastView payload={p} onOpen={() => {}} onClose={() => {}} onPause={() => {}} />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <>
    {samples.map((p) => (
      <Slot key={p.eventId} p={p} />
    ))}
  </>,
);
