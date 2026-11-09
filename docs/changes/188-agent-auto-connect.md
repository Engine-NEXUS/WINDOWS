# 188 - Agent auto-connect: find the coding agents on this laptop and hook them up

Date: 2026-11-08. Laptop feature (committed to the repository). Commit `1b7fe87`.

## Why
Agent Island (change 186) and the "watch this, then power off" features only see an agent after NEXUS has added its hook
to that agent's settings. Until now that needed the owner to say "hook up claude", "hook up opencode" and so on, once per
agent. A fresh install therefore showed an empty island. The owner's request: when NEXUS is installed on a laptop it
should scan the laptop and connect the agents by itself, with a switch in settings to turn that off.

## What it does
1. **25 seconds after start**, and then **every 30 minutes**, NEXUS looks for coding agents.
2. An agent counts as present when its **settings folder exists** (existing behaviour, `agents_present_in`) **or its CLI is
   on `PATH`** (new: `agents_on_path_in`). The PATH check matters on a fresh install, where the tool exists but has not
   created its settings folder yet.
3. For every present agent that has no NEXUS hook yet, NEXUS runs the **same `agent_hooks::install`** that the voice command
   "hook up ..." uses. That function makes a backup of the agent's settings first and only adds or removes NEXUS's own
   entries; a foreign hook is never touched.
4. If at least one agent was connected, the watch runner is started (`watch_runner::ensure_running`) and one spoken line is
   queued through `proactive_policy` at Medium urgency: "I found Antigravity on this laptop and connected it, sir.
   Restart it once so it starts reporting." (several agents: one line naming all of them).
5. The 30-minute rescan picks up an agent installed later in the day.

## Skipped on purpose
- Agents that are **observe-only** (Trae) have no hook mechanism to install.
- **Devin** is excluded: it reads Claude's settings, so it is covered by the Claude hook.
- An agent that **already has a NEXUS hook** is left alone, so the notice is only spoken when something new was added.
- Codex is reported as present, but its `notify` entry is only written when it has none of its own (`HasOwnNotify` is respected).

## The switch
- Setting `agentAutoHook` (default **on**), in Command Hub > Advanced > Agent Island: "Connect my coding agents
  automatically". The text says which agents are looked for and that a backup is made first.
- The setting is read from `settings.json` on every scan, so changing it takes effect at the next scan without a restart.
- Turning it off **does not remove** hooks that were already added. "Remove the Claude hook" (voice) still does that.
- `agentAutoHook` is a field of `NexusSettings` with a serde default of `true` and is in the settings round-trip test, so
  saving settings from the UI cannot drop it (the recurring failure mode noted in `AGENTS.md`).

## Files
| File | Change |
|---|---|
| `src-tauri/src/agent_autohook.rs` | New. `missing()` and `notice()` are pure; `enabled()`, `scan_and_hook()`, `start()` are the glue. Thread name `agent-autohook`. 4 unit tests. |
| `src-tauri/src/agent_hooks.rs` | `cli_names()` (claude, codex, agy, gemini, aider, opencode, kilo/kilocode, copilot), `agents_on_path_in()` (pure, tests a PATH string; matches `name`, `.exe`, `.cmd`, `.bat`, `.ps1`), `agents_present()` merges folder and PATH results in the fixed `ALL_AGENTS` order. |
| `src-tauri/src/commands.rs` | `agent_auto_hook: bool` in `NexusSettings` (default true); added to the settings round-trip test. |
| `src-tauri/src/lib.rs` | `pub(crate) mod agent_autohook;` and `agent_autohook::start(...)` in setup. |
| `frontend/src/settings-sidebar/AgentIslandSection.tsx` | The checkbox row and its default. |
| `AGENTS.md` | Project note. |

## Tests
- `agent_autohook::tests`: only present, hookable agents without a hook are listed; nothing to do when everything is hooked;
  the notice names each agent and uses singular/plural wording; a CLI on PATH counts even with no settings folder
  (temporary directory with fake `opencode.cmd` and `agy.exe`; Claude is not found).
- Settings round trip in `commands.rs` includes `agentAutoHook`.

## What was verified live (2026-11-08, rebuilt exe, PID 9396)
- After the first scan the Antigravity and OpenCode hooks appeared in their config files, with backups, and the spoken
  notice was queued.
- Start and stop events posted through the real generated hook script reached the monitor and produced the island pill and
  the toasts.

## Not verified (open)
- A real agent task that runs to completion: `agy --print` and `opencode run` hung when run non-interactively in this
  environment (sign-in or an old `opencode.db` with no session table is the likely cause), so no genuine finish event from
  those two was seen. Start/stop was simulated through the real hook script.
- Practice mode of the power plan (`watchPower = dry`) was not exercised. No shutdown was performed at any point.
- Screen-hold release after an agent stops was not observed.
- A true fresh install (no hooks, no settings folders) on another laptop.
- Claude Code was named in the spoken notice during testing but not hooked in that run, because a hook already existed.
