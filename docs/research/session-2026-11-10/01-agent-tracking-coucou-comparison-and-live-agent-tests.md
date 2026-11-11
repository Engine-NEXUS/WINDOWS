# Agent tracking: comparison with coucou, and what the live agent tests showed (2026-11-10)

Status: research record. Measurements come from the owner's laptop on 2026-11-10/11 (build ids below). Nothing here is a
claim of production readiness; the last section lists what is still unproven.

## 1. The question

The owner asked why NEXUS's tracking and control of coding agents has issues when the open-source project it was ported
from (`Louis-CFM/coucou`) "can track and control all of it perfectly". The answer has two parts: what NEXUS took from
coucou, and what was actually observed when the four agents were run.

## 2. What coucou is (from its public page, read 2026-11-10)

- A notch / top-of-screen island for AI coding agents (Swift on macOS; Tauri 2 on Windows and Linux).
- Supported agents: Claude Code, Codex, Cursor, Gemini CLI, Antigravity, Copilot CLI, OpenCode, Amp, Hermes, Claude
  Desktop, others via a generic `--agent` flag or a `coucou_agent` tag in the hook payload.
- Detection is per agent, never one mechanism:
  - Claude Code: hooks in its settings; a small relay script forwards events to the app over a local socket; approvals
    wait for the click.
  - Gemini CLI and Antigravity: hooks written into their config files.
  - Codex: a `--agent codex` flag or a hook installer.
  - Cursor: hooks installed next to Claude Code's.
  - OpenCode, Amp, Hermes: plugin files placed in each tool's config folder.
  - If the app is not running the hook exits at once so the agent is never blocked.
- Control: Allow / Deny (Claude also "Always"), multiple-choice questions with a "reply in terminal" fallback, live diffs,
  plan-usage limits, phone companions.
- Licence: code MIT; the name, the Mochi character, icon, sounds and media are reserved (`LICENSE-ASSETS.md`).

## 3. What NEXUS took, and what it did not

From `THIRD_PARTY_NOTICES.md` (row "Agent Island"): the permission-reply contract (`agent_approval_reply.rs`), the diff
engine (`frontend/src/agent-island/diff.ts`), approval timings and the island placement/cursor test. Ported under the
MIT notice with tests.

Not copied and not to be copied: names, the mascot, icons, sounds, `docs/media/`, `design/`. NEXUS's transport is its own
token-authenticated webhook (port 39220) rather than coucou's named pipe, and its hook installers, monitor reducer
(`agent_monitor.rs`), watch/attention logic and sidecar are NEXUS code.

So NEXUS is a partial port plus its own larger system, not a copy. "Same code, therefore same reliability" does not hold.

## 4. Live tests on the owner's laptop

Build under test: merged main (`6820cff`, release exe from `D:\ULTRON-pr\target`), webhook answering on 39220.

### 4.1 First attempt: headless runs from a helper session

Result: no agent produced a file, so NEXUS had nothing real to observe. Causes were account and configuration, not
tracking:

| Agent | Failure |
|---|---|
| Claude Code (`claude -p`) | "Invalid API key"; configured model `cf/@cf/zai-org/glm-5.2` not recognised |
| Codex (`codex exec`) | configured model `gpt-6-sol` and every override tried (`gpt-5.5`, `gpt-5-codex`, ...) rejected for a ChatGPT account |
| OpenCode (`opencode run`) | "invalid x-api-key" for the configured model |
| Antigravity (`agy --print`) | headless mode auto-denied the write/command permission |

### 4.2 Second attempt: the owner-approved route, same build family

- Antigravity with `--mode accept-edits` created its file. With a task that needs shell commands it needs
  `--dangerously-skip-permissions` (the owner had approved that flag for `agy` earlier; the folder was a scratch
  folder). Both runs sent events to NEXUS: a **Start before every model call** (11 Starts for one task) and a **Stop**
  with `fully_idle=Some(true)` and the final text (82 B and 1,279 B).
- OpenCode with the free model `opencode/glm-5.3-flash-free`, and with `opencode/deepseek-v4.1-flash-free`: the run
  hung for the full timeout. NEXUS received the **Start** events (the plugin works) but no Stop, because the model never
  finished. A model/provider problem, not a tracking problem.
- Codex: `HasOwnNotify`. NEXUS logged `[AUTOHOOK] Codex not hooked up: HasOwnNotify`: the Codex app already owns the
  `notify` entry and NEXUS leaves foreign entries alone by design. No Codex run could start on this account.
- Claude Code: not run as a separate process; its events from the live session that was building this are in the log
  (island watching `claude:<session id>`, Stop events) and are what produced the repeated announcements studied in
  document 03.

### 4.3 Findings about the Start event

Antigravity reports Start per model invocation, not per user prompt. The monitor treats Start as "a new turn began".
Harmless for state display; harmful for anything that measures how long a turn lasted (see document 03).

## 5. Why tracking "has issues", ranked

1. **Account/config of the tools themselves** (document 4.1): headless runs fail before NEXUS can see anything.
2. **Per-agent event semantics differ** (Start per call for Antigravity, Stop after every answer for interactive Claude
   Code, Codex owns `notify`). Any rule that assumes one shape misfires for another.
3. **Proof gap**: only Antigravity has been seen end to end on a real run. OpenCode start only. Codex not at all.
   Claude Code only as the session being worked in.
4. **Announcement policy** (what to say, when) was coded before the owner's wording was agreed. Fixed in part by the
   changes in documents 03 and 05.

## 6. Not proven (open)

- A real 45 s+ task being announced exactly once on a real agent.
- Codex start/stop on a working account (needs a model that account accepts, or the owner's own Codex run while NEXUS
  watches).
- OpenCode stop (needs a working model).
- Approval round trip (Allow/Deny from the island clears the agent's own prompt) on real Claude/Codex sessions. The
  14-check script `scripts/test-agent-approval-e2e.ps1` exercises the real exe through the webhook with faked hook
  events, not a real agent.
- Claude Code approval clearing when answered in the agent's own terminal.

## 7. Next step

A guided live test, one agent at a time, with the owner at the keyboard and NEXUS running with a debug port and the
log open, each with a pass rule agreed before the run (template in `docs/testing/production-readiness-plan-2026-11-10.md`
section 5).
