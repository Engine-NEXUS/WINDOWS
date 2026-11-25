# Four-star programme

Owner goal (2026-11-18): every in-scope feature above 4 stars (raw score above 80%) before a new feature is planned. One feature at a time, lowest score first: fix the flow, test it, prove it live, then update the scorecard from evidence and move on.

Source: `docs/testing/scorecard-data.json` (weights B 0.20, T 0.20, L 0.30, S 0.15, I 0.15). Stars = raw / 20. Starting point: 8 of 68 above 4 stars.

**Live proof (L) carries 30%.** A feature with L = 0 tops out at 70% (3.5 stars) however good the code is, so every feature below needs a live run. "Proved by" says who can run it: *Claude* = I can run it myself (hidden test desktop with `NEXUS_QUIET=1`, Windows Sandbox, offline runs, local CLIs); *Owner* = needs your voice, ears, camera, phone, accounts or a reboot. Owner steps are batched into short sessions so they do not block the queue.

Status: todo, working, waiting for owner, done (above 4 stars with evidence).

| # | Feature | Now | Stars | Weakest axes | Proved by | Status |
|---:|---|---:|---:|---|---|---|
| 1 | Speaker verification / owner check | 41.0% | 2.05 | L 10, I 30, S 50, T 55, B 75 | Owner voice + public impostor clips | working |
| 2 | Reverse engineer website/app | 48.0% | 2.40 | L 0, I 40, T 65 | Claude (hidden desktop) | todo |
| 3 | Focus Mode (Hand Mode load control) | 48.0% | 2.40 | L 0, I 20, T 75 | Owner camera | todo |
| 4 | GitHub by voice without owner (repo, PR list, PR number search) | 50.5% | 2.52 | L 0, I 30 | Claude (hidden desktop, GitHub sign-in) | todo |
| 5 | Notification brain: state machine and GitHub Actions / deploy watchers | 50.5% | 2.52 | L 0, I 30 | Claude (test repo) + owner OK to push | todo |
| 6 | Window theming (solid panels) | 51.5% | 2.58 | L 0, I 60, T 70, S 70 | Claude (hidden desktop) | todo |
| 7 | WhatsApp priority people (read-only) | 52.5% | 2.63 | L 5, I 50, T 65 | Owner phone (WhatsApp pairing) | todo |
| 8 | Vision-grounded click | 54.2% | 2.71 | L 25, I 40, T 66, S 70 | Claude (hidden desktop) | todo |
| 9 | Agent watch sidecar (spawn + watch CLIs) | 55.0% | 2.75 | T 30, L 45, I 60, S 70 | Claude (local CLIs) | todo |
| 10 | Proactive-speech policy | 56.0% | 2.80 | L 10, I 55, T 70 | Claude (quiet mode) | todo |
| 11 | Leave-by reminders | 56.0% | 2.80 | L 0, I 55 | Owner calendar + Routes key | todo |
| 12 | Wake-up sound | 57.0% | 2.85 | L 10, I 60, T 65 | Owner ears | todo |
| 13 | Boot hello / shutdown flow / resume hello | 57.0% | 2.85 | L 25, T 50, I 60, S 70 | Owner reboot | todo |
| 14 | Track all agents (announce each, summary) | 58.8% | 2.94 | L 5, I 65, T 70 | Claude (local CLIs) | todo |
| 15 | Profile, corrections, routines, review, curator | 58.8% | 2.94 | L 10, I 60, T 75 | Claude (quiet mode) | todo |
| 16 | Important-mail watcher + triage | 59.5% | 2.98 | L 10, I 65, T 75 | Owner Google account | todo |
| 17 | NLU fallback (BERT-Mini ONNX) | 59.8% | 2.99 | L 40, T 55, I 60, S 65 | Claude (held-out set) | todo |
| 18 | Endpointing / VAD / turn detection | 60.0% | 3.00 | T 45, I 50, L 55, S 60 | Owner voice | todo |
| 19 | Device Links (phone / laptop pairing) | 61.3% | 3.06 | L 35, I 60, T 70, B 75 | Owner phone | todo |
| 20 | Installer / release pipeline | 61.8% | 3.09 | I 50, T 55, L 55, S 65 | Claude (Windows Sandbox) | todo |
| 21 | Boot briefing + resume points | 63.1% | 3.16 | L 25, I 60, T 78 | Owner reboot | todo |
| 22 | Session snapshot / restore | 63.5% | 3.17 | L 20, I 60 | Owner reboot | todo |
| 23 | Screen analysis / narrated tour | 64.2% | 3.21 | L 30, I 55, S 78 | Claude (hidden desktop) + owner ears | todo |
| 24 | Overnight record + morning briefing | 64.8% | 3.24 | L 20, I 60 | Owner overnight run | todo |
| 25 | Live mode: WhatsApp / browser flows | 65.3% | 3.26 | L 45, I 45, T 75 | Claude (hidden desktop) + owner phone | todo |
| 26 | Timetable from an image + reminders | 65.5% | 3.27 | L 45, I 60, T 65 | Claude (quiet mode) | todo |
| 27 | Calendar read / add | 66.3% | 3.32 | L 25, I 65 | Owner Google account | todo |
| 28 | TTS offline (Kokoro-82M) | 66.5% | 3.33 | L 50, I 50, T 70 | Claude (offline run) | todo |
| 29 | Music control (Spotify / Apple Music) | 67.0% | 3.35 | L 35, I 65, T 75 | Owner Spotify | todo |
| 30 | Directed-speech gate (TV / side-talk filter) | 67.0% | 3.35 | L 35, I 55 | Owner voice + TV | todo |
| 31 | Compound commands ("X then Y") | 68.0% | 3.40 | L 40, I 55, T 75 | Claude (quiet mode) | todo |
| 32 | Activity Manager (app-only history) | 68.0% | 3.40 | L 40, I 55 | Claude (quiet mode) | todo |
| 33 | Hand mode (camera pointer) | 69.0% | 3.45 | I 45, L 55, T 70 | Owner camera | todo |
| 34 | CI gates: dead code, IPC, turn-end, hygiene | 69.0% | 3.45 | L 40, T 70, I 75 | Claude (GitHub runner) | todo |
| 35 | MCP connect system (Swiggy, Amazon, WhatsApp) | 70.3% | 3.51 | L 40, I 70, T 75 | Owner accounts | todo |
| 36 | Google accounts + OAuth | 71.0% | 3.55 | L 40, I 68 | Owner Google account | todo |
| 37 | Captions (word-by-word, black capsule) | 71.5% | 3.58 | L 55, T 70, S 70, I 70 | Claude (quiet mode) + owner eyes | todo |
| 38 | Dictation (type what I say) | 71.5% | 3.58 | L 40, I 65 | Owner voice | todo |
| 39 | Wake word "NEXUS" (openWakeWord, own model) | 71.8% | 3.59 | I 40, L 55, S 75 | Owner voice | todo |
| 40 | Orb UI and turn lifecycle | 71.9% | 3.60 | L 60, S 65, I 65, T 72 | Owner voice | todo |
| 41 | Ghost mode (voice drives the PC) | 72.0% | 3.60 | I 55, L 60, T 65 | Owner voice | todo |
| 42 | Mishearing normaliser + learned corrections | 72.0% | 3.60 | L 55, I 62, T 75, S 75 | Claude (held-out set) | todo |
| 43 | Local STT fallback (Moonshine) | 72.5% | 3.63 | L 60, T 70, S 75, I 75 | Claude (offline run, recorded clips) | todo |
| 44 | Maps / Places / Directions | 72.7% | 3.64 | L 55, I 68, T 75 | Claude (quiet mode, API keys) | todo |
| 45 | Voice personas + offline twin swap | 73.8% | 3.69 | L 55, I 70, T 75 | Claude (quiet mode) + owner ears | todo |
| 46 | Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) | 74.0% | 3.70 | L 40, I 70 | Claude (local CLIs) | todo |
| 47 | TTS volume lease / restore | 74.5% | 3.73 | L 55, T 70, I 75 | Claude (quiet off, volume probe) | todo |
| 48 | Ask-first shutdown / sleep / restart | 74.8% | 3.74 | L 50, I 70, T 75 | Owner voice (real shutdown) | todo |
| 49 | Gated commit + draft PR | 75.0% | 3.75 | L 45, I 65 | Claude (test repo) + owner OK to push | todo |
| 50 | Resource footprint (RAM / CPU / battery) | 75.3% | 3.76 | T 40, S 75 | Claude (measure) | todo |
| 51 | Window tiling + display off | 75.7% | 3.79 | L 55, I 60 | Claude (hidden desktop) + owner display | todo |
| 52 | 9Router (Cerebras > Groq > Gemini > Worker) | 76.0% | 3.80 | T 55, L 70, S 75 | Claude (quiet mode) | todo |
| 53 | Command Hub / settings UI | 76.1% | 3.80 | L 65, S 70, I 70, T 78 | Claude (hidden desktop) | todo |
| 54 | Main centre / 17 sub-centres validation | 76.5% | 3.83 | I 60, T 65, L 75 | Claude (quiet mode) | todo |
| 55 | Security: keychain, PII filter, sealed memory | 77.4% | 3.87 | L 50, I 78 | Claude (quiet mode) | todo |
| 56 | Shortcut map + action router | 77.8% | 3.89 | L 60, I 70 | Claude (hidden desktop) | todo |
| 57 | Keep-awake | 78.0% | 3.90 | L 65, I 70 | Claude (powercfg) | todo |
| 58 | Memory core (encrypted store, ranked recall, context pack) | 78.5% | 3.92 | L 55, I 70 | Claude (quiet mode) | todo |
| 59 | Open / close / focus apps | 79.8% | 3.99 | T 55, I 70, S 75 | Claude (hidden desktop) | todo |
| 60 | Architecture mapper | 79.9% | 4.00 | I 68, L 70, S 78 | Claude (hidden desktop) | todo |
| 61 | Deterministic intent parser (regex tier) | 81.0% | 4.05 | I 55, L 75, S 75 | - | done |
| 62 | Watch one agent, then power off | 81.3% | 4.06 | L 65, I 70 | - | done |
| 63 | GitHub commands (PRs, merge, review) | 81.3% | 4.06 | L 70, I 70 | - | done |
| 64 | Cloudflare Worker | 82.5% | 4.13 | L 75, I 75 | - | done |
| 65 | Global hotkey Ctrl+Space (call / cancel) | 85.3% | 4.26 | T 75 | - | done |
| 66 | TTS online (Edge, persistent connection) | 85.8% | 4.29 | T 70 | - | done |
| 67 | Action Kernel (voice file control, quarantine delete, undo) | 86.0% | 4.30 | L 75, I 75 | - | done |
| 68 | Cloud STT (Groq Whisper turbo) + hallucination filter | 87.7% | 4.38 | - | - | done |

## Rules

* Never raise a score from source or CI alone; L needs a live run recorded in `docs/testing/live-proof-ledger.md` and `docs/testing/runs/`.
* After each feature: update `scorecard-data.json`, run `node scripts/make-work-cards.mjs`, update this table.
* A feature that needs only owner time is marked *waiting for owner*; work continues with the next one.
