# Live captions: what was measured, what was fixed, and where the stars stand (2026-11-24)

Goal from the owner: make "live captions while you speak" more than 4 stars today. Result: **3.0 to 3.5 stars (60.8 % to about 69.6 %)**, not yet 4. The gap to 4.0 is now one thing nothing in an empty room can supply: a person speaking into the real microphone (about 10 minutes). This document records the measurements, the defects found and fixed, the scoring rule used, and exactly what is left.

Method: verification before plan. An Opus 5.5 advisor (read-only) audited the code first; I checked each of its findings in the code before acting on it, then measured the real engine.

## 1. What was measured (real engine, synthetic speech, no microphone)

Speech: Windows' own voices (`scripts/make-test-speech.ps1`) rendered to 16 kHz mono WAV with the start time of every word. Engine: the real local Moonshine streaming server (`server/stt_server.py`, model `small_streaming`) that NEXUS uses when no Deepgram or Groq key is set or in privacy mode. Two harnesses:

* `scripts/measure-live-captions.py`: sends the audio to the server's `/stream` at real speaking pace (100 ms per 100 ms) and records when each caption update arrives.
* `scripts/test-live-captions-e2e.mjs`: the **whole app chain** in a debug build of NEXUS on its own hidden desktop: the speech is played through the real microphone callback by a debug-only hook (`NEXUS_FAKE_MIC_DIR`, `src-tauri/src/debug_mic.rs`, not compiled into a release build), a real capture is started, and the stage page records every `stt:partial` event and samples the caption pill's text every 50 ms.

| | Short (2.1 s) | Normal (5.2 s) | Long (8.2 s) |
|---|---|---|---|
| Server alone: first caption after speech began | 485 ms | 453 ms | 485 ms |
| Whole app: first caption | 630 to 663 ms | 650 to 668 ms | 641 to 678 ms |
| Whole app: median lag of a word after it began to be spoken | 762 to 803 ms | 808 to 824 ms | 752 to 996 ms |
| Whole app: worst word | 888 ms | 1207 ms | 1671 ms |
| Final caption word error rate (server alone) | 0 | 0 | 0.071 (the same as the batch result: "5:30" for "five thirty") |
| Confirmed words changed later | 0 | 0 | 0 |

Reading: words begin to show about half a second after speech starts and trail the speaker by roughly 0.4 to 0.5 s after each word ends. That is close to commercial streaming dictation but not at its 300 ms. Every run reached the final transcript and every run showed the live text **in the page while the speech was still playing**. The captions match the batch transcript the command is run from.

Honest limits of this evidence: the speech is clean, synthetic and in one voice; the hook replaces the microphone's samples, so the device, driver, gain control and room are not exercised; a hidden desktop proves the text is in the page, not that pixels were painted; only the **local** source was run (no Deepgram or Groq key was used, so those two paths are covered by tests with stand-in servers only).

## 2. Defects found (Opus audit, each checked in the code) and what was done

| Finding | Verified | Fix |
|---|---|---|
| Caption text jumped sideways: the pill keeps the last 5 words and typed on from the old CHARACTER COUNT | yes (`useTypedText.ts`) | `typingStart` keeps what the old and new text share (common start, or a word-aligned overlap); typing cap 1.2 s to 0.5 s |
| The local server sent only the current line, so the caption dropped everything before a pause | yes (`stt_server.py`) | all lines kept by id, in order, joined |
| The first words never reached the caption: the seed audio was not fed, and chunks before the plan was decided were dropped | yes (`wakeword_oww.rs`, `stt_stream.rs`) | a bounded 4 s pre-roll sent first and in order; the seed is fed |
| The model ran on the server's event loop and could hold up the final transcript | yes (`stt_server.py`) | `add_audio` and the final flush run in a thread |
| Early guesses are revised ("link in" became "best pizza", "video" became "browser", "5am" became "5:30") and look like flicker | measured | words are shown at once; the **not yet confirmed tail is drawn dimmer** (a word is confirmed when two hypotheses in a row agree). Showing only confirmed words was simulated first and **rejected**: it removed all retractions but doubled the lag (median 665 ms to 1425 ms) |
| The Groq request had no test because its address was a constant | yes | the address can be pointed at a stand-in server in tests; 3 tests |
| Cold start: the first use downloads the Moonshine model (about 250 MB); on this machine it took more than 10 minutes at 100 kB/s | measured | **not fixed** (see section 5) |
| The last caption before the end of a short command can still be a wrong guess ("open the connection" for "open the command hub") because the engine's end-of-speech flush is dropped on purpose | measured | **not fixed**: a late caption could overwrite the final text; the final transcript replaces it about a second later and the unconfirmed tail is dimmed |

One test artefact worth knowing: the e2e test speaks real commands, so the app really answers aloud, and captions are off while NEXUS speaks (by design). The test waits for that.

## 3. The scoring rule used

Stars = score / 20 to the nearest half; score = 0.20 Built + 0.20 Tested + 0.30 Live-proven + 0.15 Safety + 0.15 Industry (`docs/testing/scorecard-data.json`).

Rule for Live-proven (proposed by the advisor, adopted here): real engine on real-time audio, measured, 20; whole app chain with the pill text recorded during playback and timings measured, +15; each extra source proven with a real key, +10 each; **ceiling without a person at a real microphone: 50**; one owner session on the real microphone (10+ commands, numbers kept), +20; days of real use, +10 to 15.

| | Built | Tested | Live | Safety | Industry | Score | Stars |
|---|---|---|---|---|---|---|---|
| Before | 90 | 75 | 20 | 85 | 60 | 60.8 | 3.0 |
| Now | 95 | 88 | 35 | 85 | 65 | **69.6** | **3.5** |

* Built 95: the defects above are fixed.
* Tested 88: new tests for every fix (Groq request 3, pre-roll 3, stability 1, typing 5 and 4, fake microphone 2), 10 Rust mutations all killed (one by hand; the typing and confirmed-word functions in the frontend are covered by tests but were not mutation-checked), an end to end script. Not 90+: the pill is still tested as a model plus a source scan, not by rendering.
* Live 35: the first two rows of the rule. **Not 50**: no person, no real microphone, one source, clean synthetic speech.
* Industry 65 only: the default source for anyone with a Groq key is Groq, which shows text in steps 1.5 to 2.5 s after speech starts. The local numbers do not count for the default source.

## 4. What it takes to cross 4.0 (75 %) and 4.5 (85 %)

* **4.0:** one owner session on the real microphone with the local source and 10 or more commands (about 10 minutes, checklist in section 6): Live 35 to 55 gives 69.6 to 75.6. Or, with no person: make the warm local server the preferred source (Industry 65 to 75: +1.5) and handle the cold start; that alone is 71.1 and still short.
* **4.5:** needs Live near 80 (real use plus Groq and Deepgram each proven with a real key) and Industry near 80 (first word within 400 ms and word-by-word growth on the default source). Not reachable today without the owner and keys.

## 5. Next work, ranked (not done)

1. **Prefer the local server when it is already warm** (no quota, private, true streaming), cached so choosing the source never makes a network call.
2. **Cold start:** never download the model on the caption path; show "downloading" in the Command Hub with progress, and use Groq or nothing until it is there. Add `model_loaded` to `/health`.
3. **Use the end-of-speech flush** safely: hold the final transcript for it or ignore it once the final has arrived.
4. **Earlier Groq captions** (first request at 1.0 s while the day's count is low): trades quota for speed, do last.
5. A rendered pill test (not only the model).

## 6. Owner checklist (about 10 minutes, real microphone, local source)

1. Start the local speech server (it starts by itself the first time; the first run downloads the model, let it finish) and turn on "Live captions while you speak" in the Command Hub, Features, Voice. No Deepgram key, Groq key optional (the local server is used only when there is no key or in privacy mode: use **Local speech recognition only** for this test).
2. Say ten different commands, short and long, including numbers ("remind me at five thirty") and one with a pause in the middle.
3. For each: do words appear while you are still speaking, does the dim tail settle, does the final line match what you said, did a pause make the text shrink?
4. Write down the count that went wrong; that is the number that moves Live from 35 to the 55 to 70 range.
