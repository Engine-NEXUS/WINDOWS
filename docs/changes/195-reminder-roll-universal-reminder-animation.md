# Reminder Roll: the universal reminder animation (preview)

Date: 2026-11-13 to 2026-11-14. Scope: a standalone, interactive **preview** of the animation NEXUS will show for every reminder. It is a design playground (`frontend/reminder-showcase.html`, served by `npm run dev` only, like `caption-showcase.html`). It is **not wired into the application yet**; the plan for that is `docs/features/192-universal-reminder-animation-and-directions-plan.md`.

## The owner's decisions, in order

1. The animation shows the current time as digits that move to the reminder time, then "Sir, I will remind you at <time>".
2. **It is universal.** It is not fixed to Maps or the leave-by feature: a trip, a plain reminder, a calendar event, a timer and a study slot all use it. (Saved as a durable note for future sessions.)
3. After the first preview: "it is perfect", with changes: **only black, white and grey**, minimalist, matching the orb; the countdown must be able to stay on screen, with a **user option** to show it or not; and after the time is set the clock must **disappear in the air, not slide and fade to the top**.

## What the preview shows

A stage standing in for the NEXUS overlay: the orb's black capsule at top centre, the reminder composition in the middle, countdown chips that dock beside the capsule.

Timeline for one reminder at Normal speed (all durations scale with the Speed control: Slow x2, Fast x0.5; reduced motion collapses them to about 10 ms):

| Step | What happens | Time |
| --- | --- | --- |
| 1 | The current time shows as four odometer columns (hour tens, hour units, minute tens, minute units) plus AM / PM; the kind and title chip fades in. | 0 to 0.2 s |
| 2 | The label changes from "Now" to "Reminder at". | 0.9 s |
| 3 | The digits roll like an odometer to the reminder time. Each column rolls forward through its digits (the hour-units and minute-tens columns make one extra full turn, the minute-units column two), staggered by 90 ms from left to right, 1.5 s each with a deceleration curve. A zero in the hour-tens column collapses to nothing so "5:00" is centred. | 0.9 s to about 2.7 s |
| 4 | An underline sweeps in under the clock. | at settle |
| 5 | The spoken line appears inside the orb's black caption capsule, letter by letter (26 ms per letter), while the capsule dot pulses faster, as the orb does when speaking. | after settle |
| 6 | "in 2 hours 14 minutes from now" appears under it. | after the line |
| 7 | After a 2.2 s hold the whole composition **vanishes in place**: opacity to 0, a blur growing to 18 px and a 5 percent grow over 1 s, ease-out, with no translation. | hold then 1 s |
| 8 | If the countdown is on, a chip drops in beside the orb capsule (the orb slider's 0.72 s `cubic-bezier(0.22, 1, 0.36, 1)` entrance) with a progress ring, the title and the time left (`2h 13m`, then `09:58` under an hour). | |
| 9 | At the time, the chip inverts to white on black with a slow pulse and shows the alert text, then clears after 4.5 s. | at fire |

## The generic event

Everything the animation reads is one payload, so a new kind of reminder needs no new animation code:

```json
{
  "event": "reminder:set",
  "id": "rem_mwfbnti8",
  "kind": "trip",
  "title": "Leave for the airport",
  "atMs": 1794603240000,
  "spoken": "Sir, I will remind you at 2:24 AM.",
  "alert": "Time to leave for the airport, sir.",
  "source": "directions",
  "display": { "countdown": true }
}
```

| Field | Meaning |
| --- | --- |
| `kind` | `trip`, `reminder`, `calendar`, `timer`, `study` (a label only; the animation is identical). |
| `title` | Shown in the kind chip and on the countdown chip. |
| `atMs` | The moment to fire (epoch milliseconds). The digits roll to its local clock time. |
| `spoken` | The line NEXUS says when the reminder is set. A timer says "Sir, timer set for 9 minutes." |
| `alert` | The line shown and spoken when it fires. |
| `source` | Which feature produced it (`directions`, `voice`, `calendar`, `timer`, `timetable`). Not used by the animation. |
| `display.countdown` | The producer's request for a chip. `false` asks for none. |

## The countdown option

A switch, "Keep the countdown on screen", remembered per device (`localStorage` key `nexus.reminder.countdown`; storage failures fall back to on).

* **On:** a chip stays beside the orb for every waiting reminder until it fires. Several reminders stack, soonest first.
* **Off:** nothing stays on screen. The reminder still fires on time and NEXUS still says it; the white alert chip still appears at fire time, then clears. The hint in the stage says "Countdown hidden. 2 reminders will still fire on time and NEXUS will say so."
* **The user's switch always wins.** A reminder can ask for no chip (`display.countdown: false`) but can never show one when the user has turned the countdown off (`wantsChip`).

## Matching the orb

The orb is pure white on an obsidian capsule. The preview uses exactly that vocabulary and nothing else:

* Palette: black, white and greys only. The page tokens are neutral in both themes (light: `#f2f2f2` ground, `#0a0a0a` text; dark: `#0a0a0a` ground, `#f2f2f2` text). The capsule is `#000` with a 10 px radius, a 1 px `rgba(255,255,255,0.16)` edge and a deep shadow in both themes, as on the real overlay.
* The spoken line and the chips use the capsule style; the capsule dot breathes slowly at rest and pulses while the line is spoken; no glow, no zoom.
* Easing and radius are the orb slider's. The orb's particle text was removed on 2026-10-06, so the animation lives in the stage overlay and does not need the orb shader.
* Accessibility: the clock carries an `aria-label`, the chips are in an `aria-live` region, the controls have visible focus, and `prefers-reduced-motion` skips the roll and the typing.

## Verification (2026-11-14, in the built-in browser pane against the dev server)

| Check | Method | Result |
| --- | --- | --- |
| Monochrome | A script read the computed `color`, `backgroundColor`, border and outline colours of every element and flagged any whose channels differ by more than 3 | none flagged, in dark and in light |
| Digit alignment when settled | Read each column's strip offset | each offset equals its digit index in em; exactly one digit fully visible per column; the clock reads the chosen time |
| Docking and chips | Counted and read the chip texts | one chip after the first reminder; three stacked soonest-first after three (`Tea 09:06`, `Leave for the airport 2h 13m`, `DSA practice 3h 04m`) |
| Switch | Toggled with chips present | off: no chips, hint text changes, setting stored as `off`; on: all chips return |
| Real firing with the countdown off | Set a reminder for the next minute with the switch off and waited | the alert chip appeared at the time, white on black (`rgb(255,255,255)` on `rgb(0,0,0)`), then cleared; the other reminder survived |
| Vanish in place | Sampled the composition's centre; set the transition to 24 s and took screenshots; read the end state | the centre never moved (x 504 throughout); the end state is `opacity 0`, `blur(18px)`, `scale(1.05)`, `translate 0 0`; a mid-vanish screenshot shows the digits as a soft blurred ghost in the same place |

Bugs found by that testing and fixed: a new reminder hid the previous reminder's chip permanently; the trip preset's alert read "Time to leave for leave for the airport"; a stray control character in a regular expression made the dev server return HTTP 500 (the pattern now needs no backslash).

**Limit of the check:** the preview pane repaints in bursts, so in-between frames cannot be sampled by timing. The vanish was verified from the end state, the unchanged centre and a slowed-down screenshot, not from a frame-by-frame capture of the real 1 s transition.

## How to open it

* Local network `http://192.168.0.119:5173/reminder-showcase.html`, or Tailscale `http://100.71.60.31:5173/reminder-showcase.html`, while `npm run dev -- --host` runs in `frontend/` (port 5173).
* A private hosted copy of the same page on claude.ai (version 3). A Vercel deployment was not possible: there is no Vercel login on the machine and credentials pasted into a chat are never used.

## Not done

* It is not in the application: no `reminder:set` event exists in Rust, no stage component, no Command Hub setting. See the plan.
* The chip's drop-in from the top is the orb's own entrance and was kept; whether the chip should instead fade in place is an open question for the owner.
* Not tested on a phone or on a second monitor.
