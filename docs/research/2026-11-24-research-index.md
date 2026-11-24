# Research index for 2026-11-24: questions, method, evidence, answers

This is the map of the research behind `docs/changes/207-session-2026-11-24-complete-change-record.md`. Each section says what was asked, how it was answered, the evidence with numbers, what the answer was, and how sure it is. Where a measurement exists, the raw numbers are given so they can be checked against the scripts.

## 1. How the research was done

1. **Read the real code first.** Every claim about behaviour was checked at a file and line before it was relied on. A model's statement was never taken as a fact until it was checked.
2. **Measure on the real engine, not a stand-in, wherever the machine allows.** Stand-in servers are used for HTTP shape tests, never for timing.
3. **An Opus 5.5 advisor, read-only,** audited the live-caption code and the battery guard design and challenged the plan. It could not edit or run anything. Its findings were each verified in the code; one it flagged as "inference, worth measuring" (that live captions could slow the final command on the local path) was treated as a hypothesis and the cheap part of the fix (moving the model work off the event loop) was done without claiming a measured gain.
4. **Score honestly** with one formula and one ceiling rule (section 7).
5. **A correction to an earlier note:** `docs/research/eight-weak-features/00-index-method-and-priorities.md` says no Python is installed on this machine. That is wrong: Python 3.12 is installed at `C:\Users\Chitk\AppData\Local\Programs\Python\Python312`, with `moonshine-voice`, `fastapi` and `uvicorn`, but without `websockets`, so the local server's `/stream` could not work with it. A test environment at `D:\nexus-captions-venv` has the full set.

## 2. Question: does "shut down after Claude finishes" work?

Source: `docs/research/watch-then-power-off-battery-guard-and-agent-coverage-2026-11-24.md`.

* **Answer:** it was built (`watch_power.rs` parser, `power_plan.rs`, `session_observer.rs`, `session_verdict.rs`, `watch_runner.rs`, `agent_hooks.rs`, the webhook) and unit-tested (30 engine tests, 19 mutations, real script-through-cmd-and-curl hook tests), but **never run live**, and its setting `watchPower` is off by default (off / dry / on; try "practice unattended shutdown" first and read the `[WATCH-POWER]` lines).
* **How it decides:** the agent's own Stop hook, plus the process tree quiet for 45 s, plus a verdict that stays "go" for 15 s; then, with the setting on, an announced 30 s countdown that stops on "cancel", on any key or mouse input, on a meeting, on the plan changing, or on not knowing whether the user is back. A timeout, no hook, off, dry or restarting NEXUS never powers off.
* **Confidence:** high about the design and the unit behaviour; nothing about real-world behaviour.

## 3. Question: does a battery trigger exist, and which agents are covered?

* **Battery trigger:** it did not exist. Built in commit `0794a0f` (see the change record, section 6).
* **Coverage before the work:** twelve agents had hooks (Claude Code, Codex, Antigravity, OpenCode, Gemini CLI, Aider, Kilo, Copilot, Windsurf, Cursor, Devin through Claude's settings, plus Trae observe-only). **Kiro was missing**; **Trae has no hook mechanism at all**.
* **Why a stop order only reaches Claude Code and Devin:** there is no channel into a running agent. Claude Code's `PreToolUse` hook runs before every tool, and NEXUS's webhook can answer it. The other agents either do not have a hook NEXUS answers or have none. For them the only honest action is to warn aloud.

### 3.1 Kiro facts (from Kiro's documentation, not from a running Kiro)

| Fact | Detail |
|---|---|
| Hook file | JSON, `{"version":"v1","hooks":[{name, trigger, action:{type:"command",command}, timeout, enabled}]}` |
| Triggers used | `Stop` and `UserPromptSubmit` |
| Where | The CLI (v3) reads `~/.kiro/hooks/` globally. The IDE reads only a project's `.kiro/hooks`. |
| Stop cannot block | A stop hook cannot hold the agent |
| Timeout units | seconds (documented); the shell is not documented, so the command has no quotes |
| Stop payload | `cwd`, `session_id`, `assistant_response` are not documented and are read best effort |
| Prompt hook stdout | may be added to the prompt, so the script prints nothing for Kiro |

### 3.2 Claude Code hook facts used for the stop order

* `PreToolUse` hook output with `permissionDecision: "deny"` and a reason tells the model the tool was refused and why. `continue: false` ends the agent loop and wins over the deny. Hence: deny first (the reason reaches Claude), `continue:false` only after 15 s.
* **Open:** the exact JSON accepted by the installed Claude Code version has not been confirmed on a real run. The checklist is in `docs/features/197-live-captions-and-agent-power-next-steps-plan.md`, section 4.

## 4. Question: how does the battery guard decide, and what went wrong in review?

The advisor read the design before it was finished and found the failures that matter. The table of 14 findings and their fixes is in the change record, section 6.3. The three that could have caused real harm:

1. **A shutdown that stops short** (an editor asks to save) would leave a dying laptop on while Claude's tools stayed refused. Fixed by a 45 s fallback to hibernate or a plain shutdown, and by clearing the stop order on every way out.
2. **Hibernate fails silently** when Windows hibernation is off. Fixed with a capability check; a hibernate setting then becomes a shutdown and says so.
3. **Sentences that arm it by accident** ("my battery is at 5 percent, shut down"). Fixed: a condition word is required, negations change nothing, and a sentence **never switches the guard on** (only "allow battery shutdown" does).

**Why a fake reading hook exists:** this machine has no battery, so the real loop could never be observed triggering. The hook is debug-build only, refuses to power anything off while in use, and was verified absent from a release-profile compile.

## 5. Question: what do live captions actually do, measured?

### 5.1 Method

* Speech: Windows SAPI voices (David, Zira) rendered to 16 kHz mono 16-bit WAV with the start time of each word.
* Engine: the real Moonshine streaming model (`small_streaming` in the server's settings; the cached model folder is `medium-streaming-en`, because the Python package fetches the language default) behind `server/stt_server.py`.
* Harness A, `scripts/measure-live-captions.py`: 100 ms audio chunks sent at 100 ms intervals to `/stream`, arrival time of every caption update recorded, then the same audio in one piece to `/transcribe`.
* Harness B, `scripts/test-live-captions-e2e.mjs`: the whole app (debug build, hidden desktop). Test speech is dropped in a folder (`NEXUS_FAKE_MIC_DIR`) and replaces the microphone callback's samples; a real capture starts; the stage page records every `stt:partial` event and samples the pill text every 50 ms.

### 5.2 Raw results, harness A (server alone)

| Sentence | Audio | First caption | Updates | Word lag median / worst | Final WER | Batch WER |
|---|---|---|---|---|---|---|
| "open the command hub" | 2050 ms | 485 ms | 3 | 702 / 787 ms | 0 | 0 |
| "search for the best pizza places near me and open the first result in my browser" | 5190 ms | 453 ms | 9 | 665 / 1103 ms | 0 | 0 |
| "remind me tomorrow at five thirty to send the project report ..." (28 words) | 8235 ms | 485 ms | 13 | 838 / 1926 ms | 0.071 | 0.071 |

The 0.071 on the long sentence is "5:30" written for "five thirty" (and is identical to the batch result).

### 5.3 What the early guesses look like (timeline of the normal sentence)

```
  453  (empty)
 1140  Search for the link in
 1703  Search for the best pizza pizza p
 2125  Search for the best pizza places
 2640  Search for the best pizza places near me
 3156  Search for the best pizza places near me and open
 3671  Search for the best pizza places near me and open the first page
 4718  ... open the first result in my video
 5750  ... open the first result in my browser
```

The wrong guesses ("link in", "page", "video") are later corrected. In the long sentence: "Yeah." at 485 ms, "5am" corrected to "5:30", "celebrate" to "send the", "answer" to "add", "5:30." repeated where "Friday" belongs.

### 5.4 The stability filter experiment (a rejected design)

A common cure for flicker is to show only the words that two hypotheses in a row agree on. It was simulated offline on the recorded timelines before any code was written:

| File | Mode | First words at | Words retracted | Median lag | Worst lag |
|---|---|---|---|---|---|
| short | show everything | 485 ms | 2 | 702 ms | 787 ms |
| short | agreed words only | 1641 ms | 0 | 1256 ms | 1541 ms |
| normal | show everything | 1140 ms | 6 | 665 ms | 1103 ms |
| normal | agreed words only | 1703 ms | 0 | 1425 ms | 2520 ms |
| long | show everything | 485 ms | 8 | 838 ms | 1926 ms |
| long | agreed words only | 1688 ms | 0 | 1667 ms | 3155 ms |

Agreed-words-only removes every retraction and **roughly doubles the lag**. The adopted design shows everything at once and draws the **unconfirmed tail dimmer**; a word becomes confirmed when two hypotheses in a row agree.

### 5.5 Results, harness B (whole app)

| | Short | Normal | Long |
|---|---|---|---|
| First caption after the speech file appeared | 630 to 663 ms | 650 to 668 ms | 641 to 678 ms |
| Median word lag (includes up to 200 ms of folder-watcher pick-up) | 762 to 803 ms | 808 to 824 ms | 752 to 996 ms |
| Worst word | 847 to 888 ms | 1096 to 1207 ms | 1164 to 1671 ms |
| Updates | 2 | 8 | 13 |
| Confirmed words changed later | 0 | 0 | 0 |
| Pill showed the live text while the speech played | yes | yes | yes |
| Dimmed (unconfirmed) tail seen in pill samples | 34 to 36 of 72 | 79 to 93 of 135 to 146 | 85 to 153 of 211 to 214 |
| Final transcript equal to what was spoken | yes | yes | yes (numbers as digits) |

**Reading:** captions begin about 0.65 s after speech starts and a word shows about 0.8 s after it begins (about 0.4 to 0.5 s after it ends). That is close to, but slower than, the 300 ms of commercial streaming dictation.

### 5.6 Audit findings and what was done

The Opus advisor's report on the caption chain, with each finding checked in the code. See the change record, section 8.2, for the table. Two were not accepted at face value: the claim that live captions slow the final command on the local path (an inference, not measured; only the cheap structural fix was made), and the claim that a hidden desktop might not paint WebView2 (true, and said plainly in every result: the text is in the page, not a screenshot).

### 5.7 Cold start (measured, then built)

The first use of the local engine downloads the model: decoder 140 MB, encoder 90 MB, cross 11 MB, frontend weights 11 MB, adapter 3.5 MB, tokenizer 0.2 MB, about 245 MB. On this machine it ran at 100 to 290 kB/s over two attempts: more than 10 minutes. During the first download the server **did not listen at all**, so the owner would see a silent speech engine. Round two added a status, a Hub card and a background download that starts nothing else.

## 6. Question: which source should live captions use?

| Situation | Source now | Why |
|---|---|---|
| Live captions off, meeting, Ghost hot mic, NEXUS speaking | none | by design |
| Privacy mode or "Local speech recognition only" | Local | audio must not leave the machine |
| A Deepgram key the owner added | Deepgram (cloud) | chosen on purpose; true streaming |
| The local server is already running | Local | free, private, true streaming, no quota |
| A Groq key, local server not running | Groq progressive | no extra RAM; steps 1.5 to 2.5 s apart, at most 4 requests per command, daily cap 600 |
| No key and no server | Local (will fail quietly) | the previous behaviour |

Captions never start the local server by themselves (RAM: about 300 MB for the loaded model against a 600 to 700 MB ceiling). Proven in the real app by `scripts/test-live-captions-source.mjs cold|warm`.

## 7. The scoring rule and the ladder

Formula: score = 0.20 Built + 0.20 Tested + 0.30 Live-proven + 0.15 Safety + 0.15 Industry; stars = score / 20 to the nearest half; 4.0 needs 75 %, 4.5 needs 85 %.

Live-proven rule: real engine measured on real-time audio 20; whole app chain with the pill text recorded during playback and timings measured +15; each extra source proven live with a real key +10; **ceiling without a person at a real microphone 50**; one owner session on the real microphone with 10 or more commands +20; days of real use +10 to 15. SAPI speech is clean and synthetic, and the fake microphone skips the device, driver and gain control, so more than about 50 from it is not defensible.

| Step | Built | Tested | Live | Safety | Industry | Score | Stars |
|---|---|---|---|---|---|---|---|
| Start of the day | 90 | 75 | 20 | 85 | 60 | 60.8 | 3.0 |
| After round one | 95 | 88 | 35 | 85 | 65 | 69.6 | 3.5 |
| After round two | 97 | 90 | 40 | 85 | 68 | 72.4 | 3.5 |
| With an owner session (estimate) | 97 | 90 | 55 to 65 | 85 | 68 | 77 to 79 | 4.0 |
| 4.5 needs | | | about 80 | | about 80 | 85 | 4.5 |

Industry is scored for the **default** source of the user: a person with a Groq key and a cold server gets Groq, so local numbers do not lift it fully.

## 8. Open questions

* What does Claude Code do with the deny-then-`continue:false` answer on the installed version? (plan 197, section 4)
* How do Groq's per-model free limits treat `whisper-large-v3` interim requests next to the final turbo request? Not published for free keys; needs the owner's Groq console during a real session.
* Does Kiro's stop hook carry `assistant_response`, `cwd`, `session_id`? Needs a real Kiro run.
* Does the isolated copy's silent first-start failure after a rebuild come from the WebView profile lock or the start-up lock? (plan 197, section 3.6)
