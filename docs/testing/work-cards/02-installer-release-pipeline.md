# Work card 02 — Installer / release pipeline

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Platform and quality. Queue position 2 of 68. Raw score **47.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 70 | 90 | **gap 20** |
| T | 30 | 70 | **gap 40** |
| L | 35 | 70 | **gap 35** |
| S | 60 | 80 | **gap 20** |
| I | 50 | 60 | **gap 10** |

Evidence today: -.

## Known gap (from the scorecard)

NSIS broken locally; MSI built; start-menu shortcut points at a stale exe.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: build scripts / Hub WebView2
- Under Hand Mode Focus Mode: n/a

## How to close each gap

- **B** (70 → 90): Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).
- **T** (30 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.
- **L** (35 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (60 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.
- **I** (50 → 60): Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.

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
