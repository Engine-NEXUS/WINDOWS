> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# CHANGELOG — 2026-08-31

## Startup Glitch Fixes

### Fixed
- **Log tail repeating every second** — `Get-NewLines` in `run.ps1` used
  `StreamReader` which buffers ahead, causing `$fs.Position` to overshoot
  the actual file end. The position tracker reset to 0 every cycle,
  re-reading the entire log. Fixed by reading raw bytes with
  `FileStream.Read`.

- **False wake trigger on startup** — The wake word model fired
  immediately on boot (probability 0.911) from audio stream
  initialization noise. Added a 3-second grace period
  (`engine_start_time` on `WakeEngine`) during which all detections are
  ignored.

- **Alt+Space hotkey killed WhatsApp windows** — `Alt+Space` is the
  Windows system menu shortcut. Registering it globally intercepted all
  `Alt+Space` events system-wide, causing WhatsApp and other apps to
  glitch when the system menu event was swallowed. Removed from the
  hotkey list.

- **WhatsApp closed on every `nexus start`** — `run.ps1` killed ALL
  `msedgewebview2.exe` processes on startup. WhatsApp Desktop, M365
  Copilot, and Windows Search all use WebView2. Fixed by walking the
  process tree from `nexus.exe` PIDs and killing only NEXUS's own
  WebView2 descendants.

- **PowerShell `[ref]` hashtable bug caused repeating logs** — Position
  tracking used `$pos = @{...}` passed via `[ref]$pos.STT`. PowerShell
  creates a boxed copy for hashtable property refs, so updates were lost
  and positions stayed at 0 forever. Fixed by using simple variables
  (`$posSTT`, `$posRust`, etc.).

- **Sidebar stuck on "Thinking..."** — `wsBridge.ts` used dynamic
  `import()` for Tauri events, which failed in bundled mode. Fixed by
  using the synchronous `window.__TAURI__.event.listen` global API.

### Files Changed
- `scripts/run.ps1` — log tail fix, WebView2 selective kill, position
  tracking variables
- `src-tauri/src/wakeword_oww.rs` — 3-second startup grace period
- `src-tauri/src/hotkey.rs` — removed Alt+Space hotkey
- `frontend/src/net/wsBridge.ts` — synchronous Tauri event listener

### Metrics
| Metric | Before | After |
|--------|--------|-------|
| Log repetition | Every 1s | None |
| False wakes on startup | 1 per boot | 0 |
| WhatsApp killed on start | Yes | No |
| Hotkeys registered | 3 (+ Alt+Space) | 2 |
| Sidebar analysis flow | Stuck on Thinking | Works |
