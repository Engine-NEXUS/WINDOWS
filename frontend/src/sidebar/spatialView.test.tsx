import { describe, it, expect, beforeEach } from "vitest";
import { renderToString } from "react-dom/server";
import { SpatialView } from "./SpatialDashboard";
import { useSidebar, type SpatialData } from "./sidebarStore";

const MOCK_SPATIAL_DATA: SpatialData = {
  title: "Screen Analysis",
  overview: "Analysis overview of active workspace.",
  items: [
    {
      id: 1,
      label: "Visual Studio Code",
      category: "editor",
      summary: "Main code workspace",
      details: [{ title: "file", value: "App.tsx" }],
    },
    {
      id: 2,
      label: "Terminal",
      category: "terminal",
      summary: "Running dev server",
      details: [{ title: "pid", value: "1234" }],
    },
  ],
  provider_used: "gemini-2.0-flash",
};

describe("SpatialView is a plain window: no heading, no dock buttons", () => {
  beforeEach(() => {
    useSidebar.setState({
      spatialData: null,
    });
  });

  it("renders the dashboard in .sidebar-view-col behind an invisible drag strip, with no header row or dock buttons", () => {
    const html = renderToString(
      <div className="unified-view" style={{ display: "flex", flexDirection: "row" }}>
        <SpatialView data={MOCK_SPATIAL_DATA} onDock={() => {}} />
      </div>,
    );

    expect(html).toContain('class="unified-view"');
    expect(html).toMatch(/<div class="sidebar-view-col">\s*<div class="sidebar-drag-strip"/);
    expect(html).toContain('class="spatial-dashboard"');
    // The window is plain: no title bar and no dock buttons (arrow keys dock it).
    expect(html).not.toContain("sidebar-header-row");
    expect(html).not.toContain('title="Dock Left"');
    expect(html).not.toContain('title="Dock Right"');
    expect(html).not.toContain("dock-icon");
  });

  it("renders the empty state the same way", () => {
    const html = renderToString(
      <div className="unified-view" style={{ display: "flex", flexDirection: "row" }}>
        <SpatialView data={null} />
      </div>,
    );

    expect(html).toMatch(/<div class="sidebar-view-col">\s*<div class="sidebar-drag-strip"/);
    expect(html).not.toContain("sidebar-header-row");
    expect(html).toContain('class="spatial-dashboard spatial-dashboard--empty"');
    expect(html).toContain("No spatial analysis yet, sir");
  });
});
