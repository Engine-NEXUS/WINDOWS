# 01 — Agent watch, keep-awake, sleep and wake: capability audit

Date: 2026-11-01. Read-only audit (no source edits, no build, no launch). One read-only command was run: `powercfg /a`.

Owner's wish: "If I leave and the laptop would go to sleep, NEXUS keeps it awake while coding agents (Claude Code, Codex, OpenCode, Antigravity, Gemini CLI, Cursor, Aider, Windsurf, Kilo, Copilot CLI ...) run. When all are finished it puts the laptop to sleep. When I come back and wake it, NEXUS tells me, reliably and in the right words, which agent completed ('Claude has finished, sir') or which needs my input ('Codex needs your input, sir'), for every agent, without issues."

Legend: **[V]** = verified by reading the code (file:line given). **[I]** = inferred (vendor behaviour, Windows behaviour, or likely runtime effect), needs a live test. **[D]** = taken from AGENTS.md / docs/changes.

---

## 0. One-paragraph verdict

About 60% of the feature exists, but only as an **explicit voice plan** ("when all my agents are done put my laptop to sleep", with the `watchPower` setting switched to `on`), not as a standing behaviour that kicks in when the owner walks away. Finish detection is solid for agents with a Stop hook. **"Needs your input" is detected for 5 of 12 agents but never spoken**: a waiting agent simply counts as "still working", holds the screen awake for up to 3 hours, and is never announced. **Nothing is told on resume from sleep**: the resume hello is deliberately content-free, and the saved agent outcome record is read only at the next app start. Several concrete bugs undermine reliability: the keep-awake watchdog can silently drop the lease mid-watch (and its refcount can be corrupted), Codex's final message is thrown away by the hook script, and hook events are lost whenever NEXUS is not running or restarts.

---

## 1. What exists, end to end

### 1.1 The path as built

| Step | Status | Evidence |
|---|---|---|
| Owner arms the watch | **Real, but voice-only and one-shot.** A plan must be said ("track all my agents", "when claude is done put my laptop to sleep"). No automatic arming when the owner leaves. | [V] `watch_power.rs:1-60` (parser; strict, needs an end clause + power phrase or "track"), `power_plan.rs:108-113` (`set` stores ONE plan in a static `Mutex`), `watch_runner.rs:419-438` (`wants_running` / `ensure_running` only run with a plan). |
| Sleep needs a setting | **Real.** `watchPower` must be `on` (default `off`; `dry` = practice). Track-only and notify-only plans run without it but never power anything. | [V] `watch_runner.rs:11,171-173,265-298,373-380`. |
| Hook install per agent | **Real** (approval, backup, only NEXUS entries touched). | [V] `agent_hooks.rs:1043-1116`, `orchestrator.rs:7130-7166`. |
| Agent → NEXUS event | **Real**: hook runs `nexus-agent-event.cmd <agent> <event>`, which `curl -m 3` POSTs stdin to `127.0.0.1:39220/agent-event` with its own bearer token. | [V] `agent_hooks.rs:239-262`, `webhook.rs:194-217`. |
| Event storage | **Real but volatile**: in-memory `VecDeque`, 16 per agent, 256 total. Lost on NEXUS restart; no on-disk spool. | [V] `agent_hooks.rs:24-27,1130,1417-1438`. |
| Detection of "agent is working" | **Partly.** An agent joins the plan when it posted any hook event since the plan was made, or (non-IDE only) its process tree uses > 3% CPU. | [V] `watch_runner.rs:541-551`, `session_observer.rs:20`. |
| Keep-awake lease | **Real but fragile** (see 4.1). Held while `agg.working` is non-empty; `ES_CONTINUOUS|ES_SYSTEM_REQUIRED|ES_DISPLAY_REQUIRED`; one heartbeat per loop tick; 10 s watchdog. | [V] `watch_runner.rs:698-706`, `keep_awake.rs:26-27,146-167,228-241`. |
| Finish detection | **Real** for hooked agents: "ended" = 2 distinct signals (agent Stop hook + 45 s hook quiet, or process idle 45 s) and nothing busy; then a verdict (Go / Hold / Unsure) from failure words, questions, login walls, an optional cloud model cross-check (stricter only). | [V] `session_observer.rs:265-296`, `session_verdict.rs:300-330`, `watch_runner.rs:476-537`. |
| Needs-input detection | **Detected, never announced.** `needs_user()` → `waiting_for_user` → a `Hold(WaitingForYou)` reason, but the agent is not "ended" (no Stop, < 2 signals), so it stays in `working` and the engine says nothing. | [V] `agent_hooks.rs:1515-1526`, `session_observer.rs:291`, `session_verdict.rs:321-323`, `watch_runner.rs:187-209`. |
| Per-agent announcement | **Real** (each finish told once after 15 s settle, again after a new turn), but only while others still work; the last one is folded into the final summary. | [V] `agent_tracker.rs:150-168`, `watch_runner.rs:191-194,222-229`. |
| "All done → sleep" | **Real in code, never run live.** Countdown 30 s announced, cancelled by voice / input / meeting / setting / plan / session change; then `start_power(Sleep)` → 4 s countdown → `SetSuspendState(0,0,0)`. | [V] `watch_runner.rs:287-329,926-951`, `orchestrator.rs:7613-7647`, `power_cmd.rs:56-61,684-690`. [D] doc 149 "Not run live: the `on` mode countdown". |
| Track-only "all done" | **Real**: final summary, tile windows, then `SC_MONITORPOWER` display-off only if idle ≥ 60 s and no meeting. | [V] `watch_runner.rs:222-229,838-898`, `window_tile.rs:262,278-288,428-458`. [D] "display-off on this Modern-Standby laptop" not run. |
| Outcome saved before power-off | **Real, only on `Fire`** (not for track/notify plans). Sealed store key `agent_overnight`, 36 h freshness. | [V] `watch_runner.rs:936-943`, `agent_outcomes.rs:12-75`. |
| Resume from sleep detected | **Real, heuristic**: wall-clock jump > 60 s over a 10 s thread sleep. | [V] `lib.rs:742-766`. |
| Spoken report on resume | **MISSING.** `on_resume` only speaks the time-of-day hello ("No personal content: just the hello"), after ≥ 20 min away and not if a hello was said in the last 2 h. The overnight agent record is read **only** in `start_boot_briefing` (app start). | [V] `memcore/briefing.rs:565-600` (`RESUME_HELLO_MIN_AWAY_SECS = 20*60`), `briefing.rs:32` (`HELLO_MIN_GAP_SECS = 2*3600`), `briefing.rs:633,677-679` (only reader of `agent_outcomes::peek`). |

### 1.2 Module map (what each file really contributes)

- `keep_awake.rs` — refcounted RAII leases + heartbeat watchdog; Windows `SetThreadExecutionState`; also a 3 s "working" poller (`start_working_poller`, `lib.rs:850`).
- `agent_watch.rs` — separate, older path: NEXUS *spawns* an agent through the Node `pty-manager` sidecar ("ask codex to X") and polls `/watch/{id}`; also a file-mtime quiescence watcher for IDEs. Speaks "Your X task just finished, sir." Acquires leases but **never heartbeats** (4.1).
- `agent_hooks.rs` — the 12-agent table, hook install/uninstall, event parsing, the event queue, `stop_is_latest`, `needs_user`, tail resolution (Antigravity transcript, Aider history, Devin relabel).
- `agent_tracker.rs` — pure: aggregate, `Announcer`, `Joiner`, sentences.
- `session_observer.rs` / `session_verdict.rs` — evidence and judgement.
- `watch_power.rs` / `power_plan.rs` — the spoken plan (parse, hold, edit, status).
- `watch_runner.rs` — pure `Engine` + the 3 s loop; keep-awake lease; countdown; follow-up commit; tidy-up.
- `agent_outcomes.rs` + `memcore/briefing.rs` — overnight record told at next app start.
- `proactive_policy.rs` — every spoken line goes through it (High = wait for a breakpoint, expire to a card after 10 min; non-critical ≤ 6 per hour, ≥ 20 s apart).
- `window_tile.rs` — columns + display off.
- `agent_followup.rs` — gated commit + draft PR (not part of this wish, but delays the countdown).

---

## 2. Per agent: how NEXUS learns "finished" and "needs input"

Hook events and their mapping are from `agent_hooks.rs:296-322,516,575-726,795-925` [V]; live status from AGENTS.md and docs/changes/139 + 149 [D].

| Agent | Finish signal (hook → kind) | Needs-input signal | Final text (tail) | Live status [D] |
|---|---|---|---|---|
| **Claude Code** | `Stop` → stop; `UserPromptSubmit` → start | `Notification` matcher `permission_prompt\|elicitation_dialog` → notification | stdin JSON `last_assistant_message` | UserPromptSubmit seen live through the quoted command; **real Stop payload never seen**. |
| **Codex** | `notify = ['<script>', 'codex', 'stop']` in `config.toml` (turn complete only) | **None.** Codex `notify` only fires on turn completion [I: Codex docs]; approvals are not reported. | **Lost (bug)**: Codex passes its JSON as an extra argv; the script reads stdin and "ignores" extra args (`agent_hooks.rs:233-234`), so `last-assistant-message` never reaches NEXUS → verdict always Unsure "could not read its final output". [V] script; [I] that stdin is empty. | Not run. |
| **OpenCode** | plugin: `session.idle` → stop; user `message.updated` → start | `permission.updated` / `permission.asked` → notification | read through SDK client in plugin | Plugin run under node with a fake client only. |
| **Antigravity** (App/`agy`/IDE) | `Stop` (+ `terminationReason`, `error`, `fullyIdle`) ; `PreInvocation` → start | **None** (only Stop/PreInvocation installed). | `transcriptPath` (path-checked, 256 KB) | **Only agent verified live**: `agy -p` with project-level `.agents/hooks.json`. Global `~/.gemini/config/hooks.json` (what the installer writes) **not exercised**. |
| **Gemini CLI** | `AfterAgent` → stop; `BeforeAgent` → start | `Notification` (no matcher) → notification | `prompt_response` | Not installed here, not run. Note: unfiltered `Notification` may also fire on idle notices → possible false "needs you" [I]. |
| **Cursor** | `stop` (`status`); `beforeSubmitPrompt` → start | **None** installed (no permission event in the table). | `afterAgentResponse` message event | Not run. |
| **Aider** | `notifications-command` → stop (fires when Aider waits for input) | **Cannot distinguish**: Aider's notification means "ready for input", which is both "done" and "asks you" [I]. | last reply in `.aider.chat.history.md` of posted cwd | Not run. |
| **Windsurf** | `post_cascade_response` → stop; `pre_user_prompt` → start | **None**. | `tool_info.response` | Not run (docs only). Hooks do not run in Restricted Mode. |
| **Kilo Code** | plugin: `session.idle` → stop | `permission.asked` → notification | via plugin | Not run (docs only). |
| **Copilot CLI** | `agentStop` → stop; `userPromptSubmitted` → start | `notification` → notification | **none** (`transcriptPath` format not read) → always Unsure | Not run (docs only). |
| **Devin CLI** | via Claude's hook, relabelled when payload has `prompt_id` and no `transcript_path` | via Claude's `Notification` | `last_assistant_message` | Not run; relabel rule from docs only. |
| **Trae** | **No hook**: observe-only (process idle) → at best "may be done, I have no hook" | **None** | none | n/a |

Summary: **can signal needs-input today**: Claude, Devin, Gemini, OpenCode, Kilo, Copilot (6, incl. Devin). **Cannot**: Codex, Antigravity, Cursor, Windsurf, Aider (ambiguous), Trae.

Best fallbacks for the "cannot" group, in order of reliability:
1. **Vendor hooks we have not wired** [I, check current vendor docs]: Cursor has `beforeShellExecution` / `beforeMCPExecution` hooks whose payload could reveal an approval gate; Windsurf has `pre_run_command` / `pre_mcp_tool_use`; Antigravity's hook set may include a tool/approval event. A "pre-tool" event followed by no "post" event for N seconds is a strong "waiting for approval" signal.
2. **Window/terminal text** read through UI Automation (Windows Terminal exposes a `TextPattern`): the existing `classify_window_text` already recognises `(y/n)`, "do you want to proceed", "press enter" (`session_verdict.rs:245`, `session_observer.rs:200-203`), but `window_text` is always `None` in the runner (`watch_runner.rs:486`). Wiring a reader would cover Codex/Aider/Trae terminals.
3. **Stall heuristic**: a started turn (start event) with no stop, low CPU and no file writes for ≥ 3 min = "may be waiting for you" (phrase it as a guess).
4. **Codex specifically**: run Codex through the existing `agent_watch` sidecar (`coding-agent-adapters` has a `blocking_prompt` state, `agent_watch.rs` already maps `blocked`), or parse its session log under `~/.codex/sessions` [I].

---

## 3. Sleep and wake on this laptop (Windows 11, Modern Standby)

`powercfg /a` on this machine: **Standby (S0 Low Power Idle) Network Connected** and **Hibernate** available; S1/S2/S3 and hybrid sleep unavailable.

### 3.1 Keeping it awake
- NEXUS asserts `ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED` [V `keep_awake.rs:231`]. On Modern Standby the machine enters standby when the **display** turns off; a system-required request alone does not stop that [I: Microsoft Modern Standby docs]. So keeping the display on is what actually keeps the agents running. Consequence: the screen stays lit for the whole run (battery, privacy: anyone can read the agent terminals).
- **User-initiated** sleep (lid close, power button, Start → Sleep) overrides every request [I]. Agents and NEXUS are then suspended by the Desktop Activity Moderator; nothing fires until resume [I].
- Critical battery action (hibernate/shut down) overrides too; NEXUS has **no battery check** in this path [V: no battery read in `watch_runner.rs`].

### 3.2 Putting it to sleep
- `SetSuspendState(FALSE, FALSE, FALSE)` [V `power_cmd.rs:684-690`]. On an S0ix machine with hibernate enabled this call is documented/reported to behave inconsistently (may hibernate, may fail) [I]. **Must be tested live.** A more native S0ix route is turning the display off (`SC_MONITORPOWER 2`, already in `window_tile.rs:428`) after releasing every execution-state request, then letting the OS idle into standby; or `powercfg`-free `CallNtPowerInformation`/`PowerRequest` cleanup [I].
- Note the track-only path already turns the display off when idle [V `watch_runner.rs:889-897`]; on this laptop that probably **starts Modern Standby** a short while later [I]. So "track all my agents" may already behave like "then sleep" without saying so.

### 3.3 During sleep
- The watch loop's clock is `Instant` (QPC) [V `watch_runner.rs:554,571`]; hook timestamps are wall-clock `SystemTime` [V `agent_hooks.rs:1440-1442`]; the proactive policy clock is `Instant` [V `proactive_policy.rs:411-413`]. Whether QPC advances across S0ix differs by platform [I]. If it does, a 3 h plan deadline can expire while asleep and the first thing said on wake is "I stopped waiting for X after 3 hours"; queued High alerts may expire to cards (10 min) during the sleep.
- Queued speech: alerts sit in `proactive_policy`'s in-memory queue; a High alert queued just before suspend can be spoken the instant the machine resumes — before the owner has unlocked, often at the lock screen [I], or expire as a card.
- Hook POSTs made while NEXUS is frozen or not running: `curl -m 3` fails silently, the event is gone [V script `agent_hooks.rs:246`; no retry].

### 3.4 Resume
- Detection by wall-clock jump [V `lib.rs:754-764`] works if the 10 s thread is frozen during standby [I].
- `on_resume` waits up to 3 min for input, then says only the hello [V `briefing.rs:571-600`]. **No agent results, no needs-input list.** The overnight record (`agent_outcomes`) is only read at app start [V `briefing.rs:633`]; after a *sleep* it waits until the next reboot/app restart, and goes stale after 36 h [V `agent_outcomes.rs:14,44-46`].
- The resume hello is skipped if a hello was spoken < 2 h earlier [V `briefing.rs:32,547-551`] and when the sleep was < 20 min [V `briefing.rs:567,573`].

---

## 4. Gaps and failure modes (specific)

### 4.1 Keep-awake can silently drop (bugs) [V]
- **Watchdog vs. slow tick.** The runner heartbeats once per tick (`watch_runner.rs:705`) after `look_at`, which can await the cloud model for up to 12 s (`watch_runner.rs:452-453`) plus the process sample. `WATCHDOG_MS = 10_000` (`keep_awake.rs:26`). A tick > 10 s force-clears **all** leases (`keep_awake.rs:154-160`) while `awake` stays `Some`, so the runner never re-acquires (`watch_runner.rs:702`). The laptop can then idle-sleep mid-run.
- **`agent_watch.rs` leases never heartbeat** (`agent_watch.rs:402,499`; no `keep_awake::heartbeat` anywhere in the file) → cleared ≈ 10 s after acquire, unless TTS happens to be beating. `wa_open.rs:428` documents the same failure for drills.
- **Refcount corruption.** Force-clear sets `count = 0` but leaves leases `active` (`keep_awake.rs:157`); when such a stale lease later drops, `release` decrements a count that now belongs to a newer lease and clears the assertion while that newer lease is live (`keep_awake.rs:120-133`).
- **`set_working`** acquires once and never heartbeats (`keep_awake.rs:203-223`), same 10 s clearance.

### 4.2 "Needs your input" is never said [V]
A waiting agent is "not ended" → stays in `working` → the engine only speaks progress for *finished* agents and a stall notice that needs the Stop to be the latest event (`watch_runner.rs:187-209`, `agent_hooks.rs:1455-1460`). Result: silence, screen kept on, until the 3 h default deadline ("I stopped waiting for Codex after 3 hours"). This is the owner's headline requirement.

### 4.3 Nothing is told on wake [V]
See 3.4. Also alerts spoken while the owner is away are spoken **into an empty room**: `proactive_policy` High alerts wait only for "nobody speaking, no drill" — no physical-presence gate (`proactive_policy.rs:436-449`; `idle_ms` is time since the last NEXUS interaction, not keyboard). After 10 min they become cards; nothing replays them on return.

### 4.4 Lost events and missed starts [V unless marked]
- NEXUS not running / restarting → hook POST lost; in-memory queue and plan lost (`agent_hooks.rs:1130`, `power_plan.rs:105`).
- A Stop that arrived **before** the plan was made is ignored (`stop_is_latest` requires `at_ms >= created_ms`, `agent_hooks.rs:1457`; `watch_runner.rs:487`). Saying "watch Claude, then sleep" after Claude already finished waits up to 3 h.
- `JoinMode::All`: an agent mid-turn that has posted nothing since the plan and uses ≤ 3% CPU (waiting on the LLM) is not joined → no lease → laptop may sleep under it (`watch_runner.rs:541-551,699-706`).
- Two sessions of the same agent share one slot [D doc 149]; the first Stop can make "Claude" look done while a second Claude session works.
- Codex tail discarded (section 2) → Codex can never be "finished clean"; with `require_clean` it blocks sleep, without it the wording says "may be done, but I could not read its final output".
- Copilot and Trae: no tail → always Unsure.

### 4.5 Wording [V]
- Per-agent lines: "Claude Code finished clean" (`agent_tracker.rs:98-104`), progress lines have no "sir". Owner wants "Claude has finished, sir" / "Codex needs your input, sir". No needs-input sentence exists at all.
- `agent_watch` path says "Your claude task just finished, sir." (adapter id, lower-case) (`agent_watch.rs:443`) — different words from the hook path for the same event.

### 4.6 Many agents at once [V]
- Finishes in the same tick are merged into one line (good). Finishes across ticks each produce a High line; with the summary and any follow-up/tidy lines the non-critical cap (6 per hour, `proactive_policy.rs:47,345-348`) turns the rest into **cards** ("rate_limited").
- The final summary repeats agents already announced (acceptable, but long).

### 4.7 Other cases
- **User present when an agent finishes**: spoken immediately at a breakpoint (fine); countdown aborts on any input (fine) [V].
- **User returns before all finish**: nothing changes; no "here's where things stand" on return [V absence].
- **Power button / lid during countdown**: OS wins; record already saved only if `Fire` ran [V `watch_runner.rs:936`].
- **Battery low**: unhandled [V absence]; Windows may hibernate mid-run.
- **Meeting**: countdown aborts (`watch_runner.rs:309-311`); High lines wait ≤ 30 min then card [V].
- **Plan is one-shot**: after the final summary the plan ends (`watch_runner.rs:226-228`); later turns of the same agents are not watched [V].
- **Fire records outcomes; track/notify plans do not** [V], so a track plan's results are not available on return either.

---

## 5. Ranked improvements to make it "perfect"

1. **Fix keep-awake reliability** (small, high value). Per-lease heartbeat (or a "held by a long-lived owner" lease that the watchdog never clears), re-acquire when the controller reports the lease was force-cleared, generation-tagged leases so a stale drop cannot clear a newer one, move the model call off the tick (spawned, cached). Add `agent_watch` heartbeats. *Live test*: run a watch, force a slow model call, check `powercfg /requests` every 5 s for 30 min.
2. **Speak "needs your input"** (core requirement). New tracker state `Waiting` (needs_user true or a waiting-question in a readable window), announced once per wait: "Codex needs your input, sir." Release the keep-awake lease (or start a "waiting" timer) after N minutes of waiting so a blocked agent cannot hold the screen on for 3 h; count a waiting agent as "not working" for the sleep decision only if the owner said so. *Test*: Claude permission prompt, OpenCode permission, Gemini notification.
3. **A "while you were away" ledger, told on return** (core requirement). Persist every finish / wait / failure event (sealed store, like `agent_outcomes`) when the owner is idle ≥ N min; on resume (and on first input after idle) speak a short ordered report: "While you were away, Claude Code finished, Antigravity stopped with 3 failing tests, and Codex needs your input, sir." Gate away-time speech on physical presence (`GetLastInputInfo`), not just breakpoints. Read the ledger in `on_resume`, not only in `start_boot_briefing`.
4. **Standing "away mode"** instead of a voice plan: when hooked agents are active and the owner has been idle ≥ X min (or locks the screen), auto-arm a track plan (with a setting `agentAwayMode: off|track|track_then_sleep`). Persist the plan across NEXUS restarts.
5. **Durable hook delivery**: the script appends the event to a spool file (`%APPDATA%\com.nexus.assistant\agent_events\*.jsonl`) when the POST fails; NEXUS replays it on start/resume. Persist the event queue.
6. **Fix the Codex tail**: pass `%3` (the JSON argv) as the body for Codex (temp file, since cmd quoting of JSON is fragile), or read the latest file in `~/.codex/sessions`. *Test*: a real Codex turn with `[WATCH-POWER] agent event: codex Stop ... text=NB` > 0.
7. **Sleep that works on S0ix**: test `SetSuspendState(0,0,0)` live; if it hibernates or fails, switch to "release requests + display off" and confirm standby via `powercfg /sleepstudy` / `powercfg /systempowerreport`. Speak which one happened in the ledger.
8. **Wording pass**: "<Short name> has finished, sir." / "<Short name> finished, but <reason>, sir." / "<Short name> needs your input, sir." Short names ("Claude", "Codex", "Copilot"). One wording module for both the hook path and `agent_watch`.
9. **Accept a Stop that predates the plan** (if it is the latest event and recent), and join agents in `All` mode from an open "start" event even with low CPU.
10. **Needs-input fallbacks** for Codex/Antigravity/Cursor/Windsurf/Aider/Trae: wire a UIA terminal/window text reader into `Inputs.window_text`; add pre-tool hooks where vendors offer them.
11. **Battery and lid awareness**: register for `PBT_APMSUSPEND` / `GUID_LIDSWITCH_STATE_CHANGE` / battery %; record "you closed the lid while Codex was working" in the ledger; warn at 15% on battery and offer to stop holding awake.
12. **Rate-limit exemption** for agent results (they are explicitly requested), or group them into one line per 60 s window.
13. **Dim instead of lit screen while away**: since the display must stay on for S0ix, lower brightness (WMI `WmiMonitorBrightnessMethods`) or show a black full-screen "NEXUS is watching 3 agents" stage overlay, and restore on input.

**Live test checklist (in order)**: (a) `powercfg /requests` shows NEXUS for a full 30-min run with model calls; (b) each agent's real Stop and Notification payload reaches `[WATCH-POWER] agent event` with non-empty text; (c) Codex permission prompt behaviour with each fallback; (d) the `on`-mode countdown into `SetSuspendState` on this laptop, then what state was entered (`powercfg /sleepstudy`); (e) lid close mid-run, then resume: what is spoken, when (before/after PIN); (f) three agents finishing within 30 s, plus one waiting; (g) NEXUS restarted mid-run: events/plan recovered.

---

## 6. Eight new related feature ideas

| # | Idea | Value | Risk |
|---|---|---|---|
| 1 | **Say repo and branch**: "Claude finished in zync on branch fix-login, sir." (from `last_cwd` + `git rev-parse`) | Owner runs agents in several repos; tells which one at once. | Low; git call must be bounded and run off the tick. |
| 2 | **One-line summary of what the agent did** from the final message (local extractive summary first, cloud only with consent) and `git diff --stat` ("changed 4 files, tests pass"). | Decide on return whether to look now. | Medium: the agent's text is untrusted (prompt injection into the summariser); keep masked, never act on it. |
| 3 | **Phone nudge**: push "Codex needs your input" to the paired phone via the Device Links Worker / SMS outbox already built. | Owner not at the desk can unblock an agent remotely. | Medium: message content leaves the device; needs opt-in and redaction of code text. |
| 4 | **Safe auto-approve policy**: per-agent allowlist ("read-only commands", "run tests") answered by NEXUS when away, via the agents' own permission hooks (Claude `PreToolUse` decision). | Overnight runs stop blocking on trivial prompts. | High: wrong approvals can damage the repo or leak secrets; must be deny-by-default, logged, and never for network/delete/push. |
| 5 | **"Where do things stand?" on demand**: a status answer listing each agent, state (working / waiting / finished / failed), repo, how long. | Owner asks once instead of checking 5 windows. | Low. |
| 6 | **Morning digest card** in the sidebar with each agent's outcome, diff stat, PR link (from `agent_followup`), and buttons "open window" / "open PR". | Visual record when speech was missed. | Low. |
| 7 | **Wake the laptop for a waiting agent**: schedule a wake timer (`SetWaitableTimer` with resume) if an agent was suspended mid-run by a lid close, or resume at a set time to continue work. | Long runs survive an accidental lid close. | Medium: unexpected wake in a bag (heat); require explicit opt-in and AC power. |
| 8 | **Cost and quota watch**: read agents' usage messages (rate limit, quota exhausted already matched as failure words) and tell "Claude hit its rate limit, it will resume at 3 pm". | Explains silence and avoids waiting for nothing. | Low-medium: parsing vendor text is brittle. |

---

## 7. What was verified vs inferred (quick index)

Verified in code: every file:line in sections 1, 4.1–4.7 and the hook tables in 2 (mapping, payload fields, missing notification events, Codex argv ignored). Verified on this machine: `powercfg /a` output. From docs (not re-run): live status column in section 2. Inferred: Modern Standby behaviour of `ES_SYSTEM_REQUIRED`, `SetSuspendState` on S0ix, QPC across standby, DAM suspension of agents, Codex `notify` not covering approvals, vendor hooks that could provide approval events, lock-screen timing of resumed speech.
