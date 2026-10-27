import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderToString } from "react-dom/server";
import { SpatialDashboard, SpatialCard, SpatialView } from "./SpatialDashboard";
import type { SpatialData, SpatialItemCard } from "./sidebarStore";

// Mock ipc to verify hover emitter
const emitLoggedMock = vi.fn();
vi.mock("../ipc", () => ({
  emitLogged: (...args: unknown[]) => emitLoggedMock(...args),
}));

describe("SpatialDashboard and SpatialCard Kit Restyle", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  const MOCK_ITEMS: SpatialItemCard[] = [
    {
      id: 1,
      label: "Visual Studio Code",
      category: "editor",
      summary: "Main code workspace",
      details: [
        { title: "File", value: "App.tsx" },
        { title: "Language", value: "TypeScript" },
      ],
      image_data_url: "data:image/jpeg;base64,VlNDT1BFX0NS",
      logo_data_url: "data:image/png;base64,VlNDT0RFX0xPR08=",
    },
    {
      id: 2,
      label: "Terminal",
      category: "terminal",
      summary: "Running dev server",
      details: [{ title: "PID", value: "1234" }],
      // no logo_data_url -> should render monogram
      // no image_data_url
    },
  ];

  const MOCK_DATA: SpatialData = {
    title: "Workspace Analysis",
    overview: "Active developer workspace analysis overview",
    provider_used: "gemini-2.0-flash",
    screen_image_data_url: "data:image/jpeg;base64,U0NSRUVOU0hPVA==",
    items: MOCK_ITEMS,
  };

  it("renders brand logo image when logo_data_url is set and monogram otherwise", () => {
    const htmlWithLogo = renderToString(
      <SpatialCard item={MOCK_ITEMS[0]} active={false} onHover={() => {}} />,
    );
    expect(htmlWithLogo).toContain('data-mode="image"');
    expect(htmlWithLogo).toContain('src="data:image/png;base64,VlNDT0RFX0xPR08="');
    expect(htmlWithLogo).toContain('alt="Visual Studio Code"');

    const htmlWithoutLogo = renderToString(
      <SpatialCard item={MOCK_ITEMS[1]} active={false} onHover={() => {}} />,
    );
    expect(htmlWithoutLogo).toContain('data-mode="monogram"');
    expect(htmlWithoutLogo).toContain('data-monogram="T"');
    expect(htmlWithoutLogo).toContain(">T</span>");
  });

  it("wraps the crop in Lens with hover magnifier and click zoom", () => {
    const html = renderToString(
      <SpatialCard item={MOCK_ITEMS[0]} active={false} onHover={() => {}} />,
    );
    expect(html).toContain('class="spatial-card-image"');
    expect(html).toContain('src="data:image/jpeg;base64,VlNDT1BFX0NS"');
    expect(html).toContain('aria-label="Zoom Captured view of Visual Studio Code"');
  });

  it("preserves card ids as spatial-card-{id}", () => {
    const html = renderToString(<SpatialDashboard data={MOCK_DATA} />);
    expect(html).toContain('id="spatial-card-1"');
    expect(html).toContain('id="spatial-card-2"');
  });

  it("invokes hover callback with item id on mouseEnter and null on mouseLeave", () => {
    const onHover = vi.fn();
    let cardEl: any;
    function TestCard() {
      cardEl = SpatialCard({ item: MOCK_ITEMS[0], active: false, onHover });
      return cardEl;
    }
    renderToString(<TestCard />);
    expect(cardEl.props.id).toBe("spatial-card-1");

    cardEl.props.onMouseEnter();
    expect(onHover).toHaveBeenCalledWith(1);

    cardEl.props.onMouseLeave();
    expect(onHover).toHaveBeenCalledWith(null);
  });

  it("applies the active-card class when active is true", () => {
    const activeHtml = renderToString(
      <SpatialCard item={MOCK_ITEMS[0]} active={true} onHover={() => {}} />,
    );
    expect(activeHtml).toContain("spatial-card spatial-card--active");

    const inactiveHtml = renderToString(
      <SpatialCard item={MOCK_ITEMS[0]} active={false} onHover={() => {}} />,
    );
    expect(inactiveHtml).toContain('class="spatial-card "');
    expect(inactiveHtml).not.toContain("spatial-card--active");
  });

  it("dock buttons call onDock('left') and onDock('right') and live in the first child of .sidebar-view-col", () => {
    const handleDock = vi.fn();
    let viewEl: any;
    function TestView() {
      viewEl = SpatialView({ data: MOCK_DATA, onDock: handleDock });
      return viewEl;
    }
    const html = renderToString(<TestView />);

    // Assert that .sidebar-header-row is the FIRST child inside .sidebar-view-col
    expect(html).toMatch(/<div class="sidebar-view-col">\s*<header class="[^"]*sidebar-header-row/);

    // Verify first child of .sidebar-view-col is AppBar
    const appBarChild = viewEl.props.children[0];
    expect(appBarChild.props.className).toBe("sidebar-header-row");

    const actions = appBarChild.props.actions;
    const [leftBtn, rightBtn] = actions.props.children;

    expect(leftBtn.props.label).toBe("Dock Left");
    expect(leftBtn.props.className).toBe("sidebar-dock-btn");
    leftBtn.props.onClick();
    expect(handleDock).toHaveBeenCalledWith("left");

    expect(rightBtn.props.label).toBe("Dock Right");
    expect(rightBtn.props.className).toBe("sidebar-dock-btn");
    rightBtn.props.onClick();
    expect(handleDock).toHaveBeenCalledWith("right");
  });

  it("renders 380px phone column, hero card, item list card, and definition list", () => {
    const html = renderToString(<SpatialDashboard data={MOCK_DATA} />);

    // 380px column
    expect(html).toContain("spatial-scroll-body");
    expect(html).toContain("spatial-phone-col");

    // Hero Card
    expect(html).toContain("spatial-hero-card");
    expect(html).toContain('class="spatial-screen-image"');
    expect(html).toContain('src="data:image/jpeg;base64,U0NSRUVOU0hPVA=="');
    expect(html).toContain('aria-label="Zoom The screen captured for this analysis"');
    expect(html).toContain("Active developer workspace analysis overview");
    expect(html).toContain("gemini-2.0-flash");

    // Parts of the screen Card
    expect(html).toContain("spatial-items-card");
    expect(html).toContain("Parts of the screen");
    // Trailing chip with count 2
    expect(html).toContain(">2</span>");

    // Definition list
    expect(html).toContain("<dl");
    expect(html).toContain('class="spatial-detail-grid"');
    expect(html).toContain("<dt");
    expect(html).toContain('class="spatial-detail-title"');
    expect(html).toContain("File");
    expect(html).toContain("<dd");
    expect(html).toContain('class="spatial-detail-value"');
    expect(html).toContain("App.tsx");
  });

  it("renders Foreground review as its own Card when review is present", () => {
    const dataWithReview: SpatialData = {
      ...MOCK_DATA,
      review: {
        id: "rev12345678",
        created_at: "2026-10-21",
        target_app: "VS Code",
        target_title: "My Project",
        completeness: "complete",
        persistence: "saved",
        technologies: [
          { name: "React", status: "confirmed", evidence: "Found in package.json", source_url: "https://react.dev" },
        ],
        warnings: [],
        pages: [],
      },
    };

    const html = renderToString(<SpatialDashboard data={dataWithReview} />);
    expect(html).toContain("foreground-review-card");
    expect(html).toContain("Foreground review");
    expect(html).toContain('aria-label="Foreground review"');
    expect(html).toContain("Technology evidence");
    expect(html).toContain("React");
    expect(html).toContain("Recreate this review");
  });
});
