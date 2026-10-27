# Work card 40 — Wake word "NEXUS" (openWakeWord, own model)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Voice front end. Queue position 40 of 68. Raw score **71.8%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 90 | 70 | met |
| L | 55 | 70 | **gap 15** |
| S | 75 | 80 | **gap 5** |
| I | 40 | 60 | **gap 20** |

Evidence today: 64 tests, replay harness.

## Known gap (from the scorecard)

Works live, but the honest held-out measure is recall 63.2% (quiet 3/10, far 15/25) and ~12 false fires/h on real background; the 98.6% figure was self-scored on training data. Target is >=95% at <=0.1/h.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: cpal audio stream thread, silence-recovery thread, openWakeWord classifier (muted by solo Hand Mode)
- Under Hand Mode Focus Mode: keeps running

## How to close each gap

- **L** (55 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (75 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.
- **I** (40 → 60): Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.

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
