# Work card 12 — Vision-grounded click

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 12 of 68. Raw score **58.3%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 85 | 90 | **gap 5** |
| T | 75 | 70 | met |
| L | 25 | 70 | **gap 45** |
| S | 85 | 80 | met |
| I | 40 | 60 | **gap 20** |

Evidence today: 81 + 3 wiring tests.

## Known gap (from the scorecard)

2026-11-24: the vision path now vets the foreground window before any screenshot is sent (bank / password-manager / unidentified windows refused aloud), rechecks the window before clicking, and says "appears to be" for a cloud-found click (3 mutations). Whole-primary-screen only; second monitor and race mode (sends to both providers) still open; measurement harness still not run. Measurement harness written (UIA ground truth vs the vision model, kept in the working tree with other uncommitted vision work); NOT run: it sends a screenshot of the whole screen to a cloud provider, so it needs a clean desktop. Found: Groq vision models are decommissioned, so auto uses Gemini only.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: GitHub REST/Worker calls, repo index cache (10 min), Gemini/Groq vision calls
- Under Hand Mode Focus Mode: on demand

## How to close each gap

- **B** (85 → 90): Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).
- **L** (25 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
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
