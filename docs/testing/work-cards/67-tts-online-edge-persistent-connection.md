# Work card 67 — TTS online (Edge, persistent connection)

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Voice front end. Queue position 67 of 68. Raw score **85.8%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 70 | 70 | met |
| L | 90 | 70 | met |
| S | 80 | 80 | met |
| I | 85 | 60 | met |

Evidence today: 44 tests.

## Known gap (from the scorecard)

409 ms median per short line, 290 ms after wake pre-warm; ~2.6 s end of speech to first audio for a cloud reply.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: Edge TTS connection, Kokoro ONNX (lazy), rodio playback, volume lease watchdog
- Under Hand Mode Focus Mode: cache refresh, voice sync and swap downloads deferred

## How to close each gap

All axis gates are met; remaining work is the shared release gates and a fresh live confirmation.

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
