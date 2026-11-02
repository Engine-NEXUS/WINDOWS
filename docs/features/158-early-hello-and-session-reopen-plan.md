# 158 - Instant hello at logon and the reopen question (plan, nothing built)

Request (2026-11-02): at wake-up say "Good afternoon, sir" at once while the app starts in the background; after 6 pm say "Welcome back, sir" and ask whether to reopen the apps from last time; a time-aware script with random variants per part of the day; yes/no answers; the session is remembered when the laptop is shut down (by an agent or by the user) and written into the script for the next wake-up.

## Measured (boot_timeline.jsonl)
| start | process start -> first sound |
|---|---|
| warm, from a terminal | 5.5 s |
| logon 2026-11-01 | 26.6 s |
`main()` is reached 86-135 ms after process start in every run, so a sound can start from inside `main()` long before the window, tray, memory store and wake engine exist.

## What exists already
- cached greeting clips per voice on disk (`tts_cache_disk`, "Good morning/afternoon/evening/night, sir"), played by `memcore::briefing::submit_hello` only after the stage is alive (the "no sound before the orb" rule), 2 h minimum gap, three periods (evening starts 17:00).
- the previous session is captured every 60 s and at session end (`memcore::session`), reopened by `restore.rs`; "Shall I reopen them?" is asked in the once-a-day briefing and the yes is handled by `memcore::offer` + the strict yes/no classifier (`confirmReply.ts`, `offer::classify_reopen`).
- the voice the user chose is the one the cached clips are rendered in; a voice swap already re-renders fixed lines.

## Plan
1. **Early hello (in-process, no second exe).** At the top of `run()` (before Tauri is built) a thread reads `hello_script.json`, picks the line for the local time, decodes the cached WAV and plays it with rodio (already a dependency). Retries for ~10 s if the audio device is not ready yet, gives up silently (the normal hello then plays later as today). The boot hello is skipped when the early one played (shared flag + a `last_played` stamp in the script, so the 2 h rule needs no memory store). New boot mark `early_hello_audio`. Expected first sound 1-3 s after logon instead of 26 s.
2. **The script file** `<app data>/hello_script.json`, written atomically by NEXUS: voice id; per part of the day (morning 05-11:59, afternoon 12-17:59, evening 18-21:59, night 22-04:59) 2-3 variants {text, wav}; the last variant used (never the same twice in a row); `last_played`; and the `offer` block (summary like "VS Code on ULTRON and Brave", app count, left_at) or null. Rewritten when the voice changes, when the variants change, and whenever the session snapshot changes (so a power cut, lid close or an agent-triggered shutdown never loses it: nothing depends on an exit event). Clips are rendered in the user's voice through the existing cache path, offline fallback = the local voice.
3. **Wording (random variant per part of day).** Before 18:00 only the greeting ("Good afternoon, sir."). From 18:00 (your rule) "Welcome back, sir." and then the question. Question asked only when the script's offer exists.
4. **Two-stage question (recommended, honest about timing).** The helper cannot hear an answer (speech recognition needs the app). So the greeting plays at once; the question "Shall I open the apps you were using - VS Code on ULTRON and Brave?" is asked by the app the moment it is ready, with the answer window open right then. Asking it at second 1 and listening only from second 5-25 would lose the user's early "yes". Target: ready at 5-8 s after the startup work in doc 157.
5. **Answers.** yes: yes, yeah, yaa, haan, ok, okay, do it, sure, go ahead, please, open them, yep. no: no, nope, don't, it's fine, leave it, not now, no need, nahi. Silence for 10 s = no (nothing is opened). Anything else = not an answer (the question is not repeated). Reuses `offer.rs` + `confirmReply.ts`; adds the Hindi/Hinglish words and the silence rule. If the user says no several days in a row the question is asked less (habits).
6. **Time check.** Done by the early step from the local clock at play time (the script only stores the lines); the app re-checks when it asks the question, so a 17:59 greeting never becomes a "welcome back" question late.
7. **Sleep/wake.** On resume the process is already running, so the same script is played instantly from the app (no startup), keeping today's wait-for-keyboard so it does not talk to an empty room.

## Risks
- Audio device not ready at logon (Bluetooth especially): retried, then skipped.
- Sound with no orb or caption: this overrides the old rule, your decision.
- Dev restarts: the 2 h gap and a `--background`-only trigger keep `nexus start` quiet.
- A new voice has no clips yet: the early hello is skipped until they are rendered.
- The answer cannot be heard before the app is ready (see 4).

## Build order
P0 early hello + script reader/writer + boot mark (measure first sound). P1 variants, 18:00 rule, voice-change re-render. P2 stage-2 question and the extended yes/no. P3 offer block from the session snapshots. P4 resume path and learning from repeated "no". Each phase is testable offline (script selection, variant rotation, time bands, answer classification are pure functions).
