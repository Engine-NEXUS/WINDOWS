# 163 — Focus Mode Lowers Heavy Background Apps During Hand Mode (2026-10-25)

## Overview

When Hand Mode (camera pointer and spatial gestures) is active, NEXUS now lowers the CPU scheduling priority and efficiency class of other heavy user applications so the camera inference pipeline and mouse cursor tracking remain smooth and jitter-free. When Hand Mode disengages, every modified process is restored **EXACTLY** to its original priority class and power throttling state, even after an unexpected app crash or kill.

Nothing is closed, killed, or suspended.

## How It Works

### 1. Pure Decision Engine (`src-tauri/src/focus_throttle.rs`)
- Implemented `decide(samples: &[ProcSample], cfg: &Config, meeting_active: bool) -> Vec<u32>` with zero OS dependencies for thorough unit testing.
- Target candidates must exceed `cfg.cpu_threshold` (default `15.0%`) and are sorted descending by CPU consumption up to `cfg.max_targets` (default `6`).

### 2. What Is Never Lowered (Exclusion Rules)
- **Active meetings**: If `meeting_active` is true (`meeting_detect::MeetingState::is_meeting_active`), nothing is lowered at all.
- **Foreground process family**: The active foreground window process (`GetForegroundWindow` + `GetWindowThreadProcessId`) and all of its child and descendant processes are never touched.
- **NEXUS process family**: NEXUS itself (`nexus_pid`) and all its child and descendant processes (such as WebView2 runtime and GPU helper processes) are never lowered.
- **Elevated and other-user processes**: Processes running elevated (admin) or under different user accounts / system services are excluded.
- **Low CPU usage**: Any process below the CPU threshold (15.0%) is ignored.
- **System & Shell processes**: `system`, `idle`, `registry`, `smss.exe`, `csrss.exe`, `wininit.exe`, `winlogon.exe`, `services.exe`, `lsass.exe`, `svchost.exe`, `dwm.exe`, `explorer.exe`, `audiodg.exe`, `ctfmon.exe`, `fontdrvhost.exe`, `searchhost.exe`, `sihost.exe`, `taskhostw.exe`, `runtimebroker.exe`.
- **Security software**: `msmpeng.exe`, `mpdefendernetworkservice.exe`, `securityhealthservice.exe`, and any process whose executable name contains `defender`, `avast`, `kaspersky`, `norton`, `mcafee`, `bitdefender`, `malwarebytes`, `crowdstrike`, or `sentinel`.
- **Call, meeting, capture, and media apps**: `zoom.exe`, `teams.exe`, `ms-teams.exe`, `webex.exe`, `skype.exe`, `discord.exe`, `obs64.exe`, `obs32.exe`, `spotify.exe`, `vlc.exe`, `wmplayer.exe`, `music.ui.exe`.
- **NEXUS driver and assistant tools**: `claude.exe`, `agy.exe`.

### 3. Windows Priority & EcoQoS Glue
- **Lowering (`lower(pid)`)**:
  - Opens process with `PROCESS_SET_INFORMATION | PROCESS_QUERY_LIMITED_INFORMATION`.
  - Verifies process is not elevated via `OpenProcessToken` and `TokenElevation`.
  - Records process creation time (`GetProcessTimes` 64-bit FILETIME) and image name (`QueryFullProcessImageNameW`).
  - Records original priority class (`GetPriorityClass`) and power throttling configuration (`GetProcessInformation(ProcessPowerThrottling)`).
  - Sets priority class to `BELOW_NORMAL_PRIORITY_CLASS`.
  - Enables Windows EcoQoS (Efficiency Mode) by setting `ProcessPowerThrottling` with `PROCESS_POWER_THROTTLING_EXECUTION_SPEED`.
- **NEXUS Self-Elevation (`raise_nexus()`)**:
  - Elevates NEXUS main process to `ABOVE_NORMAL_PRIORITY_CLASS` while Hand Mode is engaged so the camera frame pipeline takes precedence over normal user tasks. Original priority is recorded in `Baseline.nexus`.
- **Exact Restoration (`restore(&LoweredProc)`)**:
  - Verifies target PID still has the exact same process creation timestamp to guard against PID reuse.
  - Re-applies original priority class via `SetPriorityClass`.
  - Re-applies original throttle state via `SetProcessInformation(ProcessPowerThrottling)` (control mask 0 and state mask 0 resets to system-managed default).
- **Handle safety**: All Win32 handles are wrapped with an RAII drop guard (`AutoHandle`) ensuring handles are closed even on error or early return.

### 4. Durable Baseline & Crash Recovery
- **Durable file**: Baseline written to `focus_baseline.json` in the application data directory.
- **Write-before-change**: The baseline file is written atomically (temp file + rename) **BEFORE** the first process is altered and updated immediately after each successful lowering.
- **Order preservation**: Processes are restored in reverse order of lowering (last lowered restored first; NEXUS main process restored last).
- **Crash recovery (`recover_on_start(dir)`)**:
  - On application startup, if `focus_baseline.json` exists from an earlier crash or sudden power loss, `plan_restore` matches the durable baseline against currently running processes.
  - Stale/dead processes and reused PIDs are safely skipped; living lowered processes are restored to their pre-crash state, and `focus_baseline.json` is purged.
- **Clean exit (`restore_now()`)**: Called synchronously in `lib.rs` under `tauri::RunEvent::Exit` adjacent to `focus_mode::force_release("app_exit")`.

### 5. Settings and UI Integration
- **Backend setting**: Added `hand_focus_lower_apps` (defaults to `true`) to `NexusSettings` (`src-tauri/src/commands.rs`) with serde camelCase serialization (`handFocusLowerApps`).
- **Frontend toggle**: Added toggle row in `frontend/src/settings-sidebar/HandModeSection.tsx` and updated `frontend/src/settings-sidebar/SettingsSidebarApp.tsx`.

## Honest Limits

- **CPU scheduling only**: This lowers CPU scheduling priority classes and applies Windows EcoQoS (E-core scheduling affinity). It cannot alter GPU engine queues or VRAM allocations.
- **Exclusive-fullscreen / elevated games**: Heavy apps running as Administrator (elevated) or using exclusive display driver privileges cannot and will not be touched by standard user privileges.
- **System-managed throttling fallback**: When restoring a process that was not previously under EcoQoS override (control mask 0), Windows returns the process to OS-managed heuristics.

## Verification Commands for Reviewer

```bash
# Rust syntax and unit tests
cargo check --lib
cargo test --lib focus_throttle
cargo test --lib commands::nexus_settings_tests::hand_focus_lower_apps_defaults_to_true_and_round_trips

# Frontend typecheck
npm --prefix frontend run typecheck
```

## Not Run Live

- Physical Hand Mode session with a real camera and a live heavy workload (e.g., Blender rendering or video compile).
- Visual verification in Windows Task Manager of target process priority dropping to "Below normal" and Status badge showing "Efficiency mode".
- Verification of priority restoration following a simulated hard kill (`taskkill /F /IM nexus.exe`) followed by restart.
