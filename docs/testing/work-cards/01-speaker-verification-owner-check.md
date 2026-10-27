# Work card 01 — Speaker verification / owner check

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Voice front end. Queue position 1 of 68. Raw score **41.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 75 | 90 | **gap 15** |
| T | 55 | 70 | **gap 15** |
| L | 10 | 70 | **gap 60** |
| S | 50 | 80 | **gap 30** |
| I | 30 | 60 | **gap 30** |

Evidence today: Frozen f0988d9 shared gates: Rust 2537 library + 19 integration (38 ignored), frontend 687 + TypeScript/build, Worker 145, Python 60, NLU/repository guards, dead-code 148 known/none new; 40/40 mutations. Release d846dd346c470b60 built and provenance-verified; repaired owner action proof pending. Runtime/builds are owner-controlled. Separate P3 Python/docs source: 63 script tests and 18 offline mutations pass; enrollment-isolated exported-embedding scoring and median overflow regression (change169). No full application rerun/build or qualified corpus comparison for P3. P4/P5/P6 source checkpoint(change170): Python90+server31, frontend692/Worker145+types, NLU/guards,79 mutations pass. No current complete Rust/dead-code, fresh release, corpus or live proof; legacy CSV cannot qualify readiness..

## Known gap (from the scorecard)

In progress at 41% (2.05/5). Prior release 0b10fd3a1416a540 passed shipping settings checks, but the owner Notepad action failed despite verified ownership. One consented same-clip experiment reproduced command duplication with the owner vocabulary (2/2), absent with neutral decoding (2/2). Native local execution was also missing. Both fixes are frozen at f0988d9 / d7b49eeb; all shared gates pass, release d846dd346c470b60 is built and provenance-verified. The owner now controls runtime and rebuilds; edits only. No formal owner/impostor rate; no promotion. Stage heartbeat, replacement voice action, ten-start accounting, assessed-build enrollment repetition, real missing/corrupt/deleted-profile recovery and owner/non-owner/TV/replay proof remain required. Earlier failures and attribution corrections are retained in the live-proof ledger. See docs/changes/167-neutral-decoding-and-native-local-execution.md and docs/testing/runs/windows1-20261026-neutral-local-assessment.json.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: wake receiver thread, enrollment pipeline (Setup/Hub), keychain key
- Under Hand Mode Focus Mode: keeps running

## How to close each gap

- **B** (75 → 90): Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).
- **T** (55 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.
- **L** (10 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (50 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.
- **I** (30 → 60): Measure against a stated reference (published number or the previous build) on a held-out set; record the measurement and its uncertainty, not an estimate.

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
