> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — Tech Stack & Selection Rationale

> Every crate, library, and tool chosen for NEXUS, with the reason it was picked over alternatives.
> If you're considering swapping something out, read the "Why not X?" column first.

---

## Rust Main Process

| Concern | Crate | Why this one | Why not X? |
|---------|-------|--------------|------------|
| App shell | `tauri` v2 | Only option giving transparent overlay + IPC + cross-platform tray + Rust core in one package. | Electron (too heavy, ~150 MB); Flutter (no transparent overlay); Qt (no webview). |
| Wake word | `tract-onnx` + openWakeWord KWS | Pure Rust ONNX inference. < 2% CPU, offline, no JS event loop, no native C dep. | Porcupine (API key + online activation — violates privacy-first); sherpa-onnx VAD+ASR (~30% recall, clips word starts); WebAssembly AudioWorklet (forces webview to tick every audio quantum). |
| Audio capture | `cpal` | Cross-platform PCM. Route 16 kHz mono frames to OWW. | `rodio` (higher-level, less control over raw frames); Web Audio API (can't run with webview torn down). |
| Global hotkey | `tauri-plugin-global-shortcut` | Official, works on Windows + macOS + Linux (incl. Wayland via evdev/x11). | OS-specific APIs (more code, less portable). |
| Autostart | `tauri-plugin-autostart` | Official. Uses LaunchAgent (macOS) / registry (Windows) / .desktop (Linux). | Manual registry edits (already done for browser suppression, but autostart is cleaner via plugin). |
| Tray | `tauri::tray` (v2 built-in) | Native tray on all 3 OSes. | `tray-icon` crate (less Tauri integration). |
| Async runtime | `tokio` (current_thread flavor) | Lower idle footprint; single worker suffices for NEXUS's I/O. | `async-std` (less ecosystem); multi-thread tokio (overkill for one WSS connection). |
| WSS client | `tokio-tungstenite` + `rustls` | Pure-Rust TLS, no OpenSSL system dep (critical for cross-compile). | `tungstenite` + native-tls (OpenSSL dep on Linux). |
| HTTP client | `reqwest` | Async, multipart (for STT PCM upload), TLS via rustls. | `ureq` (blocking); `hyper` (too low-level). |
| Logging | `tracing` + `tracing-subscriber` | Async-friendly, zero-cost when off, structured. | `log` (no span context); `env_logger` (no async). |
| Single instance | `tauri-plugin-single-instance` | Prevents duplicate autostart launches. Also catches deep-link args on Windows/Linux. | Manual mutex file (fragile). |
| System info | `sysinfo` | Uptime (for boot greeting), process list (for meeting detection fallback). | `wmi` (Windows-only); manual Win32 calls (more code). |
| Process spawn | `std::process::Command` + `CommandExt` | Cross-platform + Windows `CREATE_NO_WINDOW` flag. | `tokio::process` (overkill for one-shot sidecar spawn). |
| WebView2 COM | `webview2_com` | Direct access to WebView2 `PermissionRequested` event for mic auto-allow. | wry's built-in handler (only handles clipboard, not mic). |
| Fuzzy match | Hand-rolled Levenshtein + HashMap | App registry needs simple fuzzy + exact lookup. No dep needed. | `strsim` (would work, but we only need Levenshtein). |
| ONNX (legacy) | `sherpa-onnx` | Used by the legacy VAD+ASR wake word + speaker verification. | `tract-onnx` (doesn't support all sherpa model ops). |

---

## Frontend (WebView)

| Concern | Library | Why this one | Why not X? |
|---------|---------|--------------|------------|
| UI framework | React 18 + TypeScript + Vite | Mature, small webview footprint, huge ecosystem. | Svelte (smaller but less ecosystem); Vue (fine, but React is more familiar). |
| State | `zustand` | 1 KB, no provider boilerplate, perfect for a state machine. | Redux (too much ceremony); Context (re-renders). |
| VAD | `@ricky0123/vad-web` (Silero ONNX) | Neural VAD — distinguishes speech from noise even at low volume. Same approach as Alexa/Siri. | RMS energy detection (can't distinguish speech from background noise at low volume). |
| ONNX runtime (browser) | `onnxruntime-web` | Runs Silero VAD in the browser via WASM. | TensorFlow.js (heavier, no Silero model). |
| Mic capture | `ScriptProcessorNode` | Proven reliable in WebView2/Electron. | `AudioWorkletNode` (Chrome optimizes away silent paths — root cause of a previous bug). |
| TTS | Web Speech API (`SpeechSynthesis`) | Built into the browser. Zero dependency. Local. | `piper` / `coqui` (would need server-side or WASM — heavier). |
| Animation | `@rive-app/react-canvas` (Rive) | GPU vector state machines; idle cost ~0. | Three.js (heavy); Framer Motion (CPU-based). |
| Shell open | `@tauri-apps/plugin-shell` | Open URLs in the system browser (for OAuth). | `window.open` (opens in WebView, not system browser). |

---

## Python Sidecar

| Concern | Tool | Why this one | Why not X? |
|---------|------|--------------|------------|
| Web framework | `FastAPI` | Async, WebSocket support, auto-docs, typed. | `Flask` (no native async WS); `Starlette` (FastAPI is Starlette + typing). |
| ASGI server | `uvicorn` | Standard FastAPI runner. Launched as `pythonw -m uvicorn sidecar.sidecar:app`. | `gunicorn` (WSGI, no native WS); `hypercorn` (fine, but uvicorn is simpler). |
| HTTP client | `httpx` | Async, streaming (for n8n SSE), timeout control. | `aiohttp` (fine, but httpx API is cleaner); `requests` (blocking). |
| Database | `SQLite` | Zero-config, file-based, sufficient for single-user credential storage. | Postgres (overkill for a sidecar); Redis (no persistence needed here). |
| Encryption | `cryptography.fernet` | Symmetric encryption at rest for API keys. | Plain text (unacceptable); AES manual (error-prone). |

---

## Server / Backend

| Concern | Tool | Why this one | Why not X? |
|---------|------|--------------|------------|
| Orchestrator | `n8n` (self-hosted) | Visual workflow editor, webhook-triggered, sub-canvas fan-out. | LangChain (code-only, no visual editing); Dify (less mature). |
| LLM | `Ollama` (Llama 3.1 8B / Qwen2.5 7B) | Local, no API key, GPU-accelerated, streaming. | OpenAI API (cloud, cost, latency); vLLM (more setup). |
| STT (local) | `faster-whisper` (CTranslate2) | 10x faster than openai-whisper, runs on CPU. | `whisper.cpp` (fine, but faster-whisper is more optimized); cloud STT (violates privacy). |
| TLS / VPN | Caddy + Tailscale | Caddy for HTTPS reverse proxy; Tailscale for zero-config WireGuard mesh. | nginx (more config); manual WireGuard (more setup). |

---

## Build & CI

| Concern | Tool | Why |
|---------|------|-----|
| Frontend bundler | Vite | Fast HMR, small output, `viteStaticCopy` for VAD model files. |
| Rust build | `cargo` + `tauri build` | Standard Tauri pipeline. |
| CI | GitHub Actions | Matrix build for Windows + macOS + Linux on tag `v*`. |
| Installer (Windows) | NSIS + MSI (via Tauri) | Both produced by `tauri build`. |
| Installer (macOS) | `.dmg` (via Tauri) | Universal binary (aarch64 + x86_64). |
| Installer (Linux) | AppImage + `.deb` (via Tauri) | AppImage is the default Linux target. |

---

## Feature Flags (`Cargo.toml`)

| Flag | Default | Effect |
|------|---------|--------|
| `wakeword-oww` | ✅ on | Use openWakeWord KWS (tract-onnx). The current, recommended engine. |
| `wakeword-sherpa` | ❌ off | Use legacy VAD+ASR (sherpa-onnx). Fallback only. |
| `wakeword-porcupine` | ❌ off | Use Porcupine. Legacy, requires API key. |
| `mock-wake` | ❌ off | Skip the wake engine entirely. Hotkey only. Used in CI where there's no audio device. |

---

## Port Allocation

| Port | Service | Why this port |
|------|---------|---------------|
| `49152` | Python sidecar (FastAPI) | IANA dynamic/private range (49152-65535). Avoids conflicts with common dev ports (3000, 5173, 8000, 8080, 8443). Configurable via `NEXUS_SIDECAR_PORT`. |
| `8000` | Local STT server (faster-whisper) | Conventional for the local STT. Configurable via `NEXUS_LOCAL_STT_URL`. |
| `5678` | n8n (on the server) | n8n's default port. |
| `11434` | Ollama (on the server) | Ollama's default port. |
