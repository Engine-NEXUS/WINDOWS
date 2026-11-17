# How assistants fix misheard words, and what NEXUS should do (2026-11-17)

Question: Siri, Alexa, Google and Wispr Flow seem to understand the sentence even when one word is heard wrong. How, and what is the best design for NEXUS?
Method: literature and documentation search (sources inline), a read of NEXUS's current code, and a feasibility prototype run on real NEXUS mishearings.
Scope note: vendors publish research papers and docs, not their full production systems. Where a claim comes from a third-party review it is marked.

## The answer in one paragraph

No assistant "just hears better". They stack five layers, and the correction of a single wrong word happens mostly **after** the recogniser, using sound and meaning together:
(1) the recogniser is **biased toward words the user is likely to say** (their contacts, apps, commands) before and during decoding;
(2) it keeps **several candidate transcripts and confidence per word**, not one string;
(3) a **second stage chooses between candidates using phonetic similarity to a known list plus the sentence around the word**;
(4) it **learns from the user's own corrections and retries**, so the same slip is fixed next time;
(5) when it is still unsure, it **asks** rather than guessing. NEXUS has pieces of (4) and a hand-written version of (3); it is missing the general phonetic stage, per-word confidence use, and the "ask when unsure" step.

## What each system does (from the sources)

| Layer | What it is | Evidence |
|---|---|---|
| Vocabulary biasing | Words the user is likely to say (contact names, app names, commands) are added to the decoding search, with their pronunciations, so they win against look-alikes. Apple reports a **57% WER cut on utterances with named entities** from letting beam search enter a context graph. | [Apple, Class LM and Word Mapping](https://machinelearning.apple.com/research/class-lm-and-word-mapping); [Snips on-device biasing](https://arxiv.org/pdf/1805.10190) |
| Retrieve phonetically similar entries | Detect an entity, retrieve **phonetically similar** names from the user's own database, give them to the model as context. Up to **30.2% relative WER reduction** on a voice-assistant task. | Apple retrieval-based LLM contextualisation (summarised in the search results) |
| Phoneme-aware understanding | Most recognition errors are phonetic confusions, so the understanding model also reads the **phoneme sequence** next to the text. Tested on real Alexa utterances. | [CASLU, ICASSP 2022](https://arxiv.org/abs/2203.12067); SIGIR 2020 phoneme representations |
| Generative error correction | A language model rewrites the recogniser's N-best list. Text-only versions **over-correct**; adding phonetic readings fixes rare words better and over-corrects less. | [Yamashita et al., Interspeech 2025](https://arxiv.org/abs/2505.17410) |
| Self-learning from behaviour | Alexa rewrites an utterance before understanding it, learned from the user rephrasing or repeating ("play maj and dragons" to "play Imagine Dragons"). **Defect rate down more than 30%**, 11.8 win/loss ratio in production tests. | [Ponnusamy et al., Alexa](https://arxiv.org/pdf/1911.02557); [personalised query rewriting](https://ar5iv.labs.arxiv.org/html/2011.04748) |
| Dictation cleanup | Wispr Flow: recogniser, then a language-model pass using the active app and nearby text, and a **personal dictionary that updates automatically when you fix a spelling**. Wispr's own docs confirm the context reading and dictionary; the "LLM cleanup" description is from reviews. | [Wispr docs](https://docs.wisprflow.ai/articles/4678293671-feature-context-awareness) |
| Prompt biasing (Whisper) | Whisper accepts a short text prompt that guides spelling of unfamiliar words. It is a nudge, not a guarantee; keep it short. **Groq's endpoint takes a `prompt` up to 224 tokens and can return per-segment `avg_logprob` and `no_speech_prob`** (verified in Groq's docs). | [Groq speech-to-text docs](https://console.groq.com/docs/speech-to-text); [rare-word Whisper work](https://arxiv.org/html/2502.11572v2) |

## What NEXUS has today (read from the code)

- `stt_groq.rs`: a fixed 16-phrase command prompt (`COMMAND_VOCABULARY`), **off by default** for non-owner voices (`sttCommandHints`), never built from the user's own apps, contacts or repos.
- `intent_parser.rs::normalize_phonetic_mishearings`: **hand-written lists** of mishearings ("goes to mode", "ghost moon", "open spot if I", "open vs coat"). Each new slip needs a human to add a line. It cannot generalise to a slip nobody listed.
- `memcore/corrections.rs`: Alexa-style learning from "no, I said X" and re-issued commands, applied only after two sightings. Good, but it matches whole heard phrases, so a slightly different mishearing of the same word starts from zero.
- Hallucination filter and VAD: stop confabulated text reaching commands.
- Not present: word-level confidence, phoneme comparison against a known list, "did you mean" confirmation for near-misses, a prompt built from the user's own entities.

Real slips in the owner's log and notes: "send down my laptop" (shut down), "open load pad" (notepad), "open brief"/"open breath" (brave), "hook up anti-pilot" (antigravity), "open over open load pad", "Open's got to look at me. Ghost mode." These are exactly the phonetic-neighbour confusions the research describes.

## Prototype: fix by sound and sentence shape (feasibility, not shipped code)

`scripts/sound_fix_prototype.py` (standard library plus CMUdict). It turns the heard words into phonemes, compares them to a **closed list for the slot the sentence shape expects** (a verb position expects a command verb; the object after "open" expects an app), using a phoneme edit distance where sound-alike swaps (p/b, t/d, vowel/vowel) are cheap, and accepts only if the best match is clearly better than the second best.

Measured on 18 real or noted mishearings: **16 corrected**, including "open load pad" to notepad, "open brief" to brave, "open spot if I" to spotify, "open vs coat" to vs code, "send down my laptop" to shut down my laptop, "ghost moon" to ghost mode. **0 of 15** ordinary sentences or already-correct commands were changed ("give me a brief summary", "open the window", "play hold on to it for me"…).

The two misses are instructive: "hook up anti pilot" resolved to copilot with a **tiny margin (0.07)** — a real system must abstain there and ask "antigravity or copilot?"; "send on my laptop" was not covered by the verb rule.

**Limits, stated plainly:** 18 cases is a smoke test, not accuracy; thresholds were set while looking at them, so the true rate is lower; brand names used hand pronunciations (production should use the G2P already shipped for Kokoro); no real noisy audio was involved, only text the recogniser produced.

## Recommended design for NEXUS

A **sound-and-meaning corrector** between the recogniser and the parser, in five parts, cheapest and safest first:

1. **Dynamic biasing prompt.** Build the Groq `prompt` (limit 224 tokens) per turn from the user's real entities: their top apps, saved contacts, repos, and the command verbs, in the owner's usual phrasing. Measure with the existing `stt_eval --bias` A/B before turning it on for everyone.
2. **Use word-level confidence.** Request `verbose_json` and treat low `avg_logprob` words as correction candidates; confident words are left alone (this is what keeps over-correction near zero).
3. **Phonetic slot matcher (the prototype, in Rust).** Closed lists per slot (command verbs, apps from the indexed 800+ apps, contacts, repos, modes), phoneme distance via the existing G2P, margin rule, and a **usage prior** from command history and recency. Auto-accept only above a high margin.
4. **Ask when unsure.** Between two near candidates, say "Antigravity or Copilot?" and learn from the answer. This turns every ambiguity into a training signal instead of a wrong action.
5. **Generalise the learning.** Store learned corrections with their phoneme key so one fix covers nearby mishearings, keep the existing two-sighting guard, and keep the Memory page list so the user can delete any.

**What not to do:** do not send every command through a language model (latency, and the 2025 paper shows text-only models over-correct commands); use a model only for free dictation, with phonetic hints, never for commands. Do not auto-rewrite ordinary talk: apply only to turns that start like a command, as `corrections.rs` already does.

## How we will know it works

Build the confusion test set from real logs (`missed_intents.jsonl` 294 lines, learned corrections, owner trials), 200 or more slips and 200 or more must-not-change sentences; report corrected, wrongly corrected, and asked, each with numerator and denominator; ship only if wrong corrections stay near zero. This extends the funnel report (`voice_funnel_report.py`) with a "corrected / asked / left alone" stage.

## Honest unknowns

- Whether Groq's turbo model honours the prompt as well as OpenAI's Whisper does for command words (needs the A/B).
- Real noisy-audio behaviour: the mic problem in note 05 (gated, near-silent input) limits what any corrector can recover; sound correction cannot restore a clipped first syllable.
- Wispr Flow's and Siri's production internals are not public; this note describes published research and docs, not their code.
