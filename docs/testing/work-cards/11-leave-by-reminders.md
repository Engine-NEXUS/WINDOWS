# Work card 11 — Leave-by reminders

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Memory and proactive. Queue position 11 of 68. Raw score **56.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 95 | 90 | met |
| T | 80 | 70 | met |
| L | 0 | 70 | **gap 70** |
| S | 85 | 80 | met |
| I | 55 | 60 | **gap 5** |

Evidence today: 12 tests, 5 mutations.

## Known gap (from the scorecard)

Logic split into pure tested functions; route lookups no longer retried every minute after a failure. Still needs a real calendar event with an address (W4).

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: memcore pollers (30-60 s ticks), session snapshot (60 s), resume sampler (30 s), activity sampler (2 s), sealed SQLite store
- Under Hand Mode Focus Mode: snapshot, resume, activity, curator, review, routines paused; mail/WhatsApp/leave-by keep polling and their speech is held

## How to close each gap

- **L** (0 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **I** (55 → 60): Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.

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
