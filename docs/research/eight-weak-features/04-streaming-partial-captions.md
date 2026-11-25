# 4. Streaming partial captions (live words while you speak)

Scorecard before: Built 80, Tested 25, Live 20, Safety 60, Industry 60 (2.5 stars). Scorecard note: "Optional, needs a key; stream module has no tests."

## What it is

While you are still speaking, the words so far appear in the speech pill (`SpeechPill.tsx`, event `stt:partial`). The text comes from a live speech stream: Deepgram Nova-2 in the cloud (needs a key) or the local speech server's `/stream`.

## Verification (what was found)

Baseline: 4 Deepgram tests, 0 tests for `stt_stream.rs`. All pass; none exercise the stream.

| # | Finding | Evidence | Severity |
|---|---|---|---|
| 1 | **The feature never ran.** The three stream commands were called only from the browser recorder's `startRecording`, reached only through `captureUntilSilence`, which has **no callers**. The real microphone path is native Rust (`wakeword_oww.rs`), which never fed the stream. No partial was ever produced | `recorder.ts:440,568` (callers: none); the IPC gate had listed these commands as one-sided | High (the feature is absent) |
| 2 | The stream **ignored `localSttOnly`** (privacy mode). With a Deepgram key it would have sent live audio to the cloud while the batch path honoured the setting | `stt_stream.rs:31-34`; `stt.rs` reads the setting at 366/496/598 | High, privacy |
| 3 | A **late message from a closed stream** could arrive in the next turn (no generation id), and a second start replaced the socket without closing the first, so both kept emitting | `stt_stream.rs:41,79` | Medium |
| 4 | Deepgram sends a separate message per segment; only the **latest segment** was shown, so a long sentence lost its start | `stt_stream.rs:49-60` (no `is_final` handling) | Medium |
| 5 | Stop sent the local server's protocol (`{"cmd":"stop"}`), not Deepgram's `{"type":"CloseStream"}` | `stt_stream.rs:125-126` | Low |
| 6 | A partial that arrived after the final left the pill up for ever (no hide timer) | `SpeechPill.tsx:112-119` | Medium |
| 7 | Partials skipped the hallucination filter and the directed gate: in a Ghost hot-mic session TV audio or NEXUS's own voice would have been shown | design | Medium |
| 8 | The batch request used `keyterm=` with `model=nova-2`; Deepgram documents `keyterm` for Nova-3 and Flux only (Nova-2 uses `keywords`). The "+30-40 %" claim in the comment was never measured, and the unit test pinned the questionable parameter | `stt_deepgram.rs`, docs.deepgram.com keyterm / keywords pages | Low to medium |
| 9 | No connect timeout; no KeepAlive (Deepgram closes after about 10 s without audio) | `stt_stream.rs` | Low |

## What was built

A rewrite of `stt_stream.rs`; the browser path is removed; the native capture feeds it.

* **Native feed.** `wakeword_oww.rs` calls `stt_stream::begin()` when a capture starts, `feed(&chunk)` right after the same samples are appended to the batch buffer (so the stream hears exactly what the batch STT will), and `end()` at the endpoint and on abort. In the audio callback `feed` is one atomic read when off and a `try_send` when on: it never waits, never locks, and a full queue drops chunks and counts them.
* **One dispatcher task owns the socket.** One stream per turn, identified by a generation; a second start closes the first socket; `end()` stops emission at once even if its queue entry cannot be queued.
* **Plan, decided in Rust per turn (`plan_for`).** Off unless `sttLiveCaptions` is on (**default off**, new setting). Off in a meeting, in a Ghost hot-mic session, and while NEXUS is speaking. In privacy mode (`localSttOnly`) or with no Deepgram key it uses only the local server, **never the cloud**; the cloud configuration is not even built.
* **Text.** Every finished segment plus the current unfinished one (`Assembler`). Deepgram gets `{"type":"CloseStream"}`, the local server `{"cmd":"stop"}`. Connect has a 3 s timeout, sends a 400 ms one.
* **Pill.** A partial hides after 6 s if no final follows. The dead browser-recorder calls and the three IPC commands are removed (the IPC gate went from 273 to 270 commands).
* **Deepgram batch URL** now sends `keywords=` (the Nova-2 parameter). The test now asserts the model and the parameter match and says to change both together.

## Verification of the fix

`stt_stream::tests` (13): privacy mode never selects the cloud even with a key; off unless enabled, never in a meeting / Ghost hot mic / over its own voice; close messages; PCM conversion and clipping; a long sentence shows every finished segment; and, against **a real WebSocket server on localhost**: a full turn streams exactly the 3 chunks sent and ends with `CloseStream`, nothing connects when the plan is off, the cloud configuration is never built in privacy mode, a second start closes the first socket and the old turn's late text is never shown, nothing is emitted after the turn ends, a full queue drops and counts without blocking, and the capture hooks exist in the native path right after the batch buffer append. Plus 2 frontend tests (stale timer, no browser streaming). Eleven mutations are in the index.

## Not done, and why

* **No live Deepgram or local-server stream was run.** It needs the owner's key (and sends their microphone audio to a third party). The mock server proves the protocol handling and the privacy gating, not Deepgram's real message format beyond the documented shape. Whether `keywords=` helps on the owner's voice needs a before and after run with the owner's key.
* **No Hub switch yet.** `sttLiveCaptions` is read from `settings.json` and survives Save (it is in `NexusSettings`), but there is no toggle in the Command Hub. Until the owner has seen it work it stays an off-by-default setting.
* The seeded audio after the wake word is not streamed (the first fraction of a second of a command may be missing from the live text; the final transcript is unaffected).
* KeepAlive is not sent (audio flows continuously while a capture runs, so Deepgram's 10 s idle close cannot occur during one).
* Partials are not passed through the hallucination filter. In Ghost hot-mic sessions they are off entirely; in ordinary turns the final transcript still goes through the filter.

## Owner checklist

1. Set `"sttLiveCaptions": true` in `settings.json` (restart). Without a Deepgram key it uses the local stream: say a sentence after the wake word and watch the pill grow. Log: `stt_stream:` debug lines only; no network.
2. Add a Deepgram key, `localSttOnly` false: the same, now through Deepgram. Check the Deepgram usage page: one connection per turn.
3. Set `localSttOnly` true with the key still present. Expected: the pill still works (local stream) and Deepgram's usage page shows **no** new connections.
4. Start a meeting call: no partials. Ghost mode: no partials.

## Follow-up 2026-11-24 (owner report: "a lot of delay between speaking and the caption")

**Why there was a delay.** Without live captions the pill shows nothing while you speak. The words appear only after the whole sentence is finished and transcribed: end of speech, then the endpoint silence (400 ms, 800 ms in "patient" mode, up to a 3 s hold if the sentence looks unfinished), then the Groq round trip (about 300 ms; the one capture in the current log shows 306 ms). For a 4 second sentence nothing is on screen for 4 seconds and then everything appears. That is the delay, and it is structural, not a bug in one line.

**Why the first version of live captions would not have fixed it on this machine.** Checked here: no Deepgram key is set, and the local speech server is not running (it is shut down 60 seconds after use to keep the RAM under the 150 MB idle limit; `lazy_stt.rs`, `STT_KEEP_ALIVE = false`). So the Deepgram stream had no key and the local stream had nothing to connect to.

**What was built.**
* **A Groq progressive source** (`Plan::Groq`): with a Groq key, no Deepgram key and privacy mode off, while you speak the audio so far is sent to Groq (the same service and key the final transcript already uses, so no new party hears you) and the answer replaces the pill text. First request after 0.9 s of audio, then at most every 1.2 s, only with 0.6 s of new audio, one at a time, at most 6 per command, at most 10 per minute across commands (Groq's free tier is 20 per minute and the final transcript needs the rest), only the last 12 s of audio, never for silence or room noise (peak under 700), silence hallucinations filtered with the same filter as the final transcript. An error (for example a 429) stops it for that command and for a minute. The final transcript is untouched.
* **The Command Hub switch**: Voice, "Live captions while you speak" (Features page and the Audio tab), off by default, marked as a cloud-using feature, with the quota cost stated. `sttLiveCaptions` was also failing the feature-registry gate (`scripts/check-feature-registry.mjs`); that gate now passes.
* Source order: Deepgram stream (real streaming) if a Deepgram key exists, else Groq progressive if a Groq key exists, else the local server. Privacy mode (`localSttOnly`) always means the local server only.

**Expected effect (not measured live).** The caption should lag your voice by about one to one and a half seconds, in steps, instead of appearing after the sentence ends. It will not be word-by-word smooth: Groq has no streaming. For smooth word-by-word text the options are a Deepgram key, or keeping the local server warm (about 300 MB of RAM, which breaks the RAM rule).

**Cost.** About 4 extra Groq requests for a 5 second command. The free tier is 2,000 requests a day, so roughly 330 commands a day with captions on instead of roughly 2,000 without.

**Verification.** 11 new tests in `stt_stream::tests` with a fake transcriber (waits for enough audio; sends everything so far, not just the new part; no new audio, no request; one at a time; per-turn cap; silence never sent; error stops the turn and blocks the budget; exhausted budget means no request; silence hallucination filtered; a finished turn shows nothing; Stop ends the task; only the recent window is sent) plus the plan and budget rules, 1 registry test. Mutation checks: 10 rules, 9 killed first time; the survivor was two guards backing each other up (the turn flag and the budget block), re-checked together and killed. Records: `docs/testing/runs/eight-weak-features-mutations-c*.json`.

**Owner checklist.**
1. Command Hub, Features, Voice, switch on "Live captions while you speak" (restart not needed; read on every command).
2. Say a 5 to 8 word command. Expected: text appears about a second after you start, grows in steps, and the final text replaces it.
3. Check the Groq console: about 4 extra requests per command.
4. Switch "Local speech recognition only" on: expected, no captions from Groq (the local server is needed).
5. If the text lags more than 2 seconds or shows junk, tell me the numbers in the log (`stt-groq: transcript ... in Nms`).

## Cost reduction (2026-11-24, after an Opus 5.5 advisor review; supersedes the numbers above)

**Measured, not guessed.** `stt_stream::tests::measure_cost_and_lag` (ignored; `cargo test --lib stt_stream::tests::measure_cost_and_lag -- --ignored --nocapture`) replays utterances in real time through the real scheduling code with a recogniser that answers in 300 ms (Groq's typical figure):

| spoken | requests, first version | requests, now | audio sent, now | first caption |
|---|---|---|---|---|
| 1.0 s ("open whatsapp") | 1 | **0** | 0 s | none needed |
| 1.5 s | 1 (answer arrived after the end: wasted) | **0** | 0 s | none needed |
| 2.0 s | 2 | **1** | 1.6 s | about 2.1 s |
| 3.0 s | 3 | **2** | 4.6 s | about 2.1 s |
| 5.0 s | 4 | **3** | 8.9 s | about 2.1 s |
| 8.0 s | 6 | **4** | 14.5 s | about 2.1 s |

**What the advisor found (read only, it also read Groq's pages).**
* Groq bills a **minimum of 10 seconds per request**, so shorter audio chunks save nothing and the "cumulative resend multiplies audio-seconds" worry was overstated for paid keys: on a paid key an interim costs well under a hundredth of a cent. On the free tier the constraint is **requests per day**, not seconds.
* Groq's rate-limits page lists `whisper-large-v3` and `whisper-large-v3-turbo` as separate rows with the same numbers, but also says limits apply at the organisation level. **Whether the two models have separate free-tier allowances is not verified**; it needs your own limits page.
* A speculative final (reuse the last interim as the final) is **not safe** as built: interim uses neutral decoding, the final uses the owner or entity vocabulary prompt and the confidence gate, plain `json` has no `avg_logprob`, and the admin-privilege proof path never saw the interim. Only a request made with exactly the final's function and prompt could be reused. Not built.

**What was changed (the advisor's items 2, 3, 4, 5).**
1. **Fewer, later requests.** First request after **1.6 s** of audio (was 0.9 s), then at most every **1.5 s** with 0.8 s of new audio, at most **4 per command** (was 6). A one-breath command costs nothing.
2. **No request while the speaker has just stopped.** If the last 200 ms are quiet the final transcript is about to be asked for; an interim then would duplicate it and could still be in flight when the turn ends.
3. **A different Groq model for captions** (`whisper-large-v3`, the larger one): if the free-tier allowance is per model, captions no longer spend the final transcript's turbo allowance. If it turns out to be per organisation this changes nothing except a slightly slower answer.
4. **Budget:** at most **8 per minute** across commands (was 10), a **daily cap of 600**, a **2-minute** pause after any error (was 1).
5. **Counted separately.** Caption requests are counted as `stt_interim` and are **not** added to the Command Hub's "requests today" (they had been inflating it two to three times). The rate-limit error text no longer hard-codes "2,000 req/day".

**Not done (and why).** Local on-device recognition for captions (Vosk, sherpa-onnx, Windows speech): each is either a sidecar of about 300 MB (breaks the RAM rule), sends audio to Microsoft (privacy), or is unverified here. Stopping captions when the stage is hidden, and reading Groq's remaining-requests header for an adaptive budget, were left because their effect is unverified for audio endpoints.

**Cost now, in the owner's terms.** Free tier: a typical 3 s long command spends 1 final plus 2 caption requests; a 5 s command 1 plus 3. With captions on, expect roughly 2 to 4 times the requests of captions off for long commands, none for short ones, and never more than 600 caption requests a day. A paid key: negligible (the 10 s minimum makes each interim cost about the same as 10 s of audio, under a hundredth of a cent).

**Whole-chain test.** `a_real_recording_goes_through_the_whole_chain_and_ends_cleanly` feeds the project's real "open whatsapp" recording in 80 ms chunks through the microphone-side handle, the dispatcher, the plan, the progressive session and the emit, then ends the turn and starts a second one.

**What to test yourself (10 minutes).**
1. Features, Voice, "Live captions while you speak" on.
2. Short command ("open notepad"): no caption appears and the Groq console shows only the normal request.
3. Long command (6 to 8 words, a pause-free sentence of about 4 seconds): a caption appears about 2 seconds in, then updates once or twice; the final text replaces it.
4. In the Command Hub the "requests today" figure goes up by 1 per command, not 3.
5. `usage_stats.json` in the app data folder shows `stt_interim` counts separately.
6. Look at your Groq limits page: do `whisper-large-v3` and `whisper-large-v3-turbo` show separate daily counters? Tell me what you see.

**Verification of the cost cuts.** Rust 3531 passed, frontend 1283 passed, all gates pass, family build compiles. Mutation checks for the caption rules (specs `scripts/mutations-eight-weak-features-b/c/c2/d.json`, records in `docs/testing/runs/`): every rule is caught. Three first-try survivors were guards that back each other up (the speaker-stopped gate and the quiet-audio gate; the per-turn failure flag and the budget block); each was re-checked by removing both together, and the speaker-stopped gate got a stronger test (one block with loud audio and a quiet tail) after its mutation survived. One process note: an earlier mutation run was started twice by mistake, the two runs raced on the same file and the killed run left two mutations in the source; they were found by checking that every mutation target was present, restored by hand, and the whole stream test file re-run before this commit.
