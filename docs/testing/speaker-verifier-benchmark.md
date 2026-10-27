# P3 offline speaker-verifier benchmark

Implemented 2026-10-26. This is score-analysis infrastructure for [phase P3](../features/152-voice-lock-evidence-and-reliability-plan.md), not a new speaker model or production calibration. There is no qualified corpus or measured comparison yet. Voice Lock stays at 41%; no queue advancement.

The owner controls NEXUS builds, runtime and settings manually. These commands only read an explicitly supplied JSON file and write a report to standard output. They do not open the microphone, load profiles, access credentials, download models, call a cloud provider or install thresholds.

```powershell
python scripts/speaker_benchmark.py <experiment.json>
python -m unittest discover -s scripts -p '*_test.py'
```

Redirect standard output to an evidence file when a real frozen experiment exists. A malformed file exits 2 with a generic error and no partial report. Import `validate(data)` for a more specific validation error during offline preparation; do not publish private source data in diagnostics.

## Input contract (schema version 1)

Objects have exactly the fields below. Unknown fields, including transcripts and file paths, are rejected. IDs and condition labels must be anonymous tokens matching `[A-Za-z0-9][A-Za-z0-9_.-]{0,95}`. Do not use real names, emails or speech text as identifiers. Syntax validation cannot establish anonymity, consent or truth of supplied labels.

| Object | Fields |
|---|---|
| Root | `schema_version`: integer 1; `experiment_id`: token; `source_revision`: lowercase 40-character Git commit hash; `models`: nonempty model list; `rows`: nonempty scored-trial list |
| Model | `id`: unique token; `license`: short declared license expression (letters, numbers, spaces, parentheses, periods, plus, underscore and hyphen); `artifact_sha256`: lowercase 64-character SHA256; `score_direction`: exactly `higher_is_owner` |
| Row | `trial_id`, `model_id`, `sample_id`, `recording_group`, `speaker_id`, `speaker_kind`, `condition`, `partition`, `score`, `latency_ms`, `peak_memory_mb` |

`speaker_kind` is one of `owner`, `live_non_owner`, `owner_replay`, `other_speaker_playback`, `synthetic_playback`. `partition` is `calibration` or `acceptance`. Scores must be finite numbers in [-1,1]. Latency and peak memory must be finite nonnegative numbers. Booleans and numeric strings are rejected. Duplicate JSON fields, model IDs and trial IDs within a model are rejected.

Each model must score exactly the same trial IDs with identical metadata; only score, latency and memory may differ. Multiple models are needed for a comparison; a one-model report is only a baseline. Each model needs both owner and live non-owner trials in calibration and acceptance. Synthetic/playback trials cannot substitute for those classes.

No sample or recording group may cross partitions. Assign a recording group to the original recording session/source, so cropped, transformed or replayed versions of that source cannot leak across splits. Live unfamiliar speakers and non-owner playback/synthetic identity tokens cannot span calibration and acceptance. The enrolled owner may span partitions using separate recordings/groups. A speaker ID cannot switch between owner/owner-replay and non-owner classes. Incorrectly assigned new tokens cannot be detected; the corpus review must check the source mapping privately.

Repeated samples or multiple trials within a group are counted as rows, with distinct sample/group/speaker counts disclosed. They do not create independent observations. Keep authentic sample lineage and record session, playback device, distance and noise groupings in a private consented corpus index, outside this score report. Treat correlated row-level intervals as descriptive, not an independence guarantee.

## Threshold and report

For each model, select the lowest observed **calibration** score strictly above every live non-owner calibration score. If no observed score qualifies, use `nextafter(max_calibration_score, +infinity)` to reject all calibration rows. Acceptance uses `score >= threshold`. This maximizes observed owner calibration acceptance subject to zero observed live non-owner calibration acceptances; it is not proof of a safe population false-accept rate. Replay calibration results do not change this stated objective.

Acceptance scores never select the threshold. Do not adjust thresholds, models or prompt/preprocessing variants after inspecting acceptance results and still call that partition held out. A changed decision requires a newly held-out acceptance set and a separate experiment record.

Reports identify the supplied revision, model hashes/license labels and SHA256 of canonical complete input JSON (sorted keys, compact separators, UTF-8, default ASCII escaping). The input fingerprint includes acceptance rows; it changes when those scores change. It is an integrity link, not a signature or independent artifact/source verification.

Every speaker kind is reported separately, including absent kinds with zero counts and null rates. Reports include per-condition results within each kind, accepted/rejected counts, acceptance rates and Wilson 95% intervals, owner FRR, live non-owner FAR, exact one-sided 95% zero-acceptance upper bound, distinct speakers/groups/samples, median and nearest-rank p95 latency, and maximum observed peak memory. Resource measurements are declared upstream results; this script does not measure a model process. Small samples, correlated recordings and collection bias must remain visible in the interpretation.

`action_success` is always `unmeasured`. An accepted score does not prove an executable action occurred or succeeded. Reports contain no automatic winner or production-ready verdict. Offline false acceptance is separate from the release programme's **non-owner executable actions** gate. Run the real-device protocol and reconcile captures independently.

## Corpus and model work still required

1. Obtain explicit participant/recording consent and establish retention/access controls. No extra diagnostic recording is authorized by this tool. Do not reuse the single retained failure diagnostic clip as a new acceptance corpus.
2. Freeze source, preprocessing, candidate artifacts, licenses, partitions and scoring procedure before acceptance evaluation. Compare the current OWW-based verifier with a dedicated speaker embedding candidate only after obtaining legitimate artifacts and auditing export support, license obligations and local resource use. No model has been selected or downloaded by this phase.
3. Independently score enrolled-owner speech across positions/noise and unfamiliar live speakers from disjoint calibration/acceptance identities. Label owner replay, other-speaker playback and synthetic playback separately. Owner-only solo testing leaves live unfamiliar-speaker gates pending.
4. Review private sample lineage for overlap and verify artifact hashes/revision independently. Generate the JSON score file, run this validator, retain its fingerprint and record successful and failed experiments. Never publish raw audio, profile vectors or participant identities in this report.
5. Use measured results to propose a reviewed integration change. Any later runtime integration, threshold installation, fresh release and device trial is separate work; the owner handles builds and runtime. Existing authorization, repetition and correction guards stay in force.

## Implementation verification

The 18 synthetic regression tests cover split leakage, identity relabelling, exact model trial alignment, calibration-only selection, reject-all ties, malformed/nonfinite data, metric separation, dependency counts, resource summaries, fingerprints and CLI failure handling. They demonstrate analysis behavior, not biometric accuracy. The existing 29 script tests also pass (47 total). Mutation results and final file hashes are recorded with [change 168](../changes/168-offline-speaker-verifier-benchmark.md).

## Exported embedding adapter (2026-10-26)

[Change 169](../changes/169-enrollment-isolated-embedding-scores.md) adds [enrollment-isolated scoring](speaker-embedding-score-workflow.md), a 19th scalar regression for finite-latency median overflow, and a repeatable 18-mutation check. Current script suite totals63. The change168 hashes/counts above remain the historical first assessment; use the change169 assessment for current files. No corpus accuracy or native inference was measured.
