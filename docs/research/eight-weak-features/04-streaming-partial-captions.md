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
