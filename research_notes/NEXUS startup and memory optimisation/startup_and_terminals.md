# NEXUS logon start-up: the three terminals and the slow hello

Read-only audit, 2026-11-01. Nothing was started, stopped, built, edited or reconfigured.
Tags: **VERIFIED** = file:line or command output seen in this audit. **INFERRED** = reasoned from verified facts, not observed directly.

---

## 1. Facts: how NEXUS starts at logon, and where the three terminals come from

### 1.1 What starts at logon

| Item | What it launches | Status |
|---|---|---|
| Scheduled task `\NEXUS` | `D:\ULTRON\src-tauri\target\release\nexus.exe --background`. Logon trigger for this user, RunLevel Limited, Interactive logon, Priority 4, no time limit, no delay. | VERIFIED (`Get-ScheduledTask` / `schtasks /query /xml`) |
| Scheduled task `\NEXUS_STT` (Hidden=True only hides it in the Task Scheduler UI) | `\\?\C:\PROJECTS\ULTRON\server\start_stt.cmd`, logon trigger, Priority 7 | VERIFIED. The target file **does not exist** (`Test-Path` = False). The project moved from C: to D: on 2026-10-17. No current code registers this task: it is left over from the old STT auto-starter (`docs/changes/25-stt-server-auto-start.md`). The only references left are `.gitignore:66-69` and `AGENTS.md`. |
| Scheduled task `\OpenClaw Gateway` (not NEXUS) | `C:\Users\Chitkul Lakshya\.openclaw\gateway.cmd`, logon trigger | VERIFIED. The target file **does not exist**. |
| HKCU/HKLM `Run`, Startup folders | No NEXUS entry: OneDrive, Lists, Figma Agent, BlueStacks Services, SecurityHealth, Realtek, AnyDesk.lnk | VERIFIED (`reg query`, `Get-ChildItem`) |

At 2026-11-01 11:36:12 all three tasks ran in the same second and all three report `LastTaskResult = 1`. Kernel boot was at 11:35:51. VERIFIED (`Get-ScheduledTaskInfo`, `Win32_OperatingSystem.LastBootUpTime`).

Code that registers `\NEXUS`:
- `src-tauri/src/lib.rs:589-644` re-registers the task on every primary start, in a background thread. It runs `reg delete` and then PowerShell `Register-ScheduledTask ... -Priority 4`. Both are spawned with `CREATE_NO_WINDOW` (lib.rs:595, 613, 640). VERIFIED
- The same code also lives in `commands.rs:2991-3056` (`set_autostart`), `tray.rs:108-124`, and the installer uninstall step at `installer/nexus-installer.nsi:1087`. VERIFIED
- Nothing registers or removes `NEXUS_STT`. The uninstaller only removes `NEXUS`. VERIFIED (grep)

### 1.2 Terminal 1: nexus.exe itself is a console program

- The release exe's PE header says Subsystem = **3 (Console)**. VERIFIED: read from `D:\ULTRON\src-tauri\target\release\nexus.exe` (built 2026-11-01 15:57).
- The attribute that should hide the console is in the wrong crate. `src-tauri/src/lib.rs:7` has `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`. `windows_subsystem` only takes effect at the root of the **binary** crate. In a library it is silently ignored. `src-tauri/src/main.rs` is only `fn main() { nexus_lib::run() }` and has no attribute. VERIFIED (code). The ignore rule is Rust behaviour, and the PE header above confirms it.
- When the task starts a console program that has no parent console, Windows gives it a new console window. On this PC that is Windows Terminal: v1.24 is installed and the default-terminal delegation is "let Windows decide". VERIFIED (`Get-AppxPackage`, `HKCU\Console\%%Startup`).
- Closing that window kills NEXUS. INFERRED (normal console behaviour).
- During development the window never shows. `scripts/run.ps1:172-175` starts nexus.exe with `-WindowStyle Hidden` and redirects stdout and stderr to a log file. VERIFIED. This is why only the logon start shows it.

### 1.3 Terminals 2 and 3: the two dead `.cmd` tasks

- Task Scheduler runs a `.cmd` action through `cmd.exe`, which opens a console window.
- Both target files are missing, so `cmd` prints "The system cannot find the path specified" and exits with code 1. That matches `LastTaskResult = 1` on both tasks. VERIFIED (result codes, missing files). The cmd error text itself was not seen: INFERRED.
- Windows Terminal's default `closeOnExit` keeps a tab or window open when the process exits with a non-zero code. It shows "[process exited with code 1]" until the user closes it. So both error windows stay on screen. INFERRED: WT default behaviour; `settings.json` has no `closeOnExit` override (VERIFIED by grep).
- **Conclusion (INFERRED, strong):** the three windows are (1) nexus.exe's own console, (2) cmd.exe for the dead `NEXUS_STT` task, and (3) cmd.exe for the dead `OpenClaw Gateway` task. Windows 2 and 3 are not created by NEXUS code. One of them does not belong to NEXUS at all.
- Not the cause:
  - NEXUS's own child processes. While nexus.exe has a console, any child started without `CREATE_NO_WINDOW` shares that same console and opens no new window. Children started with the flag get no console at all.
  - BlueStacks Services (HKCU Run) does start `cmd /c tasklist` children with their own conhost (VERIFIED in the process list). These are short flashes, not lasting windows. INFERRED.

### 1.4 Children that will open their own windows once nexus.exe is GUI-subsystem

When nexus.exe is a GUI program, every console child started **without** `CREATE_NO_WINDOW` gets its own visible console. Today these are hidden only because they borrow nexus.exe's console.

A heuristic scan found these sites with no `creation_flags` within 25 lines (Windows-relevant ones only; the many macOS/Linux-only sites are omitted):

- **`lazy_stt.rs:130`, `:179`, `:240`.** `python --version` probes plus the long-running `stt_server.py` spawn. VERIFIED by reading lines 120-298. The `:240` spawn is the most visible one: a persistent Python console window. It can start at boot through `lazy_stt::spawn_prewarm` (`lib.rs:939`) for users who run local STT.
- **`lazy_brain.rs:85`.** Python version probe.
- **`agent_watch.rs:111`.** Node version probe.
- **`github_data.rs:199`.** `gh auth token`. VERIFIED, the call is on one line.
- **`webhook.rs:428`** (`curl.exe --version`) and **`webhook.rs:462`**. VERIFIED (inline).
- **`command_executor.rs:262`.** `taskkill`. VERIFIED (inline).
- **Python sidecars:** `brain_monitor.rs:365`, `browser_center.rs:80`, `system_center.rs:132`, `youtube_center.rs:134`, `orchestrator.rs:6328`, `orchestrator.rs:7502`, `memcore/leases.rs:102`.
- **`media_player.rs:325`.** `reg`.
- **`action_kernel/paths.rs:253`, `:274`.** `cmd`.
- **`coder/grounding.rs:241`.** `cmd /C`.
- **`coder/agents.rs:156`.**
- **`rea_browser.rs:62`, `:133`.** Node and Chrome.

Unless marked VERIFIED, these come from the 25-line heuristic and need a manual check. Sites that already set the flag include `lib.rs:294/595/613/640`, `commands.rs:3016/3040/3085/3183`, `agent_watch.rs:185/245`, `lazy_brain.rs:189`, `diagnostics.rs:375`, `wakeword_oww.rs:3810/3817` and `app_registry.rs:1368/1748` (VERIFIED by grep).

Opening a GUI app such as `notepad.exe` or a browser path needs no flag.

---

## 2. Start-up timeline

### 2.1 Today's logon boot: pid 11712, 2026-11-01

Source: `%APPDATA%\com.nexus.assistant\boot_timeline.jsonl`, written by `boot::mark` (`boot.rs:137-157`). `t_ms` counts from the kernel's process-creation time. MEASURED unless marked otherwise.

| Clock | t (s) | Step | Duration | What runs in this step |
|---|---|---|---|---|
| 11:35:51 | — | kernel boot | | |
| 11:36:12.1 | 0.0 | task starts nexus.exe | | process created |
| 11:36:16.8 | **4.66** | `main` | **4.7** | before `main` runs: loading the 105 MB exe and DLLs while every logon app starts at once. Defender scanning is likely (INFERRED). Warm starts take 0.04-0.3 s. |
| 11:36:16.9 | 4.76 | `cleanup_done` | 0.1 | WebView2 profile check (`lib.rs:237-261`). Resets only when the build type changed. |
| 11:36:19.5 | 7.37 | `setup_begin` | **2.6** | Tauri builder and plugins (`lib.rs:392-470`). Warm: 0.06-0.1 s. |
| 11:36:24.6 | 12.50 | `stage_shown` | **5.1** | `stage_show_sync` (`lib.rs:486`): synchronous creation of the WebView2 environment and window. Warm: 0.5-0.8 s. On 10-31 it took 13.8 s. |
| ~11:36:42.5 | ~29.4–30.4 | setup reaches `start_boot_briefing` (`lib.rs:810`) | **~17** | INFERRED from `boot_ready`: "waited 4s incl 1s settle" puts the task's start between 29.35 and 30.35. This is the rest of `setup()` running **synchronously on the main thread**. |
| 11:36:45.4 | 33.28 | `stage_alive` (first frontend heartbeat) | 20.8 since shown | The frontend sends its first beat 500 ms after mount (`frontend/src/stage/main.tsx:231-237`). INFERRED: the WebView cannot finish loading while `setup()` holds the main thread, so it comes alive about 3 s after setup returns. |
| 11:36:46.5 | 34.35 | `boot_ready` | 1.1 | `wait_until_ready` plus `SETTLE_SECS = 1` (`memcore/briefing.rs:23-26, 522-529`). |
| ~11:36:47 | ~34.8–35.5 | **hello spoken** | ~0.5–1 | INFERRED, not logged. The user counts as present (typed the password; `PRESENT_WITHIN_SECS` = 300, `briefing.rs:29,620`). Then `submit_hello` (`briefing.rs:544-564`), the High-urgency policy (speaks unless the user or NEXUS is speaking, or a request is running; `proactive_policy.rs:135-147`), a 300 ms heads-up (`proactive_policy.rs:59,471`), `speak_line`, and the frontend playing the cached clip. The clip "Good morning, sir." is on disk, as are all 33 stock lines (VERIFIED in the dev log: "stock lines ready in 0.03s (33 from disk)"). On a memory miss, `speak_text` reads the disk cache (`tts.rs:618`). |
| 11:36:54.6 | 42.45 | `wake_capture_started` (wake word live) | 8.1 after boot_ready | The wake-engine thread is spawned only at the end of setup (`lib.rs:898-905`). In this dev run the ONNX models loaded in 0.1 s (VERIFIED, dev log). The rest is the audio-capture start and retry (`wakeword_oww.rs:3637-3643`). |

### 2.2 The pattern is the same on every logon boot (MEASURED)

| Boot | before `main` | builder | stage window | setup end → alive (INFERRED) | to `boot_ready` (≈ hello) | to wake live |
|---|---|---|---|---|---|---|
| 11-01 11:36 | 4.7 | 2.6 | 5.1 | ~17 s of setup | **34.4 s** | 42.5 s |
| 10-31 11:58 | 3.0 | 0.6 | 13.8 | ~17 s | **36.8 s** | 42.3 s |
| 10-30 19:59 | 4.8 | 0.9 | 4.9 | ~15 s | **27.0 s** | 31.9 s |
| 10-29 11:48 | 3.6 | 0.7 | 7.0 | ~15 s | **27.9 s** | 28.5 s |
| 10-28 18:18 | 3.1 | 0.6 | 4.2 | ~15 s | **23.8 s** | 25.0 s |
| warm dev start (11-01 15:57) | 0.1 | 0.07 | 0.5 | ~2 s | **4.0 s** | 4.1 s |

At logon, NEXUS needs **24–37 s from process start to the hello** and 25–42 s until it is listening. The owner's "about 10 s" is probably the time from the desktop appearing, which itself shows up 10-20 s after the task fires. INFERRED. Warm starts take 3.5-6 s.

### 2.3 Critical path to the first hello

The hello waits for the stage. This is deliberate: sound must never play without the orb and caption (AGENTS.md "Caption/Orb/Sound Sync Audit", `ttsPlayer.ts` speak() forces the orb visible). So **stage_alive is the critical path**, and it is held back by:

1. **About 15-17 s of synchronous `setup()` on the main thread after the stage is created** (`lib.rs:491-1111`). This is the largest item, INFERRED from arithmetic on five boots. Suspects, none individually timed:
   - the memcore sealed-store open and decrypt, with a Credential Manager key read, during `snapshot_at_boot` / `session::at_boot` (`lib.rs:785-790`)
   - `action_kernel::startup` (trash age purge and audit prune, `lib.rs:792`)
   - `diary::log_boot_rollup` (`lib.rs:782`)
   - `tray::setup` (`lib.rs:662`)
   - `mic_permissions::init` (`lib.rs:761`)
   - `run_full_system_audit` (`lib.rs:493`, file checks only: `diagnostics.rs:593+`)
   - `volume::init`, `focus_throttle::recover_on_start`, `device_activity::start`, `api_keys::scrub_settings_file` (`lib.rs:498-501`)
   - `tts::init_disk_cache` (`lib.rs:781`)
   - roughly 30 `start_*` spawns

   No marks exist between `stage_shown` and setup's end, so the split is unknown.
2. **WebView2 cold creation at logon:** 4-14 s inside `stage_show_sync` (MEASURED), and that also runs on the main thread.
3. **4-5 s before `main`** while every logon app competes for disk and CPU (MEASURED, cause unknown).
4. **EcoQoS (efficiency mode) is switched on before anything else** (`lib.rs:390`, `power.rs:7-26`), at the most contended moment of the session. Its effect on boot is unmeasured. A kill switch exists: `NEXUS_NO_ECOQOS=1` (`power.rs:13-16`).
5. The autostart thread starts PowerShell on every boot to re-register an unchanged task (`lib.rs:589-644`). It is off the main thread but adds logon CPU contention (~2 s, measured earlier in docs/features/123 §1.2).

Not on the critical path (VERIFIED): the hello needs no network (cached clip); the Edge pre-generation reads the disk; the calendar, session-offer and health checks run after the hello (`briefing.rs:628-670`); diagnostics run in a separate thread after a 5 s wait (`lib.rs:1087-1109`); STT and NLU are lazy.

---

## 3. Fixes, ranked

Savings are for the logon case. Everything is INFERRED until re-measured with `boot_timeline.jsonl`.

| # | Fix | Expected saving | Risk | Files |
|---|---|---|---|---|
| 1 | **Delete the two dead logon tasks**: `schtasks /delete /tn NEXUS_STT /f` and `schtasks /delete /tn "OpenClaw Gateway" /f`. The owner runs this (it changes system config). Also add `Unregister-ScheduledTask NEXUS_STT` to the uninstaller and to the autostart thread. | Removes terminals 2 and 3 | None. Both targets are already missing. OpenClaw is a third-party leftover; confirm the owner no longer uses it. | machine; `installer/nexus-installer.nsi:1087`; `lib.rs:589` |
| 2 | **Put `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` in `src-tauri/src/main.rs`** and remove the ineffective copy at `lib.rs:7`. **Do it together with:** (a) add `CREATE_NO_WINDOW` to every console child in §1.4, at least `lazy_stt.rs:240` and the `--version` probes; (b) log to a file (e.g. `%APPDATA%\com.nexus.assistant\nexus_autostart.log`, rotated), because a GUI process has no stdout when nobody redirects it. That also gives the first real log of an autostart boot (docs/features/123 cause I). | Removes terminal 1 (the permanent one) | Medium. Without (a), several new pop-up consoles appear. Without (b), autostart logs vanish. `nexus start` is unaffected: run.ps1 redirects stdout and stderr to files (`run.ps1:172-175`), and a GUI process inherits redirected handles. | `main.rs`, `lib.rs:7`, the files in §1.4, tracing init at `lib.rs:351-354` |
| 3 | **Shrink `setup()` on the main thread to "create the stage, then return".** Move everything after `stage_show_sync` that is not needed before the event loop into `spawn` / `spawn_blocking`: memcore snapshot and session at_boot, action_kernel, diary, audit print, volume and focus recovery, registries. Keep only `app.manage(...)` and the handle registrations that must exist before the first IPC call. First add marks `setup_after_audit`, `setup_after_memcore` and `setup_end` to confirm the split. | **~12-15 s** to stage_alive, and so to the hello | Medium. Ordering dependencies: `snapshot_at_boot` must come before `resume::start_recorder`; `session::at_boot` before `session::start_recorder` (AGENTS.md pitfall); `mic_permissions::init` needs the stage window; `volume::init` must run before anything speaks. Keep those pairs ordered inside one spawned task. | `lib.rs:491-1111`, `boot.rs` (marks) |
| 4 | **Spawn the wake-engine thread at the top of `setup()`**, not at `lib.rs:898`. It needs only the AppHandle plus the meeting state (`set_meeting_state`, `lib.rs:690`): create the meeting state first. | ~5-15 s earlier "listening" at logon (42 s → close to stage_alive) | Low-medium. The grace-period reset happens right before capture (`wakeword_oww.rs:3631-3635`). A wake that fires before the stage is alive must wait; `stage::wait_until_ready` exists for this (`stage.rs:305`). | `lib.rs:688-690, 897-905` |
| 5 | **Turn EcoQoS on after `boot_ready`** instead of at `lib.rs:390`. Optionally raise the task priority from 4 to 3 (AboveNormal) for logon. First A/B test it with `NEXUS_NO_ECOQOS=1` on two logons. | Unknown, est. 1-5 s across all phases | Low. This is a battery-saving decision (docs/changes/101); it stays, but is applied later. | `lib.rs:390`, `power.rs`, the task XML in `lib.rs:605` / `commands.rs:3009` / `tray.rs:108` |
| 6 | **Re-register the task only when it is missing or the exe path changed** (docs/features/123 P1.2 asked for this; only the background-thread half was built). Check with a cheap `schtasks /query /tn NEXUS /xml` (no window) or the COM API before starting PowerShell. | Less logon CPU contention, ~1-2 s est. | Low | `lib.rs:589-644` |
| 7 | **Persist the missing marks:** `setup_end`, `hello_submitted`, `first_audio_out`, `mic_live`, and logon time from the WTS session start. Otherwise none of the above can be verified. | 0 (enables the rest) | None | `boot.rs`, `briefing.rs:556`, `tts.rs` playback start |
| 8 | **Decision for the owner, not a code fix:** play the cached hello from Rust (rodio) before the stage is alive, with the orb and caption joining when ready. | Up to ~20 s on top of #3 | Breaks the 2026-10-15 rule that sound never plays without orb and caption. Needs explicit owner approval. | `briefing.rs`, `tts.rs` |
| 9 | **Time before `main` (4-5 s):** smaller binary (strip debug info / LTO; 105 MB now). A Defender exclusion for the exe is a security setting: owner only, not recommended by default. | 1-3 s est. | Low (build) / security trade-off (exclusion) | `Cargo.toml` profile |

With #1 + #2 + #3 + #4, a logon should be invisible, with hello and listening at roughly stage-cold time, ~12-18 s after process start instead of 24-37 s. That is an estimate. Hitting the owner's "immediate" also needs #8, or a WebView2 that starts warm.

---

## 4. What could not be verified

- **The three windows were not seen on screen.** The mapping in §1.3 comes from task results, missing files, the PE subsystem and Windows Terminal defaults. The Task Scheduler operational log is disabled (no events returned). The logon nexus.exe (pid 11712) is no longer running, so its console could not be inspected.
- **Why `NEXUS` reported result 1.** It probably exited when `nexus start` / run.ps1 replaced it later in the day. INFERRED.
- **When the hello was actually spoken** on any logon. The `[BOOT] hello` line goes only to the console's stdout, so no log of it exists for autostart boots.
- **How the ~15-17 s of synchronous setup splits across steps**, and whether it really blocks WebView2's completion. Inferred from `boot_ready`'s "waited Ns" plus `stage_alive` on five boots; no marks exist in between.
- **The effect of EcoQoS**, and whether it reaches the WebView2 children. In the dev run the WebView2 children show Normal / AboveNormal priority class (VERIFIED). EcoQoS state is not visible through PowerShell.
- **The unflagged child list in §1.4** comes from a 25-line heuristic scan. Only the entries marked VERIFIED were read.
- **Whether the hello cache key matches.** `hello_line` returns "Good morning, sir." and `insert_cached` stores both forms, with and without the period (`tts.rs:434-437`), so it should hit. It is unconfirmed for the friend persona ("Good morning, Lakshya.") or a stored profile name, which are not in `CACHED_PHRASES`. They would be persisted after their first synthesis.
- **Prior decisions kept:**
  - hello on every logon, not twice within 2 h (`briefing.rs:30-32`)
  - presence gate of 5 min
  - nothing before the password (docs/features/123 §3)
  - lazy STT/NLU
  - sound never without the orb (AGENTS.md)
  - the owner controls runtime and builds, so every fix above is an edit for the owner to build and test.
