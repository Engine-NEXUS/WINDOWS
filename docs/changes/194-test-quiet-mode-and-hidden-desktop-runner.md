# Test-quiet mode and the hidden-desktop test runner

Date: 2026-11-13. Scope: a way to run and test NEXUS on a machine where nobody should hear it, where the screensaver and display timeout must keep working, and where the visible desktop must stay clean. Source: `src-tauri/src/quiet.rs`, `scripts/run-hidden-test.ps1`, `scripts/drive-nexus.mjs`.

## The requirement (owner, 2026-11-13)

The machine under test is the owner's server. The owner asked that NEXUS be muted so nothing sounds on it, that testing must not remove or disturb the screensaver ("it should only show the screen when I personally move the mouse or keyboard, not by itself or by NEXUS voice command"), and that NEXUS run in an environment that does not disturb Windows so the screen is clean whenever the server is opened.

## What had been disturbing the machine

Earlier test runs (including the Group 1 run) did three things a server owner would not accept:

1. **Synthetic key presses.** The harness used `keybd_event` for `Ctrl+Space`. Synthetic input counts as user activity: it resets the idle timer and dismisses a screensaver.
2. **A screen-awake request.** NEXUS holds the screen awake with `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED)` while it speaks or works (`keep_awake.rs`), and the Agent Island holds it while a watched coding agent works. That postpones the screensaver and the display timeout.
3. **Sound and a volume change.** NEXUS speaks aloud and sets the master volume to its speech level (75 percent) while speaking, then restores it.
4. Windows (the stage overlay, the map panel, the place card, the sidebar) appeared on the visible desktop.

## Quiet mode (`NEXUS_QUIET=1`)

An environment variable of the NEXUS process, read once (`quiet::on()`), `1`, `true` or `yes`. **It is not a setting and is off by default**; a normal installation is unaffected. It changes nothing about what the assistant decides or says.

| Site | Effect in quiet mode |
| --- | --- |
| `keep_awake::assert_awake` / `clear_awake` (Windows) | Return success without calling `SetThreadExecutionState`, so Windows is never asked to keep the screen on. This also covers the Agent Island's screen hold, which goes through the same lease. |
| `volume::acquire_volume_lease`, `volume::ensure_live_lease` | Return no lease, so the master volume is never touched. |
| `tts.rs`: `apply_duck`, and the three sink-creation sites (single clip, streamed, narration) | Every playback sink is created at volume 0. Timing, captions and the turn logic run exactly as normal. |
| `wake_cue.rs` | The wake earcon plays at gain 0. |
| `window_tile::display_off` | Returns an error ("test-quiet mode: the display is left alone") so the display is never turned off. |

The Windows system voice only synthesises samples that then go through the same silenced sinks.

## The hidden desktop (`scripts/run-hidden-test.ps1`)

Starts `nexus.exe` with `CreateProcess` and `STARTUPINFO.lpDesktop = "WinSta0\NexusHiddenTest"`, after `CreateDesktop`. A Windows desktop is a separate surface: the process's windows are created there and are never shown on the desktop the user sees, take no focus and generate no input. The script also sets `NEXUS_QUIET=1`, `RUST_LOG=info` and `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`. `-Stop` ends NEXUS.

## Driving it without input (`scripts/drive-nexus.mjs`)

Connects to the stage webview over the debug port (Node 22 or later has a built-in WebSocket) and calls the application's own `orchestrator_process` command with the spoken text, which is the same entry point the voice path uses after transcription. It then prints the spoken answers (`tts: speaking '...'` lines from `%APPDATA%\com.nexus.assistant\logs\nexus.log`), the log lines of interest and the webview pages. Turns are passed in the `TURNS` environment variable as JSON (PowerShell 5.1 strips quotes from command-line arguments); `WAIT` sets the pause after each turn in milliseconds.

What this does **not** exercise: the microphone, wake-word detection, speech recognition and the speaker-verification check. Those need an acoustic path and are separate proofs.

## Measured result (2026-11-13, build `3fc76bbe872cb4cb`, 14 turns, 79 seconds)

| Measure | Method | Result |
| --- | --- | --- |
| NEXUS windows on the user's desktop | `EnumWindows` on the default desktop, counting visible windows owned by `nexus.exe` and `msedgewebview2.exe` processes (13 processes) | 0 before, 0 after |
| Master volume | Core Audio `IAudioEndpointVolume`, before and after | 100 percent both times (no dip to 75 percent) |
| Input idle time | `GetLastInputInfo` before and after | grew by 79,078 ms during 79,074 ms of testing, i.e. no input was generated |
| Launching | idle before and after launch | the idle counter kept growing (1,008,578 to 1,040,734 ms over 32 s) |

## Limits and warnings

* **Keep-awake cannot be tested under quiet mode** (the request is a no-op by design). Test keep-awake in a normal run, with the owner's agreement.
* The hidden desktop cannot be looked at. Anything that needs eyes on the screen needs a normal run or the browser preview pages.
* Quiet mode is silent, so audibility (card 5.3, the wake cue; card 5.4, the voice) needs a normal run.
* Test phrases are recorded in the memory log (`episodes.jsonl`, `memcore.enc`) like any conversation. A test that creates stored state must delete it (the live run removed the test trip reminder it created so it could not fire on the owner's real NEXUS).
* The microphone on the test machine reported zero signal during the run (the selected input produced no audio), which is why the run used `orchestrator_process` instead of speech.

## Tests

`quiet::tests::only_an_explicit_yes_turns_it_on` (parsing). The behaviour at each site is covered by the live measurement above rather than a unit test, because the sites are platform calls (`SetThreadExecutionState`, audio sinks, display power).

## Rollback

Revert the commit. Nothing is stored and no setting exists; without the variable the code paths are exactly the previous ones.
