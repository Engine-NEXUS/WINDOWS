# Change 87 — "NEXUS open Chrome" in one breath keeps "open Chrome" (Phase B2)

**Defect (audit #1):** the command capture started empty *after* the wake word fired. Everything the user said between the detection point and the moment the capture actually began was lost: the receiver-thread hop, the barge-in/TTS cut, and — when stage-2 verification is on — 0.25 to several seconds of STT round-trip. A one-breath "NEXUS open Chrome" therefore reached STT as "…Chrome", or as nothing.

The obvious fix (seed the capture from the verify ring) was tried and removed on 2026-09-25: seeding *pre-detection* audio fed Groq "NEXUS + silence + command" and it hallucinated. This change keeps that lesson: the seed starts at the **detection point**, never before it, unless you explicitly choose a negative offset.

## How it works

- Every chunk pushed into the 2.5 s verify ring bumps a monotonic `RING_TOTAL`. A wake candidate records `ring_end` = the stream position of the end of the detecting chunk (snapshotted under the ring lock together with its audio); the stage-2 self-send carries the original `ring_end`.
- At fire time `start_stt_capture_seeded(offset, ring_end)` takes the ring lock (the same lock the audio callback takes before touching the capture), copies the audio from `ring_end + offset` to "now" into the capture buffer, **replays it through the same speech/silence tracker a live chunk gets** (`capture_vad_update`, extracted unchanged from the audio callback), then starts capturing. A command already finished while verifying ends on the next chunk instead of waiting out the 8 s no-speech timeout. `SEED_UPTO` makes the callback skip chunks the seed already contains, so nothing is lost or doubled.
- Unknown position (`ring_end = 0`: barge-in candidates, hotkey) = the old fresh-start behaviour. The hotkey path is untouched.
- New setting **`wakeSeedOffsetMs`** (default **0**, clamped ±400): shifts the capture start relative to the detection point. Negative values reach back into the tail of the wake word — only use one if the offline sweep below shows it helps.

## Pick the offset from data, not by guessing

`stt_eval --capture-offset-ms N` replaces each *one-breath* row ("nexus open chrome") by only the audio the capture would receive: the real wake engine decides when it fires, the clip is cut at `fire + 80 ms + N`. A wake miss counts as a failure. The scorecard row "NEXUS open Chrome in one breath" then says *(wake + capture simulated, offset N ms)* and includes capture loss, which the old whole-clip transcription could not see. Sweep it:

```powershell
foreach ($o in -320,-160,0,80) {
  cargo run --release --example stt_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --kinds command --capture-offset-ms $o --out ../stt_off$o.jsonl
  python ../scripts/voice_scorecard.py --stt ../stt_off$o.jsonl --json ../off$o.json
}
```
Keep 0 unless a negative offset raises the one-breath rate **without** lowering "Understands the command" or raising the hallucination row. Then set `wakeSeedOffsetMs` in `%APPDATA%\com.nexus.assistant\settings.json`.

## What this does and does not fix

- Fixes: audio lost after detection (thread hop, TTS cut, verification wait). Biggest with `verifyWake` on or a slow machine.
- Does not fix: speech that overlaps the *wake word's own detection latency* (the model fires ~80–240 ms after the word ends; an immediate "open" can start inside that gap). Only a negative offset can recover that, at the risk of including the word's tail — that is exactly what the sweep decides.
- Not testable offline: the live audio callback timing. The pure parts (`seed_slice`, the seeded-capture state, `capture_start_sample`) are unit-tested; verify live by saying "NEXUS open Chrome" with no pause, with `verifyWake` on and off.

## Tests (+3 Rust, +1 Python)

`test_seed_slice` (ring bounds, offsets, unknown/future positions), `test_seeded_capture_keeps_post_detection_speech` (only post-detection audio, no wake-word samples, speech tracker primed, `ring_end = 0` falls back), `test_capture_start_sample`, Python `test_one_breath_label_reflects_capture_simulation`. Rust lib 960/960 serial; Python 26/26; frontend 164/164 (+ pre-existing unloadable test); both examples build in release.
