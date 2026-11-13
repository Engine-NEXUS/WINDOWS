# Work card 54 — Main centre / 17 sub-centres validation

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Understanding. Queue position 54 of 68. Raw score **76.5%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 65 | 70 | **gap 5** |
| L | 75 | 70 | met |
| S | 80 | 80 | met |
| I | 60 | 60 | met |

Evidence today: 16 tests.

## Known gap (from the scorecard)

Typed ActionSpec, shared policy; live walkthrough pending. 2026-11-12: machine run of card 5.13 passed 13/13 on build 95f3621b8fbc0b88; 11 of 18 centres exercised; owner voice open.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: regex tier in-process; BERT-Mini NLU sidecar (39218, lazy, 60 s idle); 9Router HTTP
- Under Hand Mode Focus Mode: on demand

## How to close each gap

- **T** (65 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.

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
