# 7. Focus Mode (Hand Mode load control)

Scorecard before: Built 90, Tested 75, Live 0, Safety 80, Industry 20 (2.5 stars).

## What it is

While Hand Mode (the camera pointer) runs, NEXUS gets out of its own way and out of the way of the pointer: its own background loops pause (`focus_mode.rs`, a lease with a 10 s watchdog) and, on request, other heavy user apps are made less demanding (`focus_throttle.rs`): up to 6 non-foreground processes above 15 % CPU get `BELOW_NORMAL` priority and Windows Efficiency Mode (EcoQoS), and are restored exactly afterwards, also after a crash (`focus_baseline.json`). Settings `handFocusMode` and `handFocusLowerApps`, both default on.

## Verification (what was found)

Baseline: 21 tests pass, but they covered the **decision** (which processes are eligible: never the foreground app, NEXUS and its children, system / security / meeting / media apps, elevated or other-user processes) and the baseline file. **Nothing ever applied the change to a real process.** The three Win32 calls that do the work (`SetPriorityClass`, `SetProcessInformation` for EcoQoS, and the restore) had no test at all, so "lowers and restores exactly" was a claim.

## What was built

`focus_throttle::real_process_tests` (4 tests, Windows only). Each test **spawns its own child process** (`ping.exe` waiting on localhost, no window) and never touches any app of the owner's:

1. `lower` really drops the child from NORMAL to BELOW_NORMAL, and turns EcoQoS on (read back with `GetPriorityClass` and `GetProcessInformation`); lowering it again is refused (an already-low process is left alone, so a restore never "restores" our own change); `restore` puts the class **and** the efficiency state back to exactly what they were.
2. `restore` refuses a process whose creation time no longer matches (a reused PID) and leaves it unchanged.
3. A dead process cannot be lowered or restored and nothing panics; pid 0 and NEXUS itself are never lowered. (First draft of this test was wrong: a process object lingers while any handle to it is open, so the test now drops the child first.)
4. The real process list (via `sample_processes`) contains this test process and never proposes it, nor its own child, as a target; in a meeting nothing is proposed.

Three mutations are in the index: restore no longer checks the creation time, lowering an already-lowered process is allowed, restore no longer puts the priority back. All are caught by these tests, none by the old pure tests.

## Finding left open (advisor)

`restore` writes back the priority it recorded even if the **user changed it meanwhile** (for example set an app to High in Task Manager during Hand Mode). Minor: it would need a compare with the current value before writing. Not changed.

## Not done, and why

* **No Task Manager before / during / after on the owner's real heavy apps** and **no Hand Mode lag baseline** (the plan's A4 tool `scripts/hand-focus-report.mjs` compares `[HAND]` logs with and without Focus Mode but there are no real logs yet). Hand Mode needs the camera.
* The GPU share is not controlled (Windows exposes no API for it from this layer); the earlier GPU-contention notice covers that case separately.
* Industry (20) is low because commercial "game mode" tools do much more; that is a product decision, not a defect.

## Owner checklist

1. Run Hand Mode for 2 minutes with Chrome playing a heavy page. In Task Manager, Details tab: Chrome's heavy process shows priority "Below normal" and the leaf icon (Efficiency mode).
2. Say "exit hand mode". Expected: the priority returns to "Normal" and the leaf disappears within a second.
3. Kill NEXUS in Task Manager while Hand Mode is on, then start it again. Expected: the app you lowered is restored at start (log: `focus_throttle: recovered`).
4. Meeting running (Zoom or Teams): nothing is lowered.
5. With and without `handFocusMode`, read the `[HAND]` summary line (`infer_p50`, `busy_skips`) and run `node scripts/hand-focus-report.mjs`.
