# Live proof ledger

One line per real run of a feature (not unit tests). A scorecard row's **L** (live-verified) value may only be raised for a run that has a line here. See `docs/features/149-feature-production-programme.md`.

Proof kinds: **BOT** = run by the assistant on this laptop with a real process / OS call; **OWNER** = run by the owner (voice, ears, eyes, real accounts); **EXT** = needs outside hardware, software, or a service.

| Date | Commit | Feature | Proof | What was run | Result | Evidence |
|---|---|---|---|---|---|---|
| 2026-10-20 | working tree (see programme commit) | Baseline | BOT | `cargo test --lib --features custom-protocol,admin-brain -- --skip tts_bench --skip tts_kokoro` | 2022 passed, 0 failed, 18 ignored | full lib run before changes |
| 2026-10-20 | working tree | Activity Manager | BOT | `-- --ignored live_foreground_sample_is_a_clean_app_name` reads the real foreground program and idle clock | pass | `[PROOF] foreground="chatgpt.exe" idle=Some(61) label=Some("Chatgpt")` |
| 2026-10-20 | working tree | Music control | BOT | `-- --ignored live_probe_players_and_phrases` reads the registry and parses 10 phrases | pass; Spotify registered, Apple Music not; "play despacito" was NOT understood before this change | `[PROOF] spotify registered=true applemusic registered=false` |

Rules:

- Record failures too; a failed run is evidence.
- Evidence is a log excerpt (`[WATCH-POWER] ...`, `[MAIN-CMD] ...`) or a path to a saved output; never a claim without one.
- A run on an older commit does not count after the feature's code changes; re-run it.
