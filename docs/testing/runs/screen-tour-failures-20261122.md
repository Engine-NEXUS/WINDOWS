# Screen analysis / narrated tour — honest failures and mutation check (2026-11-22)

Source and offline tests only. No live run: see the blocker below.

## What changed

Before, any failure of the tour's vision call was spoken as one line ("trouble reaching the vision service"), whatever the cause. The model ladder (`vision::gemini_json_with_image`) now counts why each model failed and `ladder_outcome` names the cause only when every failure had it:

| Cause | Spoken (then it reads the visible text instead) |
|---|---|
| no Gemini key | "There is no Gemini key set, sir, so I can't point things out." |
| every model 429 (marks the day's quota) | "Today's Gemini allowance is used up, sir." |
| 401/403 on every model | "Gemini rejected the key, sir. Check it in Command Hub, Accounts." |
| 5xx on every model (503 overloaded) | "Gemini is overloaded right now, sir." |
| no model reachable (connect or timeout) | "I can't reach the vision service, sir. Are we offline?" |
| whole ladder over 25 s | "The vision service took too long, sir." |
| mixed or unusable answers | generic line |

A mix of failures never claims one cause and never burns the day's quota. The email-brief and timetable readers that share the ladder were updated for the new outcomes (they say rejected key, overloaded, offline). Already covered before this pass: sensitive window refused, foreground changed before capture, cancelled tour, meeting (sidebar only).

## Tests and mutations

- Rust: `screen_tour::` 56 tests (2 new: every failure has its own line and says what happens next, the failure constants), `vision::tour_ladder_tests` (new `ladder_outcome_names_the_real_reason`), `screen_context::` unchanged. Frontend: `net/screenTour.test.ts` 8 to 12 tests (second fetch supersedes the first hide, failed tour closes the orb, no close while speech plays, foreign fallback ignored); full vitest 1249 + 4 pass, `tsc --noEmit` clean.
- Mutation check (`scripts/mutations-screen-tour.json`, results `screen-tour-mutations-20261122.json` and `screen-tour-mutations-rerun-20261122.json`): **14 of 14 caught**. First run 11/14. Two survivors were a wrong test filter in my spec (the ladder tests were never run); the third, `expected.url == current.url`, is redundant with the URL fingerprint check (an equivalent mutant), so the spec now mutates the fingerprint comparison instead, which is caught. Covered: tour engine (clear before show, End once, stale/duplicate events), the failure lines (rejected key, 503), read/scan staying on free OCR, quota only on all-429, rejected key only when all 401/403, sensitive window refused, unverified region refused, and the re-check before capture (window title, region, page fingerprint).

## Not done, and why

- **Live run (L): blocked.** The owner's NEXUS (pid 12148) is running; the app is single-instance and `scripts/run-hidden-test.ps1` force-stops every `nexus` process first. Not attempted. Owner steps: `docs/testing/four-star-screen-tour-owner-script.md` part A.
- **Answer quality (I): not measured.** Needs real Gemini calls on saved screenshots, which need the owner's key (it lives in the keychain, not readable from here without using their credential outside the app). No numbers were estimated.
- A real 429 and a real 503 cannot be produced on demand; the tests cover the mapping, not the HTTP call itself.
