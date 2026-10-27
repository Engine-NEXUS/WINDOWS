# Work card 23 — Screen analysis / narrated tour

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 23 of 68. Raw score **64.2%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 96 | 90 | met |
| T | 80 | 70 | met |
| L | 30 | 70 | **gap 40** |
| S | 78 | 80 | **gap 2** |
| I | 55 | 60 | **gap 5** |

Evidence today: 60 Rust tour tests, 11 drawing styles previewed in a browser harness.

## Known gap (from the scorecard)

Vision call now has a 20 s per-request limit (was 4 s, which made every tour fall back to OCR); text-only fallback explains the screen instead of reading junk; the model picks how to draw each item and states the intent; sidebar screenshots zoom. NOT run live: real Gemini latency, 125/150% scaling alignment, speech/callout timing.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: GitHub REST/Worker calls, repo index cache (10 min), Gemini/Groq vision calls
- Under Hand Mode Focus Mode: on demand

## How to close each gap

- **L** (30 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (78 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.
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
