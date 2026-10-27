import { useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import { PrListApp } from "./PrListApp";
import { usePrList, SubPage } from "./prListStore";
import { applyThemeMode } from "../sidebar/theme";
import "./pr-list.css";
import "../sidebar/sidebar.css";
import "../sidebar/unified.css";

function DevStandalone() {
  const { subpage, setSubpage, theme, setTheme, isAuthenticated, setAuthenticated } = usePrList();
  const [fixed400, setFixed400] = useState(true);

  // Synchronize document theme class
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
    document.body.style.backgroundColor = theme === "dark" ? "#07090e" : "#e9ecef";
  }, [theme]);

  // Handle simulating a theme change from the settings window in browser mode
  const handleSimulateSettingsTheme = () => {
    const next = theme === "light" ? "dark" : "light";
    applyThemeMode(next, true);
    setTheme(next);
  };

  return (
    <div
      style={{
        minHeight: "100vh",
        display: "flex",
        flexDirection: "column",
        background: theme === "dark" ? "#07090e" : "#e9ecef",
        fontFamily: "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
        color: theme === "dark" ? "#e6edf3" : "#1f2328",
        transition: "background-color 0.2s ease, color 0.2s ease",
      }}
    >
      {/* ── Top Developer Control Bar ── */}
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "8px 20px",
          background: theme === "dark" ? "#161b22" : "#ffffff",
          borderBottom: `1px solid ${theme === "dark" ? "#30363d" : "#d0d7de"}`,
          zIndex: 100,
          boxShadow: "0 2px 8px rgba(0,0,0,0.06)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              fontWeight: 700,
              fontSize: "13.5px",
              letterSpacing: "-0.2px",
            }}
          >
            <span style={{ fontSize: "16px" }}>⚡</span>
            <span>NEXUS GitHub Dashboard — Live HMR Canvas</span>
          </div>

          <span
            style={{
              padding: "2px 8px",
              borderRadius: "12px",
              fontSize: "11px",
              fontWeight: 600,
              background: fixed400
                ? theme === "dark"
                  ? "#238636"
                  : "#dafbe1"
                : theme === "dark"
                ? "#388bfd26"
                : "#ddf4ff",
              color: fixed400
                ? theme === "dark"
                  ? "#3fb950"
                  : "#1a7f37"
                : theme === "dark"
                ? "#58a6ff"
                : "#0969da",
              border: `1px solid ${
                fixed400
                  ? theme === "dark"
                    ? "#2ea043"
                    : "#aceebb"
                  : theme === "dark"
                  ? "#388bfd"
                  : "#54aeff"
              }`,
            }}
          >
            {fixed400 ? "400px Exact Settings Size" : "Full Fluid Width"}
          </span>
        </div>

        {/* View Switcher Tabs & Dev Controls */}
        <div style={{ display: "flex", alignItems: "center", gap: "6px" }}>
          {(
            [
              { id: "feed", label: "📋 Feed" },
              { id: "repos", label: "📦 Repos" },
              { id: "pull_requests", label: "🔀 Pull Requests" },
              { id: "issues", label: "⚠️ Issues" },
              { id: "organizations", label: "🏢 Organizations" },
              { id: "starred", label: "⭐ Starred" },
            ] as const
          ).map((tab) => {
            const active = subpage === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setSubpage(tab.id as SubPage)}
                style={{
                  padding: "5px 12px",
                  borderRadius: "6px",
                  fontSize: "12px",
                  fontWeight: active ? 600 : 500,
                  cursor: "pointer",
                  border: active
                    ? `1px solid ${theme === "dark" ? "#388bfd" : "#0969da"}`
                    : `1px solid ${theme === "dark" ? "#30363d" : "#d0d7de"}`,
                  background: active
                    ? theme === "dark"
                      ? "#1f6feb"
                      : "#0969da"
                    : theme === "dark"
                    ? "#21262d"
                    : "#f6f8fa",
                  color: active ? "#ffffff" : theme === "dark" ? "#c9d1d9" : "#24292f",
                  transition: "all 0.15s ease",
                }}
              >
                {tab.label}
              </button>
            );
          })}

          <div
            style={{
              width: "1px",
              height: "18px",
              background: theme === "dark" ? "#30363d" : "#d0d7de",
              margin: "0 6px",
            }}
          />

          {/* Width Constraint Toggle */}
          <button
            onClick={() => setFixed400(!fixed400)}
            title="Toggle between exact 400px Settings window size and full width"
            style={{
              padding: "5px 10px",
              borderRadius: "6px",
              fontSize: "12px",
              fontWeight: 500,
              cursor: "pointer",
              border: `1px solid ${theme === "dark" ? "#30363d" : "#d0d7de"}`,
              background: theme === "dark" ? "#21262d" : "#f6f8fa",
              color: theme === "dark" ? "#c9d1d9" : "#24292f",
            }}
          >
            {fixed400 ? "↔️ Expand" : "🎯 Lock 400px"}
          </button>

          {/* Simulate Settings Window Theme Toggle */}
          <button
            onClick={handleSimulateSettingsTheme}
            title="Simulate setting the theme inside the Settings window"
            style={{
              padding: "5px 12px",
              borderRadius: "6px",
              fontSize: "12px",
              fontWeight: 600,
              cursor: "pointer",
              border: `1px solid ${theme === "dark" ? "#30363d" : "#d0d7de"}`,
              background: theme === "dark" ? "#21262d" : "#f6f8fa",
              color: theme === "dark" ? "#f0f6fc" : "#1f2328",
            }}
          >
            Settings Theme: {theme === "light" ? "☀️ Light" : "🌙 Dark"} ⇄
          </button>

          {/* Auth State Switcher */}
          <button
            onClick={() => setAuthenticated(!isAuthenticated)}
            title="Toggle between Logged In (Dashboard) and Logged Out (Sign-in Gate)"
            style={{
              padding: "5px 12px",
              borderRadius: "6px",
              fontSize: "12px",
              fontWeight: 600,
              cursor: "pointer",
              border: isAuthenticated
                ? `1px solid ${theme === "dark" ? "#2ea043" : "#1a7f37"}`
                : `1px solid ${theme === "dark" ? "#d29922" : "#9a6700"}`,
              background: isAuthenticated
                ? theme === "dark"
                  ? "#23863622"
                  : "#dafbe1"
                : theme === "dark"
                ? "#d2992222"
                : "#fff8c5",
              color: isAuthenticated
                ? theme === "dark"
                  ? "#3fb950"
                  : "#1a7f37"
                : theme === "dark"
                ? "#e3b341"
                : "#7d4e00",
              display: "flex",
              alignItems: "center",
              gap: "6px",
              transition: "all 0.15s ease",
            }}
          >
            <span>{isAuthenticated ? "🟢" : "🔒"}</span>
            <span>{isAuthenticated ? "Logged In" : "Logged Out (Sign-in)"}</span>
          </button>
        </div>
      </header>

      {/* ── Main Viewport Container (Exact 400px Settings Window Match) ── */}
      <main
        style={{
          flex: 1,
          display: "flex",
          justifyContent: "center",
          alignItems: "stretch",
          padding: fixed400 ? "16px 0 24px" : "0",
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: fixed400 ? "400px" : "100%",
            maxWidth: fixed400 ? "400px" : "100%",
            height: fixed400 ? "calc(100vh - 80px)" : "100%",
            display: "flex",
            flexDirection: "column",
            borderRadius: fixed400 ? "10px" : "0",
            overflow: "hidden",
            boxShadow: fixed400
              ? theme === "dark"
                ? "0 12px 48px rgba(0,0,0,0.8), 0 0 0 1px #30363d"
                : "0 12px 40px rgba(0,0,0,0.12), 0 0 0 1px #d0d7de"
              : "none",
            transition: "width 0.2s ease, max-width 0.2s ease",
          }}
        >
          <PrListApp />
        </div>
      </main>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(<DevStandalone />);
