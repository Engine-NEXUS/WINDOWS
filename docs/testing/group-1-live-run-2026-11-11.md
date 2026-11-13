# Group 1 live run — 15 features closest to production-ready

| | |
|---|---|
| **Date opened** | 2026-11-11 |
| **Status** | **Run in progress, 2026-11-12.** Machine (BOT) passes with ledger lines for cards 5.1, 5.2, 5.4, 5.5, 5.6, 5.8, 5.13, 5.14 and, for "new tab" only, 5.15. Partial: 5.3, 5.7, 5.9. Not run: 5.10, 5.11, 5.12 (need your eyes). No feature is production-ready by the repository definition until the OWNER half of its proof is recorded. |
| **Current build** | identity `df23b69c593545d4`, built 2026-11-12 02:07 IST from commit `02c2928` (main `0c7545b` merged in). Earlier builds of this run: `edfa406ab1f4f62a`, `adcc651268d5cc0e`, `e8f39727170b94e8`; results on them are kept below as history and do not count. |
| **Release built for this run** | identity `edfa406ab1f4f62a`, built 2026-11-11 17:26 IST from commit `f158144` (`b6b0a85` plus this file); executable SHA-256 `d7f18ebc…e73bb747` matches `nexus-build.json`. Not started yet. |
| **Source commit to build** | `b6b0a85` on branch `fix/memory-key-safety-dictation-ci` (main `6820cff` + memory key safety, dictation fixes, two CI gates) |
| **Previous build** | release identity `8024418bab39dd49`, built 2026-11-07. It was stopped by the rebuild. Readings taken on it are marked preliminary and do not count (ledger rule 3). |
| **Who runs what** | **OWNER** = you, with voice, ears and eyes. **BOT** = the assistant, reading the log and the OS while you run. |
| **Your time** | About 45 minutes after the rebuild |

This file is the run sheet and the record for the first batch of the production-readiness programme. It follows the rules in [`live-proof-ledger.md`](live-proof-ledger.md) and the levels in [`feature-readiness-2026-11-03.md`](feature-readiness-2026-11-03.md).

---

## 1. What "production-ready" means here

A feature reaches level 1 only when all three are true:

1. Its unit and integration tests pass on the commit being run.
2. A real run on that commit is recorded as a line in `live-proof-ledger.md`.
3. No defect is known for it.

Condition 1 is met for all 15 features (section 3). This run sheet exists to produce condition 2. A feature that fails its run goes back to "partial", the failure is recorded, and it is fixed before another attempt.

## 2. Why these 15

They were chosen from the 45 "working, needs live proof" features because each one:

- has passing tests and no recorded defect,
- needs no outside account, key or second device,
- can be proven in a few minutes.

| # | Feature | Proof kind | Section |
|---:|---|---|---|
| 2 | Global hotkey Ctrl+Space | OWNER + BOT | [5.1](#51-global-hotkey-ctrlspace-2) |
| 31 | Open / close / focus apps | OWNER + BOT | [5.2](#52-open--close--focus-apps-31) |
| 14 | Wake-up sound | OWNER + BOT | [5.3](#53-wake-up-sound-14) |
| 9 | Online voice (Edge TTS) | OWNER + BOT | [5.4](#54-online-voice-edge-tts-9) |
| 21 | 9Router | BOT (during 5.4) | [5.5](#55-9router-21) |
| 61 | Volume lease and restore | BOT (during 5.4) | [5.6](#56-volume-lease-and-restore-61) |
| 40 | Keep-awake | BOT (during 5.4) | [5.7](#57-keep-awake-40) |
| 65 | Resource footprint | BOT | [5.8](#58-resource-footprint-65) |
| 57 | Cloudflare Worker | BOT + OWNER | [5.9](#59-cloudflare-worker-57) |
| 64 | Command Hub / settings UI | OWNER | [5.10](#510-command-hub--settings-ui-64) |
| 69 | Solid panel windows | OWNER (during 5.10) | [5.11](#511-solid-panel-windows-69) |
| 78 | Themes and contrast | OWNER (during 5.10) | [5.12](#512-themes-and-contrast-78) |
| 20 | Main centre and sub-centres | OWNER + BOT | [5.13](#513-main-centre-and-sub-centres-20) |
| 27 | File control with undo (Action Kernel) | OWNER + BOT | [5.14](#514-file-control-with-undo-27) |
| 25 | Shortcut map and action router | OWNER + BOT | [5.15](#515-shortcut-map-and-action-router-25) |

The numbers are the row numbers of the 2026-11-03 readiness report.

## 3. Test evidence on the commit to build

Run on 2026-11-11 against the working tree that became `b6b0a85`.

| Suite | Result |
|---|---|
| Rust library (`cargo test --tests --features custom-protocol,admin-brain -- --test-threads=1`) | 3,180 passed, 0 failed, 36 ignored |
| Rust integration (voice fixture, offline commands, user commands) | 20 passed |
| Frontend (`vitest run`) | 1,029 passed in 104 files; `tsc --noEmit` clean |
| Worker (`npm test`) | 145 passed; `tsc --noEmit` clean |
| Dead-code ratchet | 139 known, none new |
| Turn-end, repo hygiene, IPC contract, icons, theme contrast, hard-coded colours, window opacity, work-card drift | all pass |

Not run: the GitHub Actions workflow itself, any installer build, anything in the live app.

## 4. Before the run

### 4.1 Build and start (OWNER)

1. Check out the commit: branch `fix/memory-key-safety-dictation-ci`, commit `b6b0a85`.
2. Leave `frontend/package-lock.json` and `server/worker/package-lock.json` as they are locally; they are not part of the commit.
3. Build, then start:

   ```bash
   nexus build
   ```

   ```bash
   nexus start
   ```

4. Tell the assistant when the console shows the stage is ready. The assistant then records the new release identity from `src-tauri/target/release/nexus-build.json` and the process ID; every ledger line uses them.

### 4.2 Conditions for the whole run

- Internet on, a working microphone, speakers audible.
- No meeting app open (NEXUS mutes itself in meetings).
- Voice Lock off, Ghost mode off, Hand mode off.
- Windows volume at a level you can name (for example 30). Say the number to the assistant before 5.4.
- Keep the `nexus start` console open. The assistant reads the same lines from `logs\nexus_unified.log`.

### 4.3 What the assistant reads

| Signal | Where | Used for |
|---|---|---|
| `hotkey (…) → agent is idle, CALLING agent` / `… CANCELING calling agent` | console, log | 5.1 |
| `[ORB-HOTKEY]` | console | 5.1 |
| `[MAIN-CMD]`, `[SUB-ROUTE]`, `[SUB-PROC]`, `[DROP]` | console, log | 5.2, 5.13, 5.14, 5.15 |
| `[CUE] output opened in N ms`, `[CUE] queued -> audio handed to device in N ms` | log | 5.3 |
| `[LAT] stt … | parse … | answer … | tts … | total …` | console, log | 5.4, 5.5 |
| `[TTS]` lines (engine chosen per utterance) | console, log | 5.4 |
| `9router: trying <provider> for: …` | log | 5.5 |
| `[VOL] saved original X, setting Y for TTS`, `[VOL] restoring original volume X` | log | 5.6 |
| `[KEEPAWAKE] asserted`, `[KEEPAWAKE] cleared`, plus `powercfg /requests` | log, OS | 5.7 |
| Process tree working set | OS | 5.8 |
| `GET /health` on the Worker | network | 5.9 |
| `%APPDATA%\com.nexus.assistant\action_audit.jsonl`, `<folder>\.nexus-trash\` | disk | 5.14 |
| `[SHORTCUT]` | log | 5.15 |

---

### 4.4 Start-up of the new build, 2026-11-11 18:21 IST (BOT)

Release `edfa406ab1f4f62a` was started as PID 19892. The log's `[CAPTURE]` records carry the same build identity, so the running process is the build described above.

| Check | Reading | Meaning for the run |
|---|---|---|
| Stage | `[BOOT] stage ready after 1s`; fitted to 1366x768 | OK |
| Preflight layers 1 to 4 | all `[OK]`, 18 of 18 sub-centres registered, 668 apps loaded from cache | OK |
| Hotkeys | Ctrl+Space, Ctrl+Shift+S, Ctrl+Alt+X registered | OK for 5.1 |
| Online voice | Edge-TTS online, Kokoro on standby | OK for 5.4 |
| Microphone | `Microphone (USB PnP Audio Device)`, probe RMS 0.000003; first capture `rms_max=0.0021 voiced=0/75` | **Near-silent.** Every spoken step needs a louder microphone or a higher input level in Windows first. |
| Provider keys | Groq missing, Gemini missing, Cerebras missing | **Blocks 5.5 (9Router)** and cloud speech-to-text. Add at least a Groq key in the Hub. |
| Cloud link | "This device isn't linked to the NEXUS cloud service yet. Ask the NEXUS admin to approve it" | **Blocks 5.9 steps b and c** until this device is approved. |
| Memory, 32 s after start | 523.7 MB working set over 7 processes (`nexus.exe` 127.6 MB, six WebView2 processes 396.1 MB) | Under the 700 MB ceiling but well above the old build's settled 292.7 MB. To be re-read at five minutes idle for 5.8. |
| Launcher | `nexus start` failed: it calls `pwsh` (PowerShell 7), which is not installed on this PC. The executable was started directly instead. | **Defect found.** `nexus start` needs a fallback when PowerShell 7 is absent. |

**Second reading, 143 minutes after start (20:44 IST), nobody at the PC:**

| Check | Reading | Meaning |
|---|---|---|
| Process | PID 19892 still running, no panic, no stage blackout | Stable for 2 h 23 min idle |
| Memory | 186.9 MB working set over 7 processes (`nexus.exe` 133.5 MB, six WebView2 processes 53.4 MB) | Idle reading for 5.8: far under the 700 MB ceiling. The 523.7 MB at 32 s was start-up load. |
| Microphone | Highest level seen 0.073; 371 "mic has been silent" warnings; 21 stream restarts; 0 audio-service restarts | The microphone **does** pick up sound. The room was simply quiet. The earlier "near-silent" reading was a quiet moment, not a dead device. |
| Wake fires and turns | 0 and 0 | No false wake in 143 minutes of an empty room. Nothing was spoken to it either. |

Two things worth fixing, found from this reading: the silence warning tells the user to restart the "Intel SST driver" although this PC uses a USB microphone, and a quiet room is treated as a fault (a warning every 10 seconds and a stream restart every few minutes).

Cards that can run on this PC as it stands: 5.1 (hotkey, without the spoken parts), 5.8, 5.10, 5.11, 5.12. Everything spoken waits for the microphone; 5.5 waits for a key; 5.9 waits for the device approval.

---

## 5. The runs

Each card has the same parts: what the run proves, the steps, what counts as a pass, what would fail it, and the result. Results stay **PENDING** until the run is done on the new build.

### 5.1 Global hotkey Ctrl+Space (#2)

**Proves:** one key both calls and cancels NEXUS, in every state, and never closes a window by accident.

| Step | You do | Expected |
|---|---|---|
| a | NEXUS idle. Press Ctrl+Space, say nothing. | Orb appears and listens for about 8 s, listens once more without a word for about 8 s, then says "I didn't hear you, sir." once and hides. (Corrected 2026-11-12: the first version of this card expected no speech; one silent retry and one spoken line is the designed behaviour.) |
| b | Press Ctrl+Space, then press it again within a second. | Listening is cancelled, orb hides. |
| c | Press Ctrl+Space, say "what is the capital of France". While it is thinking, press Ctrl+Space. | The request is cancelled. No answer is spoken afterwards. |
| d | Ask the same question, let it start speaking, press Ctrl+Space. | Speech stops at once. The orb goes idle. Volume returns to your level. |
| e | Open the Command Hub (Ctrl+Shift+S). Press Ctrl+Space. | The Hub stays open. NEXUS is called. |
| f | Press Ctrl+Space, say "open Notepad". | Notepad opens. This shows a call after several cancels still works. |

**Pass:** all six behave as expected, and the log shows one `CALLING agent` or `CANCELING calling agent` line per press with the right state flags.
**Fail:** a press is ignored, a cancelled answer is still spoken, the orb stays on screen, or the Hub closes.

**Result:** PASS by machine run, 2026-11-12 02:11 to 02:12 IST, build `df23b69c593545d4`, commit `02c2928`, NEXUS PID 33244. Proof kind: BOT. The presses were sent by script (`keybd_event`), and the spoken lines were played through this PC's speakers with the Windows voice and heard by the USB microphone. **Not covered:** the owner's own voice, and what the orb and the Hub look like on screen (nobody watched the screen).

| Step | Time (UTC) | Log evidence | Result |
|---|---|---|---|
| a | 20:41:10 | `agent is idle, CALLING agent`; capture session 2 ended with no words after 10 s; silent second capture (session 3) 8 s; `speaking 'I didn't hear you, sir.'` once; volume 1.00 to 0.75 and back. The boot greeting waited until this was over (20:41:32). | Pass |
| b | 20:41:36 | `CALLING agent`, then 0.78 s later `CANCELING calling agent` (capturing=true); capture aborted; nothing spoken. | Pass |
| c | 20:41:43 | Question captured; press 0.41 s after the capture ended, while speech-to-text was running: `CANCELING` with thinking=true, `[TURN] capture session 6 cancelled before it reached the orchestrator`. The transcript arrived 1 s later and was not routed. No answer spoken. | Pass |
| d | 20:42:01 | Answer `The capital of France is Paris.` started 20:42:05.45; press at 20:42:07.08: `CANCELING` with speaking=true; volume restored to 1.00 at 20:42:08.29. | Pass |
| e | 20:42:19 | `opening settings sidebar`; 5 s later `CALLING agent`; 2 s later `CANCELING`. No window-destroy line for the Hub. | Pass in the log; not seen on screen |
| f | 20:42:30 | `groq transcript: 'Open Notepad.'`, routed to LocalCommand, `speaking 'Ok sir.'`. | Pass |

One press, one log line, correct state flags: 11 of 11 presses.

**What was not exercised live:** a cancel in the roughly 100 to 300 ms after speech-to-text has answered and before a request exists (the routing gap). The press in step c landed earlier, during speech-to-text. That path is covered by the code change in `02c2928` and by the unit tests of `turn_pending`, not by a live press.

#### History of this card (earlier builds, do not count)

| Build | What failed | Fix |
|---|---|---|
| `edfa406ab1f4f62a` (owner pressing, 2026-11-11 23:09 IST) | After "Claude Code has finished, sir." the microphone opened by itself, so the owner's first press was a cancel. | Already fixed on main (`61c6bfd`: only a reply that asks something reopens the microphone); merged in `bd9d36c`. |
| same | A silent press after an earlier miss was answered at once with "I didn't hear you, sir." | `79a760e`: a cancel or a fresh call clears the miss counter. |
| same | No spoken command could be understood: no Groq key and `moonshine_voice` not installed. | Local engine package installed (`moonshine-voice` 0.1.6); a Groq key was present in NEXUS from the next start. |
| `adcc651268d5cc0e` | A press while the question was being transcribed was read as "idle": NEXUS was called again, the old answer was spoken, and NEXUS then answered its own voice. | `4d97cd6`: new `turn_pending` marker; the hotkey treats a heard turn on its way as thinking, and its transcript is dropped. |
| `e8f39727170b94e8` | The press was logged as a cancel but the answer was still spoken (the turn was already being routed). | `02c2928`: routing runs under a 20 ms cancel check until a request owns the turn. |
| same | The boot briefing was spoken into a microphone the hotkey had just opened; NEXUS transcribed "Welcome back, sir." and replied to it. | `02c2928`: alerts wait while the user's microphone is open (a Ghost session is exempt). |

### 5.2 Open / close / focus apps (#31)

**Proves:** apps open once, an open app is focused instead of duplicated, and close works.

| Step | You say | Expected |
|---|---|---|
| a | "Nexus, open Notepad" | One Notepad window, in front. "Ok sir." is spoken only once it is in front. |
| b | "Nexus, open Notepad" again | The same window comes to the front. **No second window.** |
| c | "Nexus, open Calculator" | Calculator opens in front. |
| d | "Nexus, close Notepad" | Notepad closes. Calculator is untouched. |
| e | "Nexus, open an app called blue elephant" | NEXUS says it cannot find it. It must not say "Ok sir." |

**Pass:** exactly one Notepad window after step b (count them in the taskbar), honest answer in step e.
**Fail:** a duplicate window, a spoken success for something that did not happen, or the wrong app closed.

**Result:** PASS by machine run, 2026-11-12 02:14 to 02:15 IST, build `df23b69c593545d4`, PID 33244. Proof kind: BOT (hotkey call in place of the wake word, Windows voice through the speakers, visible windows counted from the OS after each step).

| Step | Heard as | What NEXUS did (log) | Windows after (Notepad / Calculator / in front) | Result |
|---|---|---|---|---|
| a | "Open Notepad." | `focus hit: 'Notepad' matched window 'Untitled - Notepad'`, then "Ok sir." | 1 / 0 / Notepad | Pass (a Notepad window from the hotkey card was already open, so this was a focus, not a launch) |
| b | "Open Notepad." | same focus hit, "Ok sir." | 1 / 0 / Notepad | Pass: no second window |
| c | "Open calculator." | `launching app: Calculator via Exe`, "Ok sir." | 1 / 1 / Calculator | Pass |
| d | "Close Notepad." | `closed app 'Notepad'`, "Closed Notepad, sir." | 0 / 1 / Calculator | Pass: Calculator untouched |
| e | "Open an app called Blue Elephant." | registry miss, legacy resolver, "Didn't find that, sir." | 0 / 1 / Calculator | Pass: no false "Ok sir." |

A fresh launch of Notepad from nothing was seen earlier on build `adcc651268d5cc0e` (hotkey card, step f). **Not covered:** the owner's own voice and the wake word as the way in.

### 5.3 Wake-up sound (#14)

**Proves:** the cue is audible at the wake, at a sensible level, and the microphone does not treat it as your voice.

| Step | You do | Expected |
|---|---|---|
| a | Say "Nexus" and then stay silent. | You hear the cue once. The orb listens, then hides. Nothing is transcribed. |
| b | Say "Nexus, what time is it". | Cue, then a normal answer. The first word of your command is not cut off. |
| c | Press Ctrl+Space from idle. | Same cue. |
| d | Command Hub → Audio → Wake Sound: press Play on two other styles, pick one, Save. Say "Nexus". | The new style plays at the wake. |
| e | Turn Wake Sound off, Save, say "Nexus". | No cue. NEXUS still wakes. Turn it back on afterwards. |

**Pass:** steps a–e as expected, and the log shows `[CUE] queued -> audio handed to device` under about 100 ms with no warning lines.
**Fail:** no sound, a clipped or distorted sound, the cue transcribed as speech in step a, or the first word lost in step b.

Tell the assistant how it sounded (clear, too loud, too quiet). That judgement is the OWNER half of this proof.

**Result:** PARTIAL: BOT half passes, OWNER half pending. Machine run 2026-11-12, build `c315aa3ad3fc2ec2`. In 20 wake cues (hotkey calls) the log shows `[CUE] queued -> audio handed to device` in 0 to 15 ms (6 at 0 ms, 4 at 7 ms, 3 at 9 ms, 3 at 10 ms, the rest 8, 14 and 15 ms), with no cue warning lines. The wake word as the way in, the first word of a command after the cue, the style picker and the off switch (steps a, b, d, e) were not run; whether the sound is audible, clear and at a good level needs your ears.

### 5.4 Online voice (Edge TTS) (#9)

**Proves:** replies use the online voice, start quickly, and one reply uses one voice from start to finish.

Before you start, tell the assistant your current Windows volume number.

| Step | You say | Expected |
|---|---|---|
| a | "Nexus, what is the capital of Japan" | Short answer in the usual voice. |
| b | "Nexus, tell me three facts about the moon" | A longer answer. The voice does not change part-way. Volume does not dip between sentences. |
| c | "Nexus, what is two plus two" | Short answer. |

**Pass:** every `[TTS]` line for these turns names the Edge engine, no line shows a fall-back to the local voice, and the `tts` stage in each `[LAT]` line is recorded. The assistant writes the three `[LAT]` lines into this file.
**Fail:** the local voice is used while online, the voice changes mid-reply, or a reply is silent.

There is no fixed latency limit in the plan; the earlier measurement was a 409 ms median to first audio per short line. The run records the real numbers and flags anything over 2 s.

**Result:** PASS by machine run, 2026-11-12 20:52 to 20:54 IST, build `c315aa3ad3fc2ec2`. Proof kind: BOT (speech through the speakers; the Windows volume read from Core Audio). Every `[TTS]` line in the window names the Edge engine: 11 of 11, none local, no fall-back line.

| Turn | `[LAT]` line | Voice |
|---|---|---|
| "What is the capital of Japan?" | `stt 283ms | parse 35ms | answer 548ms | tts 117ms | total 984ms | mem 34ms` | edge, `en-US-AvaNeural`, 75% |
| "Tell me three facts about the moon." | `stt 465ms | parse 440ms | tts 672ms | total 1577ms | mem 7ms` | edge; the answer is long, so NEXUS said "Here is the response in the sidebar, sir." and did not read it out |
| "What is 2 plus 2?" | `stt 217ms | parse 14ms | answer 632ms | tts 31ms | total 895ms | mem 2ms` | edge |

The one-voice-across-sentences check used a spoken two-sentence answer instead ("What is the tallest mountain?", 14 s of streamed playback): one voice, no change. Nothing is over 2 s. Earlier the same night the online voice failed twice in a row ("no audio was received from the service") and NEXUS used the local voice for 10 minutes; that is the designed breaker, not seen in this window.

### 5.5 9Router (#21)

**Proves:** general questions are answered by a direct provider (Cerebras, Groq or Gemini) and not sent through the Worker.

No extra steps. The assistant reads the same three turns from 5.4.

**Pass:** each turn has a `9router: trying <provider>` line followed by a success, and the `answer` stage of `[LAT]` is recorded. Earlier measurement: about 242 ms.
**Fail:** every provider fails and the turn falls back to the Worker for a plain question, or a provider key is reported missing.

**Result:** PASS for the Groq path, machine run, build `c315aa3ad3fc2ec2`. Each of the four general questions has `9router: trying Groq` and `fast path succeeded via Groq` in 448, 1,120, 604 and 721 ms (the `answer` stage in `[LAT]` reads 548, 632 and others). Only a Groq key is configured on this PC, so the fall-back to Cerebras and Gemini was not exercised; no turn fell back to the Worker.

### 5.6 Volume lease and restore (#61)

**Proves:** NEXUS raises the volume only while it speaks and puts your level back afterwards, including after a cancel.

The assistant reads the Windows master volume before 5.4, during the long reply in 5.4 step b, two seconds after it ends, and after the cancel in 5.1 step d.

**Pass:** the volume after each reply equals the number you named, within one step; the log shows a matching `[VOL] saved original` and `[VOL] restoring original volume` pair for each reply; no `watchdog: … force-restoring` line.
**Fail:** the volume is left at the speech level, or drops between sentences of one reply.

**Result:** PASS by machine run, build `c315aa3ad3fc2ec2`. The Windows master volume was read from Core Audio while NEXUS spoke: your level was 100%; during every reply it was 75% (NEXUS's speech level), and it was back at 100% within about 2 s after the reply ended, four times out of four. During a 14 s two-sentence reply it was sampled every second: 75% throughout, no dip between sentences. `[VOL] saved original 1.00` and `[VOL] restoring original volume 1.00` pair up in every reply (15 of 15 over the night); no `watchdog` line. After a cancel (card 5.1 step d) the volume was back at 1.00 1.2 s later. Note: the card said NEXUS raises the volume; with your level at 100% it lowers it to its 75% speech level and restores yours.

### 5.7 Keep-awake (#40)

**Proves:** Windows is told to stay awake only while NEXUS is working or speaking, and the request is cleared straight after.

The assistant runs `powercfg /requests` (needs an administrator console; if it is not available the log lines alone are recorded and the proof is marked partial) before, during the long reply of 5.4 step b, and five seconds after it.

**Pass:** no NEXUS entry before, a NEXUS `DISPLAY`/`SYSTEM` entry during, none after; the log shows `[KEEPAWAKE] asserted` then `[KEEPAWAKE] cleared`.
**Fail:** the request is still listed after the reply ends.

Earlier proof (2026-10-20) was on older code, before pinned holds were added, so it does not carry over.

**Result:** PARTIAL, machine run, build `c315aa3ad3fc2ec2`. `powercfg /requests` needs an administrator console ("This command requires administrator privileges"), so the Windows request list was not read. The log shows `[KEEPAWAKE] asserted` then `[KEEPAWAKE] cleared` as a pair around spoken turns: 15:18:50 to 15:18:54, 15:19:07 to 15:19:10, 15:19:24 to 15:19:31 (UTC). From 15:19:38 the Agent Island held the screen continuously (`agent monitor: screen hold taken`) because the Claude Code session that drives this test was working, so "nothing is held after the reply" cannot be shown while that session runs. To finish: run `powercfg /requests` in an administrator console during and after a spoken reply, with no coding agent running.

### 5.8 Resource footprint (#65)

**Proves:** NEXUS stays under the 600–700 MB memory ceiling set in the project scope.

The assistant measures the whole process tree (the `nexus.exe` process and every child, including the WebView2 processes) at three moments: idle five minutes after start, straight after the 5.4 replies, and at the end of the run.

**Pass:** total working set under 700 MB at all three moments.
**Fail:** over 700 MB at any of them, or steadily growing between the three.

**Preliminary reading on the old build** (identity `8024418bab39dd49`, PID 8536, about six hours after start, 2026-11-11). It is recorded for comparison only and is not a proof:

| Process | Count | Working set | Private |
|---|---:|---:|---:|
| `nexus.exe` | 1 | 43.1 MB | 86.8 MB |
| `msedgewebview2.exe` | 7 | 249.1 MB | 226.3 MB |
| Other children (`conhost`, `net`, `net1`) | 3 | 0.5 MB | 3.1 MB |
| **Total** | **11** | **292.7 MB** | **316.2 MB** |

Observation from that reading: `net` and `net1` child processes were present. NEXUS starts those only when it restarts the Windows Audio service because the microphone has been silent for about a minute. That points to a silent microphone on this PC at that moment. Check the microphone before starting the run; the wake-word steps cannot pass without it.

**Result:** PASS by machine reading, build `c315aa3ad3fc2ec2`, NEXUS started about 20:42 IST. Working set of `nexus.exe` and every child (8 processes including WebView2): **638 MB** at the idle reading (about 10 min after start), **630.5 MB** straight after the replies, **628.9 MB** at the end. All under 700 MB and not growing. The margin is about 60 MB, so a longer session with the response sidebar and the Hub open should be measured too (an earlier reading with the sidebar open was 548 MB on a smaller tree).

### 5.9 Cloudflare Worker (#57)

**Proves:** the deployed Worker is reachable and completes one real turn that must go through it.

| Step | Who | What | Expected |
|---|---|---|---|
| a | BOT | `GET /health` on the deployed Worker | HTTP 200, `ok: true`, protocol version matching the app |
| b | OWNER | Say "Nexus, analyse the latest PR in" followed by one of your repos. Any GitHub question works, because those always go through the Worker. | A spoken result or a clear, honest reason (for example GitHub not connected) |
| c | BOT | Read the log for the Worker request and its status | A completed request, no protocol-mismatch warning |

**Preliminary reading, 2026-11-11** (the Worker is deployed separately from the desktop build, so step a already counts for the Worker itself): HTTP 200 in 115 ms, body `{"ok":true,"service":"NEXUS Worker","protocol":"nexus-json-v1","protocol_version":"1","serverless":true,"identity":"1","entitlement_gate":"1"}`.

**Pass:** step a healthy on the day, and step b completes through the Worker.
**Fail:** health check fails, a protocol-version warning appears, or the turn returns raw JSON or silence.

**Result:** step a PASS (HTTP 200 in 0.17 s, 2026-11-12, build `c315aa3ad3fc2ec2`); steps b and c PENDING. Step b needs a GitHub question that must go through the Worker and a GitHub sign-in, which this run could not do without you. The general questions in 5.4 went through 9Router, not the Worker.

### 5.10 Command Hub / settings UI (#64)

**Proves:** every Hub page opens, shows real values, and a saved change survives a reopen.

Open the Hub with Ctrl+Shift+S or by saying "Nexus, open the command hub".

| Step | You do | Expected |
|---|---|---|
| a | Look at the home page. | Accounts, API-key count, connected services and request counters are shown, with no blank or broken rows. |
| b | Open each page in turn: API Keys, Appearance, Memory, Actions and Permissions, Activity Manager, Alerts, Device Links, then Audio, Display, Connections, Advanced. | Each opens without a blank panel or an error. Back returns to home. |
| c | Audio: change the wake sound volume, Save, close the Hub, reopen it. | The new value is still there. |
| d | API Keys: check that keys are shown masked. | No full key is ever visible. |
| e | Press Escape. | The Hub closes. |
| f | Dock Left, then Dock Right. | The panel moves and keeps its full height. |

**Pass:** all pages open, the saved value survives, keys stay masked.
**Fail:** a blank page, a setting that reverts, a full key on screen, or text that is cut off.

Name any page that looks wrong; the assistant records it against this feature.

**Result:** PENDING

### 5.11 Solid panel windows (#69)

**Proves:** panels are solid and readable over any background.

Done during 5.10. Put a bright window (a white web page) and then a dark one behind the Hub.

**Pass:** the Hub's background does not change with what is behind it, and no desktop content shows through any panel. Repeat once with the response sidebar open (ask any question that opens it).
**Fail:** any see-through or black-box panel.

**Result:** PENDING

### 5.12 Themes and contrast (#78)

**Proves:** Light, Dark and System all apply everywhere at once and stay readable.

| Step | You do | Expected |
|---|---|---|
| a | Hub → Appearance → Light, Save. | Hub, sidebar and the orb switch together. Text is readable. |
| b | Dark, Save. | Same, in dark. |
| c | System, then change Windows between light and dark (Settings → Personalisation → Colours). | NEXUS follows Windows without a restart. |
| d | Turn on a Windows contrast theme (Settings → Accessibility → Contrast themes), look at the Hub, turn it off. | Text and controls remain visible with clear borders. |

**Pass:** all four, with no unreadable text and no window left in the old theme.
**Fail:** one window keeps the old theme, or any label is too faint to read.

**Result:** PENDING

### 5.13 Main centre and sub-centres (#20)

**Proves:** a spoken command reaches the right sub-centre, and an incomplete command is asked about and not guessed.

Say each line with "Nexus," in front. The assistant matches each to its `[SUB-ROUTE]` line.

| You say | Expected sub-centre | Expected behaviour |
|---|---|---|
| "hello" | GreetingCenter | Greeting |
| "open Calculator" | AppCenter | Opens |
| "new tab" (with a browser in front) | BrowserCenter | New tab |
| "what is the tallest mountain" | KnowledgeCenter | Spoken answer |
| "remember that my favourite colour is blue" | MemoryCenter | Confirms it is saved |
| "what do you remember" | MemoryCenter | Lists it, sidebar card opens |
| "play some music" | MediaCenter | Plays, or says no player is set up |
| "list my pull requests" | GitHubCenter | List, or says GitHub is not connected |
| "what's on my calendar today" | GoogleCenter | Events, or says Google is not connected |
| "send a WhatsApp message" (nothing else) | MessageCenter | Asks who and what. It must not send anything. |
| "read my alerts" | AlertCenter | Reads alerts or says there are none |
| "shut down" | SystemCenter | **Asks first.** Answer "no". |

Not exercised here, because they change the mode of the whole machine or need set-up: GhostCenter, HandCenter, DictationCenter, FileCenter (covered in 5.14), ArchitectCenter, CommerceCenter, YouTubeCenter. The ledger line will say so; this run proves routing for 11 of the 18 registered centres.

**Pass:** every line routes to the listed centre; the incomplete WhatsApp command and the shutdown both ask before doing anything; nothing is answered by the cloud model pretending an action happened.
**Fail:** a wrong centre, a `[DROP]` for a clear command, or a claimed success with no action.

**Result:** PASS by machine run, 2026-11-12 20:23 to 20:27 IST, build `95f3621b8fbc0b88` (commit `4dd2d44`). Proof kind: BOT (hotkey call, Windows voice through the speakers). Earlier attempts on `df23b69c593545d4`, `951d8edcc97d90d2` and `504c08b43ccf2556` found defects (below); this is the clean pass on one build.

| Said | Heard as | Centre in the log | Behaviour | Result |
|---|---|---|---|---|
| hello | "Hello." | GreetingCenter | "Hey, sir. What can I do for you?" | Pass |
| open Calculator | "Open calculator." | AppCenter | "Ok sir.", Calculator opened | Pass |
| new tab | "New tab." | BrowserCenter | Calculator was in front, so there was no binding for it; the spoken line was still "New tab sir." (see 5.15) | Routing pass; the claim is fixed after this run |
| what is the tallest mountain | exact | KnowledgeCenter | Everest, 8,848 m, spoken | Pass |
| remember that my favourite colour is blue | "...favorite color is blue." | local memory hook (logged as KnowledgeCenter) | "Remembered my favorite color as blue." (one full stop) | Pass |
| what do you remember | exact | MemoryCenter | "I remember 1 fact. my favorite color is blue. across 13 recent conversations." | Routing pass; wording is ungrammatical and not fixed |
| play some music | exact | MediaCenter | "Play or pause sent, sir." | Pass |
| list my pull requests | exact | AppCenter | "Your pull requests, sir."; dashboard data fetched | Pass |
| what's on my calendar today | exact | MemoryCenter | "I'm not connected to your calendar yet, sir..." | Pass (honest) |
| send a WhatsApp message | exact | need_more_info | "Who should I message on WhatsApp, and what should I say?" Nothing sent. | Pass |
| read my alerts | exact | AlertCenter | Alerts read (repeated wording noted earlier) | Pass for routing |
| shut down | "Shut down." | power path | "Did you want me to shut down your laptop, sir?"; "No." 1.5 s after the question: "Understood, sir. Nothing will be turned off." | Pass |
| close Calculator | exact | AppCenter | `closed app 'Calculator'`, "Closed Calculator, sir." | Pass |

Defects found on earlier builds and fixed: GitHub dashboard command did nothing outside Ghost mode; Calculator could not be closed; remembered value kept the full stop; "Shut down." was read as closing an app called "Down"; alerts spoken into an open microphone; an alert released between a question and its answer was taken as the answer. Open: the "what do you remember" wording; centres not exercised (Ghost, Hand, Dictation, Architect, Commerce, YouTube; File is in 5.14); the log labels the memory write KnowledgeCenter.

### 5.14 File control with undo (#27)

**Proves:** a spoken delete is read back, needs a yes, moves files to a recoverable place, and "undo that" restores them.

Set-up (OWNER): make a folder `Downloads\nexus-test` and put three throwaway `.txt` files in it. Use nothing you care about.

| Step | You say | Expected |
|---|---|---|
| a | "Nexus, delete the text files in downloads nexus test" | NEXUS reads back what it will delete and how many. A card appears in the sidebar. Nothing is deleted yet. |
| b | "yes" | The three files leave the folder. |
| c | (look) | The files are in `Downloads\nexus-test\.nexus-trash\`, not in the Recycle Bin and not gone. |
| d | "Nexus, undo that" | The three files are back in place. |
| e | "Nexus, delete everything" | Refused, with a spoken reason. |
| f | Repeat step a, then answer "no". | Nothing is deleted. |

**Pass:** all six, and `action_audit.jsonl` has one entry each for the delete, the undo and the refusal.
**Fail:** anything deleted before the yes, files that cannot be restored, or the broad delete accepted.

If NEXUS does not understand the folder name in step a, say what it heard; the assistant records the exact transcript and the run continues with the folder moved to `Downloads` directly ("delete the text files in downloads" is **not** to be used, as it would catch your real files).

**Result:** PASS with one open note, machine run 2026-11-12 20:27 to 20:29 IST, build `95f3621b8fbc0b88`. Proof kind: BOT. Folder `Downloads\nexus-test` with three throwaway `.txt` files.

| Step | Said | What happened | Result |
|---|---|---|---|
| a | "Delete the text files in the Nexus test folder in downloads." | Read-back: "Move to the trash 3 items: throwaway3.txt, throwaway2.txt and throwaway1.txt from nexus-test, 57 B? Say yes to confirm." Files still in place. | Pass |
| b | "Yes." (1.5 s after the question ended) | "Done, sir. 3 items moved to the trash, 57 B. Say undo that to bring them back." The folder was empty. | Pass |
| c | (look) | The files are in `Downloads\.nexus-trash\<id>\`, one folder per delete, at the root of Downloads, not in the Recycle Bin and not gone. The card said the trash sits inside the test folder; it sits beside it. | Pass (card corrected) |
| d | "Undo that." | "Done, sir. I undid the delete (3 items)." All three files back. | Pass |
| e | "Delete everything." | "I won't delete everything or wipe a drive by voice, sir. Tell me exactly which files, like the PDFs in Downloads." Audit: forbidden, deny, blocked. | Pass |
| f | the delete again, then "No." | Clean "No." on build `504c08b43ccf2556`: "Understood, sir. I left everything as it was." On this build the speech engine returned "No. You can also type test in the folder search box..." (words that were not spoken), which went to the chat model. **Nothing was deleted**, but the decline was not recognised and the question stayed armed until it expired. | Open note |

`action_audit.jsonl` has the planned delete, the confirmed delete, the undo, the declined delete and the refused wipe, each once. Open: the extra words came from the speech engine on a quiet clip; the existing filter does not catch a long invented tail after a recognised answer word. Not fixed. An alert spoken between the question and the answer (seen on `504c08b43ccf2556`) is fixed in `4dd2d44`.

### 5.15 Shortcut map and action router (#25)

**Proves:** a keyboard action is sent only to the window it was meant for, and refused for protected windows.

Known limit, stated before the run: only "new tab" has a spoken phrase today. The other mapped actions (refresh, close tab, next tab and so on) exist in the shortcut map and its tests but cannot be reached by voice yet. This run therefore proves the router for the one spoken action in two browsers and for the safety refusals. If that is not enough to call the feature production-ready, it stays at "working" and the missing phrases become the next piece of work.

| Step | You do | Expected |
|---|---|---|
| a | Bring your main browser to the front. Say "Nexus, new tab". | One new tab in that browser. |
| b | Bring a second browser (Edge) to the front. Say "Nexus, new tab". | One new tab there. |
| c | Bring Notepad to the front. Say "Nexus, new tab". | A new Notepad tab, or a spoken "this app has no such shortcut". No keys sent to another window. |
| d | Open a terminal, bring it to the front. Say "Nexus, new tab". | Either the terminal's own new tab or a refusal. It must not send Ctrl+C, Ctrl+Z or similar. |

**Pass:** steps a and b each produce exactly one tab in the right window, and `[SHORTCUT]` lines show the window was re-checked before the keys were sent.
**Fail:** a tab opens in the wrong window, two tabs open, or keys reach a window that was not in front.

**Result:** PASS for the one spoken action ("new tab"); the feature stays at "working" because 22 of the 23 mapped actions have no spoken phrase. Machine run 2026-11-12 20:48 to 20:51 IST, build `c315aa3ad3fc2ec2` (commit `3d68263`). Proof kind: BOT.

| Window in front | Log | Window title | Result |
|---|---|---|---|
| Brave (a) | `[SHORTCUT] new_tab -> Ctrl+T (family:browser) verified=Yes in 42ms` | "about:blank - Brave" became "New Tab - Brave" | Pass |
| Edge (b) | `new_tab -> Ctrl+T (family:browser) verified=Yes in 45ms` | gained one page ("about:blank and 1 more page" became "New tab and 2 more pages") | Pass |
| Notepad (c) | `new_tab -> Ctrl+N (family:notepad) verified=Unconfirmed in 732ms` ("Open a new tab." was heard; the first attempt, heard as "Newtap", went to the chat model and sent nothing) | title stays "Untitled - Notepad"; one Notepad process | Right chord for that app, effect not confirmable from the title. "New tab sir." was spoken. |
| Windows PowerShell (d) | `new_tab -> Ctrl+Shift+T (family:terminal) verified=Unconfirmed in 734ms` | title stays "Windows PowerShell" | The terminal's own chord was used; no Ctrl+C or Ctrl+Z was sent. Effect not confirmable. "New tab sir." was spoken. |
| Calculator (not in the map) | `new_tab: no binding for applicationframehost.exe (known_app=false)`, then the old key path | unchanged | NEXUS said "I sent the new tab shortcut, sir, but I can't confirm this app took it." (the false "New tab sir." seen on the earlier build is fixed) |

Open: for a mapped app whose effect cannot be seen (Notepad, terminal) the router speaks the success line although it logged "Unconfirmed"; a router test pins that wording and I did not change it. The other 22 actions (refresh, close tab, next tab and so on) are reachable only through `shortcut_do`. Next piece of work: spoken phrases for them.

## 6. Suggested order

The order below needs the least switching and lets the assistant take its own readings while you do yours.

| Order | Cards | Your time |
|---:|---|---:|
| 1 | 5.8 idle reading, 5.9 step a (assistant only) | 0 min |
| 2 | 5.3 wake-up sound | 4 min |
| 3 | 5.1 hotkey | 5 min |
| 4 | 5.4 online voice, with 5.5, 5.6, 5.7 read alongside | 4 min |
| 5 | 5.2 open / close apps | 3 min |
| 6 | 5.13 sub-centres | 8 min |
| 7 | 5.15 shortcuts | 4 min |
| 8 | 5.14 file control | 6 min |
| 9 | 5.10 Hub, with 5.11 and 5.12 | 10 min |
| 10 | 5.9 steps b and c, 5.8 final reading | 2 min |

## 7. Results

Filled in as each card is run.

| # | Feature | Result | Ledger line added | Notes |
|---:|---|---|---|---|
| 2 | Global hotkey Ctrl+Space | PASS (machine run, build `df23b69c593545d4`) | yes, 2026-11-12 | Owner still to confirm by eye and own voice. Four defects found and fixed on the way. See 5.1. |
| 31 | Open / close / focus apps | PASS (machine run, build `df23b69c593545d4`) | yes, 2026-11-12 | 5 of 5 steps; no duplicate window. Owner voice still open. |
| 14 | Wake-up sound | PARTIAL: cue handed to the device in 0 to 15 ms (20 cues); audibility needs the owner | no | Owner ears needed. |
| 9 | Online voice (Edge TTS) | PASS (machine run, build `c315aa3ad3fc2ec2`) | yes, 2026-11-12 | 11 of 11 replies on the online voice; totals 0.9 to 1.6 s. |
| 21 | 9Router | PASS for Groq (build `c315aa3ad3fc2ec2`) | yes, 2026-11-12 | Fall-back providers not configured. |
| 61 | Volume lease and restore | PASS (machine run, build `c315aa3ad3fc2ec2`) | yes, 2026-11-12 | OS volume read: 100% to 75% to 100%; no dip inside a reply. |
| 40 | Keep-awake | PARTIAL | no | Log pairs seen; `powercfg` needs an administrator console. |
| 65 | Resource footprint | PASS (638 / 630.5 / 628.9 MB, build `c315aa3ad3fc2ec2`) | yes, 2026-11-12 | About 60 MB below the ceiling. |
| 57 | Cloudflare Worker | health PASS, real turn PENDING | no | 200 in 0.17 s; step b needs a GitHub sign-in. |
| 64 | Command Hub / settings UI | PENDING | no | |
| 69 | Solid panel windows | PENDING | no | |
| 78 | Themes and contrast | PENDING | no | |
| 20 | Main centre and sub-centres | PASS (machine run, build `95f3621b8fbc0b88`) | yes, 2026-11-12 | 13 of 13 lines; 11 of 18 centres exercised; defects fixed on the way. |
| 27 | File control with undo | PASS with note (machine run, build `95f3621b8fbc0b88`) | yes, 2026-11-12 | Delete, yes, undo and wipe refusal pass; one spoken decline was mangled by the speech engine (nothing deleted). |
| 25 | Shortcut map and action router | PASS for "new tab"; feature stays "working" (build `c315aa3ad3fc2ec2`) | yes, 2026-11-12 | Brave and Edge verified; Notepad and terminal unconfirmed; 22 actions have no phrase. |

**Production-ready so far: 0 of 15 by the repository definition.** Eight cards have a clean machine (BOT) pass and a ledger line: 2, 9, 20, 21, 27, 61, 65 and, for the one spoken action, 25. Features whose proof kind includes OWNER (2, 9, 14, 20, 25, 27, 31, 57) still need your half: your own voice, the wake word as the way in, and what you see and hear. Cards 64, 69 and 78 (the Hub, solid panels, themes) need your eyes; 40 needs an administrator console; 57 needs a GitHub sign-in.

## 8. After the run

For every card that passes, the assistant:

1. adds one line to [`live-proof-ledger.md`](live-proof-ledger.md) with the date, the commit and release identity, the proof kind, what was run, the result and the log evidence;
2. raises that feature's live value in `scorecard-data.json` and regenerates the work cards (`node scripts/make-work-cards.mjs`);
3. updates the table in section 7 and the count under it.

For every card that fails, the assistant records the failure in the ledger as a failure, leaves the scorecard alone, and fixes the cause before the card is tried again.

## 9. What this run does not cover

- The other 30 "working" features: 9 that need a longer session or a specific set-up, 10 that need Google, GitHub or Gemini sign-in, and 11 that cannot be finished in one day.
- The 33 partial features, including wake-word accuracy and Voice Lock, which need your recordings and trials.
- The memory-key fixes in `b6b0a85`. They are tested, but a restart with a read-back of a remembered fact is still needed. Step "remember that my favourite colour is blue" in 5.13, followed by a restart and "what do you remember", would give that proof for free; it is listed here so it is not forgotten.
