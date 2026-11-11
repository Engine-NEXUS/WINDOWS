# "Claude Code has finished, sir" spoken again and again (2026-11-10)

## 1. The report

The owner heard the same finish line three times in a row.

## 2. Evidence from `logs/nexus.log` (times UTC, the owner's laptop)

| Time | Request | Line |
|---|---|---|
| 17:07:56 | `agent_away_...` | "OpenCode has finished. Claude Code has finished, sir." |
| 17:19:28 | `agent_away_...` | "Claude Code has finished, sir." |
| 17:26:01 | `agent_away_...` | "Claude Code has finished, sir." |
| 17:29:19 | `agent_done_claude_...` | "Claude Code has finished, sir." |
| 17:39:09 | `agent_away_...` | "Claude Code has finished, sir." |
| 17:42:17 | `agent_away_...` | not spoken (rate limited) |
| 18:11:57 | `agent_away_...` | "Claude Code has finished, sir." |

## 3. Cause

`agent_attention.rs` treats **every** `Stop` event of an agent as "the agent finished". An interactive coding agent
sends a Stop after every answer it gives, including the Claude Code session being used to talk to NEXUS that day. Each
Stop produced either a spoken "has finished" (user at the PC: `agent_done_...`) or an entry in the away log that was
said on return (`agent_away_...`). There was no notion of "a task" versus "a chat reply", and no cooldown.

The earlier wording fix (plain line at the PC, "While you were away" only after sleep) changed *what* was said, not
*how often*.

## 4. The rule now (`finish_worth_telling`, `agent_attention.rs`)

A finish is told only when both hold:

- the turn that ended ran at least `MIN_TASK_MS = 45 s` (unknown start: treated as a task), and
- at least `TELL_COOLDOWN_MS = 180 s` have passed since the last finish told for that agent.

A suppressed finish prints `[AGENT] <agent> stop not told: turn <ms> ms (task needs 45000 ms), cooldown 180000 ms`, so
a silent finish is never a mystery. The rule gates both the spoken line and the away-log entry.

## 5. How a turn's length is measured (`turn_start`, `agent_hooks.rs`)

A turn begins at its **first Start after the previous Stop**. Taking the last Start (the first implementation) was wrong
for Antigravity, which reports a Start before every model call (11 for one task): every long task would have looked
short and never been announced. Test: events Start(100) Stop(200) Start(1000) Start(20000) Start(40000) Start(60000)
Stop(90000) give a turn start of 1000, not 60000.

## 6. Live evidence

Antigravity, 5.7 s task: Start, Stop (`fully_idle=Some(true)`), and `[AGENT] antigravity stop not told: turn
Some(5704) ms`. A 15-file task finished in 7.2 s and was likewise correctly not announced.

## 7. Not proven

A real task of 45 s or more being announced exactly once. The attempt to fake one with the real hook script
(`nexus-agent-event.cmd opencode start`, 52 s wait, `stop`) failed to run (shell exit code 4) and was not retried. The
unit test covers the rule; the live path for the told case is still unobserved.

## 8. Design notes and follow-ups

- 45 s and 3 min are judgement values, not measurements. If the owner wants to hear about every task including short
  ones, lower `MIN_TASK_MS`; if repeats return, raise the cooldown. They should become settings if the owner changes
  them more than once.
- The Island card keeps showing every state change; only speech and the away log are gated.
- An agent whose Stop carries an error or "needs input" is a different path (`Notification`, permission waits) and is
  not gated by this rule.
