# NEXUS — a voice-first desktop assistant

NEXUS is a Windows-first desktop assistant you talk to. A native client (Tauri v2 + Rust, React front end) listens for a wake word, turns speech into text, runs what it can on your machine, and sends the rest to cloud services. It floats above your work as a small orb and can open apps, control media, manage files, watch your mail, review GitHub pull requests, and drive the mouse and keyboard.

> **Status: pre-release (v0.1, development builds).** Many features are built and unit-tested but have not been exercised live on real hardware — the engineering log in [`AGENTS.md`](AGENTS.md) lists what was and was not run live for each one. Builds are not code-signed yet.

* Developer setup: [CONTRIBUTING.md](CONTRIBUTING.md) (isolated contributor workflow; device qualification pending)
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

Start with Git and Node 22 (24 also accepted). From a cloned checkout:

```text
node nexus.mjs doctor       # read-only prerequisite report
node nexus.mjs build        # approved setup + isolated contributor compilation
node nexus.mjs start        # matching existing build only; never stops another instance
```

System installations require approval; Python 3.12 is managed in `.venv` and Rust is pinned to 1.93.1. Default contributor builds use separate settings, accounts, credentials and output, exclude the admin brain, and disable personal autostart changes. Existing installed personal builds retain their identity. Global `nexus` registration is optional.

Read [CONTRIBUTING.md](CONTRIBUTING.md) for OS prerequisites, development/test commands, explicit personal builds, packaging, limitations and clean-machine verification. Contributor source/offline checks are not real-device support proof. Do not use a bare Cargo build for onboarding.

## Platforms

| Platform | State |
|---|---|
| Windows 10/11 | Primary target. Installer builds in CI; **unsigned**. |
| macOS | Port/setup code and native Keychain dependency exist; live behavior unverified. Initial contributor target: macOS 15+ Apple Silicon. |
| Linux | Initial contributor target: Ubuntu 24.04 x64 X11. Persistent Secret Service configured; clean-machine/device proofs pending. Other desktops/distributions experimental. |

## License and third-party software

No licence file has been added to the repository yet. Third-party components and model licences are being inventoried in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) (partial).

© 2026 Engine-NEXUS Team.
