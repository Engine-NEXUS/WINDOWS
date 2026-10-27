# Voice Lock P2 — effective native status and verified Save

Date: 2026-10-25. Updated 2026-10-26 IST: repaired shipping settings device checks pass on release `0b10fd3a1416a540`; see [current assessment](../testing/runs/windows1-20261025-voice-lock-ui-v2-assessment.md). Earlier checkpoints below are retained. Speaker verification remains 41% (2.05/5); owner/negative/recovery and capture proof are pending.

## Scope and authorization

The owner requested the next phase while P1's optimized release was compiling and its ten-trial live reconciliation was pending. P2 implementation proceeds under that instruction; neither phase is marked live-complete and the queue stays on speaker verification. [Research and ordered plan](../features/152-voice-lock-evidence-and-reliability-plan.md).

## Changes

Both Command Hub → Audio and legacy settings → Wake show **Effective Voice Lock: On / Off / Unknown**, separately from the draft toggle. Native state is refreshed on mount, focus and visibility recovery; the Audio page also refreshes on entry and profile-status changes. Refresh is available manually. This is the last completed native readback, not a continuous security monitor.

Save now requires a native readback after patch acknowledgement. Saved settings and native routing must agree; an explicitly changed Voice Lock value must match the request. Errors remain visible in the Voice Lock section. Missing/unreadable profiles refuse enable; disappearance during Save rejects verification. An active lock without a valid profile is reported as blocking voice actions, preserving keyboard/settings recovery.

Concurrent saves are refused. Older refreshes cannot overwrite newer reads or a Save. Edits made while Save is pending remain dirty; asynchronous initial settings load preserves locally edited fields. Only dirty fields are patched, preserving other-window settings. Native authorization, thresholds, default-off policy and transcription behavior are unchanged.

Implementation: frontend/src/settings/voiceLockState.ts, VoiceLockStatus.tsx, SettingsApp.tsx and frontend/src/settings-sidebar/SettingsSidebarApp.tsx. No P2 source was copied into the frozen P1 release tree during compilation.

## Regression and mutation evidence

Tests cover unknown status, native/settings mismatch, failed and slow saves, duplicate Save, stale reads, external-window state changes, refused enable, profile loss, malformed native status, draft retention and rendered effective-versus-draft wording. New tests first failed because the model did not exist. The first mutation run exposed an untested post-Save disagreement path; a regression was added before rerunning. An initial overlapping-save mutation timed out; its test now settles both requests so it fails by assertion. The mutation harness uses isolated temporary copies and never edits production source.

Evidence: docs/testing/runs/windows1-20261025-voice-lock-ui-*. 22 focused new tests and the full frontend suite of 682 tests (69 files) pass; TypeScript/production build, IPC, turn-end, repository hygiene and scoped whitespace checks pass. All seven UI model safety mutations are killed. Final counts and file hashes are in voice-lock-ui-assessment.json beside these logs. Native/Worker/Python suites were not rerun for this frontend-only phase; earlier P1 checks do not establish a new P2 full-source release assessment. These are frontend checks, not a whole-current-primary-checkout release assessment. Concurrent primary work is preserved.

## Original live walkthrough checklist — current results linked above

| Check | Acceptance | Result |
|---|---|---|
| Initial/reopened Audio | Effective display equals native voice_lock_enabled and get_settings | Pending |
| Toggle without Save | Draft changes; effective value remains the native value | Pending |
| Save On with valid profile | Saving shown; Saved only after native true readback | Pending |
| Save Off | Native false readback before Saved; keyboard recovery retained | Pending |
| Leave/reopen and another settings window | Effective display refreshes; unrelated drafts are preserved | Pending |
| Missing/corrupt/deleted profile | No silent unlock; enable refused/error visible, recovery usable | Pending |
| Slow/refused Save | No premature Saved; errors visible; unsaved edits retained | Pending |

Use a fresh provenance-verified P2 release for this walkthrough. Do not attribute it to P1's older source/build identity. Record successes and failures in the live-proof ledger against the actual release. P1's ten-start reconciliation and the owner/non-owner/replay gates still remain required; no score increase follows from this UI fix alone.

## Frozen verification follow-up

The owner requested verification. Combined P1+P2 snapshot 9c3f0320a5c7df359c6a9769e8492ed48baf8fcd passes the full shared suite: Rust 2,532 library + 19 integration (38 ignored), frozen frontend 678, Worker 145, Python 60, NLU and all repository guards, including no new dead code. All 28 UI/audit/speaker mutations rerun for this snapshot are killed. The optimized release is compiling; provenance and real-window checks are pending. See [frozen assessment](../testing/runs/windows1-20261025-voice-lock-ui-frozen-assessment.json). Primary-tree and frozen counts are distinct.
