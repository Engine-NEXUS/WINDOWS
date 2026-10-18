# 139: Ask-first shutdown phrases and "watch this, then power off" (phase W0)

Date: 2026-10-18. Plan: `docs/features/142-watch-session-then-power-off-plan.md`, builds on `138`.

## What changed

1. **Bare power verbs ask first.** `power_cmd::parse_weak` ("shut down", "turn it off", "power off", "restart", "lock it", "sign me out", ...) is now used by `orchestrator::try_power_and_howto`: NEXUS asks "Did you want me to ...?" and registers `Offer::Power`. "restart nexus" / "shut down nexus" are not power asks (a vocative needs a comma).
2. **End-of-day phrases ask first.** `power_cmd::parse_casual`: "that's it for today", "that's all for tonight", "I'm done for the day", "I'm up for tonight", "calling it a night", "I'm going to bed", "time for bed", "let's wrap up", ... Whole-utterance only; "that's it for today's agenda", bare "that's it", "good night" are left alone. Question: "Calling it a day, sir? Shall I shut down your laptop?".
3. **Learning.** A yes teaches only a SHUTDOWN phrase. End-of-day phrases are stored under a `casual: ...` key (the strict cleaner would have cut "for today" and taught a bare "that's it"). Any answer other than yes forgets the pending phrase (`forget_pending_alias`).
4. **Answers to a power question.** `memcore::offer::classify_for` is now called for the waiting offer: besides yes/no it accepts the action itself ("shut it down", "power off", "good night") for that question only, and more ways to say yes ("absolutely", "sounds good", "why not"). Negations win. (This classifier existed uncommitted but was never called.)
5. **W0: `watch_power.rs`.** Pure, order-free parser for "watch / monitor / wait for THIS-or-AGENT until it is done, [if perfect], shut down my laptop". New `ParsedIntent::WatchThenPower { agent, kind, require_clean, deadline_mins }`, claimed in `parse_deterministic_inner` before the power / dashboard parsers (so before the compound "then" split). Reads options: agent name, "even if it fails" (require_clean false), "but not if it fails" (true), "within two hours" / "no more than 30 minutes" (deadline).
6. **The runner is honest.** `orchestrator::run_watch_then_power` only reads the plan back and says power-off after a watch is NOT armed and the laptop stays on. Phases W1-W5 are not built.

## Rules the parser enforces (all tested)

- Needs a watch verb or an end clause ("when it's done"), a thing that can finish (agent name, session noun, or this/it/that), and a power phrase.
- Media things are refused ("watch this video until it's done then shut down").
- First person is refused ("when I'm done with this session shut down").
- Questions and negations are refused ("how do I ...", "don't shut down ...").
- A bare "shut down" / "turn it off" counts only with a watch verb, an agent or a session word, only as a shutdown, and only when followed by a clause word or the end ("shut down notepad" never).
- Restart / sleep / lock / sign-out need an explicit "my laptop".

## Verification

- `watch_power` 6 tests (25 sentences, 1,000+ order-swap combinations, 50+ sentences that must be ignored); `power_cmd` 23+ tests incl. 150+ end-of-day phrases; `offer` tests.
- Mutation checks on the parser (5): media check, negation guard, first-person guard, weak-power context, weak next-word check. Two survived the first time (first-person, next-word); tests were added and both are now killed.
- Full lib run (final): 1811 passed, 3 failed (the PR-list tests below). The dead-code gate shows 4 new items (`center_for`, `tts::read_tts_emotion_setting`, `vision` fields `provider` / `quota_hit`), all from other sessions' in-progress refactors; none are in the power or watch files. The 4 `offer.rs` power items it flagged before are now used.
- NOT run: anything spoken, a rebuilt app, the ask-first round trip with a real microphone, a real shutdown.

## Known unrelated failures

3 `intent_parser` PR-list tests ("open pr list" now parses as `OpenGitHubDashboard`) and a flaky `auth_vault` keyring test under parallel runs. The dead-code gate also lists items from other in-progress work (`center_for`, `tts::read_tts_emotion_setting`, `vision` fields).
