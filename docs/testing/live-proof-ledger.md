# Live proof ledger

One line per real run of a feature (not unit tests). A scorecard row's **L** (live-verified) value may only be raised for a run that has a line here. See `docs/features/149-feature-production-programme.md`.

Proof kinds: **BOT** = run by the assistant on this laptop with a real process / OS call; **OWNER** = run by the owner (voice, ears, eyes, real accounts); **EXT** = needs outside hardware, software, or a service.

| Date | Commit | Feature | Proof | What was run | Result | Evidence |
|---|---|---|---|---|---|---|
| 2026-10-20 | working tree | Keep-awake | BOT | `-- --ignored live_lease_sets_and_clears_the_real_execution_state` (CallNtPowerInformation) | pass | `[PROOF] execution_state_while_held=0x00000003`, `[PROOF] execution_state_after_release=0x00000000` |
| 2026-10-20 | working tree | Window tiling | BOT | `-- --ignored live_three_own_windows_are_tiled_into_equal_columns` (3 real windows, geometry read back) | pass | `[PROOF] tiled hwnd=3543138 rect=(0,0,640,1080)`, `...rect=(640,0,1280,1080)`, `...rect=(1280,0,1920,1080)` |
| 2026-10-20 | working tree | Display off | BOT | `-- --ignored live_display_off_then_input_wakes_it` | display_off returned Ok, then a 1 px mouse move; the screen going dark was NOT observed by anyone | `[PROOF] display_off returned Ok` |
| 2026-10-20 | working tree | Mishearing normaliser | BOT | `-- --ignored live_replay_of_the_real_missed_intent_log` over the owner's real log | total=200 unique=194, 31 now understood; top still missed: "Open.", "Ssort.", "then the" | `[PROOF] total=200 unique=194 rescued=31` |
| 2026-10-20 | working tree | Gated commit + PR | BOT | 5 tests against real git and a local bare remote (branch, commit, push, ignored file left out, secrets refused, hold/unsure untouched, push failure keeps commit) | pass | `cargo test agent_followup::tests::real_git` |
| 2026-10-20 | working tree | Security | BOT | audit of the owner's real settings.json (names/lengths only) | 5 plaintext keys found (groq, gemini, routes, places, googleMaps); fix added, first real scrub not yet run | audit command output |
| 2026-10-20 | working tree (see programme commit) | Baseline | BOT | `cargo test --lib --features custom-protocol,admin-brain -- --skip tts_bench --skip tts_kokoro` | 2022 passed, 0 failed, 18 ignored | full lib run before changes |
| 2026-10-20 | working tree | Activity Manager | BOT | `-- --ignored live_foreground_sample_is_a_clean_app_name` reads the real foreground program and idle clock | pass | `[PROOF] foreground="chatgpt.exe" idle=Some(61) label=Some("Chatgpt")` |
| 2026-10-20 | working tree | Music control | BOT | `-- --ignored live_probe_players_and_phrases` reads the registry and parses 10 phrases | pass; Spotify registered, Apple Music not; "play despacito" was NOT understood before this change | `[PROOF] spotify registered=true applemusic registered=false` |

Rules:

- Record failures too; a failed run is evidence.
- Evidence is a log excerpt (`[WATCH-POWER] ...`, `[MAIN-CMD] ...`) or a path to a saved output; never a claim without one.
- A run on an older commit does not count after the feature's code changes; re-run it.
