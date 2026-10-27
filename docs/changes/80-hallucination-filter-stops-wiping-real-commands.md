# Change 80 — Hallucination filter stops wiping real commands (Phase B6)

**Defect (audit #11/#12, confirmed offline by change 79's harness):** with a perfect transcript, `list pull requests` and `order pizza from dominos` reached the parser as an empty string, so the user heard "didn't catch that". Cause was not the `you.`/`bye.` substrings first suspected but the Latin-gibberish dictionary gate (`<30%` of words in a 150-word list; "list/pull/requests" are not in it). The substring bug was also real: `"you."`, `"bye."`, `"thank you."` matched anywhere, so `Tell mom I love you.`, `Say goodbye.` and `Thank you. Open Chrome.` were wiped.

## Fix (`src-tauri/src/stt.rs`, `apply_hallucination_filter`)

- Short junk (`thank you.`, `you.`, `bye.`) moved from "contains" to whole-utterance matches (`bye` added to `exact_fragments`; punctuation already stripped there).
- The dictionary gate no longer fires when `intent_parser::parse_deterministic` resolves the transcript to an intent — the parser that will act on it is the arbiter. Real gibberish still has no parse and is still dropped.
- Dictionary gained ~30 generic command words (list, pull, request(s), merge, order, find, read, reply, scroll, click, copy, paste, …).
- New YouTube-outro phrases (multi-word, safe as substrings): thank you so much for watching, thanks/thank you for listening, see you in the next video, like and subscribe, don't forget to subscribe, amara.org.

## Tests (+3, `stt::tests`)

`test_real_commands_survive_the_filter` (12 phrases incl. trailing-period forms), `test_youtube_outro_hallucinations_filtered` (12), `test_gibberish_still_dropped_with_parser_exemption` (4). Rust lib 938/938 serial; `e2e_voice_fixture` 2/2; `stt_eval --text-only` on the 53-phrase set: both previously wiped phrases now resolve.

## Deliberate remaining behaviour

A lone `thank you` / `bye` is still dropped (the most common silence hallucination). Say "thanks" as part of a command and it passes.

## Measure it

Re-run `stt_eval` on your recorded set and `voice_scorecard.py --baseline baseline.json`: the rows "Real commands wiped by the hallucination filter" and "Hallucinated text on silence / TV noise" show the before/after.
