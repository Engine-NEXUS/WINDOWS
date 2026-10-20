# Voice accuracy runbook — record → score → fix → retrain → compare → promote

Everything below exists and is tested; the only thing missing in the repo is **your recordings** (no accuracy number can be produced without them). Commands are PowerShell from the repo root unless noted.

## 0. One-time
```powershell
pip install sounddevice numpy scipy onnxruntime       # recorder + trainer tools
```

## 1. Record the held-out TEST set (you + 2–3 others, never used for training)
```powershell
python scripts/voice_eval_set.py plan
python scripts/voice_eval_set.py record --speaker owner   --device laptop-array --section wake       # then negative, command, silence
python scripts/voice_eval_set.py record --speaker friend1 --device laptop-array --section wake       # …other people, other sittings
python scripts/voice_eval_set.py room   --speaker owner --minutes 10 --environment tv                # 3x → 30 min; more is better (see §5)
python scripts/voice_eval_set.py validate
```
Use a **new session** (`--session`) for every sitting. Keep the TV / talk running for `room` and do not say the wake word.

## 2. Baseline scorecard (before changing anything else)
```powershell
cd src-tauri
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --out ../wake.jsonl
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --stage2 --out ../wake_stage2.jsonl   # needs GROQ_API_KEY
cargo run --release --example stt_eval  -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --simulate-endpoint --out ../stt.jsonl   # needs GROQ_API_KEY
cd ..
python scripts/voice_scorecard.py --wake wake.jsonl --stt stt.jsonl --json baseline.json --out docs/research/voice-accuracy/scorecard-baseline.md
```
Send me `scorecard-baseline.md`. The rows with the fewest stars are the work list.

## 3. Switches decided BY the data (all default OFF / unchanged)

| Question | Run | Turn on when… | Setting |
|---|---|---|---|
| Is the trainer's front end better than the shipped one? | `wake_eval --dsp legacy` vs `--dsp aligned` | recall ↑ (esp. other speakers, noisy/far/whisper) with false alarms/h not ↑ | `wakeDspProfile` |
| Command hints for non-owner speech? | `stt_eval --bias neutral` vs `--bias commands` | command match ↑ and the hallucination row not ↑ | `sttCommandHints` |
| Groq confidence gate? | scorecard "Confidence gate calibration" | it says *Safe to enable* | `sttConfidenceGate` |
| Where should the command capture start? | `stt_eval --capture-offset-ms -320/-160/0/80` | one-breath row ↑ with command match not ↓ | `wakeSeedOffsetMs` |
| Longer first-pause tolerance? | `stt_eval --simulate-endpoint --endpoint-silence-chunks 5/6/8/10` | "cut off mid-sentence" ↓ | (constant `STT_SILENCE_CHUNK_LIMIT`) |
| Is Groq turbo costing accuracy? | `stt_eval --engine groq` vs `--engine groq-large` | word accuracy ↑ meaningfully | (production model const) |
| "next us"/"next sis" as near-wakes? | `wake_eval --stage2` soundalike row | soundalike false alarms stay ≈ 0 | `NEAR_PAIRS` in `wake_names.rs` |

After each switch: re-score with `--baseline baseline.json` to get “was X%” deltas.

## 4. Threshold (C4)
```powershell
cd src-tauri
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --sweep 0.50,0.55,0.60,0.65,0.68,0.72,0.76,0.80 --out ../wake_sweep.jsonl
cd ..
python scripts/wake_calibrate.py wake_sweep.jsonl            # table + recommendation (highest recall inside both false-alarm budgets)
python scripts/wake_calibrate.py wake_sweep.jsonl --apply    # writes kws_threshold, then:
python scripts/wake_data_foundation.py fingerprint-models
```
`wakeSensitivity` (low/medium/high) is the user-facing knob around this value.

## 5. Retrain only if the scorecard says the model is the problem (C2 → C3 → C1)
1. **Clean the data by ear** (not Whisper): `python scripts/wake_audition.py segment` (cut `wake_word_data/free/` into utterances) then `review` them; also review `negative/soundalike_from_*` (the 345 files moved on 2026-09-24).
2. **Record training positives** (separate people/sittings from §1, including "okay NEXUS" and bare "NEXUS"): `python scripts/voice_eval_set.py --purpose train record --speaker friend1 --section wake` (and `negative`), then `--purpose train validate` and `python scripts/wake_data_foundation.py validate`.
3. Add hard negatives you find (real false alarms, TV phrases) through `wake_audition.py review` so they land in `negative/`.
4. `python scripts/train_local_wakeword.py --dry-run` (counts, split sizes — every split needs positives), then `python scripts/train_local_wakeword.py` → `nexus.candidate.onnx` + `train_report.json`.
5. **Score the candidate, not the report**: copy the candidate over a scratch resources dir (or `--promote` on a branch), run §2/§4 with `--dsp aligned`, compare with `--baseline`. Promote only if every wake row is ≥ the old model's and false alarms/h are not worse.
6. Promote: `--promote`, then `python scripts/wake_data_foundation.py fingerprint-models`, commit.

## 6. What “done” means (from the plan)
Wake recall per phrase ≥ 95% with the 95% lower bound ≥ 90%; ≤ 1 false alarm per 8 h on out-of-sample room audio (needs ≈ 30 h of room audio to *prove* — `wake_calibrate.py` says so); one-breath commands ≥ 95%; command match ≥ 95%; hallucination leaks and wrongly dropped commands ≤ 0.5% each. Until a row has the data, the scorecard shows **not measured** rather than a guess.
