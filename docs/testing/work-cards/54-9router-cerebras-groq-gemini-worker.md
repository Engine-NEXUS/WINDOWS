# Work card 54 — 9Router (Cerebras > Groq > Gemini > Worker)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Understanding. Queue position 54 of 68. Raw score **76.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 55 | 70 | **gap 15** |
| L | 70 | 70 | met |
| S | 75 | 80 | **gap 5** |
| I | 85 | 60 | met |

Evidence today: 18 tests.

## Known gap (from the scorecard)

~242 ms vs 600-2100 ms through the Worker.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: OAuth loopback listener, vault monitor (90 s), Worker HTTPS
- Under Hand Mode Focus Mode: vault monitor paused

## How to close each gap

- **T** (55 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.
- **S** (75 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.

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
