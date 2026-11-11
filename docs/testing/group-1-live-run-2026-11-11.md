# Group 1 live run — 15 features closest to production-ready

| | |
|---|---|
| **Date opened** | 2026-11-11 |
| **Status** | **Built, not run.** No feature in this file is production-ready yet. |
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
| a | NEXUS idle. Press Ctrl+Space, say nothing. | Orb appears and listens, then hides by itself. No "didn't catch that" speech. |
| b | Press Ctrl+Space, then press it again within a second. | Listening is cancelled, orb hides. |
| c | Press Ctrl+Space, say "what is the capital of France". While it is thinking, press Ctrl+Space. | The request is cancelled. No answer is spoken afterwards. |
| d | Ask the same question, let it start speaking, press Ctrl+Space. | Speech stops at once. The orb goes idle. Volume returns to your level. |
| e | Open the Command Hub (Ctrl+Shift+S). Press Ctrl+Space. | The Hub stays open. NEXUS is called. |
| f | Press Ctrl+Space, say "open Notepad". | Notepad opens. This shows a call after several cancels still works. |

**Pass:** all six behave as expected, and the log shows one `CALLING agent` or `CANCELING calling agent` line per press with the right state flags.
**Fail:** a press is ignored, a cancelled answer is still spoken, the orb stays on screen, or the Hub closes.

**Result:** NOT PASSED on the first attempt (2026-11-11 23:09 IST, build `edfa406ab1f4f62a`). Steps a and b only; the owner reported "something went wrong" and has not yet described what was seen. The log shows four presses, each logged once with correct state flags, and three findings:

| # | Time (UTC) | What the log shows | Finding |
|---|---|---|---|
| 1 | 17:39:22 to 17:39:28 | NEXUS spoke "Claude Code has finished, sir." (agent-finished alert), then opened the microphone by itself (capture session 6). The owner's first press 2.5 s later was therefore treated as a **cancel**, not a call. | An alert that asks nothing should not open the microphone. To the owner, the first press appears to do nothing. |
| 2 | 17:39:30 to 17:39:44 | Press, call, 8 s of silence, then a second 8 s listening window started silently (session 9). | A silent press keeps the orb up for about 16 s, not 8 s. This is the designed one silent retry; the card's "hides by itself" expectation was too short. |
| 3 | 17:39:46 to 17:39:55 | Press, call, 8 s of silence, then NEXUS said "I didn't hear you, sir." | The miss counter was not reset by the owner's cancel or by the new press, so a first silent press after an earlier miss is answered aloud. Step a expects no speech. |
| 4 | 17:39:31 to 17:40:01 | The local speech engine failed to start: `ModuleNotFoundError: No module named 'moonshine_voice'`; "STT server did not become ready in 15s". | With no Groq key and no local engine, **no spoken command can be understood on this PC**. Steps c, d and f, and every spoken card, are blocked until one of the two is in place. |

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

**Result:** PENDING

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

**Result:** PENDING

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

**Result:** PENDING

### 5.5 9Router (#21)

**Proves:** general questions are answered by a direct provider (Cerebras, Groq or Gemini) and not sent through the Worker.

No extra steps. The assistant reads the same three turns from 5.4.

**Pass:** each turn has a `9router: trying <provider>` line followed by a success, and the `answer` stage of `[LAT]` is recorded. Earlier measurement: about 242 ms.
**Fail:** every provider fails and the turn falls back to the Worker for a plain question, or a provider key is reported missing.

**Result:** PENDING

### 5.6 Volume lease and restore (#61)

**Proves:** NEXUS raises the volume only while it speaks and puts your level back afterwards, including after a cancel.

The assistant reads the Windows master volume before 5.4, during the long reply in 5.4 step b, two seconds after it ends, and after the cancel in 5.1 step d.

**Pass:** the volume after each reply equals the number you named, within one step; the log shows a matching `[VOL] saved original` and `[VOL] restoring original volume` pair for each reply; no `watchdog: … force-restoring` line.
**Fail:** the volume is left at the speech level, or drops between sentences of one reply.

**Result:** PENDING

### 5.7 Keep-awake (#40)

**Proves:** Windows is told to stay awake only while NEXUS is working or speaking, and the request is cleared straight after.

The assistant runs `powercfg /requests` (needs an administrator console; if it is not available the log lines alone are recorded and the proof is marked partial) before, during the long reply of 5.4 step b, and five seconds after it.

**Pass:** no NEXUS entry before, a NEXUS `DISPLAY`/`SYSTEM` entry during, none after; the log shows `[KEEPAWAKE] asserted` then `[KEEPAWAKE] cleared`.
**Fail:** the request is still listed after the reply ends.

Earlier proof (2026-10-20) was on older code, before pinned holds were added, so it does not carry over.

**Result:** PENDING

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

**Result:** PENDING

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

**Result:** step a PASS (preliminary, above); steps b and c PENDING

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

**Result:** PENDING

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

**Result:** PENDING

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

**Result:** PENDING

---

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
| 2 | Global hotkey Ctrl+Space | NOT PASSED (first attempt, steps a and b) | no | Mic opens by itself after an agent alert; miss counter survives a cancel; spoken steps blocked (no speech engine). See 5.1. |
| 31 | Open / close / focus apps | PENDING | no | |
| 14 | Wake-up sound | PENDING | no | |
| 9 | Online voice (Edge TTS) | PENDING | no | |
| 21 | 9Router | PENDING | no | |
| 61 | Volume lease and restore | PENDING | no | |
| 40 | Keep-awake | PENDING | no | |
| 65 | Resource footprint | idle reading taken, after-use readings PENDING | no | New build: 523.7 MB at 32 s, 186.9 MB at 143 min idle |
| 57 | Cloudflare Worker | health PASS, turn PENDING | no | 200 in 115 ms |
| 64 | Command Hub / settings UI | PENDING | no | |
| 69 | Solid panel windows | PENDING | no | |
| 78 | Themes and contrast | PENDING | no | |
| 20 | Main centre and sub-centres | PENDING | no | 11 of 18 centres in scope |
| 27 | File control with undo | PENDING | no | |
| 25 | Shortcut map and action router | PENDING | no | Only "new tab" is reachable by voice |

**Production-ready so far: 0 of 15.**

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
