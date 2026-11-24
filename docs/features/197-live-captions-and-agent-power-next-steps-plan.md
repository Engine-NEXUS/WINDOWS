# Plan 197: what comes next for live captions, the battery guard and "watch the agent, then power off" (written 2026-11-24)

Status of the work this plan follows: `docs/changes/207-session-2026-11-24-complete-change-record.md`. Evidence behind every number: `docs/research/2026-11-24-research-index.md`. Nothing below is built unless it says "done".

## 1. Where things stand

| Feature | Score | Stars | Why not higher |
|---|---|---|---|
| Live captions while you speak | 72.4 % (Built 97, Tested 90, Live 40, Safety 85, Industry 68) | 3.5 | No person on a real microphone; no real Groq or Deepgram key; Groq (the default with a key and a cold server) is 1.5 to 2.5 s behind |
| Battery guard | not yet scored | | Never run against a real low battery, a real power-off, or a real Claude Code receiving the stop order |
| Watch the agent, then power off (`watchPower`) | unchanged | | Built and unit-tested (30 engine tests and 19 mutations earlier), never run live; the setting is off by default |
| Kiro as a watched agent | not yet scored | | Never run against a real Kiro; IDE needs a project hook file |

Thresholds: 4.0 stars needs 75 %, 4.5 needs 85 %. The formula weights Built 20 %, Tested 20 %, Live-proven 30 %, Safety 15 %, Industry 15 %.

## 2. The shortest route to 4.0 stars for live captions

**One thing closes the gap, and it is not code: a person speaking into the real microphone.** At the ceiling rule (50 without a person, up to 70 with one session) Live goes from 40 to about 60 and the score from 72.4 % to about 78.4 %, which is 4.0 stars.

### 2.1 The owner's session (about 10 minutes, real microphone, local source)

1. Turn on "Live captions while you speak" in Command Hub, Features, Voice. Use "Local speech recognition only" for this test so the local server is certain to be the source. If the speech model is not downloaded yet, press **Download now** on the card and let it finish (it takes a few minutes; progress is shown).
2. Say ten different commands: short, long, with numbers ("remind me at five thirty"), with a pause in the middle, one while a fan or the TV is on, one quietly.
3. For each one write down: did words appear while you were still speaking; did the dimmed tail settle; did a pause make the text shrink; did the last caption agree with what you said; did the final line match.
4. Count the commands that went wrong. That count is the number that moves Live, and any defect becomes a ticket.
5. Optional, same sitting: enter a Groq key and repeat three commands with the local server **stopped** to see the Groq source (its extra requests show in the Groq console).

### 2.2 What the session decides

| Wrong commands out of 10 | Live | Score | Stars |
|---|---|---|---|
| 0 or 1 | 65 to 70 | 79 to 80 | 4.0 |
| 2 or 3 | 55 to 60 | 77 to 78 | 4.0 |
| 4 or more | stays near 40 to 45 | 72 to 73 | 3.5, and the defects found become the next work |

## 3. Work that can be done without the owner, ranked

Each item has: the problem, the design, what would prove it, and the risk.

### 3.1 Start the speech model download by itself (small)

* **Problem:** the card needs a button press; a first-time keyless user still waits for the whole download at first use.
* **Design:** when the local engine is going to be needed (no Groq key, or "Local speech recognition only" turned on, or the owner switches live captions on and a local source is possible) and the model state is `missing`, start `stt_model::prefetch()` once, quietly, with one spoken line the first time ("Downloading the speech model in the background, sir, about 250 megabytes."). Never in a meeting, never on a metered-connection setting if Windows reports one. Add a line to the first-run setup.
* **Prove:** unit tests for the decision (a pure function over the settings and the state); a live check in the isolated app with the cache pointed at an empty folder (needs the cache location to be overridable by an environment variable in debug builds only).
* **Risk:** a 250 MB download the owner did not ask for. Mitigation: only when the engine is needed, one line spoken, a Hub switch.

### 3.2 A switch to keep the local speech server warm on purpose (medium)

* **Problem:** with a Groq key the local server is not running, so captions use Groq. People who want local captions anyway (private, no quota, faster) have no way to ask for it without turning off Groq for everything.
* **Design:** a setting `liveCaptionsKeepLocalWarm` (default off, risk label "RAM about 300 MB") that starts the server when live captions are on and keeps it from idling out while the setting holds; the Hub row states the RAM cost. `plan_for` already prefers a warm server, so nothing else changes.
* **Prove:** the isolated app with the made-up Groq key and the switch on: the server appears, the source line says Local, and the idle monitor does not kill it; switch off: it goes.
* **Risk:** RAM. The documented ceiling is 600 to 700 MB; the server adds about 300 MB. The switch must say so and be off by default.

### 3.3 Use the end-of-speech flush safely (medium, the most visible quality gain)

* **Problem:** for short commands the last live caption can be a wrong guess ("open the connection" for "open the command hub"). The correct words arrive in the engine's flush when the stream is stopped, and the app drops that on purpose, because a late caption arriving after the final transcript would overwrite the final text.
* **Design:** on end of capture, keep the stream's generation alive for a bounded flush window (at most 400 ms) in which one more caption may be emitted **only if the final transcript for that turn has not been shown yet**; the caption pill then marks the turn final and ignores any further partial for that session (the pill already receives a `session` id and ignores nothing today; make it ignore partials whose session is not the newest or whose turn is final). The Rust side ends the window the moment the final transcript event is emitted.
* **Prove:** a unit test with a scripted server that sends the flush text after the stop message (the existing mock server supports scripted replies); a pill model test that a partial after a final is ignored; the end-to-end script's "last caption vs final" line should go from 1 differing word to 0 on the short sentence.
* **Risk:** an ordering race between the flush and the final. This is why the pill-side guard is part of the same change.

### 3.4 Earlier Groq captions (small, do last)

* Measure whisper-large-v3 turbo against large-v3 for the interim request latency, and consider the first request at about 1.0 s while the day's count is low. Trades quota for speed; Groq's per-model free limits are not published, so this stays a measured choice, not a guess. Needs a real key, so it waits for the owner.

### 3.5 A rendered pill test (small)

* The pill is tested as a pure model plus a source scan. Add a test that renders the component with a fake event source and asserts: the text only grows, the dimmed tail class appears on unconfirmed words, a final replaces the partial, a reply start hides it.

### 3.6 Make the isolated test less fragile (small)

* The isolated app often dies silently on its first start after a rebuild. Find out why (likely a stale WebView profile lock or the start-up lock racing the previous instance) and make the launcher retry by itself.

## 4. Battery guard: from "built" to "proven"

1. **Re-run "hook up Claude"** so the installed hook script learns the new tool-call answer (the script is only rewritten at the next "hook up" or the automatic refresh). Confirm in `~/.claude/settings.json` that the `PreToolUse` hook is present.
2. **Confirm the exact accepted JSON** with the installed Claude Code version: the answer used is `{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"..."}}`, and after 15 s also `"continue":false,"stopReason":"..."`. Run Claude on a harmless task, start a stop order (practice does not set one; use "allow battery shutdown" with a fake or a real low reading), and read what Claude says.
3. **Practice first:** "practice battery shutdown", then a level just below the current one while on battery with a Claude task running. Expect the practice line and nothing switched off.
4. **Countdown and cancel:** "allow battery shutdown" at a level just under the current one with Claude working. Expect the announcement, the stop order in the log (`[BATTERY] stop order`), and "cancel" restoring everything.
5. **The real power-off once, on purpose, with unsaved work in Notepad:** confirm the save prompt and the fall back to hibernate after about 45 seconds. Do this only when nothing important is running.
6. Score the guard only after steps 1 to 5; until then it is Built and Tested, not Live.

## 5. "Watch the agent, then power off": the live dry run

Not done because the owner could not shut down this machine. When it can be done:

1. `practice unattended shutdown` (sets `watchPower` to dry).
2. Start a short Claude Code task and say "watch this session until it's done and if it's perfect shut down my laptop".
3. Read the `[WATCH-POWER]` log lines: the hook's Stop event, the 45 s quiet, the 15 s verdict hold. Dry mode never powers anything off.
4. Only then, optionally, the real thing with a 30 second countdown that the owner cancels.

## 6. Agent coverage gaps (known)

| Agent | State | Next step |
|---|---|---|
| Claude Code, Codex, Antigravity, OpenCode, Gemini CLI, Aider, Kilo, Copilot, Windsurf, Cursor, Devin | hooks exist | NEXUS answers the tool-call hook only for Claude Code and Devin (the others have no tool-call hook NEXUS can answer; the Codex, Copilot and OpenCode permission hooks are also not installed), so the battery stop order only reaches those two |
| Kiro | hook file built | CLI global; IDE needs the file copied into a project's `.kiro/hooks`; payload fields unconfirmed against a real Kiro |
| Trae | no hook mechanism | observe only; NEXUS can only watch that it goes quiet and never calls the work perfect |

## 7. Getting this work into `main` (decision needed)

* The branch is **58 commits ahead of `origin/main`**; none of them exist on GitHub. Only 10 are from 2026-11-24. The other 48 (2026-11-11 to 2026-11-23: directions, Reminder Roll, quiet test mode, the Group 1 live runs and fixes, the installer pipeline, the speaker probe, window theming, the proactive-speech policy, Agent Island and others) are earlier unpushed work from other sessions, including a commit labelled "WIP: wire the universal reminder animation".
* The 2026-11-24 commits **do not apply cleanly on top of `main` alone** (tested in a throwaway worktree: the first one already conflicts in `AGENTS.md`, the work-card files and the process map), because they build on that earlier unpushed work.
* `gh` (the GitHub command line) is not installed on this machine, so a pull request cannot be opened from here without installing it or using a token.
* Options: (A) one PR with the whole branch as it is (58 commits); (B) the same content squashed into 8 themed commits (local history is rewritten after a backup branch is made); (C) a PR of only the 2026-11-24 work, which needs the earlier work merged first. The choice is the owner's; see the message that accompanies this document.

## 8. Order of work if the owner is not available

1. 3.3 the end-of-speech flush with its pill guard (largest visible gain).
2. 3.5 the rendered pill test (it protects 3.3).
3. 3.1 the automatic model download (needs the cache override for a live test).
4. 3.2 the keep-warm switch.
5. 3.6 the launcher retry.
Score effect without the owner: under 2 points in total; the owner's session is worth about 6.
