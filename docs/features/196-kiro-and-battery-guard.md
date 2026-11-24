# Kiro as a watched agent, and the battery guard (2026-11-24)

Built from the research in `docs/research/watch-then-power-off-battery-guard-and-agent-coverage-2026-11-24.md`. Reviewed by an Opus 5.5 advisor (read-only) before it was finished; every blocker it found is fixed (table at the end).

## 1. Kiro

Kiro (AWS) is the 13th coding agent NEXUS can follow. "After Kiro finishes shut down the laptop", "hook up Kiro", "remove the Kiro hook" now work like for the other agents.

* **Hook file:** `~/.kiro/hooks/nexus-agent-event.json`, a file NEXUS owns outright (a file of that name that is not ours is never overwritten or removed), in Kiro's documented schema: `{"version":"v1","hooks":[{name, trigger, action:{type:"command", command}, timeout, enabled}]}` with a `Stop` hook and a `UserPromptSubmit` hook (timeout 10 seconds). The command has no quotes (it must run in cmd and in PowerShell); the script path is the short form when it has a space.
* **Kiro's CLI v3 reads `~/.kiro/hooks/` in every workspace. The Kiro IDE reads hooks from a project's `.kiro/hooks` folder only** (no global IDE path is documented), so the IDE needs the same file copied into a project; NEXUS does not touch project folders.
* The stop payload is JSON on stdin; its fields are not documented, so `cwd`, `session_id` and `assistant_response` are read best effort. The hook script prints nothing for Kiro (its prompt hook may add stdout to the prompt).
* Found by folder (`~/.kiro`) or by `kiro-cli` / `kiro` on PATH; process words `kiro` for the observer; spoken `kiro` / `kyro` / `kiro cli`.
* **Not run against a real Kiro.** Unit tests cover the file, the install / remove / foreign-file rules, the event parser and the spoken sentences.

## 2. Battery guard

Setting `batteryGuard`: **off** (default) / **dry** (practice) / **on**. Also `batteryGuardPercent` (3 to 20, default 5), `batteryGuardAction` (`shutdown` or `hibernate`), `batteryGuardStopAgents` (default on). All appear in the Command Hub, Features, Agents.

**What you say**
* "When the battery is below 5 percent stop Claude and shut down the laptop" (also "if the charge drops under five percent tell Claude to stop here and shut down", "...hibernate...", "below 8", "can you ... if the battery goes under 8 percent"): sets the level, the action and the stop option. **It never switches the guard on by itself**: if it was off it goes to practice.
* "Allow battery shutdown": on. "Practice battery shutdown": practice. "Turn off the battery guard": off. "Battery guard status".
* "Cancel" during the countdown stops it. Negations ("don't allow battery shutdown") change nothing. Dictation ("type when the battery ...") is not claimed.

**What it does (on)**
1. Reads the battery every 20 s (5 s when low, 2 s while counting). A desktop, an unreadable level and an unknown charger state never act.
2. While an agent is working: a spoken warning at 15 %, and again at the trigger + 3 (default 8 %).
3. At the trigger **on battery**: says what is about to happen; for Claude Code, starts a stop order; starts a 30 second countdown.
4. **Stop order.** There is no channel into a running agent. Claude Code's tool-call hook already runs before every tool; for the next 15 s it answers "deny" with the reason, so Claude is told to stop and say where it stopped. If it still calls tools after 15 s the answer also carries `continue:false` (Claude Code's hook documentation: that ends the agent loop and wins over the deny). Other agents are only warned aloud (no verified way to stop them). Nothing is killed.
5. Countdown ends: shutdown (goodbye, apps closed gracefully, a last 3 s to cancel) or hibernate. If the graceful shutdown stops short because an app asked to save, 45 s later it hibernates instead (or, with hibernation off in Windows, does a plain shutdown). A spoken "cancel" is never overridden. If nothing worked it says so and lifts the stop order.
6. Stands down on a charger (even one reading), a recovered level, a spoken cancel, or the guard being switched off or to practice.

**Rules that are tested:** only a real "cancel" cancels (another power command starting does not); a flickering charger does not undo a cancel (it takes 60 s of charger); practice then allow works (the loop restarts the engine on a mode change); the end announced is the end that happens; a hibernate setting without hibernation in Windows becomes a shutdown and says so; a stop order never outlives 5 minutes, never survives a stand-down or a failed power call, and a panic in the loop restarts the loop.

**Hibernate** is a new power kind (`PowerKind::Hibernate`, `SetSuspendState(hibernate=true)`). The spoken phrase "hibernate my laptop" still means sleep, as before (not changed).

## 3. Review findings (Opus 5.5, read-only) and what happened

| Finding | Result |
|---|---|
| A shutdown that stops short (an app refuses to close) leaves the laptop to die and keeps refusing Claude's tools | **Fixed:** fall back to hibernate or a plain shutdown after 45 s; the stop order is cleared on every way out |
| Hibernate can fail silently | **Fixed:** capability check (`GetPwrCapabilities`), falls back to shutdown and says so; a failed call is spoken |
| Any other power command cancelled the guard | **Fixed:** a spoken-cancel counter (`power_cmd::cancel_count`) replaces the generation check |
| Practice blocked the real thing | **Fixed:** the loop restarts the engine when the mode changes |
| On to practice mid-countdown kept counting | **Fixed:** treated like off |
| A charger blip undid a cancel | **Fixed:** 60 s of steady charger needed |
| Loop panic is silent | **Fixed:** supervised, restarts |
| End action read at fire time | **Fixed:** the announced end is kept |
| `continue:false` hides the reason from Claude | **Fixed:** deny first, `continue:false` only after 15 s |
| A bad token printed an error body into Claude's hook | **Fixed:** always a plain `{}` (status 401) |
| "don't allow battery shutdown" switched it on; "my battery is at 5 percent, shut down" armed it; polite phrasings and "below 5" missed; dictation claimed | **Fixed** (negations, a condition word is required, "can you" allowed, a level without "percent", "type ..." skipped) |
| Kiro prompt hook may take stdout into the prompt | **Fixed:** nothing is printed for Kiro |
| Kiro hook `timeout` units, shell | Docs say seconds; the shell is unknown, so the command has no quotes |
| "hibernate my laptop" still means sleep | Left as it was |
| "cancel the battery shutdown" is not a cancel phrase (it switches the guard off, which is safe) | Left, documented |

## 4. What was verified, and what was not

* Rust unit and integration tests for every rule above; the sentences from the owner reach the guard before any other parser; the hook script prints the answer for a tool call; the webhook answers the tool-call hook with the order only while one is active and only for Claude Code and Devin; mutation checks (see `docs/testing/runs/battery-guard-and-kiro-mutations-20261124.json`).
* The isolated build test (`scripts/test-isolated-app.mjs`) runs the sentences through the real app and checks the settings; it only switches the guard to "on" when this machine's battery is above 40 % or charging.
* **Mutation checks: 24 of 24 killed** (two runs, `docs/testing/runs/battery-guard-and-kiro-mutations-20261124.json` and `-b-`): charger standing the guard down, cancel, stop-order expiry, the last look at the setting before the power call, the hook script's answer for a tool call, the webhook's answer, and others.
* **Live checks on the isolated build (contributor profile, hidden desktop, 2026-11-24): 32 of 32 pass**, including the guard defaults, the owner's sentence stored (8 percent, hibernate), a sentence never switching the guard on, a negation changing nothing, "allow battery shutdown" switching it on, "turn off the battery guard", the Kiro sentence, and the Features page showing the battery guard row. This machine has no battery, so the guard was left to do nothing (as designed), not exercised at a low level.
* **The guard was made to trigger in the real app without any power-off (2026-11-24, owner asked not to shut anything down).** A debug-build-only test hook (`NEXUS_FAKE_BATTERY_FILE`: a text file holding "4" or "4,ac") feeds the real loop a made-up reading; while it is set the guard refuses to power anything off in any mode. `scripts/test-battery-guard-live.mjs` ran against the isolated copy: **10 of 10 pass.** Practice mode at 4 % said what it would do and started nothing; "on" with a charger did nothing; "on" on battery at 4 % said the warning and started the 30 s countdown; a spoken "cancel" cancelled it; after a recovery it could start again; plugging in the charger stood it down; a countdown left to run out only logged "nothing is powered off". Not covered by this run: stopping a real Claude Code (no coding agent was running in the isolated copy, and the webhook port is held by the owner's NEXUS), and the real shutdown / hibernate call.
* A finding while testing: `scripts/build-isolated-test.mjs` did not rebuild `frontend/dist`, which the debug build embeds, so the first run showed an old Features page (the one failed check). The script now builds the frontend first.
* **Not run:** a real low battery, a real countdown or power-off, a real Claude Code `PreToolUse` hook receiving the stop order (the exact accepted JSON should be confirmed with the installed Claude Code version, and the installed hook script is only rewritten at the next "hook up" or automatic refresh), a real Kiro, hibernation on this machine.
* **A note on the isolated test copy:** it shares the owner's home folder, so its "connect coding agents automatically" step looked at the owner's real agent settings (the log shows `[AUTOHOOK] Codex not hooked up`; nothing was changed because every hook was already installed). `run-isolated-test.ps1` now switches that step off in the test profile before it starts.

## 5. Owner checklist (about 10 minutes, after a rebuild)

1. "Hook up Kiro" (if Kiro CLI is installed), then run one Kiro task; log line `[WATCH-POWER] agent event: kiro Stop`.
2. "Practice battery shutdown", then "when the battery is below <your current level + 1> percent stop Claude and shut down the laptop" while on battery with a Claude task running: you should hear the practice line and nothing is switched off.
3. "Allow battery shutdown" at a level just below the current one, with Claude working: the countdown is announced; say "cancel": nothing happens and Claude can work again.
4. Run the real thing once on purpose with unsaved work in Notepad: confirm the "save?" prompt and the fall back to hibernate after about 45 seconds.
5. Re-run "hook up Claude" so the hook script learns the new tool-call answer, and read the Claude Code version's behaviour when the stop order is active (`[BATTERY] stop order` in the log).
