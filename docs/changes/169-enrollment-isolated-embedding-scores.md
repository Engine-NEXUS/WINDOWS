# P3 enrollment-isolated embedding scores

2026-10-26. Speaker verification stays **41% (2.05/5)**, with no queue advancement. The owner-only runtime/build restriction remains in force.

## Result

`scripts/speaker_embedding_scores.py` now turns an explicitly supplied offline embedding export into the existing benchmark report. It scores maximum cosine against at least three distinct owner-only enrollment samples. Enrollment, calibration and acceptance must be disjoint by sample and recording group; owner identity and paired-model trial metadata must agree. Calibration and acceptance vectors cannot adapt the template. Enrollment is excluded from trial denominators.

The output retains a complete-input fingerprint, preprocessing hashes, dimensions, enrollment counts and a named scoring policy without publishing vectors. It inherits separate owner/live non-owner/replay/playback results and calibration-only threshold selection. Declared extraction resource measurements remain declarations; model execution, native floating-point parity and executable-action success are unmeasured.

See [the workflow/schema](../testing/speaker-embedding-score-workflow.md). No audio reader, extractor, candidate model or shipping authorization change was added.

The review also exposed a bug in the earlier scalar benchmark: two large finite latency values could produce an infinite median. A new regression failed before the stable nonnegative midpoint calculation was applied; it now passes. This is the only change to the prior benchmark's behavior.

## Coding delegation and fallback

The requested `opencode/glm-5.3-flash-free` responded on this attempt, but the main-repository request repeatedly compacted the roughly 405 KB AGENTS.md and made no edits. Only that request was interrupted. A retry in an isolated temporary directory used a short applicable-instructions file, a reference benchmark copy and the same scoped brief. It read the brief/reference and exited successfully **without creating either requested file**. No GLM implementation or review is claimed.

The supervisor then used the owner's explicit fallback authorization to implement and test the files. Local briefs are `.codex-tasks/p3-embedding-brief.txt` and `.codex-tasks/p3-editor-directory.txt`; the isolated directory remains available for inspection. No other OpenCode task or NEXUS process was stopped.

## Verification

- **63 Python script tests pass**: 29 earlier script tests, 19 scalar benchmark tests (one new overflow regression), and 15 new embedding-scoring tests.
- **18 offline mutations killed by assertion failures**: eight retained scalar safeguards, the median overflow regression, and nine enrollment/scoring/privacy safeguards. Run `python scripts/speaker_embedding_mutation_check.py`; it checks healthy baseline tests first, then changes temporary source copies only.
- IPC, turn-end, repository hygiene, window opacity, generated work-card consistency and whitespace guards pass. Hygiene checks tracked files, so it does not independently certify newly untracked scripts.
- [Assessment and hashes](../testing/runs/windows1-20261026-embedding-score-assessment.json). Synthetic tests are software evidence only. Full Rust/frontend/Worker suites and an application/release build were not run for this Python/docs phase; historical frozen-release counts remain separate.

## Preliminary model research

[The source audit](../research/speaker-embedding-candidate-audit-2026-10-26.md) records official SpeechBrain and WeSpeaker interfaces and model-license declarations. It distinguishes repository code terms from weight terms and max-cosine from averaged enrollment. No artifacts were downloaded, licenses cleared, export parity measured or model selected.

## Remaining work

An explicitly consented private corpus, authentic source/session lineage, frozen artifacts and a reviewed offline extraction adapter are still required before measuring P3. Real owner/unfamiliar-speaker/replay trials, enrollment attribution, profile recovery, capture accounting and repaired action success remain open. Source-only analysis infrastructure does not complete the production-readiness feature.
