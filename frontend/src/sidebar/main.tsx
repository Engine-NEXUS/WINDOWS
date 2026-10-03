import React from "react";
import ReactDOM from "react-dom/client";
import { UnifiedSidebar } from "./UnifiedSidebar";
// View styles — all four views live in this ONE window now.
import "./sidebar.css";
import "./unified.css";
import "../settings-sidebar/settings-sidebar.css";
import "../architect/architect.css";
import "../pr-list/pr-list.css";

// Global error handlers — catch uncaught exceptions so they're visible
// in the CDP monitor (which captures Runtime.exceptionThrown)
window.addEventListener("error", (e) => {
  console.error("[sidebar] Uncaught error:", e.error || e.message);
});
window.addEventListener("unhandledrejection", (e) => {
  console.error("[sidebar] Unhandled promise rejection:", e.reason);
});

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <UnifiedSidebar />
  </React.StrictMode>,
);
