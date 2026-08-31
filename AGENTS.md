# NEXUS — Project Notes

## STT Architecture — Moonshine In-Process (2026-08-31)

**STT is now in-process Rust — no Python sidecar.**

The old faster-whisper Python server (`server/stt_server.py`) and its
lazy manager (`src-tauri/src/lazy_stt.rs`) have been removed. STT now
uses **Moonshine Tiny** via the `transcribe-rs` crate, loaded directly
inside the Tauri process:

- **File:** `src-tauri/src/stt.rs`
- **Model:** `MoonshineVariant::Tiny`, `Quantization::Int8`
- **First-run:** `transcribe-rs` auto-downloads the Moonshine ONNX model
  to the Hugging Face cache dir (`~/.cache/huggingface` on Linux/macOS,
  `%USERPROFILE%\.cache\huggingface` on Windows). Requires network on
  first transcription only.
- **Latency:** in-process, no IPC, no port, no idle timeout.
- **Hallucination filter:** still applied in `stt.rs` — catches
  "thank you for watching", < 2 alphabetic chars, etc.

The old `lazy_stt.rs` / port 39217 / `stt_server.py` references in this
file are **historical** — kept for context but no longer apply at runtime.

## NLU Server — Lazy Python Sidecar (2026-08-31)

The **NLU server** (`server/nlu_server.py`) is the only remaining Python
dependency. It provides ML-based intent classification (BERT-Mini ONNX)
as a fallback when the deterministic parser (`intent_parser.rs`) can't
handle a command.

- **Port:** `39218` (separate from the old STT port 39217)
- **Lazy manager:** `src-tauri/src/lazy_nlu.rs` — spawns on first
  unparseable command, kills after 60s idle.
- **Model:** `server/nlu/model/nexus_nlu.onnx` + `.data` + `tokenizer/`
  — committed to git (~18 MB) so fresh clones work without downloading.
- **Requirements:** `server/nlu/requirements.txt` (numpy, onnxruntime,
  fastapi, uvicorn, pydantic, transformers).
- **Fallback:** if the NLU server is unavailable, `nlu_client.rs`
  returns `None` and the deterministic parser handles the command.

### Historical STT Pipeline Fixes (2026-08-30, faster-whisper era)

These bugs were fixed in the old faster-whisper Python sidecar. They are
**no longer relevant** (the sidecar was replaced by in-process Moonshine)
but kept for historical context:

1. **`lazy_stt.rs` path bug:** `stt_script_path()` was missing one
   `.parent()` level. Fixed by adding the correct path.
2. **`ensure_stt_running()` not called on hotkey:** Fixed by adding
   calls to `hotkey.rs` and `stt.rs`.
3. **`is_stt_responsive()` used tokio runtime:** Fixed by using a raw
   TCP connection instead.
4. **STT idle timeout too aggressive:** 60s → 5 minutes.

### Whisper hallucination filter (`stt.rs`)

The hallucination filter is still active in the new Moonshine-based
`stt.rs`. It catches common hallucinations on noisy/silent audio:
- "thank you for watching", "you", "bye", "okay", etc.
- Text with < 2 alphabetic characters
Filtered text is replaced with empty string, triggering the frontend's
"didn't catch that" retry logic (up to 3 retries).

## NEXUS CLI (`scripts/nexus.bat`)

A simple command-line interface for NEXUS:

```
nexus start       Start NEXUS with unified console
nexus stop        Stop NEXUS and STT server
nexus status      Check if NEXUS is running
nexus logs        Tail NEXUS logs in real-time
nexus build       Rebuild NEXUS
nexus diagnostics Check all service connections
nexus help        Show help
```

Added to user PATH. Available from any terminal after restarting it.

## Connection Diagnostics (`src-tauri/src/diagnostics.rs`)

Checks 5 services and logs a formatted table on startup:

| Service | Check method | Expected |
|---------|-------------|----------|
| STT | TCP connect to port 39217 + HTTP GET /health | OK if running |
| TTS | Check settings.json for provider keys | Always OK (Web Speech fallback) |
| Cloudflare Worker | HTTPS GET to /health | OK if reachable |
| GitHub | HTTPS GET to Worker /oauth/status | OK if OAuth connected |
| Google | HTTPS GET to Worker /oauth/status | OK if OAuth connected |

Also available as:
- Tauri command: `nexus_diagnostics` (returns JSON to frontend)
- CLI: `nexus diagnostics` (formatted terminal output)
- Startup: auto-logged 5s after boot

## Wake Word Model Validation + Mic Silence Recovery (2026-08-30)

### Model is PERFECT — the problem is the Intel SST mic driver

Tested the v2 `nexus.onnx` model with the exact Rust pipeline
(mel → normalize → slice[4:80] → embedding → classifier):

| Input | Model probability | Verdict |
|-------|------------------|---------|
| TTS "nexus" | 0.994 | ✅ |
| TTS "hey nexus" | 0.999 | ✅ |
| TTS "nexus wake up" | 0.999 | ✅ |
| TTS "ok nexus" | 0.999 | ✅ |
| 20 negative samples | 0.0001-0.0002 | ✅ perfect rejection |
| **Trigger rate** | **5/5 positives** | ✅ 100% recall on TTS |
| **False positive rate** | **0/20 negatives** | ✅ 0% false triggers |

The model is NOT the problem. The problem is the **Intel Smart Sound
Technology driver** — it stops delivering audio after 2-25 minutes of
use (RMS drops to exactly 0.000000 and stays there).

### Silence Recovery Thread (`wakeword_oww.rs`)

Added a background thread that monitors the audio callback counter and
automatically restarts the cpal stream when the mic goes silent.

**Settings (tuned for Intel SST bursty audio):**
- Poll interval: **5s** (was 30s)
- Silence threshold: **165 callbacks (~5s)** (was 1000/30s)
- Restart method: **`try_device_silent`** (no 5s probe — saves 5s per cycle)
- Nuclear option: every **12 restarts (~60s of silence)**, restarts the
  Windows Audio service (`net stop/start Audiosrv`) to try to unstick the
  Intel SST driver
- Total restart cycle: **~5s** (was 35s with probe)

**Why 5s?** The Intel SST driver delivers audio in brief 5-15s bursts after
each stream restart, then goes silent. A 5s poll gives us the maximum
number of chances to catch a working window.

**Confirmation RMS threshold lowered from 0.01 to 0.002:**
The 500ms confirmation window was rejecting valid wakes because the mic
fades to silence during the confirmation period. At 0.01, a wake with
RMS=0.0048 was rejected. At 0.002, it would be confirmed.

### Intel SST driver fix (requires admin)

When the mic goes permanently silent, the fix is to restart the driver:

```powershell
# Run as Admin:
pnputil /restart-device "INTELAUDIO\CTLR_DEV_51CA&LINKTYPE_02&DEVTYPE_00&VEN_8086&DEV_AE20&SUBSYS_8BE0103C&REV_10EC\5&111f6c68&0&0000"
```

Or: Device Manager > Sound, video and game controllers > Intel Smart
Sound Technology for Digital Microphones > right-click > Disable > Enable.

Or: Restart the Windows Audio service:
```powershell
Restart-Service -Name "Audiosrv" -Force
```

If none of these work, a full OS restart is required. The Intel SST
driver has a known bug where it stops delivering audio after some time.
Updating to the latest driver from the laptop manufacturer (HP) may help.

### Test scripts (in project root, gitignored)

- `test_wake_model.py` — tests the model with TTS + negative samples
- `test_live_mic.py` — records 5s from the mic and tests the model
- `test_all_devices.py` — tests all audio input devices
- `test_mic_freq.py` — records and shows frequency content
- `gen_tts.py` — generates TTS "NEXUS" samples via Windows SAPI

## RAM Optimization — Lazy Windows + Lazy STT (2026-08-30)

**Idle RAM: 384 MB** (down from 1,644 MB — 77% reduction).

### What was wrong
- `tauri.conf.json` created 5 windows at startup (main, setup, settings,
  sidebar, architect). Each WebView2 window spawns ~7 processes (~250 MB).
  4 of the 5 windows were `visible: false` but still consumed full RAM.
- The STT server (faster-whisper tiny.en) ran constantly, using ~340 MB
  even when no one was speaking.

### Fix 1: Lazy window creation (`src-tauri/src/dyn_windows.rs`)
- Only `main` (orb) is in `tauri.conf.json` — created at startup.
- `setup`, `settings`, `sidebar`, `architect` are created on-demand by
  `dyn_windows::get_or_create_window()` when first needed.
- `hide_sidebar` / `close_setup_window` / `close_settings_window` now
  **destroy** the window (not `hide()`) — kills the WebView2 process tree
  and frees ~250 MB per window.
- Platform effects (DWM corners, macOS vibrancy) applied at creation time
  inside `get_or_create_window()`.

### Fix 2: Lazy STT server (`src-tauri/src/lazy_stt.rs`)
- STT server is NOT started at boot (removed from `scripts/run.ps1`).
- `lazy_stt::ensure_stt_running()` is called when the wake word fires.
- `lazy_stt::mark_stt_request()` resets the idle timer on each transcription.
- A background thread kills the STT server after 60s of no requests.
- If an external STT server is already running on port 39217 (e.g. started
  manually), the lazy manager detects it and skips spawning its own.

### Measured RAM (idle, after fix)
| Component          | Before   | After    |
|--------------------|----------|----------|
| NEXUS.exe (Rust)   | 47.9 MB  | 40.8 MB  |
| WebView2 (1 window)| 870 MB   | 344 MB   |
| STT server         | 339 MB   | 0 MB     |
| **TOTAL**          | **1,644 MB** | **385 MB** |

### Files changed
- `src-tauri/tauri.conf.json` — removed 4 windows, kept only `main`
- `src-tauri/src/dyn_windows.rs` — NEW: dynamic window creation/destruction
- `src-tauri/src/lazy_stt.rs` — NEW: lazy STT server manager
- `src-tauri/src/lib.rs` — registered new modules, removed startup sidebar vibrancy
- `src-tauri/src/commands.rs` — all show/hide functions use dyn_windows
- `src-tauri/src/architect.rs` — uses dyn_windows for architect window
- `src-tauri/src/hotkey.rs` — sidebar close uses destroy_window
- `src-tauri/src/tray.rs` — settings menu uses dyn_windows
- `src-tauri/src/wakeword_oww.rs` — calls ensure_stt_running() on wake
- `src-tauri/src/stt.rs` — calls mark_stt_request() on each transcription
- `scripts/run.ps1` — no longer starts STT server at boot

## Sidebar — Do NOT use window-vibrancy on non-activating windows (2026-08-30)

**`src-tauri/src/lib.rs` / `src-tauri/src/commands.rs`: the sidebar window
deliberately calls NO `window_vibrancy` function** (no `apply_blur`,
`apply_acrylic`, `apply_mica`). This was a hard-won finding — do not
re-add these calls without reading this section first.

**Why**: the sidebar is a non-activating window (`focus: false`,
`alwaysOnTop: true`, `skipTaskbar: true`) so it never steals keyboard
focus from whatever app the user is working in. Windows' DWM *material*
APIs (Acrylic/Mica via `DWMWA_SYSTEMBACKDROP_TYPE`, or the legacy
`SetWindowCompositionAttribute` accent path used by `apply_blur`) render
a flat, solid **fallback color** for any window that isn't the OS-active
window — this is documented Windows behavior (Mica/Acrylic docs list
"window deactivates" as a fallback-to-solid-color condition), not
something `window-vibrancy` or Tauri can override. Confirmed via
`microsoft/microsoft-ui-xaml#10570` (`DesktopAcrylicBackdrop` loses blur
on `WS_EX_NOACTIVATE` windows) and `tauri-apps/window-vibrancy#183`
(Acrylic/Mica broken on Windows 11 24H2/25H2 in general).

Worse: calling these material APIs **overrides** Tauri's own
`transparent: true` mechanism (`tao` registers the window with DWM via
`DwmEnableBlurBehindWindow` + an empty blur region at window creation —
that's what actually makes a Tauri window see-through, no material
needed). When the material then fails to render (because the window is
never active), DWM falls back to **solid opaque** instead of the
window's original see-through state. This produced a fully opaque
black/grey panel that looked worse than doing nothing.

**The fix**: removed the vibrancy calls entirely. The sidebar was
already genuinely transparent via `transparent: true` in
`tauri.conf.json` — the same mechanism the main orb window uses
successfully. Result: sharp (not blurred) but real, focus-independent
transparency. `src-tauri/src/dwm_corners.rs` still calls
`DwmSetWindowAttribute(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND)`
directly (a plain window-shape attribute, not a material — unaffected
by the active/inactive issue) so the OS window's corners match the CSS
card's `border-radius`, avoiding a "double panel" mismatch (WebView2
has no `CornerRadius` support, so without this the DWM-painted window
rectangle and the rounded CSS card show as two different shapes).

Also removed: a CSS chromatic-aberration effect (red/cyan inset
`box-shadow` on `.sidebar-card::after`) that was meant to simulate
glass prism fringing. Without real optical refraction (no SVG
`feDisplacementMap` — `backdrop-filter: url()` is also a no-op on a
transparent WebView2, see below), it just read as a colored-border
rendering bug. Replaced with a neutral lit-bezel `box-shadow` stack
(top specular + bottom shadow) for a physical-glass feel without color.

Separately confirmed: CSS `backdrop-filter` (blur or `url()` SVG
refraction) is a **no-op in a transparent WebView2** — see
`MicrosoftEdge/WebView2Feedback#4945`. It can't composite against
nothing. Don't rely on it for this window; any blur must come from a
native OS mechanism, and per above, none is currently available for a
non-activating window without a full WinRT `DesktopAcrylicController` +
`SystemBackdropConfiguration.IsInputActive = true` interop (what
PowerToys uses for its non-activating flyouts) — out of scope unless
revisited.

### Screenshot-capture blur (the actual liquid glass)

Since native blur is unavailable for non-activating windows, the sidebar
uses a **"fake blur"**: right before `win.show()`, Rust captures the desktop
region behind the window via GDI `BitBlt`, blurs it with
`image::imageops::fast_blur(sigma=32)`, encodes it as a PNG data URI, and
hands it to the frontend as a CSS `background-image` on `.sidebar-card`
via the `--sidebar-backdrop-image` CSS variable. This gives a genuine
frosted-glass look without depending on window activation state.

**Critical timing:** the capture MUST happen before `win.show()` so the
sidebar doesn't capture itself. If the window is already visible (re-show),
capture is skipped. See `src-tauri/src/sidebar_backdrop.rs`.

Full implementation guide: `docs/features/21-liquid-glass-sidebar.md`.

### Dynamic window pending-content pattern (race-free event delivery)

When a window is created on-demand (via `dyn_windows.rs`), the WebView2
needs time to load the HTML and mount the React app. If Rust emits Tauri
events immediately after creating the window, **those events are lost**
because no listener exists yet.

**Fix:** store the content in a `static Mutex<Option<...>>` and let the
frontend fetch it on mount via a `get_pending_*` command. This is race-free
regardless of how long the WebView takes to load. If the window already
exists (React loaded), events are also emitted as a fast path.

Currently used for:
- `PENDING_SIDEBAR` in `commands.rs` → `get_pending_sidebar_content`
- `PENDING_ARCHITECT_REPO` in `architect.rs` → `get_pending_architect_repo`

**All show/create commands MUST be `async`** — `WebviewWindowBuilder::build()`
dispatches to the main thread, and a synchronous Tauri command runs on a
blocking thread that can't yield, causing a deadlock.

To reuse this pattern for a new window, see the "How to Reuse" section in
`docs/features/21-liquid-glass-sidebar.md`.

## Architecture Mapper — Phase 1 Latency Optimization (2026-08-30)

Phase 1 now uses **Approach C (hybrid)** for a 3-4s first response:

1. **Parallelized GitHub API calls** (`tokio::join!`): metadata + recursive
   tree are fetched concurrently using the symbolic ref `HEAD` (verified
   against repos with `main` and `master` default branches). Cuts ~600-1000ms
   off the critical path vs the old sequential metadata→tree flow.
2. **Instant Rust heuristic clustering** for first paint (~5ms) — the diagram
   appears in ~1-1.5s with generic layer labels.
3. **Async LLM enrichment** (`enrich_phase1` command): after first paint, the
   client POSTs the heuristic layers + sample file paths to the Worker's
   `phase1_enrich` intent. The LLM (Mistral 24B) rewrites generic labels into
   repo-specific ones (e.g. "Client / Presentation Layer" → "Next.js App
   Router (React 19)") and writes a real summary. Result streams back via
   the `architect:phase1-enriched` event ~2-3s later and merges in-place.
   **Never blocks first paint.** If the Worker/LLM fails, the heuristic
   diagram remains (graceful degradation).

| Component | File | What changed |
|-----------|------|--------------|
| Rust parallel fetch | `src-tauri/src/architect.rs` | `analyze_repo_phase1` uses `tokio::join!` + `HEAD` ref |
| Rust enrichment cmd | `src-tauri/src/architect.rs` | New `enrich_phase1` command + `Phase1Enrichment`/`EnrichedLayer` types |
| Rust session accessor | `src-tauri/src/network.rs` | New `get_session_info()` public helper |
| Worker handler | `server/worker/src/index.ts` | New `handlePhase1Enrich` + `phase1_enrich` intent dispatch |
| Frontend store | `frontend/src/architect/architectStore.ts` | New `enrichPhase1` action + `sample_file_paths` field |
| Frontend app | `frontend/src/architect/ArchitectApp.tsx` | Calls `enrich_phase1` after paint + listens for enriched event |

## Building

**Always build the desktop app with the Tauri CLI:**

```powershell
pwsh ./scripts/build.ps1          # frontend + tauri release build + bundles
```

If you need a plain cargo build (faster iteration, no installer), you **must**
pass the `custom-protocol` feature:

```powershell
npm --prefix frontend run build
cargo build --release --features custom-protocol   # run inside src-tauri/
```

### Why `custom-protocol` is mandatory

Tauri decides whether to load the bundled frontend or the Vite dev server
purely from this feature flag:

```rust
// tauri-macros/src/context.rs
dev: cfg!(not(feature = "custom-protocol")),
```

- Feature **on**  → windows load `http://tauri.localhost/...` (embedded assets)
- Feature **off** → windows load `devUrl` = `http://localhost:5173`

`cargo tauri build` adds the feature automatically; a bare `cargo build
--release` does **not**. A release binary built without it shows
`localhost refused to connect` / `ERR_CONNECTION_REFUSED` in every window,
because no Vite server is running. Clearing the WebView2 profile does not
help — the dev URL is baked into the binary at compile time.

The feature is deliberately **not** in `[features] default`, because
`tauri dev` needs it off for hot reload.

### Verifying which URL the app actually loads

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
Start-Process .\src-tauri\target\release\nexus.exe
Start-Sleep 15
Invoke-RestMethod http://127.0.0.1:9222/json/list | Select-Object title, url
```

Expected: every window on `http://tauri.localhost/...`.
Bad: `http://localhost:5173/...` → rebuild with `--features custom-protocol`.

## Frontend windows

Every window declared in `src-tauri/tauri.conf.json` must have a matching
rollup input in `frontend/vite.config.ts`, otherwise its HTML file is absent
from `dist/` and the window fails to load in release builds (dev mode hides
this — the Vite server serves any HTML file on demand).

| tauri.conf.json window | HTML            | vite input |
| ---------------------- | --------------- | ---------- |
| `main`                 | `index.html`    | `main`     |
| `setup`                | `setup.html`    | `setup`    |
| `settings`             | `settings.html` | `settings` |
| `sidebar`              | `sidebar.html`  | `sidebar`  |

## Local ports

| Service           | Port    | Notes                                        |
| ----------------- | ------- | -------------------------------------------- |
| STT (faster-whisper) | `39217` | Registered range (1024-49151), avoids ephemeral and dev ports. Override: `NEXUS_STT_PORT` |
| Sidecar (FastAPI)    | `41098` | Registered range, not ephemeral. Override: `SIDECAR_PORT` |
| Vite dev server   | `5173`  | Dev only                                     |

## Architecture (serverless — 2026-08-27)

NEXUS is now **fully serverless**. No sidecar, no n8n, no Ollama, no server.

```
NEXUS laptop → HTTP POST → Cloudflare Worker → APIs → text response
                              ↑
                        D1 database (OAuth tokens)
                        Workers AI (intent + summarization)
```

- **Worker** (`server/worker/`): Cloudflare Worker on the edge. Handles
  intent classification, API calls (GitHub/Google), summarization, OAuth
  exchange, token storage, and user registration. <5ms cold start.
- **D1**: Cloudflare's free SQLite. Stores OAuth tokens, API keys, and
  device registrations. 5GB free.
- **Workers AI**: Free tier (10K neurons/day) for intent classification
  (Qwen 0.5B) and summarization (Qwen 14B).
- **Client** (`src-tauri/src/network.rs`): HTTP POST to the Worker. No
  WebSocket. Emits state/ack/result/done events to the frontend.
- **NEXUS_SERVER_URL**: Baked into the installer at build time. Points to
  the Worker URL (e.g. `https://nexus-worker.xxx.workers.dev`).

The old sidecar (`server/sidecar/`) is kept in the repo for reference but
no longer spawned at startup. `sidecar_manager.rs` has been removed.

### Building the installer with the Worker URL

```powershell
$env:NEXUS_SERVER_URL = "https://nexus-worker.your-subdomain.workers.dev"
pwsh ./scripts/build.ps1
```

## Runtime paths (Windows)

- App data (config, logs): `%APPDATA%\com.nexus.assistant\`
- WebView2 profile: `%LOCALAPPDATA%\com.nexus.assistant\EBWebView`
  (note: **Local**, not Roaming — `app_data_dir()` returns Roaming and is the
  wrong path for WebView2)

## Wake word (openWakeWord)

- Default feature: `wakeword-oww` (tract-onnx inference in Rust)
- Models: `src-tauri/resources/oww/{melspectrogram.onnx, embedding_model.onnx, nexus.onnx}`
- Threshold: 0.35, chunk size: 1280 samples (80ms at 16kHz)
- **Detection logic (2026-08-29):** Max-based detection + secondary confirmation.
  The old averaging approach diluted single good frames (0.4+) with surrounding
  0.0s, giving avg=0.03 which never triggered. Now uses max probability in the
  12-frame buffer, so a single 0.36+ frame triggers. After a raw detection,
  collects 500ms of audio and checks RMS ≥ 0.01 to confirm real speech (filters
  noise spikes). Refractory period: 3s.
- **Model v2 (2026-08-30, Kaggle):** Retrained on Kaggle T4 GPU with:
  - 5000 positive samples (5 phrase variants: "nexus", "hey nexus", "nexus wake up", "ok nexus", "nexus please")
  - 30+ soundalike negatives (vs 8 in v1)
  - 80000 training steps (vs 50000), layer_size=64 (vs 32)
  - 2x augmentation rounds, target FP/hr=0.1 (vs 0.2)
  - Model size: 415KB (vs 205KB v1)
  - Kernel: `chitkullakshya/train-nexus-wakeword-v2`
  - v1 backup: `src-tauri/resources/oww/nexus_v1.onnx.backup`
- **Silence gate + AGC (2026-08-28):** `detect_chunk` computes RMS of each
  80ms chunk and skips the classifier entirely if RMS < 0.0005 (~-66dBFS).
  The `nexus.onnx` model emits 0.6-0.9 probabilities on pure digital silence
  (out-of-distribution input), which caused spontaneous false wakes. The
  gate prevents the model from ever seeing silence. Min positive detections
  = 2. Regression test: `test_silence_never_triggers_wake`.
  - **AGC (Automatic Gain Control):** If RMS passes the gate but is below
    TARGET_RMS (0.03), the chunk is amplified up to 50x before feeding the
    classifier. This makes quiet/whispered "NEXUS" produce the same model
    input as loud "NEXUS", so the model (trained on normal-volume TTS)
    recognizes low-volume speech without retraining.
  - Gate: 0.0005, threshold: 0.45. Pure silence (RMS=0) is blocked.
  - Model: trained on Kaggle (v22), accuracy 78.6%, recall 58.2%, FP/hr 1.33.
- **Mic device enumeration (2026-08-27):** `start_audio_capture` enumerates
  ALL input devices, probes each for 5 seconds, and picks the first one
  that produces non-silent audio (RMS > 0.0001). If all devices are silent
  (Intel SST bug), falls back to the best device anyway. This fixes the
  "wake word doesn't work, only hotkey" issue caused by cpal getting
  silence from the Intel Smart Sound Technology driver.
- **FIXED (2026-08-24):** The wake word now works — probability 0.991 for real
  "NEXUS" speech. The root cause was a 32768x input scaling mismatch: cpal
  produces f32 audio in [-1.0, 1.0] but the openWakeWord melspectrogram model
  expects int16-scale float32 values in [-32768, 32767]. Fix: multiply audio
  by 32768.0 in `wakeword_oww.rs` before feeding to the melspectrogram model.
- **Mic conflict (FIXED):** The frontend's `warmMic()` (getUserMedia via WebView2)
  conflicts with the Rust cpal wake-word stream on Intel Smart Sound Technology
  drivers. `warmMic()` is disabled at startup; the mic is acquired on first
  wake instead. This is why cpal was getting silence (RMS=0.0000).
- The global hotkey still works independently of the wake-word model.
- **Command models (2026-08-25):** Training 4 category-level acoustic models
  (`command_open`, `command_close`, `command_search`, `command_play`) via
  `train_nexus_commands.ipynb` on Google Colab. Models detect command TYPE,
  then STT extracts the parameter (which app, what query). See
  `src-tauri/resources/oww/commands/command_intents.json`.

## Known limitations (2026-08-26 audit)

- **Speaker verification is NOT wired.** Enrollment works (setup wizard ->
  embedding -> JSON on disk), but `wakeword_oww::WakeEngine::process`
  accepts every wake regardless of speaker. The verification API in
  `voice_profile.rs` is `#[allow(dead_code)]` until an audio ring buffer
  is added to retain the wake utterance for embedding extraction.
- **All windows skip the taskbar.** `main`, `setup`, `settings`, and
  `sidebar` all have `skipTaskbar: true` in `tauri.conf.json`. NEXUS is
  accessible only via the floating orb, the system tray, the global
  hotkey, and the wake word.
- **STT server auto-launcher writes `server/start_stt.cmd`** with an
  absolute path to the local Python interpreter. This file is gitignored
  (machine-specific, leaks username).
