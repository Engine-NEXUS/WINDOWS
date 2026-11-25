# 206 - Agent Island: input cards stay in step with the agent, the plan reads like /plan (plan 195, P3 and P4)

Date: 2026-11-22. Source and offline tests only; nothing run live, the owner's NEXUS untouched. Follows change 205.

## P3 - a permission card never outlives its question

Three ways a card now leaves the bar, none of which needs the agent to say anything extra:

1. **You are in that agent's window.** `FocusHold` (`agent_watching.rs`): once the agent's own window has been the foreground window for 400 ms (`FOCUS_RELEASE_MS`; alt-tab passing through restarts the clock), every card for that session is handed back: the waiting hook is released with no answer, so the agent asks in its own terminal at once, right where you are, and the card leaves the bar. Safe whichever way the agent shows its own prompt (beside the hook or only after it): you are never left with a card and no way to answer. It only looks at the window in front (`focused_now`), not at one that is merely half visible, so a split screen keeps its cards. It runs on window events and when a new card appears, so it needs no polling.
2. **The agent hung up its hook.** `wait_for_alive` looks every 150 ms whether the hook's connection is still open (`peer_closed`: a peek that reads nothing; the socket goes back to blocking mode for the reply). An agent that is answered in its own terminal stops its hook, and the card goes at once instead of after the 108 s wait. Whether Claude Code really stops the hook this way has not been seen live.
3. **The agent went on.** The next tool call of the same session (`PreToolUse`) proves an earlier card was decided: a call's own request comes after its own PreToolUse, so nothing live is cleared. Together with the existing Stop, new prompt and PostToolUse rules.

Decision: `PostToolUse` is still **not** installed. It would add one more process spawn after every tool call of every Claude session; the three rules above cover the same case without it.

Answering on the bar was already immediate (the click fills the waiting slot and wakes the hook); not changed.

## P4 - the plan reads like /plan

`planMarkdown.ts` turns the plan's markdown (from `ExitPlanMode`, up to 12,000 characters) into plain data with `marked`'s lexer: headings, paragraphs, ordered and bullet lists (nested), task lists, bold, italics, strike-through, inline code, code blocks, quotes, tables, rules. `PlanView` builds React elements from that data: **no HTML string is ever made or inserted**, so raw HTML, scripts, links and images in a plan can only appear as their plain text (tests: `<script>`, `onerror`, `javascript:` links, 100,000-character and deeply nested input). Limits: nesting 6, 400 blocks, 20,000 characters. The card says "Plan" with **Approve plan** / **Keep planning** (the answers sent are the same allow / deny as before), the text scrolls inside the card (max 250 px, no scrollbar drawn), colours come from the stage tokens.

The dev preview (`/dev-agent-island.html`) gained a "Plan to approve" button and now holds a finished agent on the bar for 4 s like the real island.

## Tests and mutation check
- Rust: `agent_watching` (focused versus visible, the 400 ms hand-back, alt-tab restart), `agent_monitor` (next tool call clears a card, another session does not; a waiting request ends when the hook hangs up and a click still wins; a real loopback socket pair for `peer_closed` including the blocking mode after).
- Frontend: `planMarkdown.test.ts` (structure, no symbols in the words, inline styles, tables, tasks, hostile input, limits), `PlanView.test.tsx` (rendered markup has real headings, lists, bold, code, table; escapes everything).
- Mutations: `scripts/mutations-agent-island-p3.json`, result `docs/testing/runs/agent-island-mutations-p3-20261122.json`.
- Gates: frontend tsc, vitest 1269, build; opacity, turn-end, hygiene, IPC, dead-code (139 known, none new).

## Review pass (same day): what was looked at and what it found
- **Looked at in the browser pane** (`/dev-agent-island.html`, dark, size 75 %): the plan card (heading, ordered and nested lists, table, code block, quote, task list, scrolling inside the card, Approve plan / Keep planning), the question card, and the pill for one agent finishing (Codex finished, Antigravity finished) and for two (2 agents finished), each with the check badge. No console errors. Found and fixed: a task-list box sat on its own line above its text. A truncated "Codex finis..." in one screenshot was the 220 ms width transition, not a bug.
- **Real socket, real code:** the hook door is tested end to end: a card goes within 3 s when the hook is hung up, and a click still reaches the agent through the same request (`webhook::agent_event_http_tests`).
- **Real Windows:** Windows accepts the three window event hooks (test). The read-only live probe on the owner laptop finds the Claude window, its z-order and that it is watched. The probe also exposed a bug that was already there: a node dev server run from a `.claude` worktree (vite) was taken for Claude Code because the folder name contains "claude", which can map a session to the wrong window. Fixed: hidden folders in a script path are ignored (mutation-checked).
- Gates: Rust lib 3456 passed, frontend 1269, tsc, build, opacity, turn-end, hygiene, IPC, dead-code, work cards.

## Not done / not proven
- **Still never seen in the real app:** the pill and the plan card in the real transparent window, hide and show speed, the 4 s hold, several monitors and scaling, and the spoken line. The preview was dark theme only.
- Whether Claude Code stops the hook when you answer in its terminal (rule 2) and whether its terminal prompt appears beside the hook or after it: one real session decides this. Rule 1 is safe either way.
- A long `Bash` command is still shown in the monospaced box, not as a code block (not asked for).
- Other agents' permission hooks (Codex, Copilot, OpenCode) are not installed with these events; only Claude Code's hook reaches this code today.
- The loop wiring (when `FocusHold` runs) has no unit test, only its pure rules.
