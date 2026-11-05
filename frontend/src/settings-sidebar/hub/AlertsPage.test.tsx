import { describe, expect, it, vi } from "vitest";
import { renderToString } from "react-dom/server";
import { AlertsPage } from "./AlertsPage";
import type { AlertItem } from "./alertsModel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue([]),
}));

describe("AlertsPage component", () => {
  it("renders top switches (NEXUS Toast, Windows Fallback, Pass DND) and privacy controls", () => {
    const html = renderToString(<AlertsPage />);

    expect(html).toContain("NEXUS Toast");
    expect(html).toContain("Windows Notification Fallback");
    expect(html).toContain("Pass Do Not Disturb for Failures");

    expect(html).toContain("Alert Privacy");
    expect(html).toContain("Normal");
    expect(html).toContain("Private");
    expect(html).toContain("Hidden");

    // Default privacy explanation (private)
    expect(html).toContain("preview text is hidden");
  });

  it("renders empty state 'No alerts yet.' when loading has finished with no alerts", () => {
    const html = renderToString(<AlertsPage initialAlerts={[]} initialLoading={false} />);
    expect(html).toContain("No alerts yet.");
    expect(html).toContain("Recent Alerts (0)");
  });

  it("renders error state and a Retry button when initialError is provided", () => {
    const html = renderToString(
      <AlertsPage initialAlerts={[]} initialLoading={false} initialError="Failed to fetch ledger" />
    );
    expect(html).toContain("Failed to fetch ledger");
    expect(html).toContain("Retry");
    expect(html).toContain('role="alert"');
  });

  it("renders alert rows with bold title, provider, status chip, relative time, count badge, and unread dot", () => {
    const now = Date.now();
    const testAlerts: AlertItem[] = [
      {
        eventId: "ev_github_fail",
        provider: "GitHub",
        icon: "github",
        status: "failed",
        title: "Build failed in engine",
        preview: "ci.yml line 42 check failed",
        updatedAtMs: now - 120_000, // 2 min ago
        read: false,
        spoken: true,
        count: 3,
      },
      {
        eventId: "ev_vercel_ok",
        provider: "Vercel",
        icon: "vercel",
        status: "recovered",
        title: "Deploy recovered",
        preview: "production preview ready",
        updatedAtMs: now - 3_600_000, // 1 hour ago
        read: true,
        spoken: false,
        count: 1,
      },
    ];

    const html = renderToString(
      <AlertsPage initialAlerts={testAlerts} initialLoading={false} />
    );

    // Grouping
    expect(html).toContain("Today");
    expect(html).toContain("Recent Alerts (2)");

    // First alert (GitHub, failed, unread, count 3)
    expect(html).toContain("Build failed in engine");
    expect(html).toContain("ci.yml line 42 check failed");
    expect(html).toContain("Failed");
    expect(html).toContain("+3");
    expect(html).toContain("2 min ago");
    expect(html).toContain('class="hx-alert-unread-dot"');

    // Second alert (Vercel, recovered, read, count 1)
    expect(html).toContain("Deploy recovered");
    expect(html).toContain("production preview ready");
    expect(html).toContain("Recovered");
    expect(html).not.toContain("+1"); // count badge only when count > 1
  });

  it("renders custom privacy setting and explanation correctly", () => {
    const html = renderToString(
      <AlertsPage initialSettings={{ notifyPrivacy: "hidden", notifyToast: false }} />
    );

    expect(html).toContain("All details masked; shows generic provider notice only.");
  });

  it("disables Clear button when there are no alerts", () => {
    const html = renderToString(<AlertsPage initialAlerts={[]} initialLoading={false} />);
    expect(html).toContain("<button");
    expect(html).toMatch(/<button[^>]*disabled[^>]*>Clear<\/button>/);
  });
});
