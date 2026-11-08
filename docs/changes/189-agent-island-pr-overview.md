# 189 - Agent Island branch: overview of the pull request

Branch `feat/agent-island`. Laptop feature: everything here ships to every NEXUS install. Date: 2026-11-08.

This page is the entry point for the pull request. Each part has its own record with the details.

## The commits
| Commit | Subject | Record |
|---|---|---|
| `475aac8` | Agent Island: running agents at the top of the screen, live tracking, approvals | [186](186-agent-island.md) |
| `2d9d8d8` | Logos: fetch square marks through the logoapi.sh MCP, bundle offline with provenance and a gate | [187](187-logoapi-logos.md) |
| `1b7fe87` | Agent auto-connect: scan the laptop for coding agents and hook them up, with a Hub switch | [188](188-agent-auto-connect.md) |
| (this commit) | Documentation for the three changes above | 188, 189 |

The branch is cut from local `main` (`0b2e311`, "Document wake-word evidence and multi-user laptop qualification plan"),
which is one commit ahead of `origin/main`. That commit is documentation only and comes along with this PR unless it is
merged first.

## 1. Agent Island (186)
A pill at the top of the screen shows the logos of the coding agents that are running. Hover opens a tracking view (per
session: folder, steps, elapsed time, a live diff of the edit being made). A permission request opens Allow / Deny by
itself and folds back after the answer. The screen is held awake while any agent works and released when they are done.
Away/return rules decide between a toast, a silent log entry and a spoken welcome summary after 20+ minutes.

Technical shape:
- Hook events from the agents arrive at the existing loopback webhook (`127.0.0.1:39220`, bearer token), which now serves
  one thread per connection and has a new `POST /agent-approval` that holds the request open until a person clicks.
- `agent_monitor.rs` turns events into per-session state and runs the permission broker and the power policy.
- `agent_island.rs` owns the window and the hover state machine.
- `agent_approval_reply.rs` produces the reply each agent expects. Ported from coucou's MIT relay (notice in
  `THIRD_PARTY_NOTICES.md`). Safety rule, tested: nothing is allowed unless a human clicked it; with no answer the hook
  prints nothing and the agent asks in its own terminal.
- Claude Code gets three more hooks (PreToolUse, SessionEnd, PermissionRequest); other agents keep their existing hooks.

## 2. Logos (187)
21 square marks fetched through the logoapi.sh MCP and bundled in the app, with a provenance file
(`frontend/public/activity-logos/SOURCES.json`) and a CI gate (`scripts/check-activity-icons.mjs`) that rejects
missing provenance, changed hashes, external references and ids the UI asks for that have no file. Wordmark-only results
and wrong matches (Microsoft products, ChatGPT/Codex, Chrome) keep their old files, with reasons in the manifest. Nothing
fetches a logo at run time.

## 3. Auto-connect (188)
NEXUS scans for installed coding agents (settings folder or CLI on PATH) 25 s after start and every 30 minutes, and adds
its hook through the same installer as "hook up ...". Backup first, foreign hooks untouched, one spoken line. Switch:
`agentAutoHook` (default on) in Command Hub > Advanced > Agent Island. It never removes existing hooks.

## Settings added
`agentMonitor`, `agentMonitorHover`, `agentMonitorDisplay` (`stay-awake` | `blank`), `agentMonitorWelcomeAfterMin` (20),
`agentAutoHook` (true). All are in `NexusSettings` and in the settings round-trip test.

## Security and privacy notes
- Loopback only, bearer token on every hook request; the token lives in the keychain and in the generated hook script.
- Agent hook configs are backed up before any edit; NEXUS edits only its own entries.
- Diff text and tool names stay on the machine. Nothing is sent to a cloud service by these features.
- Permission replies are never "allow" without a click; a timeout or a closed NEXUS means the agent's own prompt appears.

## Test status
Recorded in each change record. Summary: Rust unit tests for the monitor, island, reply contract, hooks and auto-connect;
frontend vitest and tsc; opacity, colour, IPC-contract, hygiene, turn-end, dead-code and logo gates pass with nothing new.

## Not run live (read before merging)
- A real Claude Code permission round trip with the new hooks (hook up claude again to refresh the install).
- Hover, multi-monitor, 125/150 % scaling, display wake after the optional blank.
- A real Antigravity or OpenCode task to completion (both hung non-interactively on the test machine).
- Practice-mode shutdown plan and screen-hold release.
- A fresh install on a second laptop.
No real shutdown was performed during any test.

## Unrelated working-tree changes left out
`frontend/package-lock.json`, `server/worker/package-lock.json`, `scripts/probe_microphone.py`,
`src-tauri/src/directed.rs` and `wake_results.jsonl` were modified or untracked before this work and are not part of the PR.
