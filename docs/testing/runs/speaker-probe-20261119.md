# Speaker probe, 2026-11-19: Voice Lock does not tell the owner from other people

Tool: `cargo run --release --example speaker_probe` (`src-tauri/dev_tools/speaker_probe.rs`), the real Voice Lock path (`wake_eval::speaker_embeddings` -> `wakeword_oww::extract_speaker_embedding`, enrollment threshold from `voice_profile::calibrate_owner_threshold`). Full numbers: `speaker-probe-20261119.json`.

Data: 819 owner clips from 7 recording sessions (`wake_word_data`, owner, human). Other voices: 2,000 real people from Google Speech Commands (CC-BY 4.0), one one-second word each. For each owner session with 10+ clips: enroll 5, hold out 5 (the app's own rule), score the owner's clips from the other sessions and all 2,000 other people.

| Enroll session | Threshold | Owner rejected | Other people accepted |
|---|---:|---:|---:|
| 2026-09-22 | 0.922 | 0/494 (0.0%) | 1984/2000 (99.2%) |
| 2026-09-23 | 0.925 | 1/786 (0.1%) | 1990/2000 (99.5%) |
| 2026-09-24 | 0.918 | 0/734 (0.0%) | 2000/2000 (100%) |
| 2026-09-25 | 0.923 | 1/807 (0.1%) | 2000/2000 (100%) |
| 2026-09-26 | 0.927 | 1/652 (0.2%) | 1997/2000 (99.9%) |
| 2026-10-01 | 0.926 | 3/711 (0.4%) | 1995/2000 (99.8%) |

Pooled equal error rate 20.9% (at 0.960). Median score: owner 0.972, other people 0.952.

**Cause.** The "speaker embedding" is the openWakeWord speech embedding (16 x 96 frames, mean-pooled). That model is trained to recognise words regardless of who says them, so every voice lands near the owner's. Raising the threshold cannot fix it: even the best threshold misjudges about 1 in 5 trials.

**Consequence.** With Voice Lock on, almost anyone can give commands. The earlier owner "Verified 0.94-0.96" log lines were real, but another person would score about the same.

**Fix direction.** Replace the embedding with a model trained for speaker verification (candidates: 3D-Speaker CAM++ VoxCeleb, Apache-2.0, 28 MB; WeSpeaker ResNet34 VoxCeleb, 25 MB; 3D-Speaker ERes2Net, 25 MB; ONNX from the sherpa-onnx release `speaker-recongition-models`). Measure each with this probe before choosing.

Limits: played-back recordings, short words for the other people vs wake phrases for the owner, one owner, one laptop microphone.
