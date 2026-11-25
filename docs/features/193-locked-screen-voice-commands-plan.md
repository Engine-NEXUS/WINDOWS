# Plan: voice commands on the Windows lock screen and with the screen dim or off

Date: 2026-11-15. Status: **plan only, nothing built.** Windows only (the owner dropped Mac and Linux). Research summary is in the 2026-11-15 chat; sources at the end.

## 1. Goal

When the laptop is locked, or the screen is dim or off but the laptop is not asleep, NEXUS still hears its wake word and obeys a **short allow-list**: shut down, restart, sleep, lock. Everything else is refused out loud. NEXUS never unlocks the laptop.

## 2. What is and is not possible (decided)

| Wish | Verdict |
| --- | --- |
| Wake word and commands while locked | **Yes** in software. A locked session keeps its programs and microphone. Not yet measured on this laptop. |
| Shut down / restart / sleep / lock while locked | **Yes.** `power_cmd::execute` already calls `shutdown`, `SetSuspendState`, `LockWorkStation`. Not yet tried while locked. |
| Wake a laptop that is really asleep | **No** from software. Needs a hardware keyword spotter in the audio driver, only in Modern Standby, and Windows 11 24H2 can switch wake sources off. Not planned. |
| Stay listening instead of sleeping | **Yes, as an option.** Keep the laptop in the "dim, locked, awake" state. Costs battery; measure it. |
| Unlock by voice | **Not planned.** A replayed recording of the owner would open the laptop. If wanted later: voice wakes the screen and Windows Hello (face or fingerprint) unlocks. |
| Wake-word indicator on the lock screen | **No.** The lock screen cannot be drawn on by an app. Use sound only: the wake cue and a spoken answer. |

## 3. What already exists (read, not guessed)

* `power_cmd.rs`: `parse`, `parse_weak`, `parse_casual`, `parse_loose`, `execute_now`, countdown with cancel, `goodbye_line`. The shutdown has no `/f`, so an app with unsaved work can block it.
* `early_hello.rs`: private `win::session_locked()` reads the session lock flag (measured on this machine 2026-11-03: the Windows 11 lock screen runs on the normal desktop, so only the session flag tells).
* `wakeword_oww.rs`: the per-chunk `suppressed` check (meeting, Hand Mode). Lock state is **not** consulted, so the wake word is not blocked by a lock today. Whether the whole turn survives a lock is unknown.
* `center::action_disposition` and the Voice Lock (`speakerVerification`, off by default): the existing place to refuse a command by speaker.
* `keep_awake.rs`: the lease that holds the system awake.

## 4. Design

### L0. Measure first (no behaviour change)

A log-only probe, `[LOCKED]` lines, switched on by an env var. The owner locks the laptop, then lets the screen dim and go off, and says the wake word and a command at each stage. Questions to answer:

1. Does the microphone keep delivering audio (RMS, `cam`-style counters) when locked? The Intel SST mic stalls in your notes; does a lock make it worse?
2. Does the wake word fire, and does the capture, Groq transcription and the Rust-side parse complete? The frontend (stage, `recorder.ts`) may be throttled while locked, so check which steps need it.
3. Is the spoken answer audible (rodio, Edge TTS, volume lease)?
4. What does `powercfg /a` report (S0 Modern Standby or S3)? In Modern Standby, how long until the Win32 app is frozen with the screen off?
5. Does `shutdown /s /t 0` and `SetSuspendState` work from the locked session, and does a blocking app stop the shutdown?

**Probe written (2026-11-16), not compiled or run:** `src-tauri/src/locked_probe.rs`. Start NEXUS with `NEXUS_LOCKED_PROBE=1`; it logs `[LOCKED] t=+Ns locked=<bool> mic_callbacks_5s=<n>` every 5 s plus a line on each lock/unlock. Zero callbacks while locked = the microphone stopped. Spoken answers show in the usual `tts: speaking` lines; wake/transcript lines show the rest. Measured on the machine this session ran on: `powercfg /a` = S3 only, no Modern Standby (re-check on the real laptop).

This decides whether L1 to L3 are small or need a native (no-frontend) path.

### L1. Lock state in one place

Move `session_locked()` out of `early_hello.rs` into a small `session_state.rs` with a cached value updated by a session notification (`WTSRegisterSessionNotification`, lock and unlock messages) plus the existing poll as fallback. One function, `is_locked()`, used by everything below. No other module reads the flag.

### L2. Locked policy (the safety part)

When `is_locked()`:

* The turn goes through a **locked allow-list** that accepts only `power_cmd::parse` results of kind Shutdown, Restart, Sleep, Lock. Everything else, including sign out, app launch, dictation, messages, mail, GitHub, and anything the NLU or the cloud model proposes, is refused with one fixed spoken line ("That needs you at the keyboard, sir").
* Never reaches the Worker or a cloud model, so no private memory is spoken or sent.
* **Who may give the command.** Because a stranger standing near a locked laptop can say "Nexus, shut down", the rule is: if a Voice Lock profile exists, only a verified owner turn is accepted; if none exists, all four (Shutdown, Restart, Sleep, Lock) are accepted. **Owner decision 2026-11-15: allow Shutdown and Restart while locked even without a profile.** The protection then is the switch (default off), the spoken countdown with "cancel", and never forcing apps closed. A stranger or a replayed recording can therefore shut the laptop down when no profile exists; the owner accepted that.
* Shutdown and Restart use the existing countdown with "cancel" and a 3 s final window. Never forced; if an app blocks, NEXUS says so.
* New setting `lockedVoiceCommands` (default **off**, Hub switch, loose-parsed in `NexusSettings` so it is not dropped on save). Off means today's behaviour exactly.

### L3. Staying listening

* While locked and `lockedVoiceCommands` is on, hold a keep-awake lease of the **system** (not the display) so the laptop does not drop into sleep while it is only dim. Release it on unlock, on battery below a limit, or after a quiet period (setting, default 30 minutes). Display may turn off freely.
* Measure the battery cost in L0 and show it in the Hub text next to the switch.

### L4. Feedback without a screen

Wake cue (`wake_cue.rs`) and a short spoken answer, nothing drawn. A refusal is spoken once, not repeated on every attempt. Log every locked turn (`[LOCKED] accepted/refused kind owner=`) without audio or transcript text kept.

## 5. Phases and acceptance

| Phase | Work | Acceptance |
| --- | --- | --- |
| L0 | Probe and the owner's locked/dim/off measurement | A ledger entry per question 1 to 5 above, with times. No code path changed. |
| L1 | `session_state.rs`, remove the private copy | Tests for the flag parsing and the cached value; `early_hello` behaviour unchanged; dead-code and IPC gates clean. |
| L2 | Allow-list, owner rule, setting and Hub switch | Tests: only the four kinds pass while locked; every other intent kind refused (table test over all parsed intents); unverified voice refused when a profile exists; setting survives a save; off means unchanged. Mutation checks on the allow-list and the owner rule. |
| L3 | Keep-awake lease while locked | Test of the lease lifecycle (lock, unlock, battery, timeout); owner measures battery per hour. |
| L4 | Spoken lines and log | Lines are fixed and pre-rendered; log test. |
| Live | Owner locks the laptop and tries: wake word, "shut down", "sleep", "restart", "lock", a refused request, a stranger's voice, a recording played from a phone | One ledger line each. Replay is expected to fool a speaker check; the countdown with "cancel" and the default-off switch are the safeguards. |

## 6. Decisions needed from the owner

1. ~~Shutdown and Restart while locked without a Voice Lock profile?~~ **Decided: yes, allow them.**
2. Is keeping the laptop awake while dim acceptable, and for how long on battery?
3. Should "lock" be allowed while already locked (harmless) or ignored?

## 7. What not to do

* No voice unlock, no credential provider, no password or PIN handling.
* No driver work for true Wake-on-Voice.
* Do not run lock tests on the hidden-desktop runner; a real lock needs the owner (the owner controls runtime and builds).
* Do not let the locked path call the cloud or read memory.

## Sources

* Microsoft, Voice activation: https://learn.microsoft.com/windows-hardware/drivers/audio/voice-activation
* Microsoft, Modern Standby wake sources: https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-wake-sources
* Microsoft, Windows Hello / Passport (TPM-held key design): https://learn.microsoft.com/windows/uwp/security/microsoft-passport
* Voice access on the lock screen: https://windowsreport.com/voice-access-windows-11-lock-screen/
