import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ActionReceipt, ActionReceiptCard } from "./ActionReceipt";
import type { ActionReceipt as ActionReceiptType } from "./actionReceiptModel";

describe("ActionReceipt component", () => {
  it("renders null initially when no receipt has been received", () => {
    const html = renderToStaticMarkup(<ActionReceipt />);
    expect(html).toBe("");
  });
});

describe("ActionReceiptCard rendering", () => {
  it("renders success receipt with headline, subject, dot-ok, role status and aria-live polite", () => {
    const receipt: ActionReceiptType = {
      kind: "open_app",
      headline: "Opened",
      subject: "Notepad",
      title: null,
      detail: null,
      ok: true,
    };
    const html = renderToStaticMarkup(<ActionReceiptCard receipt={receipt} />);
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("action-receipt-dot--ok");
    expect(html).not.toContain("action-receipt-dot--fail");
    expect(html).toContain("Opened - Notepad");
    expect(html).toContain("/activity-logos/notepad.svg");
    expect(html).toContain('alt="Notepad"');
    expect(html).not.toContain("action-receipt-title");
    expect(html).not.toContain("action-receipt-detail");
  });

  it("renders failure receipt with dot-fail and detail", () => {
    const receipt: ActionReceiptType = {
      kind: "open_app",
      headline: "Tried to open",
      subject: "Notepad",
      title: null,
      detail: "I could not confirm it came to the front.",
      ok: false,
    };
    const html = renderToStaticMarkup(<ActionReceiptCard receipt={receipt} />);
    expect(html).toContain("action-receipt-dot--fail");
    expect(html).not.toContain("action-receipt-dot--ok");
    expect(html).toContain("Tried to open - Notepad");
    expect(html).toContain("/activity-logos/notepad.svg");
    expect(html).toContain("action-receipt-detail");
    expect(html).toContain("I could not confirm it came to the front.");
    expect(html).not.toContain("action-receipt-title");
  });

  it("renders full search receipt with title and detail", () => {
    const receipt: ActionReceiptType = {
      kind: "search",
      headline: "Opened",
      subject: "youtube.com",
      title: "lofi beats",
      detail: "Search results. Nothing is playing yet.",
      ok: true,
    };
    const html = renderToStaticMarkup(<ActionReceiptCard receipt={receipt} />);
    expect(html).toContain("action-receipt-dot--ok");
    expect(html).toContain("Opened - youtube.com");
    expect(html).toContain("/activity-logos/youtube.svg");
    expect(html).toContain("action-receipt-title");
    expect(html).toContain("lofi beats");
    expect(html).toContain("action-receipt-detail");
    expect(html).toContain("Search results. Nothing is playing yet.");
  });

  it("renders brand logo for aliased subject like Visual Studio Code", () => {
    const receipt: ActionReceiptType = {
      kind: "open_app",
      headline: "Opened",
      subject: "Visual Studio Code",
      title: null,
      detail: null,
      ok: true,
    };
    const html = renderToStaticMarkup(<ActionReceiptCard receipt={receipt} />);
    expect(html).toContain("/activity-logos/vscode.svg");
    expect(html).toContain('alt="Visual Studio Code"');
    expect(html).toContain("Opened - Visual Studio Code");
  });

  it("renders monogram fallback for unknown subject without broken image", () => {
    const receipt: ActionReceiptType = {
      kind: "open_app",
      headline: "Opened",
      subject: "Foo Tool",
      title: null,
      detail: null,
      ok: true,
    };
    const html = renderToStaticMarkup(<ActionReceiptCard receipt={receipt} />);
    expect(html).not.toContain("<img");
    expect(html).toContain('data-mode="monogram"');
    expect(html).toContain('data-monogram="F"');
    expect(html).toContain("kit-logo-monogram");
    expect(html).toContain("Opened - Foo Tool");
  });
});
