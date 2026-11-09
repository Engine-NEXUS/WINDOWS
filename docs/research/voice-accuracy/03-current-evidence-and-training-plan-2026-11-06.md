# NEXUS wake-word accuracy: current evidence and training plan

Evidence audited: 2026-11-06. Multi-user scope confirmed: 2026-11-07. Scope: research and read-only local metadata audit. No application build, runtime operation, settings change, training, promotion, audio upload or microphone recording.

## Agreed scope

The owner selected **many users and laptop microphones immediately**. The first release gate is cross-user/cross-device performance; owner-only calibration is not its acceptance criterion. The quantities below are initial experiment budgets, not proof of broad population coverage or commercial parity.

## Answer

**Today's end-to-end wake accuracy is not established.** There are useful stored measurements, but none examined binds the currently shipped model, exact source, runtime gates and a demonstrably fresh representative test set. An honest current percentage or “Siri-level” rating therefore cannot be assigned.

The most useful stored `wake_word_data/after.json` result is **120/137 owner clips detected: 87.59%, with reported 95% interval 81.03–92.11%**. It also reports **1,504 raw neural detections on 34.15 hours of negative audio: 44.04/hour**, and 600 ms p95 latency measured from speech onset. These are historical neural-stage measurements, not today's live end-to-end wake-ups, and not evidence of acceptable false-wake behavior. This report lacks a model hash and complete run configuration.

We should first measure the frozen current pipeline, identify which stage loses true wakes or admits false ones, and then train only to address measured failures. Increasing epochs or recording thousands more repetitions of the same voice is not a reliable route to commercial quality.

## Local evidence checked

The [metadata audit](../../testing/runs/wake-evidence-audit-20261106.json) records input fingerprints, exact aggregate counts and qualification limits. Training data stays under `D:\NEXUS-Training\data`; existing junctions were neither modified nor staged. No audio was listened to, decoded for inference, uploaded or newly recorded; WAV headers were read only for duration/availability.

| Evidence | Finding | Interpretation |
|---|---|---|
| Current `nexus.onnx` | 860,081 bytes; SHA-256 `f6a84129e9265f55c41a674e82950b637b56bbacfcd594189d1f3ec619a8259b` | Matches the shipped model manifest; file integrity is not accuracy |
| Prior model copy | `nexus.previous.onnx` hash differs from today's model | Do not attribute a past run to today's model by filename alone |
| Current `eval_set/manifest.jsonl` | Absent | No standard fresh owner/other-speaker evaluation set currently available there |
| Main data manifest | 4,159 rows; 826 positive; 448 human-positive rows, all under one owner identity | No measured unseen-human generalization; not a complete count of every audio file on disk |
| Main positive phrase labels | 727 nexus, 99 hey_nexus | Current main manifest does not establish separate OK Nexus evaluation coverage |
| Separate synthetic manifest | 7,600 rows: 4,000 positive, 3,240 negative, 360 borderline; 510 synthetic speaker IDs across all rows | Useful augmentation inventory; not 510 independently recorded people |
| Sealed owner evaluation manifest | 137 clips; all files readable; about 4.41 minutes | Prior shipped model may have trained on them before sealing; no retroactive holdout guarantee |
| Public held-out manifest | 166 files, all readable; 34.1521 hours from WAV headers | Useful existing analysis set; it has already been measured and may have informed selection |
| `train_report.dry.json` | 3,633 train + 739 validation + 567 test positive recordings; 137 sealed owner takes; audit gate marked clean | A preparation/dry-run report, not proof of completed training or measured test accuracy |
| Training reports v29/v30/v33 | Older September run configurations, different model hashes | Training happened historically; those reports do not identify today's score |

The main manifest has no source-group IDs spanning its declared splits. That is a useful metadata check, not a full content-hash, speaker/session leakage or acoustic-label audit. The separate manifests must also be reconciled; counts must not simply be added as independent examples.

### Historical measurements, kept separate

| Record | Stored result | Why it is not today's score |
|---|---|---|
| `before.json` | 33/137 owner clips, 24.09%; 12,394 detections / 34.15 h = 362.91/h | No model/source/config binding; file timestamp is later than `after.json` |
| `after.json` | 120/137, 87.59%; 1,504 / 34.15 h = 44.04/h | No model binding; possible owner exposure; neural-only |
| October 11 replay baseline | 96/152 positives = 63.2% at threshold 0.68; 12 raw detections/h over only 0.42 h negative streams | Older source/model association and short negative duration; not controlled current qualification |
| September 19 99-case review | 26/60 true wakes survive full gate = 43.3%; two false accepts in 39 negatives | Mixes training/held-out/synthetic examples; old gates; not a production incidence rate |
| September neural sweep | Up to 56/60 = 93.3% at lower threshold in the old review | Different operating point and old data; cannot replace current measurement |

Do not average these results, call their difference a verified training improvement, or report 43%, 87.6% or 93% as current accuracy. The September document's statement that 5% false accepts was “safe” is historical wording, not a safety conclusion adopted by this plan.

## Commercial comparison: what is actually supported

There is no universal published percentage that makes NEXUS equivalent to Siri, Google Assistant and Alexa on different devices. Research metrics depend on microphones, language, distance, noise, playback, false-alarm operating point and the device's full cascade. The cited publications describe architectures and specific experiments, not a common current product leaderboard.

| System/evidence | Documented approach | Implication for NEXUS |
|---|---|---|
| [Apple voice trigger system, 2023](https://machinelearning.apple.com/research/voice-trigger) | Streaming detector, high-precision checker, speaker identification and directed-speech mitigation | Improving only the keyword classifier cannot qualify the whole wake pipeline |
| [Apple HomePod far-field work](https://machinelearning.apple.com/research/optimizing-siri-on-homepod-in-far-field-settings) | Multi-microphone processing, echo suppression, dereverberation and competing-talker handling | Laptop speaker echo and distant speech may need audio-front-end work as well as training |
| [Google multi-channel KWS, 2020](https://research.google/pubs/small-footprint-multi-channel-keyword-spotting/) | Multi-mic model evaluated at 0.1 false accepts/hour; relative rejection reductions on specified test sets | Compare recall at a declared false-alarm rate, not unrestricted “accuracy” |
| [Amazon word-level wake verification, 2020](https://www.amazon.science/publications/building-a-robust-word-level-wakeword-verification-network) | Secondary verification distinguishes actual keywords from false candidates; discusses ASR-verifier limitations | Measure the wake verifier independently; generic transcription of a short wake clip can be a bottleneck |
| [openWakeWord upstream documentation](https://github.com/dscripka/openWakeWord) | Shared pretrained embeddings plus a small classifier; synthetic positives; several thousand examples recommended; pretrained models use about 30,000 hours of negatives | Reuse the pretrained backbone. Data diversity and hard negatives matter; NEXUS need not train a foundation model from scratch |

Those sources support pursuing compact, staged local detection. They do not prove NEXUS can match a commercial product after a specific number of epochs or days. The present runtime uses mono downmix of its active microphone channels; this is not proof of a beamforming system equivalent to HomePod.

## Metrics and proposed targets

These are **proposed NEXUS targets**, not vendor claims and not replacements for the existing production-readiness rubric.

| Metric | First cross-user/laptop target | Higher-quality target |
|---|---|---|
| Intended wake recall, quiet/near | At least 97% observed | At least 99% observed per supported phrase |
| Noisy/far speech | At least 90% observed per declared condition | At least 95% observed per declared condition |
| Final audible/UI false wake rate | At most 0.125/hour | At most 0.1/hour, plus a separately reported tighter stretch target |
| Soundalike false activations | Separate rate for ordinary speech and confusable words | Zero observed in a declared held-out challenge set; report its size and interval |
| Wake latency | Report median/p95 and reference point | Proposed p95 at most 500 ms after phrase end; also report phrase-onset latency for comparison with old tools |
| Playback/meeting behavior | No known contradiction of declared suppression/barge-in policy | Real speaker playback, TTS and interruption checks on the intended laptop |
| Speaker authorization | Report separately from keyword detection | Existing Voice Lock programme and its independent owner/impostor/replay gates |

Report stages separately: raw neural detection → speech/checker acceptance → owner acceptance → actual wake/capture starts. Preserve default policy while measuring. A model score of 0.99 is not evidence that “99% of requests work.”

Report numerator/denominator, 95% intervals and per-condition values. At 297/300 successes the observed rate is 99%, but its lower confidence bound is below 99%. For zero false events, the exact one-sided 95% Poisson upper rate is approximately `2.996 / hours`; 30 event-free hours bounds it near 0.1/hour under that convention. The existing reporter uses a more conservative approximately 3.67-event upper bound at zero, so require **at least 48 fresh hours** for the proposed 0.1/hour gate with that reporter. About 300 event-free hours is needed for a 0.01/hour one-sided bound; its existing conservative convention would require longer. Dependence between adjacent clips/sessions must also be considered.

## Ordered work plan

### 1. Freeze and establish provenance before more training

- Identify the exact source, model/frontend hashes, threshold, sensitivity, DSP mode, sample rate/channel selection, verification policy, cold/warm STT state and Voice Lock policy. Record the running build separately; today's source asset may differ from the owner's running executable.
- Attach these identifiers and data-manifest fingerprints to every new raw evaluation result and score report. Existing aggregate JSON files cannot retrospectively supply the missing identity.
- Confirm the replay harness mirrors the actual receiver gates and define intentional exclusions. Neural replay alone is not the full product; stage-2 error/fail-open behavior also needs parity review before it qualifies a live policy.
- The existing `wake_score.py --list` was safely run during this audit. Its normal score path executes `cargo run --release`; do not run it under the owner's no-build restriction. The owner supplies a fresh qualified evaluation executable or performs that build/evaluation manually.

### 2. Create a genuinely untouched evaluation set

Use new recording sessions and unique content/session groups. Never train on final acceptance clips or use them to choose thresholds. Keep a separate calibration set and reserve final acceptance until the candidate and policy are frozen.

- Recruit an initial **30 consented participants using at least 15 physical laptops**, spanning at least eight laptop models and the accents/languages actually declared supported. Windows is the first qualification platform; macOS/Linux parity remains a separate gate.
- Assign **15 speakers to training, five to calibration and ten to final acceptance before collection**. Assign physical devices exclusively to one partition; target seven training, three calibration and five acceptance laptops, with at least three laptop models absent from training. If people share devices, assign the entire connected speaker/device group together rather than leaking a microphone across splits. If the proposed split is infeasible, recruit/reassign before recording, without reducing the final denominator.
- Collect **150 fresh positive clips per person**: three phrases × five conditions × ten repetitions, spread across at least three sittings. Conditions cover quiet near-field, normal working distance, farther within the declared range, realistic fan/background noise and soft speech. This yields **4,500 positives: 2,250 training, 750 calibration and 1,500 acceptance**. Keep all sessions from a person/device in its assigned partition. This is a pilot cohort, not representative coverage of every accent or laptop.
- Record Nexus, Hey Nexus and OK Nexus separately. The owner participates within one declared partition; their existing exposed recordings do not enter untouched acceptance. Report aggregate, per-person, per-device, per-phrase and per-condition results, including uncertainty and the worst groups. A high pooled score cannot conceal a failing participant or microphone.
- Start with **48–72 fresh continuous-negative hours across the five acceptance laptops**, at least eight hours per device and with the remaining hours distributed beforehand. Include realistic conversations, music, TV, laptop playback and silence. These hours stay outside mining and threshold selection. The pooled bound does not establish the same bound per laptop: at zero false events, the existing reporter needs roughly 37 hours per device for an upper bound below 0.1/hour; budget **48 hours per acceptance device (240 hours across five)** if that per-device claim is required.
- Synthetic voices supplement real voices; they do not replace unfamiliar live users. Phone playback is tagged replay rather than a new live speaker. Reserve larger participant/device cohorts after this pilot before making broader population claims.

### 3. Diagnose the largest failure stage first

| Observation | Next change to investigate |
|---|---|
| Neural detection misses an audible genuine phrase | Recording quality, DSP mismatch, threshold sweep, then targeted positives/training |
| Neural stage passes but verifier rejects genuine speech | Inspect short-clip verifier/window/policy; compare unchanged, tiered and dedicated keyword-checker candidates offline |
| TV or soundalikes pass neural stage | Listen/label, mine those families on development audio, tighten separation at matched recall |
| Wake accepted but no capture/UI appears | Runtime gating, cancellation, capture and event delivery; more classifier epochs will not fix this |
| Keyword accepted but owner rejected | Separate speaker-verification work card and calibration; do not weaken owner gates |
| Only during laptop playback or far speech | Echo cancellation, microphone path and room acoustics, not only classifier capacity |

Do not automatically lower the threshold, switch verification off or apply aligned DSP. Compare candidates under the same operating-point budget and preserve the owner's active settings.

### 4. First targeted training budget

These are starting experiment sizes, not required quantities or a guarantee of commercial parity:

| Data/work | Initial additional budget | Expand only if held-out analysis justifies it |
|---|---|---|
| Real multi-user positives | **30 people × 150 clips = 4,500**, on at least 15 physical laptops | Expand to 60+ people and additional device families if pilot coverage or uncertainty is inadequate |
| Training partition | **2,250 clips from 15 people**, using training-only devices | Add measured failure families without consuming acceptance people/devices |
| Calibration partition | **750 clips from five different people**, calibration-only devices | Choose threshold/DSP/checker policy here, never on final acceptance |
| Final positive acceptance | **1,500 clips from ten unseen people**, acceptance-only devices | Expand weak per-person/device/condition slices; preserve correlated-session uncertainty |
| Synthetic positives | Increase from about 4,000 manifest positives toward **10,000–20,000 balanced clips** | Independent voice families, prosody and phrase variants; pitch copies are not new humans |
| Development negative audio | **100–300 hours** of licensed speech/noise/music plus labeled failure clips | Scale hard-negative mining based on measured curves; this budget does not prove commercial parity |
| Final negative acceptance | **48–72 pooled hours** initially; **240 hours** if requiring 48 hours on each of five acceptance laptops | State pooled and per-device bounds separately; do not dilute a noisy device with quiet devices |
| Model experiments | **2–3 controlled candidate rounds**, early stopping on development metrics | Plateau triggers diagnosis or architecture/front-end review, not endless epochs |

These replace the earlier owner-first 500–700-recording proposal. Each participant supplies about 150 new positives, rather than asking the owner to supply the entire real-speaker dataset. Establish a reviewed recording/export checklist with model/build identity, microphone/driver/sample format, distance/noise/session tags, consent, and source-group IDs. Keep recordings local by default; any transfer to a private training store requires participant consent and an agreed secure transfer route. No automatic uploads or account/credential sharing. Raw voice files remain ignored and are not pushed to Git.

Use the existing audit/label review, grouped split, augmentation, hard-negative mining, candidate-only export and threshold tools. Listen to ambiguous labels: a real Nexus phrase mislabeled negative damages recall. Audit metadata saying clean does not independently establish that every new recording was correctly heard/labeled. Keep all existing public-source licence checks and private data boundaries.

### 5. Select the operating point and qualify the complete wake

Compare the current model and candidate on the same calibration data with the same runtime DSP and gates. Select the highest recall inside the declared false-alarm budget; report condition regressions. Sweep DSP/threshold/verifier independently where possible so their contributions are identifiable. Once a policy is selected, freeze it before final acceptance.

The final held-out score must cover raw detector and final wake events, not only frame classification. Then the acceptance participants perform fresh-release tests on their assigned laptops, including restart/resume, real TTS playback and cold/warm behavior. The owner coordinates signoff; one successful owner laptop does not qualify the cohort. Changes in Voice Lock must remain separately authorized and independently verified. No automatic promotion, model replacement or queue advancement occurs during this research.

### 6. Stop conditions and escalation

Stop adding data blindly if several controlled rounds plateau. Inspect labels, session leakage, front-end equivalence, verifier bottlenecks and playback echo. Only then evaluate a compact dedicated second-stage checker or a different wake model, with CPU/RAM/power measurements inside NEXUS's existing resource goals. Prefer testing the current pretrained-backbone approach first.

The stopping rule is demonstrated performance, not an epoch count: agreed recall/false-wake/latency targets on untouched data, no known contradictory behavior, exact release evidence and the existing weighted production gates. The owner-only milestone is superseded by the confirmed multi-user scope. Broad Siri/Google/Alexa parity still requires evidence across more users, devices and environments than a pilot can establish. Any participant/device group below its declared floor blocks qualification; do not hide it in a pooled average.

## Immediate next step

Complete provenance and fresh baseline before scheduling a large training run. The first coordinated action is recruiting and assigning the participant/device partitions, then preparing a reviewed contributor recording checklist based on the existing [runbook](02-runbook-record-score-fix-retrain.md). The owner prepares the evaluation executable manually. The runbook is owner-centric and is not yet a complete distributed collection tool; split enforcement, consent/export handling and per-device reporting must be reviewed or implemented before collection. That older runbook contains build, download, training, setting-write and promotion commands; they are **not authorized to run automatically** by this research request. Current project build/runtime restrictions still apply.

Research references above were checked on 2026-11-06. Local report dates and identities were inspected independently; source metadata is not a new device measurement.
