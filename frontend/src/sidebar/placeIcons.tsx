import type { ReactElement } from "react";

/**
 * The place card's icon set: simple outline icons drawn for NEXUS (24 px grid, 1.75 px stroke, `currentColor`), so
 * they follow the theme. No third-party logos are used; the two link buttons are labelled with these neutral glyphs.
 */
const PATHS: Record<string, ReactElement> = {
  pin: (
    <>
      <path d="M12 21s-6-5.2-6-10a6 6 0 1 1 12 0c0 4.8-6 10-6 10z" />
      <circle cx="12" cy="11" r="2.2" />
    </>
  ),
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  phone: <path d="M5 4h4l2 5-2.5 1.5a11 11 0 0 0 5 5L15 13l5 2v4a2 2 0 0 1-2 2A16 16 0 0 1 3 6a2 2 0 0 1 2-2z" />,
  copy: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15V6a2 2 0 0 1 2-2h9" />
    </>
  ),
  car: (
    <>
      <path d="M4 15v-3l2-5h12l2 5v3" />
      <rect x="3" y="15" width="18" height="3" rx="1" />
      <circle cx="7.5" cy="18" r="1.2" />
      <circle cx="16.5" cy="18" r="1.2" />
    </>
  ),
  bike: (
    <>
      <circle cx="6" cy="17" r="3" />
      <circle cx="18" cy="17" r="3" />
      <path d="M6 17h5l3-7h3M14 10h-3" />
    </>
  ),
  auto: (
    <>
      <path d="M4 16V9a2 2 0 0 1 2-2h8l4 5v4" />
      <path d="M4 16h14" />
      <circle cx="7" cy="18" r="1.6" />
      <circle cx="16" cy="18" r="1.6" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5M12 8h.01" />
    </>
  ),
  x: <path d="M6 6l12 12M18 6L6 18" />,
  chevron: <path d="M6 9l6 6 6-6" />,
  directions: <path d="M3 11l18-8-8 18-2-8z" />,
  globe: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18" />
    </>
  ),
  star: <path d="M12 3l2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1L3.2 9.5l6.1-.9z" />,
  utensils: <path d="M7 3v8M5 3v5a2 2 0 0 0 4 0V3M7 11v10M17 3c-2 2-2 6 0 8v10" />,
  bag: (
    <>
      <path d="M6 8h12l1 12H5L6 8z" />
      <path d="M9 8a3 3 0 0 1 6 0" />
    </>
  ),
  store: <path d="M4 9l1-5h14l1 5M4 9a2.5 2.5 0 0 0 5 0 2.5 2.5 0 0 0 5 0 2.5 2.5 0 0 0 5 0M5 11v9h14v-9" />,
  landmark: <path d="M3 20h18M5 20V10M9 20V10M15 20V10M19 20V10M2 10l10-6 10 6z" />,
  hospital: (
    <>
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M12 8v8M8 12h8" />
    </>
  ),
  fuel: <path d="M5 20V5a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1v15M4 20h11M14 9h2a2 2 0 0 1 2 2v5a1.5 1.5 0 0 0 3 0V8l-3-3" />,
  pill: <path d="M10.5 20.5a5 5 0 0 1-7-7l9-9a5 5 0 0 1 7 7l-9 9zM8.5 8.5l7 7" />,
  school: <path d="M2 9l10-5 10 5-10 5zM6 11v5c0 1.5 3 3 6 3s6-1.5 6-3v-5" />,
  hotel: <path d="M3 20V8M3 14h18v6M21 14v-2a3 3 0 0 0-3-3h-7v5M7 11.5h.01" />,
  place: (
    <>
      <circle cx="12" cy="12" r="3" />
      <circle cx="12" cy="12" r="8" />
    </>
  ),
};

export type IconName = keyof typeof PATHS;

export function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg className="pc-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
      {PATHS[name] ?? PATHS.place}
    </svg>
  );
}
