// The one dock control. Every sidebar view put a pair of these in its header, each with its own markup, glyph characters
// (◧ ◨, which depend on the installed font) and, in the Hub, its own CSS. Now: one component, SVG icons that follow the
// text colour, an accessible name, and the single `.sidebar-dock-btn` style in unified.css (token colours, 32 px,
// visible in both themes, focus ring).

export type DockSide = "left" | "right";

const LABEL: Record<DockSide, string> = {
  left: "Dock to the left",
  right: "Dock to the right",
};

/** A window with the docked side filled. Uses currentColor, so it is right in light and dark. */
export function DockIcon({ side }: { side: DockSide }) {
  const filled = side === "left" ? { x: 2.5, width: 4.3 } : { x: 9.2, width: 4.3 };
  return (
    <svg className="dock-icon" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">
      <rect x="1.7" y="2.7" width="12.6" height="10.6" rx="2" fill="none" stroke="currentColor" strokeWidth="1.4" />
      <rect x={filled.x} y="3.6" width={filled.width} height="8.8" rx="1" fill="currentColor" />
    </svg>
  );
}

export function DockButton({ side, onDock }: { side: DockSide; onDock: (side: DockSide) => void }) {
  return (
    <button type="button" className="sidebar-dock-btn" onClick={() => onDock(side)} title={LABEL[side]} aria-label={LABEL[side]}>
      <DockIcon side={side} />
    </button>
  );
}

/** The left/right pair, as it sits in a header row. */
export function DockButtons({ onDock }: { onDock: (side: DockSide) => void }) {
  return (
    <div className="sidebar-dock-controls" role="group" aria-label="Dock position">
      <DockButton side="left" onDock={onDock} />
      <DockButton side="right" onDock={onDock} />
    </div>
  );
}
