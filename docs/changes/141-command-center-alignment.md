# Command-center alignment (2026-10-19)

## Why

The 17 action-center names were mostly routing labels while execution still
used broad `Subsystem` arms. The shared `SubCenter::confirm_kind` implementations
were advisory, and compound steps did not pass through the single-turn policy.
The compound batcher also treated adjacent Worker steps as independent even
when the plan declared `depends_on` edges.

## Changed

- Added typed `CenterId`, `ActionSpec`, confirmation delegation and compound
  eligibility in `center.rs`. `prepare_action` applies one owner/evidence,
  supported-NLU-label, and slot-validation check. New side-effecting intents
  default to gated; the existing app/site-opening exception remains.
- The orchestrator consumes `prepare_action` before dispatch. BrowserCenter's
  validation now covers deterministic browser intents. Known local NLU labels
  have truthful local routes; unknown NLU labels are refused.
- Both deterministic and admin-brain compound builders use `ActionSpec` to
  exclude standalone actions. Every step is rechecked before its executor,
  including after confirmation resume. An unplannable compound asks the user
  for separate commands instead of falling through to a partial action; a
  recognized standalone step receives a specific refusal.
- Batching now requires a dependency-free read query. The present `then`
  planner gives each later step a predecessor, so execution is sequential.
  Failed, unsupported, and skipped steps stop later dependent steps.
- GitHub read commands are classified as read risk. Existing GitHub, MCP,
  file, and power confirmation handlers still own their actual gates.
- Added the canonical architecture map in
  `docs/architecture/12-command-center-alignment.md` and marked older
  documents' historical claims.

## Verification

- Focused Rust `center::` filter: 60 passed, 0 failed. The first full Rust
  library run had 1815 passes, four failures in concurrent PR-list/watch-power
  work, and one stale browser-route assertion fixed in this change. The
  post-fix rerun, excluding those four known failures, passed: 1816 tests,
  0 failed, 22 ignored. After removing an unused display adapter, the
  center/command-center filter passed 61 active tests, 0 failed, 1 ignored.
- Final `cargo check --features custom-protocol,admin-brain`: passed with
  0 warnings and 0 errors.
- TypeScript `tsc --noEmit`: passed. Vitest: 45 files, 367 tests passed.
  Production frontend build: passed, with existing `lottie-web` eval and
  Ghost mixed-import notices.
- Turn-end and repository hygiene gates: passed. IPC contract gate still
  reports unrelated unregistered/unused commands and one-sided events.
  The final dead-code ratchet reports three unrelated items (`tts.rs`:
  `read_tts_emotion_setting`; `vision.rs`: `provider`, `quota_hit`) and two
  stale baseline entries; no new item from this change.
- Same-machine, debug-mode policy microprobe (100,000 iterations): legacy
  separate-call path 1,387 ns/turn; aligned `prepare_action` 604 ns/turn.
  This was measured before removing the unused display wrapper; it measures
  Rust routing overhead, not speech or cloud response latency.
- A rebuilt Windows voice walkthrough and before/after live turn-latency trace
  have not been run. The currently running `nexus.exe` predates these edits.

Unrelated dirty-tree edits were preserved.
