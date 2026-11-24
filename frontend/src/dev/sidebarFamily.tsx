import { createRoot } from "react-dom/client";
import { WhatsAppView } from "../sidebar/views/whatsapp/WhatsAppView";
import { GmailView } from "../sidebar/views/gmail/GmailView";
import { ScreenTimeView } from "../sidebar/views/screentime/ScreenTimeView";

// Dev playground only: sample data, fixed clock.
const NOW = Math.floor(Date.now() / 1000);
const m = (id: string, minsAgo: number, text: string, fromMe: boolean) => ({ id, ts: NOW - minsAgo * 60, text, fromMe });

const wa = {
  kind: "whatsapp" as const,
  name: "Asha Rao",
  vip: true,
  messages: [
    m("1", 60 * 26, "Are we still on for the demo tomorrow?", false),
    m("2", 60 * 26 - 2, "I can bring the slides", false),
    m("3", 60 * 25, "Yes, 11 works. Send me the deck tonight.", true),
    m("4", 50, "Done, check your mail", false),
    m("5", 49, "Also the venue changed to room 204", false),
    m("6", 12, "Got it, thanks!", true),
  ],
};
const mail = {
  kind: "mail" as const,
  items: [
    { sender: "Exam Cell <exams@college.edu>", subject: "Hall ticket released for semester exams", deadline: "Friday 5 PM", category: "exam", explain: "Your hall ticket is ready to download before Friday." },
    { sender: "GitHub <noreply@github.com>", subject: "Run failed: ci / build (zync)", category: "github", explain: "The build on main failed in the test step." },
    { sender: "Supabase <noreply@supabase.io>", subject: "Your project will be paused", deadline: "Monday", category: "database", explain: "The free project pauses after 7 days of inactivity." },
    { sender: "Hack Club", subject: "Hackathon registration is open", category: "hackathon" },
  ],
};
const days = ["2026-11-16", "2026-11-17", "2026-11-18", "2026-11-19", "2026-11-20", "2026-11-21", "2026-11-22"].map((day, i) => ({
  day,
  minutes: [210, 95, 300, 180, 240, 60, 135][i],
  apps: i === 6 ? [{ app: "Brave", minutes: 70 }, { app: "Code", minutes: 45 }, { app: "WhatsApp", minutes: 15 }, { app: "Explorer", minutes: 5 }] : [],
}));
const st = { kind: "screentime" as const, enabled: true, days, sitesEnabled: true, sites: [{ domain: "github.com", minutes: 32 }, { domain: "youtube.com", minutes: 18 }, { domain: "docs.rs", minutes: 9 }] };

function App() {
  return (
    <>
      <div className="win"><WhatsAppView data={wa} nowSecs={NOW} /></div>
      <div className="win"><GmailView data={mail} /></div>
      <div className="win"><ScreenTimeView data={st} /></div>
    </>
  );
}

document.documentElement.dataset.theme = "dark";
document.getElementById("theme")?.addEventListener("click", () => {
  document.documentElement.dataset.theme = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
});
createRoot(document.getElementById("root")!).render(<App />);
