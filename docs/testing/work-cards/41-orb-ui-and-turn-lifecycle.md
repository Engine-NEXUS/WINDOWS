# Work card 41 — Orb UI and turn lifecycle

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Voice front end. Queue position 41 of 68. Raw score **71.9%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 72 | 70 | met |
| L | 60 | 70 | **gap 10** |
| S | 65 | 80 | **gap 15** |
| I | 65 | 60 | met |

Evidence today: vitest.

## Known gap (from the scorecard)

Store transitions, thinking dwell and watchdog inputs now tested; the 50-turn soak is still needed.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: stage WebView2 (fullscreen overlay), hitbox thread (30-250 ms), stage watchdog (2 s), WebGL orb loop
- Under Hand Mode Focus Mode: never paused (a starved heartbeat destroys the stage)

## How to close each gap

- **L** (60 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (65 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.

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
