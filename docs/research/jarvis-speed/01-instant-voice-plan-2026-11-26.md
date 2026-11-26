# Making NEXUS feel instant: how "Jarvis" demos work, where NEXUS is, and a plan (2026-11-26)

Research and plan only. No code was changed, nothing was built or run.

## 0. First, what we do NOT know

- **The Instagram reel was not found or watched.** Instagram shows only a few posts without a login. The guessed accounts
  `jarvisevolving.ai`, `jarvis.evolving.ai` and `jarvis_evolving.ai` do not exist. Web searches found no link between
  @evolving.ai (a news page that reposts other people's demos) and a Jarvis product.
- So this document says **nothing** about that demo's speed or accuracy. Every speed number below is either NEXUS's own
  (from the repo) or from a cited public source about voice agents in general.
- To judge that demo, the owner must send the reel link or a screen recording (section 7).

## 1. Plain-language summary

**What makes a demo feel instant.** Five tricks, usually together:
1. **It starts talking before it has the full answer.** A short "On it" or the first few words play while the rest is
   still being made.
2. **It decides you have finished speaking quickly.** Most assistants wait for a fixed silence. Good ones use a small
   model that hears that your sentence is complete and stop waiting.
3. **Simple commands never go to the cloud.** "Open Spotify" is matched on the device in a millisecond.
4. **Everything is already connected and warm** before you speak.
5. **Editing.** Many viral demos are cut, rehearsed or dubbed. Google admitted its own famous Gemini demo video
   had "latency reduced" and was built from still frames and text prompts, with the voice added later
   ([TechCrunch](https://techcrunch.com/2023/12/07/googles-best-gemini-demo-was-faked/),
   [Engadget](https://www.engadget.com/google-admits-that-a-gemini-ai-demo-video-was-staged-055718855.html)).

**Where NEXUS stands (measured, not claimed).** On a machine test in November (a recorded voice, not the owner),
a cloud question reached its first spoken word **0.9 to 1.6 s after the microphone closed**. The real gap from the
end of your speech is about **0.4 s longer**, because the clock starts only after NEXUS has waited 0.4 s of silence.
So today a spoken answer starts roughly **1.3 to 2.0 s** after you stop talking. Before that, the wake word itself
pays a cloud check of about **0.5 s** (median, one real log).

**Understanding is the bigger problem than speed.** On a stress set of 146 command phrasings, the full pipeline got
**51 %** right, **18 %** did the *wrong* thing and **31 %** fell into chat. When online, NEXUS today skips the
local AI classifier entirely, so the online figure is closer to the parser alone: **41 %** right. In one real day,
the wake speech check accepted only **5 of 45** wake triggers. A demo that "understands perfectly" mostly means the
presenter said a few rehearsed sentences.

**Realistic target (estimates, to be confirmed by phase 0).**
- Simple commands: a sound and the action started **within about 0.6 s** of the end of speech.
- Cloud answers: first word **within about 1.0 to 1.2 s**.
- Understanding on the owner's own held-out commands: **at least 90 % right, at most 1 % wrong action**.

**Recommendation: keep NEXUS's own step-by-step pipeline and make each step faster and smarter (option A).**
A cloud "speech-to-speech" model is not faster than a well-tuned NEXUS: the best measured ones take 0.6 to 1.2 s to
first audio ([Artificial Analysis](https://artificialanalysis.ai/speech-to-speech)), Google's free tier uses your
audio to improve its products ([Gemini pricing](https://aistudio.google.com/docs/pricing)), and it would send all
speech to the cloud.

## 2. A turn, today vs target

Times are from the end of the user's speech. "M" = measured on this PC, "C" = code constant, "E" = estimate.

```
TODAY (cloud question)
 wake check ~470 ms (M, before you speak the command)
 you speak ......|
                 |-- silence wait 400 (C) --|-- STT 217-465 (M) --|- parse 14-440 (M) -|-- answer 548-632 (M) --|- voice 31-117 (M) -|> first word
                 0                         400                   ~800                 ~900                    ~1500                 ~1.3-2.0 s

TODAY (simple command, one live turn, 2026-10-17)
                 |-- 400 (C) --|-- STT 638 (M) --|- parse 49 (M) -|-- ack voice 856 (M) --|> "On it sir" ~1.9 s, action time NOT measured

TARGET (simple command)
                 |- end 200-250 (E, smart turn) -|- STT 200-250 (E) -|- parse <5 -|- cached ack + action in parallel <100 (E) -|> ~0.5-0.6 s

TARGET (cloud question)
                 |- 200-250 -|- STT 200-250 -|- first LLM words 300-500 (E) -|- first clause voice 100-300 (E) -|> ~0.9-1.2 s
                 (with speculative start during speech the STT and parse can overlap the end wait)
```

## 3. Evidence table: each stage of a turn today

| Stage | Today | Source | Measured live? |
|---|---|---|---|
| Wake: neural detector | fires on a 80 ms chunk; 1.5 s cool-down between fires | `wakeword_oww.rs:4686-4730` (`LAST_NEURAL_FIRE_MS`) | Code only |
| Wake: speech check (cloud) | on by default (`verifyWake` true, mode "always"); median **0.47 s** per wake; timeout now 3 s (was 10 s) | `commands.rs:3115-3125`, `wakeword_oww.rs:2066, 5144-5151`; log analysis in `docs/research/voice-and-maps/01-...-2026-10-30.md` §1 | Yes, one owner log (6 candidates) |
| Wake: accuracy | speech check accepted **5 of 45** triggers in one day; 38 were ordinary speech; held-out detector recall **63 %** | `docs/research/voice-accuracy/05-mic-listening-diagnosis-and-solution-2026-11-14.md`; AGENTS.md "Wake Word P0" | Yes (real log; recall on held-out owner clips) |
| Wake: owner fast path | **off** by default (skips the cloud check only if voice match ≥ 0.90 and score ≥ 0.70) | `commands.rs:2454, 3583`; `wakeword_oww.rs:5192-5195` | Code only |
| Wake cue sound | 0-15 ms to the audio device | `docs/testing/group-1-live-run-2026-11-11.md` §5.3 | Yes (20 hotkey calls) |
| Capture: end of speech wait | 400 ms silence (800 ms after 2 pauses; 240/480 ms in Ghost) | `wakeword_oww.rs:3128-3140, 3593` | Code constant; the wait itself is not timed |
| Capture: smart end-of-turn | built (Smart Turn), **off** by default | `commands.rs:3142-3149, 4521` | Not proven |
| Microphone front end | 96.5 % of readings silent; driver gates quiet audio | doc 05 (above) | Yes, real log + direct test |
| STT (Groq Whisper turbo, batch upload) | **217, 283, 465 ms** (Nov); **638 ms** (Oct) | group-1 §5.4; `docs/changes/132-...md` addendum | Yes, BOT voice + one live turn |
| STT accuracy | not measured on real speech; 17 % of recogniser outputs were filtered junk in one day | doc 05 | Partly |
| Routing: rule parser | 14-49 ms usually; one 440 ms | group-1 §5.4, change 132 | Yes |
| Routing: accuracy | 146-phrase stress set: parser 41 % right; full pipeline 51 % right, 18 % wrong action, 31 % to chat | `docs/research/command-understanding/01-limits-and-measured-accuracy-2026-11-01.md` | Offline test, my phrase set |
| Routing: online path | a parser miss goes **straight to cloud chat**; the local AI classifier runs only when offline | `orchestrator.rs:1287-1308` | Code |
| Routing: AI gate | the local classifier result is used only at confidence ≥ 0.85 | `nlu_client.rs:51` | Code |
| Cloud answer (Groq gpt-oss) | **448-1,120 ms**; `answer` stage 548-632 ms; no streaming, no tool calling | group-1 §5.5; `router.rs:89-95, 295-325` | Yes, BOT |
| Reply voice (Edge, warm socket) | 31-117 ms after the answer arrives; 290-409 ms for a whole short line alone | group-1 §5.4; `docs/changes/132-...md` | Yes |
| Stock ack voice | **856 ms** from parse to sound (should be near 0; two extra frontend round trips suspected) | change 132 addendum | Yes, one turn |
| Action execution (open app etc.) | not timed anywhere | (no `[LAT]` stage for it) | No |
| Turn clock itself | `[LAT]` starts at capture stop, so it hides the 400 ms silence wait and the wake | `latency.rs:35-40`, `wakeword_oww.rs:1776` | Code |

**Contradictions and older claims to distrust.**
- The 2026-09-25 note "verifyWake defaults to false" is wrong; the code defaults to true (already corrected in AGENTS.md).
- `docs/features/153-...md` says the owner fast path is "default on"; the code reads false when unset.
- "Groq STT ~247 ms" and "9Router ~242 ms" in older notes are not what was measured (217-638 ms and 448-1,120 ms).
- The "full pipeline 51 %" includes the local classifier, which does **not** run when online.
- The `[LAT]` totals (895-1,577 ms) are therefore 0.4 s short of what the user feels.
- Every November speed figure used Windows voices through the speakers, not the owner's voice.

## 4. Understanding gaps: why commands are misunderstood or dropped

| # | Gap (where) | What the user sees | Fix | How we will know |
|---|---|---|---|---|
| U1 | Wake check rejects real wakes and the mic gates quiet speech (doc 05) | "It did not listen" | Tiered check: confident wakes fire at once; start capture in parallel with the check and throw it away on reject; raw capture mode if the driver allows | Wake success rate and ms-to-listening per wake from a `[WAKE]` line |
| U2 | Rule collisions ("close pr 254" becomes close app; "show workflow runs" becomes open app) | Wrong action | Check slots against reality (app exists? PR number?) before acting; fix the known collisions | Wrong-action rate on the held-out set |
| U3 | Online, a parser miss skips the local classifier and goes to chat | "It chatted instead of doing it" | Run the in-process classifier (`nlu_local.rs`) online too; if the sentence looks like a command, ask one short question naming the best two, never chat | Share of command-shaped sentences that reach chat |
| U4 | Classifier gate 0.85 throws away correct answers at 0.65-0.83 | Same | Lower the gate to about 0.6 when all slots resolve and the action is read-only or confirmed | Right-action rate; wrong-action must not rise |
| U5 | Recogniser mishearings ("command hop", "open open open notebook") | Wrong or no action | Owner's app and contact names as decoder hints (already built, off: `sttCommandHints`); learn from "the repeat after a miss" | Word error on owner recordings; misses that are repeated |
| U6 | Open-mic gate and Voice Lock drop turns silently (`directed.rs`) | Nothing happens | Log every drop with a reason the user can see; keep the gate | Count of silent drops per session |
| U7 | Cloud fallback has no command list | Cloud "says" it did something it cannot do | Give the cloud model the command catalogue as tools; it returns a command, which goes through the same local gates and confirmations | Cloud-routed commands that run correctly |

**How to measure accuracy (the held-out command set).**
- The owner records about **200 commands in his own words**, at normal distance and at 1 m, over at least three
  sessions, never reused for tuning. Plus **50 non-commands** (chat, TV, talking to someone else).
- Label each with the expected action and details (app, contact, PR number).
- Pass criteria: right action **≥ 90 %**; wrong action **≤ 1 %**; dropped or asked-again **≤ 5 %**; non-commands that
  trigger an action **0**; wake reached listening for **≥ 95 %** of owner wakes.
- Report with a confidence interval (the repo already has a Wilson-interval scorecard, `scripts/voice_scorecard.py`).

## 5. Architecture options

| | A. Keep the pipeline, optimise it | B. Speech-to-speech realtime with function calling | C. Hybrid (local commands, realtime model for chat) |
|---|---|---|---|
| How | Wake → stream mic → smart end-of-turn → STT → local rules + local classifier → action; cloud LLM only for questions, streamed into Edge voice | Send all mic audio to Gemini Live; it talks back and calls NEXUS functions | A for commands; B only for open conversation |
| First sound, command | ~0.5-0.6 s (E) | 0.6-1.2 s published time to first audio ([AA](https://artificialanalysis.ai/speech-to-speech)) plus the tool round trip | ~0.5-0.6 s |
| First sound, question | ~0.9-1.2 s (E) | 0.6-1.2 s (AA); not measured on this PC | 0.6-1.2 s |
| Accuracy | Ours to measure and control; rules cannot "hallucinate" an action | Strong language understanding; function calling accuracy not published | Mixed; two brains to keep consistent |
| Free tier | Groq STT and LLM limits on the owner's own key (Groq labels its table as "Developer plan" base limits: Whisper turbo 20 requests/min, 2,000/day, 28,800 audio s/day; gpt-oss 1,000 requests/day) ([Groq](https://console.groq.com/docs/rate-limits)); each STT call is billed at least 10 s ([Groq STT](https://console.groq.com/docs/speech-to-text)) | Gemini Live models are free on the free tier, but free-tier content is used to improve Google's products ([pricing](https://aistudio.google.com/docs/pricing)); audio sessions end after about 15 min without compression ([sessions](https://ai.google.dev/gemini-api/docs/live-api/session-management)); OpenAI Realtime has only paid prices, $32/$64 per million audio tokens in/out ([OpenAI](https://developers.openai.com/api/docs/models/gpt-realtime-2)) | Same as B for chat |
| Privacy | Privacy mode stays local (Moonshine); only text goes to the LLM | All speech leaves the PC; breaks privacy mode unless switched off | Chat audio leaves the PC |
| RAM | +8 MB Smart Turn ([HF](https://huggingface.co/pipecat-ai/smart-turn-v3)); optional 25-90 MB meaning model | Small locally (a socket) | Both |
| Effort | 4-6 weeks in phases | 3-5 weeks, plus rework of confirmations | 6-8 weeks |
| Risks | Hard to beat ~0.5 s with a batch recogniser | Preview models change; session limits; a cloud model deciding actions; confirmations ("send", "delete", "shut down") must stay in Rust | Two paths, two voices (Gemini voice vs Edge voice) |

**Recommendation: A.** It is free on the owner's own keys, keeps privacy mode true, stays inside the RAM budget,
keeps every confirmation gate in Rust, and the measured cloud speech-to-speech models are not faster than the
target for A. Option C can be revisited **only** if phase 0-4 cannot get cloud answers under 1.2 s.

## 6. Phased plan

Each phase ends with unit tests, the local CI script, a live run, and one line in `docs/testing/live-proof-ledger.md`.
Rollback for every phase: each change sits behind a setting with the old behaviour as the fallback.

### Phase 0: measure the real turn (first, before any speed work). Effort: 2-3 days.
- **Files:** `latency.rs` (add stages: wake fired, listening started, last voiced chunk, action finished),
  `wakeword_oww.rs` (mark last voiced chunk and wake times), `orchestrator.rs` and `command_executor.rs` (action
  finished), `tts.rs` (already marks first audio), new `scripts/turn_latency_report.py` (p50/p95 per path:
  command, cached reply, cloud answer), `scripts/voice_funnel_report.py` (wake → capture → transcript → action).
- **Build:** one `[LAT]` line that starts at the **end of speech**, not at capture stop; one `[WAKE]` line per
  candidate. Record the held-out command set (section 4).
- **Success:** 100 owner turns recorded with all stages; a baseline table written in this folder.
- **Tests:** `latency.rs` unit tests for the new stages; report script on a fixture log.

### Phase 1: quick speed wins. Effort: 3-4 days.
- **Wake:** switch to tiered checking and start the command capture in parallel with the cloud check (planned in
  `docs/features/153-...md`, not built). Files: `wakeword_oww.rs`, `commands.rs`.
- **End of turn:** test Smart Turn (built, off) on the held-out set; turn it on only if it cuts the wait to about
  200-250 ms without cutting people off. Files: `wakeword_oww.rs` (`smart_turn_poll`).
- **Stock ack:** remove the 856 ms gap (play cached lines from Rust directly). Files: `tts.rs`, frontend `ttsPlayer.ts`.
- **Commands:** play the cached ack and start the action at the same moment.
- **Success:** command first sound p50 ≤ 0.8 s from end of speech; wake-to-listening p95 ≤ 300 ms for confident wakes;
  false wakes per hour not higher than phase 0.

### Phase 2: understanding. Effort: 1-1.5 weeks.
- **Files:** `orchestrator.rs:1287-1308` (classifier online), `nlu_client.rs` / `nlu_local.rs` (gate 0.6 when slots
  resolve), `intent_parser.rs` (fix the known collisions), `center.rs` (slot checks against the app registry,
  contacts, repo list), `memcore/corrections.rs` (learn from the repeat after a miss).
- **Build:** "command-shaped means command": a short question naming two candidates instead of chat.
- **Success:** held-out right action ≥ 85 %, wrong action ≤ 2 %; stress set from 51 % to about 75 % (estimate in doc 01).

### Phase 3: start understanding before you stop talking. Effort: 1.5-2 weeks.
- **Files:** `stt_stream.rs` (partial words already flow for captions), `orchestrator.rs` (speculative parse),
  `wakeword_oww.rs`.
- **Build:** parse each partial transcript; when a complete, safe command appears (open, play, focus), prepare it
  (find the window, warm the voice) but **run it only after end of speech is confirmed and the final transcript agrees**.
  Never speculate on send, delete, merge, shut down. Partials come from the warm local Moonshine stream; Groq stays
  the final word when online. Privacy mode uses local only.
- **Success:** command first sound p50 ≤ 0.6 s; zero actions that the final transcript did not confirm.

### Phase 4: faster cloud answers and cloud-chosen commands. Effort: 1-1.5 weeks.
- **Files:** `router.rs` (streaming responses; command catalogue as function tools), `tts.rs` (speak the first clause
  as it arrives), `center.rs` (a tool call becomes a normal intent and passes the same gates).
- **Success:** cloud answer first word p50 ≤ 1.2 s, p95 ≤ 1.8 s; cloud-chosen commands never skip a confirmation
  (test: forged tool call for "send" must ask first).

### Phase 5 (optional, only if phase 4 misses): realtime experiment. Effort: 1-2 weeks.
- A switch for open conversation through Gemini Live, off by default, never in privacy mode, never for actions.
- **Success:** beats phase 4 by at least 300 ms p50 on the same questions, or it is removed.

## 7. What cannot be achieved, and what the owner must provide

**Cannot be promised.**
- "Every command understood perfectly": no system can show that; we can show a measured rate on your own speech and a
  rule that a wrong action is worse than a question.
- A full spoken answer in 100 ms: only cached lines and acks can be that fast.
- Matching a demo we have not seen, or one that was edited.
- Faster than the network: Groq, Edge and the cloud LLM have bad days (the Edge breaker already fell back once in a
  live run).

**How to tell if a demo is staged or edited** (check these on the reel):
- Jump cuts right after the user stops speaking; the reply starts on a cut.
- No visible clock, no uncut single take, no screen recording of the whole desktop.
- Always the same short, rehearsed sentences; apps already open; no background noise.
- Voice track that does not match the room sound (dubbed later).
- Ask the creator for one uncut take with a clock on screen and a command chosen by someone else.

**What the owner must provide.**
1. The reel link or a screen recording of it.
2. His own Groq key (and Gemini key if phase 5 is tried); check the real free limits on the account page.
3. A working microphone setting (the Intel Smart Sound gate is a known weak point; a USB or headset mic helps tests).
4. Three recording sessions for the held-out set (about 200 commands, 50 non-commands), and about 30 minutes of
   normal use after each phase so the `[LAT]` and `[WAKE]` lines can be read.

## Sources

- TechCrunch, Google's Gemini demo: https://techcrunch.com/2023/12/07/googles-best-gemini-demo-was-faked/
- Engadget, same: https://www.engadget.com/google-admits-that-a-gemini-ai-demo-video-was-staged-055718855.html
- Artificial Analysis speech-to-speech time to first audio: https://artificialanalysis.ai/speech-to-speech
- Gemini pricing (free tier, data use): https://aistudio.google.com/docs/pricing
- Gemini Live session limits: https://ai.google.dev/gemini-api/docs/live-api/session-management
- Gemini Live overview (tools, audio format): https://aistudio.google.com/docs/live-api
- Gemini 3.5 Transcribe (streaming WER 4.0 %): https://blog.google/innovation-and-ai/technology/developers-tools/build-real-time-voice-applications-gemini-audio/
- Groq rate limits: https://console.groq.com/docs/rate-limits
- Groq speech-to-text (10 s minimum billing): https://console.groq.com/docs/speech-to-text
- OpenAI gpt-realtime-2 pricing: https://developers.openai.com/api/docs/models/gpt-realtime-2
- Smart Turn v3 model card: https://huggingface.co/pipecat-ai/smart-turn-v3
- Earlier repo research on latency techniques: `docs/research/voice-latency/01-how-100ms-voice-responses-work.md`
