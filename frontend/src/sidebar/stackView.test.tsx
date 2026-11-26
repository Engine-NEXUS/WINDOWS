import { describe, it, expect, beforeEach, vi } from "vitest";
import { renderToString } from "react-dom/server";
import { StackView } from "./StackView";
import { useSidebar } from "./sidebarStore";
import { SAMPLE_PAYLOAD } from "./devStack";
import * as Kit from "./kit";
import type { StackPayload } from "./stackTypes";

describe("StackView component rendering (Phone Layout)", () => {
  beforeEach(() => {
    useSidebar.setState({
      stackData: null,
      stackProgress: null,
      stackFocus: [],
    });
  });

  it("renders empty state when no payload is present", () => {
    const html = renderToString(<StackView />);
    expect(html).toContain("Say: Nexus, reverse engineer this website");
    expect(html).toContain("stack-view--empty");
    expect(html).not.toContain("Dock Left");
    expect(html).not.toContain("Dock Right");
    expect(html).toContain("Tech Stack");
    expect(html).toContain("Web Reverse Engineer");
  });

  it("renders full phone-style layout when payload is provided", () => {
    const html = renderToString(
      <StackView
        payload={SAMPLE_PAYLOAD}
        progress={{ phase: "done", text: "Analysis complete", step: 5, total: 5 }}
        focus={["nextjs"]}
      />,
    );

    // 1. AppBar
    expect(html).toContain("Acme Cloud Platform");
    expect(html).toContain("acme.com · Tech stack");
    expect(html).toContain('title="Copy Report"');
    expect(html).not.toContain("Dock Left");
    expect(html).not.toContain("Dock Right");

    // 2. Hero Card
    expect(html).toContain("Acme is a modern Next.js 15 web application");
    expect(html).toContain("Next.js 15");
    // hero stat tiles (technologies / scripts / hosts) replaced the one-line summary
    expect(html).toContain("stack-stat-tiles");
    expect(html).toContain("Technologies");
    expect(html).toContain("Scripts");
    expect(html).toContain("Hosts");
    expect(html).toContain("stack-avatars");
    expect(html).toContain("Copy report →");

    // 3. Stack Mix Card
    expect(html).toContain("Stack mix");
    expect(html).toContain("By category");
    expect(html).toContain("By confidence");
    expect(html).toContain('class="kit-donut "');
    expect(html).toContain("Frontend Frameworks");

    // 4. Security Headers Card
    expect(html).toContain("Security headers");
    expect(html).toContain("4/6");
    expect(html).toContain("HSTS");
    expect(html).toContain("CSP");
    expect(html).toContain("X-Frame-Options");

    // 5. Technologies Card
    expect(html).toContain("Technologies");
    expect(html).toContain('placeholder="Search technologies..."');
    expect(html).toContain("Tailwind CSS");
    expect(html).toContain("Stripe Billing &amp; Elements");
    expect(html).toContain("Confirmed");

    // Card focus highlight
    expect(html).toContain("stack-tech-row--focus");

    // 6. Page Weight Card
    expect(html).toContain("Page weight");
    expect(html).toContain("HTML");
    expect(html).toContain("JavaScript");
    expect(html).toContain("CSS");
    expect(html).toContain("images detected");
    expect(html).toContain("Third-party hosts (10)");

    // 7. On This Page Card
    expect(html).toContain("On this page");
    expect(html).toContain("Hero &amp; Value Proposition");
    expect(html).toContain("Tiered Pricing &amp; Checkout");

    // 8. Footer
    expect(html).toContain("Not detectable from outside: server code, databases, private APIs");
    expect(html).toContain("Save as .md");
    expect(html).toContain("Logos by Logo.dev");
  });

  it("applies focus highlight class only to focused technology row", () => {
    const html = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} focus={["stripe"]} />,
    );

    // Row for stripe should have the focus class
    const stripeRowMatch = html.match(/class="[^"]*stack-tech-row[^"]*"[^>]*data-tech-id="stripe"/);
    expect(stripeRowMatch).not.toBeNull();
    expect(stripeRowMatch![0]).toContain("stack-tech-row--focus");

    // Next.js row should NOT have the focus class
    const nextRowMatch = html.match(/class="[^"]*stack-tech-row[^"]*"[^>]*data-tech-id="nextjs"/);
    expect(nextRowMatch).not.toBeNull();
    expect(nextRowMatch![0]).not.toContain("stack-tech-row--focus");
  });

  it("filters technologies by search query (name and category)", () => {
    // Search for "Stripe"
    const stripeHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialSearch="Stripe" />,
    );
    expect(stripeHtml).toContain("Stripe Billing &amp; Elements");
    expect(stripeHtml).not.toContain('data-tech-id="nextjs"');
    expect(stripeHtml).not.toContain('data-tech-id="tailwind"');

    // Search for "frontend" (category match)
    const catHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialSearch="frontend" />,
    );
    expect(catHtml).toContain('data-tech-id="nextjs"');
    expect(catHtml).toContain('data-tech-id="react"');
    expect(catHtml).not.toContain('data-tech-id="stripe"');

    // Search for non-existent query
    const emptyHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialSearch="nonexistent_xyz" />,
    );
    expect(emptyHtml).toContain('No technologies found matching &quot;nonexistent_xyz&quot;');
  });

  it("filters technologies by confidence dropdown", () => {
    // Filter Confirmed only (>= 0.8)
    const confirmedHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialConfidence="Confirmed" />,
    );
    expect(confirmedHtml).toContain("Next.js 15");
    expect(confirmedHtml).not.toContain('data-tech-id="swr"'); // SWR is Likely (0.78)

    // Filter Likely only
    const likelyHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialConfidence="Likely" />,
    );
    expect(likelyHtml).toContain("SWR");
    expect(likelyHtml).not.toContain('data-tech-id="nextjs"');
  });

  it("toggles evidence display based on expanded state", () => {
    // Next.js expanded
    const expandedHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialExpanded={{ nextjs: true }} />,
    );
    expect(expandedHtml).toContain('class="stack-evidence-list"');
    expect(expandedHtml).toContain("window.__NEXT_DATA__ inline payload");

    // None expanded
    const collapsedHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} initialExpanded={{}} />,
    );
    expect(collapsedHtml).not.toContain('class="stack-evidence-list"');
  });

  it("has no dock buttons in either state (Left / Right arrow keys dock the window)", () => {
    const iconBtnSpy = vi.spyOn(Kit, "IconButton");
    renderToString(<StackView />);
    renderToString(<StackView payload={SAMPLE_PAYLOAD} />);
    const labels = iconBtnSpy.mock.calls.map((c) => c[0]?.label);
    expect(labels).not.toContain("Dock Left");
    expect(labels).not.toContain("Dock Right");
    expect(labels).toContain("Copy Report");
    iconBtnSpy.mockRestore();
  });

  it("shows Logos by Logo.dev attribution only when at least one detection has a logo", () => {
    // SAMPLE_PAYLOAD has detections with logo: true
    const withLogosHtml = renderToString(
      <StackView payload={SAMPLE_PAYLOAD} />,
    );
    expect(withLogosHtml).toContain("Logos by Logo.dev");

    // Payload where NO detections have a logo
    const withoutLogosPayload: StackPayload = {
      ...SAMPLE_PAYLOAD,
      detections: SAMPLE_PAYLOAD.detections.map((d) => ({
        ...d,
        logo: null,
      })),
    };

    const withoutLogosHtml = renderToString(
      <StackView payload={withoutLogosPayload} />,
    );
    expect(withoutLogosHtml).not.toContain("Logos by Logo.dev");
  });

  it("a refused scan shows only the reason, never empty security or weight findings", () => {
    const refused: StackPayload = {
      ...SAMPLE_PAYLOAD,
      failed: true,
      detections: [],
      sections: [],
      summary: "I do not scan private or banking sites, sir.",
    };
    const html = renderToString(<StackView payload={refused} />);
    expect(html).toContain("I do not scan private or banking sites, sir.");
    expect(html).toContain("Not scanned");
    expect(html).not.toContain("headers set");
    expect(html).not.toContain("Page weight");
    expect(html).not.toContain("Stack mix");
    // a visual-only walkthrough that has sections keeps the normal view
    const visual = renderToString(<StackView payload={{ ...refused, sections: SAMPLE_PAYLOAD.sections }} />);
    expect(visual).not.toContain("Not scanned");
  });

  it("renders scanning state with status dot and skeleton headline", () => {
    const html = renderToString(
      <StackView
        payload={{
          ...SAMPLE_PAYLOAD,
          scanning: true,
          summary: "",
        }}
        progress={{ phase: "scanning", text: "Scanning scripts...", step: 2, total: 5 }}
      />,
    );

    expect(html).toContain("kit-app-bar-status-dot--scanning");
    expect(html).toContain("Scanning scripts...");
    expect(html).toContain("stack-skeleton-block");
  });

  it("tolerates malformed or missing fields without throwing", () => {
    useSidebar.setState({
      stackData: {
        title: "",
        url: "",
        final_url: "",
        status_code: 200,
        lang: "",
        description: "",
        scanning: false,
        detections: [],
        groups: [],
        security: {} as any,
        stats: {} as any,
        notes: [],
        sections: [],
        markdown: "",
        summary: "",
      },
    });

    expect(() => renderToString(<StackView />)).not.toThrow();
  });

  it("words an app scan as an app and never parses the exe as a web address", () => {
    const html = renderToString(
      <StackView
        payload={{
          ...SAMPLE_PAYLOAD,
          kind: "app",
          title: "Slack",
          url: "slack.exe",
          final_url: "slack.exe",
          sections: [{ idx: 0, title: "Channel list", summary: "Left rail", stack_refs: [] }],
        }}
      />,
    );
    expect(html).toContain("slack.exe");
    expect(html).toContain("App stack");
    expect(html).toContain("In this window");
    expect(html).toContain("install folder only");
    expect(html).not.toContain("Security headers");
    expect(html).not.toContain("Page weight");
    expect(html).not.toContain("third-party hosts");
    // Sections are numbered from 1, not from their 0-based index.
    expect(html).toMatch(/stack-section-num">1</);
  });

  it("explains a thin scan and shows the deeper render while it runs", () => {
    const html = renderToString(
      <StackView
        payload={{
          ...SAMPLE_PAYLOAD,
          rendering: true,
          thin: ["The site showed me a Cloudflare bot check, so I could only see its hosting."],
        }}
      />,
    );
    expect(html).toContain("stack-thin-list");
    expect(html).toContain("Cloudflare bot check");
    expect(html).toContain("hidden browser window");
    expect(html).toContain("kit-app-bar-status-dot--scanning");
  });

  it("shows a captured version next to the technology name", () => {
    const html = renderToString(
      <StackView
        payload={{
          ...SAMPLE_PAYLOAD,
          detections: [
            { ...SAMPLE_PAYLOAD.detections[0], version: "14.2.3" },
            ...SAMPLE_PAYLOAD.detections.slice(1),
          ],
        }}
      />,
    );
    expect(html).toContain("stack-tech-version");
    expect(html).toContain("14.2.3");
  });

  it("a new scan clears the previous scan's progress and lit cards", () => {
    useSidebar.setState({
      stackProgress: { phase: "done", text: "Reverse engineering done" },
      stackFocus: ["nextjs"],
    });
    useSidebar.getState().showStack({ ...SAMPLE_PAYLOAD, scanning: true });
    expect(useSidebar.getState().stackProgress).toBeNull();
    expect(useSidebar.getState().stackFocus).toEqual([]);
    // a later payload of the same scan keeps the live progress
    useSidebar.getState().setStackProgress({ phase: "touring", text: "Touring", step: 1, total: 4 });
    useSidebar.getState().showStack({ ...SAMPLE_PAYLOAD, scanning: false });
    expect(useSidebar.getState().stackProgress?.phase).toBe("touring");
  });
});
