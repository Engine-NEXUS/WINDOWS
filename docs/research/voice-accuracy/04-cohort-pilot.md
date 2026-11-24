# Multi-user wake-word pilot

Phase 1 implements recording, collection checks and local neural replay for the [multi-user plan](03-current-evidence-and-training-plan-2026-11-06.md). The initial target remains 30 people and 15 physical Windows laptops. Use a small calibration pilot first to check the collection workflow before scheduling all participants.

| Plan area | Cross-check | Next milestone |
|---|---|---|
| Collection | The old recorder had individual sessions and no cohort roster. The cohort CLI now assigns 150 positive slots per person across three sittings. | Run the calibration pilot on two assigned laptops and confirm the captured inputs and instructions. |
| Split isolation | The trainer discovers files by directory and assigns splits from group hashes. The cohort stores WAV and metadata bundles under a separate collection root and checks speaker, device, slot and PCM consistency before exporting. | Prepare a training-only export and make the trainer honor cohort assignments. Check complete trainer inputs, including mining and augmentation pools, against the frozen acceptance collection. |
| Replay | The previous evaluator could load a personal acoustic profile. Cohort replay selects explicit assets and checks the compiled DSP spec against the supplied one. The Python wrapper checks result coverage and records input hashes. | Build the updated evaluator and replay the first real calibration bundle. |
| Full wake qualification | Neural replay counts classifier detections. Actual wake events also depend on the verifier, capture, suppression and device state. | Add provenance-bound live counts, interval estimates, soundalike trials and the approved per-device acceptance criteria. |

## Prepare the roster

Use Python 3.12. Store the roster and collection under an ignored directory such as `eval_set/cohort`. Use pseudonymous participant IDs and one stable identifier per physical laptop. Multiple participants may share a laptop within their assigned split. Use consistent laptop model names. The tool checks assignments from the roster. The coordinator confirms that each identifier describes the actual device and person.

Create `eval_set/cohort/roster.json` using this example shape and assign people to `train`, `validation` or `test` before collecting data. Validation is the calibration split. Reserve at least three acceptance laptop models absent from training.

```json
{
  "participants": [
    {
      "speaker_id": "p01",
      "device_id": "lap01",
      "split": "validation",
      "laptop_model": "example model",
      "device_class": "built-in array",
      "accent": "self-described accent"
    }
  ]
}
```

## Record and combine bundles

Check the default recording input on the assigned laptop. `record` uses the existing 16 kHz mono `sounddevice` recorder. For native multichannel or native sample-rate WAV capture, use `import-wake` with an ID from `plan`. `--consent` confirms permission for this capture or import. The recorder saves each accepted slot immediately and skips saved slots when resumed.

```powershell
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json plan --speaker p01 --sitting 1
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json record --speaker p01 --sitting 1 --consent
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json import-wake --speaker p01 --id p01-s1-near-nexus-01 --wav capture.wav --consent
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json room --speaker p01 --session room1 --room-id fan1 --environment fan --wav room.wav --consent
```

Use `import-wake` or `record` for a given slot. For longer room recordings, import consecutive non-overlapping segments of up to 30 minutes. WAV inputs must be PCM16, at 16, 44.1 or 48 kHz, with one, two or four channels. Keep the participant's WAV and metadata folder together when combining `samples/<id>/` bundles. Resolve any filename collision before merging the bundles. Transfer recordings only using the route agreed with participants.

```powershell
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json audit
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json audit --require-complete
```

The audit reports missing slots and room exposure per device. It rejects inconsistent metadata and duplicate PCM even if a WAV header changes. It cannot identify resampled copies or verify who spoke, what was said, microphone placement, or room conditions. The coordinator reviews those during collection. Acceptance export requires the full roster coverage, positive matrix and at least 48 pooled room hours with eight hours per acceptance laptop. That exposure alone cannot certify the per-device false-wake limit.

## Replay the calibration split

The owner prepares a `wake_eval` executable from this source. Supply the `resources` directory containing `oww` and the desired classifier named `nexus.onnx`. Use separate resource copies for model comparisons. The command saves the selected manifest, row results and a report in a new output directory.

```powershell
python scripts/wake_cohort.py --roster eval_set/cohort/roster.json replay --split validation --evaluator PATH_TO_WAKE_EVAL.exe --resources src-tauri/resources --threshold 0.68 --dsp legacy --out eval_set/cohort/calibration-run-01
```

The report groups raw detections and recall by speaker, laptop, model, phrase, accent and conditions. It includes hashes for the evaluator, wake assets, manifest and results. Replay uses the local detector. The live verifier, speech and command execution checks remain separate. Keep the acceptance split sealed while choosing thresholds, DSP or training data, then freeze the candidate before replaying it. The CLI checks collection completeness for acceptance. The coordinator controls when acceptance audio is opened.

The current evidence comes from generated WAV fixtures and a simulated evaluator response. The next real evidence is a participant capture and a run with the updated compiled evaluator. Follow with trainer integration, a paired candidate comparison and fresh live trials on the acceptance laptops.

On 2026-11-12, the wake Python suite passed 93 tests and the voice evaluation suite passed 41. The final focused cohort suite passed seven tests, including the replay report test added after the broader suite started. Rust syntax checks passed for the four edited files. Native compilation and real audio replay are pending.
