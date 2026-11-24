# How Siri, Google, Wispr Flow and similar tools fix misheard words, and what NEXUS should do (2026-11-17)

Question: Siri, Hey Google, Wispr Flow and tools like them seem to understand the sentence even when one word is heard wrong. How, and what is the best design for NEXUS?
Method: documentation, papers and patents (inline links), a read of NEXUS's current code, and a feasibility prototype on real NEXUS mishearings.
Scope note: vendors publish research papers and help pages, not their production systems. Each row says what the source supports. Vendor accuracy numbers are the vendors' own and unverified. Alexa is deliberately left out.

## The answer in one paragraph

There are two families, and they fix a wrong word differently.
**Command assistants** (Siri, Hey Google, Windows Voice Access, Dragon commands, Talon, Home Assistant) narrow the question: they bias or restrict recognition to words the user is likely to say (contacts, apps, commands) and then **choose among sound-alike candidates using the known list plus the sentence around the word**; when unsure they **show numbered alternatives or ask**.
**Dictation tools** (Wispr Flow, Superwhisper, Aqua Voice) transcribe openly, then run a **language-model clean-up using the active app and nearby text**, with a **personal dictionary that grows when you fix a word**.
NEXUS command turns belong to the first family. NEXUS has pieces (hand-written mishearing lists, learning from corrections) but not the general sound-based chooser, per-word confidence, or the "which one did you mean?" step.

## Product by product (what the sources support)

| Product | How it handles a wrong or rare word | Source and strength |
|---|---|---|
| **Siri** | Recognition **adapts on-device** to the contacts you interact with most, new words you type and topics you prefer, kept on the device. Local place names improve with **location-based language models**. "Hey Siri" itself is **two-pass** (tiny always-on detector, then a second check) with speaker recognition, and post-trigger audio cancels false activations: Apple reports **95% of false triggers suppressed with one extra second of audio**. A separate paper decides whether speech was **directed at the assistant**. | [MacRumors iOS 15 guide](https://www.macrumors.com/guide/ios-15-siri) (secondary); Apple research: [server-side rescoring](https://machinelearning.apple.com/research/server-side-rescoring), [Hey Siri](https://machinelearning.apple.com/research/hey-siri), [streaming transformer](https://machinelearning.apple.com/research/streaming-transformer) |
| **Siri / Apple research** | Letting beam search enter a **context graph** of the user's entities cut word errors on named-entity utterances by **57%**; retrieving **phonetically similar** names from the user's database for a language model gave up to **30.2% relative** improvement. | [Apple, Class LM and Word Mapping](https://machinelearning.apple.com/research/class-lm-and-word-mapping); retrieval result from a search summary (no direct link) |
| **Hey Google / Google speech** | **Contextual biasing**: extra phrases (contacts, playlists, places) are injected into the decoder, with their pronunciations, on the fly. Google Cloud exposes it as **phrase hints** and custom classes, and warns that irrelevant hints hurt. Research: a lexicon of **phonetic** pronunciations lets the model spell rare words it never trained on (**16% relative error reduction over the previous biasing method**); on-device personalisation by associative memory; **lattice spelling correction that uses device state** as context. | [McGraw 2016](https://arxiv.org/pdf/1603.03185v2.pdf), [Google Cloud hints](https://docs.cloud.google.com/python/docs/reference/speech/2.10.0/google.cloud.speech_v1.types.SpeechContext), [on-device associative memory](https://www.arxiv.org/pdf/2110.02220), [Phoebe, phonetic biasing](https://arxiv.org/pdf/2310.00141v1) (papers and docs, not Assistant internals) |
| **Wispr Flow** | Transcribe, then a language-model clean-up using the **active app and nearby text**; **personal dictionary updates automatically when you correct a spelling**; context reading is skipped if slow, so dictation is never delayed; works from text, no screenshots (official docs). The "LLM clean-up after speech recognition" description is from reviews. | [Wispr docs](https://docs.wisprflow.ai/articles/4678293671-feature-context-awareness) |
| **Superwhisper** | Per-**mode** settings (target app, Whisper model, optional cloud LLM with your own key, custom system prompt), **custom vocabulary and automatic text replacements**, optional screen context. | Vendor and comparison pages ([Superwhisper](https://ai.superwhisper.com/vs/aqua-voice)); marketing-level evidence |
| **Aqua Voice** | Custom dictionary (reported up to 800 entries on the paid plan), aggressive AI clean-up, core dictation in the cloud; the 97.3% figure is the vendor's. | [comparison](https://www.getvoibe.com/resources/aqua-voice-vs-superwhisper/) (vendor-adjacent; treat as unverified) |
| **Windows Voice Access** | Says outright that errors come from **sound-alike words** ("sink / sync"). Saying **"correct that"** opens a window of **numbered alternatives**; if none fits, **spell it**; **"add to vocabulary"** teaches hard words. | [Microsoft support](https://support.microsoft.com/en-US/accessibility/windows/voice-access/correct-text-with-voice) (official) |
| **Dragon (Nuance)** | A misrecognised **command** is corrected differently from dictation (say it again, or hold Ctrl and repeat) so it is recognised next time; dictation uses a **correction box** with numbered choices. Community note: "scratch that" does **not** teach the voice model, only the correction box does. | [Dragon help](https://www.nuance.com/products/help/dragon/dmpe43/enx/dmpe/Content/Correction/correcting_misrecognized_commands.htm) (official) |
| **Talon Voice** | Programmable **grammars** per command; a **homophone list** (groups of sound-alikes with one canonical word) and a `phones` command that shows the alternatives; commands to add custom words and replacements. | [Talon docs](https://talonvoice.com/docs/), [community homophones](https://wolfmanstout.github.io/wolfmanstout_talon/core/homophones) (official + community; no decoding internals found) |
| **Home Assistant Speech-to-Phrase** | Not general recognition: it asks **which known phrase was spoken**, building a finite grammar from sentence templates plus **the names in your own home**, so it can fix small errors (duplicate or missing words) and keeps names exactly as written. The project states the trade: **less flexibility for speed and local operation**. | [speech-to-phrase](https://github.com/OHF-Voice/speech-to-phrase), [hassil](https://github.com/home-assistant/hassil) (project docs; no accuracy benchmark found) |
| **Research cross-check** | A 2025 study found language-model correction **over-corrects** unless it is given phonetic readings; most recognition errors are sound confusions, so models that read the **phoneme sequence** are more robust. | [Yamashita et al. 2025](https://arxiv.org/abs/2505.17410), [CASLU 2022](https://arxiv.org/abs/2203.12067) |

What the search did **not** find: product-level documentation of how Gboard, Pixel Recorder or Siri's app-name vocabulary work, how Talon decodes, or independent accuracy benchmarks for the dictation tools.

## The pattern behind all of them

1. **Know the user's world** (contacts, apps, places, commands) and use it before and during recognition.
2. **Keep alternatives**, not one answer, and use sound similarity to choose among them.
3. **Use the sentence shape**: a word in the verb position is a command verb, the word after "open" is an app.
4. **Learn from fixes**: a corrected word goes into the personal dictionary (Wispr, Voice Access, Dragon, Superwhisper).
5. **Show choices when unsure** (numbered alternatives in Voice Access, Dragon, Talon `phones`); never silently guess.
6. For **open dictation only**, add a language-model pass with app context.

## What NEXUS has today (read from the code)

- `stt_groq.rs`: a fixed 16-phrase command prompt, **off by default** for non-owner voices, never built from the user's own apps, contacts or repos.
- `intent_parser.rs::normalize_phonetic_mishearings`: **hand-written lists** ("goes to mode", "ghost moon", "open spot if I", "open vs coat"). Every new slip needs a human to add a line; it cannot generalise.
- `memcore/corrections.rs`: learns "no, I said X" and re-issued commands, applied after two sightings and only to command-shaped turns. It matches whole heard phrases, so a slightly different mishearing of the same word starts from zero.
- Hallucination filter and VAD stop invented text reaching commands.
- Missing: phoneme comparison against the user's own entity lists, word-level confidence, numbered "did you mean" alternatives, a prompt built from the user's entities.

Real slips from the owner's logs and notes: "send down my laptop" (shut down), "open load pad" (notepad), "open brief" / "open breath" (brave), "hook up anti-pilot" (antigravity), "Open's got to look at me. Ghost mode." These are the sound-alike confusions the sources describe.

## Prototype: choose by sound and sentence shape (feasibility only)

`scripts/sound_fix_prototype.py` (standard library plus CMUdict). It turns the heard words into phonemes and compares them to a **closed list for the slot the sentence shape expects**, with a phoneme distance where sound-alike swaps (p/b, t/d, vowel/vowel) are cheap, accepting only when the best match clearly beats the second best. This is the Home Assistant / Talon / Siri-context idea applied to NEXUS's slots.

On 18 real or noted mishearings: **16 corrected** ("open load pad" to notepad, "open brief" to brave, "open spot if I" to spotify, "open vs coat" to vs code, "send down my laptop" to shut down my laptop, "ghost moon" to ghost mode). **0 of 15** ordinary sentences or correct commands were changed.
The two misses: "hook up anti pilot" went to copilot with a **tiny margin (0.07)**, which a real system must turn into "Antigravity or Copilot?", and "send on my laptop" is not covered by the verb rule.
**Limits:** 18 cases is a smoke test; thresholds were set while looking at them, so the true rate is lower; brand names used hand pronunciations (production should use the G2P already shipped for Kokoro); no real audio was involved.

## Recommended design for NEXUS

Two modes, because the products show the families need different tools:

**Command turns (most of what NEXUS does): constrain and choose.**
1. **Build the recogniser's prompt from the user's world.** Groq's `prompt` takes up to 224 tokens ([Groq docs](https://console.groq.com/docs/speech-to-text)): the command verbs plus the user's top apps, contacts and repos. Measure with the existing `stt_eval --bias` A/B before enabling for everyone.
2. **Use word confidence.** Ask Groq for `verbose_json` and treat low `avg_logprob` words as candidates; leave confident words alone (this keeps over-correction near zero).
3. **Phonetic slot matcher in Rust** (the prototype): closed lists per slot, phoneme distance, margin rule, a usage prior from command history, and Talon-style **homophone groups** for known sound-alikes. Names are returned exactly as stored (Home Assistant's rule).
4. **Ask when two are close, with numbered choices** (Voice Access / Dragon / Talon): "Antigravity or Copilot? Say one or two." The answer is both the action and the training signal.
5. **Learn by sound.** Store each correction with its phoneme key so one fix covers nearby mishearings; keep the two-sighting guard and the Memory page list so any rule can be deleted (Dragon's lesson: only deliberate corrections teach the model).

**Dictation turns (type what I say): Wispr-style.** Only here add a small language-model pass given the app, nearby text and the user's dictionary, with phonetic hints to avoid over-correction, plus a personal dictionary that grows when the user fixes a word.

**Do not:** send every command through a language model (latency; text-only models over-correct commands), or rewrite ordinary talk (apply only to turns that start like a command, as `corrections.rs` already does).

## How we will know it works

Build a confusion set from real logs (`missed_intents.jsonl` 294 lines, learned corrections, trial sessions): 200 or more slips and 200 or more must-not-change sentences. Report corrected, wrongly corrected, asked and left alone, each with numerator and denominator, extending `voice_funnel_report.py`. Ship only if wrong corrections stay near zero.

## Honest unknowns

- Whether Groq's turbo model honours the prompt as strongly for command words as hoped (needs the A/B).
- Real noisy audio: the mic problem in note 05 (gated, near-silent input) limits what any corrector can recover; sound correction cannot restore a clipped first syllable.
- Siri's and Google's production internals, and the dictation tools' real accuracy, are not public; this note describes published research, docs and vendor material.
