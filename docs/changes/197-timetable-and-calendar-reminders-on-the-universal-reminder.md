# 197. Timetable and calendar reminders on the universal reminder (plan phase R5)

Date: 2026-11-14. Plan: `docs/features/192-universal-reminder-animation-and-directions-plan.md`. Builds on `195-...` (the animation) and `196-voice-reminders-and-timers.md` (the voice producer).

## What changed

Two more producers now send their reminders to the shared module, so a study slot and a calendar leave-by look and behave like a trip, a timer or a spoken reminder.

| Producer | Chip | When it fires |
| --- | --- | --- |
| Timetable (`memcore/scheduler.rs`), kind `study` | Appears 30 minutes before a slot starts, counting down to the start. A daily slot is never a chip all day. | At the start the existing reminder is spoken as before ("It's time for DSA. Shall I start?") and the alert chip shows with the line that was spoken. |
| Calendar leave-by (`memcore/leavewatch.rs`), kind `calendar` | Appears as soon as the trip time is known (event within 90 minutes) and counts down to the moment NEXUS will speak (5 minutes before the leave time). Moves when the traffic estimate moves it by a minute or more. | When `decide` speaks, the alert chip shows with that line. |

Neither producer rolls the clock: the roll is for something the user has just asked for. These are shown quietly (`reminder::track`), which publishes nothing when nothing changed, so the 30 s and 60 s ticks do not spam the stage.

A chip is removed when its reason goes: the slot fired or was deleted, the calendar event moved or was cancelled, or memory / the timetable switch / the leave-by switch was turned off (`reminder::cancel`).

## How it is built

* `reminder.rs` gained `track` (show a chip with no animation; move it only if its time moved by a minute or more, `moved_enough`) and `fire_now` (show the alert chip for something that may never have had a chip, then drop it). Both are small and use the same list as everything else.
* `scheduler.rs`: pure `slot_event` and `lookahead_events` (today's slots starting within `LOOKAHEAD_MIN`, not yet fired). Speech and the "Shall I start?" offer are unchanged.
* `leavewatch.rs`: pure `leave_chip` (alert moment, believable trip time only). `decide`, the lookups, the cost limits and the speech are unchanged.
* Producers keep their own decision logic; only the display moved to the shared module.

## Checked

* New tests: a slot gets a chip only in the half hour before it starts (31 minutes too early, the start minute left to the reminder itself, another weekday, already fired), its fields and validity, long titles and ids cut and never refused; a calendar chip is at the moment NEXUS will speak, moves when the trip is longer, is absent when `decide` should speak, absent for a wrong geocode; the one-minute rule for moving a chip.
* Full Rust lib 3,231 passed (the two TTS suites skipped as before); IPC, hygiene, turn-end and dead-code gates clean.

## Not done

* **Not run live.** No rebuilt app, no real slot, no real calendar event, no look at the chips in the real overlay. The calendar path also needs a Routes key, a saved home and a connected Google account, none of which this test machine used.
* The chip for a calendar event appears only after its first route lookup (the existing cost limit); a few minutes can pass between "within 90 minutes" and the chip.
* A chip appears during a meeting. Speech still waits for the meeting to end (unchanged); whether the chip should stay hidden during a meeting or screen share is still the owner's decision (plan section 5).
* Trip reminders still cannot be cancelled by voice; the real-overlay proof (R6) remains.
