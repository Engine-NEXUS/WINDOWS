# Windows 1.0 readiness: shared checks and speaker verification (in progress)

Date: 2026-10-24–25. Starting HEAD: `fd89438fef88dc8270998048d00d1d73baab8e27` on `feat/action-kernel`. The working tree already contained extensive changes, so HEAD alone does not identify this assessment. Fresh release provenance is verified; no production-ready score or owner acceptance proof is claimed.

## Shared checks

- Fixed the IPC checker's CRLF comment parsing inside `generate_handler!`, a wrong CodeRabbit speech command, and an unregistered dormant return-review command marker. Wired the Agent Watch event listeners and wake-sound preview, removed obsolete IPC wrappers and stale event emissions, and shrank the IPC baseline to 61 known unused commands and 6 known one-sided events. The IPC gate now passes.
- Repaired one settings test fixture, quarantined seven NLU training rows that overlapped evaluation data, regenerated the split lock after validation, and updated two Python assertions that referred to older datasets or validator wording.
- The frozen snapshot passes all automated shared gates: 2,522 Rust library + 19 integration tests (38 explicitly ignored), frontend TypeScript/build + 647 tests, Worker TypeScript + 145 tests, Python 48, NLU validation, IPC, whitespace, turn-end, hygiene, opacity, and dead-code (148 known, none new). The original five-second Smart Turn budget remains an isolated test and passes at 721 ms best of three. Earlier paging-file and timing failures are retained in the run record.
- Concurrent mail edits in the original tree were detected by source hashing and preserved. Final tests were rerun in the managed worktree at snapshot `954c57471be0564f69763f73b43e0868f50c74ca`; its stable build-input hash is `3087f61760442b846c71baaba710b625cf5ebe961e81fa5d4677c381e150ce5e`. Release candidate `812a5b3333669e02` completed from that source and passed provenance verification before and after startup. Startup reached stage ready; microphone and wake models were detected. Optional command-model, Groq-menu and registry-query warnings are retained in the run record.

## Speaker verification

- Command Hub now hosts enrollment. Ten clips are required; five are stored and five are withheld for a provisional owner threshold. This does not establish non-owner accuracy.
- A valid sealed profile is required before enabling Voice Lock. Missing/corrupt profiles, model failures, and uncertain voice turns now fail closed at wake, transcription, frontend confirmation, and Rust dispatch. A corrupt settings file retains the lock. Keyboard and direct UI controls remain available for recovery.
- The stored profile is AES-GCM sealed with a dedicated persisted Windows key, so a conversation-memory wipe cannot rotate it. Valid legacy plaintext and profiles from the earlier shared-memory-key draft migrate on load. Deletion shreds the file and reports OS failures. The single receiver-level wake check replaces the stale setting-independent engine check.
- The second pass found that enrollment bypassed the live mel normalization and reshape. It now runs the live pipeline in a blocking worker, with per-clip reset, peak-energy audio selection, and strict feature-shape checks shared with verification. A synthetic-tone model test checks inference and clip isolation; owner accuracy still requires real trials.
- Release provenance now ignores Python caches and includes the shipped OWW models/configuration. Regression tests cover both cases.
- The second pass also repaired corrupt-profile re-enrollment and settings paths that could silently disable the lock. Ordinary/full settings saves now preserve Voice Lock; an explicit settings patch changes it, and corrupt settings stay locked in both UI and dispatch. Twelve isolated safety mutations are killed, and production profile/crypto compilation passes. Real owner/impostor/TV trials, restart/recovery, and measured threshold accuracy remain required. See [the exact-source run record](../testing/runs/windows1-20261025-assessment.md).
- Added an anonymized CSV template and summary tool for the live trials. Its three tests pass; it reports owner acceptance, non-owner acceptance, executable negative actions, Wilson intervals, and a one-sided 95% upper bound when zero negative actions occur. No trial has been recorded yet.

## Release decision

Speaker verification remains **41% and in progress**. Keep Voice Lock off by default. Do not start installer readiness work until the shared gates pass and the owner records live proof against an exact source snapshot. See [the work card](../testing/speaker-verification-work-card.md).
