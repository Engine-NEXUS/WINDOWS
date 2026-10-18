> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — System Overview

> The single-page mental model for the entire NEXUS assistant.
> Read this first; everything else in `docs/` is a deep-dive into a slice of this map.

---

## 1. The Two Halives

NEXUS is split into a **Thin Client** that lives on the user's machine and a **Fat Server** that lives on a dedicated GPU host. The two halves only ever exchange **JSON text** over a single WebSocket — no audio frames cross the network in either direction.

```
┌─────────────────────────────────────────────────────────────────────────┐
│  THIN CLIENT  (per device — Windows / macOS / Linux)                    │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Tauri v2 Main Process  (Rust)                                    │  │
│  │                                                                   │  │
│  │   • openWakeWord KWS  ─────────────▶  Wake Event                  │  │
│  │   • Tier 3 command classifiers  ──▶  Command Event                │  │
│  │   • Global hotkey (Ctrl/Cmd+Shift+Space)  ──▶  Wake Event         │  │
│  │   • Meeting / privacy detector (WASAPI + process scan)            │  │
│  │   • Sleep/wake time-jump watcher  ──▶  Greeting Event             │  │
│  │   • Sidecar manager (auto-spawn pythonw.exe)                      │  │
│  │   • WebView2 mic/camera permission handler                         │  │
│  │   • App registry (pre-indexed launcher)                           │  │
│  │   • WSS bridge (text-only)  ──────────────▶  Frontend events      │  │
│  │   • Local STT HTTP client (faster-whisper on 127.0.0.1)           │  │
│  │   • Tray + autostart + single-instance + deep-link                │  │
│  └─────────────┬─────────────────────────────────────────────────────┘  │
│                │  Tauri IPC (invoke / event / win.eval)                  │
│                ▼                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  WebView: React + TypeScript + Vite                               │  │
│  │                                                                   │  │
│  │   • Zustand state machine (idle → listening → thinking → speaking)│  │
│  │   • Mic capture (ScriptProcessorNode, native SR)                  │  │
│  │   • Silero VAD (ONNX Runtime Web) — speech/silence detection      │  │
│  │   • Local TTS via Web Speech API (SpeechSynthesis)                │  │
│  │   • Intent parser (regex + Double Metaphone phonetic correction)  │  │
│  │   • Tier 3 command listener → direct execute_command              │  │
│  │   • Boot greeting handler                                         │  │
│  │   • Setup page (OAuth + API keys + voice enrollment)              │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Local STT Server  (faster-whisper on 127.0.0.1:8000)             │  │
│  │   • Receives raw PCM via HTTP multipart                           │  │
│  │   • Returns transcript text                                       │  │
│  │   • Audio NEVER leaves the device                                 │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└──────────────────────────────────┬──────────────────────────────────────┘
                                   │
                                   │  WSS (TLS via Caddy or Tailscale)
                                   │  TEXT ONLY: transcript up, result down
                                   │
┌──────────────────────────────────┴──────────────────────────────────────┐
│  FAT SERVER  (dedicated GPU host)                                       │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Python FastAPI Sidecar  (port 49152, 127.0.0.1)                  │  │
│  │                                                                   │  │
│  │   • /ws            — WebSocket endpoint (text-only protocol)      │  │
│  │   • /health        — liveness probe                               │  │
│  │   • /oauth/*       — Google + GitHub token exchange/refresh       │  │
│  │   • /apikeys/*     — Encrypted API key store (Fernet at rest)     │  │
│  │   • /device/*      — Device registration + validation             │  │
│  │   • /config/check  — Which providers are configured (no secrets)  │  │
│  │   • SQLite DB      — oauth_tokens, api_keys, user_devices         │  │
│  └─────────────┬─────────────────────────────────────────────────────┘  │
│                │  HTTP POST (transcript + credentials)                   │
│                ▼                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  n8n Master Supervisor                                            │  │
│  │                                                                   │  │
│  │   • Webhook trigger  →  classify intent (Ollama 1.5B)             │  │
│  │   • Switch node  →  fan out to sub-canvas workflows               │  │
│  │   • Sub-canvas: email.summarize, github.pr_check, calendar.peek,  │  │
│  │                  general.chat, ...                                │  │
│  │   • Aggregate  →  return reply_text (streamed SSE)                │  │
│  └─────────────┬─────────────────────────────────────────────────────┘  │
│                │  Ollama prompt                                         │
│                ▼                                                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Ollama  (Llama 3.1 8B / Qwen2.5 7B on 11 GB GPU)                 │  │
│  │   • Intent classification: 1.5B model, < 400 ms                   │  │
│  │   • Heavy summarization: 8B model                                 │  │
│  │   • OLLAMA_NUM_PARALLEL=1 (single-GPU serialization)              │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Reverse Proxy + TLS                                               │  │
│  │   • Caddy (HTTPS, WSS upgrade)  or  Tailscale (WireGuard mesh)    │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 2. The Five Golden Rules

| # | Rule | Enforcement |
|---|------|-------------|
| 1 | **Thin Client** | Local STT + local TTS. Only text crosses the network. Idle RAM < 200 MB. |
| 2 | **Fat Server** | n8n intent routing + Ollama LLM. No audio processing on the server. |
| 3 | **Text-only protocol** | WebSocket carries only JSON text frames. Binary frames are REJECTED by the sidecar. |
| 4 | **Privacy** | Microphone audio never leaves the device. STT runs on `127.0.0.1:8000`. |
| 5 | **Zero idle CPU** | Wake-word via openWakeWord KWS in Rust (~1-2% CPU), not a JS AudioWorklet loop. |

---

## 3. The Three Trigger Paths

NEXUS can be woken in three ways. All three converge on the same frontend handler `window.__NEXUS_WAKE__()`.

```
PATH A — Spoken wake word "NEXUS"
  Microphone (cpal, 16 kHz mono)
    → openWakeWord KWS (tract-onnx, 80 ms sliding window)
    → 3-stage ONNX: melspectrogram → embedding → classifier
    → probability > 0.5 + refractory 2000 ms
    → (optional) speaker verification
    → emit wake  →  win.eval("window.__NEXUS_WAKE__()")

PATH B — Tier 3 acoustic command (e.g. "open youtube")
  Same microphone + same 3-stage pipeline
    → command classifier (parallel with wake classifier)
    → probability > threshold
    → emit "command-detected" Tauri event with structured intent
    → frontend executes directly (no STT, ~200 ms)

PATH C — Global hotkey (Ctrl/Cmd+Shift+Space)
  tauri-plugin-global-shortcut
    → show overlay + focus
    → win.eval("window.__NEXUS_WAKE__()")
```

**PATH C always works**, even during meetings or manual pause — it's an explicit user action. Paths A and B are suppressed when a meeting is detected or NEXUS is manually paused.

---

## 4. The Three Response Paths

Once awake, NEXUS follows one of three response paths depending on what was said.

```
PATH 1 — Tier 3 fixed command (e.g. "mute volume", "lock screen")
  Command classifier fires  →  frontend  →  invoke("execute_command")
    → Rust executes locally  →  TTS "Ok sir."  →  hide
  Latency: ~200 ms. No STT, no network.

PATH 2 — Tier 3 parameterized command (e.g. "play <song> in spotify")
  Command classifier fires  →  frontend speaks "On it sir"
    → record 3 s  →  local STT  →  extract parameter
    →  invoke("execute_command", { action, query: param })
    →  Rust executes  →  TTS result  →  hide
  Latency: ~3-5 s (STT parameter capture).

PATH 3 — General / unknown request (e.g. "summarize my email")
  Wake  →  mic capture  →  Silero VAD  →  silence  →  finishCapture
    →  local STT (faster-whisper on 127.0.0.1:8000)  →  transcript
    →  frontend intent parser (regex + phonetic)
    →  if local intent matches  →  execute locally
    →  else  →  WSS to sidecar  →  n8n  →  Ollama  →  result text
    →  TTS speaks ack + result locally
  Latency: 5-30 s (depends on Ollama).
```

---

## 5. Process Topology at Runtime

On a Windows machine with NEXUS running, you will see these processes:

| Process | Purpose | Started by |
|---------|---------|------------|
| `nexus.exe` | Tauri main process (Rust) + WebView2 host | Autostart (LaunchAgent / registry / .desktop) |
| `pythonw.exe -m uvicorn sidecar.sidecar:app` | FastAPI sidecar (no console window) | `nexus.exe` via `sidecar_manager::init()` |
| (optional) `python.exe` STT server | faster-whisper on `127.0.0.1:8000` | User / future: bundled by installer |

The sidecar is deliberately left running after NEXUS exits, so the next launch detects it on port `49152` and skips spawning — instant startup.

---

## 6. Where to Go Next

| If you want to understand… | Read |
|----------------------------|------|
| The end-to-end sequence of a single request | [02-data-flow-graphs.md](./02-data-flow-graphs.md) |
| Which file does what | [03-component-map.md](./03-component-map.md) |
| Why each crate / library was chosen | [04-tech-stack.md](./04-tech-stack.md) |
| The frontend state machine | [05-state-machine.md](./05-state-machine.md) |
| A specific feature in depth | [../features/](../features/) |
| How credentials / API keys work | [../credentials/](../credentials/) |
| What changed and why (per commit) | [../changes/](../changes/) |
