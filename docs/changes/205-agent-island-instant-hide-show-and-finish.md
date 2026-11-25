# 205 - Agent Island: instant hide and show, the bar is the finish notice (plan 195, P1 and P2)

Date: 2026-11-22. Source and offline tests only: nothing was run live, the owner's NEXUS was not touched. Plan and analysis: `docs/features/195-agent-island-instant-and-readable-plan.md`.

## Owner rule added during the build
When the top bar shows that an agent finished, **no notification is shown at the side of the screen** for it. This holds for every coding agent the bar tracks (Claude Code, Codex, Antigravity, Devin, OpenCode and the rest that report through the hooks), and only for them: no other toast changed.

## What changed

**P1 - the bar follows your windows without a wait**
- `agent_watching.rs`: the guard before an agent counts as watched drops from 500 ms to 0 (the bar leaves the moment you open the agent's window); the guard before it returns drops from 1,500 ms to 100 ms (only a filter for a double foreground change).
- Windows tells NEXUS when the windows change: `SetWinEventHook` for foreground changes, minimise/restore and a top-level window move or resize (`start_window_events`, a thread of its own with a message loop; the callback only sets flags and wakes the loop). Child controls, the caret and the cursor are filtered out by `event_wakes` (pure, tested). With no agent running every event is ignored.
- `agent_monitor.rs` loop: it sleeps on a `Notify` instead of a fixed tick, so a hook event or window event is handled in about 12 ms (the 12 ms joins a burst into one pass). The windows are looked at again on a window event, while a change waits out its 100 ms guard (33 ms tick), and once a second as a safety net. Before: every 500 ms whatever happened.
- The process list (which window hosts which agent) is refreshed off the loop every 3 s; a session never seen before still waits for one current list, so the bar does not flash for an agent you are already inside.

**P2 - the finish is on the bar, and spoken at once**
- A `Stop` of a session that was running, for any coding agent, is a finish: Antigravity's only when it says it is fully idle (its Stop ends one invocation), and never when an agent says background work is still running (`finish_is_immediate`, pure).
- **The bar says it.** If you were not looking at that agent, its logo stays on the bar for 4 s (`FINISH_SHOW_MS`) with a check and "Claude Code finished" (or "<agent> failed" when the turn ended with an error, "2 agents finished" for several), then the bar leaves. If you were looking at it, the logo leaves at once (300 ms fade, `DONE_LINGER_MS`, was 1,500 ms). The pill is wider by 156 px while it carries the text (`FINISH_W`, same constant in Rust and the page).
- **No toast.** `finish_now` publishes no notification, only the spoken line "Claude Code has finished, sir." (or "... stopped with an error, sir.") about 700 ms after the Stop (`FINISH_GRACE_MS`, to see whether a queued message starts the next turn; a new prompt within that time cancels it). Before: 15 s lifecycle settle, 15 s tracker settle, a 3 s poll, nothing under 45 s or within 3 minutes of the last one, plus a side toast.
- Spoken only to a user at the PC who is not looking at that agent. Away: the away log and the welcome tell it, as before. Looking at it: no line and no toast (they saw it end); the settled path is told it was handled (`handled`), so it adds no toast either.
- The settled path (`agent_attention` loop) and the lifecycle receipt (`agent_lifecycle_runtime::deliver`) skip a finish the immediate path handled (`fast_told_near`, within 5 s): one finish, told once.
- Not changed: "needs your input" toasts, finishes of NEXUS-owned watches (no bar), and every non-agent notification.
- Log line per spoken finish: `[AGENT-LAT] <agent> stop -> told after N ms`.

## Tests and mutation check
- New unit tests: `agent_watching` (bar leaves at 0 ms, returns within 150 ms, pending flag, which window events wake the monitor), `agent_attention::finish_now_tests` (away, watching, repeated Stop, no minimum length, no cooldown, grace under a second, the failed line, "already told" lookup, and a source scan that `finish_now` never calls `publish_agent`), `agent_monitor::finish_tests` (a Stop of a running session ends it for every agent, Antigravity must say idle, background work blocks it, the notice stays 4 s then leaves, failed drawing, a running session never shows failed, only an agent you were not looking at keeps its logo, a Stop followed by a new prompt is not a finish), `agent_island` (pill width with the text), page `finishedText.test.ts` (names, failed, several agents, nothing while any agent runs, width).
- Mutation check: `scripts/mutations-agent-island.json`, result `docs/testing/runs/agent-island-mutations-20261122.json`.
- Gates: lib tests for the touched modules 100 passed; frontend tsc, vitest 1258, build; opacity, turn-end, hygiene, IPC, dead-code (139 known, none new), hard-coded colours.

## Found on the way
- A static holding `tauri::AppHandle` that `ingest` called made the whole lib test binary import `TaskDialogIndirect` (comctl32 v6) and fail to start (`STATUS_ENTRYPOINT_NOT_FOUND`): test executables carry no manifest. The finish is started through a closure installed by `start()` (`ON_FINISH`), which tests never reach. Keep window-system code out of anything `ingest` can call directly.

## Not done / not proven
- Nothing was seen on a real screen: how the pill looks with the text (156 px, 12 px font, the check badge), the 4 s hold, hide/show speed, `SetWinEventHook` with several monitors and 125/150 % scaling. The 1 s safety poll covers a hook that fails to install (one log line).
- Which agents really send a Stop that reaches the bar was not tried: only Claude Code's hook was ever seen live. Codex, OpenCode, Devin, Antigravity and the others depend on their hooks being installed ("hook up all agents") and on what their Stop payload says.
- The loop's wiring (events, the first-look wait, guard-time ticks) has no unit test; the pure rules inside it do. The two dedupe lines in `agent_attention` and `agent_lifecycle_runtime` have no test of their own.
- P3 (stale input card after answering in the agent, `PostToolUse`) and P4 (the plan card rendered as markdown) are not built. No hook re-install is needed for P1 or P2.
