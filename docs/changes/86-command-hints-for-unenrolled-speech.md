# Change 86 — Generic command hints for non-owner turns, A/B-gated (Phase B5)

**Gap (audit #5):** the private command vocabulary (`NEXUS_VOCABULARY`) reaches Groq only for owner-*verified* turns. Speaker verification is off by default, so nearly every real turn is `Unenrolled` and decodes with a neutral prompt — "open chrome"-class phrases get no help.

**Why not just send the owner vocabulary to everyone:** it contains private project and people names (Servx, Zync, Eesha, Lakshya, …) that would bias a stranger's speech toward the developer's repos, and any decoder prompt can make Whisper *invent* prompt words on silence/noise (measured on 2026-09-18: a "Hey Nexus" prompt flipped 2 of 4 false-alarm segments into an exact "Nexus." reading).

## What was built

- `stt_groq::COMMAND_VOCABULARY`: a short generic prompt (NEXUS, open Chrome/WhatsApp/Spotify/settings, new tab, search for, play/pause/next song, stop/cancel, ghost mode, list pull requests, send a WhatsApp message). No private names; unit-tested to stay far under Whisper's 224-token prompt window.
- `decoder_bias_for_turn(ownership, command_hints)` (pure) is now the single decision: **Verified → owner prompt (unchanged); Unenrolled + `sttCommandHints` on → generic command prompt; everything else (hints off, Uncertain, Rejected) → neutral.** A different voice never receives a bias, setting or not.
- New setting **`sttCommandHints`** (default **off** = byte-for-byte previous behaviour). Toggle: Command Hub → STT Provider → "Command hints for speech". `stt:turn_stats.decoder_bias` now reports `owner` / `commands` / `neutral`.
- Harness: `stt_eval --bias commands|neutral|owner`.

## How to decide (the A/B)

```powershell
cargo run --release --example stt_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --bias neutral  --out ../stt_neutral.jsonl
cargo run --release --example stt_eval -- --manifest ../eval_set/manifest.jsonl --root ../eval_set --bias commands --out ../stt_commands.jsonl
python ../scripts/voice_scorecard.py --stt ../stt_neutral.jsonl  --json ../neutral.json
python ../scripts/voice_scorecard.py --stt ../stt_commands.jsonl --baseline ../neutral.json
```

Turn the setting on only if **all** hold: "Understands the command" and "Word accuracy" go up, and "Hallucinated text on silence / TV noise" does **not** rise. Otherwise leave it off.

## Known inconsistency, left alone on purpose

The legacy frontend-capture path (`transcribe_audio` command, used by `recorder.ts`/`vad.ts` when the Rust capture is not in play) still sends `NEXUS_VOCABULARY` to every user. Changing it would remove the biasing the developer's own setup relies on, with no data yet; revisit after the A/B (it is a one-line switch to `decoder_bias_for_turn`).

## Tests (+2)

`test_decoder_bias_for_turn` (every ownership × setting combination), `test_command_vocabulary_is_generic_and_small`. Rust lib 957/957 serial; frontend 164/164 (+ pre-existing unloadable test); `stt_eval` builds.
