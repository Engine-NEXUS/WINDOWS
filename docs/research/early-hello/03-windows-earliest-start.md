# 03 — Starting NEXUS as early as possible on Windows 11 (logon, resume, lid open)

Date: 2026-11-02. Machine: LAKSHYA-LAPTOP, Windows 11 Home Single Language 10.0.26300, Modern Standby (S0 Low Power Idle), no S3.
Scope: research only. Nothing on the machine was changed; only read-only commands were run.

Labels used below: **[sourced]** = documented at the linked URL; **[measured]** = read from this machine's event logs / NEXUS logs today; **[inferred]** = my reasoning, not proven.

---

## 0. Headline finding (read this first)

**The scheduled task is NOT the reason the first sound takes ~26 s.** It already starts NEXUS about 1 second after the PIN is accepted. The ~26 s is spent inside NEXUS's own startup, most of it waiting for parts of Windows (WebView2, the taskbar) that are still loading at logon.

Measured timeline for today's cold logon (2026-11-02, local time IST; NEXUS log is UTC = IST − 5:30) **[measured]**:

| Time | Offset from PIN accepted | Event | Source |
|---|---|---|---|
| 14:40:28 | — | Kernel started (cold boot, Fast Startup is off) | System, Kernel-General 12 |
| 14:40:56 | — | Authentication started (PIN typed) | Winlogon/Operational 1 |
| **14:40:57** | **0 s** | Authentication stopped, result 0 (user logged on) | Winlogon/Operational 2, User Profile Service 1/2 |
| 14:40:58.2 | **+1.2 s** | `nexus.exe` main() runs (task `NEXUS` fired) | boot_timeline `main`, process StartTime |
| 14:40:58.5 | +1.5 s | NEXUS starts creating the `stage` WebView2 window | nexus.log |
| 14:40:59 | +2 s | RunOnce runs the **WebView2 installer** `setup.exe --msedgewebview --delete-old-versions` in parallel | Shell-Core 9707 |
| 14:41:06.2 | +9.2 s | `stage` window created (**7.7 s** to create one WebView2 window) | nexus.log |
| 14:41:07.6 → 14:41:21.1 | +10.6 → +24.1 s | **13.5 s with no log line at all**; ends exactly at `setup_tray_done` | nexus.log, boot_timeline |
| 14:41:23 | +26 s | Explorer finishes logon tasks (`AllLogonTasks`, `NotifyTrayStartIsReady`) | Shell-Core 62171 |
| 14:41:23.6 | +26.6 s | Boot hello queued ("Good afternoon, sir.") | nexus.log |
| 14:41:24.2 | **+27.2 s** | Hello audio starts (cache hit, Edge voice) | nexus.log `tts:playback_started`, boot_timeline `first_audio` t=26.6 s |
| 14:41:28.9 | +31.9 s | Wake word listening | nexus.log |
| 14:41:50 | **+53 s** | Explorer starts the `Run` registry key apps (Realtek, OneDrive, BlueStacks…) | Shell-Core 9705/9707 |

For comparison, the previous NEXUS start in `boot_timeline.jsonl` (a start while the desktop was already up) reached `first_audio` at **5.6 s** and `stage_shown` at 0.86 s **[measured]**. Same binary, same code: the extra ~21 s at logon is contention with the shell.

What the 13.5 s gap is **[inferred, high confidence]**: the only thing between `setup_audit_started` and `setup_tray_done` that can block that long is `tray::setup()` (`src-tauri/src/lib.rs:707`, Tauri `TrayIconBuilder::build`, `src-tauri/src/tray.rs:170`), and it finishes 2 s before Explorer reports `NotifyTrayStartIsReady`. A notification-area icon is registered with the taskbar window (Shell_NotifyIcon → message to Explorer's tray); while Explorer is still initialising, that call waits. **Verify** by adding a `boot::mark("setup_before_tray")` just before line 707 and re-reading one logon.

Second contributor **[inferred]**: NEXUS turns on Windows EcoQoS ("Efficiency Mode") at the very first line of startup (nexus.log 09:10:58.207 "EcoQoS enabled for daemon"). Efficiency mode deliberately runs the process slowly on efficiency cores; doing that during the most contended 30 s of the day stretches every step.

**Consequence:** the cheapest big win is code inside NEXUS (speak first, build tray/stage later, EcoQoS after the hello), not a different trigger. A better trigger can save at most ~1 s at logon.

---

## 1. Every way to start code early in a user session

| Mechanism | When it runs (Win 11) | Runs in user session? | Can it play sound to the user? | Verdict for NEXUS |
|---|---|---|---|---|
| **Task Scheduler — At log on (LogonTrigger), no delay** | Immediately when the logon completes, before Explorer starts the Run keys. Measured here: +1.2 s after auth. | Yes, with `LogonType=InteractiveToken` | Yes | **Best for cold logon. Already in use.** [sourced: [LogonTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-type); measured] |
| Task Scheduler — At startup (BootTrigger) | When the machine boots, before anyone logs on | No (no interactive desktop; runs in session 0 or with no user) | No reliable path | Not usable for a per-user voice app [sourced: [BootTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/boottrigger-type)] |
| Task Scheduler — On workstation unlock / Session state change (`SessionStateChangeTrigger`: `SessionUnlock`, `ConsoleConnect`, `RemoteConnect`, `SessionLock`…) | When the user unlocks / connects to the console | Yes | Yes | Good as a **re-launcher** if NEXUS crashed; not needed while NEXUS is resident (it can watch the same events itself) [sourced: [SessionStateChangeTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/sessionstatechangetrigger-type)] |
| Task Scheduler — On an event (EventTrigger), e.g. Power-Troubleshooter 1, Kernel-Power 107/507 | When the event is written | Depends on principal | Yes if interactive | **Power-Troubleshooter ID 1 does not appear on this machine** (no such events found); Modern Standby writes Kernel-Power **506 enter / 507 exit**. Event triggers are also seconds late and fire at resume, not at unlock. [measured] |
| `HKCU\…\CurrentVersion\Run` / `RunOnce` | After Explorer starts; Windows waits for the shell and then serialises them. Measured here: **Run at +53 s**, RunOnce at +2 s | Yes | Yes | Much later than the task. The ~10 s "startup delay" (`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize\StartupDelayInMSec`, `WaitForIdleState`) applies to these and the Startup folder only [sourced: [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-delay-of-running-startup-apps-in-windows-11.9144/), [NinjaOne](https://ninjaone.com/blog/enable-or-disable-delay-of-running-startup-apps-in-windows-11), [Microsoft Q&A user answer](https://learn.microsoft.com/en-us/answers/a/7617892) — third-party/user-posted, not official]. **Not relevant to NEXUS** because the scheduled task is not subject to it. Do not change it. |
| Startup folder | Same as Run (after shell, delayed) | Yes | Yes | Same as Run — later |
| Winlogon notification packages / `Userinit` / `Shell` value | Notification packages were removed in Vista; `Userinit`/`Shell` replacement runs your code instead of/before Explorer | Yes | Yes | **Do not.** Removed API / security-sensitive values that malware abuses; a bug here breaks logon for the user [sourced: [Winlogon and credential providers](https://learn.microsoft.com/en-us/windows/win32/secauthn/winlogon-and-credential-providers)] |
| Windows service (session 0) | Before logon, from boot | No — session 0 isolation since Vista; services cannot show UI to the user | Officially no supported way to the user's speakers; anecdotes of WASAPI working only in exclusive mode [sourced: [Interactive services](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services), [Session 0 isolation (askperf)](https://techcommunity.microsoft.com/blog/askperf/application-compatibility---session-0-isolation/372361), [MS whitepaper PDF](https://coretechnologies.com/WindowsServices/Microsoft-Impact-of-Session-0-Isolation-on-Services-and-Drivers-in-Windows-Vista.pdf); anecdote: [SAPI in a service](https://windows-hexerror.linestarve.com/q/so24293808-simple-text-to-speech-with-sapi-in-a-windows-service)] |
| Service + `WTSQueryUserToken` + `CreateProcessAsUser` into the user session | Service gets `SERVICE_CONTROL_SESSIONCHANGE` (WTS_SESSION_LOGON) and launches a child in the user's session | Child: yes | Child: yes | Possible, but needs an **admin install**, a LocalSystem service, and buys at most ~1 s over a logon task. Not worth it on Windows 11 Home for one user. [sourced: [WTSQueryUserToken](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken), [CreateProcessAsUser](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw)] |
| Tiny resident tray helper vs. service | Helper = per-user process started by the logon task; lives in the user session | Yes | Yes | Equivalent to what NEXUS already is. A *separate* tiny helper only helps if the main app is slow to start (see §6). |

**Which can play sound before the desktop is fully loaded?** Any process started in the user session can play through the Windows Audio service as soon as it runs; Audio service starts at boot, long before logon **[inferred from the measured timeline: the hello was played via WASAPI/rodio with Explorer only just ready; nothing in the audio path waits for Explorer]**. The logon task, RunOnce and a service-launched child all qualify; Run keys and the Startup folder do not in practice (they wait for the shell).

**Practical earliest time after PIN/Hello is accepted** **[measured + inferred]**: the process is running at **+1.2 s**. If the first thing NEXUS did was play a cached greeting with no WebView, no tray and no network, sound could start around **+1.5 to +3 s** (inferred: loading a cached audio file and opening the default render endpoint takes well under a second on this machine when not contended; budget extra for logon contention). Getting below +1 s is not possible with any supported per-user mechanism.

---

## 2. Resume from Modern Standby and lid open

### What signals exist [sourced]

| Signal | What it means | Notes |
|---|---|---|
| `WM_POWERBROADCAST` / `PBT_APMRESUMEAUTOMATIC` | System resumed; **does not mean a user is present** | Sent on every resume, including unattended maintenance wakes. [PBT_APMRESUMEAUTOMATIC](https://learn.microsoft.com/en-us/windows/win32/Power/pbt-apmresumeautomatic) |
| `PBT_APMRESUMESUSPEND` | Resume caused by user input | On Modern Standby, reports say it is not reliably sent (S3 semantics). [PBT_APMRESUMESUSPEND](https://learn.microsoft.com/en-ca/Windows/win32/power/pbt-apmresumesuspend), [Q&A on Modern Standby events](https://learn.microsoft.com/en-us/answers/questions/882272/event-that-gets-triggered-on-modern-standby) |
| `RegisterSuspendResumeNotification` | Suspend/resume callbacks without a window | [doc](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registersuspendresumenotification) |
| `RegisterPowerSettingNotification(GUID_SESSION_DISPLAY_STATUS)` | **This session's** display on/off/dim — the right one for a user-mode app | "Sent only to user-mode applications" [Power Setting GUIDs](https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids) |
| `GUID_CONSOLE_DISPLAY_STATE` | Console display on/off/dim | Docs say interactive apps should prefer `GUID_SESSION_DISPLAY_STATUS` [same page] |
| `GUID_LIDSWITCH_STATE_CHANGE` | Lid opened (1) / closed (0) | "Callback won't be called until a lid device is found and its current state is known" [same page] |
| `GUID_SESSION_USER_PRESENCE` | User providing input in this session (present / not present / inactive) | User-mode only [same page] |
| `WTSRegisterSessionNotification` → `WM_WTSSESSION_CHANGE` (`WTS_SESSION_UNLOCK`, `WTS_SESSION_LOCK`, `WTS_CONSOLE_CONNECT/DISCONNECT`, `WTS_SESSION_LOGON`) | Unlock / fast-user-switch events for the process's own session | [doc](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification) |
| Event log: Kernel-Power 506/507 (enter/exit Modern Standby) | Written by the kernel | **Measured here**: 507 "exiting Modern Standby" at 15:17:09 today and on 01-11 21:23, 21:42, 22:13 |

### How NEXUS detects resume today [measured, code read]

`src-tauri/src/lib.rs:745-770` uses a **time-jump heuristic**: a thread sleeps 10 s and checks whether wall-clock advanced by more than 60 s. Today it fired at 15:17:11.77, about **2.7 s** after Kernel-Power 507 (15:17:09). That was partly luck:

- worst-case latency is ~10 s (the sleep period) plus Desktop Activity Moderator unthrottling;
- a standby shorter than 60 s is never detected;
- lid-open **without** standby (lid closed briefly, display only), unlock after a lock, and fast user switching are never detected;
- it cannot tell "machine woke for maintenance" from "user opened the lid".

On Modern Standby, desktop (Win32) apps are paused by the Desktop Activity Moderator while the screen is off and resumed when the system exits standby, so a resident process does receive window messages on resume; the messages queued for it are delivered once it is unpaused [sourced: [Prepare software for Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/prepare-software-for-modern-standby), [Modern Standby states](https://learn.microsoft.com/da-dk/windows-hardware/design/device-experiences/modern-standby-states)].

### What a user actually experiences after lid open [inferred]

Lid open → system exits standby (Kernel-Power 507) → display on (`GUID_SESSION_DISPLAY_STATUS`=On) → **lock screen** (sign-in is normally required after standby) → PIN/Hello → `WTS_SESSION_UNLOCK`. The greeting should be tied to **unlock** (the user is now proven present and is the owner of this session), with display-on as an early warm-up signal (restart the mic stream, pre-warm the Edge voice connection, load the greeting audio) so the hello is ready the instant unlock arrives. If the user has disabled "require sign-in after sleep", there is no unlock event: fall back to display-on + `GUID_SESSION_USER_PRESENCE`=Present, while the session is reported unlocked.

### Keep resident vs. cold start [measured + inferred]

Measured now (NEXUS running since 14:40:58, idle most of the time): process tree of 7 processes, working set **208 MB**, private **249 MB**, total CPU **166 s over ~55 min** (≈5 % of one core on average, mostly wake-word inference — this is the wake listener, not the cost of "being resident"). Cold start costs ~5.6 s to first audio on an idle desktop and ~27 s at logon (measured above). For resume and lid open, NEXUS is **already resident** (it is never closed), so resume greeting latency is purely event wiring: with the right events the hello can play within ~1 s of unlock **[inferred]**. Keeping it resident is clearly better; the RAM is already being spent for the wake word.

---

## 3. Multi-user and fast user switching

Facts **[sourced/measured]**:
- The task `\NEXUS` has a Principal with `UserId` = this user's SID and a LogonTrigger restricted to `LAKSHYA-LAPTOP\Chitkul Lakshya` **[measured]**. Another Windows user logging in does **not** start this user's NEXUS. If a second user installs NEXUS they get their own task, their own `%APPDATA%\com.nexus.assistant`, their own keychain entries.
- With fast user switching, the first user's session stays logged on but **disconnected**; their NEXUS keeps running. Session notifications `WTS_CONSOLE_DISCONNECT/CONNECT` tell each instance when it loses/regains the console [sourced: WTSRegisterSessionNotification above].
- Two NEXUS instances can both be alive (one per session). Single-instance guards must use a **per-session** name (`Local\…` mutex, not `Global\…`) or the second user's NEXUS will refuse to start **[inferred]**.

Audio and mic **[inferred, should be verified on the machine with two accounts]**: audio from a session that is not attached to the console is not heard on the local speakers (only the active console session's audio is routed to the local devices). The microphone can usually still be opened in shared mode by the disconnected session's process, which means **the first user's wake word could keep listening to the second user** — a privacy problem.

How NEXUS should behave:
1. On `WTS_CONSOLE_DISCONNECT` or `WTS_SESSION_LOCK`: stop the wake-word capture, stop TTS, drop pending proactive alerts to the card queue.
2. On `WTS_CONSOLE_CONNECT` / `WTS_SESSION_UNLOCK` for **its own session id** (`ProcessIdToSessionId(GetCurrentProcessId())` == the notification's session id == `WTSGetActiveConsoleSessionId()`): re-open the mic, then greet.
3. Never greet on `WTS_SESSION_LOGON` of another session; never greet from a disconnected session.
4. Use `Local\` named objects; keep all state under the user's `%APPDATA%`.

---

## 4. Speaking at the lock screen / before the password

Possible routes: a **Credential Provider** DLL (runs inside LogonUI as SYSTEM) or a **service** playing audio. Windows itself only offers the optional "Play Windows Startup sound".

- Technically: a credential provider can run code on the lock screen; it is loaded into LogonUI, so any crash or hang breaks sign-in for every user [sourced: [Credential providers in Windows](https://learn.microsoft.com/en-us/windows/win32/secauthn/credential-providers-in-windows)]. A service in session 0 has no supported path to the user's speakers (§1).
- Security/privacy: at the lock screen nobody is authenticated. A greeting by name, "you left Claude Code running in ULTRON", calendar items or unread mail would be read to whoever opened the lid. A listening microphone before sign-in is a classic attack surface (voice commands on a locked machine). It also needs admin install and code that runs as SYSTEM.
- Recommendation: **do not speak before unlock.** At most, NEXUS can prepare (warm the audio device, load the greeting) on display-on so it speaks instantly after unlock. If the owner ever wants a sound at the lock screen, use only a neutral, content-free chime — and still not from NEXUS code running as SYSTEM.

---

## 5. This machine (read-only inspection)

**Task definition** **[measured]** (`schtasks /query /tn NEXUS /xml`):
- Trigger: one `LogonTrigger`, `UserId` = `LAKSHYA-LAPTOP\Chitkul Lakshya`, no `<Delay>` (= 0). Good.
- Principal: user SID, `LogonType=InteractiveToken`. Good (runs in the user's desktop session).
- Settings: `Priority 4` (normal-ish; default is 7), `MultipleInstancesPolicy IgnoreNew`, `ExecutionTimeLimit PT0S` (never killed), `DisallowStartIfOnBatteries false`, `StopIfGoingOnBatteries false`, `StartWhenAvailable true`. An `IdleSettings` block is present but there is no idle condition, so it has no effect. No `RestartOnFailure`.
- Action: `D:\ULTRON\src-tauri\target\release\nexus.exe --background` (the dev build output folder; a rebuild in progress at logon would make the task fail — consider launching a copied/installed binary).

**Power / Fast Startup** **[measured]**: `powercfg /a` → Standby (S0 Low Power Idle) Network Connected and Hibernate available; S1/S2/S3/Hybrid unavailable; **Fast Startup: "disabled in the current system policy"**; `HiberbootEnabled = 0`. Every shutdown is a full cold boot: kernel start 14:40:28 → event log 14:40:42; the user then spends time at the lock screen. Fast Startup only shortens the pre-logon part and is policy-disabled — not something NEXUS should touch.

**Boot-time oddities seen** **[measured]** (pre-logon, do not affect the hello): `Intel(R) Platform License Manager` start timed out after 180 s (Service Control Manager 7009, 14:40:43), `PRI-Driver` failed to start (7000), one boot driver did not load (7026). These delay boot-start services, not the user session. Worth a look for overall boot health.

**Diagnostics-Performance boot/logon durations (event 100/200)**: the log requires admin to read (`wevtutil` → Access is denied). Not measured. The Winlogon + Shell-Core events above give the same logon picture without admin.

**Startup apps** **[measured]** (`Win32_StartupCommand`): HKCU Run — OneDrive, Microsoft.Lists, Figma Agent, BlueStacks Services; HKLM Run — SecurityHealth, RtkAudUService; Common Startup — AnyDesk. They start at **+53 s**, after NEXUS has already spoken, so they do not delay the first sound. The thing that **does** run alongside NEXUS at logon is the **WebView2 installer in RunOnce** (14:40:59–14:41:01, deleting old WebView2 versions) **[measured]**, at the same moment NEXUS creates its WebView2 window (7.7 s) **[inferred contributor]**.

**What would shave seconds, biggest first** (estimates are **[inferred]**, based on the measured timeline):

| Change | Where | Expected saving at logon |
|---|---|---|
| Play the boot hello first: a native path that needs no WebView, no tray, no network (cached audio file → rodio), started right after `main()`; build stage/tray afterwards | NEXUS code (`lib.rs` setup order, `memcore::briefing`) | ~20–24 s (hello at ~+2–4 s instead of +27 s) |
| Create the tray icon on a background thread or after Explorer's `TaskbarCreated` message; never block setup on it | `tray.rs` / `lib.rs:707` | up to ~13 s (the silent gap) |
| Create the `stage` WebView2 window lazily (after the hello) | `lib.rs` / `dyn_windows.rs` | ~7–8 s off the critical path |
| Turn on EcoQoS after the hello/boot_ready, not at line 1 | `power.rs` call site | unknown, likely a few seconds under contention |
| Wake-word models + mic after the hello (they already load in 0.9 s + 4.4 s; keep them off the hello path) | already mostly so | small |
| Trigger changes | task XML | ≤ 1 s (the task already fires at +1.2 s) |

---

## 6. Ranked recommendation

### Cold logon
1. **Keep the existing At-logon task** (no delay, InteractiveToken, this user only). It is already the earliest supported per-user start (+1.2 s measured).
2. **Reorder NEXUS startup so sound comes first** (§5 table). This is where the 26 s goes. Target: first sound ≤ 4 s after PIN accepted.
3. Optional alternative to step 2, if reordering Tauri setup is hard: a **tiny native "hello" executable** (pure Rust, no WebView, ~a few MB) started by the same logon task, which plays the cached greeting and exits, while the main app starts in parallel. Costs a second process for ~2 s; avoids touching Tauri's setup order. The main app must then skip its own boot hello for that logon (coordinate via a per-session file or `Local\` event).

### Resume from Modern Standby / lid open / unlock (NEXUS is already resident)
4. In the resident process, register (once, on the existing hidden/stage window or a message-only window):
   - `WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)` → greet on **`WTS_SESSION_UNLOCK`** (and `WTS_CONSOLE_CONNECT` for fast user switching), pause mic on `WTS_SESSION_LOCK` / `WTS_CONSOLE_DISCONNECT`.
   - `RegisterPowerSettingNotification` for `GUID_SESSION_DISPLAY_STATUS`, `GUID_LIDSWITCH_STATE_CHANGE`, `GUID_SESSION_USER_PRESENCE` → on display-on / lid-open: restart the mic stream, pre-warm the Edge voice connection, load the greeting (do **not** speak yet).
   - `WM_POWERBROADCAST` (`PBT_APMRESUMEAUTOMATIC`, `PBT_APMSUSPEND`) → bookkeeping only (resume briefing data, audio-device restart), never a spoken greeting on its own.
   - Speak only when: own session is the active console session **and** it is unlocked **and** no meeting **and** (unlock event or, if sign-in after sleep is off, display-on + user presence).
5. Replace the 10 s/60 s time-jump heuristic (`lib.rs:745-770`) with the events above; keep it only as a fallback log line.

### Task XML (exact fields I would use)

```xml
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>LAKSHYA-LAPTOP\Chitkul Lakshya</UserId>
      <!-- no <Delay>: start immediately -->
    </LogonTrigger>
    <!-- Re-launcher only: if NEXUS crashed, unlock brings it back.
         While NEXUS is running, IgnoreNew makes this a no-op. -->
    <SessionStateChangeTrigger>
      <Enabled>true</Enabled>
      <StateChange>SessionUnlock</StateChange>
      <UserId>LAKSHYA-LAPTOP\Chitkul Lakshya</UserId>
    </SessionStateChangeTrigger>
    <SessionStateChangeTrigger>
      <Enabled>true</Enabled>
      <StateChange>ConsoleConnect</StateChange>
      <UserId>LAKSHYA-LAPTOP\Chitkul Lakshya</UserId>
    </SessionStateChangeTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>S-1-5-21-…-1001</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>4</Priority>
    <StartWhenAvailable>true</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <RestartOnFailure><Interval>PT1M</Interval><Count>3</Count></RestartOnFailure>
    <!-- no RunOnlyIfIdle / no network condition: either would delay the start -->
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>…\nexus.exe</Command>   <!-- an installed copy, not target\release -->
      <Arguments>--background</Arguments>
    </Exec>
  </Actions>
</Task>
```

Notes: the unlock/console-connect triggers do **not** carry the greeting — the resident process decides that from its own WTS notifications. Their only job is recovery if NEXUS is not running. When the task re-launches NEXUS on unlock, NEXUS should treat it like a resume (greet once), not like a cold boot.

### What must NOT be done
- No Windows service / session-0 process for speech; no `CreateProcessAsUser` service just to save ~1 s.
- No Credential Provider, no Winlogon `Userinit`/`Shell` edits, no speaking or listening before unlock.
- No `HKLM\…\Run` (starts for every user) and no `Global\` single-instance mutex (blocks other users).
- Do not change `StartupDelayInMSec` / `WaitForIdleState` — it is global, affects every startup app, and does not affect the NEXUS task at all.
- Do not greet on `PBT_APMRESUMEAUTOMATIC`, display-on or lid-open alone (maintenance wakes; someone else may be holding the laptop; lock screen still up).
- Do not rely on a Task Scheduler event trigger for Power-Troubleshooter ID 1 — it is not logged on this Modern Standby machine.
- Do not let a disconnected (fast-user-switched) session keep the microphone open.
- Do not launch from `target\release` at logon if a build might be running (a locked/rewritten exe makes the task fail silently).

---

## Sources
- Task Scheduler: [LogonTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/logontrigger-type), [BootTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/boottrigger-type), [SessionStateChangeTrigger](https://learn.microsoft.com/en-us/windows/win32/taskschd/sessionstatechangetrigger-type)
- Power: [Power Setting GUIDs](https://learn.microsoft.com/en-us/windows/win32/power/power-setting-guids), [PBT_APMRESUMEAUTOMATIC](https://learn.microsoft.com/en-us/windows/win32/Power/pbt-apmresumeautomatic), [PBT_APMRESUMESUSPEND](https://learn.microsoft.com/en-ca/Windows/win32/power/pbt-apmresumesuspend), [System wake-up events](https://learn.microsoft.com/en-ie/Windows/Win32/power/system-wake-up-events), [RegisterSuspendResumeNotification](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registersuspendresumenotification), [Q&A: event on Modern Standby](https://learn.microsoft.com/en-us/answers/questions/882272/event-that-gets-triggered-on-modern-standby), [Prepare software for Modern Standby](https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/prepare-software-for-modern-standby), [Modern Standby states](https://learn.microsoft.com/da-dk/windows-hardware/design/device-experiences/modern-standby-states)
- Sessions: [WTSRegisterSessionNotification](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsregistersessionnotification), [WTSQueryUserToken](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/nf-wtsapi32-wtsqueryusertoken), [CreateProcessAsUser](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-createprocessasuserw)
- Session 0: [Interactive services](https://learn.microsoft.com/en-us/windows/win32/services/interactive-services), [Session 0 isolation (askperf)](https://techcommunity.microsoft.com/blog/askperf/application-compatibility---session-0-isolation/372361), [Microsoft whitepaper (mirror)](https://coretechnologies.com/WindowsServices/Microsoft-Impact-of-Session-0-Isolation-on-Services-and-Drivers-in-Windows-Vista.pdf), [Larry Osterman: Vista audio APIs](https://learn.microsoft.com/en-us/archive/blogs/larryosterman/new-audio-apis-for-vista), anecdote: [SAPI in a service](https://windows-hexerror.linestarve.com/q/so24293808-simple-text-to-speech-with-sapi-in-a-windows-service)
- Logon: [Winlogon and credential providers](https://learn.microsoft.com/en-us/windows/win32/secauthn/winlogon-and-credential-providers), [Credential providers in Windows](https://learn.microsoft.com/en-us/windows/win32/secauthn/credential-providers-in-windows)
- Startup delay (third-party / user-posted): [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-delay-of-running-startup-apps-in-windows-11.9144/), [NinjaOne](https://ninjaone.com/blog/enable-or-disable-delay-of-running-startup-apps-in-windows-11), [Microsoft Q&A answer](https://learn.microsoft.com/en-us/answers/a/7617892), [The Windows Club](https://www.thewindowsclub.com/disable-windows-10-startup-delay-startupdelayinmsec)
- Machine evidence: `schtasks /query /tn NEXUS /xml`, `powercfg /a`, `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Power\HiberbootEnabled`, System log (Kernel-General 12/13, Kernel-Power 506/507/566, Winlogon 7001, Service Control Manager), `Microsoft-Windows-Winlogon/Operational`, `Microsoft-Windows-Shell-Core/Operational`, `Microsoft-Windows-User Profile Service/Operational`, `%APPDATA%\com.nexus.assistant\boot_timeline.jsonl`, `%APPDATA%\com.nexus.assistant\logs\nexus.log`, `Win32_StartupCommand`.
