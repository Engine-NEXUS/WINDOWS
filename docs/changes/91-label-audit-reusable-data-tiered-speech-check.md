# Change 91 — Label audit, reusable public data, synthetic speakers, tiered speech check (Phase E)

**Date:** 2026-10-21 · **Branch:** `claude/nexus-dexter-comparison-1j45rz`

## Why
Three questions from the owner after the measurement/fix program (changes 79-90) exposed gaps:

1. *"Did you cross-check that the negatives are really negative? One soundalike poisons the model."* — **No.** Every earlier label gate transcribed the clip with Whisper and trusted the text, which is exactly what poisoned the set (genuine "NEXUS" heard as "next/texas/access/nixes" was filed under `negative/`: 345 `soundalike_from_pos_*` never re-listened to, 136 `soundalike_from_rec_*` rescued on 2026-10-01). No script used the model's own score to find mislabeled clips.
2. *"Did you train the model?"* — **No.** The shipped `nexus.onnx` (2026-10-01 retrain) is unchanged. `nexus wake train` writes `nexus.candidate.onnx`; the live model is replaced only with `--promote`.
3. *"Background/TV noise already exists publicly — why record it? Does a NEXUS model not exist?"* — Right on both. No public "nexus" model exists. The repo's "background/TV" data was DSP-synthesised noise (filtered Gaussian/sines), not real recordings.

Plus a correction: the speech check (`verifyWake`) is **ON by default** in code (`commands.rs`). `AGENTS.md`, change 48, feature 67, the 2026-09-25 research note and **change 83** said "off by default"; all are now corrected in place.

The "43 %" figure: `docs/wake-word/22-hundred-combo-review.md` (2026-09-19, 99 cases, older model): neural-only recall 93.3 % → **43.3 % with the Groq speech check**; ~28 of 34 misses were STT confabulation on clips stage 1 scored 0.93-0.9996. It protected against false alarms (9.6/h neural-only on 25 min of audio) but cost half the real wake-ups. Nothing is measured for the current model — hence the tiered policy below, chosen from data.

## What was built
| Piece | File | Notes |
|---|---|---|
| Label audit (no Whisper) | `scripts/wake_label_audit.py` | provenance by file origin; out-of-fold scoring (k-fold by near-duplicate cluster, so a clip's duplicates are held out with it); flags negatives scoring ≥ 0.5, positives ≤ 0.2, too-short/near-silent positives, and *always* the old `soundalike_from_*` relabels; decisions recorded by content hash in `audit_ledger.jsonl` |
| By-ear queue | `scripts/wake_audition.py review --queue` | worst first; [p]ositive / [n]egative / [k]eep / [x]exclude → quarantine; a real soundalike scoring high is a *good* hard negative |
| Trainer gate | `scripts/train_local_wakeword.py` | refuses to train while flagged clips are unreviewed (`--skip-audit-gate` overrides and is recorded; `--dry-run` only warns) |
| Public data | `scripts/wake_data_fetch.py` | LibriSpeech, Speech Commands, MUSAN, RIRS_NOISES (CC BY 4.0 / Apache 2.0 only); resumable, size-capped, trust-on-first-use SHA-256 lock, path-traversal-safe extraction; `prepare` builds training negatives, background, synthetic "TV in a room" mixes and long **held-out** recordings; writes `DATA_SOURCES.md`. Excluded: ESC-50, WHAM (NC), DEMAND (share-alike), ACAV100M, MIT IR |
| Synthetic speakers | `scripts/wake_data_synth.py` | Piper multi-speaker voices; each MODEL_CARD licence is checked (CC0 / CC BY / PD / MIT / Apache; NC/ND/SA refused; "See URL" cards fall back to a small vetted dataset table). Clear soundalikes → `negative/`; **ambiguous** ones ("next us", "necks us") → `borderline/` for listening — a TTS can say them *as* "nexus" |
| Trainer | `scripts/train_local_wakeword.py`, `wake_train_lib.py` | public pools; synthetic/public clips use lighter augmentation; `--owner-repeat`; `--seal-owner 0.3` withholds owner takes → `sealed/manifest.jsonl`; **streaming hard-negative mining** (`--mine-rounds`) over the unused pool; vectorised high-pass (`lfilter`, identical recurrence) |
| One accuracy command | `scripts/wake_score.py` / `nexus wake score` | runs the real engine over eval_set + sealed takes + public held-out hours, merges, builds the scorecard; `--model` scores a candidate; new scorecard row for sealed owner takes |
| Speech-check policy | `wakeword_oww.rs` (`VerifyMode`, `verify_mode_from`, `should_instant_fire`), `commands.rs`, Settings select, `wake_calibrate.py --verify` / `nexus wake verify` | `wakeVerifyMode` = `always` (default, unchanged) \| `tiered` \| `off`; `wakeVerifyHigh` (default 0.97, clamped 0.80-0.9999) = stage-1 probability at/above which the check is skipped; unset mode follows the legacy `verifyWake` boolean; candidates with no neural evidence (barge-in prob 0.0) are never instant in any mode |
| CLI | `nexus.mjs` | `wake audit [review|status]`, `fetch`, `synth voices|positives|negatives`, `score`, `threshold`, `verify`; `train` now forwards flags; `devices` fixed and labelled as *synthetic* filters; the old accent synth is `synth-accents`; `data wake validate` documented as structural-only |

## Honest limits
- **No accuracy number exists yet.** The sandbox has no recordings and blocked data hosts (HuggingFace, PyTorch index); everything is verified on synthetic stand-ins and the real code paths. The trainer was run end-to-end here with real torch on a synthetic dataset (base fit → 2 mining rounds → ONNX export, candidate loads in onnxruntime).
- Piper's CLI/model-card format could not be fetched here; the adapter was written against the installed `piper-tts` 1.8 source (`PiperVoice.load`, `SynthesisConfig`), and the licence parser is conservative (anything unrecognised is skipped). Piper itself is GPL-3 — used only as an offline tool.
- The audit's screening classifier is a deliberately simple logistic regression on clip-level embedding statistics: it ranks suspicious clips for a human, it is not the product. A person still decides.
- The shipped model has probably *heard* the owner takes the trainer now seals, so its "sealed owner takes" score is optimistic; the fair before/after is the held-out eval_set and the public held-out hours (`wake_score.py` prints this).
- `tiered` stays opt-in until `nexus wake verify` recommends it on your own data; the 0.97 default is provisional.
- Public-data licences are the publishers' stated ones (checked at implementation time, not legal advice); MUSAN has per-file notes — keep `DATA_SOURCES.md` with the model.

## Verify
Rust lib **973/973** serial (971 + 2: tiered truth table incl. clamping/NaN, mode-setting fallback); Python: `test_voice_eval_tools` 41, `test_wake_train_lib` 21, `test_wake_label_audit` 16, `test_wake_data_fetch` 11, `test_wake_data_synth` 19, `test_wake_score` 3, `test_wake_dsp` 3; `tsc` clean (only the pre-existing `identityBanner.test.ts` errors); CI job `voice-accuracy-tools` extended.

## Your steps (Windows)
See `docs/research/voice-accuracy/02-runbook-record-score-fix-retrain.md` §5: `nexus wake audit` → `audit review` → `fetch` → `synth` → `train --dry-run` → `train` → `score` (shipped, then candidate with `--baseline`) → `threshold` / `verify` → promote.
