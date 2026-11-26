# Work card 07 — Window theming (solid panels)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Platform and quality. Queue position 7 of 69. Raw score **55.7%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 90 | 90 | met |
| T | 85 | 70 | met |
| L | 0 | 70 | **gap 70** |
| S | 78 | 80 | **gap 2** |
| I | 60 | 60 | met |

Evidence today: gate self-test (16 fixtures), 10/10 mutations on the real stylesheets, registry over 17 CSS files + blur scan of all panel CSS.

## Known gap (from the scorecard)

About 20 translucent or blurred layers removed from panel windows. Gate hardened 2026-11-22: resolves CSS variables across all themes, catches slash-alpha / transparent / color-mix, scans every panel stylesheet for blur, registers the hub, setup and stack roots (docs/testing/runs/window-opacity-gate-20261122.md). Still not seen in the real app: no screenshot of any panel in either theme; owner steps in four-star-screen-tour-owner-script.md part B.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: stage WebView2 (fullscreen overlay), hitbox thread (30-250 ms), stage watchdog (2 s), WebGL orb loop
- Under Hand Mode Focus Mode: never paused (a starved heartbeat destroys the stage)

## How to close each gap

- **L** (0 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (78 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.

## Shared gates (must pass on the assessed revision)

- `cargo check --lib` clean and `cargo test --lib -- --test-threads=1`; frontend `tsc --noEmit`, `vitest run`, `npm run build`; Worker tests if touched.
- `node scripts/check-turn-ends.mjs`, `check-repo-hygiene.mjs`, `check-ipc-contract.mjs`, `dead-code-audit.mjs`, `check-window-opacity.mjs`.
- Fresh release with provenance (`node scripts/build-provenance.mjs verify`) before any live claim.

## Live script (owner)

1. Write the 5–10 exact spoken or clicked steps that prove the feature, including one failure case (offline, wrong window, denied permission) and the expected honest message.
2. Run them on the fresh release; save the console log to `docs/testing/runs/<date>-<slug>.log`.
3. Record pass/fail per step, sample sizes where it is a rate, and the build identity in `docs/testing/live-proof-ledger.md`.

## Done when

- [ ] Every axis gate above is met with evidence, not estimates.
- [ ] Safety rules each have a mutation that fails a test.
- [ ] A live-proof-ledger line exists for the assessed build.
- [ ] `scorecard-data.json` updated, cards regenerated, queue order rechecked.
