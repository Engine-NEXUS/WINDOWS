# Logon 2026-11-02 20:35 IST: second opinion (read-only)

Scope: why the early hello (native orb, caption and sound) did not appear, why the `stage` window took 21 s, whether the broken memory store matters, and which fixes to make. Nothing on the machine was changed. All times are IST unless marked Z (UTC).

## Evidence checked

| Fact | Source |
|---|---|
| OS started 20:35:04.5, user logon 20:35:18, previous boot 18:53:17, logon 18:53:32. Both were Restarts on battery (`AcOnline=false`). | System log, Kernel-General 12, Winlogon 7001, Kernel-Power 105 |
| Scheduled task `NEXUS`: `D:\ULTRON\src-tauri\target\release\nexus.exe --background`, LogonTrigger, Priority 4 | `schtasks /query /tn NEXUS /xml` |
| The release exe was written at 20:19:55 (`nexus-build.json` builtAt 14:50:01Z). That is after commit 78c24c3 (20:05), which set `SCRIPT_VERSION = 2` and made the night line "Welcome back, sir". | file times, `git show 78c24c3` |
| `hello_script.json` on disk is still `"version": 1` and has no `night` line. `last_played` is 1793625828 (18:53:48). | the file |
| `tts_cache` had no `en-US-AvaNeural` clip for "Welcome back, sir" until **20:35:56**, when this run wrote it. "Good to have you back, sir" was written at 20:36:03. | `tts_cache\*.json` times |
| Timeline pid 10912 has no `early_hello_*` marks. Run 8956 (18:53) has `early_hello_desktop_shown` (8.9 s) and `early_hello_audio` (12.6 s). | `boot_timeline.jsonl` |
| nexus.exe started at 20:35:21.923, **before explorer.exe (20:35:22.473)**. Its WebView2 browser process 11300 started at 20:35:22.479, and its GPU and renderer processes at 20:35:25.7 to 25.96. The window was reported "created" only at 20:35:43.22. | `Get-Process`, `Win32_Process` command lines |
| Power throttling state: nexus.exe has `ControlMask=1 StateMask=1` (EcoQoS on). All its WebView2 children have `StateMask=0`, so they are **not** in EcoQoS. | `GetProcessInformation(ProcessPowerThrottling)`, read-only |
| The memory decrypt failure first appears at 2026-11-02T09:11:06Z (14:41 IST), in the first run of that day. `memcore.enc` was last written at 2026-11-02 00:40 IST. `nexus.log` has no lines between 11-01 12:21Z and 11-02 09:10Z. | `nexus.log`, `memory\` |

## A. Why the early hello did not appear

Code path: `lib.rs:393` → `early_hello::start_if_logon` (`early_hello.rs:282`) → `run` (`:316`).

| # | Possible reason | Verdict | Confidence |
|---|---|---|---|
| 1 | **No cached clip for the line.** At 20:35 the period is `Night`. `default_script` gives "Welcome back, sir" (`:71`). `clip_for` → `tts_cache_disk::load` (`tts_cache_disk.rs:144`) found no file: the clip was first written at 20:35:56, by this same run. `run` then printed a `println!` and returned `false` (`early_hello.rs:322-324`). | **Confirmed root cause** | high |
| 2 | 2 h gap (`MIN_GAP_SECS`, `:43`) | **Ruled out.** `load_script` (`:96-97`) discards a script of a different version and returns `default_script`, which has `last_played: None`. The file on disk is version 1 and the binary is version 2, so `due(None)` is true. A side effect: a version bump also drops the 2 h guard (`:96` returns the default without copying `last_played`). | high |
| 3 | `--background` not passed | Ruled out. The task XML has `<Arguments>--background</Arguments>`, and the log shows `startup: --background mode` at 15:05:51Z. | high |
| 4 | Setting `earlyHello` off | Ruled out. The key is absent from settings.json, and `parse_settings` defaults it to `true` (`:185`). | high |
| 5 | Thread panic | Not supported by any evidence. The script file was not rewritten and no marks were written, which fits an early return. A panic would also leave `STATE = PENDING`. That would make `owns_the_hello()` true, and the boot hello would then have been **suppressed** (`briefing.rs:547-553`). The boot hello was spoken (15:05:54.96Z), so `STATE` was `GAVE_UP`. That matches a clean `return false`. | high |
| 6 | Desktop never detected / audio device missing | Ruled out. Both checks run only after the clip check (`:327-359`). | high |
| 7 | Diagnostics invisible | Confirmed as an observability gap. `main.rs:3` sets `windows_subsystem = "windows"` for release builds, so every `println!("[EARLY-HELLO] …")` goes nowhere at a logon launch. The only written evidence is `boot::mark`, and there is no mark for "gave up: no clip". | high |

Why the clip was missing: the 20:19 binary introduced a new line, and no run of that binary had happened before the reboot. The previous app run started at about 20:04, with the old binary. So the startup cache warm-up (`tts::CACHED_PHRASES`) had never rendered "Welcome back, sir". **Each time the hello text or the voice changes, the first logon after that is silent.** The next logon should play, because both night clips now exist, unless the voice or the text changes again.

Markers that would prove the rest next time: add `boot::mark_with("early_hello_gave_up", "<reason>")` on every `return false`, plus a `tracing::info!`, because `init_logging` has already run at `lib.rs:391`.

## B. Why `stage` took 21 s (7.7 s and 8.3 s on earlier logons)

Facts:
- The stage is created **synchronously on the main thread** inside `setup` (`lib.rs:525`, the comment at `:522-524` says so).
- The WebView2 processes were fast. The renderer was up 4 s after nexus started. The remaining ~17 s was spent waiting for the controller-created callback on nexus's main thread, not inside WebView2.
- The total stall is about the same on both logons, only shifted between steps:

| Logon | stage_shown | tray step (`setup_before_tray` → `setup_tray_done`) | setup_end | setup_end, seconds after OS start |
|---|---|---|---|---|
| 18:53 (pid 8956) | 7.7 s | **18.7 s** | 26.9 s | ~45 s |
| 20:35 (pid 10912) | **21.4 s** | 7.3 s | 29.7 s | ~47 s |

So the main thread is held by something that ends at a roughly fixed point, about 45 s after boot, and whichever step is running absorbs it. The likely holder is the shell coming up, competing with nexus for the CPU. NEXUS started before explorer, so the tray icon (`Shell_NotifyIcon` talks to explorer) and WebView2 controller hosting (DComp/DWM, a parent HWND) both have to wait for it. Many other programs started in the same window: explorer, Search, StartMenu, Widgets, PowerToys (about 10 processes, PowerLauncher 45 CPU-s, CmdPal 33 CPU-s), DefenderSessionHelper, Defender configuration changes 20:35:20 to 20:36:04, and SamsungMagician installing a service at 20:35:36 (the same happened at 18:53:49). Confidence: medium. The pattern is clear, but I could not see the 18:53 process mix.

| Hypothesis | Verdict |
|---|---|
| EcoQoS inherited by WebView2 | **Ruled out.** The children read `StateMask=0`. |
| EcoQoS on nexus's own main thread making it lose under contention | Plausible contributor, not proven. It was on in every logon, including the 7.7 s ones, so it cannot explain the difference alone. Under heavy contention, an EcoQoS process is scheduled last or on E-cores. This is the A/B test in D. |
| WebView2 profile reset / first run | Ruled out. `webview_mode` is unchanged ("release") and EBWebView was created on 19-10. The reset only happens when the build type changes (`lib.rs:253-262`). |
| WebView2 runtime update | Ruled out. The runtime 156.0.4314.58 is from 01-11. |
| New binary, Defender first-run scan | Possible small part. `main` was marked at 141 ms, so the process start itself was not delayed. |
| Disk | No evidence either way. |

The Microsoft WebView2 performance guidance names cold start and real-time antivirus scanning of the runtime and user data folder as startup costs. It advises not to put WebView2 on the initial UI path:
- https://learn.microsoft.com/microsoft-edge/webview2/concepts/performance
- WebView2 in a non-interactive context: https://github.com/MicrosoftEdge/WebView2Feedback/issues/3177
- EcoQoS semantics (docs say EXECUTION_SPEED is not for foreground work; inheritance is not documented, and I measured it above): https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation

The boot hello itself is chained behind all of this:
- `boot_ready` (33.0 s) waits for `stage_alive` (32.0 s).
- The line then took about 1.8 s more because "Welcome back, sir" was not cached: Edge synthesis at 15:05:56.03Z.
- First audio came at 34.8 s, which is about 13 s after explorer started and about 30 s after logon.

## C. The broken memory store

The failure is **not new to this logon**. It has failed on every run since 14:41 IST today (2,436 warnings). Effects:
- `submit_hello` reads `last_hello_ts` through `with_store`. That returns `None`, so `hello_due(None)` is always true (`briefing.rs:37-39, 557`). The 2 h boot-hello guard is off, so a developer restart is greeted every time. The write at `:562` is also lost.
- It did **not** cause the missing early hello. `early_hello` uses `hello_script.json`, not the store.
- Session offers, resume points, the profile, the timetable, routines and corrections fall back to legacy files or nothing. That is degraded, not fatal.

`crypto::open` failed with the key from Credential Manager (`memcore/crypto.rs:115-126`, `store.rs:304-307`). So either the key `com.nexus.assistant / memcore_data_key_v1` changed after `memcore.enc` was written at 00:40, or the file is damaged. Candidates:
1. a memory wipe that rotated the key (`rotate_key`)
2. a test or tool that wrote that keyring entry with the real backend
3. a build from another worktree that started with a missing key, generated a new one and stored it
4. a torn write

The log has a hole from 11-01 12:21Z to 11-02 09:10Z, so the writer is unknown.

Safe recovery (explanation only; nothing was touched):
1. Stop NEXUS.
2. Copy `memcore.enc` somewhere safe.
3. Do not let the app "repair" by overwriting. Check whether any running code path would `flush_now` a fresh empty store over it. `open` returns `Err` before `flush_now` on decrypt failure, so the file is currently preserved.
4. Look for an older key: Credential Manager keeps only the current value, so look for a backup or another Windows user profile.
5. If no key exists that decrypts the file, the sealed data is unrecoverable by design (crypto-erase). The clean option is then to rename `memcore.enc` aside, which lets the app start a new sealed store and re-import the legacy JSON. That is the owner's decision.

Add a startup line that logs a key fingerprint (a hash prefix, never the key) and the file's header and length, so the next mismatch can be attributed.

## D. Fixes, ranked (expected saving at logon)

1. **Never be silent for want of a clip** (saves the whole 30 s gap to first sound, typically 12 to 30 s). Do any of these:
   - render the hello clips at build time, or right after a voice or text change, instead of only at app start
   - in `run`, fall back to any cached hello line for the voice ("Good evening, sir" existed)
   - include the script lines in `CACHED_PHRASES` and warm them at the end of every run

   Also keep `last_played` across a version bump (`early_hello.rs:96`).
2. **Make give-ups visible**: `boot::mark` plus `tracing` for every `return false` (0 s, but it is how the next failure gets diagnosed).
3. **Take stage creation off the critical path for the hello.** Let the boot hello speak when the early hello gave up, without waiting for `stage_alive`; use the native early orb as the visual. Saves about 10 to 20 s on a contended logon.
4. **Do not block `setup` on WebView2.** Create the stage asynchronously, and run tray creation and anything else that talks to explorer off the main thread, or after the desktop is shown. This removes the "whichever step absorbs the stall" effect. Saving: the 7 to 18 s that currently hits the `stage_alive`/boot-hello chain.
5. **Cache "Welcome back, sir."** It is now cached. Synthesis on a cold logon cost about 1.8 s.
6. **EcoQoS during boot only.** Enable it after `boot_ready` instead of at `lib.rs:429`. The expected saving is unknown, from 0 to several seconds; the A/B test below decides.
7. Restore the memory store (C). This saves no time, but brings back the hello gap guard and session offers.

### A/B test for EcoQoS (owner runs it; nothing changed by me)

The kill switch is `NEXUS_NO_ECOQOS=1` (`power.rs:15`). The scheduled task inherits the user environment, so:

1. Set it as a **user** variable (System Properties → Environment Variables, or `setx NEXUS_NO_ECOQOS 1`).
2. Restart and log on.
3. Keep the same startup apps, the same power source (battery or AC, written down), and no rebuild in between. The first launch of a new binary is a confound.
4. Alternate B, A, B, A, B, A (at least 3 each). For A, remove the variable with `reg delete HKCU\Environment /v NEXUS_NO_ECOQOS` and sign out and in.
5. For each logon, record from `boot_timeline.jsonl`:
   - `stage_shown`, the tray delta, `setup_end`, `stage_alive` and `first_audio`
   - their offset from OS start (System event 12)
   - explorer's start time
6. Confirm in the log that each run printed `EcoQoS disabled by NEXUS_NO_ECOQOS=1` or `enabled`.

Decision rule: adopt "EcoQoS after boot" only if the median `setup_end` (offset from OS start) improves by at least 3 s and the ranges do not overlap. Otherwise contention from the shell and startup apps dominates, and fix 4 is the real one.
