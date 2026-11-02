# Early hello + early yes/no: papers and open-source options

Date: 2026-11-02. Research only, no code changed.

**Problem.** After logon NEXUS needs 10-25 s before it can listen. The owner wants an instant greeting and then a spoken question ("Shall I reopen the apps you were using?"). The answer must be caught even if the user replies in the first seconds, before the full app (wake engine, STT sidecar, orchestrator) is ready.

**How to read the labels.**
- **[read]**: I opened the page or README in this session.
- **[summary]**: I only saw a search-result summary.
- **[prior]**: from my training knowledge of the paper. I did not re-open it here, so check the numbers before quoting them.
- **[inferred]**: my own reasoning, with no source.

---

## TL;DR recommendation

Build **two stages, with the buffer handed over**. Do not build a tiny on-device yes/no detector as the main path.

1. **Stage 0** is a tiny early process or thread inside the main binary. It does three things:
   - plays a pre-rendered WAV from disk with rodio (greeting plus question, no TTS engine needed);
   - opens **one** cpal capture stream as soon as playback ends;
   - writes 16 kHz mono PCM into a ring buffer for up to ~6 s, with Silero VAD (~2 MB ONNX, <1 ms per 30 ms chunk) used only to decide where the reply starts and ends.
2. **Stage 1** is the normal app. When ready, it **takes ownership of that same cpal stream**, so a second capture stream is never opened. It transcribes the buffered clip with the normal path: Groq first, Moonshine tiny or small as the local fallback. It then applies a **small fixed yes/no word list** (yes, yeah, yep, sure, okay, ok, haan, ha, han ji, theek hai / no, nope, nahi, na, not now, later). Anything else counts as "unclear" and gets one re-ask.

**Optional later step:** if measurements show the main stage is often not ready within ~8 s of the answer, add a **sherpa-onnx keyword spotter** (English zipformer, ~5 MB int8, open-vocabulary keyword list, official Rust crate) to Stage 0 as an *early decision*. Keep the buffered clip as the authority.

**Why not a tiny yes/no classifier first.** Hinglish words ("haan", "nahi") are absent from every public keyword dataset. "Okay" and "no" are very short and are known to be the hardest targets for wake-word-style models. A wrong "yes" relaunches apps, so a false accept costs the user something. Buffer-and-transcribe reuses machinery NEXUS already has [inferred].

---

## A. Research papers

| Paper | Authors, year | Key result | Size / compute | Label |
|---|---|---|---|---|
| Speech Commands: A Dataset for Limited-Vocabulary Speech Recognition (arXiv:1804.03209) | P. Warden, 2018 | 105,829 one-second utterances of 35 words, including **yes** and **no**. This is the standard KWS benchmark. No "okay", "yeah", "haan" or "nahi". | n/a | [prior] |
| Hello Edge: Keyword Spotting on Microcontrollers (arXiv:1711.07128) | Y. Zhang, N. Suda, L. Lai, V. Chandra (Arm), 2017 | DS-CNN reaches ~95% on the 12-class Speech Commands task (yes/no/up/down/... plus silence and unknown). | ~38k-500k params, a few MOps per inference, fits in tens of KB on a microcontroller | [prior] |
| Streaming Keyword Spotting on Mobile Devices (arXiv:2005.06720) | O. Rybakov, N. Kononenko, N. Subrahmanya, M. Visontai, S. Laurenzo (Google), 2020 | Shows how to convert non-streaming KWS models into streaming ones with state, cutting latency and compute with little accuracy loss. microWakeWord uses the same idea. | per-frame inference, ~tens of k params | [prior] |
| MatchboxNet (arXiv:2004.08531) | S. Majumdar, B. Ginsburg (NVIDIA), 2020 | ~97.5% on Speech Commands v2 (35 classes) | ~93k params | [prior] |
| Broadcasted Residual Learning for Efficient KWS (BC-ResNet, arXiv:2106.04140), Interspeech 2021 | B. Kim, S. Chang, J. Lee, D. Sung (Qualcomm), 2021 | BC-ResNet-1: 96.6% (v1) / 96.9% (v2) with **9.2k params, 3.1M MACs**. Scaled variants: 98.0% (v1) / 98.7% (v2). | 9.2k params is about 40 KB fp32 | [summary] https://arxiv.org/abs/2106.04140 |
| Keyword Transformer (KWT, arXiv:2104.00769) | A. Berg, M. O'Connor, M. T. Cruz, 2021 | ~98.6% on Speech Commands v2 | 0.6M-5.4M params; much heavier than BC-ResNet for no gain at this task | [prior] |
| Moonshine: Speech Recognition for Live Transcription and Voice Commands (arXiv:2410.15608) | N. Jeffries et al. (Useful Sensors), 2024 | Compute grows with clip length (no 30 s padding as in Whisper), so a 1-2 s clip is very cheap. Tiny has a WER similar to Whisper tiny.en. | tiny ≈ 27M params (~100 MB fp32, smaller quantized) | [prior]; repo [read] https://github.com/moonshine-ai/moonshine |
| Silero VAD | Silero team (snakers4), ongoing | Each 30+ ms chunk takes <1 ms on a single CPU thread. Supports 8/16 kHz. Repo gives only qualitative quality claims; numbers are on its wiki. | ~2 MB, ONNX available, MIT | [read] https://github.com/snakers4/silero-vad |
| WebRTC VAD (GMM-based, from the Chromium WebRTC stack) | Google, ~2011 onward | Very cheap and widely used, but more false "speech" on noise than neural VADs. NEXUS already uses a WebRTC-style VAD in wake. | KB-size, no model file | [prior] |
| Perceived latency / turn-taking (Stivers et al., PNAS 2009, "Universals and cultural variation in turn-taking") | Stivers et al., 2009 | Across 10 languages, the typical gap between conversational turns is ~0-200 ms. An answer can therefore arrive within half a second of the question ending. | n/a | [prior] |

**What the papers mean for this design** [inferred]:
- Tiny KWS models of 10-100k params reach 95-98% **only on their closed training words, in clean 1-second clips**. Real-world performance on spontaneous, untrimmed speech with new words (yeah, okay, haan, nahi) is not reported. It would need a custom dataset that does not exist.
- Turn-taking research says the user may answer **within ~200 ms** of the question ending, and possibly over its last syllable. So capture must already be open when playback ends. Pre-roll matters: the buffer should start at the last ~300 ms of playback, with echo handling (see section C).
- On "pre-roll / ring-buffer designs", I found no dedicated paper. It is standard engineering practice: Alexa sends ~500 ms of audio from before the wake word, and openWakeWord and Porcupine demos keep a rolling buffer. NEXUS already has `VERIFY_RING` [prior/inferred].
- On "voice-assistant cold start" as a research topic, I found no paper. The literature is about model size and latency, not app boot [inferred].

---

## B. Open-source and commercial engines

The NEXUS Hinglish column is my estimate unless marked otherwise.

| Engine | Size | Load time | yes/no + "yeah/okay/haan/nahi" | Latency | Licence | Rust embedding | Usable in an early stage? |
|---|---|---|---|---|---|---|---|
| **openWakeWord** (already in NEXUS via tract) | Shared melspec + embedding models already bundled; each keyword head is a few hundred KB (NEXUS `nexus.onnx` is ~0.8 MB) | Already loaded by wake; ~<1 s [inferred] | Custom heads are trained on synthetic TTS. The README recommends "several thousand" generated examples and ~30,000 h of negatives. Short one-syllable words like "no", "ha" and "yes" are **poor wake-word targets**: high false accepts on ordinary speech [inferred; matches NEXUS's own 2-syllable "nexus" false-accept history]. You would need 6-10 heads (yes, yeah, okay, sure, haan, no, nope, nahi), each tuned. | 80 ms frames | Code Apache-2.0; **pre-trained models CC BY-NC-SA 4.0**; your own heads are your licence | Already done in NEXUS (tract) | Yes, the pipeline already exists. But the main wake engine *is* what takes time to boot. | [read] https://github.com/dscripka/openWakeWord |
| **Picovoice Porcupine / Rhino** | Porcupine ~1-2 MB, Rhino context ~small [prior] | Fast, <100 ms [prior] | Rhino (speech-to-intent) with a "yes/no" context would be ideal technically. English only, no Hinglish words unless spelled phonetically. | very low | **Forever-Free plan is personal, non-commercial only**. The SDK is Apache-2.0, but custom models trained in the Console are under non-commercial terms. An older report says Rhino personal contexts expired after 1 month (unverified). | Official Rust SDKs exist [prior] | Technically yes. **Licence makes it unfit if NEXUS ships to others.** | [summary] https://picovoice.ai/blog/introducing-picovoices-free-tier , https://picovoice.ai/docs/faq/general |
| **sherpa-onnx KWS** (k2-fsa) | English GigaSpeech zipformer KWS: encoder 4.6 MB int8 (12 MB fp32) + decoder 272 KB + joiner 160 KB, about **5 MB int8** (dir ~19 MB) | ~0.3-1 s [inferred] | **Open vocabulary.** You list keywords in a text file and convert them to BPE tokens; no retraining. "yes", "yeah", "okay", "no", "nope", "not now" are all expressible. "haan" / "nahi" must be spelled through English BPE ("HAAN", "NAHI"), and accuracy is unknown because the model was trained on English GigaSpeech only. A zh-en model (~3M, 160/320 ms chunk variants) also exists. | Streaming, ~160-320 ms chunks | Apache-2.0 [prior] | **Official `sherpa-onnx` Rust crate** (safe RAII bindings, KeywordSpotter type, links statically, auto-downloads a prebuilt Windows x64 lib). Community `sherpa-rs` is deprecated. | Yes. Best candidate if an on-device early decision is wanted. Brings its own ONNX Runtime, about 10-20 MB extra binary [inferred]. | [read] https://k2-fsa.github.io/sherpa/onnx/kws/pretrained_models/index.html ; [summary] https://docs.rs/sherpa-onnx |
| **Vosk** (Kaldi-based) with a restricted grammar | small-en-us-0.15 ≈ 40-50 MB | ~1-2 s [prior] | Grammar mode (`SetGrammar(["yes","no","yeah","okay","haan","nahi","[unk]"])`) forces output into that list. Out-of-vocabulary words like "haan" need the lexicon or a soundalike spelling [prior]. Accuracy on a closed grammar is usually good. | near real time | Apache-2.0 | `vosk` crate wraps libvosk; needs the DLL shipped [prior/summary] | Possible, but 40-50 MB and a separate runtime for one question. | [summary] https://huggingface.co/rhasspy/vosk-models |
| **whisper.cpp tiny / base** | tiny ~75 MB, base ~142 MB (ggml) [prior] | ~0.3-1 s load | Fine for English yes/no/okay. Known **hallucinations on very short or silent clips** (NEXUS's own filter exists for this). "haan/nahi" may come out as Hindi script or noise. | A 2 s clip costs ~0.2-0.5 s CPU for tiny [inferred] | MIT | `whisper-rs` crate [prior] | Heavier than needed. Duplicates Moonshine. | [prior] |
| **Moonshine tiny** (already NEXUS's local fallback family) | ~27M params; README mentions "tiny 1MB models" for its micro line | NEXUS loads the small/medium model in ~1.8 s; tiny is faster [inferred] | Good English yes/no. Hinglish unknown. Compute grows with clip length, so it suits a 2 s clip. | ~tens of ms for a 2 s clip [inferred] | MIT for English models | Currently a Python sidecar in NEXUS; native ONNX in Rust is possible | It is the **Stage 1 fallback**, not Stage 0 (the Python sidecar is the slow thing). | [read] https://github.com/moonshine-ai/moonshine |
| **Silero VAD** | ~2 MB | <100 ms | VAD only, no words | <1 ms per 30 ms chunk | MIT (README; a badge says CC BY-NC, which conflicts, so check before shipping) | `ort`, or run via tract/sherpa-onnx (sherpa-onnx bundles Silero VAD) | **Yes, ideal for Stage 0 endpointing.** | [read] https://github.com/snakers4/silero-vad |
| **microWakeWord** (Home Assistant / ESPHome) | TFLite Micro streaming inception; example tensor arena ~22 KB RAM | instant | Wake-word style, so it has the same short-word problem as openWakeWord. Trained with synthetic data like openWakeWord. | 20 ms stride | Apache-2.0 [prior] | TFLite in Rust is awkward; no direct benefit over openWakeWord on a PC | No | [summary] https://github.com/kahrendt/microWakeWord , https://esphome.io/components/micro_wake_word/ |
| **Rhasspy / Wyoming protocol** | protocol only | n/a | Useful idea: a stage that only streams audio events (`audio-start`, `audio-chunk`, `audio-stop`) to whichever service is ready. Same idea as the buffer hand-off. | n/a | MIT [prior] | You would only reuse the message shape | Design inspiration only | [prior] |
| **Mycroft Precise** | small GRU | fast | Wake word only; project abandoned with Mycroft (2023) | — | Apache-2.0 | Python/TF | No | [prior] |
| **Snips NLU / ASR** | — | — | Archived after the Sonos acquisition (2019); do not use | — | Apache-2.0 (NLU) | — | No | [prior] |
| **Coqui STT** (ex-DeepSpeech) | ~45 MB tflite + scorer | ~1 s | Supported hot-words and custom scorers; **project shut down (2024)** | — | MPL-2.0 | `coqui-stt` crate, unmaintained | No | [prior] |
| **Kaldi** | large | slow | Basis of Vosk; using it raw is heavy | — | Apache-2.0 | Hard | No; use Vosk if you want Kaldi | [prior] |

**Note on Hinglish** [inferred]: no open model above was evaluated on "haan / nahi / han ji / theek hai". Groq Whisper large-v3-turbo handles code-mixed Hindi-English reasonably, sometimes in Devanagari, so Stage 1 should normalise both scripts ("हाँ", "हां", "नहीं" → yes/no). This is the strongest argument for **transcribing the buffered clip with the big cloud model** rather than deciding early on device.

---

## C. Design patterns

### Option 1: Two stages with the buffer handed over (recommended)

Stage 0 plays a pre-rendered greeting and question, opens one capture stream, keeps a VAD-trimmed ring buffer, and hands the buffer plus the live stream to Stage 1, which transcribes it.

- **Reliability.** High. The answer is never lost: it waits in memory until Stage 1 can read it. Accuracy is whatever Groq or Moonshine already gives, plus a closed yes/no word list, which is very forgiving.
  - Risk: if Groq is offline *and* the Moonshine sidecar is slow, the decision is late. That is still correct, just slow. Speak "Reopening now" when it lands.
- **CPU / RAM.** Tiny. 6 s × 16 kHz × 2 bytes = 192 KB. Silero adds ~2 MB plus ~1% of one core. No extra model if the existing wake VAD is reused.
- **Microphone conflicts.** This is the decisive point.
  - WASAPI shared mode *does* allow several clients to capture the same endpoint [summary: https://learn.microsoft.com/windows/win32/coreaudio/user-mode-audio-components].
  - But NEXUS has documented cases on the Intel Smart Sound Technology driver where a second capture (WebView2 getUserMedia next to cpal) made the cpal stream silent. There is also a current cpal issue where WASAPI capture returns zeros after a resampling change (https://github.com/RustAudio/cpal/issues/1200) [summary].
  - So **Stage 0 must open the one and only cpal stream, and the wake engine must adopt it** (pass the stream or its channel, not a device name). Never open, close, and reopen during boot: each reopen on Intel SST risks the known silence bug, and it loses the audio in the gap.
- **Echo.** The capture overlaps the end of NEXUS's own question. Use a short guard (start keeping audio ~150-250 ms before playback ends, but mark it as "under our own voice"). Drop transcript words that match the question text; NEXUS already does this in its directed-speech echo check. Without real AEC, a user who barges in over the question may get a clipped first word. Acceptable.
- **Privacy.** Audio stays in RAM. It goes to Groq only if cloud STT is on (the same rule as every turn), and is discarded after the decision. No disk write.
- **Complexity.** Low to medium. One small boot module, a hand-off API, and a word list. No new model, no training data.

### Option 2: Tiny on-device yes/no detector in Stage 0

- **Reliability.** Lower for this audience. Closed-set KWS reaches 95-98% on clean Speech Commands "yes/no" [prior], but:
  - there is no public training data for "haan/nahi/okay/yeah";
  - short words have high false accepts;
  - a false "yes" reopens apps, which is noticeable and annoying.
  - sherpa-onnx KWS removes the training problem for English, but Hinglish accuracy is unknown.
- **CPU / RAM.** Small: sherpa-onnx int8 ≈ 5 MB model, about 20-40 MB RAM with its ONNX Runtime [inferred]. openWakeWord heads cost less but need training.
- **Microphone conflicts.** Same rule: one stream, adopted later.
- **Privacy.** Best: nothing leaves the device.
- **Complexity.** Medium to high. A new runtime (sherpa) or N trained heads (openWakeWord), threshold tuning per word, evaluation recordings, and a fallback for "unclear" anyway.

### Hybrid (later, only if measurements justify it)

Stage 0 runs sherpa-onnx KWS on the buffer as it fills.
- A **confident English yes or no** acts early.
- **Anything else**, including all Hinglish, waits for Stage 1's transcription of the same buffer.
- If both run and disagree, Stage 1 wins **before** any app launch.

Since reopening apps waits for Stage 1 anyway (it owns `restore.rs`), the early decision only saves the speech of the acknowledgement. **That is why the hybrid is probably not worth it** [inferred].

### Comparison

| | Buffer hand-off | Tiny yes/no detector |
|---|---|---|
| Catches early answers | Yes (buffered) | Yes |
| English yes/no accuracy | Groq/Moonshine + word list: high | 95-98% clean, lower on real speech |
| Hinglish | Reasonable via Groq | Unknown / poor |
| New model / training | None | sherpa (no training, English only) or openWakeWord (training) |
| RAM | ~0.2 MB buffer (+2 MB VAD) | 5-40 MB |
| Mic conflict risk | Same (one adopted stream) | Same |
| Privacy | Cloud if cloud STT enabled | Fully local |
| Complexity | Low-medium | Medium-high |
| Decision time | When Stage 1 is ready (maybe 5-20 s) | <1 s after the answer |

---

## Recommendation

1. **Ship Option 1.**
   - Pre-rendered greeting and question WAV, played by rodio from the earliest point in `setup()`, or from a tiny separate launcher if the main binary itself starts slowly.
   - One cpal stream opened right after playback (or just before it ends).
   - Silero VAD or the existing VAD for endpointing.
   - A RAM ring buffer of up to 6-8 s, adopted by the wake engine and STT when ready.
   - Groq transcription, Moonshine fallback, closed yes/no list including Devanagari forms, and one re-ask on "unclear".
2. **Test on the owner's Intel SST laptop first.** Prove that the adopted single stream never goes silent across the Stage 0 → Stage 1 hand-off. This is the real risk, not recognition.
3. **Only if** logs show Stage 1 regularly takes more than ~10 s after the answer, add a sherpa-onnx KWS early English decision, with Stage 1 transcription as the authority.
4. **Do not** use Picovoice (non-commercial licence) or the pre-trained openWakeWord models (CC BY-NC-SA). Do not train openWakeWord heads for one-syllable words.

## Sources actually opened in this session

- https://github.com/snakers4/silero-vad
- https://github.com/dscripka/openWakeWord
- https://github.com/moonshine-ai/moonshine
- https://k2-fsa.github.io/sherpa/onnx/kws/pretrained_models/index.html

## Seen only as search summaries

- https://arxiv.org/abs/2106.04140 (BC-ResNet)
- https://docs.rs/sherpa-onnx , https://docs.rs/crate/sherpa-rs
- https://picovoice.ai/blog/introducing-picovoices-free-tier , https://picovoice.ai/docs/faq/general
- https://github.com/kahrendt/microWakeWord , https://esphome.io/components/micro_wake_word/
- https://huggingface.co/rhasspy/vosk-models
- https://github.com/RustAudio/cpal/issues/1200
- https://learn.microsoft.com/windows/win32/coreaudio/user-mode-audio-components

## From prior knowledge, not re-opened (verify before citing numbers)

- Warden 2018 (arXiv:1804.03209)
- Zhang et al. 2017 Hello Edge (arXiv:1711.07128)
- Rybakov et al. 2020 (arXiv:2005.06720)
- Majumdar & Ginsburg 2020 MatchboxNet (arXiv:2004.08531)
- Berg et al. 2021 KWT (arXiv:2104.00769)
- Jeffries et al. 2024 Moonshine (arXiv:2410.15608)
- Stivers et al. 2009 PNAS turn-taking
- whisper.cpp, Vosk grammar API, Coqui and Snips status
