# 194 — Understanding a misheard command: the plan and how it was built (plan 06, steps 1 to 4)

Date: 2026-11-22. Repository of the code: `Engine-NEXUS/WINDOWS`. Research behind it, with sources, method and every
measurement: [NEXUS-PAPERS `research/voice-accuracy/misheard-words-research-sources-plan-and-implementation-2026-11-22.md`](https://github.com/Engine-NEXUS/NEXUS-PAPERS/blob/main/research/voice-accuracy/misheard-words-research-sources-plan-and-implementation-2026-11-22.md) (also NEXUS-PAPERS `features/59`).

This file is the engineering record: what the plan said, what was built for each step, how the pieces fit, which decisions
were taken and why, how it was tested, and what is still open.

---

## 1. The problem in one paragraph

NEXUS hears a command through Groq's Whisper (or local Moonshine offline) and gets text back. Speech recognisers confuse
words that sound alike: the owner's own logs contain "open load pad" (Notepad), "open brief" and "open breath" (Brave),
"open spot if I" (Spotify), "hook up anti-pilot" (Antigravity), "send down my laptop" (shut down). Before this work NEXUS
could only recover such slips through hand-written lists in `intent_parser.rs::normalize_phonetic_mishearings` (every new
slip needed a human to add a line) and through letter-based fuzzy matching in the app registry, which cannot see that
"load pad" sounds like "notepad". When nothing matched, the command fell through to legacy resolvers or the cloud model.

## 2. The plan (plan 06, written 2026-11-17)

Command assistants (Siri, Google, Windows Voice Access, Dragon, Talon, Home Assistant) all do the same five things: know
the user's world, keep alternatives and choose by sound, use the sentence shape, learn from fixes, and ask with numbered
choices when unsure. Plan 06 turned that into five steps for NEXUS command turns, all on the laptop and free:

| Step | Plan 06 wording | Where it lives | Status (2026-11-22) |
|---|---|---|---|
| 1 | Build the recogniser's prompt from the user's world | `stt_groq.rs`, `app_registry.rs` | **Built**, opt-in, merged (WINDOWS PR 70) |
| 2 | Use word confidence (`verbose_json`, low `avg_logprob`) to decide execute-or-ask | none | **Rejected with data**, docs only (WINDOWS PR 71) |
| 3 | Phonetic slot matcher in Rust (closed lists, phoneme distance, margin rule) | `sound_match.rs`, `app_registry.rs` | **Built** for the app slot, merged (WINDOWS PR 69) |
| 4 | Ask when two are close, with numbered choices; the answer is the training signal | `sound_match.rs`, `app_registry.rs`, `memcore/offer.rs`, `orchestrator.rs`, `command_executor.rs` | **Built and run live**, open (WINDOWS PR 72) |
| 5 | Learn by sound: store each correction with its phoneme key, keep the two-sighting guard | `memcore/corrections.rs` | **Not started** |

Plan 09 (same day) fixed the constraint that shapes all of this: NEXUS is free for every user on any laptop. So every
step had to work with **no key and no network** where possible (layer A), cost at most a few megabytes of memory, and
never put a paid model in the path.

Rules that held across all steps:

- A sound-based guess **never acts on its own**. It becomes a question; only the user's answer opens anything.
- Names come back **exactly as stored** in the registry (Home Assistant's rule), never as the model spelled them.
- Installers are never offered as a choice.
- Every new behaviour has an off switch (`NEXUS_NO_SOUND_MATCH=1`, `sttCommandHints`), and the default for anything that
  sends more data to a cloud service stays off.

---

## 3. Step 3 first: the sound matcher (`sound_match.rs`, PR 69)

Step 3 was built before steps 1, 2 and 4 because the others depend on it.

### 3.1 What it does

`PhoneticIndex::new(names)` turns every known app name (each search key of each app, grouped by the app's display name)
into a phoneme sequence. `nearest(query)` turns the heard words into phonemes and returns the single clearly-best name:

- **Pronunciation.** The CMU Pronouncing Dictionary (BSD-style licence), compacted into `sound_match_cmudict.txt` by
  `scripts/gen_pron_lexicon.py` and embedded with `include_str!` (paged in from the executable, never copied to the heap).
  A hand table covers brands the dictionary lacks or says oddly (`nexus`, `spotify`, `whatsapp`, `antigravity`,
  `vscode`, `windsurf`, `kilo` …). Unknown words fall back to a letter-to-sound rule.
- **Distance.** A phoneme edit distance where a vowel-for-vowel swap costs 0.35, a swap inside a confusable consonant
  group (p/b, t/d, k/g, f/v, s/z/sh/zh, ch/jh, m/n/ng, l/r, th/dh, w/v) costs 0.4, any other swap 1.0 and an insert or
  delete 0.8, normalised by the longer sequence (0 = same sound, about 1 = unrelated).
- **Decision.** Accept only if the best distance is at most `MAX_DIST` = 0.34 **and** it is at least `MIN_MARGIN` = 0.16
  better than the nearest *different* app (names of one app share a group, so "chrome" and "google chrome" are not rivals).
- **Footprint.** About 0.5 MB of heap for a line-offset index built on first use; about 1 ms per query over 100 apps in a
  debug build. It deliberately does not load the offline voice's G2P (+125 MB measured).

### 3.2 Where it plugs in

`app_registry::assess_open_target(query)` is the launch gate every spoken "open X" goes through. It already had tiers
(resolved phrase, exact key, prefix, contains, fuzzy) and an installer refusal. PR 69 added `MatchKind::Phonetic`,
consulted **only** when the older tiers find nothing, and `kind_may_launch` returns false for it: a phonetic hit always
becomes `OpenAssessment::Confirm` ("I couldn't find 'load pad' exactly — did you mean Notepad?") plus a
`memcore::offer::Offer::OpenApp`. A "yes" calls `confirm_resolution(spoken, display_name)`, so the phrase resolves
directly next time.

---

## 4. Step 1: the speech hint from the user's own apps (PR 70)

### 4.1 Before

`sttCommandHints` (default off) sent a fixed 16-sentence prompt ("Open Chrome. Open WhatsApp. …") to Groq on turns from
unenrolled voices. It was never built from the user's apps.

### 4.2 Built

- `app_registry::pick_hint_names(entries, n)` / `hint_names(n)`: the user's most-used launched apps (highest
  `use_count`), installers excluded, one entry per display name.
- `stt_groq::entity_prompt_text(names)`: a bare, comma-separated list ("Brave, Spotify, WhatsApp.") capped at
  `HINT_MAX_NAMES` = 12 names and `HINT_MAX_CHARS` = 180 characters, cleaned to letters and digits.
- `stt_groq::entity_decoder_prompt()`: cached (rebuilt only when the list changes), falls back to the neutral prompt when
  the user has opened nothing yet. `decoder_bias_prompt(DecoderBias::Commands)` now returns it.
- A counting bug blocked the whole idea: `record_usage` only counted the exact search phrase, so every `use_count` was 0
  and the hint would always have been empty. `count_use(&entry)` now counts **the app that was actually opened** (called
  at both the focus-existing and launch paths of `command_executor::resolve_and_open_app`), and `record_usage` no longer
  double-counts.

### 4.3 Why a bare list, and why still off by default

The A/B in the research file: the old sentence prompt made Whisper write command words into **6 of 6** silent or noise
clips and gained nothing; the bare list of the user's apps fixed "anti-gravity" (25 to 28 of 30 command clips) with no
new text on silence. The clips were clean synthetic speech, so the gain is an upper bound; the setting stays off until a
real-microphone trial. App names (never paths) go to Groq only on turns where the setting is on.

### 4.4 Checked live

A rebuilt app opened Calculator once and Notepad twice through `execute_command`; the counts persisted (1 and 2), so the
hint now has real favourites. "kalkulator" got a "did you mean Calculator?"; nonsense got "not found".

---

## 5. Step 2: word confidence, rejected (PR 71, documentation only)

Plan 06 proposed asking whenever Whisper was unsure. Groq's `verbose_json` gives one `avg_logprob` per **segment**, not
per word, and on the 168 A/B transcriptions it did not separate right from wrong short commands (right median −0.48,
wrong median −0.45). A threshold would ask about many correct commands and still miss the wrong ones. Decision: the
"execute or ask" decision comes from the **sound-match margin** (step 4), not from recogniser confidence. The existing
`sttConfidenceGate` remains a garbage filter for invented text.

---

## 6. Step 4: "Clock or Block? Say one or two" (PR 72)

### 6.1 What the user hears

| User says | Before | After |
|---|---|---|
| "open slap" | nothing found (the margin rule refused because Slack and Skype were equally close) | "I heard 'slap' — Slack or Skype? Say one or two." |
| "open windsor" | "did you mean Where Winds Meet?" (a text guess: the key "winds") | "I heard 'windsor' — Windsurf or Winword? Say one or two." |
| then "the first one" / "one" / "slack" | — | opens Slack and remembers "slap" means Slack |
| then "no" / "not now" | — | "Understood, sir. I won't open anything." |
| then a bare "yes" | — | "Which one, sir? I heard 'slap' — Slack or Skype? Say one or two." |
| then a mumble ("dirt.") | — | asked once more with the same question |
| then "what time is it" | — | the question is dropped and the sentence runs as a normal command |

### 6.2 Data flow

```
voice -> STT text -> intent "open <phrase>" -> command_executor::resolve_and_open_app(phrase)
   -> app_registry::assess_open_target(phrase)
        tiered lookup (resolved / exact / prefix / contains / fuzzy)
          confident  -> Proceed (launch)
          contains/fuzzy guess -> lookup_phonetic_pair? -> Pick   (else Confirm, the single question)
        nothing found -> lookup_phonetic (one clear sound match) -> Confirm
                      -> lookup_phonetic_pair (two close whole names) -> Pick
                      -> ProceedLegacy
   Pick -> memcore::offer::set(Offer::OpenAppPick{choices, spoken}, armed) ; speak pick_question(...)
next turn -> orchestrator::process_transcript offer intercept
   pick_route(choices, transcript, classify(transcript))
     Answer   -> run_offer_reply: parse_pick -> confirm_resolution(spoken, chosen) -> resolve_and_open_app(chosen)
                                  no pick (a "no"/"later") -> "I won't open anything"
     AskAgain -> re-arm the offer, speak "Which one, sir? …"
     NotAnAnswer -> drop the offer, handle the sentence normally
```

### 6.3 The pieces

**`sound_match.rs`**
- `enum Found { One(Hit), Close(Hit, Hit) }`.
- `PhoneticIndex::nearest_or_close(query)`: returns `One` exactly when `nearest` would (same thresholds); otherwise sorts
  every name by distance and returns `Close(best, runner_up)` when the best is within `MAX_DIST` and the nearest
  **different** app is within `MAX_DIST + PAIR_SLACK`. `PAIR_SLACK` is **0.0** (it started at 0.06; see 6.4).

**`app_registry.rs`**
- `OpenAssessment::Pick { entries: [AppEntry; 2] }`.
- `lookup_phonetic_pair(query)` / pure `phonetic_pair_in(cache, q)`: drops the pair when the two are one app under two
  names, when either is an installer (`is_installer_name`, `launch_looks_like_installer`), or when either matched on only
  part of a longer name.
- `is_whole_name_key(key, display_name)`: the display name, lower-cased, with parenthesised tags ("(User)", "(x86)") and
  tokens containing digits (versions) removed; the key must equal it, or be a prefix covering at least 60% of it, or do
  either after dropping a vendor prefix (`google `, `microsoft `, `windows `). "spline" for "Spline 0.20.51", "kiro" for
  "Kiro (User)", "photos" for "Google Photos" pass; "link" for "Phone Link", "compass" for "MongoDB Compass", "steam" for
  "Steam Support Center" fail.
- `assess_open_target`: a `Pick` when nothing matched but a pair exists, and when the tiered lookup produced only a
  Contains/Fuzzy guess and a whole-name pair exists (sound over the whole phrase is stronger evidence than a substring).

**`memcore/offer.rs`**
- `Offer::OpenAppPick { choices: Vec<String>, spoken: String }`, reply window `REPLY_WINDOW_LOW` like the single question.
- `pick_question(spoken, choices)` builds the spoken line.
- `parse_pick(text, choices) -> Option<usize>`: after removing filler ("the", "number", "option", "pick", "open",
  "please", "nexus", "sir", …) and a noun "one" ("second one"), a single remaining ordinal decides ("one", "1", "first",
  "won" → first; "two", "2", "second" → second; "to"/"too" → second only when nothing else is left). Otherwise a word of
  three or more letters that belongs to exactly one of the names picks it ("the chat one"); a shared word ("google") or two
  names at once is no pick. More than nine words is never a pick.

**`orchestrator.rs`**
- `enum PickRoute { Answer, AskAgain, NotAnAnswer }` and pure `pick_route(choices, transcript, reply)`.
- Two new arms in the offer intercept of `process_transcript` (before the generic arms): `AskAgain` re-arms the offer and
  speaks "Which one, sir? …"; `Answer` goes to `run_offer_reply`.
- `run_offer_reply` arm for `OpenAppPick`: the pick calls `app_registry::confirm_resolution(spoken, chosen)` and then
  `command_executor::resolve_and_open_app(chosen)`; no pick → "I won't open anything".
- The mumble re-ask now speaks the pick question for `OpenAppPick` instead of "was that a yes or a no?".
- `may_answer_offer`: an `Uncertain` voice can never answer `OpenAppPick` (it launches an app), same as `OpenApp`.

**`command_executor.rs`**
- `resolve_and_open_app` handles `Pick`: sets the offer (armed, because its question is being spoken now) and returns
  `CommandResult { success: false, message: pick_question(...) }`, the same shape as the single question, so every
  caller speaks it and opens the same answer window.

### 6.4 Decisions taken while building, and why

1. **Margin, not confidence, decides when to ask.** From step 2's data.
2. **Two choices, never three.** Voice Access shows a numbered list on screen; NEXUS asks by voice, where two options are
   easy to hold in mind and "one or two" is easy to say and to recognise.
3. **A bare "yes" asks again rather than picking the first.** Guessing would defeat the point of asking.
4. **Anything that is not an answer drops the question.** Same rule as every other offer: NEXUS never traps the user in a
   question.
5. **Whole names only** (added after checking the owner's 320-app list: "lock" was being asked as "Clock or Phone Link?").
6. **No runner-up slack** (same check: second choices at distance 0.39 were never sensible: "trey" → Google Drive,
   "jim" → Gmail).
7. **A sound pair beats a substring guess** (found in the live run: "windsor" was asked as "Where Winds Meet?").
8. **The pick teaches through the existing `confirm_resolution`**, exactly like a "yes" to the single question. Storing
   the phoneme key and requiring two sightings is step 5, kept separate so this change stays small.

---

## 7. How it was tested

### 7.1 Unit tests (all in `src-tauri`, `cargo test --lib`)

| Area | Tests |
|---|---|
| `sound_match::close_tests` | a close pair is returned (Clock/Block for "lock", Notion/Motion for "potion"); a clear winner stays `One`; far phrases give nothing; names of one app are not rivals |
| `memcore::offer` | `a_number_picks_one_of_two` (one, 1, the first, number one, first one, option 1, won / two, 2, second one, to, too); `a_distinctive_name_word_picks_one`; `anything_unclear_is_not_a_pick` (yes, maybe, a long sentence, "go to the settings page"); question text and reply window |
| `app_registry` | close pair found, clear winner not a pair; installer and same-app exclusion; `only_whole_names_can_be_one_of_the_two_choices`; `a_pair_with_one_word_of_a_longer_name_is_not_asked`; the launch gate returns `Pick` for "lock" (Clock + Block) and for "windsor" (Windsurf + Winword + Where Winds Meet) and still opens "clock" at once; the older `gate_asks_instead_of_launching_guesses` still passes ("mummy chat", "chroem" keep the single question) |
| `orchestrator` | `replies_to_the_numbered_app_question_are_routed_safely`; `who_may_answer_an_offer` extended with `OpenAppPick` |

Full library suite on the final code: 3,376 passed, 0 failed (37 ignored); one network-probing test
(`test_connect_state_live_bridges`) failed once while a NEXUS instance was starting and passes on its own. Repository
gates: dead-code ratchet (139 known, none new), IPC contract, turn-end, hygiene, window opacity, hard-coded colours.

### 7.2 Offline probe against the owner's real app list

A throwaway test loaded the owner's real `%APPDATA%\nexus\app_cache.json` (320 apps, 836 search keys) and ran 69 typed
phrases through the matcher. It found the problems in decisions 5 and 6; after the fixes only Slack/Skype ("slap"),
Windsurf/Winword ("windsor") and Kiro/Curl ("kilo") remained as pairs. Details in the research file, section 5.4.

### 7.3 Live run in the real app

A contributor release built from the PR branch (`node nexus.mjs build`, isolated identity
`com.nexus.assistant.contributor`) was started on its own hidden Windows desktop in quiet mode, with a debug port, and
driven through its real commands: `execute_command {action: "open_app"}` for the launch gate and `orchestrator_process`
for the answers. The owner's personal NEXUS was stopped for the run at the owner's request (the contributor build refuses
to start while another NEXUS is running).

| Step | Observed |
|---|---|
| open "slap" | "I heard 'slap' — Slack or Skype? Say one or two."; log `[SOUND-MATCH] 'slap' sounds like both 'slack' (0.25) and 'skype' (0.34)` |
| "no" | "Understood, sir. I won't open anything." |
| open "slap", then "yes" | "Which one, sir? I heard 'slap' — Slack or Skype? Say one or two." |
| open "slap", then "the first one" | opened Slack; the resolution cache gained `slap → Slack` |
| open "slap" again | opened Slack directly ("Ok sir."), no question |
| open "windsor" (first build) | "did you mean Where Winds Meet?" — the gap that produced decision 7 |
| open "windsor" (rebuilt) | "I heard 'windsor' — Windsurf or Winword? Say one or two." |
| open "kilo" | single question "did you mean Kiro (User)?" (its runner-up matched only "cli" of "Spotify Cli" and was dropped) |
| "what time is it" while a question waited | handled as a normal question |

The shared registry files were backed up before the run and restored afterwards; Kiro, opened by a "yes" during the run,
was closed.

---

## 8. What is not done or not proven

- **Real speech.** Every phrase in the live run was typed into the real command path; no microphone audio was used, so
  how often real speech produces a close pair is unmeasured. Live `[SOUND-MATCH] ... both` log lines will show it.
- **The new re-ask wording for a mumble** and the spoken "two" are unit-tested, not observed live: a second batch in the
  live run was answered by the owner's personal build, which something relaunched during the test (its parent process had
  already exited and it was not the logon task).
- **Step 3 covers only the app slot.** Verbs ("send down" → "shut down"), modes ("ghost moon") and contacts still rely on
  the hand-written lists.
- **Step 5** (learn by sound with the two-sighting guard) is not started.
- **Two pre-existing issues found on the way, not fixed here:** `cargo test` writes to the real
  `%APPDATA%\nexus\app_cache.json` and `app_resolution_cache.json` (it inserts "CustomTestApp" and touches timestamps), and
  the personal and contributor builds share that folder, so a contributor test can change the owner's learned phrases.

## 9. Files

| File | Change |
|---|---|
| `src-tauri/src/sound_match.rs` | phoneme table, distance, `PhoneticIndex`, `nearest`, `nearest_or_close`, `Found`, tests (PR 69, PR 72) |
| `src-tauri/src/sound_match_cmudict.txt`, `.LICENSE`, `scripts/gen_pron_lexicon.py` | pronunciation data and its generator (PR 69) |
| `src-tauri/src/app_registry.rs` | `MatchKind::Phonetic`, `lookup_phonetic`, `lookup_phonetic_pair`, `is_whole_name_key`, `OpenAssessment::Pick`, `pick_hint_names`, `hint_names`, `count_use` (PRs 69, 70, 72) |
| `src-tauri/src/stt_groq.rs` | `entity_prompt_text`, `entity_decoder_prompt`, caps (PR 70) |
| `src-tauri/src/command_executor.rs` | `count_use` calls, `Pick` handling (PRs 70, 72) |
| `src-tauri/src/memcore/offer.rs` | `Offer::OpenAppPick`, `pick_question`, `parse_pick` (PR 72) |
| `src-tauri/src/orchestrator.rs` | `PickRoute`, `pick_route`, intercept arms, reply arm, mumble re-ask, `may_answer_offer` (PR 72) |
| `scripts/prompt_ab.py` | the speech-prompt A/B (PR 70) |
| `docs/changes/201`–`204` | per-step change records |
