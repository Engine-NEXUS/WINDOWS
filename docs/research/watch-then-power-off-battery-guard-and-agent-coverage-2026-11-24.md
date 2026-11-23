# Does "shut down when the coding agent finishes" work? Low-battery stop? Which agents? (research, 2026-11-24)

Nothing was built for this; it is a read of the code, the project's own 204 tests, the owner's logs and settings, a probe of the real spoken-phrase parser, and public documentation.

## Short answers

| Question | Answer |
|---|---|
| "Nexus, shut down the laptop after Claude has finished working" | **Built and tested, never run for real, and switched OFF.** The sentence is understood; the shutdown only happens when `watchPower` is `on` (default `off`) |
| "If the battery is under 5 %, tell Claude to stop and shut down" | **Does not exist.** No code reads the battery level anywhere, and the sentence is not understood |
| Works for any coding agent | **Partly.** 12 agents are known; 9 can be followed to a finish; 1 (Trae) can only be observed; **Kiro is missing** |
| Is live captions at 4 to 4.5 stars? | **No: 3.0 stars (60.8 %).** 4.0 needs 75 %, 4.5 needs 85 %. See the last section |

## 1. Shut down after the agent finishes

**What exists** (`watch_power.rs`, `power_plan.rs`, `watch_runner.rs`, `session_verdict.rs`, `session_observer.rs`, `agent_hooks.rs`, `agent_tracker.rs`, `power_cmd.rs`): a parser for the sentence, a held plan, the agent's own "stop" hook as the finish signal, a verdict (did it end well?), and a countdown. To power off, all of this must hold at once: `watchPower` is `on`; a plan is held; the session ended on two independent signals; the verdict stayed "go" for 15 s; then a 30 s spoken countdown that any "cancel", key or mouse movement, meeting, or change of plan stops; then the normal shutdown (goodbye, graceful app close, a last 3 s cancel window). Timeout (default 3 h, ceiling 12 h), a missing hook, "just tell me", and a restart of NEXUS never power off.

**Tests:** all pass: watch_power 9, power_plan 10, watch_runner 37 (including seven end-to-end timelines), session_verdict 18, session_observer 18, agent_hooks 49, agent_tracker 4, agent_outcomes 6, power_cmd 23, agent_followup 30, agent_autohook 4 (**208**), plus 19 mutation checks recorded in AGENTS.md.

**The spoken phrase.** Probed through the real parser (`parse_deterministic`) 2026-11-24:

| You say | Understood as |
|---|---|
| "nexus shutdown the laptop after the claude has finished working" | watch Claude, then shut down |
| "after the claude has finished shutdown the laptop" | same |
| "when claude is done shut down my laptop", "shut down the laptop when claude finishes" | same |
| "after codex / antigravity / opencode / cursor / trae / gemini cli finishes shut down ..." | the named agent |
| "when all my agents are done shut down the laptop" | all hooked agents |
| "after any coding agent finishes shut down the laptop" | the session in front |
| "after **kiro** finishes shut down the laptop" | **not understood** |

**What the owner's machine shows (read-only look at settings and logs):**
* `watchPower` is not set, so it is **off**. A shutdown can never happen today. To try it safely say "practice unattended shutdown" (dry run: says it, never does it), then "allow unattended shutdown".
* The Claude Code hook **is installed and delivering**: 150 starts, 96 stops, 7 notifications in the logs (`[WATCH-POWER] agent event: claude Stop`). So the finish signal reaches NEXUS.
* **No plan has ever been held, no countdown, no dry run** appears in any log. The whole chain from the sentence to the countdown has only run in tests, never in a real session.
* Hooks are installed for Claude Code, Antigravity (`~/.gemini/config/hooks.json`), Cursor, OpenCode and Windsurf. **Codex has a config but no NEXUS hook**; Copilot has none. No events from any agent but Claude appear in the logs, so those hooks are untested in the field.

**Verdict:** the design is sound and heavily tested; it has not been proven live. It also depends on the hook's "stop" arriving, which the logs show it does for Claude.

## 2. Low battery: stop the agent here, then shut down

**Nothing exists.** There is no call to Windows' battery status (`GetSystemPowerStatus`) in the source. The two sentences probed ("when the battery is below 5 percent stop claude and shut down the laptop", "if the charge drops under five percent tell claude to stop here and shut down the laptop") were **not understood**; the second was even taken as "exit ghost mode" (a wrong match worth fixing on its own).

What would be needed, in order of safety:
1. **A battery watcher**: read the level and charging state about every 30 s (Windows `GetSystemPowerStatus`, no new dependency). Act only when on battery, never while charging.
2. **A warning first**, spoken at a higher level (for example 15 %, "sir, 15 percent and Claude is still working") and again at 8 %. A hard stop at 5 % is late: Windows' own default action at the critical level (usually 5 %) is to hibernate, so NEXUS would race the operating system.
3. **Stop the agent at a safe point.** There is no message channel into a running agent, so "tell Claude to stop here" has to be done through the hook that every tool call already goes through:
   * **Claude Code:** a hook may print `{"continue": false, "stopReason": "..."}` and Claude stops processing (official hooks documentation; the reference says it takes precedence over a block decision). It takes effect at the next hook event (before the next tool or at the end of the turn), not mid-thought. One public report says a `PreToolUse` hook returning it did not block; this needs a test with the installed version.
   * **Others:** blocking tool calls from a pre-tool hook is the common mechanism, but the support differs by agent and is **not verified** here for Cursor, Gemini CLI, OpenCode, Antigravity, Windsurf and Kiro. **Codex has a notify hook only (no blocking).** **Trae has no hook at all**, so the only options are a keystroke to its window or ending the process.
   * **Fallback for any agent:** a spoken and on-screen warning, then the existing graceful close (apps that ask to save are named and left open) before power-off. Killing an agent's process loses its in-progress edit, so it is the last resort and needs the owner's say-so.
4. **Then shut down or hibernate.** "Hibernate" is currently parsed as **sleep** (`power_cmd.rs` maps it to `Sleep`); for a dying battery a real hibernate (state saved to disk) is better than a shutdown, which loses open work. That is a missing power kind.
5. **Safeguards that already exist** and would apply: announced countdown, cancel by voice or any key or mouse, never in a meeting, a final summary told next morning.

## 3. "Any coding agent": what is covered

From `agent_hooks.rs` (`ALL_AGENTS`, 12) and the installed files:

| Agent | Finish signal | State |
|---|---|---|
| Claude Code | `Stop` hook | **delivering in the logs** |
| Codex | `notify` (turn complete) | known; **hook not installed** on this machine |
| Antigravity | `Stop` plus `PreInvocation` (shared with the `agy` CLI and IDE) | installed; not seen in the logs; must report fully idle |
| OpenCode | its own plugin file | installed; not seen in the logs |
| Gemini CLI | `AfterAgent` | known (its own `~/.gemini/settings.json`); not checked on this machine |
| Cursor | `stop` | installed; not seen in the logs |
| Aider | notifications-command plus its history file | known |
| Devin CLI | reads Claude's hook settings, relabelled | covered by Claude's hook |
| Windsurf | its hooks file | installed; not seen in the logs |
| Kilo Code | plugin | known |
| Copilot CLI | its hook file | known; **not installed** |
| Trae | **no hook mechanism exists**: observe only (process, window title), and a finish cannot be certain | observe-only |
| **Kiro** | **not supported** | **missing** |

**Kiro can be added.** Its documentation lists an "Agent Stop" hook (fires when the agent finishes responding) in both the IDE and the CLI; the CLI v3 hook files live in `.kiro/hooks/*.json` and receive the event as JSON (the CLI names it `stop`, the IDE legacy name is `agentStop`; the CLI v3 migration guide says a Stop hook cannot block with exit code 2 but can return a `block` decision to make Kiro continue). The 12 known agents plus Kiro would be the 13th; the hook installer, the parser's agent list, the settings paths and the event parser each need one entry.

**What "any coding agent" cannot promise:** an agent with no hook (Trae, and any future one) can only be guessed from its process and window, which the project's own rule (never shut down on silence alone) refuses to power off on. For those the honest behaviour is "I can tell you when it looks finished, but I will not shut down on a guess".

## 4. Suggested plan, if you want it built (nothing started)

| Step | What | Why first |
|---|---|---|
| 1 | One **live dry run** with Claude: say "practice unattended shutdown", then "watch claude and shut down the laptop when it finishes", let a short task finish, read `[WATCH-POWER]` | the only thing that turns "tested" into "works"; no code needed |
| 2 | Add **Kiro** (parser word, hook installer, event parser, tests) | small, same pattern as the others |
| 3 | Install the missing **Codex** and **Copilot** hooks via the existing "hook up" flow and run one task each | moves them from "known" to "seen" |
| 4 | **Battery guard**: read the level, spoken warnings at 15 % and 8 %, parser for the sentence, a `battery_guard` setting off by default, dry-run mode | the new feature |
| 5 | **Soft stop through the hook** for Claude (`continue:false`) behind the guard, tested against the installed Claude version, then the other agents one by one as verified | needs a live check per agent |
| 6 | A real **hibernate** power kind | for low battery it is the safer end state than a shutdown |
| 7 | Fix the misparse of a battery sentence as "exit ghost mode" | a plain bug |

Opus review is advisable before step 5 (it touches every agent's hook and can interrupt work).

## 5. Are the live captions at 4 or 4.5 stars? No

The project's own formula: stars = score / 20, rounded to the nearest half; the score weights Built 20 %, Tested 20 %, Live-proven 30 %, Safety 15 %, Industry 15 %.

| | Built | Tested | Live | Safety | Industry | Score | Stars |
|---|---|---|---|---|---|---|---|
| Live captions now | 90 | 75 | 20 | 85 | 60 | **60.8 %** | **3.0** |
| Needed for 4.0 | | | | | | 75 % | 4.0 |
| Needed for 4.5 | | | | | | 85 % | 4.5 |

Why it is not there, and what would move it:
* **Live-proven (20 to 65 or more)** is the whole gap: even with Live at 70 and nothing else changed the score is 75.8 % (4.0). It needs the owner's real run: words appearing while speaking, the Groq console showing the extra requests, the per-model limits question answered. Nothing in an empty room can do this.
* **Tested (75 to 85+):** the real HTTP request to Groq (`transcribe_interim_with_groq`) has no test against a local stand-in server because the Groq address is a constant; a test with an injectable address would close it. The caption pill's behaviour is only tested by source scan.
* **Industry (60):** commercial dictation shows words within about 300 ms, word by word. Groq has no streaming, so the Groq source shows text in steps one to two seconds behind. **4.5 is not reachable on Groq alone**; it needs a real streaming source (Deepgram, or a local streaming model, which costs RAM). 4.0 is reachable with Groq if the live run goes well.
* The first-request wait (1.6 s) means a short command never shows a caption: that is a cost choice, not a defect, but a reader may call it lag.

Recommendation: do the owner's live run first (about 10 minutes), then close the Groq request test, then decide whether a Deepgram key (or a warm local model) is worth it for 4.5.
