# Change 90 — Threshold calibration, by-ear labelling, training recorder, STT polish (Phases C2–C5, D)

Closes the code side of the voice-accuracy program. The operating procedure is `docs/research/voice-accuracy/02-runbook-record-score-fix-retrain.md`.

## C4 — threshold from data
`wake_eval --sweep 0.50,0.60,…` evaluates every clip (and room recording) at each threshold with the real engine; `scripts/wake_calibrate.py` prints recall / soundalike false alarms / false alarms per hour (95% Poisson upper bound) per threshold and recommends the highest-recall threshold inside both budgets (default ≤ 1 per 8 h, ≤ 1% soundalikes). It says plainly when the room audio is too short to *certify* the budget (≈ 30 h needed for 1 per 8 h). `--apply` writes `kws_threshold` (then `fingerprint-models`).

## C2 — label by ear
`scripts/wake_audition.py`: `segment` cuts `free/` recordings into single utterances (energy based, clicks dropped, ≤ 4 s pieces); `review` plays each clip and moves it to `positive/` or `negative/` on your keypress (`audition_NNNN.wav`, log in `audition_log.jsonl`, nothing deleted); `stats`. Whisper is deliberately not involved (it poisoned 136 real recordings on 2026-10-01).

## C3 — training recordings, kept apart from the test set
`voice_eval_set.py --purpose train …` runs the same guided protocol but writes `trn-…` ids, `split="train"`, into `wake_word_data/positive|negative/` + `manifest.jsonl` (the layout the trainer and `wake_data_foundation.py validate` read). `validate` refuses a `trn-` row in the held-out set and vice versa. Mining hard negatives from the rotating `verify_NN.wav`/`capture_NN.wav` debug dumps was **not** built: they are private audio and need an explicit opt-in design; the audition tool covers the manual path.

## C5 — iterate
Runbook §3–§5: score → mine failures → retrain → score the candidate with `--baseline` → promote only if no wake row regresses.

## D — STT polish
- **Learned corrections**: whole-word, case-insensitive on both the test and the replacement (previously `includes()` was case-sensitive but `replace` was `/gi`, and "in"→"inn" would rewrite "painting"); the learned `context_before` word is honoured; the applied corrections are counted (`hits`, `last_hit` in `learned_corrections.json`, new `record_correction_hit` command; old files load unchanged). +4 vitest, +1 Rust.
- **Filter statistics**: `stt_filter_stats` counted an already-empty STT reply as a filter hit, inflating the confabulation rate; hits are now only non-empty → empty, and `empty_raw_count()` reports the rest.
- **turbo vs large**: `stt_eval --engine groq-large` runs `whisper-large-v3` through the identical path so the scorecard shows what the faster production model costs (evaluation only; the app is unchanged).
- **Prompt window**: both decoder prompts are unit-tested to stay far under Whisper's 224-token limit (change 86).

## Verify
Rust lib 971/971 serial; vitest 168/168 (+ the pre-existing unloadable `identityBanner.test.ts`); Python `test_voice_eval_tools` 37, `test_wake_dsp` 3, `test_wake_train_lib` 14; `wake_eval`/`stt_eval` build in release; `wake_eval --selftest --sweep` runs on the real engine for both front-end chains.
