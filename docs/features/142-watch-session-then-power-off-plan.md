# Watch this session, and if it finished perfectly, shut down the laptop (PLAN ONLY)

Status: plan, nothing built. Builds on `138` (power phrases), `122` (agent watcher), `139` (agent task reconciliation).

## What the user wants

"Nexus, watch over this session until it is complete, and if it is perfect, shut down my laptop."
Said in many different ways. NEXUS observes the coding session on its own, decides whether it ended well, and only then powers off, with no unsafe surprise.

## What exists today (read from the code, 2026-10-18)

| Piece | Where | What it does | Problem for this feature |
|---|---|---|---|
| Watch intent | `intent_parser.rs::parse_coding_command` (0a/0b), `ParsedIntent::WatchAgentTask` | 4 rigid regexes ("watch over this and update me when X finished") | No power clause, no "perfect" condition; every other wording misses |
| Dispatch | `orchestrator.rs::run_watch_agent_task` | Picks target from name or foreground title, then sidecar watch or desktop watch | Result is only a spoken alert |
| CLI watch | `agent_watch.rs::start_watch` + sidecar `server.cjs` | **Spawns a NEW agent process** in a PTY and waits for the adapter's `task_complete` | It watches a fresh session, not the one the user is in ("this session"). It can say "watching Claude" while watching nothing real |
| Desktop watch | `agent_watch.rs::poll_desktop_task_quiescence` | "Done" = no file modified in the TOP level of the project folder for 6 s | A long thinking or test-running agent writes nothing for minutes, so this reports "done" too early. Fine for a notification, **not safe for a shutdown** |
| Keep awake | `keep_awake::acquire_lease` | Held during the watch | Reusable as is |
| Power | `power_cmd` (`begin`, `is_current`, `cancel`, `FINAL_CANCEL_WINDOW_SECS`, `goodbye_line`), `orchestrator::start_power` | Goodbye line, graceful app close that names apps asking to save, 3 s cancel window | Reusable as the last step |
| Local trigger | `webhook.rs` (127.0.0.1:39220, bearer token) | Any local script can post a transcript | Can carry agent-side "I am finished" signals |
| Screen text | `ocr.rs`, UIA window text | Can read a terminal window | Fallback evidence only |
| Compound split | `command_center::split_compound` splits on " then ", ", then" | "watch this, then shut down" would be cut into two commands and the watch would be lost | The new intent must be claimed BEFORE the compound fast path (`orchestrator.rs` ~1695) |

## Design

### 1. One intent: `WatchThenPower`

`ParsedIntent::WatchThenPower { agent: Option<String>, kind: PowerKind, require_clean: bool, deadline_mins: Option<u32> }`

Parsed by a slot grammar, not one regex, so word order does not matter. An utterance is claimed when it has all three parts:

- **WATCH**: watch, watch over, monitor, keep an eye on, keep track of, babysit, look after, supervise, stay on, wait for, track, observe, follow, see this through
- **SUBJECT**: this / it / that / this session / this task / the session / my session / this chat / this run / this build / this agent / the agent / my coding agent / an agent name (claude, codex, antigravity, cursor, trae, windsurf, aider, opencode, hermes, vs code, gemini)
- **END**: until it is complete / done / finished / over / through / ready / stops / stops running / has stopped / is done / ends / wraps up / completes / has nothing left to do
- **GATE** (optional, sets `require_clean`): if it is perfect / if everything is perfect / if all is good / if it went well / if it succeeds / if it works / if there are no errors / only if it is clean / if there are no issues / if all tests pass / only if it is successful. No gate word: still require clean (the safe default), say so out loud.
- **POWER**: any phrase `power_cmd::parse` accepts, plus the casual set from doc 138 ("shut down my laptop", "turn off the pc", "power off", "shut it down", "switch it off", "put it to sleep", "sleep" / "restart" / "lock" are allowed but the example above is shutdown).
- **OPTIONAL FLAGS**: "even if it fails" sets `require_clean = false`; "but not if it fails / if something breaks, don't" sets `require_clean = true`; "within 2 hours / by 3 am" sets `deadline_mins`.

Example phrasings that must all land on one intent (these become the test corpus, 60+ with order swaps):

1. watch over this session until it's complete and if it's perfect shut down my laptop
2. nexus, can you watch this session and when it's done, if everything is perfect, power off the pc
3. when claude finishes, if there are no errors, shut down my laptop
4. keep an eye on this and turn off the computer once it's finished successfully
5. if the build passes and the agent is done, shut my laptop down
6. shut down my laptop after this finishes, but only if it went well
7. monitor codex, and when it's done and all tests pass, switch off the laptop
8. wait for this to finish and then shut down, but don't if there are errors
9. once antigravity is done and it looks perfect, shut it down
10. stay on this session; if it completes without problems, power the laptop down
11. let it finish, check it's perfect, then shut the system down
12. I'm going to bed, watch this and shut down when it's perfect
13. after this task is complete, if nothing failed, put the laptop to sleep
14. shut down the laptop when it's done even if it fails (require_clean = false)
15. watch it and shut down in two hours at the latest (deadline)

Also claimed locally: "never mind the shutdown, just tell me" (downgrade to notify only), "cancel the shutdown plan", "what are you watching", "change that to sleep", "shut down even if it fails". All route to a small `PendingPowerPlan` state, never to the cloud model.

### 2. Observe the REAL session (the hard part)

Order of preference, strongest first. Completion needs at least two independent signals.

1. **Agent-side signal (strongest).** Claude Code has a `Stop` hook and a `Notification` hook; Codex has a `notify` command; other agents have similar. NEXUS offers a one-time "connect this agent" step that adds a hook which POSTs `{agent, session, event, exit_reason}` to a NEW localhost endpoint (`/agent-event`, same bearer token pattern as `webhook.rs`, never 0.0.0.0). The agent itself says "I stopped", so there is no guessing. Needs the user's one-time approval to edit the agent's settings file; never done silently.
2. **Attach to the real process.** Find the agent's process tree (claude/codex/node under the terminal or IDE). Completion = the whole tree is idle (CPU near 0, no child processes such as test runners or builds, no disk write) for N seconds. This replaces "top-level file mtime".
3. **Window evidence.** Read the terminal or IDE window text (UIA first, OCR fallback) for: spinner or "esc to interrupt" gone, prompt back, "Done", summary present. Also catches blocking questions ("Do you want to proceed? (y/n)", permission prompts, login) which mean NOT finished.
4. **Sidecar PTY watch** stays for the case "NEXUS starts the task itself" (`ask claude to ...`), where it really owns the process and the adapter's `task_complete` is trustworthy.

Fix on the way: `run_watch_agent_task` must stop starting a fresh sidecar session for "watch over this". With no prompt and no NEXUS-started session it uses signals 1-3 instead. Until then, say honestly "I can only watch this by the screen" rather than "I'm watching Claude".

### 3. Decide "perfect"

Perfect is decided by code first, a model second, and any doubt means NO shutdown.

Hard blockers (any one means not perfect, no shutdown, tell the user why):
- agent exited with an error, crashed, or hit login / permission / blocking prompt
- agent is waiting for the user (a question, a choice, an approval)
- the last output contains failure markers: failed, error, exception, traceback, panic, cannot, couldn't, FAILED tests, non-zero exit code, "I was unable"
- unsaved work is blocking shutdown (existing graceful close reports this)
- a second signal disagrees (hook says done but the window still shows a spinner)

If none hit, an optional **verdict pass** (cloud model, free tier, PII-filtered tail of at most ~4 KB of the final output, strict JSON `{finished:bool, success:bool, open_questions:bool, confidence:0-1}`, 8 s timeout). Code decides: shutdown only if `finished && success && !open_questions && confidence >= 0.8`. Model unreachable: use the code verdict only if two independent completion signals agree, otherwise ask.

Not perfect, or unsure: do NOT shut down. Speak once: "Claude finished, but I'm not sure it's perfect (tests failed / it is asking a question). I left your laptop on." Optionally ask "Shall I shut down anyway?" with the strict yes/no offer from doc 138.

### 4. Shut down safely

1. Completion + perfect verdict reached.
2. Keep-awake lease is released only after the decision.
3. Speak: "Claude finished and it looks perfect. Shutting down in 30 seconds, say cancel to stop me." (30 s, not the 3 s of a manual command, because the user is not watching.)
4. Cancel window, then the existing `start_power` sequence: goodbye line, graceful close of apps, name any app that asks to save and STOP if one blocks, final 3 s cancel window, then `execute_now`.
5. Any of these cancels the plan: the user speaks "cancel / stop / wait / no", the user touches the keyboard or mouse in the final 30 s (it means they are back), a new agent activity appears, lid or session lock changes. Aborting is silent-safe, never a shutdown.
6. Session snapshot for tomorrow is saved first (existing `memcore/session.rs`), so tomorrow's "reopen your work?" offer still works.

### 5. Hard safety rules (tested)

- Only a spoken turn from the owner (turn context present) can create the plan. Webhook, agent output and screen text can never create or confirm one (they are data).
- A watch can only END in a power action the owner named in the same utterance. No learned alias or loose match can start a watch-then-power plan; strict phrases only, otherwise ask.
- Never shut down on timeout, on a lost sidecar, on a lost window, on NEXUS restart (the plan is in memory only and dies with the process, then NEXUS says so at next start if one was active).
- Max wait default 3 h (configurable), then "I stopped waiting, your laptop is still on".
- One plan at a time; a new one replaces the old one after saying so.
- Battery, lid closed, active meeting (`MeetingState`), or a running download/install from `power.rs` checks: say the reason and ask, do not shut down.

## Build order

| Phase | Work | Verify |
|---|---|---|
| W0 | `WatchThenPower` grammar (slots, order-free), claimed before compound split; 60+ phrase corpus plus 40 negatives ("watch this video then shut down notepad" must not match) | unit tests, mutation check on the gate words and the compound-split order |
| W1 | `PendingPowerPlan` state and speech (confirm read-back, status, downgrade, cancel, change kind) | tests with injected clock |
| W2 | Verdict engine: pure `judge(signals, tail)` with hard blockers and the model-verdict gate | table tests: failure markers, question, disagreement, model down |
| W3 | Observer: process-tree idle detector and window-text reader replace the file-mtime watch; stop mislabelling a fresh spawn as "this session" | tests on injected process snapshots; live check needs a real agent |
| W4 | Agent hook endpoint (`/agent-event`) and one-time "connect Claude Code / Codex" setup | token tests, localhost-only test, hook round trip |
| W5 | Shutdown stage: 30 s spoken countdown, cancel on user input, reuse `start_power` | tests for each cancel path, "never fires after cancel" |
| W6 | Docs change record and the AGENTS.md note | gates: lib tests, tsc, vitest, IPC and dead-code ratchets |

Cannot be verified without the owner: a real Claude Code or Codex session through the full path, a real Windows shutdown, the hook install on their machine, audio of the countdown.

## Decisions (owner, 2026-10-18: "use all the recommended answers")

1. Unsure or imperfect result: tell the user, stay on, then ask once "shut down anyway?" (strict yes/no offer from doc 138).
2. Agent hooks: NEXUS may add the Stop hook to Claude Code / Codex settings once, with the user's approval each time. Never silently.
3. Countdown when unattended: 30 s, "cancel" or any keyboard / mouse input stops it.
4. Verdict: cloud free tier with the PII filter; code-only checks with an ask on any doubt when the model is unreachable.

## Progress

- **W0 built (2026-10-18):** `src-tauri/src/watch_power.rs` (pure parser + `read_back`, 6 tests incl. a 1,000+ order-swap corpus and a negatives list), `ParsedIntent::WatchThenPower`, claimed in `parse_deterministic_inner` BEFORE the power and dashboard parsers (so before the compound "then" split too), routed in `center.rs` / `orchestrator.rs`. The runner `run_watch_then_power` only reads the plan back and says honestly that power-off after a watch is NOT armed yet. W1-W5 not started.
- **Ask-first wiring built (doc 138 follow-up):** in `orchestrator::try_power_and_howto` a bare power verb (`parse_weak`: "shut down", "turn it off", "restart", ...) and an end-of-day phrase (`parse_casual`: "that's it for today", "I'm calling it a night", "I'm up for tonight", ...) now ask once; a yes runs the normal graceful flow and teaches only a SHUTDOWN phrase; any other answer forgets the pending phrase. End-of-day phrases are stored under their own `casual: ...` key so a yes cannot teach the far broader "that's it".
- **W1 built (2026-10-18):** `power_plan.rs` holds the one plan in memory (dies with the process on purpose). Voice commands, claimed as `ParsedIntent::WatchPlanCommand`: "cancel the shutdown plan", "what are you watching", "never mind the shutdown, just tell me", "shut down even if it fails" (the ONLY way "only if perfect" is relaxed), "make it sleep instead" / "change that to restart". Each parser demands that EVERY word of the sentence belongs to its command's small vocabulary, so "restart the app instead" and "cancel the plan for the weekend" are never claimed. `ARMED_SUPPORTED = false` makes every answer say the plan is only noted and the laptop stays on; a test pins this until the observer phase flips it. 7 tests; 7 mutations killed (one was a redundant guard, removed).
- **W2 built (2026-10-19):** `session_verdict.rs`, the pure "was it perfect?" engine. `judge(evidence, require_clean)` -> `Go` / `Hold(reasons)` / `Unsure(reasons)`. Hard blockers: crash, login wall, a waiting prompt or trailing question, 40+ failure markers in the last output ("N failed", Traceback, panicked, "I was unable", rate limit, ...) with neutral phrases ("0 errors", "no failed tests") cancelled; "still busy" and fewer than two distinct signals can only give `Unsure`; the model's JSON (`parse_model_json`) can make the result stricter, never rescue a failure, and counts only at confidence >= 0.8; with no model and no output read there is no go. "Even if it fails" relaxes only failure markers, crash and the model's `success`. `spoken_hold` says why the laptop was left on. 12 tests, 12 mutations killed (2 survived first; tests added).
- **W3 built (2026-10-19):** `session_observer.rs` turns what can be seen into `Evidence`: process-tree finder (never NEXUS's own tree), `IdleTracker` (CPU, new child process, disk traffic; 45 s quiet and 3 samples), window-text classifier (spinner, "esc to interrupt", prompts, login wall) with a stability timer, `Observer::step`, `evidence_from_watch_json` for sessions NEXUS started through the sidecar, and `ProcSampler` (sysinfo). Honest limit: **process idleness alone can never give a go** (an agent waiting for its model is idle too), and **no window-text reader exists yet**, so for a session NEXUS did not start only the agent's Stop hook (W4) can supply the second signal. Both modules are `#[cfg(test)]` in `lib.rs` until W5 wires them, so no dead code ships.
- **W5 built (2026-10-19): the unattended part, OFF by default.** `watch_runner.rs`. A pure `Engine` (injected clock) decides; a 3 s loop gathers the observations and carries out the actions.
  - **Setting `watchPower`** in `NexusSettings`: `off` (default; anything unreadable also means off), `dry` (practice: says what it WOULD do, never does it) or `on`. Changed by voice, owner turns only (the new intents are side-effecting by default in `center::is_side_effect`): "allow unattended shutdown", "practice unattended shutdown", "turn off unattended shutdown", "is unattended shutdown on". Read fresh on every tick.
  - **Starting:** making a plan ("watch this session and shut down when it's perfect") resolves the agent (named, or from the window in front: Claude Code or Codex only), checks that its Stop hook is installed, and starts the loop. Without the hook, or with the setting off, NEXUS says why it is not watching and keeps the plan.
  - **Evidence:** the agent's Stop hook (the ONLY way a session NEXUS did not start can be judged), its process tree going quiet for 45 s, a new prompt (`UserPromptSubmit`) cancelling an older stop, a permission prompt (`Notification`, matcher `permission_prompt|elicitation_dialog`) meaning "waiting for you", and the agent's final message from the hook's stdin as DATA (masked, last 4 KB). No window-text reader exists, so for these agents the hook is required.
  - **Verdict:** `session_verdict::judge` on that evidence, then an optional free-cloud second opinion (logged in the Memory page's cloud log; skipped while name hiding is on) that can only make it stricter. It must stay Go for 15 s before anything is concluded.
  - **Not perfect / not sure:** NEXUS tells the user why and leaves the laptop on. Only with the setting `on` (and not "just tell me") it also asks "Shall I shut down anyway?" with the strict yes/no answer of doc 138.
  - **Perfect:** `dry` says what it would do and ends. `on` announces aloud (taking the floor at once) "... I'm shutting down your laptop in 30 seconds. Say cancel, or touch the keyboard, to stop me." and counts down 30 s. ANY of these stops it and says so: "cancel" / "stop" / "wait" (the same cancel the manual shutdown uses), a key or mouse movement after the announcement, a meeting starting, the plan cancelled or replaced, the setting changed away from `on`, the session becoming busy or imperfect again, or NEXUS being unable to tell whether the user is back. At the end it re-checks the setting, the plan and the `powerCommands` switch, and only then runs the normal shutdown sequence (goodbye, graceful close of apps, a last 3 s cancel window).
  - **Never powers off on:** a timeout (3 h default, 12 h ceiling, "within two hours" honoured), a lost process, no hook, a restart of NEXUS (the plan is in memory only), `off`, `dry`, "just tell me", or doubt of any kind.
  - **Verified:** 30 engine tests including 7 end-to-end timelines that drive the REAL observer, hook store-style evidence and verdict into the engine (a perfect finish fires once after the countdown; no hook, a failure in the final message, no message at all, a new prompt, off, practice and each cancel path do not); 19 deliberate-bug checks on the engine, all caught (two survived first, tests added). The hook now carries the agent's final message: the real generated script was run through cmd and curl.exe with a piped stdin and the request checked.
  - **NOT verified, and you should know before turning it on:** anything with a real Claude Code or Codex session (the exact JSON their hooks pipe in beyond the documented `last_assistant_message`, how Claude's `Stop` fires around tool use and sub-agents), whether the process-quiet signal behaves on your machine (an agent waiting for its model is quiet too, which is why a Stop hook is required as the second signal), the spoken countdown and the cancel by voice, and the real shutdown. **First run: say "practice unattended shutdown", make a plan, and watch the `[WATCH-POWER]` lines.** A laptop on battery with the lid closed was not considered.
- **W4 built (2026-10-19):** `agent_hooks.rs` + a new `POST /agent-event` route on the existing localhost listener (127.0.0.1:39220). The agent's Stop hook runs a tiny script (`<app data>/agent_hooks/nexus-agent-event.cmd`, `.sh` off Windows) that posts `{"agent":"claude","event":"stop"}`; the endpoint has its OWN token (keychain service `agent_event`), never the `/trigger` token that can run spoken commands, and the token lives only in that script, not in any agent config. A body carries two words (agent in {claude, codex}, event in {stop, notification}); extra fields are ignored and never stored. Voice: "hook up claude" asks first (`Offer::HookSetup`, strict yes), backs the file up, writes atomically, edits only NEXUS's own entries (Claude Code `~/.claude/settings.json` Stop + Notification hooks, merged beside the user's own; Codex `~/.codex/config.toml` top-level `notify`, refused when the user already has their own); "remove the claude hook" removes only ours; "is the claude hook installed" also says when it last reported stopping. Verified for real on this machine: the generated script run through cmd and curl.exe posts the exact request (path, token header, JSON body) to a local socket, including Codex-style extra arguments, which are ignored. Claude Code's hook format was checked against its documentation; Codex's `notify` was only confirmed as a top-level `array<string>` (how the JSON payload is passed was not documented, and the script ignores it either way). **Not run:** a real Claude Code or Codex session with the hook installed; the voice approval round trip. The events are recorded but nothing consumes them until W5.
- Verified (W0): full lib 1806 pass; 4 fail, none from this work (3 `intent_parser` PR-list tests from the GitHub dashboard work, 1 `auth_vault` keyring test, see the change record). Not run: anything spoken, a rebuilt app.
