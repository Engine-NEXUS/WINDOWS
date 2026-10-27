# P4/P5 trial accounting and P6 source checkpoint

2026-10-26. The owner requested continuing through the remaining phases with edits, planning and tests. Source-only work and permitted automated checks are recorded here. **The P1–P6 programme is not fully complete or production accepted.** Corpus/model comparison, full exact-source application gates and real-device proof remain open; score stays41% (2.05/5), with no queue advancement.

## Changes

The legacy CSV summarizer could previously return `numeric_targets_met=true` for owner rows plus mixed live/TV negatives without capture, native lock or provenance qualification. It now always identifies those files as unqualified descriptive evidence. Results retain compatibility metrics, label mixed negatives and separate speaker kinds and their conditions. Ragged rows, padded IDs/headers/conditions and delimited blank records fail instead of crashing or silently disappearing. CLI failures no longer expose a traceback or source contents.

`scripts/voice_trial_evidence.py` adds a separate offline declared-metadata format for qualified trial accounting. It distinguishes enrollment/calibration/acceptance, captures, lock state, verifier availability, command validity, five speaker classes and three capture paths. Duplicate captures, invalid provenance shapes, contradictory action success, mixed owner identities and partition/lineage leakage fail validation. Excluded locked negative/unverified actions still block the numeric screen. Lock-off controls remain separate.

Command fidelity is independent of identity acceptance: a verified owner with a failed or unobserved action is reported as an owner acceptance plus action failure/unmeasured result. Reports expose measured intent/target mismatches. They contain aggregate metrics and fingerprints, not transcripts, targets, vectors or trial/person IDs. The numeric screen **never** establishes production readiness; metadata still needs independent reconciliation and device observation.

[Evidence format and interpretation](../testing/voice-trial-evidence-protocol.md). [P1–P6 status and owner handoff](../testing/voice-lock-phase-closeout.md). The earlier work card's obsolete launch recipe is now explicitly historical; it is not current runtime guidance. No shipping authorization, model threshold or runtime setting was changed.

## Requested editor and review

OpenCode `opencode/glm-5.3-flash-free`, session `ses_e86e34b13ffeLmbFH7bfxPgKnp`, edited the two legacy CSV files in an isolated directory and passed its nine tests. The supervisor ran those tests against the old source first: two failures and three missing-output errors exposed the pre-fix behavior. They passed with GLM's implementation.

Independent review added three failing regressions for padded headers, silently skipped blank records and per-kind condition separation. The supervisor made the narrow fixes; all12 legacy tests pass. The larger follow-up reporter request stalled after reading the brief and created no files; only that request was interrupted after several minutes. Under the owner's explicit fallback authorization the supervisor implemented the richer reporter and its18 synthetic tests.

The test runner initially used the wrong server working directory (`agent/tests` from the repository root). That invocation ran no tests and is excluded. The corrected `server` run passed31 tests. The new trial mutation runner initially refused its denominator mutation because the anchor appeared three times rather than two; the helper was corrected and the complete mutation run then passed. Neither failure is hidden or counted as a passing run.

## Verification

| Check | Current result / scope |
|---|---|
| Python script suite | **90 pass**: existing/P3 tests60, legacy12, richer evidence18; synthetic/offline tooling |
| Server Python | **31 pass**, corrected server working directory |
| Frontend | **692 tests /69 files pass**, TypeScript no-output check passes; no production build |
| Worker | **145 tests /11 files pass**, TypeScript no-output check passes; no deploy/build |
| NLU |512 evaluation rows /8 sources valid |
| Provenance regression |1 test passes against temporary fixtures; does not verify a new executable |
| Trial mutations |**21/21 assertion failures**, temporary synthetic copies |
| P3 mutations |**18/18 assertion failures**, temporary embedding/score copies |
| Existing safety mutations |**40/40**: speaker/enrollment15, UI11, native decoder/dispatch7, capture audit6, async opener1; temporary harnesses/mocks |
| Source guards | IPC, turn-end, hygiene, opacity, generated work-card consistency and whitespace pass |

The native tests compile isolated harnesses and temporary test profiles/stubs. They do not rebuild or operate the NEXUS application. Hygiene covers tracked files; it is not an independent audit of every untracked addition. Worker emits an existing Vite configuration-loader warning. Git emits line-ending conversion warnings.

[Assessment, current source fingerprint and file hashes](../testing/runs/windows1-20261026-phase-closeout-assessment.json). [Final script-test output](../testing/runs/windows1-20261026-phase-closeout-script-tests.log). Counts for earlier frozen releases remain separately attributed. The working tree is dirty and was not turned into a new assessed release revision.

## Gates deliberately left open

No NEXUS start/stop/restart, runtime setting changes, recording, installed profile access, application/release build or deployment occurred. Complete application Rust library/integration and the compile-dependent dead-code gate were not rerun; historical results cannot fill those current gates. No fresh executable provenance or real owner action was tested.

P3 still needs consented independent corpus data, artifact/license/export verification and a measured candidate comparison. P4 needs held-out command fidelity and stage health on a fresh release. P5 needs qualified owner/unfamiliar-live/replay evidence plus restart/profile/model/capture/confirmation and keyboard recovery. P6 needs every shared check on one frozen source, the owner-built verified release, real-device proof, second review and rubric thresholds. A passing declared numeric screen or this source checkpoint closes none of those gates automatically.
