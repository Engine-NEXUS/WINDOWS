# Speaker embedding candidates: preliminary source audit

Date: 2026-10-26. P3 research only. No artifact downloaded, candidate selected, inference executed or NEXUS runtime changed. Published results do not establish NEXUS owner/impostor/replay performance.

| Candidate | Primary-source findings | Open work before an experiment |
|---|---|---|
| Existing NEXUS OWW feature baseline | Local `src-tauri/src/voice_profile.rs` uses maximum cosine over enrolled vectors. The live path also depends on the existing audio-window/preprocessing pipeline. | Freeze the actual bundled model and preprocessing hashes; export explicit offline embeddings through a separately reviewed adapter. Python cosine calculations are not native f32/inference parity proof. |
| SpeechBrain ECAPA-TDNN VoxCeleb | Model card labels the model Apache-2.0, describes speaker embedding extraction and cosine verification, and specifies 16 kHz mono for supplied tensors. The documented interface uses SpeechBrain/PyTorch. [Official model card](https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb/raw/main/README.md). | Verify exact pinned artifact notices and dependencies. Audit an ONNX export and compare embeddings against the original runtime if an export is used. Measure short wake-phrase and command recordings locally; the card explicitly limits claims about other datasets. |
| WeSpeaker VoxCeleb ResNet34 ONNX | Official pretrained documentation lists both checkpoint and ONNX runtime variants, including ResNet34. It states pretrained model terms follow the corresponding dataset, and gives VoxCeleb CC BY 4.0 as its example. [Official pretrained-model documentation](https://raw.githubusercontent.com/wenet-e2e/wespeaker/master/docs/pretrained.md). | Verify exact checkpoint/export provenance, model-specific license and notices; pin preprocessing, artifact hash and runtime. Do not infer weight terms solely from the repository code license. Measure local latency, memory and accuracy before choosing. |

WeSpeaker's repository code uses Apache-2.0. That code license does not resolve the separate model-weight declaration above. [Repository license](https://raw.githubusercontent.com/wenet-e2e/wespeaker/master/LICENSE).

Sherpa-ONNX documents a speaker-identification model catalogue and an extractor API in its official example. Its example averages enrollment embeddings; NEXUS currently uses a maximum over individual enrolled vectors. Treat averaging as a separate experimental scoring policy rather than an interchangeable baseline. The example also opens a microphone: it was read for API context only and was not run. [Speaker-identification documentation](https://k2-fsa.github.io/sherpa/onnx/speaker-identification/index.html), [official example](https://raw.githubusercontent.com/k2-fsa/sherpa-onnx/master/python-api-examples/speaker-identification.py).

## Decision boundary

The ONNX entry makes WeSpeaker a plausible deployment experiment; SpeechBrain provides a documented dedicated speaker-verification reference. This is an inference from the interfaces, not a selection or performance ranking. Exact artifact licenses, CPU/Windows provider support, minimum practical clip duration, export correctness, language/noise robustness, RAM and latency remain unmeasured. Do not install either candidate into Voice Lock yet.

Next: establish independently consented enrollment, calibration and acceptance recordings; freeze their lineage and model/preprocessing artifacts; extract embeddings offline; compare paired trials using the benchmark. No extra recording consent is implied, and the single retained diagnostic clip must not be repurposed as held-out acceptance evidence. Owner acceptance, live unfamiliar speakers, owner replay and other/synthetic playback remain separate gates.
