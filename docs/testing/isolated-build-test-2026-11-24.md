# Isolated build test of the live captions switch and the eight-feature fixes (2026-11-24)

The owner asked for the switch to be built and then tested thoroughly on a real build. The usual quiet test harness (`scripts/run-hidden-test.ps1`) **force-stops every `nexus` process**, which would have killed the NEXUS the owner was using (pid 3340, running since the evening before). So the build was tested in an isolated copy instead, and the owner's NEXUS was never touched (checked by pid and start time before and after).

## How it was isolated

* **A contributor-profile build** (`--features custom-protocol,contributor`, the project's own mechanism): its own app identity `com.nexus.assistant.contributor`, own data folder (`%APPDATA%\com.nexus.assistant.contributor`), own keychain and own WebView profile. It therefore holds **none of the owner's API keys**: nothing was sent to Groq, Deepgram or Gemini.
* **Built with the same overlay the project's build script uses** (`TAURI_CONFIG` from `scripts/contributor-cli.mjs::configuration`, `NEXUS_MANAGED_PYTHON` pointing at the checkout's `.venv`), into `D:\nexus-screen-target` (debug profile, about 2 minutes). `node nexus.mjs build` itself refuses to run while a NEXUS is running.
* **A real finding on the way:** the first launch exited at once. Both identities used one **fixed** start-up lock name (`Local\com.nexus.assistant-boot`), so an installed personal NEXUS made every contributor build think it was a second launch and refuse. The lock name now includes the app identity (`boot::primary_mutex_name`); a personal build keeps exactly the old name (pinned by a test), so installed copies are unaffected.
* **Run on its own hidden Windows desktop in quiet mode** (`scripts/run-isolated-test.ps1`, new; unlike `run-hidden-test.ps1` it never stops another process, and `-Stop` stops only the copy that runs this exe path). Its log goes to `D:\nexus-isolated.log`; debug port 9333.
* **Driven through its own debug port** (`scripts/test-isolated-app.mjs`, new): the real Tauri commands and the real Command Hub page, nothing synthetic about the app.

## Result: 21 of 21 checks passed

| Check | Result |
|---|---|
| The isolated app starts and its stage page is reachable; no panic at start-up; contributor identity | pass |
| `get_settings` has `sttLiveCaptions`, **false by default** | pass |
| `settings_patch` turns it on; read back; written to `settings.json` | pass |
| A **full Save** of another field keeps it on (the field is not wiped; this is the classic regression for a new setting) | pass |
| A **real native capture** starts and stops (`stt-capture: started ... stopping`) | pass |
| With the switch **on**, the real capture reaches the stream dispatcher, which tries the local speech server and logs `stt_stream: connect failed ... refused` (the local server is not running here, as expected; no cloud) | pass |
| With the switch **off**, the dispatcher logs `not streaming this turn (live captions are off)` and connects to nothing | pass |
| **Privacy mode** (`localSttOnly`) with captions on: only the local server is ever considered; Deepgram and Groq never appear | pass |
| Ordinary turns still work through the real orchestrator ("hello" handled locally; "what time is it" routed to the backend) with no error or panic | pass |
| The usage figure is readable | pass |
| The **Command Hub, Features page** shows "Live captions while you speak" and clicking the switch changes the saved setting (false to true) | pass |
| The **Command Hub, Audio page** shows it; its switch is a draft that saves with the page's Save button, like the other Audio switches (false to true after Save) | pass |

## What this does NOT prove

* **Words appearing while someone speaks.** The hidden desktop has no one speaking, and playing a recording through the speakers would not be quiet. So the path **microphone -> Groq -> caption on screen** was not run. What was run live is everything up to the dispatcher's decision, in the real binary. The Groq request scheduling is covered by the 28 stream tests (fake recogniser, a real localhost WebSocket server, a real recording through the whole chain) and the real-time measurement.
* **Any Groq or Deepgram call.** The isolated profile has no keys on purpose.
* **The release build.** This was a debug build (the release build takes about 50 minutes). Optimised behaviour, startup time and memory are unmeasured.
* **The owner's own profile**: the switch's default for an existing `settings.json` is covered by the "off by default" check on a profile that has no entry, which is the same situation.

## Rerun it

```powershell
# build (about 2 minutes), from the repository root
node scripts/build-isolated-test.mjs
powershell -File scripts/run-isolated-test.ps1
node scripts/test-isolated-app.mjs
powershell -File scripts/run-isolated-test.ps1 -Stop
```

## Owner steps that remain (10 minutes, on your own NEXUS after a rebuild)

1. Turn on Features, Voice, "Live captions while you speak".
2. Say a short command: no caption. Say a long one (about 4 seconds): a caption about 2 seconds in, then updates, then the final text.
3. Look at your Groq limits page: separate counters for `whisper-large-v3` and `whisper-large-v3-turbo`?
