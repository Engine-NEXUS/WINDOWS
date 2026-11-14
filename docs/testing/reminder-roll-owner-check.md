# Reminder Roll: what only the owner can judge (plan phase R6)

Date: 2026-11-14. Plan: `docs/features/192-universal-reminder-animation-and-directions-plan.md`. A log cannot show whether an animation looks right or a voice sounds right, so these checks are yours. Each takes under a minute. Say the phrase, look and listen, then answer pass or fail with a few words.

Run NEXUS normally (not the hidden test mode). Countdown switch: Command Hub, display options, "Reminder countdown" (default on).

| # | Producer | Say or do | You should see | You should hear |
| --- | --- | --- | --- | --- |
| 1 | Voice reminder | "Nexus, remind me to call mom in 2 minutes" | Digits roll from now to the time, the line appears letter by letter in the black capsule, the clock vanishes in place (fade and blur, no slide), a chip counts down beside the orb | One short confirmation |
| 2 | Same, at fire time | wait 2 minutes | The alert chip shows the reminder | NEXUS says it |
| 3 | Timer | "Nexus, set a timer for 1 minute" | Same roll, chip counts down | Confirmation, then the alert at zero |
| 4 | Cancel | "Nexus, cancel the timer" | The chip disappears | Confirmation |
| 5 | List | "Nexus, what reminders do I have" | none | A spoken list |
| 6 | Switch off | Turn the countdown switch off, repeat 1 | The roll still plays; no chip stays | as 1 |
| 7 | Trip | "Nexus, remind me to leave for Charminar at <time>" | Roll, chip | Confirmation with the time |
| 8 | Timetable | A slot 25 minutes ahead | A quiet chip, no roll | Nothing until the start, then the usual question |
| 9 | Calendar | An event with an address within 90 minutes (needs Routes key, saved home, Google account) | A quiet chip, no roll | "Time to leave for ..." at the chip's time |

Also tell me:

* Does the chip overlap the background-task chip or the speech pill beside the orb?
* Should chips fade in place instead of dropping in from the top?
* Should chips stay hidden during a meeting or screen share?
* Does anything move, glow or show a colour other than black, white and grey?

Record your answers in `docs/testing/live-proof-ledger.md` (one line per number above).
