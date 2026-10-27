# Enrollment wake-isolation follow-up — 2026-10-25

**Superseded:** a new regression exposed cancellation waiting indefinitely for a pending microphone permission request. It failed on this revision. The release attempt was stopped before completion and was never launched; see [the revised assessment](windows1-20261025-enrollment-v2-assessment.md).

Status: **in progress**. The owner's failed interaction on release `812a5b3333669e02` is preserved in the ledger. No accuracy trial or production-ready score is claimed.

## Exact source

- Assessed snapshot: `ade0bfdddd99d52efe14da089d33ee5b7493d8a0` (descends from the original frozen `954c57471be0564f69763f73b43e0868f50c74ca`).
- Stable build-input SHA-256: `328ab8cd23bf39fde595a16ee15dbd18ecbaf81f2c5c7ec4c76203a2c604cf11`.
- Managed checkout: `C:\Users\Chitkul Lakshya\.codex\worktrees\windows-readiness\ULTRON`.
- The main D: branch/index and unrelated working edits are preserved. The old candidate was stopped; its unique startup/session logs remain available.
- Early compile attempts were stopped after review found a pending-wake timing race and moved its blocking microphone handshake off the UI thread. The `*-superseded.log` files preserve those exits; they are not completed tests. Final native checks and release run sequentially with one Cargo job.

## Gates

| Check | Result | Evidence |
|---|---|---|
| Complete Rust library + integration | Pass: 2,524 library + 19 integration; zero failures, 38 ignored | [Native log](windows1-20261025-enrollment-fix-rust.log) |
| Frontend TypeScript / build / tests | Pass: TypeScript/build and 655 tests (67 files) | [Frontend log](windows1-20261025-enrollment-fix-frontend.log) |
| Worker TypeScript / tests | Pass: TypeScript and 145 tests (11 files) | [Worker log](windows1-20261025-enrollment-fix-worker.log) |
| Python / NLU / provenance regression | Pass: 48 tests, 512 eval rows / 8 sources and provenance regression | [Python/NLU log](windows1-20261025-enrollment-fix-python-nlu.log) |
| Safety mutations | Pass: 14/14 caught | [Mutation log](windows1-20261025-enrollment-fix-mutations.log) |
| IPC | Pass: 236 registered, 176 invoked; 60 known unused + 6 one-sided events | Newly used `pause_wakeword` removed from unused baseline |
| Turn-end / hygiene / opacity / whitespace | Pass | 132 turn-end files, 1,574 hygiene files, 12 CSS surfaces; `git diff --check` |
| Dead-code / isolated latency | Pass: 148 known findings, none new; 742 ms against 5,000 ms budget | [Dead-code log](windows1-20261025-enrollment-fix-dead-code.log); [isolated latency](windows1-20261025-enrollment-fix-latency.log) |
| Fresh release / provenance | Stopped before completion; superseded by cancellation regression | [Release build](windows1-20261025-enrollment-fix-release-build.log) |
| Windows credential-store prerequisite | Pass: reserved credential written/read/deleted across two processes | [Keyring log](windows1-20261025-enrollment-fix-keyring.log); storage backend only |
| Owner enrollment interaction / accuracy | Not rerun | Needs replacement candidate; keep Voice Lock off |

## First live checks on the replacement candidate

1. Re-enroll all ten clips. During every countdown and recording, saying Nexus / Hey Nexus / OK Nexus must not play a wake cue, open the orb, start a normal capture, or execute a voice action. Record any interruption and the clip number.
2. Confirm all three prompts appear in both five-clip halves; the profile shows five stored clips with five held out for calibration.
3. After completion, ordinary wake listening must return. Check Nexus / Hey Nexus / OK Nexus separately, then cancel each resulting turn without an executable action.
4. Start re-enrollment and navigate away during a countdown, then repeat during recording. Browser capture must stop and ordinary listening must return; the existing valid profile must remain available if no replacement was saved.
5. Record microphone permission or device errors honestly. A failed pause cannot start enrollment; a failed resume cannot show a clean success.
6. Only after the interaction passes, run the existing owner/non-owner/TV accuracy and recovery protocol. The old enrolled profile may contain interrupted clips; do not use it as the new acceptance profile.

Use the [main work card](../speaker-verification-work-card.md) for trial counts, uncertainty and the no-executable-negative-action gate. Speaker verification stays at 41% (2.05/5), and the queue does not advance.

The native library suite completed in 145.91 seconds after compilation. Both `enrollment_mic_baton_blocks_native_capture` and `enrollment_pause_waits_for_in_flight_wake_dispatch` passed. Test compilation reported unused `WINDOW_STYLE` (window_tile), unused `facts` (app_probe), and unused test helper `still_missed` (missed_intent_logger); these warnings are retained in the full log. They are not reported as zero-warning output.

The final isolated latency measurement ran directly from the assessed test executable path `D:\ULTRON\src-tauri\target\debug\deps\nexus_lib-6c52099a28e36522.exe` (SHA-256 `3EDF5788ED54AC2FBD01EE375DC6BF8E77FD71B98C7EEB72463F0E9F4C8A39D7`). A Cargo invocation had recompiled unchanged test inputs after the frontend build; its output is retained separately. The direct benchmark passed at 742 ms best of three. Release compilation started after the native suite and guards.
