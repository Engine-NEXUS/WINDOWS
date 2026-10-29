# NEXUS frontend UI/UX surface audit (light + dark, consistency, window-opacity rule)

Scope: D:\ULTRON\frontend (read-only). Evidence = code read + counts by grep + a few dev-harness renders in the built-in browser (Vite dev server, since stopped; NEXUS app untouched). Anything not directly read or rendered is marked INFERENCE. No web sources were used; "Cited findings" cite file:line in the repo.

## 1. Inventory of surfaces (files, CSS, how each picks its theme)

### Takeaway
There are 8 Vite entries (setup, settings, sidebar, stage, companionHud, orbStudio, hand, githubDashboard; `frontend/vite.config.ts`) plus dev-only pages. Only 5 of them call `initThemeSync()`; the sidebar hosts 7 views in one window that use at least 6 different token systems; the newest surfaces (CodeRabbit, notify-expand, task chip, hand chip) are fixed dark.

### Cited findings
| Surface | Entry / files | CSS | Theme source |
|---|---|---|---|
| Stage: orb capsule | stage.html, `stage/OrbFrame.tsx` | `src/styles.css` (`.orb-capsule`, light override at :290-302) | `initThemeSync()` in `stage/main.tsx:16`; `:root[data-theme="light"]` selectors |
| Stage: captions | `stage/ResponseCaption.tsx`, `LiveCaption.tsx` | `stage/ghost.css` (0 theme selectors, 17 hex, 52 rgba) | none: fixed black "caption-black-capsule" (by design per AGENTS) |
| Stage: loading spinner | `stage/LoadingIndicator.tsx` | ghost.css / styles.css | none |
| Stage: background task chip | `stage/BackgroundTaskChip.tsx` | `stage/background-task.css` (40 lines, 0 vars, 0 theme selectors) | none, hard-coded `#151515` / `#f1f1f1` / `#2b2b30` (background-task.css:126-128) |
| Stage: notify expand | `stage/NotifyExpand.tsx` | `stage/notify-expand.css` (20 lines) | none, `#151515` `#f1f1f1` (:91-92) |
| Stage: tour overlay | `stage/TourOverlay.tsx` | `stage/tour.css` (16 light selectors, 64 var uses) | `:root[data-theme="light"]` |
| Stage: hand overlay | `stage/HandOverlay.tsx` | `stage/hand-overlay.css` (0 theme selectors, rgba amber/dark) | none |
| Stage: annotations / dev | `stage/annotations/*` | tour.css, inline CSS in `devHarness.tsx:304` (backdrop-filter blur(8px)) | `initThemeSync` in devHarness |
| Unified sidebar shell | sidebar.html, `sidebar/UnifiedSidebar.tsx` (hosts assistant, settings, architect, pr-list, spatial, annotate, stack views, :118-152) | `sidebar/unified.css` (`--hub-*` tokens, :234-257), sidebar.css, settings-sidebar.css, architect.css, pr-list.css | `initThemeSync()` lazily at UnifiedSidebar.tsx:86 |
| Assistant view (responses, CodeRabbit panel, event detail, markdown) | `sidebar/SidebarApp.tsx` | `sidebar/sidebar.css` (1947 lines, `--apple-*` tokens at :15-72), `sidebar/coderabbit.css` | `--apple-*` swap on `data-theme`; but CodeRabbit is fixed dark (see Q2) |
| Command Hub (pages: Audio, Display, Connections, Advanced, Memory, API keys, MCP, Actions & Permissions, Activity, Voice enrollment, Appearance) | `settings-sidebar/SettingsSidebarApp.tsx` + `hub/*.tsx` | `hub/hub.css` (2523 lines, `--hx-*`), settings-sidebar.css, mcpServers.css, activity-manager.css, voiceEnrollmentHub.css | `--hx-*` swap (hub.css:20/35), 95 light override blocks |
| Architect view | `architect/ArchitectApp.tsx` | architect.css (`--apple-*`, 19 theme selectors) | data-theme |
| PR list / GitHub dashboard | `pr-list/PrListApp.tsx`, github-dashboard.html | pr-list.css (`--gh-*`, class `.theme--light/.theme--dark` on the shell, :8/:32) | component prop, NOT `data-theme` |
| Stack / Spatial / Annotate views | `sidebar/StackView.tsx`, `SpatialDashboard.tsx`, `AnnotatePalette.tsx` | kit/kit.css (`--kit-*`), stack.css (`--stack-*`) | data-theme |
| Settings window (standalone, legacy) | settings.html, `settings/SettingsApp.tsx`, `settings/main.tsx` | settings.css (`--nx-*` via tokens.css, 14 light blocks) | `initThemeSync()` SettingsApp.tsx:113 |
| Setup wizard | setup.html, `setup/SetupApp.tsx` | setup.css (185 var uses, 0 light blocks, relies on `--nx-*`) | `initThemeSync()` SetupApp.tsx:28 |
| Orb studio | orb-studio.html (inline `<style>` with its own `--bg`/`--text`) | inline | no `initThemeSync` found in the entry (grep of src for the call lists only: companion-hud, settings, setup, sidebar, stage) INFERENCE: always dark/own palette |
| Companion HUD (calibration) | companion-hud.html, `companion-hud/main.tsx:5` | companion-hud.css (`--hud-*`, `--slider-*`; also `@media (prefers-color-scheme: light)` at :349) | initThemeSync + a second system-preference path |
| Hand camera window | hand.html, `hand/*` | hand/hand.css (`:root{color-scheme:dark}`, :1) | fixed dark (debug window, invisible 8x8 by default per AGENTS) |
| Design playgrounds / gallery | caption-showcase.html, pebble-showcase.html, caption-tuner.html (not shipped; vite.config.ts comment) | inline | n/a |

- `initThemeSync()` callers: companion-hud/main.tsx:5, settings/SettingsApp.tsx:113, setup/SetupApp.tsx:28, sidebar/UnifiedSidebar.tsx:86, stage/main.tsx:16 (grep). No call in orb-studio or hand.
- Dock-button implementations: shared `.sidebar-dock-btn` (unified.css:105) used by Architect (ArchitectApp.tsx:256-259), PR list (PrListApp.tsx:446-449), Annotate (AnnotatePalette.tsx:124-127), Spatial (SpatialDashboard.tsx:74-117); `IconButton label="Dock Left"` in StackView.tsx:254-310; a different `.hx-header-dock button` in the Command Hub (hub.css:116-145, SettingsSidebarApp.tsx:452-453); Hub also has full-width "◧ Dock Left"/"◨ Dock Right" buttons (SettingsSidebarApp.tsx:784-793). The Assistant view uses `useArrowDock` (SidebarApp.tsx:316) and `.sidebar-drag-strip` (coderabbit.css:62-63) with no dock buttons.

### Inferences
- A single sidebar window therefore mixes `--hub-*`, `--apple-*`, `--hx-*`, `--gh-*`, `--kit-*`, `--stack-*` and `--nx-*`; "theme parity" must be fixed per view family, not once.

### Gaps
- Orb-studio's theme behaviour was only inferred from grep; not rendered.
- Setup wizard, Architect, Hub sub-pages (Audio, Display, MCP, Memory), tour overlay and stage windows were not rendered (stage needs the Tauri runtime).

## 2. Theme architecture and hard-coded colours

### Takeaway
`theme/tokens.css` is a clean token file (dark default, `:root[data-theme="light"]` override) but is imported by only 6 CSS files; the larger surfaces define their own parallel palettes, and several surfaces ignore light mode entirely. `initThemeSync` is dark-by-default and not prefers-color-scheme aware.

### Cited findings
- tokens.css: dark = `:root` (:6), light = `:root[data-theme="light"]` (:48). Imported via `@import` by architect.css:6, settings.css:5, settings-sidebar.css:1, setup.css:6, sidebar.css:11 (grep). Other CSS files never import it.
- `theme.ts`: `resolveThemeMode` returns "light" only for the literal "light", else "dark" (theme.ts:14-16); initial value from `localStorage["nexus-theme-mode"]`, then `get_settings().themeMode`, then `settings:updated` and a Tauri `nexus:theme-change` broadcast (theme.ts:51-95). No `prefers-color-scheme` use in theme.ts. So a first run on a light-OS machine is dark. `useThemeMode` (useThemeMode.ts) only re-reads on the CustomEvent.
- Parallel token namespaces (count of `--x-` declarations): `--nx` 95 (tokens.css), `--hx` 34 (hub.css), `--apple` 70 (sidebar.css 40 + architect.css 30), `--gh` 58 (pr-list.css), `--kit` 36, `--hub` 18 (unified.css), `--stack` 14, `--tour` 12, `--hud/--slider` 22, `--liquid` 12.
- Same concept, different values: dark background `#0a0a0c` (nx-bg, hub-bg, apple-surface-base all agree) but card `#141418` (nx) vs `#15151b` (hub) vs `#15151c` (hx) vs `#121216` (apple sidebar); accent blue `#0a84ff` (hub/apple) vs `#ffffff` (`--nx-accent-blue`, tokens.css:21 - "blue" is white); light accent `#007aff`; GitHub view uses `#0d1117/#161b22` and accent `#2f81f7` / `#0969da` (pr-list.css:8-52).
- Per-file literal counts (hex / rgba( / var( uses / theme-selector lines), from grep:
  - hub.css 204 / 205 / 125 / 96; pr-list.css 112 / 54 / 93 / 0 (uses `.theme--light`); sidebar.css 106 / 98 / 152 / 2; architect.css 68 / 71 / 17 / 19; settings-sidebar.css 68 / 126 / 32 / 37; coderabbit.css 54 / 0 / **0** / 0; settings.css 41 / 68 / 180 / 14; kit.css 40 / 2 / 42 / 2; mcpServers.css 31 / 29 / 20 / 17; setup.css 27 / 12 / 185 / 0; tour.css 22 / 73 / 64 / 16; companion-hud.css 22 / 58 / 14 / 3; ghost.css 17 / 52 / 7 / **0**; hand.css 12 / 4 / 0 / 0; background-task.css 7 / 1 / **0** / 0; notify-expand.css 6 / 1 / **0** / 0; hand-overlay.css 1 / 12 / 0 / 0.
- Fixed-dark, no light handling (confirmed by render for CodeRabbit): coderabbit.css `.crp` background `#151515`, text `#ececec` (:8-9), borders `#2a2a2a`/`#262626`, links `#ff7a4d`; notify-expand.css `.nx-expand` `#151515`/`#f1f1f1` (:91-92); background-task.css `.nx-task-chip` `#151515`/`#f1f1f1` (:126-127), spinner `#3a3a42`/`#8ab4ff` (:140-141); hand-overlay.css `.hand-chip` `rgba(14,18,26,.92)` (:200); hand.css fixed dark (`#07090d`, :2).
- Render evidence: on /dev-coderabbit.html with `data-theme="light"` forced on `<html>`, the CodeRabbit panel remained fully dark (screenshot taken). NOTE the dev page does not call `initThemeSync`, so the page itself never switches; the CSS has no light selector either way (`coderabbit.css` has zero `data-theme` rules and zero `var(` uses).
- sidebar.css (Assistant view) has only the two token blocks as theme-aware parts (`[data-theme]` at :15 and :45) but 23 hard-coded white text colours, e.g. `.sidebar-topic-title` `#ffffff` (:240), `.annotate-title` (:477), `.nexus-markdown-body .nexus-h1` and `strong` white (:728, :779), `.sidebar-dock...` hover/box rgba(255,255,255,…) at :505, :547, :998, :1529-1536, :1678-1800, plus black scrims `rgba(0,0,0,.45/.6/.75/.88)` at :592, :978, :1042, :1294. In light mode `.sidebar-card` becomes `var(--apple-surface-base)` = `#f8f9fa` (sidebar.css:152, :45-70), so white headings/strong text land on near-white (contrast 1.05:1 for #fff on #f8f9fa). INFERENCE for the rendered Assistant view (sidebar.html shows an empty shell in the browser harness because the content arrives by Tauri events; not rendered).
- The stale header comment in sidebar.css:1-9 still describes "Liquid Glass / native DWM acrylic"; contradicts the current solid-window rule (stale doc, not a runtime defect).
- Light mode of the Command Hub (rendered by forcing `data-theme="light"` on /settings.html): the hub card, rows, text and dock buttons render correctly (dark text on white, white rows) apart from mid-transition frames. In the same render the dev toolbar card (`.nx-dev-toolbar-wide`, settings.css:88-100, `background: var(--nx-bg-card)` but fixed `rgba(255,255,255,.12)` border and light-on-dark text) turned white with its labels invisible. This toolbar is rendered only by `settings/main.tsx:105` (dev mode, "100% Fluid (Window Match)" preview strip), so it is a dev-harness defect, not a shipped window (INFERENCE from the main.tsx location; I did not read whether it is gated by `isTauri`).
- `/dev-stack.html` forced light: StackView rendered correctly (white cards, dark text, tinted chips); its dev toolbar stays dark and has a "Toggle theme" button.
- GitHub dashboard (pr-list): own theme classes `.theme--light/.theme--dark` chosen by the component, not by `data-theme`. Forced to light, the page rendered a white sheet; the `.sidebar-dock-btn` inside its header computes `color: rgb(255,255,255)`, `border: rgba(255,255,255,.15)`, background transparent (JS computed style) on a `#ffffff` header (`--gh-header-bg` light, pr-list.css:21) => glyph and border effectively invisible in light.
- Contrast of the main token pairs (computed with WCAG 2.x relative luminance, alpha pre-blended on the bg):
  - Dark `--nx-text-primary #fff` on `#0a0a0c` 19.8:1, on card `#141418` 18.4:1; secondary (72% white) 10.2 / 9.8; tertiary (45% white) 4.48 on bg (just under 4.5) / 4.53 on card.
  - Light primary `#111827` on `#f8f9fa` 16.8:1, on white 17.7:1; secondary (68% black) 7.7 / 7.8; tertiary (45% black) **3.32 / 3.36 (fails AA 4.5 for body text)**.
  - Hub tokens: dark `--hub-text-faint` (40% white) on `#0a0a0c` 3.77 (fails AA for small text); light `--hub-text-faint` (40% black) on `#f8f9fa` **2.84** (fails); dim text passes (9.3 dark, 7.0 light).
  - `--hub-accent` `#0a84ff` on `#15151b` 4.98 (pass); light `#007aff` on white 4.02 (below 4.5 for text; fine for large/UI).
  - Light semantic colours on white: warning `#d97706` 3.19, success `#059669` 3.77, error `#dc2626` 4.83 (warning/success below 4.5 as text).
  - Fixed-dark panels: CodeRabbit muted `#8a8a8a` on `#151515` 5.29; locked nav row `#6c6c6c` 3.48 (fails); `#ff7a4d` links 7.08; task-chip time `#9aa0aa` 6.94; `.crp-pill--not_reviewed #9a9a9a` 6.49.
  - Dock button `#fff` on light `#f8f9fa`: 1.05:1 (see Q3).
- Borders: `--nx-border` computes to 1.33:1 (dark) and 1.25:1 (light) against the bg; fine as decorative hairlines but they are the only boundary of cards in light mode (WCAG 1.4.11 asks 3:1 for UI component boundaries; INFERENCE that inputs relying only on these borders are low-visibility).
- `prefers-color-scheme` is used only in: companion-hud.css:349 (`@media (prefers-color-scheme: light)` which sets slider tokens from the OS setting, independent of the app's `data-theme`, so the HUD can disagree with the Hub setting), architect.css:275 comment (React Flow "system" colour mode), and `@media (prefers-reduced-motion)` in tokens.css. There is also a `color-scheme` declaration only in hand.css:1 (grep of `color-scheme` not run over all files; INFERENCE that form controls/scrollbars do not follow the theme elsewhere).

### Inferences
- The recent additions (coderabbit.css, notify-expand.css, background-task.css, hand-overlay.css) all share one pattern: standalone hex palette, no `var()`, no light branch, written after the "solid" decision. They look like a different product from the Hub/Stack views that do respect the theme.
- NotifyExpand and the task chip live on the transparent stage over arbitrary desktop content (like the black orb capsule), so a fixed dark pill is defensible there; but NotifyExpand "grows into the sidebar rectangle" (notify-expand.css:87), which in light mode is a white sidebar, so the hand-off will pop from dark to light. CodeRabbit is inside the sidebar and has no such excuse.
- Dark-default plus a `localStorage`-first read means a hub switch to Light is only applied in other windows once the Tauri broadcast/`get_settings` returns; a short dark flash on open in light mode is plausible (not rendered).

### Gaps
- Contrast numbers cover token pairs only; per-component pairs inside hub.css (204 hex literals) were not exhaustively computed.
- Whether each Hub sub-page (Audio/Display/MCP/Memory/API keys/Actions) has a light variant for every element: 95 light override blocks exist in hub.css but their coverage was not rendered.

## 3. Dock buttons, chrome, states, accessibility, scale consistency

### Takeaway
At least five distinct dock-button implementations coexist; the shared `.sidebar-dock-btn` is white-on-transparent and breaks in light mode, all are 28 px (below the 44 px touch guideline but at/above the 24 px WCAG 2.2 AA minimum), and keyboard focus styling is almost absent.

### Cited findings
- `.sidebar-dock-btn`: 28x28, radius 9px, border `rgba(255,255,255,.15)`, `color:#ffffff`, transparent background, hover `rgba(255,255,255,.08)` (unified.css:105-127). Hub dock: 28x28, radius 8px, tokenised colours `var(--hx-*)` with `box-shadow 0 1px 2px` (hub.css:123-145). Different radii (9 vs 8) and different theming between the two.
- Dock glyphs are the text characters ◧ / ◨ (SettingsSidebarApp.tsx:452-453, 784, 793; SpatialDashboard.tsx:78,85,113); StackView uses `IconButton` with `label`. Accessibility labels: dock buttons in Architect/PR list/Annotate/Hub have only `title="Dock Left"` (ArchitectApp.tsx:256-259 etc.), no `aria-label`; `title` is not reliably exposed as an accessible name for a glyph-only button. `IconButton` (StackView, Spatial) takes `label`; I did not open IconButton to confirm it sets aria-label (INFERENCE).
- `aria-label` occurs in 81 places across tsx; `<button` appears in 40 tsx files (grep counts; not a per-button audit).
- Focus rings: `:focus-visible` appears only in mcpServers.css:277-278 and lens.css:14,66. `outline: none/0` is declared in 14 CSS files (30 occurrences): architect, companion-hud, pr-list, settings, hub, settings-sidebar, setup, coderabbit, kit, lens, sidebar, stack, ghost, liquid-glass. So most inputs/buttons lose the default focus ring with no replacement (INFERENCE; some have `:focus` border styles, not enumerated). `--apple-focus-ring #fa586a` and `--nx-border-focus` tokens exist (sidebar.css:41, tokens.css:19) but `:focus-visible` is not applied to buttons generally.
- Scrollbars: unified.css:259-270 hides every scrollbar globally with `!important` (`scrollbar-width:none`, `::-webkit-scrollbar{display:none}`), affecting all views in the unified sidebar; pr-list.css:164-171 defines its own scrollbar styling that is therefore overridden. Consequence: no visible scroll affordance, and no thumb for mouse-only users (INFERENCE on usability; hub has `.hx-scroll`).
- Radii used (count of declarations): 8px x54, 6px x36, 10px x35, 12px x29, 4px x24, 9999px x16, 14px x15, 2px x11, 999px x9, 18px x8, 5px, 3px, 9px, 11px. Tokens define 6/10/10/12/full (tokens.css:112-116) but the project note says "blunt 10px"; `.sidebar-card` is 18px (sidebar.css:154), cards 12-14px; dock buttons 8-9px.
- Type scale: sizes in use include 8.5, 9, 9.5, 10, 10.5, 11, 11.5, 12, 12.5, 13, 13.5, 14, 14.5, 15, 16, 16.5, 17, 18, 19, 20, 21, 22, 24, 27, 30, 32 px (grep). Token scale is 11/13/14/15/18/24/32 (tokens.css:84-90); half-pixel sizes are numerous (38 declarations at 10px, 15 at 10.5px, 25 at 11.5px, 15 at 12.5px, 13 at 13.5px). Sub-11px text exists (9-10.5px x ~64).
- Font stacks: `--nx-font` (9 uses) vs ad-hoc stacks (SF Pro/-apple-system variants ~8, plain `monospace` x10, `"Segoe UI", system-ui` in the newest files) (grep).
- Spacing scale: `--nx-space-*` defined (4/8/12/16/20/24/32/40) but background-task.css, notify-expand.css, coderabbit.css use literal paddings (7px 12px, 16px 18px, 14px 16px 20px, 9px 12px) with no tokens.
- Hit targets: dock 28 px; CodeRabbit nav rows pad 9px 12px (~34 px high, coderabbit.css:22); `.crp-btn` pad 5px 12px at 12px font (~28 px, :74); `.crp-chip` 3px 10px (~24 px, :67); `.crp-back` 4px 0 padding (~25 px, :35).
- Hover/active: dock `:active { transform: scale(.94) }` (unified.css:126), Hub `scale(.92)` (hub.css:143): inconsistent values; `.crp-*` hover only changes background (#202020).
- Drag strip: `.sidebar-drag-strip` is an invisible 14 px strip at the top of `.sidebar-card` (coderabbit.css:62-63); no title bar or dock buttons on the Assistant/CodeRabbit view (per its comment); docking there is by arrow keys (`useArrowDock`, SidebarApp.tsx:316). Discoverability is nil unless documented (INFERENCE).
- Hand/Companion HUD: companion HUD has `.hud--light {}` empty rule and a comment "reserved for the pill-level luminance theme (plan 03, deferred)" (companion-hud.css:352-354).

### Inferences
- The light-mode dock-button failure is real and affects Architect, PR list/GitHub dashboard, Annotate and (via the shared class) Spatial; the Hub's dock buttons are themed and fine.
- Keyboard users have no consistent focus indication in the sidebar family; with scrollbars hidden too, the sidebar is mostly pointer-only (INFERENCE from absent CSS; behaviour of the browser default ring on `<button>` after `outline:none` rules was not tested per element).

### Gaps
- Per-element tab order and `IconButton` implementation not read.
- Exact keyboard shortcuts exposed (Esc, arrows) beyond UnifiedSidebar's global Esc handler (UnifiedSidebar.tsx:92-105, hides the sidebar and stops TTS from any nested input, including text fields) not reviewed for conflicts.

## 4. Window-opacity rule vs glass / backdrop-filter

### Takeaway
The repo gate `scripts/check-window-opacity.mjs` passes ("13 CSS files in registry, 0 violations", run once, read-only), but it only checks registered panel selectors; translucent/backdrop surfaces remain on the stage overlay and in a few places in panel CSS.

### Cited findings
- Registry (scripts/window-opacity-registry.json) covers sidebar.css (`.sidebar-card`, `.spatial-card`, `.spatial-dashboard`, `.analysis-card`), notify-expand.css (`.nx-expand`), stack.css, unified.css, kit.css, coderabbit.css (`.crp`), lens.css, architect.css, settings.css, settings-sidebar.css, hub.css (`.hx-card`), pr-list.css (`.gh-dashboard-shell`), companion-hud.css (`.hud`). Not registered: `.nx-task-chip` / `.nx-task-stack` (background-task.css), `.hand-chip`, `#ghost-guide-bubble`, `.caption-*`.
- `backdrop-filter` occurrences: ghost.css:91-92 (`#ghost-guide-bubble`, `blur(12px)` with `rgba(15,23,42,.85)`), ghost.css:303 (`blur(6px)` on a translucent `rgba(10,14,22,.55)` annotation text), ghost.css:402-403 and 412-413 and styles.css:82-83 (`backdrop-filter: none !important` explicitly disabling on captions/capsule), stage/annotations/devHarness.tsx:304 (dev). sidebar.css has one mention (comment). No backdrop-filter in hub/settings/architect/pr-list/setup CSS.
- Translucent fills on panel-like surfaces: `.hand-chip` `rgba(14,18,26,.92)` / live `rgba(160,30,30,.95)` (hand-overlay.css:200,205); ghost-guide-bubble (above); `.sidebar-card::after` and liquid-glass.css (110 lines; imported by sidebar.css:12) still ship glass tokens (`--lg-sheen-opacity: 0`, `--lg-rim-width: 0px` set at sidebar.css:38-39, 68-69 to neutralise them); tokens.css `--nx-glass-*` names remain with `--nx-glass-blur: none`.
- `html, body, #root { background: transparent !important }` in sidebar.css:87-90 is bundled into the unified sidebar; the solid look depends on `.unified-shell` painting `var(--hub-bg)` (unified.css:277). If a view ever renders outside `.unified-shell` it is transparent (INFERENCE).
- Tour/annotation overlays (tour.css, 73 rgba) are by nature translucent canvas overlays on the stage, not panel windows; not counted as violations here.

### Inferences
- Strictly, `#ghost-guide-bubble`, `.hand-chip` and the lens/ann text over screen content are the remaining glass-like items; they live on the click-through stage, which the rule's registry does not target.

### Gaps
- The rule text in AGENTS ("solid windows", guard `check-window-opacity.mjs`) was read from the supplied notes, not from the script source.

## 5. Cross-cutting inconsistencies (summary list)

1. Seven-plus token systems (Q2) with differing card colours and accents; only `--nx-*` is documented as "the" token file.
2. Theme selector mechanisms differ: `:root[data-theme]` (nx, hub, tour), bare `[data-theme]` (apple, kit, stack, architect), `.theme--light` class (pr-list), `@media (prefers-color-scheme)` (companion HUD), none (CodeRabbit, task chip, notify, hand chip, ghost, orb-studio, hand window).
3. Dark-default theme resolution ignores the OS preference (theme.ts:14).
4. Dock buttons: 5 implementations; `.sidebar-dock-btn` is white-only (unified.css:105-127) and invisible in light (Architect, PR list, Annotate, Spatial).
5. Radius, font-size and spacing scales are not enforced (Q3 counts); newest CSS ignores tokens.
6. Focus indication and aria names missing on glyph buttons; global scrollbar hiding.
7. Light-mode text contrast: `--nx-text-tertiary` 3.3:1 and `--hub-text-faint` 2.8:1 on light backgrounds; warning/success text colours <4.5:1 in light.
8. Stale or misleading comments: sidebar.css header ("Liquid Glass / DWM acrylic"), tokens.css header says "Pitch Black (#000000)" but dark bg is `#0a0a0c`.

## 6. Per-surface verdict (dark / light)

| Surface | Dark | Light | Main defects (file:line) |
|---|---|---|---|
| Stage orb capsule + captions | OK | OK (capsule has override; captions intentionally black) | n/a |
| Stage loading spinner | OK (not read in detail) | not verified | ghost.css has no theme rules |
| Background task chip | OK | Fixed dark (acceptable on stage, unthemed) | background-task.css:126-141 hard-coded |
| Notify expand | OK | Fixed dark; hands off to a light sidebar | notify-expand.css:91-92 |
| Tour overlay | OK | Has light branch (16 selectors) | not rendered |
| Hand overlay chip | OK | Fixed dark | hand-overlay.css:200-205 translucent |
| Assistant view (responses/markdown) | OK | LIKELY BROKEN: white text on `#f8f9fa` (INFERENCE) | sidebar.css:240, 477, 728, 779 etc. |
| CodeRabbit panel + event detail | OK | BROKEN/ignored: stays dark inside light sidebar (rendered) | coderabbit.css:8-9 and every colour line, 0 var() |
| Command Hub (home, pages) | OK (rendered) | OK on hub home (rendered) | tertiary/faint contrast 2.8-3.4:1 |
| Hub MCP / memory / API keys / actions pages | not rendered | not rendered (17-96 light blocks present) | unverified |
| Architect view | not rendered | has light branch (19 selectors) | dock button invisible in light (ArchitectApp.tsx:256) |
| PR list / GitHub dashboard | OK (rendered) | White sheet OK, dock buttons invisible (computed #fff) | unified.css:105-127; separate `.theme--*` mechanism |
| Stack / Spatial / Annotate | Stack OK | Stack OK (rendered) | Spatial/Annotate use the white dock button |
| Settings window (legacy) | OK | Page OK, dev toolbar invisible text (dev only) | settings.css:88-100, main.tsx:105 |
| Setup wizard | not rendered | uses `--nx-*` only, no light overrides (0 blocks) | setup.css |
| Orb studio | inline palette, not rendered | no theme sync found | orb-studio.html |
| Companion HUD | OK | split: `data-theme` + `prefers-color-scheme` | companion-hud.css:349, `.hud--light {}` empty |
| Hand camera window | fixed dark (debug) | fixed dark | hand.css:1-2 |

## Housekeeping
- Dev server started with `npm run dev` in D:\ULTRON\frontend, used for screenshots of /dev-coderabbit.html, /settings.html, /dev-stack.html, /github-dashboard.html (light/dark by forcing `data-theme` on `<html>` because the dev pages mock Tauri), then killed (port 5173 listeners stopped). Browser viewport emulation reset to desktop. NEXUS app not started, stopped or rebuilt. No source file edited. One-time read-only run of `node scripts/check-window-opacity.mjs`.
- Caveat on screenshots: several light-mode frames were captured during the 150-200 ms colour transitions, so grey intermediate fills in them were discarded as artefacts; conclusions rely on computed styles and CSS reading, not on those fills.
