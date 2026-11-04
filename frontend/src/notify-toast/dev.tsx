// Dev-only preview of every toast layout (served by the Vite dev server; not part of the release bundle).
import { createRoot } from "react-dom/client";
import "../theme/tokens.css";
import { NotifyToastView } from "./NotifyToastView";
import type { ToastPayload } from "./notifyToastModel";

const base = { seq: 1, kind: "", count: 1, durationMs: 5000 };
// heights mirror notify_toast::toast_height(rows) = 148 + 20 * rows (logical px)
const samples: Array<{ p: ToastPayload; h: number }> = [
  {
    p: { ...base, eventId: "ev_1", provider: "GitHub", status: "failed", headline: "Build failed in zync", action: "View log",
      rows: [{ label: "Repo", value: "alice/zync" }, { label: "Branch", value: "feat/notify-toast" }, { label: "Run", value: "#1793813131" }, { label: "Failed step", value: "step 5" }] },
    h: 148 + 80,
  },
  {
    p: { ...base, eventId: "ev_2", provider: "Mail", status: "info", headline: "Interview on Monday at 10:30 — please confirm", action: "Open mail",
      rows: [{ label: "From", value: "Asha Rao" }] },
    h: 148 + 20,
  },
  {
    p: { ...base, eventId: "ev_3", provider: "Vercel", status: "recovered", headline: "zync is live again", action: "View deploy",
      rows: [{ label: "Project", value: "zync" }, { label: "Deploy", value: "dpl_8fK2x" }] },
    h: 148 + 40,
  },
  {
    p: { ...base, eventId: "ev_4", provider: "Calendar", status: "info", headline: "Dentist appointment", action: "Open event",
      rows: [{ label: "Reminder", value: "Leave in about 12 minutes for Dentist appointment" }] },
    h: 148 + 20,
  },
  {
    p: { ...base, eventId: "ev_5", provider: "WhatsApp", status: "info", headline: "Mummy", action: "Open chat", rows: [{ label: "Chat", value: "new message" }] },
    h: 148 + 20,
  },
  {
    p: { ...base, eventId: "ev_6", provider: "Agent", status: "info", headline: "Claude Code needs your input", action: "Go to agent", count: 3,
      rows: [{ label: "Folder", value: "ULTRON" }] },
    h: 148 + 20,
  },
  {
    p: { ...base, eventId: "ev_7", provider: "Mail", status: "info", headline: "Mail alert", action: "Open mail", rows: [] },
    h: 148,
  },
];

function Slot({ p, fading }: { p: ToastPayload; h: number; fading?: boolean }) {
  return (
    <div className="slot">
      <NotifyToastView payload={p} fading={fading} onOpen={() => {}} onClose={() => {}} onPause={() => {}} />
    </div>
  );
}

createRoot(document.getElementById("root")!).render(
  <>
    {samples.map((s) => (
      <Slot key={s.p.eventId} {...s} />
    ))}
  </>,
);
