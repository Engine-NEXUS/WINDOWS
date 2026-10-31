# Design system best practice for a Tauri/WebView2 desktop assistant (light + dark)

Scope note: about 17 tool calls, so coverage is partial. Items marked UNVERIFIED come from background knowledge, not from a page I read this session. Fetches of m3.material.io, m2.material.io, Apple HIG and Primer returned empty or title-only content, so no Material 3, Apple or Primer numbers are cited from those sites.

## Token architecture and theme switching

### Takeaway
Use three tiers (primitive, semantic, component), define every semantic role once with a light and a dark value, and switch themes by setting one attribute plus `color-scheme` on `:root`. The DTCG format reached a stable 2025.10 release with a Resolver module for light/dark contexts, and Radix's 12-step scale is a ready-made model for role-to-step mapping.

### Cited Findings
- The Design Tokens Community Group released its first stable spec (2025.10) on 2025-10-28, in three modules: Format, Color and Resolver. The Color module supports Display P3, Oklch and all CSS Color 4 spaces. — [DTCG announcement](https://www.w3.org/community/design-tokens/2025/10/28/design-tokens-specification-reaches-first-stable-version/)
- The Resolver module is the mechanism for context-dependent tokens (theme, size, accessibility mode) without duplicating token files. — [DTCG announcement](https://www.w3.org/community/design-tokens/2025/10/28/design-tokens-specification-reaches-first-stable-version/)
- The spec is stable per the DTCG, but it is "not a W3C Standard nor on the W3C Standards Track". — [Format Module 2025.10](https://www.w3.org/community/reports/design-tokens/CG-FINAL-format-20251028/)
- Radix Colors 12-step scale, by step:
  - 1 app backdrop
  - 2 subtle background (cards, sidebars)
  - 3 normal component background
  - 4 hover
  - 5 pressed/selected
  - 6 subtle non-interactive borders
  - 7 subtle interactive borders
  - 8 stronger borders and focus rings
  - 9 solid backgrounds
  - 10 solid hover
  - 11 low-contrast text
  - 12 high-contrast text
  — [Radix: understanding the scale](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale)
- Radix guarantees APCA Lc 60 for step 11 text and Lc 90 for step 12 text on a step 2 background from the same scale. — [Radix](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale)
- Fluent 2 organises colour tokens into alias groups (for example `colorNeutralBackground1`). Each alias has separate light and dark global values and rest/hover/pressed/selected states. — [Fluent 2 color tokens](https://fluent2.microsoft.design/color-tokens) (search snippet only; the page was not fetched)
- In Fluent web components, one `baseLayerLuminance` value flips the neutral layer recipes, and nested containers step from `neutralLayer1` to `neutralLayer2`. — [Fluent UI web components styling](https://learn.microsoft.com/en-us/fluent-ui/web-components/getting-started/styling)
- `color-scheme: light dark` on `:root` makes the browser adjust canvas, scrollbars, form controls and system colours. It does not restyle your own elements, which still need `prefers-color-scheme` or `light-dark()`. — [MDN color-scheme](https://developer.mozilla.org/en-US/docs/Web/CSS/color-scheme)
- A `<meta name="color-scheme" content="light dark">` placed before the stylesheets helps avoid a flash of the wrong colours during load. — [MDN color-scheme](https://developer.mozilla.org/en-US/docs/Web/CSS/color-scheme)
- `light-dark(a, b)` picks a value from the used scheme, and it only resolves to dark if `color-scheme` is declared. — [MDN color-scheme](https://developer.mozilla.org/en-US/docs/Web/CSS/color-scheme)
- WebView2 `ICoreWebView2Profile.PreferredColorScheme` sets the `prefers-color-scheme` value the page sees, and also the colours of WebView2 dialogs and context menus. The default is AUTO (follow the OS). — [WebView2 ICoreWebView2Profile](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2profile)
- Windows apps default to the user's OS theme, and an app may override it. — [Color in Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)

### Inferences
- Recommended tiers:
  - Primitive: `--ref-neutral-1..12`, `--ref-accent-1..12`, plus status ramps.
  - Semantic: `--surface-app`, `--surface-panel`, `--surface-card`, `--surface-raised`, `--text-primary`, `--text-secondary`, `--border-subtle`, `--border-strong`, `--accent`, `--focus-ring`, `--danger`, `--warning`, `--success`.
  - Component: `--dock-btn-bg-hover` and similar, aliasing semantics only.
- Components should never reference primitives. Only the semantic layer changes between themes.
- Theme-switch mechanics, in the order they should be applied:
  1. Put `color-scheme: light dark` and the meta tag in each window's HTML.
  2. Define `:root{--x:light}` and `:root[data-theme="dark"]{--x:dark}`, plus `@media (prefers-color-scheme: dark){ :root:not([data-theme]) {...} }` for "system".
  3. Resolve and set `data-theme` and `style.colorScheme` in a tiny blocking inline script in `<head>`. This avoids a flash.
  4. In Tauri, give each window a matching native background colour to hide the pre-paint frame (UNVERIFIED for Tauri specifics).
- Because "follow system" is WebView2's default, `prefers-color-scheme` already tracks the OS. If the app has its own Light/Dark/System setting, `data-theme` must win and the native profile scheme should be set to match so native dialogs agree. Multi-window apps need the choice broadcast across windows (Tauri events or the `storage` event).
- The DTCG Resolver fits a "theme = light | dark | high-contrast" matrix. Build with Style Dictionary or Terrazzo (UNVERIFIED which currently supports 2025.10).

### Gaps
- Material 3 colour-role names and tonal values: m3.material.io returned no content, so I cannot cite them.
- GitHub Primer's theming approach: not fetched.
- A verified Tauri recipe for a flash-free first paint: not found.
- Fluent 2 dark-layer hex values: not retrieved.

## Dark-theme specifics, materials, and solid panels

### Takeaway
Dark surfaces should get lighter as they get more important, avoid pure black and white pairs, and keep accents softer. Windows reserves Acrylic for transient surfaces and uses Mica as an opaque base, so solid panels are consistent with Windows guidance.

### Cited Findings
- In both light and dark modes on Windows, darker colours mark less important background surfaces and lighter, brighter colours mark important ones. — [Color in Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Windows accent colours are generated and optimised for contrast in both modes, and should be used sparingly. — [Color in Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Windows names ambient glare as a risk: a black page can be unreadable outdoors, and a white page painful in a dark room. Red/green colour blindness affects about 8% of men and 0.5% of women, so colour must not be the sole differentiator. — [Color in Windows](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/color)
- Material 2 dark theme (secondary source): base surface #121212; elevation is shown with white overlays from 0% to 16%, with 5% at 1dp; WCAG 4.5:1 should hold on all elevation surfaces. Material 3 replaced overlays with a surface tint. — [Material 2 dark theme (search summary)](https://m2.material.io/design/color/dark-theme)
- Windows materials:
  - Acrylic is semi-transparent frosted glass, "used only for transient, light-dismiss surfaces such as flyouts and context menus".
  - Mica is an opaque material tinted with the user's wallpaper, mode-aware, with built-in active/inactive window states.
  - Smoke is translucent black in both modes and dims content behind a modal.
  — [Materials in Windows apps](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)
- WCAG thresholds: text 4.5:1 (large text 3:1) per 1.4.3, and 3:1 for UI components and states per 1.4.11. — [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- APCA is the alternative model: Radix targets Lc 60 for low-contrast text and Lc 90 for high-contrast text. — [Radix](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale)

### Inferences
- Concrete dark neutral recipe: use app background about L 12–15% (for example #141414 to #1b1b1f, not #000), and step each layer up about 3–5 L points (panel, card, raised). Body text at about #e6e6e6 to #f0f0f0 instead of #fff (UNVERIFIED as numbers; consistent with the Material 2 #121212 base). Avoid #000 with #fff, since halation and glare are harsher.
- For accents in dark mode, shift to a lighter, slightly less saturated tone (roughly Radix steps 9–11 of the dark scale) and check 4.5:1 against surface-card. The "tone 200" rule is secondary-sourced and unconfirmed.
- Shadows barely read on dark surfaces, so use lightness steps plus a 1px subtle border (Radix step 6) for separation.
- The app's mandate for solid windows aligns with Windows guidance. Mica needs an HWND backdrop that is outside WebView2's content and is not reliably honoured when the window is non-activating. This app's own earlier finding was that DWM materials fall back to solid on inactive windows (project AGENTS.md; Windows docs describe Mica's inactive state, not this exact failure). Solid is the safer choice for an always-on overlay, and Smoke-style dimming (translucent black) is only for modals.

### Gaps
- Primary Material 3 or Apple HIG dark-theme pages: not retrievable here.
- The 15.8:1 figure sometimes cited for #121212 and white was not confirmed.

## Components and numeric guidance

### Takeaway
Pointer targets must be at least 24x24 CSS px (WCAG AA); Windows suggests about 40x40 for touch-capable targets. Use a 4 px base grid, 100–300 ms motion, and a focus ring of at least 2 px with 3:1 contrast.

### Cited Findings
- WCAG 2.5.8 (AA): pointer targets at least 24x24 CSS px. Exceptions:
  - A smaller target passes if a 24 px circle centred on it does not overlap another target.
  - An equivalent control exists elsewhere on the page.
  - The target is inline in a sentence.
  - The browser controls its size.
  - A particular presentation is essential.
  — [Understanding 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
- 2.5.5 Enhanced is the stricter option for important controls. The page I fetched gave no numeric value; the 44x44 figure is from background knowledge (UNVERIFIED here). — [Understanding 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html)
- Windows: touch targets should be about 7.5 mm square, which is 40x40 px at 135 PPI and 1.0x scale. Adjust bigger for frequent touches or severe error consequences. — [Windows targeting](https://learn.microsoft.com/en-us/windows/apps/develop/input/guidelines-for-targeting)
- WCAG 2.4.11 Focus Not Obscured (Minimum, AA): the focused component must not be entirely hidden by author-created content. — [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- WCAG 2.4.13 Focus Appearance is Level AAA. It requires a focus indicator at least as large as a 2 CSS px perimeter of the unfocused component, with 3:1 contrast between focused and unfocused pixels. — [WCAG 2.2](https://www.w3.org/TR/WCAG22/)
- Windows notification guidance:
  - Notifications should be informative, not noisy.
  - Buttons trigger immediate actions.
  - Progress bars show long-running operations.
  - Suppress alerts into Notification Center when interruption is not warranted.
  - Respect Windows 11 Focus Sessions via `FocusSessionManager`.
  - Clear old notifications once the user has seen the content elsewhere.
  — [App notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)

### Inferences (my proposed specs, not sourced numbers unless stated)
- Spacing scale: 4, 8, 12, 16, 24, 32 (4 px base grid, UNVERIFIED as a standard but ubiquitous).
- Radii: 4 for controls, 8 for cards, 12 for panels/toasts, full for chips and pills.
- Type: 12 / 13 / 14 / 16 / 20 px; 14 px body for dense desktop UI; line-height 1.4–1.5 (UNVERIFIED).
- Motion: 100–150 ms for hover, press and toggle; 200–250 ms for panel and toast enter; exit about 70% of enter; ease-out in, ease-in out. This range matches the brief's 100–300 ms; I did not verify a source for exact values.
- Dock/pin button for a side panel: 32x32 visual, 24 px glyph box (16 px icon), minimum hit area 32x32 (exceeds the 24 px AA floor; 40 if the panel is touch-used). States: rest, hover (step 4), pressed (step 5), `aria-pressed` for pin toggle, and a selected state that is not colour-only (use a filled icon variant). Tooltip after about 500 ms hover or on focus, with the shortcut shown. Icon-only buttons need `aria-label`.
- Header row: 40–48 px height, title left, dock/close right with 8 px gaps (keeps adjacent 32 px targets separated).
- Segmented control: 28–32 px tall, items at least 24 px wide, `role="radiogroup"` with arrow-key navigation (roving tabindex).
- Switch: about 40x20 track, a 32 px or taller row hit area, `role="switch"`, with the label as the accessible name.
- Chips/status pills: 20–24 px tall, text 12 px, always an icon or text word in addition to colour (colour-blindness note above); 4.5:1 text on the pill background.
- Cards: surface-card, 1px border-subtle, 8 px radius, 16 px padding.
- Empty, loading and error states: each has a title, one sentence of cause or next step, and one primary action. Loading uses skeletons for lists over about 300 ms and a spinner otherwise (UNVERIFIED thresholds). Errors use text plus an icon, not colour alone.
- Background-task indicators: a determinate bar when progress is known, an indeterminate bar otherwise. Expose with `role="progressbar"` and `aria-valuenow`, or `aria-busy` for indeterminate. Completion transitions to a toast or chip, and the chip is dismissible.
- Toasts: a title of 1 short line, body of at most 2 lines, 0–2 actions, auto-dismiss about 5–8 s unless an action is needed, announced via `role="status"` (polite) or `role="alert"` for errors. Pause the timer on hover or focus (WCAG 2.2.1 time adjustable; UNVERIFIED mapping).

### Gaps
- Microsoft's stated numeric limits for toast text length: the fetched page didn't give them.
- Fluent 2 per-component specs (sizes, radii, motion tokens): not fetched.
- Apple HIG numbers: not retrieved.

## Accessibility: reduced motion, forced colors, keyboard, screen readers

### Takeaway
Forced-colors mode strips shadows, background images and colour choices, so state cues must survive as borders, outlines and system colours. Honour reduced motion with a media query, and give every control a name and a visible focus ring.

### Cited Findings
- `forced-colors: active` is on when the OS enables, for example, Windows High Contrast. The browser then also selects a matching `prefers-color-scheme` from the lightness of `Canvas`. — [MDN forced-colors](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors)
- In forced colours, `color`, `background-color`, `border-color`, `outline-color`, `text-decoration-color` and SVG `fill`/`stroke` are overridden; `box-shadow` and `text-shadow` become `none`; non-url `background-image` becomes `none`; `color-scheme` becomes `light dark`. — [MDN forced-colors](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors)
- The browser picks system colours from native element semantics, not ARIA roles, so `role="button"` on a `div` is not given `ButtonText`. — [MDN forced-colors](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors)
- Authors can use system colours (`Canvas`, `CanvasText`, `ButtonText`) directly on properties that are not forced, and `forced-color-adjust: none` opts an element out. MDN advises small fixes, not a separate design, for example replacing a box-shadow with a `2px ButtonText` border. — [MDN forced-colors](https://developer.mozilla.org/en-US/docs/Web/CSS/@media/forced-colors)

### Inferences
- Rules to apply:
  - Use real `<button>`, `<input>` and similar elements where possible.
  - Never rely on shadows or background tints alone for selected or focused state; add a border or outline (which forced mode keeps).
  - Keep focus rings as `outline`, not `box-shadow`.
  - For custom icons use `currentColor` so they follow forced colours.
- Reduced motion: wrap non-essential transitions and animations in `@media (prefers-reduced-motion: reduce)` and replace movement with opacity or an instant change (the media feature name is standard CSS; no page fetched this session).
- Keyboard: logical DOM order equals focus order, roving tabindex inside segmented controls and toolbars, Esc closes panels and returns focus to the trigger, and an always-on overlay must not trap focus. Pair with WCAG 2.4.11 so toasts or the overlay do not cover the focused element.
- Screen readers: `aria-label` on icon-only buttons, `aria-pressed` for toggles, `aria-live` regions for toasts and task status.

### Gaps
- No source fetched for `prefers-reduced-motion` or ARIA patterns; these rely on standard knowledge.
- Windows-specific behaviour of Narrator with WebView2 overlays: not researched.

## Verification practice

### Takeaway
Run screenshots in both colour schemes (plus forced colours and reduced motion), add automated contrast checks, and lint out raw colours.

### Cited Findings
- Playwright accepts `colorScheme` with `'light'`, `'dark'` or `'no-preference'` through config (`use: { colorScheme: 'dark' }`), `test.use`, or context/page options; `page.emulateMedia({ colorScheme: 'dark' })` changes it mid-test. — [Playwright emulation](https://playwright.dev/docs/emulation)
- Playwright's emulation page I fetched did not document `reducedMotion` or `forcedColors`; a third-party guide says forced colours can be emulated and that reduced motion stabilises snapshots. — [Playwright emulation](https://playwright.dev/docs/emulation); [secondary guide](https://qaskills.sh/blog/playwright-emulate-media-print-styles)

### Inferences
- Playwright projects: one per scheme (`light`, `dark`, optionally `forced-colors` and `reducedMotion: 'reduce'`), each running the same `await expect(page).toHaveScreenshot()` tests on every window route (hub pages, side panel, toasts, overlay). Keep baselines per project. Disable animations and mask dynamic content for stable diffs.
- Contrast automation: `@axe-core/playwright` (rule `color-contrast`) in each scheme; axe's contrast rule has known limits with gradients and images (UNVERIFIED), so add a token-level script that computes ratios for every text/surface pair in the token file, using `culori`, `colorjs.io` (which has APCA support) or `apca-w3` (UNVERIFIED availability).
- Token lint: Stylelint with `declaration-property-value-disallowed-list` or `stylelint-declaration-strict-value` to ban raw hex/rgb/hsl in component CSS, and an ESLint rule or grep check for colours in TSX inline styles (UNVERIFIED rule names). Allow raw colours only in the primitives file.
- The target-size and focus rules can be checked in Playwright by reading bounding boxes (at least 24x24) for every `button, [role=switch], [role=tab]`.

### Gaps
- I did not verify the current versions or exact configuration keys for Stylelint plugins, axe-core, `colorjs.io` or `apca-w3`.
- Playwright `forcedColors` and `reducedMotion` syntax was not confirmed from official docs this session.

## Windows 11 desktop assistants and overlays

### Takeaway
Follow Windows layering (Mica as base, Acrylic only for transient flyouts, Smoke for modals), honour the OS theme and Focus Sessions, and keep notification content short and actionable.

### Cited Findings
- Acrylic is limited to transient, light-dismiss surfaces; Mica is the opaque base layer; Smoke dims for modal dialogs. — [Materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials)
- Notifications can be silently placed into Notification Center, and apps should detect Windows 11 Focus Sessions. — [App notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)
- Notification actions that are background tasks (such as quick reply) should not launch the app; clicking a notification should open the app in that notification's context. — [App notifications design basics](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-ux-guidance)

### Inferences
- An always-on overlay should behave like Smoke or a transient surface: solid, minimal, dismissible, and never covering focus targets. Proactive speech or toast alerts should respect Focus Sessions (suppress or queue) and escalate only for high urgency.
- Use the OS accent colour only as a hint; keep a fixed brand accent that is tested in both themes, since Windows accent values are auto-adjusted but your custom surfaces are not.

### Gaps
- Fluent 2 for Windows component pages (spacing, type ramp, control heights) and Windows App SDK specifics: not retrieved.
- Official guidance specific to "AI assistant" overlays on Windows 11: none found.
