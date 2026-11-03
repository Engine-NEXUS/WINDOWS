# Administrator, Voice Lock and wake-word training: read-only audit

Date: 2026-11-01. Read-only. No source edited, nothing built or launched, no voice profile or secret opened.
Legend: **[code]** = verified by reading the source at the cited line; **[doc]** = stated in project docs or
commit messages (not re-run here); **[inferred]** = my reasoning from the code, not measured.

## 0. Short answers to the owner

**(a) How does the Administrator feature work today?** It does not open. The code understands the spoken
commands, checks that the words came from the real microphone, writes an audit line and then refuses. There is
no voice check for the Administrator, no face check, no challenge phrase, no TOTP and no paired laptop in code.
Nothing an Administrator could do is live. **[code]**

**(b) Can training with more voices make the wake word better and also recognise you?** Partly yes, but it is
two different models with two different jobs, and they must be trained in opposite ways:

- The **wake word model** (`nexus.onnx`) answers *"was the word NEXUS said?"*. It is meant to fire for
  **anyone**. It is already trained on your takes plus thousands of synthetic voices. More real voices saying
  "NEXUS" make it better at hearing the word; they do **not** make it recognise you, and they make it accept
  those people more readily (which is its job).
- **Voice Lock / speaker verification** answers *"is this the owner?"*. Here other people's voices are useful
  only as **negatives** (impostors) to set and test the threshold, while more of **your** clips (different
  days, rooms, mics, a cold) make the owner template stronger.
- Big caveat: today Voice Lock does not use a speaker model at all. It reuses the wake word's own feature
  extractor (`embedding_model.onnx`) and averages it. That extractor was built to recognise *words regardless of
  speaker*, so it is a weak identity signal. Retraining the wake word will not fix that; a dedicated speaker
  embedding model is needed. "Perfect" is not achievable for voice alone (NIST forbids voice biometrics as an
  authenticator); "very good plus a second factor for risky actions" is.

## 1. Administrator feature

### What is built **[code]**

| Piece | Where | What it does |
|---|---|---|
| Command grammar | `src-tauri/src/admin_privilege.rs:263-393` | Exact phrases: start/end/status of an "administrator session", and "admin review <repo / PR n in repo / latest PR in repo / this repo> to my laptop / to the user sidebar". Anything else starting with "admin" is claimed as `Unsupported` so the cloud LLM never interprets it (`:321-334`, `:366`). Grammar is independent of learned STT corrections, NLU and fuzzy matching (`:207-208`). |
| Native capture proof | `admin_privilege.rs:16-73` | Rust STT registers the exact transcript with a session number; it is one-use, exact-text, 60 s TTL, max 8 (`:11-14`, `:39-47`). Registered only from the native receiver (`wakeword_oww.rs:4148`, `:4176`), invalidated on cancel (`wakeword_oww.rs:3435`). A WebView cannot mint one. |
| Session clock | `admin_privilege.rs:75-103` | 15 min max, 5 min idle. **The `Session` struct has no production constructor**: the only `Session { .. }` literals are in tests (`:595`, `:600`, `:611`). |
| Capability policy | `admin_privilege.rs:111-122` | Requires capture proof, then a live session; export to a laptop additionally needs a paired laptop and device-user approval. |
| Orchestrator intercept | `orchestrator.rs:928-977` | Begin session always answers "Administrator verification is not available on this device yet. Both voice and face must be enrolled..." (`:946-950`). Review is called with `paired_laptop = false` hard-coded (`:961`), so it always fails. Everything is audited as deny / blocked (`:938-944`, `:970-976`). |

### What is only planned **[doc]**

`docs/features/145-administrator-control-implementation.md:21-31` lists phases A0-A6. Only **A0 (native
provenance)** is started (`:25`, `:35`). Not built: A1 trusted identity (device signing key, separate voice and
face enrollment, templates only), A2 first-device bootstrap with signed lease, A3 live ceremony (speaker match +
replay/spoof rejection + face match + presentation-attack detection + fresh spoken challenge), A4 session and
actions through `prepare_action`, A5 fresh review and delivery, A6 release evidence. Face, liveness, anti-spoof
and challenge code does not exist in `src-tauri/src` (grep found no verifier). TOTP is not part of the
Administrator design at all; it appears only in the separate "NEXUS Cloud" vision (feature 121).

### Important structural gap **[doc + code]**

The ordinary wake-owner gate rejects a non-owner **before transcription** when Voice Lock is on
(`wakeword_oww.rs:4231-4238`). An Administrator at someone else's laptop is a non-owner there, so A3 must add a
separate "Administrator candidate" branch before that filter (`145-...md:28`, `:35`).

Also note: `admin_config.rs` is **not** the Administrator identity. It is the admin-brain switch (Qwen local
model) read from `admin.json` (`admin_config.rs:1-10`, `:48-60`). Do not confuse the two.

**Nothing Administrator-related is live.** What an Administrator can do today: ask, get a polite refusal, and
close a session that cannot exist.

## 2. Voice Lock (speaker verification)

### How it works **[code]**

- **Embedding model**: the openWakeWord `embedding_model.onnx` (same file as the wake word), 16 frames x 96
  dims, mean-pooled to one 96-dim vector (`voice_profile.rs:5-18`, `:375-386`). The 1.28 s most energetic
  window is used for both enrollment and live checks (`:353-373`).
- **Enrollment**: exactly 10 clips of 1-5 s; first 5 stored, last 5 held out for calibration and never stored
  (`commands.rs:497-523`). Phrases "nexus", "hey nexus", "ok nexus" (`:521`).
- **Threshold**: `calibrate_owner_threshold` = (lowest held-out owner score - 0.05), clamped to 0.45..0.95,
  fails enrollment if the owner is inconsistent (`voice_profile.rs:331-346`). **Only owner clips are used. No
  impostor data sets the threshold** [code]. 0.45 is a borrowed OVOS default (`:23-29`).
- **Decision**: max cosine over stored vectors (`:315-327`); Verified >= threshold, Uncertain within 0.10 below,
  else Rejected (`:120-122`, `:252-263`). Two checks: at wake fire (`wakeword_oww.rs:4518-4547`, fails closed on
  None, `:4231-4238`) and per captured turn (`wakeword_oww.rs:4462-4507`, routing gate `orchestrator.rs:835`,
  `voice_profile.rs:99-101`: when locked, only Verified voice turns run; keyboard/UI stay available).
- **Storage**: sealed AES with its own OS-keychain key `voice_profile_data_key_v1`, read back through a fresh
  handle to reject a non-persistent backend (`voice_profile.rs:41-71`), atomic pending-file write and shred of
  the old file (`:195-214`). Stores embeddings, not audio. Invalid / missing profile fails closed.
- **Growth**: `add_embedding` keeps up to 40 vectors FIFO (`:31-34`, `:219-224`) but it is called only from
  `enroll`; **there is no implicit enrollment / adaptation in production** [code, grep].
- **Replay / liveness**: none. There is no anti-spoofing model anywhere [code, grep].

### Measured results

| Evidence | Value | Status |
|---|---|---|
| Owner verified turns | 0.944 (`AGENTS.md:125`), 0.942 (`:77`), 0.960 (`:73`) | single diagnostic turns, not a rate [doc] |
| Owner acceptance rate | none | [doc] "No formal owner/impostor rate" (`docs/testing/scorecard-data.json:90`) |
| Non-owner / TV / replay | 0 qualified trials | [doc] owner is alone; `qualified_corpus_trials: 0` (`runs/windows1-20261026-speaker-benchmark-assessment.json`) |
| Offline benchmark tools | `speaker_benchmark.py`, `speaker_embedding_scores.py` + 63 tests, 18 mutations | software checks on synthetic fixtures only (changes 168, 169) |
| Dedicated model | WeSpeaker ResNet34 ONNX / SpeechBrain ECAPA audited on paper | not downloaded, not selected (`docs/research/speaker-embedding-candidate-audit-2026-10-26.md:3`, `:17`) |

**Real measured accuracy: unknown.** Acceptance gate is >= 95 % owner acceptance and zero executable non-owner
actions over >= 300 qualified negatives plus live unfamiliar voices, replay, restart and profile recovery
(`docs/testing/voice-lock-phase-closeout.md:15`).

### Why the score is 41 % **[code + doc]**

`scripts/make-work-cards.mjs:17`: weights B 0.2, T 0.2, L 0.3, S 0.15, I 0.15. Row values (`scorecard-data.json:85-89`):
B 75, T 55, **L 10**, S 50, I 30 -> 15 + 11 + 3 + 7.5 + 4.5 = **41**. The 30 %-weight live column is 10 because no
owner/impostor/replay trial set exists; S and I are low because there is no anti-spoofing and a word-feature
embedding instead of a speaker model. Gates to advance: raw >= 90 and B>=90/T>=70/L>=70/S>=80/I>=60
(`make-work-cards.mjs:18-19`).

## 3. Wake word model

### How it is trained **[code + doc]**

`scripts/train_local_wakeword.py:1-40`: a small classifier on top of openWakeWord's frozen mel + embedding
models, focal loss, group-level 70/15/15 split by recording or manifest source group (`wake_train_lib.py:5-6`,
`:115-119`), train-only augmentation (speed, reverb, noise 0-20 dB, telecom, fan), seeded, writes
`nexus.candidate.onnx`, `--promote` replaces. Label gate: refuses to train until `wake_label_audit.py` flags are
reviewed by ear (`:21-24`). Owner takes (`owner_mic`, `by_ear`, `:154`) get full augmentation and 3x repeat
(`:241`); 30 % are sealed out of every split for scoring (`:242`, `:168`). Positives also include synthetic
Piper/Edge/SAPI voices (`wake_data_synth.py`) and public negatives/background (LibriSpeech etc. via
`wake_data_fetch.py`). Hard-negative mining streams the unused pool (`:31-32`).

`nexus wake improve` (`scripts/wake_improve.py:1-18`) runs seal -> before -> fetch -> synth -> audit (stops for
the by-ear review) -> train -> after -> verdict IMPROVED/SAME/REGRESSED/NO_DATA; `--promote` only on IMPROVED.
Data lives in `D:\NEXUS-Training\data` behind junctions (verified the folders exist).

**Is it owner-only? No.** It is owner + synthetic + public data. **Does it identify who speaks? No.** It is a
word detector. Identity is Voice Lock's job.

### Last measured result **[doc, commit da4383c, 2026-10-25]**

Shipped model after by-ear audit: false alarms **363/h -> 44/h** on 34.15 h public held-out audio; withheld
owner takes **24.1 % -> 87.6 %** (120/137; the old model had learned them as negatives); 40/40 fresh close-mic
takes. Not measured: per phrase, other speakers, noisy/far/whisper (no `eval_set`). 44/h is far above the
1-per-8-h target. Local `wake_word_data/before.json` / `after.json` show the same numbers [verified file
contents; their timestamps are out of order, so treat them as the commit message describes].

### More voices: what helps which model

| Data | Wake model | Voice Lock |
|---|---|---|
| More owner "NEXUS" takes (days, rooms, mics, ill) | positives; keep 30 % sealed | grow template (more enrolled vectors) and calibration set |
| Other people saying "NEXUS" | **positives** -> better recall for everyone | **impostor trials** -> set threshold from data, measure FAR |
| Other people saying other things / TV | negatives / false-alarm hours | impostor + ambient trials |
| Owner recordings replayed through a speaker | do not care (a word is a word) | **spoof negatives** for an anti-spoof model |

Does adding other voices make the wake model accept them? **Yes, by design**, and that is correct. Voice Lock is
the separation layer: wake fires for anyone, then the speaker check decides owner vs not
(`wakeword_oww.rs:4224-4238`). The risk is the current Voice Lock embedding: because it comes from the wake
model's word-feature extractor, retraining the classifier head does not change it, but if the embedding model
itself were ever fine-tuned on "NEXUS from many speakers" it would get **more** speaker-invariant and Voice Lock
would get **worse** [inferred]. Keep the two feature extractors separate.

### Correct recipe so both improve

1. Separate corpora with consent, grouped by speaker and session; a speaker never spans train and test.
2. Wake model: diverse positives (owner + many real and synthetic speakers), hard negatives (soundalikes, TV),
   hours of held-out room audio for false alarms/hour. Promote only on IMPROVED.
3. Speaker verifier: a dedicated pretrained speaker model (ECAPA/ResNet, not trained on NEXUS data at first);
   owner enrollment (>= 10 clips over >= 3 sessions), held-out owner trials, >= 300 impostor trials from >= 20
   speakers, replay trials. Threshold from calibration split only (the tooling exists: `speaker_benchmark.py`).
   Optional score normalisation (AS-norm) with an impostor cohort.
4. Certification: report owner acceptance and non-owner false accepts with confidence intervals on a sealed
   test set, per condition (near/far/noisy/other mic).
5. Enrollment growth: add only high-confidence, consented owner turns; cap and age vectors; re-check the
   held-out owner set after every change.

### Safe "keeps improving" loop (proposal) [inferred]

Opt-in setting -> a turn scoring well above threshold **and** followed by a confirmed action becomes a
*candidate* embedding (embedding only, no audio, sealed) -> weekly the candidate profile is scored against the
sealed owner set and the impostor cohort -> applied only if owner acceptance rises and false accepts do not; else
discarded and logged -> visible in the Memory page with one-click roll-back. Never adapt from Uncertain turns, from
a turn followed by "no, stop", or while a replay detector is unsure (prevents poisoning by a recording).

## 4. Gaps and risks

- **Replay**: a phone playing the owner saying "NEXUS ..." passes; no anti-spoof [code].
- **TV / other voices**: only as good as a word-feature cosine; not measured [doc].
- **Threshold from owner only**: min-owner minus 0.05 can be loose if impostors score close [code].
- **Sick voice / mic change / Bluetooth / distance**: 5 stored vectors from one sitting; no drift handling.
- **Children / siblings**: similar voices are the hardest case for any verifier.
- **Per-turn check uses the command audio**, not the wake word; short commands give weak embeddings [inferred].
- **Privacy**: embeddings sealed, separate key, shredded on delete [code]; one consented diagnostic clip is kept
  sealed (`AGENTS.md:73`). Wake training audio in `D:\NEXUS-Training` is plain WAV on disk, never pushed [doc].
- **Regulatory**: voice is not an authenticator under NIST SP 800-63B-4 (`145-...md:40`).

## 5. Ranked improvements

1. Run the Voice Lock trial protocol: owner 100+ trials and 300+ qualified non-owner/TV trials (friends, family,
   recordings of public speakers) to get a real rate. Without this nothing else can be judged.
2. Replace the speaker embedding with a dedicated model (WeSpeaker ResNet34 ONNX candidate) after license and
   local latency/RAM check; compare offline with the existing benchmark tool.
3. Set the threshold from impostor data on a calibration split.
4. Add replay/spoof detection (or at least a challenge phrase for risky actions).
5. Enrollment v2: clips across sessions and mics; "add more clips" without full re-enroll.
6. Consented adaptation loop (section 3) with score gate and roll-back.
7. Wake model: build `eval_set` (other speakers, far, noisy), then cut false alarms toward 1 per 8 h.
8. Administrator A1-A3 only after 1-4 exist.

## 6. Eight new identity feature ideas

| Idea | Value | Risk |
|---|---|---|
| Per-person profiles with permissions (owner: all; family: music, timers; guest: questions only) | household use without full lock | misidentification grants wrong rights; needs good verifier |
| Guest mode ("NEXUS, guest mode") | lend laptop safely; no memory, no files, no messages | users forget it is on; time-box it |
| Voice drift alert ("your voice sounds different, re-check?") | catches cold, new mic, profile ageing | false alarms annoy; keep silent unless repeated |
| Challenge phrase for risky actions (read back 3 random words) | blocks replay of old recordings | slower; STT errors cause retries |
| Windows Hello confirm for irreversible actions | strong second factor, already on device | needs Windows Hello hardware/PIN |
| Who-did-what log (owner / guest / rejected per action) | trust and debugging | stores identity history; keep local, sealed, 30 days |
| Child-safe profile (blocks purchases, messages, shutdown) | family safety | children's voices drift fast; re-enroll often |
| "Not me" button on a sidebar card that turns a false accept into an impostor trial | improves threshold from real errors | must not be pressable by the voice that was wrongly accepted |
