# 173 - UI plan, Phase 1: one token foundation and correct theme switching

Date: 2026-10-29. Phase 1 of the plan in
[`docs/research/ram-and-ui-research/reports/RAM usage and UI UX plan.md`](../research/ram-and-ui-research/reports/RAM%20usage%20and%20UI%20UX%20plan.md).
Source and tests only. The app was not rebuilt or run; the checks below ran in the dev browser and the test suites.

## What the audit found

- At least seven parallel colour systems (`--nx`, `--hub`, `--hx`, `--apple`, `--gh`, `--kit`, `--stack`), only five CSS files
  importing the shared tokens.
- `theme.ts` only understood "light" and "dark" and defaulted to dark; "System" in the Hub appearance page was converted to a
  fixed light or dark once, so it never followed Windows.
- Each window painted its default colours first and switched after its bundle loaded (a wrong-colour flash).
- Failing contrast, computed from the token values: tertiary text 3.3:1 in light, Hub faint text 2.8:1 in light and 3.8:1 in
  dark, warning (`#d97706`) and success (`#059669`) text on white 3.2 and 3.8:1, Hub accent `#007aff` on white 4.0:1, Hub danger
  `#ff3b30` on white 3.5:1.

## What changed

| Area | Change | Where |
|---|---|---|
| Tokens | A semantic layer `--ui-*` (surfaces, text, borders, focus ring, accent fill/text/on-accent, danger/warning/success text and fill) next to the legacy `--nx-*` names. The semantic names point at the legacy values, so nothing moves visually; new roles are added where none existed. `color-scheme` follows the theme. | `frontend/src/theme/tokens.css` |
| Contrast | Fixed at the source: tertiary text (dark 0.52, light 0.56 alpha), Hub faint text (0.50 / 0.55), light warning `#b45309`, success `#047857`, Hub accent `#0062cc`, Hub danger `#d70015`. | `tokens.css`, `sidebar/unified.css` |
| Theme model | `ThemePreference` = light, dark or **system**. System follows Windows live (media-query listener), is saved as "system", and is broadcast between windows as the preference (each window resolves it the same way). Sets `data-theme` and `style.colorScheme`. | `frontend/src/sidebar/theme.ts` |
| Hub UI | The Appearance page now saves and shows "System" for real instead of flattening it; the old Settings window no longer downgrades a saved "system". | `AppearancePage.tsx`, `SettingsSidebarApp.tsx`, `SettingsApp.tsx` |
| First paint | Every HTML entry loads `public/theme-boot.js` (an external file, so the CSP needs no exception) and carries `<meta name="color-scheme">`, so the theme is set before the first paint. | `frontend/public/theme-boot.js`, 16 `frontend/*.html` |
| Gate | `scripts/check-theme-contrast.mjs` resolves the tokens in both themes, composites translucent colours, and checks 78 pairs (text 4.5:1, focus ring and meaningful borders 3:1). It runs in CI (`ci.yml`) and in `scripts/ci-local.mjs`. | `scripts/`, `.github/workflows/ci.yml` |

## Checks run

- Contrast gate: 78 pairs pass, margins at least about 4.7:1 for text. Deliberately set a light token to 0.30 alpha: the gate failed, then restored.
- Frontend: TypeScript clean, 757 tests pass (79 files), production build with no warning lines.
- Tests added: theme preference and the live follow-Windows logic (fake `matchMedia`), the contrast maths and that every measured token exists in both themes.
- Dev browser (not the real app): `data-theme`, `color-scheme` and the tokens switch for light, dark and system; "system" resolves against the OS; a reload with "system" saved comes up in the OS theme.
- The Hub and the dev pages were checked this way only; the real windows were not rendered.

## Not done yet (later phases)

- The other palettes (`--hx`, `--hub`, `--apple`, `--gh`, `--kit`, `--stack`, `--tour`, `--hud`) are not yet mapped onto `--ui-*`. That is done view by view in Phase 3, with the screenshots of Phase 6.
- The fixed-dark surfaces (CodeRabbit panel, notification card, task chip, hand chip), the invisible dock buttons, the pebble orb in light mode and the stage size are Phases 2, 3 and 4.
- Live follow of a Windows theme change could not be triggered in the dev browser (its colour-scheme emulation sends no change event); the logic is covered by the unit test. Check once in the real app: set the Hub to System, flip Windows between light and dark.
- The saved value "system" is stored as the existing `themeMode` string in settings.json; existing users keep their stored "dark" or "light".
