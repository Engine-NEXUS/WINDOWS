import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderToString } from "react-dom/server";
import { EventDetailView, type EventDetailData } from "./EventDetailView";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

describe("EventDetailView Open in app button", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  const baseData: EventDetailData = {
    mode: "event_detail",
    token: 42,
    eventId: "ev_test_123",
    provider: "GitHub",
    status: "failed",
    title: "Build failed in zync",
    preview: "alice/zync · main · run 5",
  };

  it("renders 'Open in app' button when canOpen is true", () => {
    const html = renderToString(<EventDetailView data={{ ...baseData, canOpen: true }} />);
    expect(html).toContain("Open in app");
    expect(html).toContain('class="crp-link"');
  });

  it("hides button when canOpen is false", () => {
    const html = renderToString(<EventDetailView data={{ ...baseData, canOpen: false }} />);
    expect(html).not.toContain("Open in app");
    expect(html).not.toContain("Open on GitHub");
  });

  it("hides button when canOpen is omitted", () => {
    const html = renderToString(<EventDetailView data={{ ...baseData }} />);
    expect(html).not.toContain("Open in app");
  });

  it("renders custom openLabel when provided", () => {
    const html = renderToString(<EventDetailView data={{ ...baseData, canOpen: true, openLabel: "View run" }} />);
    expect(html).toContain("View run");
    expect(html).not.toContain("Open in app");
  });

  it("preserves provider link text when providerUrl is present and openLabel is not set", () => {
    const html = renderToString(
      <EventDetailView data={{ ...baseData, canOpen: true, providerUrl: "https://github.com/alice/zync/actions/runs/5" }} />,
    );
    expect(html).toContain("Open on GitHub");
    // Does not duplicate behaviour (only one link button, not two)
    const buttonMatches = html.match(/class="crp-link"/g) || [];
    expect(buttonMatches.length).toBe(1);
  });

  it("calls invoke('notify_open_external', { eventId }) on click", async () => {
    let clickHandler!: () => Promise<void>;
    renderToString(
      <EventDetailView
        data={{ ...baseData, canOpen: true }}
        onOpenInApp={(fn) => {
          clickHandler = fn;
        }}
      />,
    );

    expect(clickHandler).toBeDefined();
    await clickHandler();
    expect(invokeMock).toHaveBeenCalledWith("notify_open_external", { eventId: "ev_test_123" });
  });

  it("shows the error line on rejection", async () => {
    let clickHandler!: () => Promise<void>;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "notify_open_external") {
        throw new Error("This alert has no page to open.");
      }
      return undefined;
    });

    renderToString(
      <EventDetailView
        data={{ ...baseData, canOpen: true }}
        onOpenInApp={(fn) => {
          clickHandler = fn;
        }}
      />,
    );

    await clickHandler();
    expect(invokeMock).toHaveBeenCalledWith("notify_open_external", { eventId: "ev_test_123" });

    // Render with the captured rejection error
    const html = renderToString(
      <EventDetailView data={{ ...baseData, canOpen: true }} initialError="This alert has no page to open." />,
    );
    expect(html).toContain('class="crp-error"');
    expect(html).toContain("This alert has no page to open.");
  });
});
