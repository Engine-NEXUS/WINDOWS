# 139: Ask-first shutdown phrases and "watch this, then power off" (phase W0)

Date: 2026-10-18. Plan: `docs/features/142-watch-session-then-power-off-plan.md`, builds on `138`.

## What changed

1. **Bare power verbs ask first.** `power_cmd::parse_weak` ("shut down", "turn it off", "power off", "restart", "lock it", "sign me out", ...) is now used by `orchestrator::try_power_and_howto`: NEXUS asks "Did you want me to ...?" and registers `Offer::Power`. "restart nexus" / "shut down nexus" are not power asks (a vocative needs a comma).
2. **End-of-day phrases ask first.** `power_cmd::parse_casual`: "that's it for today", "that's all for tonight", "I'm done for the day", "I'm up for tonight", "calling it a night", "I'm going to bed", "time for bed", "let's wrap up", ... Whole-utterance only; "that's it for today's agenda", bare "that's it", "good night" are left alone. Question: "Calling it a day, sir? Shall I shut down your laptop?".
3. **Learning.** A yes teaches only a SHUTDOWN phrase. End-of-day phrases are stored under a `casual: ...` key (the strict cleaner would have cut "for today" and taught a bare "that's it"). Any answer other than yes forgets the pending phrase (`forget_pending_alias`).
4. **Answers to a power question.** `memcore::offer::classify_for` is now called for the waiting offer: besides yes/no it accepts the action itself ("shut it down", "power off", "good night") for that question only, and more ways to say yes ("absolutely", "sounds good", "why not"). Negations win. (This classifier existed uncommitted but was never called.)
5. **W0: `watch_power.rs`.** Pure, order-free parser for "watch / monitor / wait for THIS-or-AGENT until it is done, [if perfect], shut down my laptop". New `ParsedIntent::WatchThenPower { agent, kind, require_clean, deadline_mins }`, claimed in `parse_deterministic_inner` before the power / dashboard parsers (so before the compound "then" split). Reads options: agent name, "even if it fails" (require_clean false), "but not if it fails" (true), "within two hours" / "no more than 30 minutes" (deadline).
6. **The runner is honest.** `orchestrator::run_watch_then_power` only reads the plan back and says power-off after a watch is NOT armed and the laptop stays on. Phases W1-W5 are not built.

## Rules the parser enforces (all tested)

- Needs a watch verb or an end clause ("when it's done"), a thing that can finish (agent name, session noun, or this/it/that), and a power phrase.
- Media things are refused ("watch this video until it's done then shut down").
- First person is refused ("when I'm done with this session shut down").
- Questions and negations are refused ("how do I ...", "don't shut down ...").
- A bare "shut down" / "turn it off" counts only with a watch verb, an agent or a session word, only as a shutdown, and only when followed by a clause word or the end ("shut down notepad" never).
- Restart / sleep / lock / sign-out need an explicit "my laptop".

## Verification

- `watch_power` 6 tests (25 sentences, 1,000+ order-swap combinations, 50+ sentences that must be ignored); `power_cmd` 23+ tests incl. 150+ end-of-day phrases; `offer` tests.
- Mutation checks on the parser (5): media check, negation guard, first-person guard, weak-power context, weak next-word check. Two survived the first time (first-person, next-word); tests were added and both are now killed.
- Full lib run (final): 1811 passed, 3 failed (the PR-list tests below). The dead-code gate shows 4 new items (`center_for`, `tts::read_tts_emotion_setting`, `vision` fields `provider` / `quota_hit`), all from other sessions' in-progress refactors; none are in the power or watch files. The 4 `offer.rs` power items it flagged before are now used.
- NOT run: anything spoken, a rebuilt app, the ask-first round trip with a real microphone, a real shutdown.

## W1 (same day, second commit): the held plan

`power_plan.rs` + `ParsedIntent::WatchPlanCommand`. `run_watch_then_power` now stores the plan; cancel / status / "just tell me" / "even if it fails" / "make it sleep instead" edit or report it. Everything said still states that nothing is armed (`ARMED_SUPPORTED = false`). Strict whole-sentence vocabularies per command; 7 tests and 7 killed mutations.

## W2-W4 (2026-10-19)

W2 `session_verdict.rs` (judge), W3 `session_observer.rs` (evidence; both `#[cfg(test)]` until W5), W4 `agent_hooks.rs` + `POST /agent-event` (own token; approval-gated, backed-up edit of Claude Code `settings.json` Stop/Notification hooks and Codex `config.toml` `notify`; only NEXUS entries are ever replaced or removed). A real generated script was run through cmd and curl.exe against a local socket in a test. See doc 142 for the details and the honest limits.

## W5 (2026-10-19): the unattended shutdown, off by default

`watch_runner.rs`, setting `watchPower` (off / dry / on), `agent_hooks` extended so a Stop hook carries the agent's final message and a new prompt cancels an older stop. See the W5 block in doc 142 for the exact conditions, every cancel path and what was NOT verified. Practice mode first.

## Live miss 2026-10-19: "practice unattended shutdown" was heard, then thrown away

The first live try said it three times and got "didn't catch that" each time. The audio and Groq were fine (the log showed `practice unattended shut down.`, `practice unobtended shutdown.`, `Raktis unattended shutdown.`); the dictionary gate in `stt::evaluate_transcript` blanked all three because under 30% of their words were in a 194-word list. The same gate also blanked `shut down my laptop`, `power off the computer` and `order paneer butter masala` (simulated against the real list).

- The gate is now skipped when NEXUS has a grammar for the sentence (`stt::is_known_command`: `parse_deterministic`, plan, watch and power parsers). Gibberish never parses. The word list also gained the power/session words.
- Every blank has a reason (`dict_gate`, `non_ascii`, `fragment`, `repeat_loop`, `cjk`, `too_short`, `hallucination`) in `stt:turn_stats`, a visible `[STT-DROP] heard '...' -> blocked by ...` console line, and a `missed_intents.jsonl` entry (source `stt_filter`).
- `scripts/run.ps1` now shows `[STT-DROP]` and `[WATCH-POWER]` lines. **Before this the `[WATCH-POWER]` lines this doc and doc 142 tell you to read were hidden by the console filter.**
- `NEXUS_VOCABULARY` teaches the three unattended-shutdown phrases (134 of the 140-word cap).
- `power_plan::parse_command` repairs a misheard "unattended" ("unobtended", "un attended", one-typo variants); "unintended" and "unextended" are excluded.
- Tests: 6 new in `stt`, 1 in `stt_groq`, 1 in `power_plan`; the grammar bypass is covered by a test that fails without it (mutation checked; a first version survived because the widened word list made it pass on its own).
- NOT fixed: `Raktis unattended shutdown.` (Whisper's guess for "practice") now passes the word gate, matches no grammar and goes to chat as an ordinary question; only the decoder prompt can make Whisper hear "practice". The "didn't catch that" reply when the filter, not the microphone, produced the empty text is unchanged. NOT run live (needs `nexus build`).

## More agents (2026-10-19): Antigravity, OpenCode, Gemini CLI, Cursor, Aider

Everything W4/W5 did for Claude Code and Codex now works through the same table-driven `agent_hooks::Agent`:

| Agent | Where the hook goes | Stop signal | Final text |
|---|---|---|---|
| Antigravity (App, `agy`, IDE) | `~/.gemini/config/hooks.json`, named hook `nexus-agent-event` (`Stop`, `PreInvocation`) | `Stop` with `terminationReason`, `error`, `fullyIdle` | `transcriptPath` (path-checked, last 256 KB) |
| OpenCode | its own file `~/.config/opencode/plugins/nexus-agent-event.js` | `session.idle` (sub-sessions ignored) | read through the SDK client; `permission.*` = needs you; user `message.updated` = new turn |
| Gemini CLI | `~/.gemini/settings.json` hooks | `AfterAgent` | `prompt_response` |
| Cursor | `~/.cursor/hooks.json` | `stop` (`status`) | `afterAgentResponse` (a separate `message` event) |
| Aider | `~/.aider.conf.yml` | `notifications-command` | last reply in `.aider.chat.history.md` of the posted working directory |

- The verdict only gets stricter from what an agent says about HOW it stopped: an `error` / `terminationReason` with error/abort/cancel words / Cursor `aborted|error` / an `ERROR_MESSAGE` after the last Antigravity reply = failed; Antigravity `fullyIdle:false` = still busy.
- A Stop with no text (Aider without a history file, a transcript that cannot be read) cannot reach "go": NEXUS tells you and, in `on` mode, asks "Shall I shut down anyway?".
- `run_watch_then_power` finds "this session" from the window title for any of them, else from the agent process under the foreground window.
- **Found live, fixed:** `agy` runs hook commands through `cmd /C` and a quoted path with a space fails ("is not recognized as an internal or external command"), so the Antigravity command is written with the 8.3 short path and refused when none exists. A real `agy -p` run in a temp project (a git repo; hooks in `.agents/hooks.json` are only loaded from a project root) posted `start` and `stop` through the generated script. The installer writes the GLOBAL `~/.gemini/config/hooks.json` (the documented location for the App, CLI and IDE); that file was not exercised, so the first `hook up antigravity` is also the first real test of it.
- **Also found:** the `[WATCH-POWER]` lines that docs 139/142 tell you to read were hidden by `scripts/run.ps1`'s filter; it now shows `[WATCH-POWER]` and `[STT-DROP]`.
- **Not run:** OpenCode, Gemini CLI, Cursor and Aider themselves (OpenCode's plugin was run under node with a fake client; Gemini is not installed here), Claude's and Codex's real Stop payload, the spoken flows, a real shutdown.

## Live fixes 2026-10-19 (evening): own hook not recognised, prompt echo, mumbled "yes"

- **Own Antigravity hook not recognised (real bug).** `agy` runs hooks through `cmd /C`, so the installer writes the 8.3 short path; `antigravity_group_is_ours` looked for the marker text inside the value and the short path (`NEXUS-~1.CMD`) has none. A second "hook up antigravity" then refused ("couldn't read the settings file safely"), `is_installed` was false (the watch plan said "hook up antigravity first" and never started) and "remove the hook" did nothing. Fix: new commands end with the marker as a trailing argument (`... antigravity stop nexus-agent-event`; the script reads only its first two); a group under our key whose every command is exactly `<NEXUS-~N.CMD | nexus-agent-event.cmd> antigravity <stop|start>` is also ours (the user's existing file), is upgraded in place, found and removable. Anything else under our key is still left alone.
- **Prompt echo.** Whisper continued the decoder prompt: `hook up anti-pilot. hook up anti-gravity.`. `agent_hooks::parse_command` and `watch_power::parse` now also try the LAST sentence alone (strict grammars unchanged). `hook up antigravity. open chrome.` and `hook up anti-pilot.` still parse as nothing.
- **A quiet "yes" heard as "dirt."** An armed question used to be dropped and the noise sent to the cloud. A 1-3 word non-command reply now gets ONE re-ask ("Sorry sir, was that a yes or a no?", `offer::reask_once`, `intent_parser::is_mumble`); a second unclear reply or any command behaves as before.
- **Earlier the same day (already committed):** the Groq decoder prompt may not exceed 896 characters (my additions made it 928, every turn fell back to slow local STT); `NEXUS_VOCABULARY` is now 856 chars with a character-limit test. A bare "yes"/"no" with a question waiting is an answer, not a greeting.
- **Verified:** 115 tests (offer, is_mumble, agent_hooks, orchestrator) passed before another session broke the lib-test build (`intent_parser.rs` PR-list tests, partial move of `subpage`); mutations killed: legacy-hook recognition, marker argument, last-sentence fallback. **Not killed/run yet** (lib tests did not compile at the time): the "legacy rule accepts any command", "re-ask unlimited" and "mumble accepts commands" mutants.
- **Not run live:** the global `~/.gemini/config/hooks.json` being read by `agy`, a spoken upgrade of the existing file, OpenCode/Gemini/Cursor/Aider, the countdown, a real shutdown.

## Live miss 2026-10-19 (22:30): the Stop arrived, nobody said whether it was great

- **Log:** `hook up antigravity` / `yes` worked; the Stop hook reached NEXUS for the first time (`signals=[AgentHook]` from tick 111 and for 17 minutes) but `ended` stayed false, so the engine concluded nothing and spoke nothing.
- **Cause:** the second signal could only be `ProcessIdle`, and the process tree also vetoed (`busy`). For Antigravity the matched tree is the whole Electron IDE (renderers, language server, file watchers): never 45 s quiet, `busy` flapped. The model cross-check only ran after a rule-only `Go`, so it was never asked either. And there was no spoken outcome for "a stop is pending but unexplained".
- **Fix:** `Signal::HookQuiet` (the agent's Stop is still its latest event after 45 s, its payload does not say `fullyIdle: false`); for IDE agents (`Agent::is_ide`: Antigravity, Cursor) process CPU / children / disk no longer feed `still_busy` (terminal agents keep the veto); a stop that stays unexplained for 150 s is said out loud once with the reasons (`STALL_NOTICE_MS`, `spoken_reasons`); `[WATCH-POWER] agent event: ...` per hook event and a tick line that prints on every change and now shows `stop_age_s`, `tail_len`, `stop_ok`, `fully_idle`, `tree_cpu`; the model cross-check result is logged.
- **Honest limit:** `AgentHook` and `HookQuiet` both come from the hook channel, so for an IDE agent the "two independent signals" rule is weaker. What still stands between a wrong Stop and a power-off: the 15 s settle, the model cross-check (can only be stricter), the announced 30 s cancellable countdown, input / meeting / plan cancels, and `dry` as the default. A later improvement would be an independent file signal (the transcript stops growing).
- **Verify:** 157 tests (session_*, watch_runner, agent_hooks, webhook, offer) pass; 11 mutations killed (quiet threshold, fullyIdle check, IDE exemption, stop-required, stall repeat, stall immediacy, later-event reset, Cursor-as-IDE, plus the earlier legacy-hook, re-ask and mumble rules). **Not run live:** the spoken "Practice run, sir: Antigravity finished and it looks perfect" after a real Antigravity task, the real Stop payload's `fullyIdle` / transcript text (the new `agent event` log line shows it), `on` mode.

## Live miss 2026-10-19 (23:50): a research report was called "not perfect"

- **Log:** the full chain worked live (Stop with 3871 B of text, `fully_idle=Some(true)`, `HookQuiet` after 47 s with a busy IDE tree ignored, spoken result about 70 s after the Stop). The result was wrong: *"the output mentions a failure (3 failing)"*. The task was research / plan / compare; Antigravity's report quoted "3 failing PR-list tests" as a finding about the repo (taken from AGENTS.md) and changed nothing.
- **Cause:** `scan_tail` is a plain word scan and cannot tell "my run failed" from "the report discusses failures"; the model cross-check only ran after a rule-only `Go`.
- **Fix:** the cross-check model now also reads the output when the ONLY blocker is a failure word (`wants_model_check`). Its answer has a new field `failure_in_this_run`; a failure word is cleared only when `failure_in_this_run == false`, `success`, `finished`, no open questions and `confidence >= 0.9` (`model_clears_failure_words`). Nothing else is relaxed: an error the agent itself reported, a trailing question, login wall, waiting prompt, busy tree, too few signals and a missing model answer all still hold. This knowingly breaks the old "the model can only be stricter" rule for this one hold, and the override is always said aloud (`cleared_failure_note`, in the practice, just-tell-me and countdown messages and in the tick log). A hold now quotes the line it objected to ("The line was: ...", the table cell or the stretch around the words).
- **Verify:** 162 tests (session_*, watch_runner, agent_hooks, webhook, offer); 11 mutations killed (confidence, missing field, open questions, finished, success, any-hold, require_clean, quote, two note paths, quote window). **Not run live:** the model's real judgement of this report (prompt quality unknown), `on` mode.

## Known unrelated failures

3 `intent_parser` PR-list tests ("open pr list" now parses as `OpenGitHubDashboard`) and a flaky `auth_vault` keyring test under parallel runs. The dead-code gate also lists items from other in-progress work (`center_for`, `tts::read_tts_emotion_setting`, `vision` fields).
