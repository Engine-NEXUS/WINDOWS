# P4/P5 trial evidence protocol

2026-10-26. This reporter reads an explicit offline metadata JSON file. It does not listen, transcribe, open an app, inspect the installed profile/settings or operate NEXUS. Builds and runtime remain owner-controlled.

The legacy five-column CSV summarizer remains available for descriptive historical results. It now always reports `numeric_targets_met=false`, because it cannot establish capture/lock/provenance qualification. Its aggregate negative rates are explicitly mixed-class statistics; use separate speaker/condition counts. Historical files and old results must be retained rather than relabelled as qualified evidence.

```powershell
python scripts/voice_trial_evidence.py <reviewed-trials.json>
python -m unittest discover -s scripts -p '*_test.py'
python scripts/voice_trial_mutation_check.py
```

## Exact schema

Root fields are exactly `schema_version` (integer1), `source_revision` (lowercase40-character Git hash), `source_hash` (lowercase64-character fingerprint), `build_identity` (lowercase16-character identity), `rows` (list, possibly empty). These values are declarations and must be checked independently against source, build manifest and capture logs. The reporter does not authenticate them.

Each row has exactly these fields:

| Fields | Values |
|---|---|
| `trial_id`, `run_id`, `speaker_id`, `recording_group`, `sample_id`, `condition` | Anonymous tokens matching `[A-Za-z0-9][A-Za-z0-9_.-]{0,95}`; do not enter names, emails, target names or transcripts |
| `capture_session` | Positive integer below2^64 or null when attribution is missing; bool is refused |
| `speaker_kind` | `owner`, `live_non_owner`, `owner_replay`, `other_speaker_playback`, `synthetic_playback` |
| `partition` | `enrollment`, `calibration`, `acceptance` |
| `wake_path` | `neural`, `hotkey`, `followup` |
| `voice_lock_enabled`, `verification_reached`, `valid_executable_command`, `executable_action` | Strict booleans |
| `verification` | `accepted`, `rejected`, `uncertain`, `unavailable` |
| `intent_match`, `target_match`, `requested_action_succeeded` | Boolean or null when unmeasured |

Duplicate trial IDs and duplicate attributed `(run_id,capture_session)` pairs are refused. Different runs can reuse a session number. Samples and recording groups cannot cross enrollment/calibration/acceptance partitions; unfamiliar identities cannot cross calibration/acceptance. Source lineage must stay consistent, and one enrolled owner identity is used per evidence packet. Repeated samples inside acceptance remain rows but their distinct sample/group/speaker counts are disclosed. Changing IDs does not create independence.

A true requested-action-success claim requires an observed executable action and independently measured correct intent and target. The schema rejects contradictory success claims. It cannot establish that a human observation or declared label is true.

## Qualification and failure accounting

A trial enters the numeric acceptance screen only when it is in the acceptance partition, Voice Lock is on, a capture is attributed, verification was reached and is available, and the command was valid/executable. Enrollment, calibration, no-capture, lock-off, unavailable-verifier and invalid-command rows remain visible as excluded counts with reasons. More than one reason may apply to a row.

All locked rows are checked for non-owner executable actions and actions without positive attributed verification, including otherwise excluded rows. Either failure blocks the numeric screen. Lock-off control actions remain excluded controls. Owner replay is a negative class even though it uses the enrolled owner's voice.

The declared numeric screen requires at least95% qualified owner acceptance, at least300 qualified negatives, at least one qualified live unfamiliar-speaker observation and no observed locked negative/unverified action. This does not impose a sufficient speaker/session diversity design or establish population security. Results separate all speaker kinds and their condition/wake-path breakdowns. Repeated playback is not a replacement for independent live unfamiliar voices. Mixed negative action counts are labelled; per-kind acceptance results are the appropriate identity comparisons.

The report always says `production_readiness=not_determined`. Numeric targets are only a screen over declared observations. Confidence intervals assume independent rows; distinct counts and the exact zero-action upper bound are not proof of independence. Syntax checks, provenance-shaped strings and hashes cannot replace independent capture reconciliation, native lock readback, artifact verification and device observation.

## Command fidelity is separate

For qualified verified-owner trials, the reporter gives requested-action success/failure/unmeasured counts and independent intent/target match counts. A verified owner whose app never opened remains an owner acceptance with an action failure. Missing observations remain unmeasured. Measured wrong-target/intent locked actions are reported as safety/fidelity failures. No capture receipt or dispatch entry is promoted to action success.

A numeric speaker screen can pass while command-fidelity or another P6 gate fails. Keep those failures open in the ledger; do not promote the feature. The output contains aggregate metrics and source/build/input fingerprints, but no trial/person IDs, transcripts, targets or recordings.

Invalid/unreadable JSON, extra fields, duplicate JSON keys and malformed data exit2 with a generic error and no partial report/private-input echo. Keep the reviewed input privately with its source logs; output fingerprints link to the exact declared metadata, not an independently verified release.

## Completion boundary

Use [the phase closeout](voice-lock-phase-closeout.md) for every P1–P6 gate. The current tests use synthetic data only: no new voice recording or live accuracy evidence was collected. Missing/corrupt/deleted-profile, restart, model/capture failure, non-owner confirmations, stage health and keyboard recovery remain real-device gates. Score stays41%, and the queue stays on speaker verification.
