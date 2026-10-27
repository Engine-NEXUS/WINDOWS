# P3 offline speaker-verifier benchmark infrastructure

2026-10-26. Speaker-verification feature remains in progress at **41% (2.05/5)**. This is a source/docs phase; the owner's NEXUS runtime, builds and settings were not operated.

## Problem and behavior

P3 needs a reproducible comparison of OWW-based verification and dedicated speaker embeddings. No qualified independent corpus or candidate measurement exists yet. The existing live CSV summarizer does not enforce a paired-model experiment, calibration/acceptance separation or recording lineage.

`scripts/speaker_benchmark.py` adds a separate offline JSON score-analysis CLI. It validates paired trial metadata, source/model declarations, speaker identities, partitions and finite measurements before reporting anything. Cross-partition sample/group reuse and unfamiliar-speaker reuse fail validation. It chooses a threshold from calibration only, then reports held-out owner FRR and live non-owner FAR with uncertainty, conditions and separate replay/playback/synthetic results. Reports include declared model hashes, an input fingerprint, group/sample/speaker counts and declared latency/memory summaries. No model executes, no threshold is installed, and executable action success remains unmeasured.

The schema, manual workflow, dependence caveats and pending corpus/license/export work are documented in [the benchmark protocol](../testing/speaker-verifier-benchmark.md). The current shipping authorization and decoding code are unchanged.

## Requested coding delegation

The owner requested OpenCode with `opencode/glm-5.3-flash-free` as editor, with the supervising agent planning/testing and editing only if the delegated editor failed. OpenCode listed the model and started session `ses_e8703f62fffeta9giAOWq5HhEs`, but produced no response or edits after approximately five minutes. Only that request was interrupted. The supervising agent used the explicitly authorized fallback to implement the scoped files. No GLM code or successful GLM review is claimed. The brief is retained locally at `.codex-tasks/p3-speaker-benchmark-brief.txt`.

## Verification

- 18 new synthetic benchmark tests and 29 existing script tests pass: **47 total**. New identity-relabel, path-identifier and input-fingerprint tests were observed failing before their narrow fixes, then pass.
- Eight deliberate mutations each trigger assertion failures: remove split isolation, tune on acceptance, admit calibration impostor ties, admit incomplete classes, allow unpaired models, allow identity relabelling, change the acceptance boundary and disable the input fingerprint.
- Repository hygiene, generated work-card drift and whole-tree whitespace checks pass. Hygiene checks tracked files; the new tool accepts anonymous score metadata and contains no credentials. Git emits existing line-ending conversion warnings.
- [Assessment with source file hashes and mutation outcomes](../testing/runs/windows1-20261026-speaker-benchmark-assessment.json). Mutation runs use temporary copies and synthetic data; they do not touch NEXUS.
- No Rust/frontend/Worker or release build was run for this Python/docs-only phase. Previous frozen release gate results do not cover these new tool files or concurrent primary-tree edits.

## Remaining gates

The script validates declared score metadata; it cannot establish consent, authentic sample lineage, correct labels, verified artifacts or unbiased model selection. Correlated row-level intervals must not be treated as independent population evidence. No qualified trial was collected, no candidate selected, and no owner/non-owner acceptance rate established by these synthetic tests. Corpus collection, model/license/export audit, measured comparison, shipping integration and owner-controlled release/live checks remain pending. Keep the queue on speaker verification.
