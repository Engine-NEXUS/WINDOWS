# Change 89 — Training and serving use the same front end; trainer rewritten (Phase B7 + C1)

## Why

The audit found that the classifier was trained on audio shaped by one high-pass/gate/AGC chain and served audio shaped by another:

| | trainer | runtime (before) |
|---|---|---|
| AGC target / cap | 0.030 / 15× | 0.035 / 30× |
| minimum gain | none | per-device `pre_gain` (2.5× on the bundled profile) on **every** chunk |
| loud speech | untouched | ×2.5 then clipped |
| high-pass | 80 Hz | per-device (128.3 Hz bundled) |
| gate | 0.002 | per-device (0.003) |

and the trainer reported optimistic numbers: window-level random split (same recording in train and val), `best_weights = state_dict().copy()` (an alias — "best epoch" was the last), no seed, never read `free/`.

## B7 — one spec, two chains, switchable and measurable

- `src-tauri/resources/oww/dsp_spec.json` is the single source of truth. Rust (`src/wake_dsp.rs`, `include_str!`) and Python (`scripts/wake_dsp.py`) both read it, and both assert the same golden AGC tables (`wake_dsp::tests`, `scripts/test_wake_dsp.py`).
- `legacy` reproduces the previous runtime exactly (test: `test_legacy_is_the_old_inline_formula`). `aligned` is the trainer's chain. **Default stays `legacy`** — nothing changes for you until you measure.
- Setting `wakeDspProfile` ("legacy" | "aligned"), applied live on save; `wake_eval --dsp legacy|aligned` and `stt_eval --dsp …` run the real engine with either chain, so the held-out set tells you which gives higher recall at equal false alarms:

```powershell
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --dsp legacy  --out ../wake_legacy.jsonl
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --dsp aligned --out ../wake_aligned.jsonl
python ../scripts/voice_scorecard.py --wake ../wake_aligned.jsonl --baseline ../legacy.json
```
Switch only if aligned wins on recall (esp. "other people" and "noisy/far/whisper") without raising false alarms/hour. (A model retrained with the shared front end — below — should be scored with `aligned`.)
- `model_manifest.json` refreshed (it described a different model than the one shipped: manifest `2de9887…`, shipped `563b4af…`) and its `runtime` block is now **derived** from `acoustic_profile.json` + `dsp_spec.json` instead of hand-typed stale numbers (0.35 vs the real 0.68). New CI job `voice-accuracy-tools` runs `wake_data_foundation.py validate` plus the tool tests.
- Not changed: the bundled `acoustic_profile.json` is still one developer's mic (HP 128 Hz, pre-gain 2.5, threshold 0.68) — whether a more conservative default is better is exactly what the `--dsp` / `wakeSensitivity` / threshold sweeps will say.

## C1 — trainer rewrite (`scripts/train_local_wakeword.py`, `wake_train_lib.py`)

- Front end = the shared `aligned` spec; proper anti-alias resampling (`resample_poly`) for non-16 kHz files (was linear interpolation).
- Splits are by recording — or by manifest `source_group_id` / `speaker_id`+`session_id` — 70/15/15, deterministic by hash (adding recordings never reshuffles old ones). Augmentation is **train only**; val/test are clean.
- Real best-epoch `copy.deepcopy`; seeded everything (`--seed`).
- Recall / false-alarm at the **deployed** threshold (`acoustic_profile.json`) and at 0.50, per epoch on val and once on the untouched test split.
- New augmentations: speed ±8%, reverb, start shift, random gain, real background noise at 0–20 dB SNR (+ the previous telecom / fan-hum / quiet-speech ones).
- Safer output: writes `nexus.candidate.onnx` + `train_report.json`; the shipped model is replaced only with `--promote` (old one kept as `nexus.previous.onnx`), then run `wake_data_foundation.py fingerprint-models`.
- `--dry-run` (no torch needed) prints the split/window counts and errors clearly if any split has no positives.

## Verify

Rust lib 970/970 serial (+5); Python: eval tools 27, `test_wake_dsp` 3, `test_wake_train_lib` 14 (real mel/embedding ONNX models, end-to-end `--dry-run` on synthetic recordings). The torch part of the trainer is not executed here (no torch in this sandbox); it is the unchanged model/loss with the fixes above — run `--dry-run` first on your machine.
