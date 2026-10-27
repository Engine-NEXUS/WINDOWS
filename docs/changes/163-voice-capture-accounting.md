# Native voice capture accounting — P1

Date: 2026-10-25. Status: implementation and automated gates passed; fresh release build starting. No new live capture proof yet. Speaker verification remains 41% (2.05/5).

## Why and implementation

Ten reported owner attempts produced seven identifiable completed captures. Code inspection found that native session IDs advanced only on abort. Normal consecutive captures reused a label.

- Every native capture request now opens an audit attempt with a process-run ID and monotonic attempt number. Accepted starts advance the native session; newer captures invalidate older in-flight buffers.
- A lifecycle token moves with the audio packet. Outcomes cover microphone-held/duplicate requests, cancellation/supersession, receiver failure, stale work, empty/phantom input, owner holds, transcription errors/filters and delivery failure/success. Drop records abandonment; process crashes remain unfinished evidence.
- Start, callback handoff and abort share a barrier; buffers retain the session captured under it. Final stale checking, proof registration and event emission share that barrier too.
- Canonical [CAPTURE] records contain no audio, transcript, vectors, similarity scores, credentials, contacts or file paths. Executable hashing/manifest linkage happens before audio starts. This proves only executable/manifest linkage, not independent source-tree integrity.
- [CAPTURE-DISPATCH] records routing receipt by run/session. Neither transcript emission nor routing receipt proves a successful action.
- scripts/capture_audit_summary.py reconciles requests, actual starts and terminal outcomes. It reports missing starts, duplicate terminals, reused sessions, late stages, uncorrelated dispatch and unexpected dispatch after a non-delivered outcome. Legacy/incomplete logs do not pass.

## Research and plan

[Implementation plan](../features/152-voice-lock-evidence-and-reliability-plan.md): P1 accounting, P2 native-state UI, P3 verifier comparison, P4 transcription diagnosis, P5/P6 live/release gates. [Research](../research/elevenlabs-architect-guided-nexus-review.md) applies installed ElevenLabs Architect guidance locally; no remote review was obtained. Both are linked from the main, research, feature and testing indexes.

## Checks and retained failures

- Reconciliation tests first failed on the missing implementation; red output is retained. All twelve current reconciliation tests pass, including quoted-marker injection and expected ten-versus-seven starts.
- Six audit-module tests pass in isolation. Six mutations are killed: reused audit IDs, duplicate terminals, disappearing abandoned packets, wrong-executable trust, reused native sessions and stale-session acceptance. Generation mutations use the actual native helper functions/tests.
- Primary-checkout native runs encountered concurrent incomplete GitHub work, new focus-throttle trait imports, and an unclosed orchestrator delimiter. Logs are retained. A narrow closure-to-function lifetime fix and Manager import were made to compile those paths; no concurrent feature was removed. The FindPr match was repaired by concurrent work.
- Integrated validation moved to the existing readiness worktree: parent 50dcda2 plus only P1 source/scripts; initial snapshot 23078e140159701644d71735e6dd4c246c5e492d, followed by evidence-reader hardening snapshots 90efc8d and c430a21c9000e00be131478a7a718f53b5f23baf, input ca2daa4bff07a9b6f7dd6b3b797cc09dc5bee188e573fd74d769b2e2ffbe9cba; native Rust source is identical across these snapshots. The primary branch/index and unrelated edits are preserved.
- Isolated full Rust library 2,532 plus 19 integration tests pass (zero failures; 38 explicitly ignored). Frontend TypeScript, 656 tests and production build; Worker TypeScript and 145 tests; Python 60; NLU; provenance regression; IPC, turn-end, hygiene, opacity, whitespace and dead-code guard (148 known, none new) pass. The 15 existing speaker, six audit/generation and two evidence-reader mutations are all killed (23/23). Fresh optimized release provenance and real-device reconciliation remain pending.

Evidence: docs/testing/runs/windows1-20261025-capture-audit-*.

## Live recheck after a verified fresh release

Record the executable/run identity. Perform ten keyboard-start trials with condition labels; separately exercise cancellation, duplicate start and silence. Reconcile saved native logs:

```text
python scripts/capture_audit_summary.py <log> --run-id <run-id> --expected-starts 10
```

Compare physical trial count with native request count too: a hotkey that never reaches native capture remains a discrepancy. Emitted transcripts without routing receipt require investigation; independently observe requested action success. No identity/replay accuracy or UI/save fix is established by this phase. Do not advance the feature queue.
