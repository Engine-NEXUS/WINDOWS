# Enrollment-isolated embedding score workflow

P3 source-only workflow, 2026-10-26. This tool reads an explicitly provided offline embedding export. It does not extract embeddings, open audio, read NEXUS profiles or change shipping verification. The owner still controls all builds, runtime and settings.

## Purpose

The score benchmark alone accepts declared scalar scores. The embedding adapter computes those scores reproducibly from separate enrollment vectors using `max_enrollment_cosine_v1`: normalize each vector, calculate its cosine against each enrollment vector for that model, then keep the maximum. Calibration/acceptance vectors never enter the enrollment template. This is the conceptual maximum-cosine rule in NEXUS; Python arithmetic does not prove native f32, audio preprocessing or model inference parity. It is not the averaged-template policy in Sherpa's example.

```powershell
python scripts/speaker_embedding_scores.py <private-export.json>
python -m unittest discover -s scripts -p '*_test.py'
```

The CLI emits a benchmark report with `embedding_provenance`. There is no scores-only mode. Keep the provenance with the metrics: it fingerprints the complete original embedding export and identifies the scoring policy, preprocessing hashes, dimensions and enrollment counts without publishing vectors.

## Input additions to the score schema

The exact root fields remain `schema_version`, `experiment_id`, `source_revision`, `models`, `rows`. See [the scalar-score contract](speaker-verifier-benchmark.md) for common metadata and identifier restrictions.

| Object | Difference from scalar-score input |
|---|---|
| Model | Add `embedding_dimension` (integer 2–4096) and `preprocessing_sha256` (lowercase 64-character SHA256). Keep `id`, `license`, `artifact_sha256`, `score_direction`. |
| Trial row | Replace `score` with `embedding` (nonzero list of finite numeric values with the model's exact dimension). Add `enrollment` as a permitted partition. All other row fields remain required. |

All models must cover the same complete trial set and metadata, including enrollment. At least three distinct owner-only enrollment samples are required per model. One owner identity must match all enrollment, owner and owner-replay rows across models. A sample cannot appear twice within one model. Recording groups and samples cannot cross enrollment/calibration/acceptance boundaries. Required owner and live non-owner calibration/acceptance classes and unfamiliar-speaker separation are inherited from the score validator.

Use the original source/session lineage to assign recording groups, including derived crops and playback. Merely inventing different IDs does not make recordings independent. The validator checks declared metadata, not the truth of a private corpus. No new recordings or access to an existing installed profile are authorized by this workflow. Enrollment exports and embeddings are sensitive biometric data; keep them in a controlled private corpus, not versioned documentation or score reports.

Finite zero vectors, dimension mismatches, boolean/nonfinite/oversized numeric components, missing metadata, duplicates and partition leakage fail validation. Normalization scales by the largest magnitude before calculating the norm to avoid ordinary finite overflow/underflow. Scores are clamped for floating-point roundoff. Declared latency/memory values refer to upstream extraction; this tool does not measure extractor resource use or scoring overhead.

## Interpretation and remaining work

Enrollment rows are omitted from the score trial denominators. Calibration selects a threshold under the documented zero-observed-live-impostor rule; acceptance is evaluated separately. Input/model/preprocessing hashes are integrity declarations and do not establish correct extraction, artifact authenticity, consent or independence. Retain original artifacts and review these separately.

[The preliminary candidate audit](../research/speaker-embedding-candidate-audit-2026-10-26.md) records official sources and unresolved export/license questions. No dedicated model or threshold has been selected or installed. The next meaningful measurement needs an independently consented and partitioned corpus plus a reviewed offline extractor. Synthetic fixture tests are not speaker-accuracy, replay-resistance or executable-action evidence. Score remains41%; no feature queue advancement.
