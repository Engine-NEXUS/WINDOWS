# 211: Hold keys, a listening session, and timing from speech to action (2026-11-26)

> **Superseded in part by 213:** the three extra keys and their switches were removed. The `[LAT-ACT]` timing stays.

First two steps of `docs/research/jarvis-speed/02-reel-and-plan-201-analysis-2026-11-26.md` ("measure first, then the
cheap speed wins"). All three switches are **off by default**, need a restart, and are in Command Hub > Voice.

| Switch | Key | What it does |
|---|---|---|
| `holdToTalk` | hold Ctrl+Alt+D | the words are typed into the window in front |
| `holdToCommand` | hold Ctrl+Alt+Enter | the words run as a command; letting go ends the turn |
| `listenSession` | Ctrl+Alt+L | commands one after another with no wake word |

## How it works

* **Held capture** (`hold_to_talk.rs`, `wakeword_oww.rs`): the key release ends it, not silence (60 s cap). Letting go
  in under 300 ms drops it. A released capture with no microphone audio is dropped after 1.5 s. Owner check (Voice
  Lock), speech-to-text choice and privacy mode are the same as any turn. Typing refuses protected, elevated and
  password targets, another window than at key-down, and held Ctrl/Alt/Shift/Win. Refused during Ghost mode.
* **Listening session** (`listen_session.rs`): the microphone re-opens after NEXUS has been idle for 800 ms. Each
  capture uses the short open-microphone silence wait (240 ms, 480 ms after pauses) and passes the directed-speech gate
  in Rust; silence, noise and ignored speech are dropped with no answer. It waits during speech, requests, meetings and
  hand mode. It ends with the key, Ctrl+Space, "stop listening" / "that's all", Ghost mode, or 120 s without an accepted
  command. The gate keeps its own off switch (`directedGate`); with it off, everything heard is acted on.
* **Timing**: the clock now starts at the end of speech (the silence wait is shown as `endwait`). Each local command
  that runs from a voice turn prints one `[LAT-ACT]` line; for "open app" it is printed when the window check returns.
  `node scripts/summarize-latency.mjs <log>` gives median and p90 per stage. The lines contain no spoken words.

## Not run, not measured

* **Nothing here has been run with a real voice.** No microphone is connected to this machine (only two display audio
  outputs), so no key was held, no session was used and **there is no timing baseline yet**.
* The `[LAT-ACT]` "act" time for an app includes the window check, which polls, so it can read up to one poll high.
* Ghost-mode commands, cloud answers and multi-step commands do not print `[LAT-ACT]`.
* An empty result from a held command goes to the ordinary "didn't catch that" retry.
* The live-caption pill may stay on screen after a dictation.
* The keys were checked on this PC only; elsewhere another program may hold one (a failure is logged, nothing else
  tells the user).

## Owner check (about 15 minutes, needs a microphone)

1. Turn the three switches on, restart NEXUS.
2. Click into Notepad, hold Ctrl+Alt+D, say a sentence, let go.
3. Hold Ctrl+Alt+Enter, say "open calculator", let go.
4. Press Ctrl+Alt+L, say "open notepad", then "open calculator", talk to someone else, then "stop listening".
5. Say 50 ordinary commands over a day, then run the summary script on the log.

## Results of the automated checks

* Rust: 3,680 library tests and 21 integration tests pass (admin features, 39 + 7 ignored).
* Frontend: 1,296 tests and the type check pass. Gates: turn-end, hygiene, IPC, feature registry, window opacity,
  readiness, work cards and dead code (nothing new). The `mock-wake` build compiles.
* Isolated app (hidden desktop, switches on): it starts and all three keys register. No key was pressed.
* Found there: Ctrl+Alt+Space, the first dictation key, is already held by another program on this PC (which one was
  not established). Dictation now uses Ctrl+Alt+D.
* No new test covers the live wiring (key press to capture to command, or a session capture being dropped); only the
  decision rules are unit-tested.
