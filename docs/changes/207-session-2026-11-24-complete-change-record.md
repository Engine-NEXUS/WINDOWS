# Change record 207: everything done on 2026-11-24 (branch `claude/screen-tour-4star-021137`)

Written for the owner. It covers ten commits made on 2026-11-24, in the order they were made, with what was asked, what was found, what changed, how it was tested, and, as important, what was **not** verified. Nothing here is on GitHub: the branch is local, 58 commits ahead of `origin/main` (`f5cfb09`, 2026-11-22), and no commit of this branch exists on any remote branch.

Companion documents:

* Plan for what comes next: `docs/features/197-live-captions-and-agent-power-next-steps-plan.md`
* Research index with the measurements and audits: `docs/research/2026-11-24-research-index.md`
* Per-feature records already in the repo: `docs/features/196-kiro-and-battery-guard.md`, `docs/research/live-captions-4-star-2026-11-24.md`, `docs/research/watch-then-power-off-battery-guard-and-agent-coverage-2026-11-24.md`, `docs/testing/isolated-build-test-2026-11-24.md`, `docs/research/eight-weak-features/`

## 0. The rules the work followed

* The owner's own NEXUS (a release build, process 3340) was never stopped, started, rebuilt or changed. Every live test ran on a **separate copy**: a debug build with its own identity (`com.nexus.assistant.contributor`), its own data folder, its own keychain, its own start-up lock, on its own hidden Windows desktop, in quiet mode (no sound, no volume or screen changes). `scripts/run-hidden-test.ps1` force-stops every nexus process, so it was **never** used. The new `scripts/run-isolated-test.ps1 -Stop` stops only its own executable.
* Verification before plans. For every feature the code was read and measured first; plans came after. An Opus 5.5 model was used as a **read-only advisor** (it could not edit anything); its findings were each checked in the code before being acted on.
* Honest scoring. The scorecard formula is fixed (Built 20 %, Tested 20 %, Live-proven 30 %, Safety 15 %, Industry 15 %; stars are the score divided by 20, rounded to the nearest half). Evidence without a person at a real microphone is capped.
* Build output went to `D:\nexus-screen-target`. Nothing was pushed, no PR was made, and `docs/testing/four-star-programme.md` was not edited.

## 1. The ten commits at a glance

| # | Commit | Size | What it is |
|---|---|---|---|
| 1 | `8b81dd9` | 55 files, +3127 / -242 | Eight weak features: verified first, defects fixed, documented |
| 2 | `69ef4c5` | 10 files, +614 / -8 | Live captions: Command Hub switch and a Groq progressive source |
| 3 | `fa1d890` | 13 files, +332 / -46 | Live captions made cheaper, counted separately, whole-chain test |
| 4 | `d76e240` | 7 files, +332 / -4 | Isolated build test of the captions switch (21/21); start-up lock made identity-aware |
| 5 | `3460358` | 1 file, +108 | Research: shutdown-after-the-agent, low-battery guard, agent coverage, captions star check |
| 6 | `0794a0f` | 28 files, +2085 / -20 | Kiro as a watched agent and the battery guard |
| 7 | `c71f867` | 3 files, +143 / -2 | Battery guard: debug-only fake battery and a live test that cannot power anything off |
| 8 | `595abce` | 6 files, +398 / -1 | Live captions groundwork: Groq request tests, fake microphone, measuring scripts |
| 9 | `830c8fb` | 85 files, +1569 / -108 | Live captions round one: measured on the real engine, defects fixed (85 files because the work cards were regenerated) |
| 10 | `1aa7432` | 29 files, +727 / -42 | Live captions round two: warm local server preferred, speech model status and download |

## 2. Commit 1, eight weak features (`8b81dd9`)

**Asked:** lift the eight features rated about 2.5 stars (reverse engineer, Focus Mode, GitHub by voice without the owner, the notification brain, WhatsApp priority people, vision-grounded click, the admin brain, streaming captions), with verification before any plan and everything written in `docs/`.

**Method:** read each feature's real code path, reproduce, rank findings, then fix. Details per feature are in `docs/research/eight-weak-features/00-...md` and one file per feature.

**Real defects found and fixed** (these were bugs, not polish):

| Defect | Where | Effect before the fix |
|---|---|---|
| Vision click uploaded the whole screen with no check of which window was in front, and clicked after a multi-second cloud call without checking again | `orchestrator.rs::run_click_element` | Could send another window's content to the cloud and click in the wrong place if the window changed during the call |
| The brain's label ran unchecked inside "X then Y", and `min_confidence` was never read | `orchestrator::accept_brain_result`, `brain_floor` | A low-confidence guess was executed as a command |
| The GitHub watcher took the newest run of any workflow or branch | `github_watch::facts_for_repo_on` | A failure on an unrelated branch was reported as the repo's state |
| **Live captions never ran**: the streaming module's only caller had no callers | `stt_stream.rs` (rewritten) | The feature was dead code |
| The scanner ignored a "no proxy" guarantee and did not know VitePress | scanner | Wrong or missing technology results |

**Outcome:** Rust 3516 tests and frontend 1282 tests pass, every repository gate passes, and **27 of 27** deliberately introduced breakages were caught (`docs/testing/runs/eight-weak-features-mutations*`). A real mock-server test showed that a source-only "late message" test never fed a late message, so the guard logic was extracted into a pure `deliver()` that a test can call.

**Not run live and not scored:** every "Live-proven" column (Deepgram, local streaming, a real WhatsApp bridge, real GitHub accounts, the vision harness, a live scan, Task Manager proof). The checklists are at the end of each document.

## 3. Commits 2 and 3, the live captions switch and its cost (`69ef4c5`, `fa1d890`)

**Asked:** put the switch for live captions in the Command Hub, then reduce the cost, with the Opus advisor consulted read-only.

* **Hub switch** `sttLiveCaptions` (default off) in the Features page, Voice group, risk label "cloud".
* **Groq progressive source:** with no Deepgram key and not in privacy mode, the app asks Groq's large Whisper model for the speech so far, repeatedly while the person talks, so words appear while speaking.
* **Cost reduction** (from the advisor's review): first request only after **1.6 s** of speech, **at most 4 requests per command**, a **daily cap of 600**, the audio window is limited, silence and room noise are never sent, a request is skipped when the speaker has just stopped (the final request is coming and an interim would duplicate it), requests are **counted separately** (`stt_interim`) so the Hub's "requests today" figure is not inflated, and a failed or rate-limited request stops requests for the turn and backs the budget off for two minutes.
* A whole-chain test and a cost/lag measurement were added.

## 4. Commit 4, isolated build test (`d76e240`)

**Asked:** build the captions switch and test it thoroughly on a real build **without** stopping the owner's NEXUS.

* New: `scripts/build-isolated-test.mjs` (contributor-profile debug build), `scripts/run-isolated-test.ps1` (hidden desktop, own data and keychain, never stops another process), `scripts/test-isolated-app.mjs` (checks through the debug port).
* Result at that commit: **21 of 21** live checks, including the switch on both the Features and Audio pages and the real native capture reaching the stream dispatcher.
* **Real finding:** the start-up lock was a single fixed name (`Local\com.nexus.assistant-boot`), so a contributor build was refused beside a personal NEXUS. `boot::primary_mutex_name(APP_ID)` now includes the identity; the personal name is unchanged (tested). Record: `docs/testing/isolated-build-test-2026-11-24.md`.
* Later the script grew checks for the battery guard, Kiro, the speech model card, and (found while testing) the script now also **rebuilds `frontend/dist`**, because a debug build with the custom protocol embeds the pages at compile time and the first run had shown an old Features page. It now totals **33 checks, all passing**.
* Another finding: the isolated copy shares the owner's home folder, so its "connect coding agents automatically" step looked at the owner's real agent settings (the log showed `[AUTOHOOK] Codex not hooked up`). Nothing was changed, because every hook was already installed, but the launcher now switches that step off in the test profile before it starts.

## 5. Commit 5, research (`3460358`)

`docs/research/watch-then-power-off-battery-guard-and-agent-coverage-2026-11-24.md` answers four questions before any building:

1. Does "shut down after Claude finishes" work? It was **built and unit-tested but never run live**, and its setting `watchPower` is off by default.
2. Does a "battery below N percent, stop the agent, shut down" trigger exist? **No.**
3. Which coding agents are covered? Twelve had hooks; **Kiro was missing**; Trae has no hook at all.
4. Are live captions at 4 to 4.5 stars? **No: 3.0 stars (60.8 %).** The score table and what would move it are in the document.

## 6. Commit 6, Kiro and the battery guard (`0794a0f`)

### 6.1 Kiro (the 13th watched agent)

* Hook file `~/.kiro/hooks/nexus-agent-event.json` in Kiro's documented schema (`{"version":"v1","hooks":[{name, trigger, action:{type:"command",command}, timeout, enabled}]}`), with a `Stop` hook and a `UserPromptSubmit` hook, timeout 10 seconds. The file is owned outright by NEXUS: a file of that name that is not NEXUS's is never overwritten or removed.
* The command has **no quotes**, because the shell Kiro uses is not documented and it must work in both `cmd` and PowerShell; the script path is the short (8.3) form when it contains a space.
* **Kiro's CLI (v3) reads `~/.kiro/hooks/` in every workspace. The Kiro IDE reads hooks only from a project's `.kiro/hooks` folder.** NEXUS does not touch project folders, so for the IDE the same file must be copied into a project.
* The stop payload's fields are undocumented, so `cwd`, `session_id` and `assistant_response` are read on a best-effort basis. The script prints **nothing** for Kiro because its prompt hook may add stdout to the prompt.
* Found by the `~/.kiro` folder or by `kiro-cli` / `kiro` on the PATH; spoken as `kiro`, `kyro`, `kiro cli`, `kiro-cli`, `kiro ide`; a sound-alike entry for speech matching (`K IY R OW`).
* Touches: `agent_hooks.rs`, `watch_power.rs`, `power_plan.rs`, `session_observer.rs`, `watch_runner.rs`, `agent_watching.rs`, `directed.rs`, `sound_match.rs`.

### 6.2 The battery guard

**Asked:** when the battery is below N percent, tell the coding agent to stop where it is and shut down the laptop, for any coding agent.

New module `src-tauri/src/battery_guard.rs` (about 1000 lines including tests).

**Settings:** `batteryGuard` off / dry (practice) / on, default **off**; `batteryGuardPercent` 3 to 20, default 5; `batteryGuardAction` shutdown or hibernate; `batteryGuardStopAgents` default on. All appear in Command Hub, Features, Agents.

**What you say:** "when the battery is below 8 percent stop Claude and hibernate the laptop" sets the level, the action and the stop option and **never switches the guard on by itself** (off becomes practice); "allow battery shutdown" switches it on; "practice battery shutdown"; "turn off the battery guard"; "battery guard status". Negations ("don't allow battery shutdown") change nothing; dictation ("type when the battery ...") is not claimed. These sentences reach the guard before any other parser.

**What it does when on:**

1. Reads the battery every 20 s (5 s when low, 2 s while counting). A desktop, an unreadable level and an **unknown charger state never act** (unknown counts as plugged in, because a wrong "on battery" would power off a laptop that is not dying).
2. While an agent is working: a spoken warning at 15 % and again at the trigger plus 3.
3. At the trigger **on battery**: says what is about to happen, starts the stop order (Claude Code only), starts a **30 second countdown**.
4. **Stop order.** There is no channel into a running agent. Claude Code's tool-call hook (`PreToolUse`) already runs before every tool, so for the next 15 s the webhook answers it with `permissionDecision: deny` and the reason (Claude is told to stop and say where it stopped); after 15 s the answer also carries `continue:false` (Claude Code's documentation says that ends the loop and wins over the deny). Other agents are only warned aloud, because there is no verified way to stop them. **Nothing is killed.** The same hook is shared by Devin.
5. Countdown ends: the graceful shutdown sequence (goodbye, apps closed gracefully, a last 3 s to cancel), or hibernate. If the graceful shutdown stops short (an app asked to save), **45 s later** it hibernates instead, or does a plain shutdown if Windows hibernation is off. A spoken "cancel" is never overridden. If nothing worked it says so and lifts the stop order.
6. Stands down on a charger (even one reading), a recovered level, a spoken cancel, or the guard being switched off or to practice. It re-arms only after 60 s of steady charger or a recovered level.

New in this commit: `PowerKind::Hibernate` (`SetSuspendState`), a capability check (`GetPwrCapabilities`: sleep state 4 supported and a hibernation file present) so a hibernate setting without hibernation becomes a shutdown **and says so**, and a cancel counter in `power_cmd` so only a real "cancel" cancels (another power command starting does not).

### 6.3 The Opus review of the guard (all fixed)

| Finding | Fix |
|---|---|
| A shutdown that stops short leaves the laptop to die and keeps refusing Claude's tools | Fallback to hibernate or plain shutdown after 45 s; the stop order is cleared on every exit |
| Hibernate can fail silently | Capability check; falls back to shutdown and says so; a failed call is spoken |
| Any other power command cancelled the guard | A spoken-cancel counter replaces the generation check |
| Practice mode blocked the real thing | The loop restarts the engine when the mode changes |
| "On" to "practice" mid-countdown kept counting | Treated like "off" |
| A charger blip undid a cancel | Needs 60 s of steady charger |
| A loop panic is silent | The loop is supervised and restarts |
| The end action was read at fire time | The announced end is the end that happens |
| `continue:false` hides the reason from Claude | Deny first, `continue:false` only after 15 s |
| A bad token printed an error body into Claude's hook | Always a plain `{}` with status 401 |
| "don't allow battery shutdown" switched it on; "my battery is at 5 percent, shut down" armed it; polite phrasings and "below 5" were missed; dictation was claimed | Negations, a required condition word, "can you" allowed, a level without "percent", "type ..." skipped |
| The Kiro prompt hook may take stdout into the prompt | Nothing printed for Kiro |
| "hibernate my laptop" still means sleep | Left as it was (documented) |

### 6.4 Tests

24 of 24 deliberately introduced breakages were caught (`docs/testing/runs/battery-guard-and-kiro-mutations-20261124.json` and `-b-`): a connected charger standing the guard down, cancel, stop-order expiry, the last look at the setting before the power call, the hook script's answer for a tool call, the webhook's answer, and others. Isolated live checks: the defaults, the owner's sentence stored (8 percent, hibernate), a sentence never switching the guard on, a negation changing nothing, "allow battery shutdown", "turn off the battery guard", the Kiro sentence, and the Features page showing the row.

## 7. Commit 7, the guard triggering for real, safely (`c71f867`)

**Asked (after the owner said they could not shut down because other processes were running):** see whether it triggers, without shutting down.

This machine has no battery, so the guard could never trigger by itself. A **debug-build-only** hook was added: when the environment variable `NEXUS_FAKE_BATTERY_FILE` names a file, its text is the battery reading ("4" means 4 percent on battery, "4,ac" means on the charger). **While it is in use the guard refuses to power anything off in any mode** (`fake_battery_in_use`). The hook is not compiled into a release build (checked with a release-profile compile).

`scripts/test-battery-guard-live.mjs` drove the **real loop** in the isolated copy: **10 of 10 checks passed** (practice speaks and starts nothing; on a charger nothing happens; on battery at 4 percent the warning and the 30 s countdown; a spoken "cancel" cancels; after a recovery it can start again; plugging in the charger stands it down; a countdown left to run out only logs "nothing is powered off").

Not covered: stopping a real Claude Code (no coding agent was running in the test copy, and the webhook port 39220 is held by the owner's NEXUS), the real shutdown or hibernate call, and a real battery.

## 8. Commits 8 and 9, live captions round one (`595abce`, `830c8fb`)

**Asked:** make live captions more than 4 stars today. Result: **3.0 to 3.5 stars (60.8 % to 69.6 %)**, not 4.

### 8.1 What was measured

Windows' own voices made three test sentences (`scripts/make-test-speech.ps1`, 16 kHz mono WAV plus the start time of each word, collected inside a small compiled helper because PowerShell's event queue dropped half of them). The **real local Moonshine streaming server** was measured at real speaking pace (`scripts/measure-live-captions.py`), then the **whole app** was run: a debug-only fake microphone (`src-tauri/src/debug_mic.rs`, enabled only by `NEXUS_FAKE_MIC_DIR`, compiled only into debug builds, no new IPC command) plays the speech through the real microphone callback, a real capture starts, and the stage page records every `stt:partial` event and samples the caption pill's text every 50 ms (`scripts/test-live-captions-e2e.mjs`).

| | Short (2.1 s) | Normal (5.2 s) | Long (8.2 s) |
|---|---|---|---|
| Server alone, first caption | 485 ms | 453 ms | 485 ms |
| Whole app, first caption | 630 to 663 ms | 650 to 668 ms | 641 to 678 ms |
| Whole app, median word lag after the word began | 762 to 803 ms | 808 to 824 ms | 752 to 996 ms |
| Whole app, worst word | 888 ms | 1207 ms | 1671 ms |
| Final caption word error rate (server alone) | 0 | 0 | 0.071, the same as the normal transcript |
| Confirmed words changed later | 0 | 0 | 0 |

### 8.2 Defects found by the Opus audit and verified in the code, and the fixes

| Finding | Fix |
|---|---|
| The pill kept the last five words and typed on from the old character count, so the line jumped sideways | `typingStart` keeps what the old and new text share (a common start, or a word-aligned overlap); the typing cap went from 1.2 s to 0.5 s |
| The local server sent only the current line, so a pause made the caption drop everything before it | All lines are kept by id, in order, joined |
| The first words never reached the caption: the seed audio (spoken after the wake detection) was not fed, and chunks before the plan was decided were dropped | A bounded 4 s pre-roll in `stt_stream.rs` sent first and in order; the seed is fed |
| The model ran on the server's event loop and could hold up the final transcript | `add_audio` and the end flush run in a thread |
| Early guesses are revised ("link in" became "best pizza", "video" became "browser", "5am" became "5:30"), which reads as flicker | Words are shown at once; the not-yet-confirmed tail is drawn dimmer (a word is confirmed when two hypotheses in a row agree: `Stability`, `partialStableChars`). **Showing only confirmed words was simulated first and rejected: it removed all retractions but doubled the lag (median 665 ms to 1425 ms)** |
| The Groq interim request had no test because its address was a constant | The address can be pointed at a stand-in server in tests; 3 new tests (the large model and a bearer key, 429 and 401 in plain words, no key sends nothing, a dead server is an error) |

### 8.3 Scoring (the rule adopted)

Live-proven: real engine measured 20, plus the whole app chain with timings 15, plus each extra source proven with a real key 10, with a **ceiling of 50 without a person at a real microphone**. Result: Built 95, Tested 88, Live 35, Safety 85, Industry 65 gives **69.6 %, 3.5 stars**. Industry stayed low because the default source for anyone with a Groq key is Groq (1.5 to 2.5 s behind); local numbers do not count for it.

### 8.4 Known and not fixed

The last caption of a short command can still be a wrong guess ("open the connection" for "open the command hub") because the engine's end-of-speech flush is dropped on purpose, so a late caption can never overwrite the final text. The final transcript replaces it about a second later and the unconfirmed tail is dimmed.

### 8.5 Test and mutation record

10 Rust breakages all caught (`docs/testing/runs/live-captions-mutations-20261124.json`; nine by the tool, one by hand because its text appears three times in a file). The frontend typing and confirmed-word functions have tests (5 and 4) but were not mutation-checked.

## 9. Commit 10, live captions round two (`1aa7432`)

**Asked:** build "prefer the warm local server" and the model pre-download.

* **Warm local server preferred.** `plan_for` order is now: off conditions, then privacy mode (local), then a Deepgram key the owner added (cloud), then **a local server that is already running (`Gates.local_warm`)**, then a Groq key, then local. Warm is one local connection per turn (`lazy_stt::is_stt_responsive`). **Captions never start the server** (a test forbids `ensure_stt_running` in the module), because a loaded model costs about 300 MB of RAM against the 600 to 700 MB ceiling.
* **Speech model status and background download.** New `src-tauri/src/stt_model.rs`: reads the model cache (`%LOCALAPPDATA%\moonshine_voice\...` on Windows) and reports ready / downloading with megabytes / missing / unknown; two commands, `stt_model_status` and `stt_model_prefetch`; the download runs `stt_server.py --prefetch` (no server, nothing loaded into memory). `/health` now says `model_loaded`. A card in Command Hub, Features, Voice shows the state with a **Download now** button and polls every 3 s while downloading.
* Why it matters: the first use of the local engine downloads about 245 MB (decoder 140 MB, encoder 90 MB and others). On this machine that took over 10 minutes at 100 kB/s, and until it finished the server did not even listen.
* The bundled copy of the server (`src-tauri/resources/server/stt_server.py`) is now identical to `server/stt_server.py` again (it had not carried the earlier fixes).
* A log line now names the source each turn uses (`stt_stream: this turn's live captions come from Local|Groq|Cloud`).
* **Live proof in the real app** (`scripts/test-live-captions-source.mjs cold|warm`): with a made-up Groq key and **no** local server the turn chose Groq and did not start the server; with the server running it chose Local; the status command and the card answered ("Downloaded"). The whole isolated script then passed **33 of 33**, and the end-to-end caption run again showed the first caption at about 0.67 s.
* Mutations: 8 of 8 caught (one survived first because its test used a layout that could not be mistaken; the test was made realistic and it was then caught).
* New score: Built 97, Tested 90, Live 40, Safety 85, Industry 68 gives **72.4 %, still 3.5 stars**.

## 10. Test totals at the end of the day

| Suite | Result |
|---|---|
| Frontend (vitest) | 1295 tests pass (123 files); `tsc` clean |
| Rust, targeted for what changed | 183 (captions, capture, speech) and 42 (stream, model, lazy_stt) pass; earlier full runs: 3516 at the eight-features commit |
| Gates | repo hygiene, turn-end, IPC contract (272 commands, 213 invoked), window opacity, work-card drift, dead-code ratchet (137 known, none new) pass |
| Isolated live checks | 33 of 33 (`test-isolated-app.mjs`), battery 10 of 10, captions end-to-end all pass, source choice cold and warm pass |
| Mutation checks | 24 + 10 + 8 = 42 new Rust breakages caught, plus 27 at the eight-features commit |

## 11. Pitfalls found (so they are not hit twice)

* The isolated app very often **dies silently on its first start after a rebuild**; starting it a second time works.
* Captions are **off while NEXUS is speaking** (by design), so a test that says real commands aloud must wait for the reply to finish. This caused two apparently failed runs that were not bugs.
* This machine's system Python had no `websockets` package, so the local server's `/stream` could not work with it. The installer requirements (`uvicorn[standard]`) are right; a test venv exists at `D:\nexus-captions-venv`.
* A source-scanning test that cuts a file at the first `#[cfg(test)]` is broken by adding a test-only item above the real code; cut at `mod tests {` instead.
* The tool that applies one deliberate breakage cannot handle text that occurs more than once in a file; do those by hand and restore.
* Quoting in shell here-documents and `node -e` strips backslashes: write scripts with the file tool.
* The Windows event queue in PowerShell drops events from a speech synthesizer; collect them in compiled code.
* `get_settings` does not return API keys, so a test cannot read a key back to prove it was set.

## 12. What is verified and what is not

**Verified (with the evidence above):** the Kiro hook file and parsing; the battery guard's decisions, countdown, cancel, charger stand-down and no-power-off safety with a fake reading in the real loop; the webhook's reply to the tool-call hook (unit level); the caption chain end to end with the real local engine and synthetic speech; the choice of caption source; the speech model status command and card.

**Not verified:** a real low battery; a real power-off or hibernation on this machine; a real Claude Code receiving the stop order (the installed hook script is only rewritten at the next "hook up" or automatic refresh); a real Kiro; Codex and Copilot hooks (not installed with the tool-call event); a person speaking into a real microphone; Groq or Deepgram with a real key; pixels actually painted (a hidden desktop proves the text is in the page, not a screenshot); a first-time model download from nothing through the new button (only unit tests cover the missing and downloading states).
