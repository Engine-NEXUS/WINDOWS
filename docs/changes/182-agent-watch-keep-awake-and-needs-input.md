# 182 - Agent watch: reliable keep-awake, "needs your input", and the away log (2026-11-01)

From the audit `docs/research/capabilities-audit/01-agent-watch-keep-awake-sleep-wake.md` (fixes 1, 2, 3, partly 5).

## Built
- **Keep-awake (`keep_awake.rs`)**: two kinds of hold. Short leases keep the heartbeat rule. New **pinned** holds (`acquire_pinned(max)`) need no heartbeat and end when dropped or after 8 h (`MAX_WATCH_HOLD`); `watch_runner.rs` and both `agent_watch.rs` watches use them. Before, one slow cloud check (up to 12 s) made the 10 s watchdog drop the screen lock and the runner never took it back. Loose leases carry an epoch, so a late drop of a lease that was already force-cleared can no longer release a newer live one (the count corruption in the audit).
- **`agent_attention.rs` (new)**: a wait for the user is told once ("Codex needs your input, sir" is stored as the away line), only when the user has been away 30 s and has not touched anything since the prompt arrived; reminders every 10 min, at most 3. Every agent finish while the user is away is also remembered (also with no plan watching, and the plan's richer line replaces it). When the user comes back to the keyboard it says once: "While you were away, sir: OpenCode needs your input. Claude Code finished clean." What needs the user comes first; one line per agent (latest state wins); sealed store key `agent_away_log`, 24 h.
- Tests: keep_awake 16 (4 new: pinned survives silence, pinned time limit, silent loose lease dropped but pin stays, stale drop cannot release a live lease), agent_attention 10, plus the existing watch_runner/agent_tracker/agent_hooks/agent_outcomes groups: 121 pass.

## Not done (still open from the audit)
- Codex's final message (hook script ignores the extra argument), hook events are still in memory only, sleep itself unverified on this laptop (Modern Standby), battery/lid checks, an automatic away mode, a cap on how long a *waiting* agent keeps the screen on (only the 8 h cap), needs-input for Codex/Antigravity/Cursor/Windsurf/Trae (no signal exists; a window-text fallback is possible).
- The spoken "needs input" happens when the user is back, not while away (deliberate: nobody hears an empty room).
- **Not run live**: nothing audible, no real agent prompt, no sleep/wake. Needs a rebuild.
