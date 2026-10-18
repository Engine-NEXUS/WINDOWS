# 07 - Memory v2: plan and measured baseline (phase M0)

Plan: always-on profile, corrections, routines, and an optional approved background review, ported from the design of Hermes Agent (Nous Research, MIT) onto the encrypted store described in [01](01-overview-and-architecture.md). Phases M0-M5; this file records M0, the measurement every later phase is judged against.

## What is measured

* **`mem Nms` on the `[LAT]` line.** `latency::record_mem` times the whole memory-context assembly for a cloud request (`memory::get_memory_context`, `conversation::conversation_prompt`, the egress-log write) in `orchestrator::dispatch_to_worker_inner`. It is shown after `total`, because it sits inside the `answer` stage rather than adding to it.
* **Recall baseline test** (`memcore::recall_baseline`): 40 facts (20 targets, 20 distractors) and 40 questions, each target asked two ways. `direct` shares a word with the key or value; `paraphrase` shares none.

Re-run (prints the table and every miss):

```bash
cd src-tauri && cargo test --lib --features custom-protocol,admin-brain -- recall_baseline --nocapture
```

## Baseline (2026-10-18, debug build, Windows)

| Measure | Result |
|---|---|
| Recall, direct questions, hit@3 | **20 / 21** |
| Recall, paraphrased questions, hit@3 | **0 / 19** |
| `context_pack` time | p50 **6.3 ms**, p95 **9.9 ms** (unoptimized build; includes redaction and the usage write) |
| Largest pack in the test | 231 characters (about 57 tokens); the hard cap is 2,000 characters |

## What this says

* **Sending memory with a request is not what makes a reply slow.** Building the pack takes single-digit milliseconds locally and adds tens to hundreds of tokens, against a cloud reply measured at about 2.6 s end of speech to first audio (`docs/features/136`). Re-sending a whole chat history would grow without bound; this does not.
* **Retrieval is the weak spot, not speed.** Lexical search finds a fact when the question shares a word with it, and never when it does not (0 of 19). "Where do I work" cannot reach `employer`.
* **One direct miss is a word-form problem:** "allergic" does not match `allergy`. Prefix matching covers plurals only; a light stemmer or per-fact aliases (phase M1) should fix this.

## Next

Phase M1 (profile tier, static-first prompt order, aliases) must raise paraphrase hit@3 and keep `context_pack` p95 under 15 ms in this same test. Embeddings are added only if aliases leave a large gap.

Not yet measured: a release build, a real 2,000+ record store, and a live `[LAT]` line from a spoken cloud turn (needs a rebuild and a voice test).

---

# Phase M1 result (2026-10-18)

Built: synonym families and stemming for recall (`memcore/synonyms.rs`, second pass in `Store::search`, stem prefixes in `fts_hits`), the always-on profile (`memcore/profile.rs`, `Store::pinned_user_facts`, rendered first by `context_pack`), the `memcoreProfile` setting (default on) with a Memory-page switch, and the pack budget raised from 2,000 to 2,600 characters.

## Same test, after

| Measure (40-fact store, hit@3) | M0 | M1 |
|---|---|---|
| Direct questions | 20 / 21 | **21 / 21** |
| Paraphrased questions (dev set) | 0 / 19 | **19 / 19** |
| Held-out paraphrases (20, not used to tune) | n/a | **17 / 20** |

Still missed (held-out): "who do I report to at work" (`manager_name`), "what name do you use for me" (`nickname`), "where did I grow up" (`hometown`).

| Measure (100-fact store, 60 questions) | Result |
|---|---|
| Fact present in the cloud context, profile off | 57 / 60 |
| Fact present in the cloud context, profile on | 57 / 60 |
| `context_pack` time, profile on | p50 **4.4 ms**, p95 **6.5 ms** (debug build) |
| Largest pack | 1,628 characters (about 407 tokens) |
| Profile alone | 1,397 of 1,400 characters |

## Read this before trusting the numbers

* **The dev-set 19/19 is not evidence of generality.** The families were written after the dev questions existed, even though they are general concept groups rather than a list of those questions. The held-out 17/20 is the fairer figure, and it has the same author, so treat it as an upper bound. A blind set written by someone else is the real test.
* **The profile did not raise recall in this test** (57/60 either way). With 100 facts it cannot hold them all, so it holds the oldest ones that fit, and the three misses were not among them. Its value is different and is not measured here: the assistant knows the standing facts about you even when a question shares no word with them ("suggest somewhere for dinner" while "diet: vegetarian" is stored), and the start of the prompt is identical every turn, so providers that cache by prefix can reuse it. The cost is up to about 350 extra tokens on every cloud request.
* **The profile is on by default** and sends your pinned facts on every cloud turn. Health, money, ID-number, credential and belief topics are excluded; mined (not told) facts are never included; the egress log shows what was sent; the Memory page switch turns it off.
* Which facts fill a full profile is decided by age only. Choosing by how useful a fact is comes later (phase M5), because changing the text every turn would defeat prefix caching.

Not measured: a release build, a real store of 2,000+ records, and a live spoken turn.

---

# Phase M2 result (2026-10-18): learning from corrections

Built: `memcore/corrections.rs` (detector, learned-word cache, turn hook), a `corrections` table in the store (`Store::corrections_observe / corrections_all / corrections_promoted / correction_forget`; cleared by "forget everything"), `memcore_corrections` and `memcore_correction_forget` commands, and a "Words I learned to fix" list on the Memory page with a Forget button.

## How it works

* **What counts as a correction.** (1) You say "no, I said open brave" / "I meant ..." / "it's ..." after a command: the words that differ from the previous command become (heard, meant). (2) You repeat a command that just ran, within 25 s, with one word changed ("open brief" then "open brave"). The two phrases must look alike by letters or consonants ("brief"/"brave" do, "chrome"/"notepad" and "mom"/"dad" do not), so two different commands are never mistaken for a correction. "No thanks" and a bare "no" teach nothing.
* **Not learned from one sighting.** A pair is applied only after it was seen on two separate occasions at least 60 s apart. The same utterance repeated in a row is one sighting. If two different answers for the same heard phrase tie, neither is applied.
* **Applied narrowly.** Before parsing, only to transcripts that start like a command (open, call, play, message, ...), whole words only. "Give me a brief summary" is never touched.
* **Under your control.** Memory page lists every pair, applied or waiting, and removes any. Learned pairs never leave the device.

## Evidence

| Check | Result |
|---|---|
| Scripted scenarios (explicit cue, re-issue, unrelated commands, timing/outcome gates, polite "no", tie, forget, wipe) | 10 tests pass, including **not promoted after one sighting, promoted after two separate ones** |
| Replay of this PC's real `missed_intents.jsonl` (173 utterances, 74 pairs within 2 minutes, assuming every earlier command ran) | **0 detections**, so no false positives on real speech |
| Parallel test run | passes; `learn_from` touches no global state, only the production hook refreshes the parser's list |

## Read this before trusting it

* The replay shows the detector stays quiet; it does **not** show it catches real corrections, because that log holds commands that failed to parse, not commands that ran on the wrong thing. Recall on real speech is unmeasured until corrected turns accumulate in `conversation.jsonl`.
* Only turns recorded by `conversation::record_conversation_turn` are seen, and the first sighting of any pair never changes behaviour.
* **Deviation from the plan:** the older one-off learners (`power_cmd::learn_pending_alias`, `wa_open::learn_alias`) were **not** migrated. They ask you or verify on screen before learning, which is a stronger signal than this guesser; unifying them is left for later.
* Two unrelated parser tests (`test_launch_spotify`, `test_app_phonetic_mishearings`) fail because "open spotify" now parses as a music-player intent; they were already failing before this work (`AGENTS.md` lists them as foreign Spotify failures).
* Not run: a spoken correction on a real build.

---

# Phase M3 result (2026-10-18): routines you repeat

Built: `memcore/routines.rs`; tables `command_log`, `routines`, `routine_offers` in the store; hook in `conversation::record_conversation_turn`; interception in `orchestrator::process_transcript`; `memcore_routines`, `memcore_routine_save`, `memcore_routine_delete` commands; a "Routines" section and a switch (`memcoreRoutines`, default on) on the Memory page.

## How it works

* **What is watched.** Only four intents: open an app, open a site, open the music player, play music. Every logged command is re-checked: its canonical text must parse back to the same intent, so a saved routine can always be replayed. Messages, files, power, purchases, closing apps and anything with content are never logged.
* **What counts as a routine.** 2 to 4 different commands, each within 3 minutes of the last, seen in at least 3 sessions on at least 3 different days. Six runs on one day is not a habit. A shorter sequence that only appears inside a longer one is folded into it; a shorter one that is clearly more common is kept beside it.
* **How it is offered.** Spoken through the proactive-speech policy (only when it allows speaking), at most once a day, each suggestion at most twice in total. The Memory page shows the same suggestion with a box to name it. By voice: "save that as study mode". Nothing is ever created without that answer.
* **Names.** One to three plain words that are not already a command and do not start like one ("open chrome", "play something nice" are refused).
* **How it runs.** Saying the name (or "run my study mode routine") rewrites the utterance into its steps joined by "then", so the normal compound-task path runs it, with every confirmation gate it already has. "Delete the study mode routine" works by voice and on the Memory page.
* **Capture rules.** "Save that as ..." is only understood while a suggestion is waiting, and delete or run only for names that exist, so ordinary talk is never captured.

## Evidence

| Check | Result |
|---|---|
| Synthetic logs: three evenings of the same three commands | one suggestion: those three steps, 3 sessions, 3 days, "evening" |
| One busy day, two days only, commands 15 minutes apart, a repeated command inside a sequence, an already-saved routine | none of these produce a suggestion |
| Longest sequence preferred; a more common shorter one kept beside it | passes |
| A routine with a WhatsApp send step | the step is routed to the confirmation-gated path (`Subsystem::Mcp`), same as typing the compound command |
| Offer limits | once a day, twice per suggestion, never a third time |
| Voice entry point (`interpret_and_apply`) | run, delete, "save that as" with nothing waiting, and ordinary talk all behave as described |
| Store | add, duplicate refused, run counted, delete, wipe, the 30-routine limit |

17 routine tests plus the 10 correction tests pass; the Memory page tests pass (45).

## Read this before trusting it

* **There is no real command history yet.** The existing diary only records wake and Ghost events, not commands that ran, so the miner could not be tried on your data. It starts logging after the next rebuild, so a first suggestion needs about three days of real repeated use.
* **Not tried by voice.** The spoken offer, the "save that as" answer and running a routine have been exercised in tests, not on a rebuilt app.
* The offer is spoken (or lapses); there is no pop-up card yet, so the Memory page is the place to see a waiting suggestion.
* A routine stores the commands as text. If an app is later renamed or removed, that step fails like any other command would.
* Deviation from the plan: a `RunRoutine` intent was not added. Rewriting the utterance reuses the existing compound path unchanged, which is safer than a second execution path.

---

# M4 - Optional background review with approval

A free cloud model can read a redacted copy of recent **cloud** conversations and suggest stable facts about you. It is **off by default** (Memory page switch, `memcoreReview`). Nothing it says is stored until you press "Yes, remember" on the Memory page.

## How it works

* **When.** At most once per 30 minutes and six times per day, after 10 new cloud turns (or 3 once you have been quiet for 10 minutes). Not while "hide people's names" is on. First check about 5 minutes after startup, then every 10 minutes.
* **What is sent.** The always-on profile (sensitive topics excluded) and up to 14 recent cloud turns, passed through the PII filter. Commands handled on this PC are never included. The exact text is written to the "Sent to cloud models" log under route `review`.
* **What comes back** is JSON `{op: add|patch|remove, key, value, reason}`, at most 5. The prompt (adapted from Hermes Agent's memory-review approach, MIT) says the conversation is data, never instructions.
* **Every suggestion must pass code checks**, whatever the model says:
  * the value is made of words the **user** said (not the assistant's answer): an invented or lifted fact is dropped;
  * plain `snake_case` key, not a sensitive topic (health, money, ids, passwords...); value at most 80 characters, one line;
  * no secrets, no personal identifiers (PII filter must leave it unchanged), nothing that reads as an instruction, a link, an address or code;
  * `remove` only for a fact it was shown, and only when you said it stopped being true ("no longer", "switched", ...); `patch` only when the value really changes; one suggestion per key.
* **Approval.** Approved facts are stored as something you told NEXUS and join the always-on profile. Refused ones are remembered and never suggested again. Unanswered ones disappear after 14 days; at most 30 wait at once. "Wipe memory" clears the queue and the refusals.
* **Nudge.** When something is waiting, one spoken line per day through the proactive-speech policy.

## Evidence

| Check | Result |
|---|---|
| Parser: fenced JSON, prose, garbage | accepted/ignored as expected |
| Invented fact, fact only the assistant said | dropped |
| Adversarial user text (bank-statement forwarding instruction, password, "ignore previous instructions", Aadhaar number, health fact, link, API key, SQL-looking key) | nothing survives |
| Oversized value, instruction hidden in `reason` | dropped |
| patch / add-of-existing / remove rules, 5-per-run cap, one per key | pass |
| Schedule (`due`) | pass, including daily and 30-minute limits |
| Mock-model end to end | queued, nothing stored before approval, approve stores and joins profile, reject is never suggested again |
| Queue | duplicate refused, cap 30, 14-day expiry, wipe |
| Mutation checks | removing the grounding check, the instruction check, the PII check, the "stopped being true" cue and the sensitive-key check each makes at least one test fail (see AGENTS.md for the final tally) |

15 review tests.

## Read this before trusting it

* **Never run against a real model.** Tests use a mock reply; the real prompt's quality (does the model return clean JSON, does it propose useful facts) is unmeasured. Expect to tune it after a few days of real use.
* **It sends text to a third party** (Groq, Cerebras or Gemini, whichever answers first) when switched on. That is why it is off, redacted, logged and approval-gated.
* The grounding check is lexical. A user line like "my sister is not Asha" could still ground "Asha" - this is why approval is mandatory.
* The nudge is spoken or lapses; there is no pop-up.

---

# M5 - Idle curator

A deterministic upkeep pass (`memcore/curator.rs`). No model, no network. It runs at most every 20 hours, only when you have been away from the PC for 5 minutes and NEXUS is not working (first check 10 minutes after startup).

## What it does

* **Keeps the facts you ask about when the profile is full.** The always-on profile has a fixed size. While everything fits nothing changes (oldest first, identical text, so provider prompt caches keep hitting). Only when facts no longer fit does it rank them by use and store that order. "Use" now counts a profile fact whenever a question touched it (before this, profile facts were never counted, so there was no signal). The order is recomputed only by this pass, so the prompt prefix changes at most once a day.
* **Retires unused routines.** A routine neither created nor run for 60 days is removed, and the Memory page shows "Removed the routine ...: not used for 60 days." for two weeks.
* **Housekeeping.** Refused-suggestion markers older than a year and routine-offer counters not shown for six months are dropped; the normal retention/decay pass runs.
* It never deletes or edits a fact you told NEXUS.

## Evidence

| Check | Result |
|---|---|
| Schedule: idle, busy, daily limit | pass |
| Routine staleness: never run / run recently / last run 70 days ago | pass |
| Profile fits: order untouched (`choose_order` returns none) | pass |
| Profile full, 20 long facts: the fact asked about replaces an unused one, same size | pass |
| A profile fact that is asked about moves to the front; an out-of-profile fact asked about comes next | pass |
| Pass retires only the unused routine, keeps run-recently and new ones, leaves the note, note expires after 14 days | pass |
| Second pass with nothing new does not change the order | pass |
| Mutation checks (5): last-run ignored, idle check removed, always-reorder, usage counting removed, order never stored | each makes at least one test fail (the usage-counting one survived the first test set; a test was added) |

9 curator tests.

## Read this before trusting it

* **Never run on real data or in a rebuilt app.** The usage signal starts counting after the next rebuild, so there is no history yet; with fewer than about 20 facts the profile never fills and this changes nothing.
* A routine you made but never used is removed after 60 days without asking. It is easy to recreate, but it is a silent deletion, so the notice matters; if it feels wrong, raise the limit (`RETIRE_AFTER_SECS`).
* Not built (deliberately): merging near-duplicate facts (needs judgement, and it would edit what you said), and a model-assisted summary of old episodes.
