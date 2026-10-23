# NEXUS voice accuracy: honest scorecard, then fix and retrain (wake word + speech-to-text)

## Context

You asked for the real accuracy of the "NEXUS" / "hey NEXUS" wake-up and of speech-to-text (STT), in a table of stars and %, so you can see how far each part is from perfect and what to train next. Three read-only audits of the code, docs and scripts produced the answer below.

**The honest headline: there is no trustworthy accuracy number for the current model.** Every wake-word recall figure in the docs was measured on files the model trained on, or on a random split of the same recordings. The newest retrain (2026-10-01) has no benchmark at all. STT accuracy has never been measured on real microphone audio; the WER figures in the docs are vendor numbers. The checkout also contains no recordings (`wake_word_data/` is gitignored) and no labelled voice test set. So the plan is: **measure first, fix known bugs, then retrain against a real held-out set.**

Decisions already made with you: keep the stage-2 "is it really 'nexus'?" speech check but fix it and measure it with/without; record the test set with **you plus 2–3 other people**.

## Scorecard (where NEXUS stands today)

Stars = confidence that it works in the real world **today**, not the % claimed in docs. ★★★★★ proven on held-out real audio and at target · ★★★★ strong, mostly proven · ★★★ plausible but unproven · ★★ known gaps or only simulated evidence · ★ broken by design or unmeasured. "Proven" = measured on audio the model never trained on. Almost every proven cell is empty — that is the finding.

### Wake word (the model that hears "NEXUS")

| What | Claimed in docs | Proven (held-out real audio) | Target | Stars |
|---|---|---|---|---|
| "NEXUS" recall, owner voice | 97.5% (trainer, leaky split) · 98.6% (on training files) | none for the current model. Older model: 93% neural-only, **43%** after the speech check | ≥ 95% | ★★★☆☆ |
| "hey NEXUS" recall | ~98% (before a rebalance; no per-phrase test since) | none | ≥ 95% | ★★★☆☆ |
| "okay NEXUS" recall | only synthetic-voice evidence | none | ≥ 95% | ★★☆☆☆ |
| Other people / accents | 250 synthetic voices only; 94 human clips, all one speaker, one laptop mic | none | ≥ 90% | ★☆☆☆☆ |
| Noisy room, far away, whisper | 84–96% (simulated by distorting training clips: circular) | none | ≥ 90% | ★★☆☆☆ |
| False alarms | 0/186 and 0/90 (training files) · 1/500 = 0.2% · 1.67% (trainer) · **9.6 per hour** real room (older model) | none for the current model | ≤ 1 per 8 h | ★★☆☆☆ |
| Wake speed | < 35 ms (classifier only) | with the speech check: +0.25–2 s; a cooldown bug can drop verified wakes | ≤ 400 ms (p95) | ★★★☆☆ |
| "Nexus open Chrome" in one breath | n/a | **the part after "nexus" is lost by design** (capture starts only after detection + check) | ≥ 95% | ★☆☆☆☆ |

### Speech-to-text (turning your voice into a command)

| What | Claimed in docs | Proven (real audio) | Target | Stars |
|---|---|---|---|---|
| Raw word accuracy | Moonshine small 92.2% (7.84% WER, vendor figure) · Groq turbo about 95% on clean English (external) | never measured on NEXUS audio | ≥ 95% | ★★★☆☆ |
| Understands the command (text to action) | 98% (49/50 typed phrases) · 90.0% NLU candidate · 86.3% shipped NLU | typed text only, no audio | ≥ 95% | ★★★★☆ |
| Hallucination protection | filters exist; no rate was ever recorded | gaps: "thanks for watching"-style phrases leak; real commands containing "you.", "bye." or unusual words (e.g. "list pull requests", "order pizza from Dominos") are likely wiped | leaks ≤ 0.5%, wrongly dropped ≤ 0.5% | ★★☆☆☆ |
| "NEXUS" at the start of a command | works only if spelled exactly "nexus" | Whisper hears "lexus" 37% of the time in isolation; "next us", "nixes", "nexis" are not handled | ≥ 98% | ★★☆☆☆ |
| End to end (wake, capture, STT, action) | 66–73% (modelled, not measured) | none | ≥ 90% | ★★☆☆☆ |

**Overall: ★★☆☆☆ — the building blocks are decent, but nothing is proven, and several concrete bugs cap real accuracy.** The next step is to replace this table's "claimed" columns with measured ones.

## What is actually wrong (evidence-ranked)

Wake word chain (all from `src-tauri/src/wakeword_oww.rs` unless noted):
1. **Command speech after "nexus" is not captured.** `start_stt_capture` starts empty (:2746-2761) after detection plus the speech check; "open Chrome" said in one breath is clipped or lost.
2. **A verified wake can be eaten by the 1.5 s cooldown.** `LAST_NEURAL_FIRE_MS` is stored (:3525) before the check finishes; the second candidate sent by `fire()` (:3798) is suppressed by :3498, and a stale `VERIFIED_BYPASS` can later fire an unrelated, unverified candidate.
3. **Cold local engine stalls the wake.** The local STT server is killed after 60 s idle (`lazy_stt.rs:25`, `lib.rs:782`); the first wake waits up to 10 s, then fails open.
4. **Name handling is exact-spelling only.** The verifier's near-word list (:1950) and `strip_wake_prefix` (`intent_parser.rs:274`) don't match: next us, nixes, next sis, nexxus.
5. **The command vocabulary hint reaches only enrolled owners** (:3394); with speaker verification off (default), Groq gets a neutral prompt.
6. **Training and runtime see different audio.** Four gain schemes; runtime multiplies loud speech by 2.5x and clamps; no anti-alias filter before 16 kHz; the bundled `acoustic_profile.json` is tuned to the developer's own mic and ships to everyone; thresholds 0.35 / 0.45 / 0.50 / 0.68 appear in different places.
7. **Endpointing uses raw RMS** (:2580-2603): a noisy fan never reaches silence, a quiet mic is rejected, a 400 ms pause cuts a command.
8. **The trainer overstates quality** (`scripts/train_local_wakeword.py`): random window split leaks the same recording into train and validation; `best_weights = state_dict().copy()` is a shallow copy so "best epoch" is just the last; no seed; the user's `free/` recordings are never read; no real-noise mixing, reverb or time shift.
9. **Settings contradict each other:** `wakeSensitivity` is read by nothing; `localSttOnly` is true in defaults but false in the readers; a settings parse error silently resets everything.
10. **Model manifest is stale:** manifest says `2de9887…`, the shipped model is `563b4af…`; the wake-data gate is not in CI.

STT chain (`src-tauri/src/stt.rs`, `stt_groq.rs`, `intent_parser.rs`):
11. **Hallucination filter drops real commands:** substring match on "you." / "bye." / "thank you." (:592-597) and the common-word gate (<30% common words) wipe phrases like "list pull requests", "order pizza from Dominos", "tell mom I love you". (Found with a hand port to Python, so it must be confirmed in Rust first.)
12. **Uncovered hallucination classes:** "thank you so much for watching", "see you in the next video", "like and subscribe", "thanks for listening", "Amara.org".
13. **No confidence rejection in the main path**; `no_speech_prob` is always 0.000 on `whisper-large-v3-turbo`; Moonshine fallback gets only the text filter.
14. **Labelled-data gap:** zero real-microphone labelled utterances in the repo; `collect_nlu_samples.py` stores no audio and lets the user edit the transcript.

## Plan

### Phase A — Make the scorecard real (build the measurement; no model changes yet)

- **A1. Held-out recording tool.** Add a `heldout` mode to `scripts/record_wake_samples.py` that writes a manifest using the schema already in `scripts/wake_data_foundation.py` (speaker, session, mic, phrase, distance, noise, volume). Protocol: 3–4 speakers × ("NEXUS", "hey NEXUS", "okay NEXUS") × ~30 takes, across near/far, quiet/noisy, normal/whisper/fast; plus 60 real commands each, plus 30+ minutes of out-of-sample room audio (TV, talk, silence) for false alarms. These are never used for training.
- **A2. Rust-faithful wake evaluator.** A small Rust example/bin (e.g. `src-tauri/examples/wake_eval.rs`) that feeds the manifest's WAVs through the real engine in `wakeword_oww.rs` (same high-pass, AGC, frame logic, threshold, cooldown), with the stage-2 check on and off. Output: recall per phrase / speaker / condition with 95% confidence intervals, false alarms per hour, time-to-fire.
- **A3. STT evaluator, two stages.** `scripts/stt_eval.py` runs each command WAV through the production path (Groq when a key is set, else Moonshine, using the exact production prompt) and records raw transcripts + WER (`jiwer`). A Rust example then applies the real `apply_hallucination_filter`, `strip_wake_prefix` and `parse_deterministic` and compares to the expected intent. Reports: WER, command exact-match, filter false-rejects, hallucination leaks on the silence/noise clips, "nexus"-prefix handling.
- **A4. Scorecard generator.** `scripts/voice_scorecard.py` merges A2 + A3 JSON into the table above (measured values, stars from the thresholds in this plan) and writes it to `docs/`. Reusable as `nexus voice score`. Baseline run happens **before** any fix so improvements are provable.

### Phase B — Fix the known bugs (independent of new data; verify each with A2/A3)

- **B1 (risk 2):** let a verified candidate bypass the cooldown (check `VERIFIED_BYPASS` first) and clear the bypass on timeout. Test: unit test over the candidate loop's decision function (extract it to a pure function).
- **B2 (risk 1):** record the ring position at detection and seed `STT_CAPTURE_BUFFER` with audio that arrives after the wake word, so one-breath commands survive. Verify with A2/A3 on the one-breath clips; fall back to the current behaviour if the harness shows regressions.
- **B3 (risk 3):** if the local engine is cold, skip or shorten the stage-2 check and fail open immediately; align the 10 s verifier timeout with the 20 s health wait; optionally keep the sidecar warm while a ghost session or recent wake is active.
- **B4 (risk 4):** one shared wake-name list (new small module) used by both the verifier and `strip_wake_prefix`: add next us, nixes, next sis, nexxus, nexis, lexus-class variants, commas/periods. Tests for every form, in the same style as `intent_parser.rs:6534-6571`.
- **B5 (risk 5):** use the owner command vocabulary prompt when ownership is `Unenrolled`, not only `Verified`. A/B it with A3 first: prompts can cause words to be invented on silence, so keep it only if the hallucination-leak rate does not rise.
- **B6 (risks 11–12):** fix the filter: match "you." / "bye." / "thank you." only as the whole utterance; exempt command vocabulary (derive an allowlist from `NEXUS_VOCABULARY` and the parser's verbs) from the common-word gate; allow common accented letters; add the uncovered YouTube-style phrases. Add Rust tests with the exact phrases above (confirming or refuting the Python-port finding first).
- **B7 (risk 6, medium):** one shared preprocessing spec (high-pass, gate, AGC target/cap, no unconditional pre-gain on loud speech) used by trainer, evaluator and runtime; low-pass before decimation; per-device calibration writes the profile into AppData and the bundled default becomes conservative. Single threshold source; refresh `model_manifest.json` and add `wake_data_foundation.py validate` to CI.
- **B8 (risk 9, small):** make `wakeSensitivity` move the threshold (or remove it), make `localSttOnly` readers match the struct default, merge settings per field instead of resetting on parse failure.
- **B9 (risk 7, medium):** noise-floor-relative endpointing and a more tolerant first pause. Only if A3 shows capture cut-offs.
- **B10:** Moonshine fallback gets the same filter; on the Groq path, try `verbose_json` compression-ratio / avg-logprob as a confidence signal (the harness decides if it helps; `no_speech_prob` is known dead on turbo).

### Phase C — Train the wake model properly

- **C1. Trainer fixes** (`scripts/train_local_wakeword.py`): `copy.deepcopy` the best weights, seed everything, split by `source_group_id` / speaker / session using the manifest (the leakage check already exists in `wake_data_foundation.py`), evaluate at the deployed threshold with the shared DSP spec, read `free/`, align positives to the word end, add real-noise mixing at 0–20 dB SNR, reverb, random time shift, speed ±8%, gain.
- **C2. Data audit by ear:** a small audition script to listen to and re-label the 345 files moved to `negative/` on 09-24 and the 136 rescued clips; Whisper must not be the judge (it caused the earlier poisoning).
- **C3. Data collection campaign:** you plus 2–3 others record the *training* positives (a separate session from the held-out set), including "okay NEXUS" and standalone "NEXUS", the two weakest phrases. Mine hard negatives from real false alarms (e.g. `necess_0003`) and from the rotating `verify_NN.wav` / `capture_NN.wav` dumps (needs an explicit, privacy-safe opt-in).
- **C4. Calibrate the threshold** from the held-out ROC using the Rust frame logic (target ≤ 1 false alarm per 8 h at maximum recall); ship it as the default; update the manifest.
- **C5. Iterate** score → mine failures → retrain → rescore until every wake row meets its target at the **lower** confidence bound.

### Phase D — STT polish
Learned-correction word boundaries and a hit counter; compare `whisper-large-v3` against `turbo` for the command path with A3; keep the vocabulary prompt under the 224-token limit; record filter hit/leak rates continuously (`stt_filter_stats` already exists but counts empty replies as hits).

## Critical files

- Wake: `src-tauri/src/wakeword_oww.rs`, `src-tauri/resources/oww/{acoustic_profile.json,model_manifest.json,nexus.onnx}`, `src-tauri/src/acoustic_profile.rs`, `src-tauri/src/commands.rs`
- STT: `src-tauri/src/{stt.rs,stt_groq.rs,stt_learning.rs,lazy_stt.rs,intent_parser.rs}`, `frontend/src/audio/recorder.ts`
- Training/measurement: `scripts/{train_local_wakeword.py,verify_hardened_model.py,record_wake_samples.py,wake_data_foundation.py,soak_wake_fa.py,test_device_invariance.py}`; existing test fixture `src-tauri/tests/e2e_voice_fixture.rs`
- New: `src-tauri/examples/wake_eval.rs`, an STT text-stage example, `scripts/stt_eval.py`, `scripts/voice_scorecard.py`, a shared wake-name module, an audition script

Reuse, don't rebuild: the manifest schema and leakage check in `wake_data_foundation.py`; `TurnStats` / `stt:turn_stats` for capture diagnostics; `stt_filter_stats`; the existing 54 wake tests and 194 parser tests; the Linux build scaffolding used in the last session to compile and run Rust tests here.

## Verification

- Every fix gets a unit test; run `cargo test --lib -- --test-threads=1` (baseline 922) and frontend/worker suites.
- The scorecard script is run before Phase B (baseline) and after each phase; the table in `docs/` is the acceptance record.
- Limits of this environment: the Linux container has no recordings, no microphone and no numeric Python stack, and no Windows audio path. I can build and unit-test the harness here (and install the numeric packages from PyPI), but **accuracy numbers come only from your recordings run on your Windows machine.** Until then every "proven" cell stays empty rather than guessed.
- Done when: wake recall per phrase ≥ 95% (lower CI bound ≥ 90%) for each speaker, ≤ 1 false alarm per 8 h on out-of-sample audio, one-breath commands ≥ 95%, STT command exact-match ≥ 95%, hallucination leaks and wrongly dropped commands ≤ 0.5% each.

## Housekeeping

Two commits from the previous task are still unpushed (the Claude GitHub App is not installed for Engine-NEXUS, so `git push` returns 403). They will be pushed as soon as plan mode ends and access exists.
