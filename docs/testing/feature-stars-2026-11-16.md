# Feature stars out of 5 (2026-11-16)

I didn't re-test anything. I converted the repo's own scorecard (`docs/testing/scorecard-data.json`, 72 features) into stars. Stars are the weighted score divided by 20, rounded to the nearest half. The score weights Built 20%, Tested 20%, Live-proven 30%, Safety 15% and Industry-comparison 15%. Live-proven carries the most weight and is the weakest column for almost everything.

## Improve first (2.5 stars or less)

| Stars | Feature | Why it's low |
|---|---|---|
| 2.0 | Speaker verification / Voice Lock | Live 10%, industry 30%. Replay is not blocked, and the owner action trial still hasn't passed. |
| 2.5 | Installer / release pipeline | Tests 30%, live 35%. Plan: [194](../features/194-installer-release-pipeline-above-4-stars-plan.md). |
| 2.5 | Reverse engineer website/app | Never run live. |
| 2.5 | Focus Mode | Never run live. |
| 2.5 | GitHub by voice without owner | Never run live. |
| 2.5 | Notification brain and deploy watchers | Never run live. |
| 2.5 | Window theming (solid panels) | Never run live. |
| 2.5 | WhatsApp priority people | Live 5%, and the read-receipt test hasn't been done. |
| 2.5 | Vision-grounded click | Live 25%, industry 40%. |

## Middling (3 to 3.5 stars)

- **3.0 stars:**
  - Agent watch sidecar
  - Proactive-speech policy
  - Leave-by reminders
  - Wake-up sound
  - Boot hello / shutdown flow
  - Track all agents
  - Profile / corrections / routines
  - Important-mail watcher
  - NLU fallback
  - Endpointing / VAD
  - Device Links
  - Boot briefing
  - Session restore
  - Screen analysis / tour
  - Overnight record
- **3.5 stars:**
  - Live mode (WhatsApp / browser)
  - Timetable from an image
  - Calendar
  - Offline TTS (Kokoro)
  - Music control
  - Directed-speech gate
  - Compound commands
  - Activity Manager
  - Hand mode
  - CI gates
  - MCP connect
  - Google accounts
  - Captions
  - Dictation
  - Orb UI
  - Ghost mode
  - Mishearing normaliser
  - Local STT (Moonshine)
  - Maps / Places / Directions
  - Voice personas
  - Agent hooks
  - TTS volume lease
  - Ask-first shutdown
- **Wake word "NEXUS" (3.5 stars):** the score is better than the evidence. The honest held-out measurement is 63% recall with about 12 false fires per hour. The target is at least 95% recall at no more than 0.1 false fires per hour.

## Good (4 stars or more)

- **4.5 stars:**
  - Cloud STT (Groq)
  - Online TTS (Edge)
  - Action Kernel (file control with undo)
  - Ctrl+Space hotkey
- **4.0 stars:**
  - Cloudflare Worker
  - GitHub commands
  - Architecture mapper
  - Intent parser
  - Watch-then-power-off
  - Open/close apps
  - Memory core
  - Keep-awake
  - Shortcut router
  - Security (keychain / PII filter)
  - Main centre
  - Command Hub
  - 9Router
  - Window tiling
  - Resource footprint
  - Gated commit + PR

## By area (average score)

| Area | Score |
|---|---|
| Memory and proactive | 61% |
| Platform and quality | 62% |
| Integrations | 67% |
| Voice front end | 68% |
| Desktop control | 68% |
| Understanding | 69% |
| Agent control | 70% |

## Out of scope in the scorecard (lowest stars)

- macOS / Linux parity: 1.0
- Administrator control: 1.5
- Streaming captions (Deepgram): 2.5
- Admin brain (Qwen): 2.5

## Caveats

- **Unscored features.** The scorecard doesn't yet include:
  - Reminder Roll
  - the Directions rebuild
  - Agent Island
  - agent auto-hook
  - logoapi logos
  - the Quiet test mode
  - the locked-screen probe

  The locked-screen probe isn't compiled yet. Rows for these can be added to `scorecard-data.json`.
- **Where to start.** The highest-value fix is Live-proven, because most features sit at 0 to 55% there. Run the live checklists: [live-checklist-2026-10-27.md](live-checklist-2026-10-27.md) and `docs/testing/work-cards/`. Fixing the wake word's real recall is the next biggest gain, since every voice feature depends on it.
