# 195 - Agent Island: instant hide/show, instant finish, readable plan (analysis and plan)

Date: 2026-11-22. Analysis only, nothing built, nothing run live. Code read: `agent_monitor.rs`, `agent_island.rs`, `agent_watching.rs`, `agent_attention.rs`, `agent_tracker.rs`, `agent_lifecycle.rs`, `agent_hooks.rs`, `frontend/src/agent-island/*`. Feature record: `docs/changes/186-agent-island.md`.

## What the owner asked for

1. A coding agent (Claude) finishes: the bar leaves at once, a one-row "Claude has finished" toast shows, NEXUS wakes and says "Claude finished, sir".
2. In the agent's own window the bar is gone; switching to another app or screen brings it back **immediately**.
3. When the agent needs input the bar shows it. Going straight to Claude must not leave stale cards. Answering in Claude must clear the card at once; answering on the bar must reach Claude at once.
4. The input card shows the plan as readable text, the way Claude shows `/plan`, not raw symbols.
5. Fast and light.

## What exists and works (read from code, with unit tests; not seen live)

- Hide while you are in the agent: `agent_watching.rs`, setting `agentIslandHideWatched` (default on). A session is "watched" when its window is foreground or at least half visible.
- Show/hide of the pill itself: the island state machine (`agent_island.rs`) runs every 33 ms while visible.
- Permission requests: held-open hook, 108 s, Allow/Deny and answer buttons for questions, card goes away after the answer.
- Finish: toast through `notify_hub::publish_agent`, speech through `proactive_policy` ("<Agent> has finished, sir.").

## Gaps found, with the cause in code

| # | What you asked | What happens now | Cause |
|---|---|---|---|
| G1 | Switching away shows the bar at once | Bar returns about 1.5 to 2 s later | `Debounce`: `WATCH_OFF_MS` 1,500 ms to show, `WATCH_ON_MS` 500 ms to hide; windows polled every 500 ms (`watch_at`), processes every 3 s. Worst case about 2 s to show, 1 s to hide |
| G2 | Opening Claude hides the bar at once | Bar leaves after 0.5 to 1 s | Same debounce and poll |
| G3 | Finish: bar leaves, one-row finished toast | Logo lingers 1.5 s, then leaves; nothing on the bar says "finished" | `DONE_LINGER_MS` 1,500 for a fade, no "finished" row state on the island |
| G4 | "Claude finished, sir" right away | Spoken about 15 to 20 s after the stop, and often not at all | Stop hook, then lifecycle `SETTLE_MS` 15 s, then tracker `ANNOUNCE_SETTLE_MS` 15 s, then a 3 s poll. Also `MIN_TASK_MS` 45 s: a turn shorter than 45 s is never told; `TELL_COOLDOWN_MS` 3 min between tellings; not spoken at all when you are looking at that agent; speech goes through `proactive_policy` High (waits for a gap in speech) |
| G5 | Input card clears when I answer in Claude | Stays until the turn ends or you send the next prompt | `answered_elsewhere` clears on PostToolUse, Stop, Prompt or End, but **PostToolUse is not installed** (`CLAUDE_EVENTS`; change 186 chose it to save a process spawn), and a later PreToolUse is not counted as proof |
| G6 | Going straight to Claude hides the input card | Card stays: approvals are not filtered by "watched" | `island_view` filters sessions only, `approvals` are copied as they are |
| G7 | Plan shown neatly | Raw markdown in a monospaced `<pre>` (`#`, `*`, backticks visible), cut at 12,000 chars | `AgentIslandApp.tsx` renders `ai-summary--plan` as plain text |
| G8 | Fast and light | Every 500 ms a full top-level window enumeration on a blocking thread, a process list every 3 s, 33 ms tick while visible | Polling instead of the foreground-change event |
| G9 | Is the terminal prompt shown at the same time as the bar? | **Unknown.** The doc says the permission schema was never seen on this Claude Code version | Needs one live check (owner script) |

## Plan, in order

**P1 - Instant watched/unwatched (G1, G2, G8).** Replace the 500 ms polling with a Win32 foreground event (`SetWinEventHook`, `EVENT_SYSTEM_FOREGROUND`, plus `EVENT_OBJECT_LOCATIONCHANGE` filtered to the agent's window for move/resize) that marks the monitor dirty. Evaluate watched-ness on the event, not on a timer. Debounce becomes a tiny flicker guard: hide on the next frame (0 ms), show after about 150 ms so an alt-tab pass-through does not blink (the test `debounce_hides_after_half_a_second_and_shows_after_a_second_and_a_half` is rewritten). Cache the agent's window handle (re-resolve only when the process list changes, every 3 s on a worker, never on the hot path). Target: hide under 100 ms, show under 250 ms, idle cost about zero between events.

**P2 - Finish is immediate and visible (G3, G4).**
- On a Stop that the lifecycle already accepts as final, set the session to `Finished` at once: the logo turns into a one-row "Claude has finished" chip for about 4 s (if you are looking elsewhere), then the pill leaves. If you are in that agent the chip is skipped.
- Speak at the stop, not after 30 s: keep the lifecycle's own check (background work, IDE agent not idle) but cut the second settle (`ANNOUNCE_SETTLE_MS` 15 s) for agents that report a definitive Stop (Claude Code's `Stop` hook). Remove the 3 s poll by announcing from the event.
- Short turns: tell any finish that you were waiting on (an island card or a bar chip was shown for it) regardless of `MIN_TASK_MS`; keep the 45 s / 3 min limits only for the optional extra voice line when you are present and looking elsewhere (owner decision, see Questions).
- Wording stays "Claude Code has finished, sir." (already the current line); the wake animation uses the existing proactive path.

**P3 - Input cards are always in step with Claude (G5, G6, G9).**
- Install `PostToolUse` for Claude Code (one extra small process per tool call; measure it) so an answer given in Claude clears the card within one event. Also count the next `PreToolUse` of the same session as proof the earlier request was answered.
- While the user is looking at that agent (watched), keep the card collapsed to a thin "needs input" mark rather than hiding it: it is the only cue if the terminal dialog is scrolled away. Owner choice if it should hide fully.
- Answering on the bar: confirm the round trip in the live check (click Allow, the agent continues within one second).
- G9 live check on the real Claude Code version: does the terminal show its own prompt while the hook waits (then both can answer) or only after the hook gives up (then the bar is the only place for up to 108 s)? The fix for the second case is a much shorter hold when you are at the PC and looking at the agent's window.

**P4 - Plan card that reads like `/plan` (G7).** Render the `ExitPlanMode` text with the sidebar's existing sanitised renderer (`frontend/src/sidebar/markdownRenderer.ts`, `marked` + DOMPurify, already used by the sidebar): headings, lists, bold, code blocks, tables. Fixed-height scroll area, plan title line, Approve / Keep planning buttons. Questions keep the current radio/checkbox layout. Test with a real plan fixture and a hostile one (script tags, huge input). Same renderer for long `Bash` commands (code block) instead of one long line.

**P5 - Measure.** A log line per hop (`[AGENT-LAT] stop_event -> island_hidden`, `focus_event -> island_shown`, `permission -> card_shown`, `click -> hook_reply`) and a test with an injected clock for each limit above. Report numbers, not claims.

## Not in scope here

Other agents' hooks (Codex, OpenCode) keep today's behaviour until Claude's path is proven. No change to the keep-awake rules.

## Questions that change the build

1. Should the spoken "finished" be said even for a very short turn (under 45 s) when you were not looking at that agent? (Proposed: yes if the bar showed that agent; otherwise keep the limits.)
2. While you are inside Claude and it needs input: thin mark on the bar (proposed) or nothing?
3. How long should the "finished" chip stay: 4 s proposed.

## Risks

- A foreground hook callback must stay tiny (post a flag; no window enumeration inside it) or Windows drops the hook.
- Showing 150 ms after an alt-tab can blink on a fast double alt-tab; the guard time is a hypothesis to tune with the owner's hands.
- `PostToolUse` adds one process spawn per tool call; measured cost to be recorded before it stays on.
