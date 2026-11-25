# Proactive-speech policy — replayed timelines and mutation check (2026-11-22)

Source only, no NEXUS runtime. Code: `src-tauri/src/proactive_policy.rs` (`timeline_tests`). Scorecard row: Proactive-speech policy.

## What was run

The real `Engine` was driven on a 2-second tick through generated timelines: meetings (5-30 min), the user speaking (3-23 s), NEXUS speaking (3-28 s), a drill or request running (10-250 s), Focus Mode (2-12 min), user activity resets, and alerts arriving at about 5 per hour (Low 10 %, Medium 40 %, High 35 %, Critical 15 %; 30 % carry one of 4 event keys). After each simulated 2 hours the signals go quiet for 2 more hours so every alert must end. The timelines are generated from a fixed seed. They are **not recordings of a real person**, so they test the rules, not how the thresholds feel.

Checked on every spoken line: nothing but Critical is spoken in a meeting or over speech; Critical talks over speech only after its 8 s polite window and only lifts the meeting mute when the override is on; Medium needs 30 s idle and no focus/work; one line per tick; at least 20 s between non-critical lines; at most 6 non-critical lines per hour; Low is never spoken; every alert ends exactly once (spoken or card) and nothing is left pending.

## Result

`every_promise_holds_on_300_replayed_timelines` and `with_the_meeting_override_off_critical_waits_like_high` pass. Timing table (1,000 timelines = 2,000 simulated hours, 8,385 spoken lines, one seed, so the percentiles carry no confidence interval):

| Tier | Alerts | Spoken | Became a card | Wait before speaking p50 / p95 / max |
|---|---:|---:|---:|---|
| Low | 1,104 | 0 | 1,104 | n/a |
| Medium | 4,238 | 3,558 (84 %) | 680 (16 %) | 20 s / 15.5 min / 50.6 min |
| High | 3,752 | 3,218 (86 %) | 534 (14 %) | 0 s / 13.2 min / 30.1 min |
| Critical | 1,609 | 1,609 (100 %) | 0 | 0 s / 8 s / 8 s |

Reading: Critical is never delayed past its polite window; High and Medium long waits are meeting time (the engine holds them out of a meeting up to 30 min). Cards are alerts that expired or hit the hourly cap.

## Mutation check (`scripts/mutate-check.mjs`, spec `scripts/mutations-proactive.json`)

14 of 14 single-line mutations make the `proactive_policy::` tests fail: High over the user, High over NEXUS, High in a meeting, Critical without the polite wait, Critical that waits forever, Critical without the meeting override, Medium without the idle requirement, Medium during focus, hourly cap off by one, no minimum gap, several lines in one tick, Low speakable, expired alerts dropped silently, snooze ignored. Detail per mutation: `proactive-policy-mutations-20261122.json`. A first version of the simulation used per-tick probabilities that kept the user in a meeting about 90 % of the time; it was corrected before these numbers were taken.

## Not proven

Nothing was run live (a meeting, a real alert, Focus Mode in the app). Nothing in the app produces a Critical alert yet. Thresholds are still hypotheses. See `docs/testing/four-star-screen-tour-owner-script.md`, part C.
