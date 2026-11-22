# Agent Island: live check for change 205 (about 10 minutes, after the next build)

Needs a build that contains change 205 and Claude Code hooked up ("hook up claude"). Look at the log for `agent island:` and `[AGENT-LAT]` lines.

1. **Bar leaves at once.** Start a long Claude task in a terminal, then click another app. The bar should show Claude's logo. Click the terminal: the bar should be gone in the same moment, not half a second later.
2. **Bar returns at once.** Click away from the terminal again: the logo should be back within about a tenth of a second. Alt-Tab to the terminal and straight back: it should not blink. Try a second monitor and a window dragged across screens.
3. **Finish shown on the bar.** With Claude working and you in another app, let it finish. The pill should keep the logo with a check and say "Claude Code finished" for about 4 seconds, then leave. NEXUS should say "Claude Code has finished, sir." right after. **No notification should appear at the side of the screen.** Time it by eye; the log line `[AGENT-LAT] claude stop -> told after N ms` should read close to 700 plus the time to speak.
4. **Finish while you are in Claude.** The bar stays away, no line is spoken, no side toast.
5. **Short turn.** Ask Claude a one-line question while you are in another app. It should still show and say the finish (before: nothing under 45 s).
6. **Failed turn.** Interrupt a task with Esc: if Claude reports an error the pill says "failed". Tell me what it said if it did not.
7. **Other agents.** Do the same with Codex, OpenCode, Devin, Antigravity if you use them. Write down which ones never showed a finish: that means their hook did not reach NEXUS.
8. **Away.** Leave the PC for a minute while an agent finishes: nothing should show or speak; on return the usual welcome tells it.

Write the result next to each number. Anything that blinks, lags or says it twice is the finding I need.
