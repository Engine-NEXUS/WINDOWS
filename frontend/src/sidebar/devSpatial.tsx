import { StrictMode, useState } from "react";
import { createRoot } from "react-dom/client";
import { SpatialView } from "./SpatialDashboard";
import type { SpatialData } from "./sidebarStore";
import "./sidebar.css";
import "./unified.css";
import "./kit/kit.css";

const SAMPLE_SPATIAL_PAYLOAD: SpatialData = {
  title: "Screen Analysis",
  overview: "Workspace layout with communication tools, code editor, and system terminal active on screen.",
  provider_used: "gemini-2.0-flash",
  screen_image_data_url:
    "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=",
  items: [
    {
      id: 1,
      label: "Slack Workspace",
      category: "messaging",
      summary: "Team channel discussion with 3 unread messages in #general",
      details: [
        { title: "Team", value: "NEXUS Core" },
        { title: "Status", value: "Online" },
      ],
      logo_data_url:
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><rect width='24' height='24' rx='6' fill='%234A154B'/><text x='12' y='16' font-size='12' font-family='sans-serif' font-weight='bold' text-anchor='middle' fill='%23ffffff'>#</text></svg>",
      image_data_url:
        "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=",
    },
    {
      id: 2,
      label: "Gmail Inbox",
      category: "email",
      summary: "Inbox view showing 2 prioritized messages from reviewers",
      details: [
        { title: "Account", value: "dev@nexus.local" },
        { title: "Unread", value: "2" },
      ],
      logo_data_url:
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24'><rect width='24' height='24' rx='6' fill='%23EA4335'/><text x='12' y='16' font-size='11' font-family='sans-serif' font-weight='bold' text-anchor='middle' fill='%23ffffff'>M</text></svg>",
      image_data_url:
        "data:image/jpeg;base64,/9j/4AAQSkZJRgABAQEASABIAAD/2wBDAP//////////////////////////////////////////////////////////////////////////////////////wgALCAABAAEBAREA/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQABPxA=",
    },
    {
      id: 3,
      label: "Terminal - Dev Server",
      category: "terminal",
      summary: "Vite development server listening on port 5173",
      details: [
        { title: "Process", value: "node (vite)" },
        { title: "Port", value: "5173" },
      ],
      // No logo_data_url -> will test monogram fallback
    },
  ],
};

function DevSpatialApp() {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [dockDir, setDockDir] = useState<string>("right");

  const toggleTheme = () => {
    const next = theme === "dark" ? "light" : "dark";
    setTheme(next);
    document.documentElement.setAttribute("data-theme", next);
  };

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
        background: theme === "dark" ? "#0a0a0c" : "#f8f9fa",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "6px 12px",
          background: theme === "dark" ? "#15151b" : "#ffffff",
          borderBottom: "1px solid rgba(128,128,128,0.2)",
          fontSize: "12px",
          color: theme === "dark" ? "#ffffff" : "#111827",
        }}
      >
        <span>
          <strong>Dev Spatial Preview</strong> · Dock: {dockDir}
        </span>
        <button
          type="button"
          onClick={toggleTheme}
          style={{
            padding: "4px 10px",
            borderRadius: "6px",
            border: "1px solid rgba(128,128,128,0.3)",
            background: theme === "dark" ? "#202029" : "#e8e8ed",
            color: "inherit",
            cursor: "pointer",
            fontSize: "11px",
          }}
        >
          Theme: {theme}
        </button>
      </div>

      <div style={{ flex: 1, minHeight: 0, display: "flex", justifyContent: "center" }}>
        <div style={{ width: "380px", height: "100%", display: "flex" }}>
          <SpatialView data={SAMPLE_SPATIAL_PAYLOAD} onDock={(d) => setDockDir(d)} />
        </div>
      </div>
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(
    <StrictMode>
      <DevSpatialApp />
    </StrictMode>,
  );
}
