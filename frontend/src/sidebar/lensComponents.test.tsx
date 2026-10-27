import { describe, it, expect } from "vitest";
import { renderToString } from "react-dom/server";
import { Lens } from "./Lens";
import { ZoomViewer } from "./ZoomViewer";
import { SpatialDashboard } from "./SpatialDashboard";
import { useSidebar, resolveEscDismiss, type SpatialData } from "./sidebarStore";

describe("Lens component rendering & accessibility", () => {
  it("renders container with role='button', tabIndex=0, and aria-label", () => {
    const html = renderToString(
      <Lens src="data:image/jpeg;base64,abc" alt="Screen preview" />
    );

    expect(html).toContain('role="button"');
    expect(html).toContain('tabindex="0"');
    expect(html).toContain('aria-label="Zoom Screen preview"');
    expect(html).toContain('class="lens-container"');
    expect(html).toContain('<img src="data:image/jpeg;base64,abc" alt="Screen preview"/>');
    expect(html).toContain('class="lens-bubble"');
  });

  it("supports wrapping children while preserving custom image classes", () => {
    const html = renderToString(
      <Lens
        src="data:image/jpeg;base64,abc"
        alt="Screen preview"
        className="spatial-screen-lens"
      >
        <img
          className="spatial-screen-image"
          src="data:image/jpeg;base64,abc"
          alt="The screen captured for this analysis"
        />
      </Lens>
    );

    expect(html).toContain("spatial-screen-lens");
    expect(html).toContain("spatial-screen-image");
    expect(html).toContain('class="lens-bubble"');
  });
});

describe("ZoomViewer component rendering & accessibility", () => {
  it("renders zoom viewer container and toolbar controls", () => {
    const html = renderToString(
      <ZoomViewer src="data:image/jpeg;base64,xyz" alt="Enlarged document" />
    );

    // Container fills lightbox body and is keyboard accessible
    expect(html).toContain('class="nexus-zoom-viewer"');
    expect(html).toContain('tabindex="0"');
    expect(html).toContain('role="region"');
    expect(html).toContain('aria-label="Interactive zoom viewer for Enlarged document"');

    // Image element
    expect(html).toContain('class="nexus-zoom-img"');
    expect(html).toContain('src="data:image/jpeg;base64,xyz"');
    expect(html).toContain('alt="Enlarged document"');

    // Floating toolbar
    expect(html).toContain('class="nexus-zoom-toolbar"');
    expect(html).toContain('role="toolbar"');
    expect(html).toContain('aria-label="Zoom in"');
    expect(html).toContain('aria-label="Zoom out"');
    expect(html).toContain('aria-label="Reset zoom"');
    expect(html).toContain("Reset");
    expect(html).toContain('class="nexus-zoom-readout"');
    expect(html).toContain("100 %");
  });
});

describe("sidebarStore openImage action", () => {
  it("openImage sets activeImage and triggers lightbox overlay hierarchy", () => {
    const store = useSidebar.getState();
    expect(store.activeImage).toBeNull();
    expect(resolveEscDismiss(store.activeImage !== null)).toBe("close-window");

    store.openImage("data:image/jpeg;base64,foo", "Detailed inspection");
    const updated = useSidebar.getState();
    expect(updated.activeImage).toEqual({
      src: "data:image/jpeg;base64,foo",
      alt: "Detailed inspection",
    });

    // Lightbox modal is open -> Esc hierarchy closes overlay first
    expect(resolveEscDismiss(updated.activeImage !== null)).toBe("close-overlay");

    // Close lightbox
    updated.setActiveImage(null);
    expect(useSidebar.getState().activeImage).toBeNull();
    expect(resolveEscDismiss(useSidebar.getState().activeImage !== null)).toBe("close-window");
  });
});

describe("SpatialDashboard Lens integration", () => {
  it("wraps screenshot, crops, and public page in Lens magnifier", () => {
    const mockData: SpatialData = {
      title: "Workspace Analysis",
      overview: "Overview of current window",
      provider_used: "gemini-flash",
      screen_image_data_url: "data:image/jpeg;base64,SCREEN_BASE64",
      items: [
        {
          id: 1,
          label: "Editor Panel",
          category: "code",
          summary: "VS Code editor",
          details: [{ title: "Language", value: "TypeScript" }],
          image_data_url: "data:image/jpeg;base64,CROP_BASE64",
        },
      ],
      review: {
        id: "rev12345678",
        created_at: "2026-10-20",
        target_app: "Browser",
        target_title: "Documentation",
        completeness: "complete",
        persistence: "saved",
        technologies: [],
        warnings: [],
        pages: [
          {
            url: "https://example.com/docs",
            title: "Docs Home",
            captured_at: "2026-10-20",
            image_data_url: "data:image/png;base64,PAGE_BASE64",
            method: "crawl",
            completeness: "complete",
          },
        ],
      },
    };

    const html = renderToString(<SpatialDashboard data={mockData} />);

    // 1. Screenshot wrapped in Lens
    expect(html).toContain('class="spatial-screen-image"');
    expect(html).toContain('src="data:image/jpeg;base64,SCREEN_BASE64"');
    expect(html).toContain('aria-label="Zoom The screen captured for this analysis"');

    // 2. Crop item wrapped in Lens
    expect(html).toContain('class="spatial-card-image"');
    expect(html).toContain('src="data:image/jpeg;base64,CROP_BASE64"');
    expect(html).toContain('aria-label="Zoom Captured view of Editor Panel"');

    // 3. Public page wrapped in Lens
    expect(html).toContain('src="data:image/png;base64,PAGE_BASE64"');
    expect(html).toContain('aria-label="Zoom Captured public page: Docs Home"');
  });
});
