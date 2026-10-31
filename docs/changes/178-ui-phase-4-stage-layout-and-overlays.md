# 178 - UI plan, Phase 4: stage layout, pebble orb in Light, captions and chips that never collide

Date: 2026-10-29. Phase 4 of [`docs/research/ram-and-ui-research/reports/RAM usage and UI UX plan.md`](../research/ram-and-ui-research/reports/RAM%20usage%20and%20UI%20UX%20plan.md),
after Phase 3 ([175](175-ui-phase-3a-coderabbit-assistant-and-stage-chips.md), [176](176-ui-phase-3b-github-dashboard-and-architect.md),
[177](177-ui-phase-3c-hub-settings-setup-and-hud.md)). Source and tests only; the real stage was not rendered (it needs the app).

## A bug from my own Phase 3a, found and fixed here

Phase 3a moved the stage chips, the notification card and the hand chip onto `--stage-*` colours defined in `theme/tokens.css`.
The stage window never loaded that file (it imports only `ghost.css` and `styles.css`), so on the real stage those values were
undefined and the chips would have drawn with no background. The dev-browser checks of 3a used pages that do load tokens, so they
did not show it. `ghost.css` now imports `../theme/tokens.css`; the rendered check below confirms the chips, the card and the live
caption get their solid dark surface on the stage page.

## What was wrong (from the stage audit)

- The stage window was created at a fixed 1920x1080 logical size and never fitted to the monitor.
- The pebble orb's pouch was a hard-coded black SVG fill, so with the Light theme (black particles) it was likely invisible.
- The hand chip and the response caption used the same bottom row; with a visible taskbar they overlapped (7 px at 1080p, 16 px at 150 %).
- The background-task stack sat at a magic `top: 112px`, ignoring the spinner's real size and position.
- The live caption (the user's own words) had no background: white text on a light wallpaper is about 1.2:1.
- The capsule's hover added an amber glow that does not belong to the white palette; the tour pin's white text on `#5a9eff` to `#259ed6` was 2.7 to 3.0:1; stage chips had a 1.15 to 1.3:1 edge against dark content; looping animations ignored reduced motion.

## What changed

| Area | Change |
|---|---|
| Layout owner | New `stage/layoutSlots.ts`: the caption slot (the tuned 0.515 / 0.895), the task-stack slot hanging under the spinner's real rect, `intersects`. The response caption, the live caption, the hand chip and the task chips all read it. |
| Hand chip | `chipPlacement` takes the caption's top edge and stacks the chip above it, so it never shares the caption's row. The old position is kept when no caption is passed. |
| Task chips | `BackgroundTaskChip` listens to the spinner rect (`stage:loading_rect`, physical px divided by the device pixel ratio) and places the stack beneath it through the CSSOM. |
| Pebble in Light | The pouch fill and its container read `--orb-surface` (black in Dark, `#fafafa` in Light, the same pair as the classic capsule). |
| Live caption | The user's words sit on a solid pill (`.live-caption-pill`, the stage surface with a lighter border) instead of bare text. |
| Small fixes | Removed the amber hover glow on the capsule; tour/spatial pin gradient darkened (`#2f6fd6` to `#1d6fb8`, white text above 4.5:1); stage chips and the notification card get a 1 px light ring so their edge reads on dark windows; a stage-wide `prefers-reduced-motion` block stops the looping pulses, drops and sweeps. |
| Stage size | `stage::fit_to_primary_monitor` sets the stage's position and size to the primary monitor's physical rectangle on every show (`stage_target_rect`, unit-tested for 1366x768, 1920x1080, 2560x1440, 3840x2160, a monitor at a negative offset and a monitor that reports no size). |

## Checks run

- `layoutSlots.test.ts`: the orb capsule (pebble, top dock), spinner, task stack and caption never intersect on 1366x768, 1920x1080 at 100, 125 and 150 %, and 2560x1440 at 100 and 150 %; the hand chip never overlaps the caption with the taskbar hidden or showing.
- Rust: 3 tests for the stage rectangle.
- Dev browser (the stage page, with the real chip, card and caption markup injected): `--stage-surface` is `#151515`; the task chip, the notification card and the live caption pill compute solid dark with light text in both themes; the pebble fill computes black in Dark and `#fafafa` in Light.
- Full local CI, frontend and Rust jobs: all 16 checks pass (cargo check in three configurations with zero warnings, dead-code ratchet, full Rust test run, 768 frontend tests, build with no warnings). Hard-coded colour baseline now 1,292.

## Not done (rest of Phase 4)

- **Pebble hit box.** Rust still registers a 200 x 200 click zone for the pebble's 301 x 105 shape. The fix (the frontend reports its real capsule rect through `stage_set_hitboxes`) touches live click handling and was not changed without a live test.
- **Escape ownership.** Stage Escape handlers run in an unfocused click-through window and probably never fire; moving ownership to Rust (as Ghost and Hand do) and stopping a pin dismissal from cancelling the turn needs a live test.
- **Spinner size.** `loadingSize` still resizes only the wrapper, not the Lottie.
- **Display and scale changes.** The stage is fitted when it is shown; a change of resolution or scale while it is up is picked up at the next show or watchdog rebuild, not instantly.
- **Native toast** (icon, tag and group per entity, expiry, replacing the failed toast on recovery), the dead `orbColor` control, the classic/pebble default mismatch (Rust "classic", frontend "pebble"), live regions for captions and the spinner.
- Not run on a scaled display or in the live app: the stage fit, the chip stacking and the pebble in Light should be checked once at 125 % and with the Hub on Light.
