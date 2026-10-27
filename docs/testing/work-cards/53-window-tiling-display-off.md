# Work card 53 — Window tiling + display off

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 53 of 68. Raw score **75.7%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 85 | 70 | met |
| L | 55 | 70 | **gap 15** |
| S | 88 | 80 | met |
| I | 60 | 60 | met |

Evidence today: 21 pure + 2 live.

## Known gap (from the scorecard)

Real proof: 3 own windows tiled into exact 640px columns (0-640-1280-1920) and display_off returned Ok. Still needs: the finish sequence on real agent windows, the screen actually going dark seen by the owner, an elevated window skip (W1).

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: foreground/UI Automation calls, SendInput, keep-awake poller (3 s)
- Under Hand Mode Focus Mode: keep-awake never paused

## How to close each gap

- **L** (55 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.

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
