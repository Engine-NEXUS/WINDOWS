# Work card 03 — Reverse engineer website/app

Generated from `docs/testing/scorecard-data.json` by `scripts/make-work-cards.mjs`. Do not edit by hand: put findings in the run records and the live-proof ledger, then update the scorecard data and regenerate.

Area: Desktop control. Queue position 3 of 68. Raw score **48.0%** (production needs ≥ 90% and every axis gate).

| Axis | Now | Gate | Status |
|---|---:|---:|---|
| B | 85 | 90 | **gap 5** |
| T | 65 | 70 | **gap 5** |
| L | 0 | 70 | **gap 70** |
| S | 80 | 80 | met |
| I | 40 | 60 | **gap 20** |

Evidence today: 140 focused tests (scanner, hidden render, accuracy replay, scroll stepper, runner outcomes, phrasings) + 8 safety mutations.

## Known gap (from the scorecard)

Change 197 (source only): honest outcomes (no tour after a refusal, no 'found 0'), cancel everywhere, hidden InPrivate render for SPAs, bot-wall/sign-in/SPA-shell reasons, mouse-free UIA scrolling that stops when the user takes over, table cleanup, offline accuracy harness. Live 2026-11-15 on bb04c8bb: voice dispatch works after the normal-mode arm fix, but the tab address could not be read (likely a full-screen browser); source now leaves full screen with F11 to read it, accepts Brave's location bar class and logs a [REVERSE] url diag line. Table 243 -> 364 with Retire.js (Apache-2.0) and Library Detector (MIT) fingerprints; Wappalyzer forks are GPL-3.0 and not used. Still no successful live scan: needs a rebuild and the owner walkthrough (Next.js, WordPress, Cloudflare-protected site, one desktop app) and the 50-site capture.

## Runs as (processes, for contention and Focus Mode)

- Processes/threads: GitHub REST/Worker calls, repo index cache (10 min), Gemini/Groq vision calls
- Under Hand Mode Focus Mode: on demand

## How to close each gap

- **B** (85 → 90): Wire it end to end in the shipping path (no dead code, reachable by the user, IPC registered, setting kept in `NexusSettings`).
- **T** (65 → 70): Add focused tests for the unproven paths; mutation-check each safety rule (break it, a test must fail); keep fixtures independent of the author's own phrasing where the feature is voice-facing.
- **L** (0 → 70): Run it on the real app and hardware with a fresh release (`nexus build` + provenance verify), follow the live script below, and record logs under `docs/testing/runs/` and a line in `live-proof-ledger.md`.
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
