> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — Floating Desktop Assistant
## Principal Architecture & Implementation Specification

> Thin Client (Tauri v2 + Rust + React/TS) ⇄ Fat Server (n8n Supervisor + Ollama)
> Target: Windows 10/11, macOS (Apple Silicon + Intel), Linux (X11 + Wayland)

---

## 0. Design Philosophy

| Principle | Enforcement |
|---|---|
| **Thin Client** | Local STT + local TTS. Only text crosses the network. Idle RAM < 200 MB (with whisper model). |
| **Fat Server** | n8n intent routing + Ollama LLM. No audio processing on the server. |
| **Text-only protocol** | WebSocket carries only JSON text frames. No binary audio frames in either direction. |
| **Privacy** | Microphone audio never leaves the device. STT runs locally via faster-whisper on localhost. |
| **Overlay correctness** | Transparent, frameless, always-on-top, with *region-aware* click-through. |
| **Zero idle CPU** | Wake-word via openWakeWord KWS (tract-onnx, pure Rust), not a JS AudioWorklet loop. |

---

## 1. High-Level Topology

```
                       ┌─────────────────────────────────────────────────────────┐
                       │              FAT SERVER (dedicated GPU host)            │
                       │  ┌──────────────┐    webhook    ┌────────────────────┐  │
                       │  │  Reverse Proxy│  ─────────▶  │ n8n Master         │  │
                       │  │ (Caddy/Tailscale│            │ Supervisor         │  │
                       │  │  + WSS TLS)   │             │  (intent router)   │  │
                       │  └──────┬───────┘              └─────────┬──────────┘  │
                       │         │                                │ dispatch    │
                       │         │               ┌────────────────┼──────────┐  │
                       │         │               ▼                ▼          ▼  │
                       │         │        ┌──────────┐  ┌──────────┐ ┌────────┐ │
                       │         │        │ Ollama   │  │ Sub-flow │ │ TTS    │ │
                       │         │        │ (LLM)    │  │ canvases │ │ synth  │ │
                       │         │        │ 11GB GPU │  │ (PRs,    │ │(piper/ │ │
                       │         │        └────┬─────┘  │ mail,cal)│ │ coqui) │ │
                       │         │             │        └──────────┘ └───┬────┘ │
                       │         │             └──── aggregated JSON ───┘      │
                       └─────────┼──────────────────────────────────────────────┘
                                 │ WSS (TLS) — TEXT ONLY (transcript up, result down)
                                 │
   ┌─────────────────────────────┼─────────────────────────────────────────────┐
   │              THIN CLIENT (per device)                                      │
   │  ┌──────────────────────────────────────────────────────────────────────┐ │
   │  │  Tauri v2 Main Process (Rust)                                          │ │
   │  │   ├── openWakeWord KWS (tract-onnx, ~1-2% CPU)  ──▶  Wake Event        │ │
   │  │   ├── Global Hotkey (Ctrl/Cmd+Shift+Space)  ──▶  Wake Event           │ │
   │  │   ├── click-through region manager (set_ignore_cursor_events)        │ │
   │  │   ├── autostart + system tray (tauri-plugin-*)                        │ │
   │  │   └── WSS bridge  ──▶  frontend events (Tauri IPC)                    │ │
   │  │  Tauri IPC  (invoke / event)                          ▲               │ │
   │  │                          ▼                                          │ │
   │  │  Webview: React + TS                                                 │ │

---

## 2. End-to-End Sequence Diagram (ASCII)

```
 User           Tauri/Rust            Frontend (React)        Local STT       Server (sidecar+n8n)
  │                 │                       │                    │                    │
  │ wake phrase ─┐  │                       │                    │                    │
  │ (or hotkey) │  │                       │                    │                    │
  │             ▼  │                       │                    │                    │
  │  OWW KWS callback ▼ │                       │                    │                    │
  │  ──wake─────▶ │ emit("assistant:wake") │                    │                    │
  │               │ ───────────────────────▶│                    │                    │
  │               │                         │ state=Listening    │                    │
  │               │                         │ mic ON + VAD start │                    │
  │ "Summarize    │                         │                    │                    │
  │  my email" ◀──┘                         │                    │                    │
  │               │                         │ VAD: silence─┐      │                    │
  │               │                         │   stop mic   │      │                    │
  │               │                         │ PCM buffered │      │                    │
  │               │                         │   LOCAL STT  │      │                    │
  │               │                         │ ── PCM to localhost:8000 ──▶             │
  │               │                         │ ◀── transcript: "summarize email" ─────  │
  │               │                         │   WSS CONNECT     │                    │
  │               │  ─── ws: {type:"start"} ──────────────────────────────────────▶ │
  │               │  ─── ws: {type:"transcript", data:"summarize email"} ────────▶ │
  │               │                         │                    │ Master Supervisor  │
  │               │                         │                    │  classify intent ──┐
  │               │                         │                    │                   │ sub-canvas:
  │               │                         │                    │ ◀─── email.fetch ──┘
  │               │                         │                    │ ───── prompt ────────────▶ Ollama
  │               │                         │                    │ ◀──── summary text ────── │
  │               │ ◀──── ws: {type:"ack", data:"On it, sir."} ─────────────────── │
  │               │                         │ LOCAL TTS speaks   │                    │
  │               │                         │ "On it, sir."      │                    │
  │               │ ◀──── ws: {type:"result", data:"You have 3 emails..."} ─────── │
  │               │                         │ LOCAL TTS speaks   │                    │
  │               │                         │ the result         │                    │
  │               │ ◀──── ws: {type:"done"} ───────────────────────────────────── │
  │               │                         │ state=Idle         │                    │
```

**Key difference from old architecture:** Audio goes to localhost STT only.
The server never receives audio. TTS is done locally via Web Speech API.
Only text crosses the network (transcript up, ack + result down).
  │               │ emit("assistant:idle")  │                    │                    │
  │               │ ───────────────────────▶│ state=Idle        │                    │
  │ ◀──────── spoken answer ─────────────────                    │                    │
```

---

## 3. Resource Budget (Client)

| Metric | Idle | Active (listening+streaming) |


---

## 4. Crate & Stack Selection (justified)

### Rust (Main process)
| Concern | Crate | Why |
|---|---|---|
| App shell | `tauri` v2 | Only option giving transparent overlay + IPC + cross-platform tray + Rust core. |
| Wake word | `tract-onnx` (pure Rust ONNX) + openWakeWord KWS | < 2% CPU, offline, no JS event loop, no native C dependency. Replaces Porcupine (which required API key + online activation). See [wake-word docs](./wake-word/). |
| Audio capture (feed to KWS) | `cpal` | Cross-platform PCM; route 16 kHz mono frames to openWakeWord. |
| Global hotkey | `tauri-plugin-global-shortcut` | Official, works on all 3 OSes incl. Wayland. |
| Autostart | `tauri-plugin-autostart` | Official, uses LaunchAgent/MS reg/.desktop. |
| Tray | built-in `tauri::tray` (v2) | — |
| Async runtime | `tokio` (current_thread flavor) | Lower idle footprint; single worker suffices. |
| WSS client | `tokio-tungstenite` + `rustls` | Pure-Rust TLS, no OpenSSL system dep (critical for cross-compile). |
| Audio encode (Opus) | `opus` (audiopus_lite) | Compress mic stream before WSS. |
| Logging | `tracing` + `tracing-subscriber` | Async-friendly, zero-cost when off. |
| Single instance | `tauri-plugin-single-instance` | Prevent duplicate autostart launches. |
| Secret storage | `keyring` | OS keychain for device token. |

### Frontend (Webview)
| Concern | Library | Why |
|---|---|---|
| UI framework | React 18 + TypeScript + Vite | Mature, small webview footprint. |
| Animation | `@rive-app/react-canvas` | GPU vector state machines; idle cost ~0 vs Three.js. Framer Motion fallback for layout. |
| VAD | `@ricky0/vad-web` (onnx) or lightweight RMS AudioWorklet | Browser-native, no server round-trip. |
| Mic capture | `AudioWorkletNode` raw PCM → Tauri IPC | Lower latency than MediaRecorder; stream frames directly. |
| State | `zustand` | 1 KB, no provider boilerplate. |

### Server
| Concern | Tool |
|---|---|
| Orchestrator | n8n (self-hosted, webhook-triggered) |
| LLM | Ollama (Llama 3.1 8B Q4_K_M / Qwen2.5 7B) |
| STT | `faster-whisper` (CTranslate2) on CPU, or whisper.cpp |
| TTS | `piper` (fast, neural, streaming chunks) |
| TLS/VPN | Caddy reverse proxy + Tailscale mesh |

---


---

## 6. Wake-Word Engine: openWakeWord KWS in Rust

### 6.1 Approach
The wake-word engine uses **openWakeWord** — a continuous keyword spotting (KWS) system that scores every 80ms audio chunk for the target word "NEXUS" without using VAD or ASR. The engine is implemented in pure Rust via `tract-onnx` (no native ONNX Runtime dependency).

- **3-stage ONNX pipeline:** melspectrogram → embedding → custom NEXUS classifier
- **Models:** `melspectrogram.onnx` (1.0MB) + `embedding_model.onnx` (1.3MB) + `nexus.onnx` (~0.8MB, custom-trained)
- **Audio:** `cpal` stream → downmix to mono → resample to 16kHz → 1280-sample (80ms) chunks
- **Detection:** rolling probability buffer + threshold (0.5) + refractory period (2000ms)
- **Speaker verification:** sherpa-onnx speaker embeddings (second stage, optional)
- On detection → tokio channel → `window.__NEXUS_WAKE__()` in frontend

### 6.2 Why not VAD+ASR? Why not Porcupine?
The original approach used VAD (Silero) + ASR (sherpa-onnx Zipformer) + text matching, achieving only ~30% recall. VAD clips the start of short words and ASR misrecognizes "nexus" as "mexic", "next", etc. Porcupine was considered but rejected because it requires an API key and online activation (violates the privacy-first principle). See [wake-word/02-wake-word-architecture-decision.md](./wake-word/02-wake-word-architecture-decision.md) for the full comparison.

### 6.3 CPU budget
The KWS engine uses ~1-2% CPU at idle (3 small ONNX models, ~11-22ms inference per 80ms chunk). The audio capture thread is the only always-on thread; everything else is event-driven. Implementation: `src-tauri/src/wakeword_oww.rs`.

### 6.4 Why not WebAssembly AudioWorklet?
A Worklet runs *inside the webview audio graph*, forcing the webview/V8 loop to tick every audio quantum → defeats the idle budget and keeps WebView2 memory resident. KWS in Rust keeps the webview free to be torn down between interactions.

### 6.5 Feature flags
```toml
default = ["wakeword-oww"]
wakeword-porcupine = []   # legacy
wakeword-sherpa = []      # old VAD+ASR (fallback)
wakeword-oww = []         # openWakeWord KWS (default)
mock-wake = []            # CI: no audio capture, hotkey only
```

### 6.6 Custom model training
The `nexus.onnx` classifier is trained using `train_nexus_oww.ipynb` in Google Colab (T4 GPU, ~1 hour). Uses synthetic Piper TTS data — no real user audio needed. See [wake-word/06-model-training.md](./wake-word/06-model-training.md).

---

## 7. Frontend State Machine

```
            wake / hotkey
   Idle ─────────────────────▶ Listening
   ▲                              │  VAD silence → stop mic

---

## 8. Server-Side Intent Router (n8n Master Supervisor)

### 8.1 Topology
- One **Webhook trigger** `POST /supervisor` accepting `{transcript?, audio?, userId, deviceId, sessionId}`.
- **Classify Intent**: fast Ollama model (Qwen2.5 1.5B) with strict JSON schema → `{intent, confidence, args}`. `confidence < 0.5` falls back to `general.chat`.
- **Router (Switch node)** fans out to sub-canvas `Execute Workflow` nodes: `email.summarize`, `github.pr_check`, `calendar.peek`, `general.chat`.
- Each sub-canvas returns `{reply_text, context}`.
- **Aggregate → TTS (piper)** → stream PCM chunks back over the originating WebSocket session keyed by `sessionId`.

### 8.2 Concurrency & Ollama queue
- Single GPU: `OLLAMA_NUM_PARALLEL=1`. n8n "Execute Once per Item" + a **Redis mutex** (n8n Redis node) for micro-task slots so ≤5 users serialize fairly.
- Heavy summarization → 8B model; intent classification → 1.5B → routing latency < 400 ms.
- Importable blueprint: `server/n8n/master_supervisor.blueprint.json`.

### 8.3 Sub-canvas responsibilities
| Canvas | Triggers | Tools | Output |
|---|---|---|---|
| email.summarize | IMAP/Graph API fetch last N | 8B summary | bullet list |
| github.pr_check | GitHub REST | 8B review digest | PR status + risk |
| calendar.peek | Google/Microsoft Graph | 8B narration | next 3 events |
| general.chat | — | 8B conversation | free-form reply |

---

## 9. Build & Installer Recipe


---

## 10. Security & Privacy
- All traffic over WSS (Caddy TLS) or Tailscale (WireGuard mesh) — no plaintext.
- Per-device token issued by server; rotate on revoke; stored in OS keychain.
- Audio never persisted on client; server stores transcripts only on opt-in.
- Porcupine AccessKey no longer needed (replaced by openWakeWord — no API key required).
- CSP in `tauri.conf.json` restricts `connect-src` to the backend host + `ipc:`/`http://ipc.localhost`.
- Capabilities (`capabilities/main.json`) grant only the window/cursor/hotkey/autostart/event scopes needed — least privilege.

---

## 11. File Manifest

```
NEXUS/
├─ docs/ARCHITECTURE.md                 # this spec
├─ README.md
├─ scripts/build.ps1                    # build helper (per-OS)
├─ .github/workflows/release.yml        # cross-OS release CI
├─ src-tauri/
│  ├─ Cargo.toml                        # crate selection (see §4)
│  ├─ tauri.conf.json                   # transparent overlay + plugins + bundle
│  ├─ capabilities/main.json            # least-privilege scopes
│  ├─ entitlements.plist               # macOS entitlements
│  ├─ build.rs
│  └─ src/
│     ├─ main.rs                        # binary entry → lib::run()
│     ├─ lib.rs                         # Tauri builder + plugin wiring
│     ├─ window_manager.rs              # overlay + set_click_through IPC
│     ├─ hotkey.rs                       # Ctrl/Cmd+Shift+Space → wake
│     ├─ wakeword_oww.rs                # openWakeWord KWS (tract-onnx) — default
│     ├─ wakeword.rs                    # legacy VAD+ASR (sherpa-onnx) — fallback
│     ├─ network.rs                     # WSS bridge (tokio-tungstenite)
│     └─ tray.rs                        # system tray
├─ frontend/
│  ├─ package.json
│  ├─ vite.config.ts
│  ├─ tsconfig.json
│  ├─ index.html
│  └─ src/
│     ├─ main.tsx
│     ├─ App.tsx                        # wake handler + lifecycle
│     ├─ styles.css
│     ├─ store/assistant.ts             # zustand state machine
│     ├─ overlay/clickThrough.ts        # region-aware click-through
│     ├─ audio/recorder.ts             # AudioWorklet PCM capture
│     ├─ audio/pcm-worklet.js           # resample → 16k Int16 blocks
│     ├─ audio/vad.ts                   # Silero VAD + RMS fallback
│     ├─ audio/ttsPlayer.ts             # gapless WebAudio TTS playback
│     ├─ net/wsBridge.ts                # open/close + server event handling
│     └─ avatar/Avatar.tsx              # Rive state machine
└─ server/n8n/master_supervisor.blueprint.json
```

---

## 12. Known Limitations / Assumptions
- **Compositor required on Linux** for `transparent: true`. On bare WMs (e.g. i3 without compositor) the overlay degrades to an opaque rounded rectangle.
- **Wayland global hotkeys** are unreliable without a portal; `tauri-plugin-global-shortcut` uses `evdev`/`x11` backends — on pure Wayland the wake word remains the primary trigger; document the hotkey limitation for Wayland users.
- **Custom wake word model** (`nexus.onnx`) must be trained via the Colab notebook before the KWS engine can detect "NEXUS". The `mock-wake` feature exists for CI without the model.
- **Streaming TTS over WebSocket** is implemented client-side; the n8n blueprint uses `respondToWebhook` for the simple case — a custom n8n node (or a small FastAPI sidecar holding the session map keyed by `sessionId`) is recommended for true chunked streaming. See `_meta` note in the blueprint.
- **Ollama single-GPU** enforces sequential micro-tasks; for 5 concurrent users with heavy summarization, consider a second small model slot or GPU offload scheduling — out of scope for v1.

1. `pnpm create tauri-app@latest NEXUS` → Rust + React-TS template.
2. Add plugins: `pnpm --dir frontend add @tauri-apps/plugin-global-shortcut @tauri-apps/plugin-autostart @tauri-apps/plugin-single-instance @tauri-apps/plugin-shell @tauri-apps/plugin-store zustand @rive-app/react-canvas`.
3. Add Rust crates per `src-tauri/Cargo.toml`.
4. **Windows (NSIS .exe):** install NSIS; set `TAURI_SIGNING_PRIVATE_KEY` + `..._PASSWORD` (EV cert pfx, base64) → `tauri build` auto-signs. WebView2 bootstrapper bundled.
5. **macOS (.dmg):** universal binary — build both `aarch64-apple-darwin` and `x86_64-apple-darwin`; set `APPLE_CERTIFICATE`, `APPLE_ID`, `APPLE_PASSWORD` (app-specific), `APPLE_TEAM_ID` → `tauri build` runs `xcrun notarytool` automatically. `entitlements.plist` declares audio input + network client; `LSUIElement=true` hides dock icon.
6. **Linux:** AppImage (default target) + `.deb`. Install `libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev`.
7. CI matrix in `.github/workflows/release.yml` builds all three OSes on tag `v*`.
8. Local: `pwsh ./scripts/build.ps1 -Release`.

   │                              ▼
   │                          Thinking  ◀── ws state event
   │                              │  first tts_chunk
   │                              ▼
   └──── done event ◀────────  Speaking
```

- Single source of truth: `zustand` store `useAssistant` (`frontend/src/store/assistant.ts`).
- Transitions are pure (`transition(from,to)`); side effects subscribed via effects.
- Rive state machine inputs `isListening`, `isThinking`, `isSpeaking`, `idle` driven from the store (`avatar/Avatar.tsx`).
- After 4 s idle, fade window opacity to 0.08 (still catchable) — `App.tsx` effect.

## 5. Window & Click-Through Strategy

### 5.1 Region-aware click-through
Tauri exposes `window.set_ignore_cursor_events(ignore: bool)` which is **whole-window**. We need transparent pixels to pass clicks *through* while the avatar stays interactive.

**Strategy A — Hit-test polling (chosen):** Frontend listens to `pointermove`; uses `document.elementFromPoint(x,y)`. If the topmost element is the transparent root (no avatar underneath), call `setIgnoreCursorEvents(true)`; when over the avatar, call `setIgnoreCursorEvents(false)`. This toggles per mouse-move only when crossing the avatar boundary (debounced) to avoid thrash.

**Strategy B — Forward region to Rust (fallback):** Frontend sends the avatar bounding box via IPC; Rust computes a layered region. OS-specific; only used if A shows jank.

Implementation: `src-tauri/src/window_manager.rs` + `frontend/src/overlay/clickThrough.ts`.

### 5.2 Window config (`tauri.conf.json`)
`transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `resizable: false`, `shadow: false`, `focus: false` (so it won't steal focus from the active app). Linux `transparent` requires a compositor (detect & degrade). macOS: `hiddenTitle: true`, `tabbingIdentifier: null`.

|---|---|---|
| RAM | 40–90 MB | < 250 MB |
| CPU | ~0% (event-driven) | Wake-word 0.5%, audio encode 1–2% |
| Network | 0 | Opus 16–32 kbps up + TTS 32 kbps down |
| Disk (installed) | ~25 MB binary + 30 MB webview runtime |

Enforced via: native openWakeWord KWS (no JS audio loop), `tokio` single-thread, no background polling, lazy frontend (webview only mounted while visible).

   │  │   ├── Audio recorder (AudioWorklet → Tauri IPC)                      │ │
   │  │   ├── VAD (WebRTC VAD RMS / @ricky0/vad-web)                         │ │
   │  │   ├── State machine (Rive / Framer): Idle→Listen→Think→Speak          │ │
   │  │   └── TTS audio stream player (WebAudio)                              │ │
   │  └──────────────────────────────────────────────────────────────────────┘ │
   └────────────────────────────────────────────────────────────────────────────┘
```
