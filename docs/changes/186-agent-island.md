# 186 - Agent Island: running coding agents at the top of the screen

Date: 2026-11-07. Laptop feature (committed to the repository; the home server reuses the same app).

## What it does
- A small pill at the top-centre of the primary display shows **only the logos of the coding agents that are running**, each with a state dot (working / waiting for you / done). It is hidden when nothing runs.
- **Hover** the pill (about 120 ms) and it opens into the island: a **Tracking** tab (per session: agent, folder, what it is doing, elapsed time) and an **Approvals** tab. Leave it (about 450 ms) and it folds back.
- A **permission request** opens the island by itself on the Approvals tab with **Allow / Deny** (and answer buttons for Claude Code's questions). After the answer it **folds back to the one-row pill**. A finished agent's logo leaves the row; the pill disappears with the last one.
- **Screen:** while any agent works, a pinned keep-awake lease holds the screen on. When all are done (10 s settle) it is released and the normal Windows sleep rules apply. Optional setting `agentMonitorDisplay = "blank"` also turns the display off, but only when you have been idle at least 60 s and no meeting is on (`window_tile::may_blank_display`).
- **You are using the PC** (other apps, a video): a finish or a long-waiting request tells you through the existing toast (`agent_attention` -> `notify_hub::publish_agent`).
- **You have left:** nothing is shown or spoken; finishes and unanswered requests go into the existing away log. On **return** the log is told: after `agentMonitorWelcomeAfterMin` (default **20**) minutes away it is a spoken summary (welcome back); after a shorter absence it is a quiet toast per agent with **no welcome**.
- The NEXUS orb is not part of this feature; the island has its own look (no mascot).
- **Tracking view (added after review of coucou's layout):** the expanded island has a left rail (agent logo with a state bubble, the folder as the session name, the agent, elapsed time, and the **tool steps** of the turn: Read, Edit, Bash, Done, each with a check or a spinner) and a right **code pane** with a file tab (language badge, file name, a live dot), the full path, and a **live diff** of the Edit / MultiEdit / Write being made: line numbers, removed lines struck through on red, added lines on green, syntax colours, a typing caret while the call runs. Several agents: a logo switcher above the rail. The diff text comes from the PreToolUse hook (so it shows as the call starts); the page runs the line diff (`frontend/src/agent-island/diff.ts`, ported from coucou's `core/diff.ts`, size guards: 200 KB / 4,000 lines on the page, 64 KB per call on the wire, over that only "too large").

## How
- `agent_monitor.rs` (new): hook events -> sessions (per `session_id`, so two Claude sessions stay apart), permission broker (hold-open wait, 108 s), power policy, 150 ms/1 s loop, commands `agent_monitor_list`, `agent_approval_decide`, event `agent_monitor:update`.
- `agent_island.rs` (new): hover state machine and the window (placed top-centre, shown without focus through `notify_toast::show_without_focus`, resized between pill and panel), command `agent_island_mode`, event `agent_island:mode`.
- `agent_approval_reply.rs` (new): the reply each agent expects on stdout. **Ported from coucou's MIT `reply.rs`** (see THIRD_PARTY_NOTICES.md). Rule kept and tested: nothing is allowed without a decision a human clicked; with none, the hook prints nothing and the agent asks in its own terminal.
- `webhook.rs`: one thread per connection (a held request must not stall other hooks; max 32), new `POST /agent-approval`, and `/agent-event` also feeds the monitor (events `pretool`, `end` are monitor-only). Same loopback bind and agent-event bearer token as before.
- `agent_hooks.rs`: Claude Code now also installs PreToolUse, SessionEnd and PermissionRequest hooks; the generated script has a `permission` branch that runs `curl -m 112` against `/agent-approval` and prints the reply. The hook timeout for it is 120 s. Existing installs are re-installed by "hook up claude" (`claude_has_hook` now requires the new events).
- `agent_attention.rs`: `take_entries`, `welcome_due`; the return summary is spoken only after a long enough absence.
- Frontend: `frontend/agent-island.html`, `src/agent-island/*` (pure model + tests), Command Hub card `AgentIslandSection.tsx` (Advanced page). Agent logos come from the existing `/activity-logos/*.svg`.
- Settings (`NexusSettings`): `agentMonitor`, `agentMonitorHover`, `agentMonitorDisplay`, `agentMonitorWelcomeAfterMin`.

## Deliberate differences from coucou
- Transport is NEXUS's token-authenticated loopback webhook, not a named pipe without a session token.
- No Mochi, no coucou name/artwork/sounds. Different layout and visual language.
- "Always allow" is not persisted (v1).
- PostToolUse is not installed (one process spawn less per tool call); the tool shown is the last PreToolUse.

## Verify
- Rust: 112 tests in the touched modules pass (reducer timelines, approval wait/timeout, power policy with a fake display, hover state machine, reply contract, 20-minute rule, settings round trip); full lib run: see the run record.
- Frontend: tsc clean, vitest 856, `npm run build` includes `dist/agent-island.html`; opacity, hard-coded-colour, IPC-contract, hygiene, turn-end and dead-code gates pass with nothing new.

## Not run live (open)
- A real Claude Code session with the new hooks (permission schema is documented, not yet seen on this machine's Claude Code version).
- Hover on real hardware, multi-monitor, 125/150 % scaling, fullscreen video underneath.
- Whether the display wakes on input after `blank`, and Modern Standby behaviour (change 148 left this open).
- Hooks of agents other than Claude Code (permission replies exist for Codex/Copilot/OpenCode in the reply contract, but only Claude Code's hook is installed with them).
- A lost event while NEXUS is down: the monitor shows only what it has seen since it started; a stale "running" session expires after 15 minutes.
