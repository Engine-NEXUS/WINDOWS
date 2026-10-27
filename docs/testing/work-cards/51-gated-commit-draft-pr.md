# Work card 51 — Gated commit + draft PR

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Agent control. Queue position 51 of 68. Raw score **75.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 100 | 90 | met |
| T | 90 | 70 | met |
| L | 45 | 70 | **gap 25** |
| S | 92 | 80 | met |
| I | 65 | 60 | met |

Evidence today: 30 (5 real git).

## Known gap (from the scorecard)

Real git against a local bare remote: branch, commit, push, ignored files left out, secrets/detached/merge/hold refused, push failure keeps the commit. Still needs: a real GitHub draft PR from a throwaway repo (owner approval).

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: agent hooks webhook (port 39220), agent_watch Node sidecar (39222, lazy), watch_runner (3 s)
- Under Hand Mode Focus Mode: keeps running

## How to close each gap

- **L** (45 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.

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
