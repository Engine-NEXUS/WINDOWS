# 176 - UI plan, Phase 3 (second part): GitHub dashboard and Architect follow the one theme switch

Date: 2026-10-29. Continues [175](175-ui-phase-3a-coderabbit-assistant-and-stage-chips.md). Source and tests only; not run in the live app.

## What was wrong

- The PR list / GitHub dashboard chose its theme with a class (`theme--light` / `theme--dark`) from its own store, copied from
  `getThemeMode()` and a separate event listener, instead of the `data-theme` attribute every other window uses. Two switches could
  disagree, and its colours were invisible to the contrast gate.
- Its muted text failed contrast in both themes (light `#8c959f` 2.9:1 on white; dark `#6e7681` 4.1:1 on its background) and its dark
  accent link was 4.06:1 on subtle surfaces.
- `architect.css` carried a second copy of the `--apple-*` token block with the failing text values (tertiary 3.3:1 in dark, 3.3:1 in light).

## What changed

| Area | Change |
|---|---|
| Theme switch | `pr-list.css`: the dark palette is now the base of `.gh-dashboard-shell`, the light palette is `[data-theme="light"] .gh-dashboard-shell`, and the 20 other `.theme--*` rules became `:root:not([data-theme="light"])` / `[data-theme="light"]`. `PrListApp.tsx` no longer adds the class, and its own theme store sync and listeners were removed. The `?theme=` dev URL parameter now applies the real theme. |
| Contrast | Dashboard muted text light `#59636e`, dark `#8e96a0`; dark accent `#4493f8`. The contrast gate measures the dashboard palette too (168 pairs, was 144). |
| Architect | `--apple-text-primary/secondary/tertiary` come from `--ui-text-*` in both themes (hard-coded colours 138 to 132). |
| Ratchet | Hard-coded colour baseline shrank to 1,679. |

## Checks run

- Frontend: TypeScript clean, 761 tests pass, production build with no warnings, all 11 frontend CI checks pass.
- Dev browser (GitHub dashboard dev page), both themes: the shell computes `#0d1117` / light text in dark and white / dark text in
  light; the class `theme--*` is gone; the dock buttons keep their visible styling; the sign-in card renders light in light mode.

## Still to do in Phase 3

- Architect has about 50 `[data-theme="light"] .architect-*` patch rules and 132 colours; those move onto tokens next (not rendered
  this round: Architect has no dev page).
- The Hub sub-pages (`hub.css` 408, `settings-sidebar.css` 194, `mcpServers.css` 60), Setup, Settings, Orb Studio, the companion HUD,
  and the radius, spacing and type standardisation.
