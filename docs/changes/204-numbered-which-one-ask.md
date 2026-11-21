# 204 — "Clock or Block? Say one or two" (plan 06 step 4)

Owner: when a spoken app name sounds about equally like two different apps, NEXUS no longer guesses one and asks
"did you mean X?". It asks which, by number, and the answer teaches the match.

## What changed

- `sound_match.rs`: `PhoneticIndex::nearest_or_close` returns `Found::One(hit)` (a clear winner, same rule as before) or
  `Found::Close(a, b)` when the two best DIFFERENT apps are both within `MAX_DIST` (+ `PAIR_SLACK` 0.06 for the runner-up).
  Names of one app never count as rivals (they share a group).
- `app_registry.rs`: `lookup_phonetic_pair`, new `OpenAssessment::Pick { entries }`. Used in two places only: when the
  tiered lookup found nothing and two apps sound alike, and when the tiered lookup made a text-level guess
  (Contains / Fuzzy, e.g. "lock" is inside "clock") that sounds just as much like another app. An installer is never a
  choice (the pair is dropped); one app under two names is not a pair. `NEXUS_NO_SOUND_MATCH=1` disables it.
- `memcore/offer.rs`: `Offer::OpenAppPick { choices, spoken }`, `pick_question`, `parse_pick`. A pick is "one"/"1"/"first",
  "two"/"2"/"second" (also "won", "to", "too" when the reply is only filler around them) or a word that belongs to exactly
  one name ("the chat one"). A word both names share, or two names at once, is not a pick.
- `orchestrator.rs`: `pick_route` decides how a reply is handled: a pick, a "no" or "later" answers; a bare "yes" asks
  "Which one, sir?" again (never guesses); anything else drops the question and runs as a normal command. An uncertain
  voice can never answer it (`may_answer_offer`), same as the single yes/no launch question.
- The pick is the training signal: `confirm_resolution(spoken, chosen)` remembers it, exactly as a "yes" did for the
  single question. The two-sighting guard and phoneme-keyed learning are step 5, not done here.

## Verified

- New unit tests: sound pair / clear winner / far phrase / same-app names (`sound_match::close_tests`), pick parsing and
  question text (`offer`), pair lookup, installer and same-app exclusion, launch gate returns `Pick` for "lock" with
  Clock + Block installed and still opens "clock" at once (`app_registry`), reply routing (`orchestrator`).
- Existing `gate_asks_instead_of_launching_guesses` ("mummy chat" -> Google Chat, "chroem") still passes: single
  guesses keep the single question.

## Not verified

- A live spoken round trip (question spoken, mic window opens, "two" heard, app opens). The path reuses the same result
  message and offer window as the single question, but no one has said it to a running NEXUS.
- How often real app lists produce a close pair. The test lists show it for short words (lock, potion, time); a real
  list of 100+ apps will produce more near-ties than a 5-app test, so the first live logs (`[SOUND-MATCH] ... both`)
  decide whether `PAIR_SLACK` stays 0.06.
