# NEXUS — Quick Start

NEXUS is pre-release software for Windows 10/11. This guide gets a development or test build running and gives you a few things to try. For what the project is and what it sends to the cloud, read the [README](README.md) first.

## 1. Install or build

**From an installer** (if you were given one): run `NEXUS_*_x64-setup.exe`. It installs per user under `%LOCALAPPDATA%\NEXUS` and does not need Administrator rights. The build is **unsigned**, so Windows SmartScreen will warn you — choose *More info → Run anyway* only if you trust where it came from.

**From source:**

```bash
npm --prefix frontend install
node nexus.mjs build     # frontend + Rust release build
node nexus.mjs start     # run it (unified console on Windows)
node nexus.mjs check     # diagnostics for tools, Python/Node, the Worker and models
```

You need Node.js 20+, Rust 1.77+, and WebView2. Python 3.11/3.12 is used for the local speech and NLU helpers and is started only when needed; Node is needed only for the coding-agent watcher.

## 2. First launch

1. The **Setup Wizard** opens: allow the microphone and pick a voice (the default is the "JARVIS" persona; 10 are available).
2. Optionally connect accounts. Google needs **your own** OAuth client ID/secret (Command Hub → Advanced); GitHub and Swiggy use the NEXUS Worker.
3. Add your own **API keys** (Groq for speech-to-text, Gemini/Cerebras for reasoning) in the Command Hub → API Keys. Without a Groq key, speech recognition falls back to the slower local model.

## 3. Talk to it

* Press **`Ctrl+Space`** to start listening; press it again to cancel.
* Or say **"NEXUS"** (wake-word accuracy is still being tuned, so the hotkey is the reliable route).
* `Esc` closes the Command Hub and settings windows and cancels a Ghost-mode session. `Ctrl+Alt+X` is the kill-switch for the full-screen overlay.

## 4. Things to try

| Say | What should happen |
|---|---|
| "Open Notepad" / "Open Chrome" | Opens or focuses the app. |
| "Pause the music" / "Next song" | Controls Spotify or Apple Music. |
| "What is the capital of France?" | Answered by the cloud reasoning chain and spoken aloud. |
| "Delete the PDFs in Downloads" | NEXUS reads back what it will move, waits for "yes", then moves the files to `.nexus-trash`. "Undo that" restores them. |
| "Ghost mode" | Hands-free desktop control; say "exit ghost mode" or press `Esc` to stop. |
| "Analyse PR 254 in owner/repo" | Fetches the diff from GitHub and shows a review in the sidebar (needs GitHub connected). |
| "Open architecture mapper" | Opens the codebase mapper for a repository (layers, dependency graph, blast radius). |
| "What do you remember?" | Shows what the memory core holds, with where each item came from. |

Several of these have only been tested in unit tests, not by voice on real hardware. If something does not work, run `node nexus.mjs check` and see the engineering log in [`AGENTS.md`](AGENTS.md) for known gaps.

## 5. Troubleshooting

| Issue | What to do |
|---|---|
| Microphone silent or blocked | Use `Ctrl+Space`. Check *Windows Settings → Privacy → Microphone* allows desktop apps. |
| Intel Smart Sound mic goes silent (RMS 0.000000) after a few minutes | Known driver bug. Device Manager → *Sound, video and game controllers* → *Intel Smart Sound Technology for Digital Microphones* → Disable, then Enable. Or `Restart-Service -Name "Audiosrv" -Force` in an Administrator PowerShell. Updating the OEM audio driver can help. |
| No speech recognition | Add a Groq API key, or check that Python is installed so the local Moonshine fallback can start. |
| Voice sounds different or is silent offline | Edge TTS needs the internet; the offline Kokoro voice downloads on first online run. |
| Change voice or settings | Right-click the tray icon → *Settings*, or say "open the command hub". |
| Windows shows a blank overlay or "localhost refused to connect" | The binary was built without `--features custom-protocol`. Rebuild with `node nexus.mjs build`. |
