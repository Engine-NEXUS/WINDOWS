# Change 79 — Voice accuracy measurement harness (Phase A)

**Why:** there was no trustworthy accuracy number for the wake word or STT (every recall figure in the docs was measured on training files or a leaky random split; STT was never measured on real audio). Audit + plan: `docs/research/voice-accuracy/01-voice-accuracy-audit-and-plan-2026-10-20.md`. This change builds the measurement only — **no model or runtime behaviour changed.**

## What was added

| Piece | File | What it does |
|---|---|---|
| Evaluator core | `src-tauri/src/voice_eval.rs` (+13 tests) | WAV parsing, the production 16 kHz downmix/resample mirror, drives the **real** `WakeEngine::process` (clock-slaved to the audio, determinism and silence/noise tests), the real hallucination filter + `strip_wake_prefix` + `parse_deterministic`, stage-2 gate, Groq/Moonshine transcription facade, WER. Registered in `lib.rs`; never used by the app. |
| Wake evaluator | `src-tauri/examples/wake_eval.rs` | manifest → per-clip fired / time-to-fire / max probability, long-recording false alarms, optional `--stage2` (real Groq check + production two-key gate), `--threshold` override, `--selftest` (synthetic Piper positive controls). |
| STT evaluator | `src-tauri/examples/stt_eval.rs` | manifest → acoustic pre-gate → Groq/local STT (owner/neutral decoder prompt) → filter → parser; WER, intent match, filter drops, hallucination leaks. `--text-only` scores supplied transcripts (no mic/key). |
| Held-out recorder | `scripts/voice_eval_set.py` | guided, resumable recording (`record`/`room`), `plan`, `validate`, `stats`. 276 clips per speaker + 30 min room audio overall; split is always `test`; manifest fields follow `wake_data_foundation.py`. |
| Scorecard | `scripts/voice_scorecard.py` | merges both outputs into the stars/% table; Wilson 95% intervals, Poisson bound for false alarms/hour; stars use the *lower* bound; unmeasured rows show "not measured", 0 stars; `--baseline` shows "was …". |
| Tests | `scripts/test_voice_eval_tools.py` (21) | plan, validation, statistics, star rules, report. |

Visibility changes in `wakeword_oww.rs`: `mod engine` → `pub(crate) mod engine`, `vad_trim` → `pub(crate)`.

## Harness sanity check (synthetic Piper voice — NOT an accuracy number)

`wake_eval --selftest`: "nexus" 0.929, "hey nexus" 0.993, "okay nexus" 0.969 fire; "lexus", "hello there", "open chrome" stay silent (≤ 0.003). **"next us" also fires (0.890)** — a soundalike the neural model accepts; the stage-2 check or retraining has to catch it.

## First real findings (offline, deterministic)

Running 53 expected-phrase rows through the real text pipeline (`stt_eval --text-only`) **confirmed audit defect #11**: the hallucination filter wipes real commands even when the speech engine transcribed them perfectly —

- `list pull requests` → dropped (filtered to empty)
- `order pizza from dominos` → dropped
- a lone `thank you` → dropped (acceptable trade-off, but it should be a deliberate one)

All other 48 phrases resolved to the expected intent. Fix is Phase B6.

## How to run the baseline (Windows)

```powershell
python scripts/voice_eval_set.py record --speaker owner --device laptop-array --section wake      # then command / negative / silence
python scripts/voice_eval_set.py room   --speaker owner --minutes 10 --environment tv              # x3, 30+ min total
# repeat for 2-3 other people with --speaker friend1 ... (separate sessions)
python scripts/voice_eval_set.py validate
cd src-tauri
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --out ../wake.jsonl
cargo run --release --example wake_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --stage2 --out ../wake_stage2.jsonl   # needs GROQ_API_KEY
cargo run --release --example stt_eval  -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --out ../stt.jsonl                  # needs GROQ_API_KEY (or --engine local)
python ../scripts/voice_scorecard.py --wake ../wake.jsonl --stt ../stt.jsonl --json ../baseline.json --out ../docs/research/voice-accuracy/scorecard-baseline.md
```

Keep `baseline.json`; after each fix phase re-run with `--baseline baseline.json` to get "was …" deltas. `eval_set/` is git-ignored (voice recordings are personal data).

## Limits (stated, not hidden)

- Accuracy numbers need the owner's recordings on Windows; none exist in this checkout.
- Offline clips cannot measure the capture chain (audio lost between "nexus" and the start of command capture); the one-breath row is therefore labelled "STT + parser only".
- Word-accuracy CI treats words as independent (optimistic); use it for ranking, not for claims.

## Verify

Rust lib 935/935 serial (baseline 922 + 13 new); Python 21/21; both examples build in release; Windows-only paths type-checked via temporary Linux scaffolding (removed before commit).

> **Correction (change 85):** `wake_eval.rs` / `stt_eval.rs` were not actually committed with this change — `src-tauri/examples/` was git-ignored. They are tracked from change 85 onward.
