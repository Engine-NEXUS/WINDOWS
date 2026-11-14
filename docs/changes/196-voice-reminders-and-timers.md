# 196. Voice reminders and timers (plan phase R4)

Date: 2026-11-14. Plan: `docs/features/192-universal-reminder-animation-and-directions-plan.md`. Builds on `195-reminder-roll-universal-reminder-animation.md`.

## What it does

NEXUS now sets, lists and cancels spoken reminders and timers, and every one of them uses the universal reminder (`reminder.rs`): the clock rolls to the time, the countdown chip waits beside the orb, and the alert chip shows when it fires.

| You say | What happens |
| --- | --- |
| "remind me to call mom at 5 pm" / "at 5" / "at noon" / "tomorrow at 8 am" / "tonight at 9" | Reminder at the next such time. A bare hour takes whichever of am or pm comes next; "tomorrow at 5" is the afternoon, "tomorrow at 9" the morning. NEXUS says the time back ("tomorrow at 8:00 AM"). |
| "remind me in 45 seconds to check the oven", "in an hour and a half", "in half an hour" | Reminder after that wait (seconds up to a week). |
| "remind me at 5 pm to call mom", "set a reminder for 6:30 pm to start dinner" | Same, time first. |
| "remind me to call mom" (no time) | NEXUS asks when; nothing is set. |
| "set a timer for 10 minutes", "set a 25 minute timer for the pomodoro", "timer for 90 seconds" | Timer (up to 24 hours), optional label. "set a timer" alone asks how long. |
| "cancel the timer", "cancel all reminders", "cancel the reminder to call mom", "stop the pasta timer" | Cancels what is meant; asks which when several match. |
| "what reminders do I have", "how much time is left on the timer" | Reads up to three, soonest first. |

The task is said back to you ("call my mom" becomes "call your mom"). A reminder that came due while NEXUS was off is still said, marked as late.

## How it is built

* `src-tauri/src/voice_reminders.rs`: a **producer** only. It decides when and what to say; the display and the list are `reminder.rs`. The words and the time maths are pure functions (`parse`, `parse_duration`, `resolve_when`, `reminder_set_line`, `list_line`, `pick_to_cancel`, `take_due`) tested without a window or a clock.
* Items are kept in `voice_reminders.json` (atomic write, damaged file reads as empty). A one-second check takes what is due, calls `reminder::fired` and sends the line through `proactive_policy::submit` (High), so meeting mode, an open microphone and a pending question are respected. A reminder is never promised unless it was saved first.
* One new `ParsedIntent::VoiceReminders { cmd }`, claimed before the older executive-assistant parser and after the trip parser, so "remind me to leave for the airport at 5 pm" still belongs to directions. The older "in N minutes" intent now runs through the same module (the memory-lease path is no longer used for new reminders; leases already stored still fire).
* Cancelling by voice only touches voice reminders and timers. Trip reminders are not cancelled this way yet.

## Checked

* 19 module tests: lengths in every spoken form, clock times with am, pm, noon, 24-hour, "in the evening", tomorrow, tonight; "at" and "in" inside the task not cutting it wrongly; a missing time still parsing so NEXUS can ask; timers with labels; cancel and list forms; 17 ordinary sentences left alone ("cancel the order", "stop the music", "open the timer app"); next-occurrence rules (5 pm said at 10 am, 9 am said at 11 pm, a bare 5 said at 3 pm, 6 pm and 4 am, noon/midnight, the exact minute already past); event validity; late wording; list wording; cancel choice; a due item taken once; file round trip and damaged file.
* One pipeline test through `parse_deterministic`: the new phrases land on `VoiceReminders`, trips stay `TripReminder`, and four ordinary sentences are not taken.
* Full Rust lib 3,226 passed (the two TTS suites skipped as before); IPC, hygiene, turn-end and dead-code gates clean.

## Not done

* **Not run live.** Nothing was spoken, heard or seen: no rebuilt app, no real timer firing, no chip on the real stage, no speech-recognition run of these phrases. Speech recognition may hear "five PM" as "5 p.m." or "at five" differently; the parser takes both, but the real wording has not been collected.
* Trip reminders cannot be cancelled by voice; "what reminders do I have" does list them.
* No repeating reminders ("every day at 8"), no "snooze", no reminders by date or weekday ("on Friday"), no "in the morning" without a clock time.
* A tomorrow reminder shows its chip with the countdown only; the chip does not say "tomorrow".
* Calendar and timetable producers (R5) and the owner's look at the real overlay (R6) remain.
