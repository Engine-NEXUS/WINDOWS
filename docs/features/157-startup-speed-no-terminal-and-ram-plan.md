# Startup speed, no terminal at logon, and RAM: plan (nothing built yet)

Evidence: `research_notes/NEXUS startup and memory optimisation/startup_and_terminals.md` and `ram.md` (two read-only audits on Opus), plus my own re-checks marked CHECKED below.

## 1. The three terminals at logon (cause found)
At logon three scheduled tasks all fire and all three open a console window:
1. `\NEXUS` runs `nexus.exe --background`. The release exe is a console program: the attribute that hides the console (`windows_subsystem = "windows"`) is only in `lib.rs:7`, where Rust ignores it, and `main.rs` has none. CHECKED. A `nexus start` from the dev folder hides it because `run.ps1` starts it hidden, which is why it looks fine while we test.
2. `\NEXUS_STT` points at `C:\PROJECTS\ULTRON\server\start_stt.cmd`, which no longer exists (the project moved to D:). CHECKED: task exists, target missing. It fails with code 1 and the terminal stays open.
3. `\OpenClaw Gateway` (not NEXUS) points at a `gateway.cmd` that is also missing. CHECKED. Same effect.
Closing terminal 1 also kills NEXUS, which is why it must not exist.

### Fix
- **Now, by you (system config, so I did not touch it):** delete the two dead tasks:
  `schtasks /delete /tn NEXUS_STT /f` and `schtasks /delete /tn "OpenClaw Gateway" /f`. Removes two terminals.
- **Code:** move `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]` into `main.rs`. Then every program NEXUS starts that would open its own window must get `CREATE_NO_WINDOW`: first `lazy_stt.rs:240` (the Python speech server), then the version probes (`lazy_stt.rs:130/179`, `lazy_brain.rs:85`, `agent_watch.rs:111`, `github_data.rs:199`, `webhook.rs:428/462`, `command_executor.rs:262`). The list is from a quick scan; a manual pass over every `Command::new` is part of the work. With no console, add a log file so logon problems stay diagnosable.
- Stop re-registering the scheduled task with PowerShell on every boot (`lib.rs:589-644`); do it only when it is missing or the exe path changed. Also remove `NEXUS_STT` in the uninstaller.

## 2. Why the hello takes 10+ seconds (measured)
From `%APPDATA%/com.nexus.assistant/boot_timeline.jsonl`:

| | process start to ready (hello) | to wake word listening |
|---|---|---|
| Logon boots 10-28 to 11-01 | 24 s, 28 s, 27 s, 37 s, 34 s | 25 s to 42 s |
| Warm start from a terminal | about 4 s | about 4 s |

So at logon it is 25-37 s, not 10 s, and a warm start is 4 s. The hello is a cached clip on disk, so it is not network or voice generation. Speech is held until the overlay window is alive (a deliberate rule: no sound without the orb and caption), and the overlay is late because of the main thread:
- about 17 s of synchronous work in `setup()` after the overlay window is created (memory store open and decrypt with a Credential Manager read, action-kernel start, diary rollup, tray, permissions, audit). This split is inferred; there are no timing marks inside setup yet.
- WebView2 starting cold at logon, the overlay window created synchronously (5 s today, 13.8 s on 10-31).
- Efficiency mode (EcoQoS) switched on at `lib.rs:390`, the busiest moment (effect unmeasured).
- The wake engine thread starts at the very end of setup, so listening is late.
- Logon itself is busy: other startup apps compete for disk and CPU.

### Fix (ranked by expected saving)
1. **Add timing marks first** (end of setup, each heavy step, first audio played) so we stop guessing. Small, and it makes every later step measurable.
2. **Make `setup()` do only: create the overlay window, then return.** Run memory store, diary, action kernel, tray, audit and the rest in background tasks. Keep the ordering that matters (memory snapshot before the session recorder; volume restore before anything speaks). Inferred saving: 12-15 s.
3. **Start the wake engine at the top of setup**, so listening starts 5-15 s earlier.
4. **EcoQoS only after boot is ready** (test first with `NEXUS_NO_ECOQOS=1` on two logons).
5. **Create the overlay window off the critical path** where Tauri allows, and give the WebView2 browser a warm start (create it first, before the rest of setup).
6. **Owner decision:** play the cached hello before the overlay is alive. Fastest, but breaks the "no sound without the orb" rule. My recommendation: keep the rule and win the time in steps 2-5; target a hello within about 5 s of logon, then decide.

Target: logon hello in 5-8 s (from 25-37 s), wake word listening within about 10 s.

## 3. RAM (measured on the running release build, idle, 16:10 today)
- NEXUS is about 334 MB private (507 MB working set): nexus.exe 80 MB, WebView2 GPU process 101 MB, browser process 43 MB, two renderers about 85 MB, utility processes 26 MB. No Python helper and no local voice loaded.
- The dev launcher adds about 194 MB (`node nexus.mjs start` 134 MB, `cdp_monitor.js` 60 MB). An installed build would not have these. The debug port in `run.ps1:170` is also on by default.
- Two old notes are wrong: "idle 104 MB" and "each window costs about 250 MB" (windows share one browser and GPU process; an extra window costs 40-90 MB).

### Configurations
| | Changes | Saving | Cost to speed or features |
|---|---|---|---|
| Zero-risk | launcher exits after starting NEXUS and the debug tap becomes opt-in; flip `DEV_KEEP_WINDOWS_ALIVE` (`dyn_windows.rs:281`) so the hidden sidebar is really freed; share the speaker-check copy of the wake models (`wakeword_oww.rs:4475/4523`, Voice Lock is on); store the reply cache as 16-bit; stage window out of WebView2 low-memory mode during turns | dev setup about 235-300 MB; installed build about 45-100 MB | none; the stage change should help the orb's first frame |
| Balanced | unload local NLU after about 15 min idle (the wake-time prewarm should hide the reload); one thread for small speech models; idle kill for the admin brain | about 45-60 MB more | short reload after long idle, unmeasured |
| Aggressive | fewer orb particles and a lower-resolution stage canvas; WebView2 memory flags; shorter Kokoro grace period; one memory trim after long idle | about 50-150 MB more | visual and support trade-offs; Microsoft says not to rely on browser flags in production |
Rejected even for aggressive: turning off the Kokoro memory arena (saves about 100 MB, about 34 % slower) and the smaller local speech model (loses accuracy).

Recommendation: Zero-risk now, Balanced after measuring, Aggressive only if a target machine needs it.

## 4. Build order
1. Timing marks and a boot log file (also needed once the console is gone).
2. `main.rs` console attribute + `CREATE_NO_WINDOW` on every spawned program + task re-registration only when needed. You delete the two dead tasks.
3. Slim `setup()` + wake engine first + EcoQoS after ready. Measure three real logons against the table above.
4. RAM Zero-risk set; re-measure with the same protocol (private bytes of all NEXUS processes at idle after 10 minutes, and time to hello).
5. RAM Balanced set, behind a setting, after step 4 numbers.

## 5. Not verified
When the hello was actually spoken on a logon (the boot lines only go to the console); the split of setup time between steps; what EcoQoS does at startup; whether the low-memory stage slows the orb; per-model memory inside nexus.exe; whether `nexus_nlu.onnx` and its `.data` file are duplicates (disk size only).

## 6. Decisions needed
1. OK to delete the two dead scheduled tasks yourself (commands above)? `OpenClaw Gateway` is not part of NEXUS.
2. Keep the "no sound before the orb is alive" rule (recommended) or play the cached hello immediately?
3. Start with RAM Zero-risk only, or Zero-risk plus Balanced?
