# 201: "did you mean Notepad?" for apps the recogniser heard by sound (Layer A, step 1)

Part of plan 09 (free-for-everyone understanding). First on-device piece: **no key, no network**.

**Problem.** A phrase heard as "open load pad" or "open spot if I" found nothing in the app registry (letter-based tiers cannot see that "load pad" sounds like "notepad"), so the launch path fell through to the legacy resolvers.

**What changed.**
- `sound_match.rs`: converts the heard words and every known app name to phonemes (CMU Pronouncing Dictionary, compact table built into the executable, a hand table for brand names, a letter fallback) and compares them with a phoneme edit distance in which sound-alike swaps are cheap. It returns a name only when it is clearly the best (distance at most 0.34 and at least 0.16 better than the nearest *different* app).
- `app_registry.rs`: a new `MatchKind::Phonetic` tier used **only** by the launch gate (`assess_open_target`) when the older tiers find nothing. A phonetic hit is always a guess: the user hears "I couldn't find 'load pad' exactly - did you mean Notepad?", and a yes is remembered by the existing confirm-and-learn path. It can never launch an app on its own. `NEXUS_NO_SOUND_MATCH=1` turns it off.

**Footprint.** About 0.5 MB of heap (a line-offset index built on first use); the 2 MB table is paged in from the executable. It deliberately does not use the offline voice's G2P (about +125 MB when loaded). Measured query time: about 1 ms over 100 apps in a debug build.

**Measured (unit tests, 101-app fixture).** 23 typical and real slips: 20 right, 0 wrong, 3 no suggestion ("slap", "zoom in", "word pad"); the 8 real slips from the owner's logs: 8/8. Unrelated dictionary words: 3.9% get a (wrong) suggestion (149 of 3,869), which costs one "did you mean?" question, never an action. Threshold trade-offs are printed by `cargo test --lib tune_grid -- --ignored --nocapture`.

**Limits.** The fixture is a typical app list, not the owner's registry; the slip set is small and was used to choose the thresholds, so the true rates are probably a little worse. Real audio was not involved. Verbs, modes and contacts (note 06's other slots) are not covered yet.

**Data licence.** CMU Pronouncing Dictionary, BSD-style; notice added to `THIRD_PARTY_NOTICES.md` and `src-tauri/src/sound_match_cmudict.LICENSE`. The Python wrapper package it was copied from is GPL and is not used; `scripts/gen_pron_lexicon.py` regenerates the table from the original data.
