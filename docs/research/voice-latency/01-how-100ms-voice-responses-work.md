# How ~100-300 ms voice responses work, and what NEXUS can realistically do

Date: 2026-10-30. Research only; no code changed. Evidence labels used throughout:

- **(a) Vendor-published** number (marketing or docs; measured on the vendor's terms)
- **(b) Peer-reviewed / arXiv** result
- **(c) Inference** (mine, not a sourced fact)

Caveat: figures come from search results and summaries. Primary pages were not all opened in full. Where a number could not be found, that is stated. Nothing here is invented.

## 1. The core idea

Nobody gets a *full spoken answer* in 100 ms. The "75 ms / 40 ms / 160 ms" headlines are **time to first audio** of one *model stage* (TTS, or a speech model) once it already has text or audio. Products feel instant because they (1) overlap every stage (stream, never wait), (2) start TTS from the first few LLM words, (3) detect end of turn early, (4) keep connections warm, and (5) play fixed phrases from cache. The rest of the answer is spoken while the model keeps generating. (c)

## 2. What vendors publish (a)

| Product | Published number | What it measures | Source |
|---|---|---|---|
| ElevenLabs Flash v2.5 | ~75 ms | Model latency; footnote: excludes application and network latency | https://elevenlabs.io/docs/overview/models |
| ElevenLabs Turbo v2.5 | ~250 ms | Same basis (Replicate listing) | https://replicate.com/elevenlabs/flash-v2.5 |
| ElevenLabs Flash, independent | 197 ms median first audio (50 trials) | Third party, streaming, includes network | https://humannessindex.vapi.ai/models/elevenlabs-flash-v2-5 |
| Cartesia Sonic | 40 ms (Turbo) / ~90 ms (standard Sonic-3) first audio; original Sonic 135 ms | Aggregator reports of vendor claims; Turbo is a best case with a 500-char limit | https://www.eesel.ai/blog/cartesia-sonic-3-review , https://www.cartesia.ai/blog/sonic |
| Deepgram Aura-2 | sub-200 ms TTFB at launch; later ~90 ms steady state, p95 under 200 ms | Vendor engineering post | https://deepgram.com/learn/introducing-aura-2-enterprise-text-to-speech , https://www.plushcap.com/content/deepgram/blog/deepgram-how-we-took-aura-2s-ttfb-from-200ms-to-90ms-engineering-real-time-voice-ai-at-scale |
| Deepgram Flux (STT + end-of-turn) | ~260 ms end-of-turn (quickstart); other listing: median under 300 ms, p95 ~1.5 s | Speech end to turn-end event | https://developers.deepgram.com/docs/flux/quickstart.mdx , https://www.together.ai/models/flux |
| Flux "eager end of turn" | ~150 ms median saving when eager turns fire | Telnyx release note, not Deepgram | https://telnyx.com/release-notes/automatic-eager-end-of-turn-deepgram-flux.md |
| OpenAI GPT-4o audio (May 2024) | 232 ms min, 320 ms average (earlier pipeline 5.4 s) | Model audio-in to audio-out at launch. Realtime API end-to-end figure: **not found** | https://vercel.com/ai-gateway/models/gpt-4o.md (secondary summary) |
| Kyutai TTS 1.6B | 220 ms from first text token to first audio chunk; accepts streamed text | Model level | https://kyutai.org/next/tts |
| Kyutai Unmute | ~450 ms TTS latency on unmute.sh (~750 ms on one L40S) | Definition unclear; unverified | https://github.com/kyutai-labs/unmute |
| Groq LPU (Llama 2 70B, 2024) | 0.22 s time to first token, ~185 tok/s (Groq); 241 tok/s (ArtificialAnalysis) | Old model generation. Current Llama 3.x TTFT: **not verified** | https://groq.com/groq-lpu-inference-engine-crushes-first-public-llm-benchmark/ |
| Vapi | No headline figure. Five stages: transport, endpointing, transcription, model, synthesis. Dashboard total omits endpointing and transport; model TTFT often the largest piece | | https://docs.vapi.ai/assistants/model-intelligence/understanding-latency |
| Pipecat / LiveKit | No official end-to-end number found. A third-party guide budgets STT+turn ~400 ms, LLM TTFT 300-500 ms, TTS ~200 ms; one single-author benchmark measured 540 ms p95 for Pipecat+Claude+Cartesia (anecdotal) | | https://dev.to/kenimo49/i-benchmarked-5-voice-ai-stacks-only-2-stayed-under-300ms-2bka |

Read-across (c): every fast figure is one stage. A cascaded pipeline from end of speech to first audio is realistically ~500-800 ms even when well engineered; only native speech-to-speech models approach 200-300 ms.

## 3. arXiv / peer-reviewed results (b)

| Result | Number | Source |
|---|---|---|
| Moshi: full-duplex speech-text model with "Inner Monologue" text stream | 160 ms theoretical, ~200 ms practical (abstract). The ~230 ms human-conversation comparison came from a truncated snippet; verify in paper | https://arxiv.org/abs/2410.00037 |
| Kyutai Delayed Streams Modeling (DSM): ASR and TTS as one architecture with a configurable delay between aligned streams, adjustable at inference | Secondary summaries: 6.4% avg WER on OpenASR leaderboard; TTS WER 1.72% EN / 2.96% FR; latency "a few hundred ms". Check the paper | https://arxiv.org/abs/2509.08753 |
| Speculative decoding (Leviathan et al., ICML 2023): small draft model proposes, large model verifies in parallel, identical output distribution | 2x-3x faster decoding on T5-XXL | https://arxiv.org/abs/2211.17192 |

Not researched in depth: StreamSpeech, specific streaming-TTS papers, low-latency turn-taking papers (listed as leads at the end).

## 4. The techniques and which ones are real levers

| Technique | What it saves | Evidence |
|---|---|---|
| Streaming ASR | No upload-then-transcribe wait | (a) Deepgram, (b) DSM |
| VAD plus semantic end-of-turn | Cuts silence timeout (often 500-800 ms) to ~250 ms; eager mode starts the LLM early | (a) Flux docs, Telnyx. LiveKit "85% fewer interruptions" is vendor-reported via a weak secondary source |
| LLM token streaming into TTS at clause boundaries | Audio starts after the first clause, not after the whole reply | (a) Unmute pipes first LLM words into TTS: https://kyutai.org/2025/05/22/unmute.html |
| TTS WebSocket with chunk schedule and flush | Controls when audio generation starts; docs recommend `flush` at the end of each turn; lower schedule trades quality | (a) https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets . `auto_mode` confirmed only in a third-party clone's docs |
| LLM normalises text instead of TTS | Flash keeps normalisation off for latency | (a) ElevenLabs models page |
| Persistent connection / pre-warm | Removes handshake from first audio | (c) general practice; NEXUS measured 1009 -> 409 ms median, 290 ms after wake pre-warm (project notes, change 132) |
| Fast inference hardware (Groq) | LLM first token in the low hundreds of ms | (a) Groq post |
| Speculative decoding | 2-3x generation speed; helps tokens/s more than first token | (b); relevance to NEXUS is (c) low |
| Native speech-to-speech | Removes ASR->LLM->TTS hand-offs | (b) Moshi ~200 ms; (a) GPT-4o 320 ms |
| WebRTC vs WebSocket | WebRTC helps browser/phone jitter; a local desktop app is not transport-bound | (c) |
| Cached fixed phrases | First audio = local file play | (c) |
| Filler / acknowledgement | Perceived latency only | (c) |

## 5. What NEXUS can realistically do

### Honest framing

- A **full LLM-generated answer** cannot be spoken in ~100 ms. Floor: end-of-speech detection + STT (~250 ms) + LLM first clause + TTS first audio. Even fully overlapped on cloud models, expect roughly **0.9-1.3 s to first audio**. (c)
- ~100 ms **is** achievable for cached stock lines, command acknowledgements played from local clips, and a short earcon or filler. (c)
- The measured 1.8-3.9 s is end of speech to *end of reply*, so it mixes in reply length. First-audio time is not separately known; measure it. (c)

### Latency budget (end of speech to first audio)

"Today" is from the brief and project notes. "Achievable" is my estimate (c), not a measurement.

| Stage | Today | Achievable | How |
|---|---|---|---|
| End-of-speech detection | endpoint 240-800 ms depending on mode | 160-320 ms | Keep Ghost 240 ms; use the existing turn-detect model to shorten the timeout when the sentence is complete; do not go below ~240 ms |
| STT (Groq Whisper) | ~250 ms | 200-250 ms | Pooled HTTP/2 client already; keep |
| Intent parse (Rust) | <5 ms | <5 ms | Already on capture thread |
| Command ack | cached ack when matched | 20-100 ms | Play pre-rendered ack, execute in parallel |
| LLM first token | not given separately | 150-400 ms | 9Router Cerebras/Groq/Gemini cascade; small fast model for chat; short prompts |
| LLM to first speakable clause | waits for whole reply or ~60 chars | first 8-15 words | Stream tokens, flush at first clause |
| TTS first audio (Edge, warm) | 290-410 ms | 250-350 ms | Send only the first clause first; queue the rest |
| TTS offline (Kokoro) | ~4.3 s for a 3-word line on battery | cache-only | Not a live voice (already project policy) |
| **Total to first audio, cloud answer** | ~1.5-3 s (inference) | ~0.9-1.3 s | Sum of above |
| **Total to first audio, command** | via TTS path | ~100-200 ms | Cached ack |

### Ranked changes

1. **Speak from the first LLM clause.** Stream router tokens; flush to Edge at the first comma/clause or ~40 chars, then 1-2 sentence chunks, one voice per utterance. Biggest win for long answers. The project has sentence-chunked streaming but not LLM-token streaming into it (marked "not built" in AGENTS.md).
2. **Local-first for commands.** Anything the Rust parser matches never touches the cloud: speak a cached or templated ack at once, run the action concurrently. Extend stock-ack coverage to the most frequent results.
3. **Cache stock and templated replies.** Pre-render the top N lines per voice (persisted on disk already via `tts_cache_disk`); first audio becomes a file play.
4. **Acknowledgement within ~200 ms for slow paths.** Cached filler or earcon while the LLM works; drop it if the first clause is ready sooner. Perceived latency only; avoid repetitive fillers.
5. **Speculative LLM start on tentative end of turn.** Start the LLM on a short pause with a syntactically complete transcript, cancel if speech resumes (Flux eager idea, ~150 ms where it fires). Costs extra requests against free quotas, so gate it.
6. **Keep connections hot.** Edge pre-warm at wake exists; confirm the router's Groq/Cerebras/Gemini clients are pooled and warmed.
7. **Smaller first model for chat**, larger only for analysis. Measure with `[LAT]` before choosing; current Groq model TTFT was not verified.
8. **Measure first audio.** Add `first_audio_ms` per turn and report p50/p95 by path (command / cached / cloud).
9. **Optional paid/keyed streaming TTS** (Cartesia, ElevenLabs Flash, Deepgram Aura) could trim ~100-200 ms vs Edge but breaks the free-BYOK promise or adds keys. (c)
10. **Not worth it here:** native speech-to-speech (needs a GPU server), WebRTC, local speculative decoding.

### Bottom line

Realistic: **~100-200 ms** first audio for commands and cached lines; **~0.9-1.3 s** first audio for cloud answers, the rest of a long reply spoken while it streams. Full-answer time stays bounded by how long the reply takes to say.

## 6. Papers and sources to read

- Moshi, arXiv 2410.00037 (https://arxiv.org/abs/2410.00037): full-duplex speech-text model, ~160 ms theoretical / ~200 ms practical.
- Delayed Streams Modeling, arXiv 2509.08753 (https://arxiv.org/abs/2509.08753): one streaming architecture for ASR and TTS with tunable delay.
- Speculative Decoding, arXiv 2211.17192 (https://arxiv.org/abs/2211.17192): draft-and-verify, 2-3x decoding speedup, same outputs.
- Kyutai TTS (https://kyutai.org/next/tts): 220 ms text-token-to-audio, text-streaming TTS.
- Kyutai Unmute (https://kyutai.org/2025/05/22/unmute.html , https://github.com/kyutai-labs/unmute): reference cascaded agent piping first LLM words into TTS.
- ElevenLabs WebSocket TTS guide (https://elevenlabs.io/docs/eleven-api/guides/how-to/websockets): chunk schedule and flush for agents.
- ElevenLabs models (https://elevenlabs.io/docs/overview/models): the ~75 ms Flash claim and its footnote.
- Cartesia Sonic launch (https://www.cartesia.ai/blog/sonic): state-space-model TTS.
- Deepgram Aura-2 TTFB post (https://www.plushcap.com/content/deepgram/blog/deepgram-how-we-took-aura-2s-ttfb-from-200ms-to-90ms-engineering-real-time-voice-ai-at-scale): how a vendor cut TTFB 200 to 90 ms.
- Deepgram Flux quickstart (https://developers.deepgram.com/docs/flux/quickstart.mdx): end-of-turn detection and eager mode.
- Vapi latency explainer (https://docs.vapi.ai/assistants/model-intelligence/understanding-latency): clearest official stage breakdown of a cascaded agent.
- Groq benchmark post (https://groq.com/groq-lpu-inference-engine-crushes-first-public-llm-benchmark/): LPU TTFT and throughput method (older model).
- Still to read (not opened here): StreamSpeech; Pipecat and LiveKit Agents official docs; OpenAI Realtime API docs; Smart Turn and LiveKit turn-detector model cards.
