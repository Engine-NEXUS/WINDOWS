# 204 — "Clock or Block? Say one or two" (plan 06 step 4)

Owner: when a spoken app name sounds about equally like two different apps, NEXUS no longer guesses one and asks
"did you mean X?". It asks which, by number, and the answer teaches the match.

## What changed

- `sound_match.rs`: `PhoneticIndex::nearest_or_close` returns `Found::One(hit)` (a clear winner, same rule as before) or
  `Found::Close(a, b)` when the two best DIFFERENT apps are both within `MAX_DIST` (`PAIR_SLACK` is 0: see below).
  Names of one app never count as rivals (they share a group).
- `app_registry.rs`: `lookup_phonetic_pair`, new `OpenAssessment::Pick { entries }`. Used in two places only: when the
  tiered lookup found nothing and two apps sound alike, and when the tiered lookup made a text-level guess
  (Contains / Fuzzy, e.g. "lock" is inside "clock") that sounds just as much like another app. An installer is never a
  choice (the pair is dropped); one app under two names is not a pair; both choices must have matched on the app's whole
  name (`is_whole_name_key`: a version, a "(User)" tag or a Google/Microsoft/Windows prefix may be dropped, one word of a
  longer name may not). `NEXUS_NO_SOUND_MATCH=1` disables it.
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

## Checked against the owner's real app list (320 apps)

The first version, tuned on 5-app test lists, asked nonsense on the real list: "lock" -> Clock or Phone Link, "trey" ->
Trae or Google Drive, "photo" -> Google Photos or Grand Theft Auto V, "said" -> Zed or Windows App Cert Kit, "x" -> two
VS command prompts. Each bad second choice had matched one word of a longer name ("link", "drive", "auto", "cert",
"x86"), or sat at distance 0.39 inside the 0.06 slack. Two rules fixed it: whole-name keys only, and no slack. Over 69
probe phrases the questions left are Slack or Skype ("slap"), Windsurf or Winword ("windsor") and Kiro or Curl ("kilo");
every other phrase keeps the earlier behaviour (one "did you mean", an exact open, or nothing).

## Not verified

- A live spoken round trip (question spoken, mic window opens, "two" heard, app opens). The path reuses the same result
  message and offer window as the single question, but no one has said it to a running NEXUS.
- How often real speech produces a close pair. The probe phrases were typed, not heard; live `[SOUND-MATCH] ... both`
  lines will show it.
- Separate, pre-existing: `cargo test` writes to the real `%APPDATA%
exuspp_cache.json` and
  `app_resolution_cache.json` (it inserts "CustomTestApp" and touches timestamps), and both the personal and
  contributor builds share that folder.
