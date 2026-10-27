# Work card 03 — Focus Mode (Hand Mode load control)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 3 of 68. Raw score **48.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 90 | 90 | met |
| T | 75 | 70 | met |
| L | 0 | 70 | **gap 70** |
| S | 80 | 80 | met |
| I | 20 | 60 | **gap 40** |

Evidence today: ~60 tests, 4 mutations.

## Known gap (from the scorecard)

Own-loop gating, CPU-priority lowering of other heavy apps with exact restore and crash recovery; unit-tested only. No Task Manager before/during/after proof and no Hand Mode lag baseline; GPU share is not controlled.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: hand WebView2 renderer (camera + MediaPipe), hand-out thread, 80 ms watchdog, GPU shared with the stage WebView2
- Under Hand Mode Focus Mode: owner of Focus Mode

## How to close each gap

- **L** (0 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **I** (20 → 60): Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.

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
