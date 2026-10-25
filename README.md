# NEXUS — a voice-first desktop assistant

NEXUS is a Windows-first desktop assistant you talk to. A native client (Tauri v2 + Rust, React front end) listens for a wake word, turns speech into text, runs what it can on your machine, and sends the rest to cloud services. It floats above your work as a small orb and can open apps, control media, manage files, watch your mail, review GitHub pull requests, and drive the mouse and keyboard.

> **Status: pre-release (v0.1, development builds).** Many features are built and unit-tested but have not been exercised live on real hardware — the engineering log in [`AGENTS.md`](AGENTS.md) lists what was and was not run live for each one. Builds are not code-signed yet.

* Quick start: [QUICKSTART.md](QUICKSTART.md)
* Documentation index: [docs/README.md](docs/README.md)
* Privacy and what leaves your device: [privacy.html](privacy.html) · [terms.html](terms.html)

---

## What it does

| Area | What works today |
|---|---|
| **Voice** | Wake word *"NEXUS"* (openWakeWord, runs on-device), `Ctrl+Space` hotkey to start or cancel a turn, barge-in, meeting-privacy mode that stops the mic during calls. Wake-word accuracy is still being tuned (see `AGENTS.md`). |
| **Speech to text** | Groq `whisper-large-v3-turbo` (cloud) is primary; local Moonshine is the offline fallback; Deepgram is optional. English only. |
| **Speech out** | Microsoft Edge neural voices online (default persona "JARVIS", 10 personas), local Kokoro-82M voice only when offline. |
| **Understanding** | Deterministic parser first, then a small on-device BERT-Mini classifier, then cloud LLMs (Cerebras → Groq → Gemini → the Worker). |
| **Desktop control ("Ghost mode")** | Open and focus apps, keyboard-first control with Windows UI Automation, dictation, WhatsApp opening, voice file operations with quarantine-delete and undo (Windows only). |
| **Hand mode** | Camera-based pointer and gestures (Windows only, experimental). |
| **Memory** | Encrypted on-device memory (facts, resume points, timetable, important-mail previews) with a log of what is sent to the cloud. |
| **Google** | Gmail deadline watcher, Calendar read/write, Maps directions — requires your own Google OAuth client. |
| **GitHub** | Voice PR analysis and 28 PR/repo commands; architecture mapper with import graph and blast radius. |
| **Coding agents** | Starts Claude Code, Codex, Aider and others and tells you when they finish. |

## Architecture

```
NEXUS desktop (Rust + Tauri v2, React UI)
  ├─ wake word (openWakeWord via tract-onnx)              on-device
  ├─ intent parser + BERT-Mini NLU                        on-device
  ├─ memory core (AES-256-GCM encrypted SQLite)           on-device
  ├─ action kernel (file policy, quarantine, audit)       on-device
  │
  ├─ Groq / Deepgram            speech to text            cloud
  ├─ Microsoft Edge TTS         text to speech            cloud (unofficial endpoint)
  ├─ Cerebras / Groq / Gemini   reasoning, vision         cloud
  └─ Cloudflare Worker + D1     OAuth tokens, quotas,     cloud
                                device identity, OTA models
```

Local Python sidecars (Moonshine STT, NLU server) are started on demand on `127.0.0.1`. Node is needed for the optional coding-agent watcher.

## What leaves your device

NEXUS is local-first, **not** offline-only. Depending on what you use, these are sent to third parties:

* **Audio** of your spoken command → Groq (or Deepgram if configured). `localSttOnly` is meant to keep audio on the device, but it has had bugs (see `AGENTS.md`) — verify before relying on it.
* **Transcripts** and a ranked, PII-redacted **memory context pack** → the NEXUS Worker and the LLM providers it routes to.
* **Spoken replies** (including names in alerts) → Microsoft Edge TTS.
* **Screenshots** for screen analysis and vision grounding → Google Gemini.
* **OAuth tokens** for GitHub, Google and Swiggy → stored by the NEXUS Cloudflare Worker (D1). API keys are kept in your OS keychain.

The Memory page in the Command Hub shows a 7-day log of exactly what each cloud turn sent. There is no analytics or telemetry. Full details: [privacy.html](privacy.html).

## Build and run (development)

Prerequisites: Node.js 20+, Rust 1.77+, Windows 10/11 with WebView2. Python 3.11/3.12 is needed for the local STT/NLU sidecars; Node is needed for the agent watcher.

```bash
npm --prefix frontend install
node nexus.mjs build        # frontend + Rust release build (adds the required custom-protocol feature)
node nexus.mjs start        # run with the unified console
```

A bare `cargo build --release` produces a binary that points at the Vite dev server; always build through `nexus build` or pass `--features custom-protocol`. `nexus help` lists the other commands (`dev`, `check`, `train`, `collect`, `audit`, `wake`).

Run the checks CI runs:

```bash
(cd frontend && npx tsc --noEmit)
(cd frontend && npm test)
(cd src-tauri && cargo test --lib --features custom-protocol,admin-brain -- --test-threads=1)
(cd server/worker && npm test)
```

## Platforms

| Platform | State |
|---|---|
| Windows 10/11 | Primary target. Installer builds in CI; **unsigned**. |
| macOS | Compiles in release CI; not tested. Ghost, Hand mode and UIA are Windows-only; the OS keychain is not wired up on macOS yet, so do not use it with real credentials. |
| Linux | Media control only; not supported. |

## License and third-party software

No licence file has been added to the repository yet. Third-party components and model licences are being inventoried in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) (partial).

© 2026 Engine-NEXUS Team.
