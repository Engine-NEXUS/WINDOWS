import { describe, expect, it, vi } from "vitest";
import { renderToString } from "react-dom/server";
import { DockButton, DockButtons, DockIcon } from "./DockButtons";

describe("shared dock buttons", () => {
  it("renders a left and a right button with accessible names, one shared class and SVG icons", () => {
    const html = renderToString(<DockButtons onDock={() => {}} />);
    expect(html).toContain('aria-label="Dock to the left"');
    expect(html).toContain('aria-label="Dock to the right"');
    expect((html.match(/class="sidebar-dock-btn"/g) ?? []).length).toBe(2);
    expect((html.match(/<svg/g) ?? []).length).toBe(2);
    expect(html).toContain('role="group"');
    // No font-dependent glyph characters.
    expect(html).not.toMatch(/[◧◨]/);
  });

  it("the icons are decorative (hidden from screen readers) and colour with the text", () => {
    const html = renderToString(<DockIcon side="left" />);
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain("currentColor");
  });

  it("the left icon fills the left pane and the right icon the right pane", () => {
    const left = renderToString(<DockIcon side="left" />);
    const right = renderToString(<DockIcon side="right" />);
    expect(left).toContain('x="2.5"');
    expect(right).toContain('x="9.2"');
  });

  it("clicking reports the side", () => {
    const onDock = vi.fn();
    const el = DockButton({ side: "right", onDock });
    el.props.onClick();
    expect(onDock).toHaveBeenCalledWith("right");
  });
});
