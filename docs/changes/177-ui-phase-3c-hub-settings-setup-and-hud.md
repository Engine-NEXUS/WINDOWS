# 177 - UI plan, Phase 3 (third part): Command Hub, Settings, Setup and the companion HUD on the theme tokens

Date: 2026-10-29. Continues [176](176-ui-phase-3b-github-dashboard-and-architect.md). Source and tests only; not run in the live app.

## What was wrong

- The Command Hub (the main settings surface) used its own `--hx-*` palette and about 400 hard-coded colours; its dark base was patched
  into light by around 50 per-class `[data-theme="light"]` rules with fixed slate colours. In a measured pass over every Hub page in
  Light, 21 text elements failed WCAG contrast (white-on-white badges, 1.0 to 1.6:1 pills, 2.1 to 4.4:1 muted text).
- `hub.css` hid every scrollbar again after Phase 2 had restored thin themed ones.
- Several Hub pages coloured text with inline `style={{ color: "#34d399" }}` in the TSX, which a stylesheet fix cannot reach.
- The companion HUD's light sliders followed the Windows setting (`prefers-color-scheme`), not the app's theme.

## What changed

| Area | Change |
|---|---|
| Hub palette | `--hx-*` is now the shared theme tokens under the old names (`--hx-text` = `--ui-text-primary`, accent/danger/ok = the `--ui-*-text` roles). Fills use `--ui-accent-solid` / `--ui-danger-solid` with `--ui-on-accent`. The Hub's thin themed scrollbars come from `unified.css`. |
| Stylesheets | A one-off tokeniser mapped the dark-base colour declarations in `hub.css`, `settings-sidebar.css`, `mcpServers.css`, `settings.css` and `setup.css` onto surface, text, border, accent and status tokens (white text, translucent-white backgrounds and borders, blue/green tints, fixed slate and blue text colours). Light-only patch rules were left in place; their text colours now use the tokens. |
| Inline styles | The same mapping on the inline colours of the Hub TSX (Actions, Device Links, Memory, Hub home, MCP panel, settings sidebar, agent watch, Settings and Setup apps). The rule-effect pills (Allow / Ask / Deny) now use the status tokens. |
| Status text | Light-mode warning and success text darkened (`#92400e`, `#065f46`) so they read on their own tinted pills. The contrast gate measures 168 pairs. |
| Companion HUD | The light slider tokens now key off `data-theme`; the unused `.hud--light` copy was removed. |
| Ratchet | Hard-coded colours: 1,685 before this round of Phase 3, now 1,295 (Hub 408 to about 210, settings-sidebar 194 to 133, Settings and Setup lower). |

## Checks run

- Frontend: TypeScript clean, 761 tests pass, production build with no warnings, all 11 frontend CI checks pass.
- Dev browser, the Hub (`settings.html`): an automated contrast pass over the home view and all 11 pages in Dark and Light, text
  elements measured against their composited background, found 0 failures in 1,088 elements (before: 21 in Light). Light renders
  cleanly on the Actions page (screenshot).
- Setup page: both themes measured; the only flagged element is the primary button, whose gradient background the measuring script
  cannot read (its colours come from the contrast-checked tokens). Two fixed-dark provider chips keep white text.

## Not done (rest of Phase 3)

- About 1,295 colour literals remain in 20 stylesheets, mostly the per-theme palettes, shadows, gradients, brand logos and the
  light patch rules that still override tokens in Light. The ratchet stops them growing; removing the light patches is next.
- Architect's own `[data-theme="light"] .architect-*` patches and its 132 literals; `ghost.css`, `tour.css`, `styles.css`.
- Orb Studio (its own inline-script page) and the real windows were not rendered; only the dev pages were measured.
- Radius, spacing and type standardisation.
