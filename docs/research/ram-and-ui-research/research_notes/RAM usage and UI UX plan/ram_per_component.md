# NEXUS RAM per component: idle, active, peak, lifecycle, and the 600-700 MB ceiling

Researcher notes, 2026-10-29. Read-only: nothing was started, stopped, rebuilt or edited. Sources are repo files (paths relative to `D:\ULTRON`), source code and a live process check. Labels: **MEASURED** (number recorded in a repo doc with its date, or taken by me now), **ESTIMATED** (from code or file sizes, no measurement), **UNKNOWN**. No web sources were used; no URLs apply.

Metric note (from `docs/features/130-ram-usage-by-state-measured-and-estimated.md`, section 0): Windows has three numbers. Commit/private bytes is the safe "will it fit" number. Working set (WS) is resident memory including shared DLL pages. Private working set is what Task Manager shows by default and is heavily trimmed when idle (nexus.exe: 16.5 MB private WS vs 69.6 MB commit). Everything below states which one it is where known.

## 1. What do the repo docs already record as measured (with dates)?

### Takeaway
All hard numbers come from one measurement day, 2026-10-16/17, on the owner's HP i7-1355U / 15.7 GB laptop (docs 130, 131, 132, 136), plus older 2026-08/09 figures from AGENTS.md that describe a superseded architecture. Only the idle tree, Kokoro and the NLU model were really measured; hand mode, extra windows, ghost, agent-watch and offline STT have never been measured.

### Cited Findings
Idle process tree, release build running for hours, stage overlay up, wake word listening (MEASURED 2026-10-16, `docs/features/130-ram-usage-by-state-measured-and-estimated.md` section 1; repeated in `docs/features/136-...compendium.md` section 4.1):

| Process | Commit MB | WS MB | Private WS MB |
|---|---|---|---|
| nexus.exe (Rust core, wake engine, audio, clients, stock-line cache) | 69.6 | 94.1 | 16.5 |
| WebView2 browser process | 39.2 | 51.0 | 11.3 |
| WebView2 renderer (stage page: orb + captions) | 51.6 | 46.2 | 26.9 |
| WebView2 GPU process (WebGL orb) | 82.7 | 12.4 | 4.5 |
| WebView2 utility x2 | 20.1 | 25.1 | 4.5 |
| WebView2 crashpad | 3.4 | 3.7 | 0.7 |
| conhost (dev launch only) | ~1-2 | 3.1 | 0.4 |
| **TOTAL idle** | **268.9** | **235.6** | **64.8** |

- Peak seen by nexus.exe since start: WS 165.6 MB, commit 81.2 MB (MEASURED 2026-10-16, doc 130 section 1).
- Kokoro local TTS (int8 `model_quantized.onnx`, 88.1 MB file), measured in a unit-test process (adds about 10-15 MB harness overhead), WS (MEASURED 2026-10-16/17, doc 131 and doc 136 section 4.2):
  - harness baseline about 15 MB;
  - pronunciation dictionary built (misaki-rs parses about 14 MB JSON into hash maps): +about 125 MB (139-153 total);
  - ONNX session loaded: +135 MB (275-288 total);
  - after first synthesis: +64 MB (339-350);
  - after 5 syntheses: 368-379 MB;
  - after `unload()`: 59-75 MB, i.e. about 304 MB returned;
  - peak WS 395.6 MB / commit 358.8 MB (doc 130 section 2). Net cost on top of idle: about +340-360 MB commit, +360-400 MB resident.
- ONNX variants (MEASURED, doc 131 / doc 136 section 4.3, release test build, laptop on battery): default 379 MB and RTF 2.40; arena + memory-pattern off 277 MB but RTF 3.21 (34 % slower); 4 threads arena off 276 MB; 2 threads arena off 275 MB. Doc 136 problem 12 records that arena-off is therefore "not a default". Speed figure is pessimistic (battery, 14 % charge). An earlier AC figure RTF 0.7-1.7 is recorded (R) and not re-measured.
- NLU in-process model (tract BERT-Mini): +42 MB, first parse 662 ms, about 37 ms afterwards, only about 23 MB returned on drop; one run in a unit-test process (MEASURED 2026-10-16, doc 132 section 0 and doc 136 section 4.2). Model files 16.8 + 16.8 MB (doc 130 section 3). The earlier 60-100 MB estimate was wrong and replaced by 42 MB.
- Older recorded numbers (AGENTS.md "Multi-Worker Optimization", 2026-09-01): idle about 104 MB before the first transcription, about 232 MB after STT model loaded, about 582 MB with Kokoro; nexus.exe 47.8 MB, orb WebView2 35.8 MB, STT Python 20.6 -> 128.6 MB. AGENTS.md "RAM Optimization" (2026-08-30): 5 windows at boot = 1,644 MB, after lazy windows = 385 MB total (nexus.exe 40.8, WebView2 one window 344, STT 0). Doc 130 section 6 states these come from the pre-stage-overlay architecture and are superseded by the 2026-10-16 idle tree.
- Moonshine/Python STT fallback (RECORDED, not re-measured, AGENTS.md 2026-09-01 and doc 130 section 3): +130 MB loaded, +340 MB while transcribing, medium model about 400 MB. AGENTS.md also lists Moonshine sizes: tiny about 100 MB, small about 300 MB, medium about 400 MB. Default is now `small_streaming` (AGENTS.md 2026-10-05, RAM budget).
- One extra WebView2 window: about 250 MB per window recorded on 2026-09-01 before low-memory mode (doc 130 section 3, labelled R/E). Low-memory mode saves about 40 MB per AGENTS.md 2026-09-01.
- EcoQoS: declared in `src-tauri/src/power.rs` (change 101, `docs/changes/101-cpu-ram-battery-energy-optimizations.md`); it is a CPU-scheduling hint, no RAM figure recorded. Windows then trims idle working set, which is why private WS is only 16.5 MB.
- `docs/research/performance/01-cpu-ram-battery-optimization-plan.md` (2026-10-07) quotes a daemon RAM target of about 45-52 MB and stage WebView2 about 110-140 MB after optimisation, and a leak concern of up to +100 MB over time from detached canvases. These are plan figures (ESTIMATED), not measured. The measured 2026-10-16 tree (nexus.exe 94 WS, WebView2 138 WS) is consistent with the WebView2 range and higher than the 45-52 MB daemon target.
- Two further doc-132/136 points: no `[RAM]` sampler was ever built (doc 136 line "Not built: `[NET]`/`[RAM]` samplers"). My search of `src-tauri/src` found no `ram_budget.rs` and no `[RAM]` log line, and `nexus_unified.log` (last written 2026-10-29 01:50) contains none.

### Inferences
- The only trustworthy measured baseline is the 2026-10-16 idle tree (269 MB commit / 236 MB WS). Anything above it is partly estimate.
- Kokoro is the single largest item (about 300-400 MB) and the only big one that is both measured and unloadable.

### Gaps
- No measurement exists for: hand mode, Command Hub / Settings / Sidebar / Architect window, ghost mode, agent-watch Node sidecar, offline STT on the current default model, Qwen brain, CodeRabbit CLI, memcore in-memory blob, the setup window. These are all still "must measure" in doc 132 section 7.
- No release-build measurement with the owner's current (late October) build. All measured numbers are about 13 days old and predate many features (notify hub, Focus Mode, deploy watchers, loading registry, Voice Lock enrollment).

## 2. Per-component table (idle / active / peak) with source and lifecycle

### Takeaway
Always-on cost is about 236 MB WS (269 MB commit); everything else is on demand and idle-unloaded, except the NLU model and the Qwen brain, which stay loaded for the session once started. Only the first three rows are measured.

### Cited Findings
Table. Figures are resident MB unless marked "commit". Source key: M = MEASURED (doc/date), E = ESTIMATED, U = UNKNOWN.

| Component | Idle | Active | Peak | Source | Lifecycle (from code) |
|---|---|---|---|---|---|
| nexus.exe core (Rust, wake engine tract/ONNX mel+embedding+nexus.onnx, audio, orchestrator, stock-line cache, memcore, pollers) | 94 WS (70 commit; 16.5 private WS when trimmed) | not separated | 166 WS / 81 commit | M 2026-10-16 doc 130 | Always on. Wake models: `src-tauri/resources/oww/` (melspectrogram.onnx, embedding_model.onnx, nexus.onnx; sizes not measured here). EcoQoS set at start (`power.rs`). |
| Stage WebView2 tree (6 processes: browser, renderer, GPU, 2 utility, crashpad) incl. voice orb WebGL | 138 WS (197 commit; GPU process 83 commit but 12 WS) | U (orb animating at 60 fps) | U | M 2026-10-16 doc 130 | Always on: stage is the fullscreen click-through overlay window; the orb, captions, loading indicator and hand overlay are `<div>`s inside it (AGENTS.md 2026-10-04 consolidation). WebView2 low-memory target set in `mic_permissions.rs::set_low_memory_mode` (SetMemoryUsageTargetLevel LOW). Orb WebGL loop paced to about 25 fps idle and halts on `document.hidden` (change 101); particles 5,200 default, halved on battery (Avatar.tsx). Watchdog in `stage.rs` recreates the window on blackout. |
| Edge TTS (cloud voice) | 0 model; stock-line cache about 4-5 MB (inside idle) | +5-15 (96 KB per second of decoded 24 kHz f32; 30 s reply 2.9 MB) | U | E, doc 130 section 3 | Cloud, persistent shared connection (AGENTS.md 2026-10-17). No model RAM. |
| Kokoro local TTS (offline voice) | 0 (unloaded). Log line seen: "Edge TTS (cloud) active - Kokoro standby (unloaded, 0 MB RAM)" | +340-360 commit / +360-400 WS speaking | WS 395.6 / commit 358.8 | M 2026-10-16/17 docs 130, 131 | Lazy. Loaded when offline or when a local voice is needed. `tts_network.rs`: unloaded after 10 min of stable online (`LOCAL_GRACE_PERIOD` 600 s, checked every 30 s) or after 10 min with no local synthesis (`LOCAL_IDLE_UNLOAD` 600 s). `nexus_unified.log` 2026-10-28T19:31:47 shows "Kokoro unloaded - network stable, ~260-370 MB RAM freed". Files are in `src-tauri/resources/kokoro/` (model_quantized.onnx, voices). |
| Groq STT (cloud) | 0 | tiny (16 kHz f32 = 0.64 MB per 10 s of capture) | U | E doc 130 | Cloud; shared reqwest client (AGENTS.md 2026-10-07). No RAM beyond buffers. |
| Moonshine / Python STT sidecar (`server/stt_server.py`, port 39217) | 0 (not running) | +130 loaded / +340 transcribing (small); medium about 400 | U | R (AGENTS.md 2026-09-01, doc 130); not re-measured | Lazy. `lazy_stt.rs`: `STT_IDLE_TIMEOUT` 60 s, `STT_KEEP_ALIVE=false`, so it is killed 60 s after the last request, except while a Ghost session is active (`idle_kill_due`). Spawned only when Groq is unavailable / localSttOnly. Note: older AGENTS.md text says the sidecar is kept alive permanently; the current code says otherwise. |
| NLU BERT-Mini (in-process tract, `nlu_local.rs`) | 0 until first use | +42 (after load) | U | M 2026-10-16 doc 132 | Lazy but pre-warmed on the wake word (`wakeword_oww.rs` calls `nlu_local::prewarm()`). I found no idle-unload in `nlu_local.rs` (it holds an `RwLock<Option<Arc<NluModel>>>`); the 5-minute unload is a plan only (doc 132). The old Python NLU sidecar `lazy_nlu.rs` no longer exists in `src-tauri/src`; `server/nlu_server.py` remains in the repo. Treat as resident for the rest of the session once loaded. |
| Qwen brain (admin-only, `lazy_brain.rs`, `server/admin/brain_server.py`, Qwen2.5-0.5B GGUF 480 MB file) | 0 on family builds (compiled out without `admin-brain` feature) | U | U | E (file size only) | Spawned non-blocking on first need; "No idle timeout (always loaded)" per `lazy_brain.rs` header. Admin builds only, so excluded from the owner-facing ceiling unless the owner is the admin. |
| Command Hub / Settings / Sidebar (unified window `sidebar`), Architect, Setup | 0 (window not created) | +100-250 per window | U | R/E (about 250 MB recorded 2026-09-01 before low-memory mode; doc 130 estimate 100-250) | On demand via `dyn_windows::get_or_create_window`; closing destroys the WebView2 so memory is returned (AGENTS.md 2026-08-30). Sidebar, settings and architect views are one unified window since 2026-10-01. Settings window about 400 px wide, full-height. |
| Hand Mode (window `hand`, MediaPipe hand_landmarker 7.5 MB model + 9.1 MB wasm, camera frames) | 0 | +150-300 | U | E doc 130 (never measured) | Created on demand ("hand mode"), destroyed on exit; camera only while on; auto-exit after 60 s with no hand (`handIdleExitMs`), also AGENTS.md note that the hand window must stay visible. GPU delegate calibration on first 25 frames. |
| Ghost Mode (UIA caches, hot-mic loop, ring) | 0 | +10-30 | U | E doc 130 | No new process; session-scoped. Keeps the Moonshine sidecar warm (no idle kill). |
| Agent-watch Node sidecar (`agent_watch.rs`, node-pty + coding-agent-adapters, about 68 MB on disk) | 0 | +40-120 commit / +30-50 WS | U | E doc 130 (analogy to local node processes) | Lazy; `SIDECAR_IDLE_TIMEOUT` 10 min, idle monitor thread. Not bundled; needs Node. |
| memcore (sealed SQLite blob held in memory, `memcore/store.rs`, `memcore.enc`) | included in nexus.exe | grows with memory size | U | E | The database is held in memory and rewritten as one sealed blob per write (doc 109 / AGENTS.md 2026-10-07). Size of the owner's real store: UNKNOWN (no `memcore.enc` was found in the app-data listing I took). |
| CodeRabbit CLI / AI review jobs (`coderabbit.rs`, `coderabbit_review.rs`) | 0 | U; spawns an external CLI process per job (`Command::new`), timeouts 20 s status / 300 s login / 90 s install | U | U | Short-lived child processes, not resident. Their RAM is outside NEXUS's own accounting. No measurement. |
| Notification hub (`notify_hub.rs`, `notify_brain.rs`, native toasts) | included in core | negligible (state tables) | U | E | In-process, always on; state persisted in the sealed store. |
| Background pollers (sentinel, mailwatch, github_watch, deploy_watch, leavewatch, resume, scheduler) | included in core | each poll allocates HTTP buffers briefly | U | E | In-process tokio tasks, always on when their setting and credentials exist. Cadences from code and AGENTS.md: Sentinel loop 15 s (`google/sentinel.rs`, silent sleep without a Google token); mail 90 s active / 8 min away (AGENTS.md); GitHub Actions watch 2 min with ETag, 5 min backoff (`github_watch.rs`); deploy watch 5 min (`deploy_watch.rs`); resume sampler 30 s. They add CPU wakeups rather than meaningful RAM. |

### Inferences
- Always-on baseline = core + stage tree. Sum of the measured rows: 94 + 138 + about 3 = 236 WS. If the Windows trim has not happened (right after activity) resident climbs back toward the peak numbers (nexus.exe up to 166 WS).
- Pollers and notification hub live inside nexus.exe's heap; they are part of the 94-166 MB core, not separate.
- Docs 130/132 and AGENTS.md disagree with each other in several places because they describe different architectures (see section 5 for the conflicts to flag).

### Gaps
- Hand, extra windows, offline STT, ghost, agent-watch active and peak columns are UNKNOWN or estimates. Peak for most components is UNKNOWN.
- No per-window figure after the low-memory mode was applied to dynamic windows is recorded anywhere. Whether `set_low_memory_mode` runs for every dynamic window or just those passing the permission handler: the code path in `mic_permissions.rs` line 51 is invoked from `allow_media_for_window`, which the AGENTS.md says is registered per dynamic window (hand), so it is not clear that the Hub/Settings windows get it. UNKNOWN.

## 3. Boot-time vs on-demand, idle-unload timers, and stacking against the ceiling

### Takeaway
At boot only the Rust core and the stage WebView2 tree exist (about 236 MB WS). Nothing else is loaded until used. Idle unloads exist for Kokoro (10 min), Moonshine (60 s), agent-watch (10 min) and windows (destroyed on close); NLU and Qwen brain have no idle unload. The 600-700 MB ceiling is exceeded in several plausible stacks, most easily by Kokoro plus any extra window plus hand mode.

### Cited Findings
Timers (code):
- Kokoro: 600 s stable-online grace and 600 s idle (`tts_network.rs` lines 31-32, 211-212); network checked every 30 s.
- Moonshine STT: 60 s idle (`lazy_stt.rs` line 25), skipped during a Ghost session.
- Agent-watch sidecar: 10 min (`agent_watch.rs` line 31).
- NLU: none found (`nlu_local.rs`); pre-warm on wake word.
- Qwen brain: none (`lazy_brain.rs`).
- Hand Mode idle exit: 60 s with no hand (`handIdleExitMs`, AGENTS.md 2026-10-14); window destroyed on exit.
- Windows: destroyed, not hidden, on close (`dyn_windows.rs`).

Stacking arithmetic. Use the most reliable numbers: base 236 WS, NLU +42, Kokoro +360-400, Hub/other window +100-250 (estimate), hand +150-300 (estimate), Moonshine +130 to +340, agent-watch +30-50, ghost +10-30. WS values (MEASURED base and Kokoro, the rest ESTIMATED):

| Scenario | Computed total WS | Against 600-700 MB |
|---|---|---|
| Idle, cold | 236 (MEASURED) | well under |
| Idle, NLU warm | about 278 | under |
| Normal online turn (speaking via Edge, ghost) | 290-350 (doc 130) | under |
| Online + Command Hub window open | about 385-575 (doc 130 range) | under, near top |
| Offline, Kokoro loaded and speaking | 236 + 42 + 360-400 = about 640-680; doc 130 says 650-730 | at the ceiling; upper end exceeds 700 |
| Offline + Moonshine active (Groq unreachable) | 990-1,070 (doc 130 section 4 row 10) | exceeds |
| Online + hand mode + Hub window + NLU | 236 + 42 + 150-300 + 100-250 = about 530-830 | can exceed |
| The task's example: Kokoro + Moonshine sidecar + NLU + 3 windows open | 236 + 42 + 360-400 + 130-340 + 300-750 = about 1,070-1,770 | exceeds by 400-1,000+ MB |
| "Everything on" (doc 130 state 11) | 1.1-1.5 GB (doc 132: 1.2-1.6 GB) | exceeds by 400-900 MB |

In practice Kokoro and Moonshine are mutually exclusive in normal operation: Kokoro loads only when voice must be offline and Moonshine only when Groq is unreachable. These coincide exactly in the offline case, which is the worst realistic scenario (doc 130 state 10).
- The documented plan (doc 132) says "everything on under 700 is reachable but tight", needs a `ram_budget.rs` governor (not built) and unbuilt reductions: compact pronunciation dictionary (-100 to -115, measured size), idle unload of NLU, ORT-format model (-40 to -90, estimate), orb GPU release, hand hosted in the stage WebView, in-process offline STT.
- The owner clarified that 700 is a goal, not a hard-coded cap, and that speed beats RAM (doc 132 addendum, doc 136 section 1, AGENTS.md 2026-10-17 "speed first").

### Inferences
- Under normal online daily use (no Kokoro, no sidecars, at most one window) NEXUS sits at about 240-400 MB WS, comfortably below the ceiling.
- The ceiling is breached by (a) the offline combination Kokoro + Moonshine, (b) Kokoro plus the Hub, or (c) hand mode plus a second window plus warm NLU. Each of these is plausible but short lived because of the idle timers, except that nothing unloads the NLU or the Qwen brain.
- Because Windows trims idle processes (EcoQoS, low-memory WebView2), Task Manager's default column will read much lower than WS or commit; for a ceiling check use WS or commit.

### Gaps
- Whether the late-October features added resident state (notification hub, loading registry, notify brain, Voice Lock profile / speaker embedding model, speaker verification ONNX) is UNKNOWN; no figure.
- Whether the Hub and Settings windows still cost 100-250 MB under low-memory mode is UNKNOWN; the 90 MB target in doc 132 is only a goal.
- Speaker-embedding / Voice Lock models (AGENTS.md 2026-10-24 to 2026-10-28) have no RAM measurement in any doc I read.

## 4. Live process check (made now)

### Takeaway
NEXUS was not running when I checked, so I measured nothing of NEXUS and did not start it.

### Cited Findings
- Check time: 2026-10-29 18:29:13 local, via PowerShell `Get-Process`.
- No `nexus` process and no `python`/`pythonw` process existed (MEASURED now).
- There were 6 `msedgewebview2` processes (started 11:48:55, parent PID 14040, whose command line has `--webview-exe-name=SearchHost.exe` and a user-data dir under `MicrosoftWindows.Client.CBS_cw5n1h2txyewy`). These belong to Windows Search/Start, not NEXUS: WS 7.1, 13.0, 33.7, 12.6, 9.3, 9.3 MB, private bytes 3.5, 60.1, 40.7, 12.6, 8.4, 48.8 MB (MEASURED now). Do not attribute them to NEXUS.
- Also present: 9 `node` processes (19.9-93.1 MB WS, 21-114 MB private) and 3 `node_repl` (13.5-14.5 MB WS), started 17:04-18:22. These are tooling for the current Claude session, not the NEXUS agent-watch sidecar, which would be a child of nexus.exe. They are shown for scale only: a bare Node process here costs 20-93 MB WS, which supports the 30-50 MB WS agent-watch estimate.
- Most recent NEXUS log (`C:\Users\Chitkul Lakshya\AppData\Roaming\com.nexus.assistant\nexus_unified.log`, last written 2026-10-29 01:50) contains no memory sampler lines. It does record, on 2026-10-28: "Edge TTS (cloud) active - Kokoro standby (unloaded, 0 MB RAM)" and at 19:31:47Z "[tts_net] Kokoro unloaded - network stable, ~260-370 MB RAM freed", confirming the Kokoro unload path runs in the real app. `stt_server.log` was last written 2026-08-29 and has no RAM lines.

### Inferences
- A fresh real-world idle measurement needs the owner to start NEXUS; I did not.

### Gaps
- No current-build measurement of any NEXUS process exists; the latest is 2026-10-16.

## 5. Conflicts and caveats the report writer should carry forward

### Takeaway
Several numbers in the docs are superseded or in tension; use the dated measurements, not the older prose.

### Cited Findings
- AGENTS.md "Multi-Worker Optimization" idle figures (104-232 MB) vs doc 130 idle tree (236 MB WS / 269 MB commit): different architectures; doc 130 says the older ones are superseded.
- AGENTS.md says STT is kept alive permanently (`STT_KEEP_ALIVE=true`) in one section; `lazy_stt.rs` now has `STT_KEEP_ALIVE=false` with a 60 s idle kill and a comment "strict <150MB RAM limit". Trust the code.
- Doc 132 first says NLU costs 60-100 MB (E) then corrects to 42 MB (M). Use 42.
- Doc 131 recommends ONNX arena-off (-88 MB); doc 136 problem 12 reverses this because it is 34 % slower; do not present arena-off as shipped.
- The owner's stated ceiling is 600-700 MB (doc 132, "hard cap 700, soft 600-650"), later softened to "a goal, not a hard-coded cap"; doc 136 open item asks whether the all-on ceiling should be 700, 850 or "Generous".
- `ram_budget.rs`, the `[RAM]` sampler and the Hub "Memory" card are PLAN only; none exist in `src-tauri/src` (my grep for `ram_budget` found nothing).
- Measurement caveats from doc 136 section on pitfalls: Windows trims idle working sets; unit-test processes add about 15 MB; debug vs release; battery vs AC.

### Inferences
- A correct summary for the owner: online and idle is far below 700 MB; offline with local voice is at 650-730 MB; offline with the Python STT fallback is about 1 GB; and "everything on" is about 1.1-1.6 GB. The first two rows rest on measurements, the rest on estimates.

### Gaps
- Does not cover browser-side costs of user apps NEXUS launches, or Python install footprint on disk.
