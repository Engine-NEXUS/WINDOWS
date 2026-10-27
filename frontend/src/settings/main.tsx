import React, { useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import { initBrowserDevMocks } from "./devMocks";
import { SettingsSidebarApp } from "../settings-sidebar/SettingsSidebarApp";
import { SettingsApp } from "./SettingsApp";
import { AnimatedThemeToggler } from "../theme/animated-theme-toggler";
import { initWindowBorderColor } from "../settings-sidebar/hub/AppearancePage";

// Style imports for Command Hub & Unified Sidebar
import "../sidebar/unified.css";
import "../settings-sidebar/settings-sidebar.css";
import "../settings-sidebar/hub/hub.css";
import "../settings-sidebar/mcpServers.css";
import "./settings.css";

// Initialize mock environment for standalone browser dev testing
initBrowserDevMocks();

const isTauri = typeof (window as any).__TAURI_INTERNALS__ !== "undefined" && !(window as any).__TAURI_DEV_STANDALONE__;

type SizePreset = "slim" | "dock" | "modal" | "full";

const PRESET_DIMS: Record<SizePreset, { width: string; height: string; label: string }> = {
  modal: { width: "740px", height: "960px", label: "740 × 960 px (Command Center Modal)" },
  dock: { width: "520px", height: "980px", label: "520 × 980 px (Unified Sidebar Dock)" },
  slim: { width: "400px", height: "1040px", label: "400 × 1040 px (Slim Card Dock)" },
  full: { width: "100%", height: "100%", label: "100% Fluid (Window Match)" },
};

function SettingsRoot() {
  const isStandalone = !isTauri;
  const [sizePreset, setSizePreset] = useState<SizePreset>(() => {
    if (typeof window === "undefined") return "full";
    const p = new URLSearchParams(window.location.search).get("size") as SizePreset;
    if (p && PRESET_DIMS[p]) return p;
    return "full";
  });

  const [viewMode, setViewMode] = useState<"hub" | "legacy">(() => {
    if (typeof window === "undefined") return "hub";
    const v = new URLSearchParams(window.location.search).get("view");
    return v === "legacy" ? "legacy" : "hub";
  });

  const [isPopup, setIsPopup] = useState(() => {
    if (typeof window === "undefined") return false;
    const urlParams = new URLSearchParams(window.location.search);
    return urlParams.get("popup") === "1" || window.opener != null;
  });

  useEffect(() => {
    initWindowBorderColor();
    if (!isStandalone) {
      document.body.classList.add("is-tauri");
      return;
    }

    document.body.classList.add("is-browser-dev");

    const checkPopup = () => {
      const urlParams = new URLSearchParams(window.location.search);
      const popup = urlParams.get("popup") === "1" || window.opener != null;
      setIsPopup(popup);
      if (popup) {
        document.body.classList.add("is-popup");
      } else {
        document.body.classList.remove("is-popup");
      }
    };

    checkPopup();
    window.addEventListener("resize", checkPopup);
    return () => window.removeEventListener("resize", checkPopup);
  }, [isStandalone]);

  const handleOpenPopup = () => {
    const dims = PRESET_DIMS[sizePreset];
    const w = parseInt(dims.width, 10) || 740;
    const h = parseInt(dims.height, 10) || 960;
    window.open(
      `/settings.html?popup=1&size=${sizePreset}&view=${viewMode}`,
      "NEXUS_Command_Hub",
      `width=${w},height=${h},menubar=no,toolbar=no,location=no,status=no,resizable=yes`
    );
  };

  // If inside native Tauri or popout window, render directly
  if (!isStandalone || isPopup) {
    return (
      <div className="unified-shell" style={{ width: "100%", height: "100vh" }}>
        <div className="unified-view-host">
          <div className="unified-view" style={{ display: "flex", width: "100%", height: "100%" }}>
            {viewMode === "hub" ? <SettingsSidebarApp onDock={() => {}} /> : <SettingsApp />}
          </div>
        </div>
      </div>
    );
  }

  const currentDims = PRESET_DIMS[sizePreset];

  return (
    <div className={`nx-dev-viewport nx-dev-preset-${sizePreset}`}>
      {/* ── Top Dev Toolbar with Size Presets ── */}
      <header className="nx-dev-toolbar-wide">
        <div className="nx-dev-brand">
          <span className="nx-dev-dot" />
          <span className="nx-dev-title">
            <strong>{viewMode === "hub" ? "NEXUS Command Hub" : "Legacy Settings"}</strong>
            <span className="nx-dev-dim-badge">{currentDims.label}</span>
          </span>
        </div>

        <div className="nx-dev-controls">
          <div className="nx-dev-button-group" role="group" aria-label="Window Size Presets">
            <button
              type="button"
              className={`nx-dev-pill-btn ${sizePreset === "slim" ? "is-active" : ""}`}
              onClick={() => setSizePreset("slim")}
              title="Slim Dock (400 × 1040) — Recent compact vertical dock"
            >
              📱 Slim (400)
            </button>
            <button
              type="button"
              className={`nx-dev-pill-btn ${sizePreset === "dock" ? "is-active" : ""}`}
              onClick={() => setSizePreset("dock")}
              title="Tall Dock (520 × 980) — Native unified sidebar geometry"
            >
              🖥️ Dock (520)
            </button>
            <button
              type="button"
              className={`nx-dev-pill-btn ${sizePreset === "modal" ? "is-active" : ""}`}
              onClick={() => setSizePreset("modal")}
              title="Expanded Command Center (740 × 960) — Full workstation modal"
            >
              🪟 Hub Modal (740)
            </button>
            <button
              type="button"
              className={`nx-dev-pill-btn ${sizePreset === "full" ? "is-active" : ""}`}
              onClick={() => setSizePreset("full")}
              title="Full Window (100% Fluid) — Fills your browser size"
            >
              ↔️ Full Window
            </button>
          </div>

          <div className="nx-dev-button-group">
            <AnimatedThemeToggler showLabel={true} />
            <button
              type="button"
              className={`nx-dev-pill-btn ${viewMode === "legacy" ? "is-active" : ""}`}
              onClick={() => setViewMode(viewMode === "hub" ? "legacy" : "hub")}
              title="Toggle between Command Hub and Legacy prototype dialog"
            >
              {viewMode === "hub" ? "Show Legacy" : "Show Command Hub"}
            </button>
            <button
              type="button"
              className="nx-dev-pill-btn nx-dev-popup-action"
              onClick={handleOpenPopup}
              title="Pop out into native standalone desktop window size"
            >
              ⧉ Pop out
            </button>
          </div>
        </div>
      </header>

      {/* ── Active Window Container ── */}
      <div
        className={`nx-dev-window-frame nx-frame-${sizePreset}`}
        style={
          sizePreset === "full"
            ? { width: "100%", height: "100%", flex: "1 1 0%", minHeight: 0, maxWidth: "100%" }
            : { width: currentDims.width, height: "100%", maxHeight: currentDims.height, flex: "1 1 0%", minHeight: 0 }
        }
      >
        <div className="unified-shell" style={{ width: "100%", height: "100%" }}>
          <div className="unified-view-host" style={{ width: "100%", height: "100%" }}>
            <div className="unified-view" style={{ display: "flex", width: "100%", height: "100%" }}>
              {viewMode === "hub" ? <SettingsSidebarApp onDock={() => {}} /> : <SettingsApp />}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <SettingsRoot />
  </React.StrictMode>
);
