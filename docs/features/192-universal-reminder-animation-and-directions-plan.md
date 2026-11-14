# Plan: the universal reminder animation, wired into NEXUS, and the directions follow-up work

Date: 2026-11-14. Status: **directions rebuilt and proven on a machine run; the reminder animation exists only as a preview.** This plan is what remains. Records of what is already built: `docs/changes/193-...`, `194-...`, `195-...`. Evidence and open assumptions: `docs/research/directions-and-reminders/01-...`.

## 1. Goal

When NEXUS sets any reminder, the user sees one consistent, minimal, orb-matched animation: the time rolls from now to the reminder time, NEXUS says it, the clock vanishes in place, and (if the user wants) a small countdown chip stays beside the orb until the reminder fires. It must be **universal**: the same animation for a trip, a plain reminder, a calendar event, a timer and a study slot. It must never be coded inside `directions.rs` or any other producer.

## 2. Status

| Piece | State |
| --- | --- |
| Directions: parser, photo card, honest answers, follow-ups, trip reminders, route options | Built, tested (3,225 Rust tests), proven by a machine run on a hidden desktop in quiet mode |
| Test-quiet mode and hidden-desktop runner | Built, measured |
| Reminder Roll animation | Built as a browser preview (`frontend/reminder-showcase.html`); owner reviewed and approved the look; changes made after review (monochrome, countdown option, vanish in place) |
| `reminder.rs` (event, list, announce/retime/fired/cancel, `reminder_list`), stage `ReminderRoll` with model, slot and styles, `reminderCountdown` setting and Command Hub switch, directions trip reminders sending the event | **Built (R0 to R3), commit `83f9d06` and `4ae79e6`, 3,233 Rust and 1,154 frontend tests, one machine run on the stage; not yet looked at by the owner in the real overlay.** The spoken line is shown by the existing caption system, not by the component. |
| Voice reminders and timers (R4): `voice_reminders.rs`, "remind me to X at T / in D", "set a timer", "cancel the timer / reminder", "what reminders do I have" | **Built in source, 19 module tests and 1 pipeline test, full lib 3,226 passed; not run live, not heard.** Record: `docs/changes/196-voice-reminders-and-timers.md` |
| Timetable and calendar producers (R5), the real-overlay proof (R6) | **Not built** |
| Owner's own-voice and ears-and-eyes proofs for directions | Not done |

## 3. Architecture

```
 producers                         one event                    stage (frontend)
 ---------                         ---------                    ----------------
 directions::trip_reminder  ─┐
 memcore::leavewatch (calendar)─┤
 memcore::scheduler (timetable)─┼─►  reminder::set(payload)  ─►  emit "reminder:set"  ─►  ReminderRoll.tsx
 voice "remind me to X at T" ───┤       (one Rust module)          + keep list in Rust        (roll, speak line,
 timers                       ──┤                                  + fire at atMs             vanish, chips)
 study slots                  ─┘
                                          fire ─► proactive_policy::submit(alert)  +  emit "reminder:fire"
```

### 3.1 The contract (Rust: `src-tauri/src/reminder.rs`, new)

```text
ReminderEvent { id, kind, title, at_ms, spoken, alert, source, display: { countdown: bool } }
```

Serialised in camelCase exactly as in the preview payload (`atMs`, `display.countdown`). The module:

* `set(app, event)`: validates (non-empty title up to 60 characters, `at_ms` in the future, kind from a closed list), stores it, emits `reminder:set`, and returns the line to speak (`spoken`). Replacing an event with the same `id` is allowed (re-measured trips).
* `fire`: at `at_ms` calls `proactive_policy::submit(app, "reminder_<id>", alert, AlertUrgency::High)` and emits `reminder:fire` so the stage shows the alert chip. Speech goes through the existing policy (meeting mode, an open microphone, a question waiting) rather than a new speaking path.
* `cancel(id)` and `list()` for "cancel the reminder" and "what reminders do I have".
* Persistence: one JSON file in the application data folder (atomic write, same pattern as `trip_reminders.json`), reloaded at start so reminders survive a restart. Past-due reminders are dropped after 30 minutes.

### 3.2 The stage component (`frontend/src/stage/ReminderRoll.tsx` and `reminder.css`, new)

A port of the preview, not a rewrite:

* The odometer columns, the label swap, the letter-by-letter spoken line in the black caption capsule, the vanish in place (opacity, blur 18 px, scale 1.05, 1 s) and the countdown chips, with the same timings, scaled by the existing speech/animation speed conventions and collapsed under `prefers-reduced-motion`.
* **Placement.** The composition is centred under the orb; the chips dock to the right of the orb capsule using `stage:orb_rect` (as `ResponseCaption` and `SpeechPill` already do) and `layoutSlots.ts`. `BackgroundTaskChip` already occupies space beside the orb, so the first implementation task is to read how it is positioned and share its slot instead of overlapping it.
* **Click-through.** The stage window is click-through; chips register hit boxes only if they need to be clickable (they do not in the first version).
* **Speech sync.** The spoken line is shown by the existing caption pipeline (`tts:caption`) when NEXUS actually speaks, so the letters track the real voice instead of a fixed 26 ms timer. The preview's timer is the fallback when speech is muted.
* The component never reads `kind` for behaviour, only for its label.

### 3.3 The setting (the owner's option)

`reminderCountdown` in `NexusSettings` (`commands.rs`, default **true**), loose-parsed like the other settings so one bad field never resets the rest, with a Command Hub switch next to the other display options and a round-trip test (`settings must stay in NexusSettings` is a known pitfall: a field missing from the struct is dropped on save). The stage reads it at start and on a settings-changed event. Rules, as in the preview: the user's setting always wins; a reminder may ask for no chip (`display.countdown: false`) but can never show one when the user has turned it off; with the countdown off the alert chip still shows at fire time.

### 3.4 Producers (each a small change, in this order)

1. `directions::trip_reminder` (replace its private `trip_reminders.json` loop with `reminder::set` plus a re-measure hook; keep the sanity limit and the wording).
2. Voice: "remind me to <thing> at <time>", "remind me in 20 minutes to <thing>", "set a timer for 10 minutes", "cancel the reminder" (new parser entries, `kind: reminder` and `timer`).
3. `memcore::scheduler` timetable slot reminders (`kind: study`) and `memcore::leavewatch` calendar leave-by (`kind: calendar` / `trip`).

Each producer keeps its own decision logic; only the display and firing move to the shared module.

## 4. Phases and acceptance

| Phase | Work | Acceptance |
| --- | --- | --- |
| R0 | `reminder.rs` (event, store, fire, cancel, persistence), IPC `reminder_list` / `reminder_cancel`, registered commands, IPC contract gate | Unit tests: validation, persistence round trip, damaged file, past-due drop, firing exactly once, replace by id. No dead-code additions. |
| R1 | `ReminderRoll.tsx` and styles ported from the preview; listens to `reminder:set` and `reminder:fire` | Component tests: digit targets for every minute, 12 to 1 and 11 to 12 hour edges, midnight, blank hour-tens; the chip list sorts soonest first; vanish leaves the composition's position unchanged; reduced motion; the countdown switch matrix (user off / producer off / both on). `tsc` and `npm run build` clean. |
| R2 | `reminderCountdown` setting and Command Hub switch | Settings round-trip test; default true; a save without the field keeps it. |
| R3 | Producer 1 (directions) wired | The directions live run repeats: "remind me to leave for Charminar at <time>" shows the animation in a real stage window and the alert at the time, in quiet mode on the hidden desktop (the stage window can be read over the debug port). |
| R4 | Voice reminders and timers | Parser tests with guards ("remind me to call mom at 5 pm" is a reminder; "when should I leave my job" is not). |
| R5 | Timetable and calendar producers | Existing `scheduler` and `leavewatch` tests still pass; each now emits one `reminder:set`. |
| R6 | Proof | A ledger line per producer; the owner's eyes-and-ears check of the animation in the real overlay (it cannot be judged from a log). |

## 5. Risks and open questions

* **The chip's entrance.** The preview drops the chip in from the top, as the orb slider does. The owner has not said whether the chip should instead fade in place. Decide before R1 ships.
* **Overlap with `BackgroundTaskChip` and the speech pill** beside the orb; multiple reminders can stack several chips. Cap the visible chips (for example three plus "+2") and test with two monitors.
* **Ghost mode, meeting mode and Hand mode.** The stage is click-through and shared; the animation must not appear over a screen share (the stage is deliberately not capture-excluded, so a reminder during a meeting should be silent and text-only; confirm with the owner).
* **Time zones, daylight saving and sleep.** `at_ms` is absolute; a reminder due while the laptop slept fires at the next tick and says it is late rather than silently dropping.
* **Speech muted or microphone dead.** The reminder still fires through `proactive_policy`; the alert chip is the guaranteed channel.
* **Frame-by-frame verification.** The preview pane cannot sample in-between frames. R3 should record the real overlay (screen capture of the hidden desktop is not possible; use a normal run with the owner's agreement, or `PrintWindow` of the stage window).

## 6. Directions follow-ups that are not part of the animation

* A live run with a Maps key (steps, live traffic, a verified avoid) once the owner provides one through the Command Hub.
* A real photo for an ordinary business (the sources only cover landmarks and sites with a usable `og:image`).
* "Send it to my phone" (needs a phone link; not possible today).
* Ask "from where?" when the location is coarse, instead of only advising to turn on location.
* Speech-recognition proof of the new phrases with the owner's voice and the wake word.

## 7. What not to do

* Do not put animation or timing logic in a producer.
* Do not let a producer override the user's countdown switch.
* Do not add a colour: the animation is black, white and grey like the orb.
* Do not run reminder tests on the visible desktop, with sound, or with synthetic input (use `scripts/run-hidden-test.ps1` and `scripts/drive-nexus.mjs`).
