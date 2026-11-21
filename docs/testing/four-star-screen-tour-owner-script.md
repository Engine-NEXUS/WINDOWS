# Owner script — screen tour, window theming, proactive speech (live checks)

Why this exists: the quiet hidden-desktop harness (`scripts/run-hidden-test.ps1`) cannot be used while your NEXUS is running. NEXUS is single-instance (a second launch hands over to the running one), and the script begins with `Get-Process nexus | Stop-Process -Force`, which would stop yours. So the live (L) axis for these three features is waiting for you. Everything below is read from a fresh release that contains commit "Screen tour / window theming / proactive policy four-star pass". Build it yourself (`nexus build`), then `node scripts/build-provenance.mjs verify`.

Save the console log of each part to `docs/testing/runs/<date>-<slug>.log` and tell the tracker session the result per step.

## A. Screen tour (10 steps, about 8 minutes, needs a Gemini key for steps 1-6)

Open a page with a short list (a news article or a recipe list) in your browser first.

1. Say "Nexus, analyse my screen." Expect: a short spoken acknowledgement, the orb hides, a spinner, then the orb returns and one ring/callout at a time, each with its own spoken line, no callout before its line starts. Note whether the ring lands on the thing being described (pass/fail).
2. Same on a YouTube video page. Expect: the spoken overview names the video's topic or title, never a person's face identification.
3. Say "Nexus, what am I looking at?" and, while it speaks, press Esc. Expect: speech stops, callouts clear, the orb closes, nothing is left on screen.
4. Say "Nexus, explain my screen" and then say "stop" in the middle. Same expectation.
5. Repeat step 1 at 125 % and 150 % Windows scaling (Settings, Display). Pass if rings still land on the right items.
6. Time step 1: from the end of your sentence to the first callout. Write the seconds down (the tour allows up to 25 s; after 6 s you should hear "Give me a minute, sir.").
7. Failure, no key: in Command Hub, clear the Gemini key, repeat step 1. Expect: "There is no Gemini key set, sir, so I can't point things out. Reading the visible text on your screen instead." and then the text answer. Put the key back.
8. Failure, offline: turn Wi-Fi off, repeat step 1. Expect: "I can't reach the vision service, sir. Are we offline? Reading the visible text on your screen instead." Turn Wi-Fi on.
9. Failure, sensitive window: open a password manager or a banking page, repeat step 1. Expect: "This window is sensitive, so I won't analyse it." and nothing is captured.
10. Failure, wrong key: type a wrong Gemini key, repeat step 1. Expect: "Gemini rejected the key, sir. Check it in Command Hub, Accounts. Reading the visible text on your screen instead." Restore the key.

A real 429 and 503 cannot be produced on demand; they are proven by the unit tests only.

## B. Window theming (6 steps, about 5 minutes)

For each window, check that you cannot see the desktop or another app through it, in dark and in light theme (Command Hub, Display, theme toggle):

1. Assistant sidebar (say "Nexus, what is a pull request?" so the sidebar opens).
2. Command Hub.
3. Settings window.
4. Setup window (`nexus setup` or the tray menu).
5. Architecture mapper ("open architecture mapper").
6. Reverse-engineer stack view and the PR list.

Pass per window: fully solid in both themes, no blur, no see-through edges. Note any window that shows the desktop behind it.

## C. Proactive speech (6 steps, about 10 minutes)

1. Say "Nexus, remind me in 1 minute to stretch". While you are talking to someone else (keep speaking), the reminder must wait until you stop.
2. Start a Teams/Meet call. Trigger the same reminder. Expect: silent card in the meeting, spoken only after the call ends (within 30 minutes), never during it.
3. With Hand Mode on (Focus Mode), a Medium alert (a non-urgent mail) must not be spoken while the pointer is in use.
4. Idle for 30 seconds, then a Medium alert: it is spoken once, then not repeated for 20 s.
5. "Not now" then a Medium alert: it stays a card for 15 minutes.
6. Critical alerts: nothing in the app produces one yet. Use `invoke("proactive_snooze")` only to check that a snooze does not change anything for Critical. (This is the open gap in the scorecard.)

Report pass/fail per step. Do not count a step you did not run.
