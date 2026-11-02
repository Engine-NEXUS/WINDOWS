# 185 - Early hello: the greeting plays the moment the desktop is shown (2026-11-02)

Plan: docs/features/158-early-hello-and-session-reopen-plan.md (P-1 + P0). Coded by Claude: the owner asked for GLM 5.3 Flash through OpenCode, but OpenCode answered "Free usage limit reached" (three attempts produced no files), and the owner chose "I code it".

## Built
- `early_hello.rs`: a thread started at the top of `run()`, ONLY for the logon-task launch (`--background`) and when `earlyHello` (default true, in `NexusSettings`) is on. It waits (up to 90 s) until the DESKTOP is shown: the input desktop is the normal "Default" one (the logon animation and the lock screen are on "Winlogon"), the taskbar window is visible and the desktop window is visible; after 250 ms it plays the cached clip of the user's edge voice for the part of the day (morning 05-11:59, afternoon 12-17:59, evening 18-21:59, night 22-04:59; night uses the evening clip until new clips exist) through rodio at 85 % volume, retrying the audio device for 10 s. No clip, no device or no desktop -> it gives up quietly and the normal hello speaks as before.
- `hello_script.json` (app data): voice, lines per part of the day, last variant, last played; a variant is never repeated back to back; at most one early hello per 2 h; a voice change keeps `last_played`.
- The normal boot hello (`memcore::briefing::submit_hello`) stays silent while the early one is waiting or has played, and records `last_hello_ts`.
- Boot marks: `setup_before_tray`, `early_hello_desktop_shown`, `early_hello_audio` (see `boot_timeline.jsonl`) to measure the 13.5 s tray gap and the real time to sound.

## Verify
Rust lib suite passes; early_hello 8 tests, including a by-hand test that the real Windows checks say "shown" on a live desktop (passes on this machine). NOT run live: a real logon (needs a rebuild + restart), audio at logon, the lock-screen gate. Not built yet: the native orb, the early question + microphone buffer, welcome-back wording after 18:00, the unlock/lid triggers (plan 158 P1-P4).

## Update 2026-11-02 (after the first real logon test)
Owner report: it said "Good evening" but no orb, no caption, and the volume was not set to the speech volume. Boot timeline of that logon: `stage_shown` 7.7 s, `early_hello_desktop_shown` 8.9 s, `early_hello_audio` 12.6 s (3.7 s from desktop to sound), `setup_tray_done` 26.6 s (tray setup alone took ~18.7 s: confirmed).
- **Volume**: the early path now takes the same `volume::acquire_volume_lease(ttsVolume)` lease as every spoken line (75 % from `ttsVolume`) and restores the user's volume afterwards. (Root cause: the new code played through rodio directly and skipped the lease.)
- **Orb + caption**: new `early_orb.rs`: a native layered, click-through, never-focused window drawn in software: the black pebble capsule at the saved position (`orbPosition`, `orbHorizontalPct`, `orbScoopWidth/Height`) with a white particle sphere that swells with the voice (envelope of the clip), and the black caption pill below it revealing the words letter by letter from the clip's word timings; fades in, lingers, fades out, then the window is destroyed. A lightweight stand-in for the WebGL orb, which does not exist until the app window is up (5-25 s). Verified as images rendered from the real code (`write_preview_images` test); not yet seen at a real logon.
- **Delay**: the clip is read and the audio device is opened WHILE waiting for the desktop (new mark `early_hello_audio_ready`), so the sound starts ~250 ms after the desktop appears instead of 3.7 s (expected; to be confirmed by the next timeline).
- Settings read early by `parse_settings` (tested, clamps nonsense). Rust lib suite passes.
- Still open: the 18.7 s `tray::setup` wait, the early question + microphone buffer, welcome-back after 18:00, unlock/lid triggers, the real WebGL orb taking over from the stand-in.
