# Change 85 — Local-engine filter + measurable Groq confidence gate (Phase B10)

## 1. The local fallback was unfiltered (fix, always on)

`stt::transcribe_audio`, `transcribe_samples` and `transcribe_samples_verbose` filtered Groq output with `filter_transcript_counted` but returned the Moonshine/whisper sidecar's text **raw** (the audit assumed it got "only the text filter"; it got none). A user without a Groq key, in Local-only mode, or with Groq down therefore fed "Thank you.", looped phrases and foreign confabulations straight into the command parser. All three local paths now go through `finish_local` (same filter, same SLO counters, same turn marker). The offline evaluator still calls `transcribe_local` directly, so its "raw" column stays raw.

## 2. A confidence signal from Groq, built to be measured before it is trusted (off by default)

- `GroqSegment` now carries `compression_ratio`. `stt_groq::confidence_rejects(segments)`: reject when **every** segment's `avg_logprob` is below −1.2, or **any** segment's `compression_ratio` is above 2.6; no segment data = never reject. (`no_speech_prob` is not used: it is 0.000 on every `whisper-large-v3-turbo` segment.)
- New setting **`sttConfidenceGate`** (default `false`; toggle "Reject low-confidence speech" in Command Hub → STT Provider). Off = the request and behaviour are exactly as before. On = the turn asks for `verbose_json` and an unreliable transcript becomes empty (the existing "didn't catch that" retry).
- The thresholds are placeholders at Whisper's reference values with margin; **do not enable it on faith.** `stt_eval` now records `min/max_avg_logprob`, `max_compression_ratio`, `max_no_speech_prob`, `n_segments`, `gate_rejects` for every Groq row, and `voice_scorecard.py` prints a **Confidence gate calibration** table: for each threshold, hallucinations still caught (silence/noise clips that survived the filter) vs real commands wrongly rejected, plus a "Safe to enable" / "Do not enable" verdict at the same ≤ 0.5% wrongful-rejection budget as the filter.

## 3. Fix to Phase A: the two evaluator examples were never committed

`.gitignore` ignored all of `src-tauri/examples/` (a pre-existing rule), so `wake_eval.rs` and `stt_eval.rs` from change 79 existed only in the working tree — a Windows checkout of the branch would not have had them. `.gitignore` now un-ignores exactly those two files and they are committed with this change. (Other files under `examples/` stay ignored.)

## Tests

`stt::test_local_output_is_filtered`, `stt_groq::test_confidence_rejects`, `voice_eval::test_seg_stats_summarises_segments`, 4 Python tests for the sweep/recommendation/report. Rust lib 955/955 serial; Python 25/25; frontend 164/164 (+ the pre-existing unloadable `identityBanner.test.ts`); both examples build in release.

## To decide whether to enable it

Run `stt_eval --engine groq` on your recorded set, then `voice_scorecard.py --stt stt.jsonl`. Enable the toggle only if the table says "Safe to enable"; if it recommends different numbers, change the two constants in `stt_groq.rs` first.
