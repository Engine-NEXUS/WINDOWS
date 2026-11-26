# Work card 19 — Installer / release pipeline

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Platform and quality. Queue position 19 of 69. Raw score **61.8%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 85 | 90 | **gap 5** |
| T | 55 | 70 | **gap 15** |
| L | 55 | 70 | **gap 15** |
| S | 65 | 80 | **gap 15** |
| I | 50 | 60 | **gap 10** |

Evidence today: 19 node tests + 14 mutations (version sync, download pins); 34-check silent install/uninstall smoke script passed once in a clean Windows 11 Sandbox.

## Known gap (from the scorecard)

2026-11-16 source pass (plan 194, change 198): family build by default, one Windows release workflow with checksums and a draft release, Python download pinned in the template (SHA-256 set, signature checked), unpinned 337 MB model download removed, shortcut repointing. 2026-11-17: the admin-variant installer (74,991,120 bytes, SHA-256 d3e4906e..., unsigned) installed and uninstalled cleanly in a fresh Windows Sandbox: 34 of 34 checks (files, Add/Remove entry, Publisher, nexus:// link, Start-menu shortcut, real Python 3.12, 9 pip packages, nothing left after uninstall). A first run crashed after the Python download (fixed by quiet curl output) and a second stalled while the Sandbox session was disconnected, so the 40-minute install time is NOT representative. Still open: app never launched after install, no upgrade-over-old-version test, family build not compiled, no signing or updater, pip packages are ranges (no exact lock), install time unmeasured.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: build scripts / Hub WebView2
- Under Hand Mode Focus Mode: n/a

## How to close each gap

- **B** (85 → 90): Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).
- **T** (55 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.
- **L** (55 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
- **S** (65 → 80): List every way it can act wrongly or leak (wrong window, stale state, secrets, privacy), make each fail closed, say failures honestly aloud, and add a regression test per item.
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
