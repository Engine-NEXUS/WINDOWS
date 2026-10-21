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
| Speech check: always / tiered / off? | `wake_eval --stage2` then `nexus wake verify` | the table shows tiered keeps recall with room fires inside budget | `wakeVerifyMode`, `wakeVerifyHigh` |
| "next us"/"next sis" as near-wakes? | `wake_eval --stage2` soundalike row | soundalike false alarms stay ≈ 0 | `NEAR_PAIRS` in `wake_names.rs` |

After each switch: re-score with `--baseline baseline.json` to get “was X%” deltas. (`nexus wake score` does the wake half in one command.)

Note: `nexus data wake validate` only checks that the model hash and the data files are consistent. It measures **no accuracy**; `nexus wake score` does.

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

## 5. Retrain (Phase E order: audit → clean by ear → fetch → synth → train → score → calibrate → promote)

You do **not** need to record TV, room noise or everyday speech: real public audio of all of it is fetched, and thousands of synthetic speakers say "NEXUS". Only your own voice (and 2-3 other people for the test set, §1) is recorded.

```powershell
# 0. one-time
pip install numpy scipy onnxruntime sounddevice soundfile piper-tts torch onnx

# 1. AUDIT - find poisoned labels from the AUDIO (no Whisper). Mandatory before training.
nexus wake audit                 # inventory by origin + out-of-fold scoring -> wake_word_data/audit_flags.jsonl
nexus wake audit review          # listen worst-first: [p]ositive [n]egative [k]eep [x]exclude. A real soundalike
                                 #   ("texas", "next us") that scores high is a GOOD hard negative - answer [n].
nexus wake audit status          # exit 0 only when every flagged clip has a decision (the trainer enforces this)

# 2. REUSABLE DATA (permissive licences only; licences + attribution -> wake_word_data/public/DATA_SOURCES.md)
nexus wake fetch list
nexus wake fetch get librispeech-dev-clean librispeech-test-clean librispeech-train-clean-100 speech-commands musan rirs-noises --max-gb 12
nexus wake fetch prepare --max-hours 8          # public/negative, public/background (+ synthetic "TV in a room"), public/heldout (long, never trained on)

# 3. SYNTHETIC SPEAKERS (Piper; each voice's MODEL_CARD licence is checked)
nexus wake synth voices
nexus wake synth positives --per-phrase 800     # "hey/okay/hi/hello nexus" + bare "nexus"
nexus wake synth negatives                      # clear soundalikes -> negative/; AMBIGUOUS ones ("next us") -> borderline/
python scripts/wake_audition.py review --dir wake_word_data/borderline     # listen: some ARE "nexus". p/n decides.
nexus wake audit                                # re-run: new files get scored too

# 4. TRAIN a candidate (a dry run first: every split needs positives)
nexus wake train --dry-run
nexus wake train                # base fit -> hard-negative mining x2 (--mine-rounds) -> nexus.candidate.onnx + train_report.json
                                #   30 % of your own takes are SEALED (--seal-owner) and never trained on -> sealed/manifest.jsonl

# 5. SCORE the SHIPPED model, then the candidate, on audio neither trained on
nexus wake score --json before.json
nexus wake score --model src-tauri/resources/oww/nexus.candidate.onnx --baseline before.json --out docs/research/voice-accuracy/scorecard-after.md
```
Reading the score: the **sealed owner takes** row is optimistic for the *shipped* model (it was trained on those takes before they were sealed). The fair before/after is the held-out `eval_set` (other speakers) and the **public held-out hours** (false alarms/hour) - neither model has heard them.

6. **Choose the operating point from data**
   ```powershell
   cd src-tauri
   cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --resources <dir with oww/ using the candidate> --sweep 0.50,0.60,0.68,0.76,0.80 --out ../wake_sweep.jsonl
   cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --stage2 --out ../wake_stage2.jsonl     # GROQ_API_KEY
   cd ..
   nexus wake threshold wake_sweep.jsonl          # kws_threshold (add --apply)
   nexus wake verify wake_stage2.jsonl            # speech-check policy: off | always | tiered + wakeVerifyHigh
   ```
   The speech check is **on** by default (`verifyWake` defaults to true in code). In the 2026-09-19 99-case review it cost half the real wake-ups on the old model (43 % vs 93 % neural-only recall; ~28 of 34 misses were Groq confabulating text for clips the model scored 0.93-0.9996). `wakeVerifyMode: tiered` skips the check for very sure candidates; set it only when `nexus wake verify` recommends it for *your* model.
7. **Promote** only if every wake row is >= the old model's and false alarms/hour are not worse: `nexus wake train --promote` (or copy the candidate over `nexus.onnx`), then `python scripts/wake_data_foundation.py fingerprint-models`, commit.

## 6. What “done” means (from the plan)
Wake recall per phrase ≥ 95% with the 95% lower bound ≥ 90%; ≤ 1 false alarm per 8 h on out-of-sample room audio (needs ≈ 30 h of room audio to *prove* — `wake_calibrate.py` says so); one-breath commands ≥ 95%; command match ≥ 95%; hallucination leaks and wrongly dropped commands ≤ 0.5% each. Until a row has the data, the scorecard shows **not measured** rather than a guess.
