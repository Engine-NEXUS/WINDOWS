# 174 - UI plan, Phase 2: one dock control, visible focus, themed scrollbars, 24 px targets

Date: 2026-10-29. Phase 2 of [`docs/research/ram-and-ui-research/reports/RAM usage and UI UX plan.md`](../research/ram-and-ui-research/reports/RAM%20usage%20and%20UI%20UX%20plan.md),
on top of Phase 1 ([173](173-ui-phase-1-tokens-and-theme-switching.md)). Source and tests only; the app was not rebuilt or run.

## What the audit found

- Five separate dock-button implementations (Architect, PR list, Annotate, Hub header, Stack/Spatial kit buttons), each with its own
  markup, text glyphs (`◧ ◨`, which depend on the installed font) and, in the Hub, its own CSS.
- The shared `.sidebar-dock-btn` was white on transparent: 1.05:1 on a light header, so the dock buttons of Architect, PR list,
  Annotate and Spatial were invisible in light mode (confirmed on a render).
- 28 px buttons with only a `title`, no accessible name; `outline: none` in 30 places with no keyboard focus ring; every scrollbar
  hidden with `!important`; several controls under the 24 px target size.

## What changed

| Area | Change | Where |
|---|---|---|
| One dock control | `DockButton`, `DockButtons` (the left/right pair) and `DockIcon`: SVG icons that follow the text colour, `aria-label` ("Dock to the left/right"), grouped with `role="group"`. Replaced the duplicated markup in Architect, PR list, Annotate, the Hub header and the Stack/Spatial kit buttons. | `frontend/src/sidebar/kit/DockButtons.tsx` (+ the six views) |
| One style | `.sidebar-dock-btn` now reads the theme tokens (card surface, text, subtle border, hover surface), is 32 px, has a press scale; the Hub's own copy of the style was removed. Visible in light and dark. | `sidebar/unified.css`, `settings-sidebar/hub/hub.css` |
| Header row | `.sidebar-header-row` has a 44 px minimum height; dock controls and header gaps are 8 px. | `sidebar/sidebar.css` |
| Focus | A global 2 px keyboard focus ring (`--ui-focus-ring`, 3:1, checked by the contrast gate) for buttons, links, tabs, switches, radios, checkboxes, summaries and selects; it overrides the old `outline: none` rules and only shows for keyboard focus. Text inputs keep their own focus style. | `theme/tokens.css` |
| Scrollbars | The blanket "hide every scrollbar" rule is replaced by thin themed scrollbars, so a scrolling list shows that it scrolls. | `sidebar/unified.css` |
| Hit targets | 24 px minimum for the small controls found by the audit: Stack hero link, hosts toggle, technology chips, dropdown chips (26 px), PR sign-in PAT toggle, Hub "+ Add" links. | `stack.css`, `kit/kit.css`, `pr-list.css`, `hub.css` |

## Checks run

- Frontend: TypeScript clean, 761 tests (80 files) pass, production build with no warning lines, all ten frontend CI checks pass.
- New tests: dock buttons render an accessible name, one shared class, SVG icons (no glyph characters), the right pane filled per side, and report the side on click. The old test that looked for the glyph characters was updated.
- Dev browser (Vite dev pages, not the real app), dark and light with transitions disabled:
  - Dock button on Spatial, Stack, GitHub dashboard and Hub: 32 x 32, accessible name, dark text on a white surface in light and white on dark in dark. Before, the PR dashboard computed `rgb(255,255,255)` on a white header.
  - Pressing Tab on the Hub page shows a 2 px solid ring (`#1d4ed8` in light).
  - An audit of every `button`, role control, link, select and input (size at least 24 px and an accessible name) found only the controls fixed above, the dev-only toolbar, and two inline links in the sign-in sentence (inline links are exempt in WCAG 2.5.8).

## Not done / open

- **Assistant view dock control: decided, keep none.** The report proposed a visible dock control there; the owner chose to keep that view without dock buttons (drag the strip, or arrow keys). No change made.
- The Phase 2 plan's Playwright check is not set up (Playwright is not a dependency); the audit above was run once by hand in the dev browser. The automated both-theme screenshot and size/name checks belong to Phase 6.
- The remaining `outline: none` rules are not deleted; the global ring overrides them for keyboard focus. Removing them is Phase 3 clean-up per view.
- The real windows (Hub in the app, Architect, PR list, Annotate) were not rendered; only the dev pages were.
