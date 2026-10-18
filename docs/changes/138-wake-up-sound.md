# 138 - Wake-up sound ("NEXUS heard you")

A short sound plays the instant NEXUS wakes (wake word or Ctrl+Space), so you know it is listening without looking at the screen. On by default; switch, sound choice and level are in Command Hub > Audio > Wake Sound.

## Sounds (all synthesized, `src-tauri/src/wake_cue.rs`)

| Key | What | Length |
|---|---|---|
| `rise` (default) | two soft bells, G5 then D6 (rising fifth) | 190 ms |
| `glass` | one bell, 1568 Hz | 230 ms |
| `pluck` | short warm note, 880 Hz | 130 ms |
| `triplet` | C6 E6 G6 quick | 170 ms |
| `sweep` | glide 600 -> 1800 Hz into a bell | 210 ms |

Mono, 48 kHz, peak 0.5 (-6 dBFS) before the user's level, 5 ms raised-cosine attack, 6 ms fade-out, over 80 % of the energy in 0.6-4 kHz (tested). The design follows Google's earcon guidance (short, few, consistent), the 1-4 kHz hearing-sensitivity range and Brewster's distinctness rules; none of those sources give exact milliseconds, so the lengths are design choices. To listen outside the app: `cargo test --lib write_candidate_wavs -- --ignored` writes `src-tauri/target/wake_cue_candidates/*.wav`.

## Behaviour

* **Plays:** at the wake-word fire (single choke point in `wakeword_oww.rs`, before capture starts) and Ctrl+Space from idle (`hotkey.rs`).
* **Silent:** cancel presses, Ghost hot-mic turns, empty-transcript retries, confirm/offer/clarification listening windows (they use the frontend `start_stt_capture` IPC, which does not cue), meetings (same mute as speech), setting off. A test pins the wiring.
* **Own output stream** opened on demand and closed 20 s after the last cue (a permanently open stream would keep waking the CPU); opened early at the wake candidate (`prewarm`) so a verified wake does not wait for the device. It does not use the TTS generation counter, `set_tts_playing`, the volume lease or keep-awake.
* **Echo guard:** there is no echo cancellation, so while the cue plays (+120 ms tail) the capture loop does not count chunks as voiced speech; audio is still buffered, so a fast first word is not clipped.
* Settings `wakeSound` (true), `wakeSoundStyle` ("rise"), `wakeSoundVolume` (70) in `NexusSettings`; IPC `preview_wake_sound(style, volume)`.

## Verify

10 unit tests (length, level, click-free attack/release, spectral band, distinctness, WAV round trip, parsing, settings, echo window and tail, meeting/switch rule, wiring pin); 5 mutations (window predicate, meeting rule, echo tail, attack ramp, peak level) each fail a test (the peak one survived first because the test compared the constant to itself; now a literal). Frontend: `wakeSounds.test.ts`.

## Not verified (needs your ears and a rebuild)

* That it is audible and pleasant on your speakers / headset, and which candidate you prefer (change the default after listening).
* **Echo/clipping trade-off** with a real mic: say "Nexus, open Notepad" right away; the transcript must not contain the cue and the first word must not be clipped. The tail is `ECHO_TAIL_MS`.
* **Latency:** the log line `[CUE] queued -> audio handed to device in N ms` and `[CUE] output opened in N ms`. Not measured.
* **Bluetooth headsets** often swallow the first 100-300 ms after idle; the cue may be clipped.
* **Low system volume** makes the cue quiet; it does not take the TTS volume lease (deliberate, v1).
* Also changed: the smart-turn timing test now allows 5 s (it measured 2.2 s inside the full lib run, ~50 ms alone, because earlier tests leave ONNX threads spinning).
