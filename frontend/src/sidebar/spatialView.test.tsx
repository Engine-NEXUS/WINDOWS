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

describe("SpatialView dock button layout and column container", () => {
  beforeEach(() => {
    useSidebar.setState({
      spatialData: null,
    });
  });

  it("renders with data inside flex-row wrapper, sharing .sidebar-view-col with header as first child", () => {
    const handleDock = (_dir: string) => {};

    const html = renderToString(
      <div className="unified-view" style={{ display: "flex", flexDirection: "row" }}>
        <SpatialView data={MOCK_SPATIAL_DATA} onDock={handleDock} />
      </div>,
    );

    // Verify parent flex row wrapper
    expect(html).toContain('class="unified-view"');

    // Verify header and dashboard are wrapped together in .sidebar-view-col
    expect(html).toContain('class="sidebar-view-col"');
    expect(html).toContain("sidebar-header-row");
    expect(html).toContain('class="spatial-dashboard"');

    // Assert that .sidebar-header-row is the FIRST child inside .sidebar-view-col
    expect(html).toMatch(/<div class="sidebar-view-col">\s*<header class="[^"]*sidebar-header-row/);

    // Assert that the dashboard body is inside the same container following the header
    const colIdx = html.indexOf('class="sidebar-view-col"');
    const headerIdx = html.indexOf("sidebar-header-row");
    const dashboardIdx = html.indexOf('class="spatial-dashboard"');

    expect(colIdx).toBeGreaterThan(-1);
    expect(headerIdx).toBeGreaterThan(colIdx);
    expect(dashboardIdx).toBeGreaterThan(headerIdx);

    // Dock buttons are inside the header row
    expect(html).toContain('title="Dock Left"');
    expect(html).toContain('title="Dock Right"');
    expect(html).toContain("◧");
    expect(html).toContain("◨");
  });

  it("renders empty state wrapped in .sidebar-view-col with header as first child", () => {
    const html = renderToString(
      <div className="unified-view" style={{ display: "flex", flexDirection: "row" }}>
        <SpatialView data={null} />
      </div>,
    );

    expect(html).toContain('class="sidebar-view-col"');
    expect(html).toMatch(/<div class="sidebar-view-col">\s*<header class="[^"]*sidebar-header-row/);
    expect(html).toContain('class="spatial-dashboard spatial-dashboard--empty"');
    expect(html).toContain("No spatial analysis yet, sir");
  });
});
