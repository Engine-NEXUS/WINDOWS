# 175 - UI plan, Phase 3 (first part): CodeRabbit, Assistant view and stage chips on the theme tokens

Date: 2026-10-29. Phase 3 of [`docs/research/ram-and-ui-research/reports/RAM usage and UI UX plan.md`](../research/ram-and-ui-research/reports/RAM%20usage%20and%20UI%20UX%20plan.md),
after Phase 1 ([173](173-ui-phase-1-tokens-and-theme-switching.md)) and Phase 2 ([174](174-ui-phase-2-dock-buttons-and-chrome.md)).
Phase 3 is large (about 1,700 hard-coded colours across 20 stylesheets), so it is done in steps in the plan's order
"by damage". This record is steps (a) and (b) plus the stage rule and a gate that stops the problem growing. Source and tests only.

## What was wrong

- `coderabbit.css`: a fixed dark panel (`#151515`, 39 hex colours, no tokens). In Light mode it stayed dark inside a light sidebar.
- `sidebar.css` (the Assistant view): white text hard-coded in about 23 places (headings, bold, table headers, annotate palette),
  white-on-light about 1.05:1, dark text shadows meant for a wallpaper, black scrims for table headers and code blocks.
- The stage chips and the notification card each carried their own copy of the same dark colours; the hand chip was translucent
  (against the solid-surface rule). No written rule said whether stage elements follow the theme.
- Nothing stopped new hard-coded colours from being added.

## What changed

| Area | Change |
|---|---|
| CodeRabbit panel | `coderabbit.css` rewritten onto `--ui-*` tokens: surfaces, text, borders, status pills, the review-severity scale, the note box, the running-review spinner and sweep. It follows Light and Dark. It imports `tokens.css` itself, so it is also right when loaded alone (the dev harness). |
| New tokens | `--ui-surface-sunken`, `--ui-attention-*` (CodeRabbit orange), `--ui-info-text`, the severity scale `--ui-sev-*`, `--ui-success-solid` / `--ui-on-success`. Each is defined for both themes and measured by the contrast gate. |
| Assistant view | `sidebar.css`: the `--apple-text-*` tokens now come from `--ui-text-*` in both themes (this alone fixes every use of them); fixed-white text, translucent-white borders and backgrounds, dark text shadows and black scrims were replaced with tokens; text on a coloured fill (badges, retry, confirm) uses the `--ui-on-*` tokens; the confirm spinner follows `currentColor`. Intentional dark surfaces stay: the image letterbox, image zoom button and the lightbox scrim. |
| Stage rule (documented in `tokens.css`) | Every chip and card on the stage stays **solid dark in both themes**, so it reads on any wallpaper or window; the sidebar, Hub and settings windows follow the theme. New `--stage-*` tokens; the task chip, notification card and hand chip use them (the hand chip is now solid, not translucent). |
| Contrast gate | `scripts/check-theme-contrast.mjs` now measures 144 pairs (was 78): attention, info, the severity scale, code blocks, success and attention fills, and the stage pairs. |
| New gate | `scripts/check-hardcoded-colors.mjs` (CI step and `ci-local`): a ratchet over `frontend/src/**/*.css`. A stylesheet may not gain a hex, `rgb()` or `hsl()` value; a new stylesheet starts at zero; the baseline (`scripts/baselines/hardcoded-colors.json`, 1,685 colours in 20 files) can only shrink. `tokens.css` is the only file allowed to define colours. |

## Checks run

- Frontend: TypeScript clean, 761 tests pass, production build with no warnings, all 11 frontend CI checks pass.
- Dev browser, both themes (transitions disabled): the CodeRabbit panel renders light (`#f8f9fa` panel, white cards, dark text) and dark;
  Assistant-view headings, bold text, table headers, retry button and topic title compute dark-on-light in Light and white-on-dark in Dark
  (the table header and code block no longer use black scrims).
- Mutation: adding a hex to a stylesheet made the colour gate fail; restored.

## Not done yet (rest of Phase 3, in plan order)

- (c) Architect, PR list and the GitHub dashboard: retire `.theme--light/.theme--dark` in favour of `data-theme` (`architect.css` 138, `pr-list.css` 166 colours).
- (d) The Hub sub-pages (`hub.css` 408, `settings-sidebar.css` 194, `mcpServers.css` 60), Setup (39), Settings (109) and Orb Studio.
- (e) The companion HUD (80): remove its own `prefers-color-scheme` path.
- Standardising radii, spacing and type by token, and the Assistant view's remaining 84 colour literals (mostly the `--apple-*` surface and accent definitions).
- The real windows were not rendered; the Assistant view was checked on injected markup with the real stylesheet, not with live content.
