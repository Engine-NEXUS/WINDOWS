# The Mac reel, plan 201, and how NEXUS can get that fast (2026-11-26)

Analysis only. No source code was changed, nothing was built into the app, the app was not started.
One throw-away benchmark was compiled **outside the repo** (scratchpad); `git status` was clean before this file.

Labels used: **M** = measured by me today, **R** = read in the code (file:line), **I** = inferred, **U** = unknown.

---

## Verdict (read this first)

**Implement plan 201 with changes, not as written, and not in its order.**
Its safety rules are right and worth keeping word for word. But its first phase (W0: FFT in Smart Turn, the VAD
allocation) makes **no measurable difference** to the time between your last word and the app opening: with today's
settings it saves **0 ms**, and at best about 80-160 ms. Several of its facts are wrong for today's code because it was
written on an old branch (41 commits behind `main`). The one thing that actually makes the reel possible, a **source of
live words that arrives within about 0.3 s**, is assumed to exist in the plan ("partials every ~150 ms"). It does not:
today's live words come either from Groq (first one after 1.6 s of speech, then every 1.5 s or more) or from the local
Moonshine server (measured lag about 0.8 s). Speculative execution built on those would be slower than just waiting.

## One-screen summary

- **What the reel does.** One continuous listening session (no wake word per command), live words on screen about
  0.2 s behind the voice, and the Notes app starts opening **about 0.4 s after the word "app" while the man is still
  talking**. Other commands finish 0.6-1.3 s after he stops. Only safe actions are shown, in one rehearsed take
  (old test notes like "hello. This is a test." are already in the Notes list, so this was not the first try).
- **What "Jev" is.** A hosted, closed "decision" model by TypeSafe AI (released 2026-09-15): it reads text or JSON and
  returns typed answers. It is **not** a speech recogniser. The speech-to-text in the reel is a separate, unknown
  component. Jev's speed claims come from the company and early users only.
- **NEXUS today, "open notepad"** (after the wake word): about **0.9-1.7 s** from your last word to the window (estimate
  built from measured parts; the whole chain has never been timed end to end). The reel: 0.6-1.3 s after the end of
  speech, and 0.4 s *before* the end for a mid-sentence command. NEXUS is roughly 0.3-0.5 s behind on single commands
  and cannot do the mid-sentence trick at all.
- **Where the time goes (biggest first):** the cloud transcript (0.22-0.64 s), the 0.4 s silence wait, the wake check
  (about 0.47 s, paid once per wake), then opening the app itself (not measured).
- **What would really close the gap, in order:** (1) measure the real chain once; (2) cheap wins that need no new model:
  push-to-talk for commands (hold-to-talk already exists for dictation) and a "conversation session" without a wake word
  per command (Ghost mode already does this, with a 240 ms end wait); (3) pick a live-word source that is fast enough,
  by measurement on your voice; (4) only then, speculative opening of apps/tabs under strict rules, first in "shadow"
  mode (log only, do nothing).
- **Cannot be matched exactly:** a free, local, accurate speech recogniser with 0.2 s word lag on this laptop within
  ~650 MB is not proven to exist; Groq (your free key) will never give live words; mid-sentence actions will sometimes be
  wrong and need an "undo".

---

## A. What is really in the reel, and what NEXUS has

I looked at `sheet01.png` and `pill0.png` myself; the rest relies on your timings.

| Reel behaviour | How established | NEXUS has? |
|---|---|---|
| One continuous session, pill "Go ahead, I'm listening...", stop button | M (pill0) | **Partly.** Ghost mode is a continuous session with no wake word per turn and a 240 ms end wait (R: `wakeword_oww.rs:1715-1723`, `:3147`). Normal mode needs the wake word every turn. |
| Live words about 0.2 s behind the voice, revised as he goes ("notes app" -> "Notes app for me?"; you saw "search normal" -> "search Norbert") | M (pill0 shows lower-case "notes app" later capitalised) | **Partly, slower.** Live captions exist, off by default (`sttLiveCaptions`). Sources: local Moonshine stream (measured lag ~0.8 s, AGENTS.md "Live captions"), Deepgram with a key, or Groq "progressive" requests (first after 1.6 s of audio, then ≥1.5 s apart, max 4 per turn; R: `stt_stream.rs:192-202`). |
| Acts mid-sentence: Notes opens ~0.4 s after "app", **before** "for me and…" | your timing; pill0 shows the window rising while the caption still ends at "the notes app" | **No.** Captions are only shown on screen; nothing acts on them. The orchestrator gets one final transcript after the end wait (R: `wakeword_oww.rs:1756-1849`). |
| It did not wait for a boundary word ("and", "then") or a pause | M (pill0) | Plan 201 *requires* a boundary word or 200 ms pause, so it would be slower than the reel by design. |
| Chained command with "and once you're there, can you create a new note" | M (pill0) | **Mostly no.** The compound splitter only splits on "then", "and then", "after that", "afterwards", ";" (R: `command_center.rs:357-363`). "and once you're there" is not split. A verb-gated "and" split exists in `flows/` but is off (`flowsAndSplit`). |
| Polite, filler-filled speech ("Alright. Can you open up the notes app for me? And, um, …") | M | **Partly.** The parser strips "can you", "please", "for me" (R: `intent_parser.rs:1280, 1502`). Fillers are cleaned for dictation (`dictation_text.rs`), not for commands. |
| Type text into the new note ("title say hello") | your timing | **Yes, differently.** "type …" dictation exists with window refusals; there is no "make the title say X" command. |
| Google search / open x.com / open Photo Booth / take a picture | your timing | Search: yes (`browser.rs`). Open site: yes. Open app: yes (`command_executor.rs:920`). Camera snapshot: no. |
| Errors, corrections, destructive actions | not shown | Not judgeable from the reel. |

**About Jev** (web, not verified by me beyond these pages): TypeSafe AI released Jev on 2026-09-15 as a closed, hosted
model that answers typed questions about text/JSON; no open weights; available through a waitlist and listed on
Cloudflare Workers AI and Vercel AI Gateway ([InfoQ](https://infoq.com/news/2026/10/typesafe-ai-jev-released),
[YourStory](https://yourstory.com/ai-story/what-is-jev-typesafe-ai-decision-model),
[Vapi blog](https://vapi.ai/blog/early-experiment-with-jev),
[Deepgram blog](https://deepgram.com/learn/audio-decision-composition-five-open-source-jev-projects)).
So the reel's "understanding" step is likely a fast structured classifier over live text. NEXUS already has a local
equivalent for simple commands (the rule parser, single-digit to tens of ms) and does not need Jev for "open notes".
Jev could matter for messy sentences; that is a later experiment, not a prerequisite.

---

## B. Plan 201's factual claims, checked against today's code

**Branch problem first (M, git).** Plan 201 lives on `origin/feat/sidebar-system`, whose fork point with `main` is
`97c5695` (2026-11-25); the branch is 41 commits behind and 3 ahead (only the plan). On that branch `stt_stream.rs` is
the **old 130-line version** that AGENTS.md says "was never called"; `main` has a 1,408-line rewrite (+1,484/−138 in the
diff), and `orchestrator.rs` differs by +289 lines. The branch also lacks hold-to-talk and the earlier Opus plan
(`01-instant-voice-plan`). Implementing on that branch would conflict and would build on code that no longer exists.
Any work must start from `main`.

| Claim in plan 201 | Verdict | Evidence |
|---|---|---|
| Smart Turn uses a naive DFT, "~129 ms vs ~12 ms with FFT" | **Half right.** The DFT is naive (R: `turn_detect.rs:84-92, 130-143`). But 129 ms was a *first call including ONNX warm-up* (doc `jarvis-landscape/03…:39-40`). I measured the feature step alone, release build, same sizes (400-point frames, 800 frames): **DFT 29.2 ms, rustfft 5.7 ms**, identical output (max diff 0), best of 7 (M, scratchpad bench; rustfft 6.4.1 is already in `Cargo.lock` via tract). Saves ~23 ms on a worker thread. |
| Smart Turn and Silero VAD are off by default | **True.** R: `commands.rs:3134-3154` (`unwrap_or(false)`). |
| The FFT makes turns end sooner | **Barely, and not by default.** The model runs on a worker thread and is polled once per 80 ms chunk (R: `wakeword_oww.rs:2994-3031`). With the default "medium" eagerness the earliest Smart Turn stop is 5 chunks = 400 ms (R: `turn_detect.rs:250`), **exactly the same as the normal 400 ms rule** (R: `wakeword_oww.rs:3138`). So "medium" can only *lengthen* turns, never shorten them. Only "high" (earliest 240 ms) can save up to 160 ms; the FFT can at most let a verdict land one 80 ms chunk earlier. |
| "fewer cut-offs than 800 ms baseline" | **Wrong baseline.** The normal end wait is 400 ms; 800 ms applies only after 2 mid-sentence pauses (R: `wakeword_oww.rs:3138-3143, 3668`). |
| `vad.rs` allocates per frame | **True but irrelevant.** About 4 small buffers per 32 ms frame (R: `vad.rs:52-58, 84`). A few KB per frame costs microseconds next to the ONNX call (I), and Silero is off by default. Fine as tidy-up, zero user-visible gain. |
| `stt_stream.rs` emits partials every ~150 ms | **Wrong.** 150 ms is only how often the Groq loop *wakes up to check* (R: `stt_stream.rs:198`). Real Groq partials: first after 1.6 s of audio, then ≥0.8 s of new audio **and** ≥1.5 s gap, max 4 per turn, 8 per minute, 600 per day, none in the last 200 ms of speech (R: `stt_stream.rs:192-202, 228-232, 484-489`). A 1-second command gets **zero** partials. Local Moonshine stream: measured first caption ~0.65 s, median word lag ~0.8 s (AGENTS.md, live captions entry). Deepgram: real interim results, needs a key (R: `stt_stream.rs:32`). |
| Groq has token streaming | Not claimed by 201, but implied by W2. **Groq STT is batch upload only** (R: `stt_groq.rs`, `stt_stream.rs:50-53` comment). |
| Orchestrator waits for the end of turn before parsing | **True.** One final transcript per capture (R: `wakeword_oww.rs:1756-1849`, then `transcribe_and_emit`). |
| Parser < 5 ms | **Unproven.** The measured `parse` stage is 14-49 ms, once 440 ms (`group-1-live-run` §5.4). That stage includes more than the regex; the regex alone was never timed. Either way it is not the bottleneck. |
| `ActionSpec` + risk tiers exist | **True.** R: `center.rs:40-73`; `action_kernel/mod.rs:23-34, 148-167`. **But** the default for any intent not listed is `Risk::Read` (`mod.rs:165`), i.e. "safe". A speculative runner must use an **explicit allow-list**, never "risk ≤ Reversible". `OpenApp` is also the parser's permissive fallback (R: `intent_parser.rs:702` comment), the place known collisions land ("show workflow runs" → open app). |
| `action_kernel` quarantine + undo | **True.** The only permanent deletes are the age purge and its own tests (R: `action_kernel/files.rs:489, 507, 706`). |
| `desktop_control.rs` effect checks | **True** (Unconfirmed outcomes, R: `desktop_control.rs`, 7 uses). Open-app also checks the window is in front before saying "Opened" (R: `orchestrator.rs:2468-2475`). |
| `sonora` pure-Rust AEC3 | **Unverified.** Not in `Cargo.toml`, not mentioned anywhere in the repo. I did not confirm the crate exists or its licence. Treat as an idea, not a plan item. |
| Named files exist | `turn_detect.rs`, `vad.rs`, `stt_stream.rs`, `stt_groq.rs`, `stt_learning.rs`, `dictation.rs`, `desktop_native.rs`, `desktop_control.rs`, `browser_url.rs`, `barge_gate.rs`, `stage.rs`, `flows/`, `action_kernel/` exist (M). `center/` is a single file `center.rs`, not a folder. There is no `committed_cursor` anywhere (it is new). |
| "Shell" intent must not run speculatively | There is no general shell intent; harmless as a rule. |

**Success gates that are unrealistic or cannot be measured today**

- W0 "Smart Turn features < 15 ms": easy (5.7 ms) but says nothing about the user's wait.
- W1 "measurable drop in proper-noun WER": there is no baseline and no owner recording set yet.
- W1/W4 "zero wrong-window pastes/edits": no counter exists; can only be "0 in N scripted trials".
- W2 "safe opens start < 350 ms after clause end": impossible with today's live-word sources (0.8 s local lag,
  ≥1.6 s Groq). And "clause end" has no timestamp in the logs.
- W2 "0 % destructive misfires": good as a **code rule** (unit test: the speculative runner rejects every intent not on
  the allow-list), meaningless as a live percentage.
- W3 "< 300 ms stop with speakers on": depends on an echo canceller nobody has verified exists.

---

## C. Time budget: last word → action, "open notepad"

| Stage | NEXUS today | Source |
|---|---|---|
| Wake word + cloud speech check | ~0.47 s median, paid per wake, partly while you speak (not after the last word if you say it in one breath) | M one log, `01-instant-voice-plan` table |
| End-of-speech wait | 400 ms + up to 80 ms chunk rounding (240 ms in Ghost mode) | R `wakeword_oww.rs:3138, 3147, 1715-1723` |
| Owner check / filters | small, not timed separately (I) | — |
| Cloud transcript (Groq, batch) | 217-638 ms | M `group-1` §5.4, change 132 |
| Parse + gates | 14-49 ms | M same |
| Launch (registry lookup ~0.1 ms, then Windows starts Notepad) | launch + window drawing **not measured** (I: 0.2-0.6 s) | R `command_executor.rs:920-1010` |
| "Ok sir" | after the window is checked in front, does not delay the action | R `orchestrator.rs:2458-2500` |
| **Total to window** | **≈ 0.9-1.7 s** (I, sum of the above, never timed end to end) | |
| **Reel** | 0.6-1.3 s after the end (Arc, search, x.com, Photo Booth); −? s mid-sentence (Notes 0.4 s after "app", before the sentence ended) | your timings |

**What each proposed change saves (from the end of speech):**

| Change | Saving | Why |
|---|---|---|
| W0 FFT | **0 ms** today (Smart Turn off); ≤ 80 ms with Smart Turn "high" | see section B |
| W0 VAD allocation | 0 ms | microseconds |
| W0 turning Smart Turn on ("medium") | 0 ms or **slower** | earliest stop = today's rule |
| Smart Turn "high" | up to 160 ms, risk of cutting you off | 240 vs 400 ms |
| Push-to-talk for commands (reuse hold-to-talk) | ~300-400 ms | key release ends the turn: ≤ 50 ms key polling (R: `global-hotkey-0.8.0/src/platform_impl/windows/mod.rs:157-168`) + ≤ 80 ms chunk, instead of 400-480 ms |
| Session mode without wake per command (Ghost-like) | ~0.47 s per command (wake check) + 160 ms (240 ms end wait) | already exists in Ghost mode |
| A live-word source with ~0.3 s lag + speculative safe opens | 0.4 s to over 1 s; can start *before* you finish | only way to do the reel's Notes moment |
| Local final transcript instead of Groq for short commands | 0.2-0.6 s, **if** the local model is accurate enough on your voice | unmeasured |

**So W0 first is the wrong order for your goal.** It is a five-minute tidy-up, not a speed phase.

---

## D. My recommendation

### Architecture

1. **Two listeners, one decider.**
   - A *fast* listener produces live words for a **small command grammar** only (open/focus app, new tab, open site,
     search, media). It decides *whether to start early*.
   - The *accurate* listener (Groq Whisper on your key; Moonshine in privacy mode) still produces the final transcript.
     It *checks* every early action and is the only source for anything else.
2. **Fast listener choice (decide by measuring, not by belief).** Candidates, all free or bring-your-own-key, no GPL:
   - Local Moonshine stream (already built). Measured lag ~0.8 s: probably too slow for mid-sentence, fine for captions.
   - sherpa-onnx streaming Zipformer (Apache-2.0, small models, designed for low lag). **Unmeasured on this laptop**;
     needs a RAM check against the ~650 MB ceiling.
   - Windows' own speech recogniser (continuous "hypothesis" events, zero download, no extra licence). Accuracy on names
     **unknown**; worth one afternoon because it costs almost no RAM.
   - Deepgram streaming with your own key (real interim words; free credit, not free forever; audio leaves the PC; never
     in privacy mode, already enforced at `stt_stream.rs:83-88`).
   - Groq: **cannot** be the fast listener.
   Pick the one with the best "command word right, and seen within X ms" on your own recordings.
3. **Commit rules for an early action** (all must hold):
   - Intent is on an **explicit allow-list**: focus an already-open window, open an allow-listed app, open a new tab /
     known site, web search, media play/pause. Nothing else, ever (no send, delete, close, power, GitHub write, typing,
     file moves, agent tasks, orders).
   - The app/site resolves to a **confident** registry match (`assess_open_target` returns `Proceed`, R:
     `command_executor.rs:942-972`); a guess, a pick-list or an installer never starts early.
   - The words for the clause are **unchanged across two live updates at least ~250 ms apart** (time, not tick count).
   - **No longer name could still be coming:** if another registry entry starts with the heard name ("notepad" vs
     "notepad++", "photo" vs "photo booth"), wait for the next word or the end.
   - **No negation or correction anywhere in the sentence so far, or in the next word:** "don't", "not", "no", "wait",
     "actually", "instead", "cancel", "never mind", "stop", "rather". A correction *after* an early open cancels all
     later steps; NEXUS then says what it already did ("I'd already opened Notes; say 'close it' if you want it closed")
     and **never closes anything by itself** (closing can lose work).
   - Owner check (Voice Lock) already passed for this capture; never in a meeting; never from an ambient/hot-mic turn the
     directed gate has not accepted.
   - The final transcript must agree. If it disagrees, log it as a speculative miss (the key number to watch) and do not
     run the rest of the chain.
   - Steps after the first in a chain ("create a new note", "type hello") run only after the end of speech and only if
     the previous step was **confirmed** (window really in front). This is plan 201's halt rule, which I agree with.
4. **Safety gate stays where it is:** Rust, `center::prepare_action`, `action_kernel` quarantine, confirmations held in
   Rust. The early path is just an earlier *entry* into the same gates with a stricter allow-list.

### Phases, in my order

| # | What | Gate (measurable) | Effort | Risk | Only you can test |
|---|---|---|---|---|---|
| 0 | **Measure the real chain** (plan 01 phase 0): one `[LAT]` line from the last voiced chunk to "window in front", per path. | 50 of your turns logged; baseline table written. | 2-3 days | none | speaking 50 commands |
| 1 | **Cheap wins, no new model:** push-to-talk for *commands* (reuse `hold_to_talk.rs`, route the text as a command instead of typing); a "listening session" toggle that behaves like Ghost's hot mic (no wake per command, 240 ms end wait, directed gate on). Optional: FFT + Smart Turn "high" A/B, as a side task. | p50 last word → window ≤ 0.9 s on push-to-talk; no rise in cut-offs on 30 clips. | 3-5 days | hot-mic hears the TV (directed gate exists) | the key feel, the session on/off, a TV test |
| 2 | **Choose the fast listener:** record ~100 short commands of yours; run each candidate offline; report word lag and "command word right". | a candidate with command words right ≥ 95 % and lag p50 ≤ 350 ms, within RAM budget; or a written "none qualifies". | 1 week | none may qualify | recordings, keys (Deepgram if tried) |
| 3 | **Shadow mode:** run the commit rules on live words, log "would have opened X at t", do nothing. | 300 of your turns: 0 early actions that the final transcript contradicts; median would-be start ≥ 0.3 s earlier than phase 1. | 1 week | rules too strict = no gain | normal use for a few days |
| 4 | **Live early opens** behind a switch (off by default), allow-list only. | same numbers live; every miss logged and spoken. | 3-4 days | an unwanted window now and then | live use |
| 5 | **Understanding:** split chained sentences on "and once you're there", "and then can you"; command-side filler and "no wait" cleaning; app-aware word hints (plan 201's W1, which is good). | held-out set right ≥ 90 %, wrong ≤ 1 % (plan 01 targets). | 1-1.5 weeks | collisions | recording the held-out set |
| 6 | Instant local "stop" with echo cancelling (plan 201 W3). | stop heard within 300 ms with speakers on, 20 tries. | 1-2 weeks, research first | the AEC crate may not exist / fit | speakers on, real room |

### What cannot be matched (plainly)

- A guaranteed 0.4 s mid-sentence open **for free, locally, on this laptop**: not shown possible until phase 2 measures it.
- Live words from Groq, your current free key: Groq has no streaming. Deepgram gives them but costs money after the
  free credit and sends audio out.
- Zero wrong early actions: the rules make them rare and harmless (only reversible opens), not impossible.
- Judging the reel's accuracy: it is one rehearsed take of eight safe actions.
- Mac-specific bits (Notes "new note", Photo Booth camera) have no direct Windows equivalent today.

### Corrections I would make to plan 201

1. Rebase the plan on `main`; drop references to the old `stt_stream.rs`. `center/` → `center.rs`.
2. Remove "~150 ms partials". State the real sources and their lag; make "choose a fast listener" its own phase.
3. Replace "129 ms → 12 ms" with "29 ms → 6 ms feature step (measured), ≤ 80 ms effect, 0 ms by default", and move it out
   of the first phase.
4. Fix the baseline: 400 ms, not 800 ms. Note that Smart Turn "medium" cannot shorten turns.
5. Speculative gate = explicit allow-list + confident registry match + prefix check + negation check + time-based
   stability, not "risk tier" (the risk default is `Read`).
6. Drop the requirement of a boundary word/200 ms pause as the *only* trigger (the reel didn't need one), but keep it as
   one of the stability signals.
7. Add phase 0 measurement and a shadow mode before anything acts early.
8. Make W1's gates concrete (owner recording set first). Mark `sonora` as "to be verified".
9. Add push-to-talk for commands and a wake-less session: the cheapest real speed wins.
10. Keep section 4 (safety invariants) unchanged.

### Verdict paragraph

Plan 201 points in the right direction (start early on safe things, never on dangerous ones, verify the target, stop a
chain on any doubt) and its safety section should be kept as is. But it starts with work that does not change what you
feel, it assumes live words that NEXUS does not have fast enough, and it was written against code that `main` has since
replaced. Implement it **with changes**: rebase on `main`; measure first; take the cheap wins (push-to-talk for
commands, a wake-less session); then measure which live-word source is fast enough on your voice; then run the early
opens in shadow mode before they ever act. Do the FFT change whenever convenient, as a five-minute tidy-up.

---

## What I could not check

- End-to-end time of any NEXUS command (never timed; phase 0 exists for this).
- Smart Turn ONNX inference time steady-state (only the feature step was measured; old note says 129 ms first call).
- Whether `sonora` exists and its licence.
- sherpa-onnx / Windows speech / Deepgram lag on this laptop.
- Hold-to-talk live: not run. Read in code: release is detected by polling the Space key every 50 ms in the
  `global-hotkey` crate, so key-up to end of capture is ≤ ~130 ms (I); text is typed only into the window that was in
  front at key-down and only after Ctrl/Alt/Shift/Win are up (R: `hold_to_talk.rs:73-82, 145-152`). It is dictation only:
  nothing said this way becomes a command.
- The reel's speech recogniser and whether the video was edited between commands.

## Sources

- [InfoQ: TypeSafe AI Jev released](https://infoq.com/news/2026/10/typesafe-ai-jev-released)
- [YourStory: What is Jev](https://yourstory.com/ai-story/what-is-jev-typesafe-ai-decision-model)
- [Vapi: early experiment with Jev](https://vapi.ai/blog/early-experiment-with-jev)
- [Deepgram: five open-source Jev projects](https://deepgram.com/learn/audio-decision-composition-five-open-source-jev-projects)
- Repo: `docs/research/jarvis-speed/01-instant-voice-plan-2026-11-26.md`, `docs/research/jarvis-landscape/03-smart-turn-prototype-2026-10-04.md`, `docs/testing/group-1-live-run-2026-11-11.md`, `docs/changes/132-…md`, AGENTS.md (live captions entry).
