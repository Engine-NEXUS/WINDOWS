# Work card 28 — Screen analysis / narrated tour

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 28 of 68. Raw score **66.7%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 96 | 90 | met |
| T | 88 | 70 | met |
| L | 30 | 70 | **gap 40** |
| S | 84 | 80 | met |
| I | 55 | 60 | **gap 5** |

Evidence today: 60 Rust tour tests (+2), 12 frontend tour-flow tests (+4), 14/14 mutations killed on the tour engine, failure lines, ladder classification and capture re-checks.

## Known gap (from the scorecard)

2026-11-22: vision failures are spoken with their real cause (no key, quota, rejected key, 503, offline, timeout) instead of one generic line; ladder classification and every safety rule mutation-checked (docs/testing/runs/screen-tour-failures-20261122.md). Vision call has a 20 s per-request limit; text-only fallback explains the screen. NOT run live (owner NEXUS running, harness would stop it): Gemini latency, 125/150% scaling alignment, speech/callout timing, answer quality. Owner script part A.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: GitHub REST/Worker calls, repo index cache (10 min), Gemini/Groq vision calls
- Under Hand Mode Focus Mode: on demand

## How to close each gap

- **L** (30 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
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
