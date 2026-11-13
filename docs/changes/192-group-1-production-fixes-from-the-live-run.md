# Group 1 production fixes found by the live run

Date: 2026-11-11 to 2026-11-12. Scope: the code changes made while running the "Group 1" production-readiness programme (the 15 features closest to production-ready), plus the memory-key and dictation fixes that were already on this branch. This record is about **code**. The run sheet, the per-card results and the proof-ledger lines are in `docs/testing/group-1-live-run-2026-11-11.md` and `docs/testing/live-proof-ledger.md`.

Nothing here makes a feature "production ready" by itself. The repository's definition needs (1) tests passing on the commit, (2) a live run recorded in the ledger on current code, and (3) no known defect. Every fix below was found by a live run, fixed, and re-run on a new build; the owner's own-voice and eyes-and-ears proofs are still outstanding and are listed at the end.

## Why these fixes exist

The Group 1 run drove the real application with a Windows voice speaking through the speakers into the microphone, scripted `Ctrl+Space` key presses, and log-driven waits. It read the answers from the application log, the master volume from Core Audio, and window counts from the operating system. All results are machine (BOT) proofs. Each run found defects that unit tests could not, because the defects are timing and state problems between components (capture, transcription, routing, alerts, speech).

## Commit 1 (earlier on this branch): memory key safety, dictation, CI gates

Source commit `b6b0a85`.

| Area | Defect | Fix |
| --- | --- | --- |
| Memory core | A memory wipe rotated the data key but a failed write could leave the stored key unable to open the file on disk. | A wipe now stores the new data key, re-seals the file, and puts the old key back if that write fails, so the stored key always opens the file on disk. The process-wide key follows a committed rotation. |
| Memory core | Two spellings of one folder could open two in-memory stores. | One store per real folder (canonical path). |
| Dictation | "one dollar" and "one rupee" were pluralised; weekdays and months were not capitalised. | Singular units for one; weekdays and months are capitalised (except "may" and "march", which are ordinary words). The corpus rows that pinned the old output were corrected. |
| CI | The window-opacity gate and the work-card drift check did not run in CI. | Both run. |

## Commit 2: the Group 1 fixes, in the order they were found

### 1. Hotkey: a press while a turn is in flight (card 5.1)

* **Symptom (live log 2026-11-11 18:30:02).** After a capture ended and before the orchestrator installed a request, nothing reported NEXUS as busy. A `Ctrl+Space` press in that gap called NEXUS again, so the old question was still answered aloud and the new capture replied to that answer.
* **Cause.** The gap between "capture heard speech" and "a request exists" was invisible to the hotkey.
* **Fix.** New module `src-tauri/src/turn_pending.rs`. `mark(session)` is called before transcription starts, `clear()` when the request is installed or the transcript is empty, `cancel()` from the hotkey. It expires after 5 s so it can never get stuck, and a cancelled session is remembered for 30 s so its transcript is dropped when it arrives. The hotkey treats a pending turn as thinking (`has_request: has_active_request() || turn_pending::is_pending()`).
* **Second gap (live log 2026-11-11 20:26:25).** A press about 100 ms later, while the machine-learning parse was running and no request existed yet, was logged as a cancel but the answer was still spoken. Routing now runs under a 20 ms check (`tokio::select!` loop in `process_transcript_inner`) until a request owns the turn.
* **Silent-miss counter.** A silent press after an earlier miss was answered "I didn't hear you, sir." because the miss counter survived the user's cancel and the new call. `resetMissStreak()` in `frontend/src/audio/recorder.ts` is called on `orb:cancel` and `orb:wake`; the silent auto-relisten (same attempt) deliberately does not clear it. A unit test pins this.

### 2. Alerts spoken into the user's own turn (cards 5.1, 5.14)

* **Symptom A (2026-11-11 20:25:53).** The boot briefing was spoken into a microphone the hotkey had just opened; NEXUS transcribed its own greeting and answered it.
* **Symptom B (2026-11-12 04:35:37).** "Claude Code has finished, sir." was released in the half second between NEXUS asking a delete question and the answer window opening, was transcribed as the user's answer, and NEXUS replied "Sorry sir, was that a yes or a no?".
* **Fix.** `proactive_policy::mic_belongs_to_user(voice_heard, capturing, ghost_session)` is true for an open microphone outside a Ghost session, so the alert waits. `memcore::offer::asked_within(Duration)` holds alerts for 45 s after a question is armed. Both are included in the policy's `working` signal together with `turn_pending::is_pending()`. Tests cover all three.

### 3. "list my pull requests" did nothing outside Ghost mode (card 5.13)

`ParsedIntent::OpenGitHubDashboard` was only dispatched inside a Ghost session; elsewhere it was routed and then silently ignored. The normal-mode intercept in the orchestrator now handles it (`run_open_github_dashboard`).

### 4. "close Calculator" could not find the app (card 5.2)

The code looked for `calc.exe`, a launcher that has already exited; the app runs as `CalculatorApp.exe` (or `Calculator.exe`). `running_process_names("calc.exe")` now returns both names, used by `close_app`. A test pins the mapping.

### 5. Remembered facts kept the speech-to-text full stop (card 5.13)

"Remembered my favorite color as blue." stored the trailing punctuation with the value. `memory::clean_value` strips trailing `. ! ? , ;`. A test covers it.

### 6. File control rejected ordinary phrases (card 5.14)

* "delete the text files in the nexus test folder in downloads" was answered "I couldn't match that command". The word "text" excluded the phrase as a message, and only a destination could be a sub-folder. `not_about_files` now treats "text files" and "documents" as files; `split_folder` returns `"<name> folder in <alias>"`; `default_folder` falls back to `resolve_dest`.
* The same phrase then listed the three files and asked which one, because the plural was only read from the type word. "files" now sets `plural` in `action_kernel/resolve.rs`.
* The safety properties are unchanged: the delete still needs the spoken yes, still goes to the folder's `.nexus-trash`, undo restores it, and "delete everything / wipe my drive" is still refused locally.

### 7. "Shut down." became a close-app command (card 5.14)

"Shut down." (transcribed with a space) parsed as `close_app("down")` and NEXUS answered "I couldn't find Down running, sir." `intent_parser::parse_deterministic` now returns `None` for phrases that `power_cmd::parse_weak` recognises, before the file-action parser, so a bare power phrase reaches the ask-first power path. Test cases: "shut down", "Shut down.", "Nexus, shut down.", "shutdown", "turn it off".

### 8. An unmapped app was told it opened a tab (card 5.15)

For applications without a shortcut-map entry the legacy key path was used and NEXUS spoke "New tab sir." even though nothing could be confirmed. `unverified_key_line(label)` now says the key was sent but not confirmed. A router test still pins the "New tab sir." wording for the mapped-and-verified case; the unverified case for Notepad and the terminal is a known remaining wording gap (see below).

## Results on the final Group 1 build

Machine (BOT) passes, with ledger lines: 5.1 hotkey (6 of 6), 5.2 open and close apps (5 of 5), 5.4 online voice (11 of 11 replies on the Edge engine, 0.9 to 1.6 s), 5.5 the Groq path of the router, 5.6 volume (100 percent to 75 percent to 100 percent, no dip inside a reply), 5.8 footprint (638, 630.5 and 628.9 MB against the 700 MB ceiling), 5.13 sub-centres (13 of 13), 5.14 file control, 5.15 the "new tab" shortcut on Brave and Edge. Partial: 5.3 (the cue reached the audio device in 0 to 15 ms across 20 cues; audibility not judged), 5.7 (the log pairs; `powercfg /requests` needs an administrator console), 5.9 (Worker health 200 in 0.17 s; the GitHub question needs a sign-in). Not run: 5.10, 5.11, 5.12 (they need the owner's eyes).

## Still open

* The owner's own-voice and ears-and-eyes proofs for cards 2, 9, 14, 20, 25, 27, 31 and 57.
* A spoken "No." can come back from the speech engine with invented extra words, so a decline is not recognised (nothing is deleted when that happens). The hallucination filter does not catch a long invented tail after a recognised answer word.
* "I remember 1 fact. my favorite color is blue. across N recent conversations." is ungrammatical.
* The router speaks "New tab sir." for an unconfirmed Notepad or terminal result; a router test pins that wording.
* 22 of the 23 mapped shortcut actions have no spoken phrase.
* `nexus start` fails without `pwsh` and the microphone-silence warning blames an "Intel SST driver".
* A press in the 100 to 300 ms routing gap has unit-test coverage only, not a live press.

## Files

`src-tauri/src/turn_pending.rs` (new), `wakeword_oww.rs`, `hotkey.rs`, `orchestrator.rs`, `proactive_policy.rs`, `memcore/offer.rs`, `command_executor.rs`, `memory.rs`, `intent_parser.rs`, `action_kernel/voice.rs`, `action_kernel/resolve.rs`, `frontend/src/audio/recorder.ts`, `frontend/src/stage/orbRuntime.ts`, and their tests.

## Verification

Rust library 3,197 tests plus 20 integration tests passed on the final Group 1 commit (serial, `--features custom-protocol,admin-brain`); frontend 1,135 tests and `tsc` clean; the dead-code ratchet (139 known, none new), the IPC contract, hygiene and work-card drift gates pass.
