# 185 - Early hello: the greeting plays the moment the desktop is shown (2026-11-02)

Plan: docs/features/158-early-hello-and-session-reopen-plan.md (P-1 + P0). Coded by Claude: the owner asked for GLM 5.3 Flash through OpenCode, but OpenCode answered "Free usage limit reached" (three attempts produced no files), and the owner chose "I code it".

## Built
- `early_hello.rs`: a thread started at the top of `run()`, ONLY for the logon-task launch (`--background`) and when `earlyHello` (default true, in `NexusSettings`) is on. It waits (up to 90 s) until the DESKTOP is shown: the input desktop is the normal "Default" one (the logon animation and the lock screen are on "Winlogon"), the taskbar window is visible and the desktop window is visible; after 250 ms it plays the cached clip of the user's edge voice for the part of the day (morning 05-11:59, afternoon 12-17:59, evening 18-21:59, night 22-04:59; night uses the evening clip until new clips exist) through rodio at 85 % volume, retrying the audio device for 10 s. No clip, no device or no desktop -> it gives up quietly and the normal hello speaks as before.
- `hello_script.json` (app data): voice, lines per part of the day, last variant, last played; a variant is never repeated back to back; at most one early hello per 2 h; a voice change keeps `last_played`.
- The normal boot hello (`memcore::briefing::submit_hello`) stays silent while the early one is waiting or has played, and records `last_hello_ts`.
- Boot marks: `setup_before_tray`, `early_hello_desktop_shown`, `early_hello_audio` (see `boot_timeline.jsonl`) to measure the 13.5 s tray gap and the real time to sound.

## Verify
Rust lib suite passes; early_hello 8 tests, including a by-hand test that the real Windows checks say "shown" on a live desktop (passes on this machine). NOT run live: a real logon (needs a rebuild + restart), audio at logon, the lock-screen gate. Not built yet: the native orb, the early question + microphone buffer, welcome-back wording after 18:00, the unlock/lid triggers (plan 158 P1-P4).
