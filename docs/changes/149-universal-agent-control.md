# 149 — Universal agent control: track every coding agent, finish together, shut down, remember

Date: 2026-10-20. Plan: `C:\Users\Chitkul Lakshya\.claude\plans\gleaming-exploring-oasis.md`.

## What was asked
One command ("track all the agents running and update me when it is complete") that hooks every coding agent on the laptop, keeps the display awake while they work, announces each completion, then arranges their windows in equal columns and turns the display off. "Shut down after X" with no condition means done for today: power off whatever happened, remember the cause, and say it next morning with an offer to reopen the other apps. A commit / pull-request step must stop (and say why) when the result has errors, without affecting the shutdown.

## Built
- **Hooks (P1)** `agent_hooks.rs`: Codex, Devin (reads Claude's settings; relabelled by payload), Windsurf, Kilo (plugin), Copilot CLI; Trae is observe-only (no hook mechanism). "Hook up all agents" installs for every agent present, with approval, backup, only NEXUS entries touched. Per-agent event cap so a chatty agent cannot evict another's Stop. The folder an agent works in (`cwd`, or Antigravity's first workspace path) is remembered (`note_cwd` / `last_cwd`).
- **Tracker (P2)** `agent_tracker.rs` (pure): aggregate verdict over many agents, per-agent completion announcements merged to respect the proactive policy, joiner modes (single / named set / all), final summary. `watch_runner.rs` runs one track per agent and holds a keep-awake lease with a heartbeat.
- **Finish sequence (P3)** `window_tile.rs` + `Action::TidyUp`: after the summary is spoken, windows of the followed agents are put in equal columns (elevated and shared-terminal windows are left and said aloud), then the display is turned off only if the user has been idle and no meeting is on.
- **Shutdown rule + overnight record (P4)** `watch_power.rs`, `agent_outcomes.rs`, `memcore/briefing.rs`: a plain "shut down after X" is unconditional; "if it's perfect" stays conditional. Before power-off the per-agent outcome is saved (sealed); the next boot says what happened and asks "Do you want me to open the other apps you were using too?".
- **Commit / draft PR follow-up (P5)** `agent_followup.rs`: "… commit and raise a PR" sets `WatchPlan.followup`. NEXUS asks once ("Shall I, sir?", `Offer::PlanFollowUp`); only a yes (`power_plan::confirm_followup`) lets the runner start the step, once per agent per plan, only for an agent that finished and only on a clean verdict (the module's own gate: no secrets-like files, nothing-to-commit and in-progress git states refused, argument-array git, new branch, draft PR). The shutdown countdown waits while git runs (`Obs.followup_busy`). What the step said, or why it did not run ("commit stopped because errors were found"), is spoken and added to the overnight record. The commit result never decides the shutdown.

## Verify
- Rust unit tests for watch_power, power_plan, watch_runner (incl. new hold-while-committing and yes-gate tests), agent_hooks (incl. cwd), agent_followup, window_tile: 149 passed. Mutation checks on the follow-up wiring (see below).
- `cargo check --lib --features custom-protocol,admin-brain`: clean.
- Not mine: `intent_parser::tests::foreground_review_and_recreation_are_distinct_commands` fails ("analyze the current app"); it belongs to the concurrent screen-analysis work.

## Not run live
Codex / Devin / Windsurf / Kilo / Copilot hooks (read from docs), Devin relabelling, Codex trust step; tiling on real windows (maximised, minimised, shared terminal, multi-monitor); display-off on this Modern-Standby laptop; a real git commit / push / draft PR from a finished agent; the folder (`cwd`) each agent's Stop hook reports; the overnight briefing after a real shutdown; the `on` mode countdown after a tracked finish. Two sessions of the same agent share one slot.
