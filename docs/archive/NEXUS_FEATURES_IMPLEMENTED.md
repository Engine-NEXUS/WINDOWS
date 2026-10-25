> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS Assistant — Complete Feature Implementation Log

This document details every feature implemented, bug fixed, and architecture
decision made across the `prem224k` and `prem22k` branches, forming the basis
of PR #7 (merge of both branches into `main`).

**Date:** 2026-08-29
**Branches merged:** `prem224k` + `prem22k` → `main`
**PR:** #7

---

## Table of Contents

1. [GitHub PR Analysis via Voice](#1-github-pr-analysis-via-voice)
2. [STT Mishearing Fixes (Post-Processing + Hotwords)](#2-stt-mishearing-fixes)
3. [Sidebar Streaming Text Animation](#3-sidebar-streaming-text-animation)
4. [On-It-Sir → Here-Is-The-Analysis Flow](#4-on-it-sir--here-is-the-analysis-flow)
5. [Worker Fuzzy Repo Name Matching](#5-worker-fuzzy-repo-name-matching)
6. [Wake-Word Reliability (Single-Frame High-Confidence)](#6-wake-word-reliability)
7. [State-Dependent Hotkey (Sidebar-Aware)](#7-state-dependent-hotkey)
8. [CSP for Silero VAD CDN](#8-csp-for-silero-vad-cdn)
9. [Multi-Voice TTS Engine (prem22k)](#9-multi-voice-tts-engine)
10. [Non-Activating Floating Overlay (prem22k)](#10-non-activating-floating-overlay)
11. [Linux D-Bus MPRIS Media Control (prem22k)](#11-linux-d-bus-mpris-media-control)
12. [Dual-Phase VAD Post-TTS Mute Gate (prem22k)](#12-dual-phase-vad-post-tts-mute-gate)
13. [GitHub Actions CI/CD (prem22k)](#13-github-actions-cicd)
14. [Setup Wizard Redesign (prem22k)](#14-setup-wizard-redesign)
15. [Architecture Decisions](#15-architecture-decisions)
16. [Conflict Resolution Strategy](#16-conflict-resolution-strategy)

---

## 1. GitHub PR Analysis via Voice

### Problem
The user wanted to say "analyse PR 5 in servx" and have NEXUS:
1. Fetch the PR from GitHub using stored OAuth credentials
2. Send the PR context (files, diffs, commits, reviews) to Cloudflare GLM-4.7-Flash
3. Return a senior-engineer code review
4. Show it in the sidebar (not speak the entire long response)
5. Speak only "Here is the analysis, sir"

### Implementation

**Worker side** (`server/worker/src/index.ts`):
- `handleGitHubAnalyse()` (lines 555–664): Complete PR analysis pipeline
- `parsePRRequest()`: Extracts PR number and repo name from transcript
  - Supports: `PR 24`, `PR #24`, `pull request 24`, `in servx`, `of servx`, `from servx`
- `resolveRepo()`: Resolves short repo names (e.g. `servx`) against the user's
  authenticated GitHub repositories (queries up to 100 repos sorted by updated)
- `fetchPRContext()`: Fetches PR metadata, changed files/diffs, commits,
  inline comments, and reviews via GitHub REST API
- Model selection:
  - Default: `@cf/zai-org/glm-4.7-flash` (fast, normal context)
  - Deep: `@cf/zai-org/glm-5.3-flash` (re-evaluations or >520K char context)
- Review prompt covers: Summary, Risk Assessment, Code Quality, Suggestions,
  Verdict, file names + line numbers, edge cases, error handling, test coverage
  gaps, security implications

**Client side** (`frontend/src/net/wsBridge.ts`):
- `shouldShowSidebar()`: Decides whether a response warrants the sidebar
  - Gate 1: Response length >= 80 chars
  - Gate 2: Not a local-command verb (open/close/play)
  - Gate 3: Contains info/research intent keyword (analyse, review, PR, etc.)
- Result handler: If `showSidebar=true`, invokes `show_sidebar_with_content`
  IPC command and speaks only "Here is the analysis, sir"

**Rust side** (`src-tauri/src/commands.rs`):
- `show_sidebar_with_content()`: Shows sidebar window AND directly injects
  content into the sidebar WebView DOM via `eval()` (more reliable than
  cross-window Tauri events)

### Testing Results
- `analyse PR 1 in servx` → 5,184-char analysis
- `analyse PR 5 in servx` → 12,817-char analysis
- `analyse the PR in servx` (no number) → 12,515-char analysis (latest PR)
- Invalid PR → useful not-found error
- Voice test: Full flow works end-to-end

---

## 2. STT Mishearing Fixes

### Problem
The local STT server uses `tiny.en` (39M params, fastest model) which
struggles with brand names and technical terms:
- "analyse" → "unless", "analyze", "and let's"
- "PR 5" → "pf5", "p r 5", "pe5"
- "servx" → "cervix", "service", "weeks", "serve x"

### Implementation

**STT server** (`server/stt_server.py`):
- Added `initial_prompt` to every transcription call:
  ```
  "The user is giving voice commands to a desktop assistant.
   Common commands include: analyse PR 5 in servx, review PR 3 in servx...
   Recognised names: servx, NEXUS, ULTRON, github, gmail."
  ```
  This biases the Whisper decoder toward expected vocabulary.
- Dynamic hotwords file (`%APPDATA%\com.nexus.assistant\stt_hotwords.txt`):
  - Hot-reloaded on every transcription request (no restart needed)
  - NEXUS writes the user's GitHub repo names here so Whisper recognises them
  - Built-in hotwords include: servx, NEXUS, ULTRON, gmail, github, etc.

**Frontend post-processing** (`frontend/src/audio/recorder.ts`):
- `correctSttTranscript()` function applies regex corrections:
  - `unless` → `analyse` (common mishearing)
  - `analyze` → `analyse` (American → British)
  - `pf5` / `p r 5` / `pe5` / `pr5` → `PR 5`
  - `cervix` / `service` / `weeks` / `serve x` → `servx` (when preceded by in/of/from)
- Applied in both `finishCapture()` and `finishCaptureFromVad()` right after
  STT returns the transcript, before intent parsing

### Testing Results
- STT heard "unless PR 5 in servx" → corrected to "analyse PR 5 in servx" → full analysis
- STT heard "analyze PR 5 in servx." → corrected to "analyse PR 5 in servx." → full analysis
- STT heard "Analyze PR 5 in servx like this" → corrected → full analysis

---

## 3. Sidebar Streaming Text Animation

### Problem
The user wanted text to appear in the sidebar like ChatGPT/Gemini:
- Words fade in sequentially from top to bottom
- Left-to-right flow within each line
- Natural word spacing (no uneven gaps)
- Line breaks preserved
- Auto-scroll while text appears

### Implementation

**Rust** (`src-tauri/src/commands.rs` — `show_sidebar_with_content()`):
- Splits response by line (`\n`)
- Each line split by whitespace
- Empty words skipped
- Each word becomes a `.word` span with:
  - The trailing space INSIDE the span (except the last word in each line)
  - `display: inline` (NOT `inline-block` — that caused spacing issues)
- `<br>` elements preserve newlines
- Animation delays staggered ~28ms per word, capped at 2000ms
- Timer scrolls the response container every 50ms
- Scrolling stops after the animation window

**CSS** (`frontend/src/sidebar/sidebar.css`):
```css
.word {
  opacity: 0;
  animation: wordFadeIn 0.4s ease forwards;
}
@keyframes wordFadeIn {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: translateY(0); }
}
```

### Bug Fixed: Uneven Spacing
- Initial implementation used `display: inline-block` which broke natural text flow
- Fix: Removed `inline-block`, kept words inline, put trailing space inside each span
- Live test confirmed: "Perfect spacing"

---

## 4. On-It-Sir → Here-Is-The-Analysis Flow

### Problem
PR analysis takes 10-20 seconds (GLM model inference). The user wanted:
1. Immediate "On it sir" acknowledgement when the command is recognised
2. Orb disappears (no awkward waiting state)
3. When the result arrives: orb reappears briefly, says "Here is the analysis sir"
4. Sidebar shows the full PR review with streaming animation
5. Orb auto-closes after the short confirmation

### Implementation

**Detection** (`frontend/src/audio/recorder.ts`):
```typescript
function isLongRunningQuery(transcript: string): boolean {
  const hasAnalyse = /\b(analy[sz]e|review|deep\s*dive|critique|evaluate|...)\b/.test(t);
  const hasPR = /\b(pr|pull\s*request)\b/.test(t);
  const hasPRNumber = /\bpr\s*#?\s*\d+\b/.test(t);
  return (hasAnalyse && (hasPR || hasRepo)) || hasPRNumber;
}
```

**Acknowledgement** (`frontend/src/audio/recorder.ts`):
```typescript
async function ackLongRunningQuery(): Promise<void> {
  store.setState("speaking");
  store.addAssistantMessage("On it sir.");
  await speak("On it sir");
  store.setVisible(false);  // Hide orb
  store.setState("thinking");
}
```

**Result handler** (`frontend/src/net/wsBridge.ts`):
- When result arrives: `store.setVisible(true)` (show orb briefly)
- Speak "Here is the analysis, sir"
- After TTS completes: `store.setVisible(false)` (auto-close orb)
- Sidebar stays visible until dismissed via hotkey

**Critical fix**: `captureInProgress` was set to `false` only AFTER
`sendTranscript` returned (which blocks 10-20s). During this time, all new
voice commands were silently skipped. Fix: Set `captureInProgress = false`
BEFORE calling `sendTranscript`.

---

## 5. Worker Fuzzy Repo Name Matching

### Problem
STT mishears repo names ("servx" → "service", "cervix"). The Worker's
`resolveRepo()` only did exact and substring matching, so fuzzy mishearings
returned "repository not found".

### Implementation

**Worker** (`server/worker/src/index.ts` — `resolveRepo()`):
- Added Levenshtein distance-based fuzzy matching
- Three-tier matching:
  1. Exact match (case-insensitive)
  2. Substring match (either direction)
  3. Fuzzy match: Levenshtein distance / max_length < 0.6
     - Prefix bonus: If first 3 chars match, score × 0.5
     - Length filter: Skip repos too different in length
- `levenshtein()` function: Standard DP implementation

### Testing Results
- "service" → "servx" (fuzzy match, 4162-char analysis generated)
- "weeks" → not matched (too different, 4/5 chars different — expected)
- "PR 5 in service" (no "analyse" keyword) → intent classifier catches it

---

## 6. Wake-Word Reliability

### Problem (prem224k)
The openWakeWord model (78.6% accuracy, 58.2% recall) sometimes produces a
single high-confidence detection (e.g. 0.89) but the adjacent frames are below
threshold. The 2-frame smoothing requirement (`MIN_POSITIVE_DETECTIONS = 2.0`)
silently discarded these valid detections.

### Implementation (`src-tauri/src/wakeword_oww.rs`)
- Added `SINGLE_FRAME_HIGH_CONFIDENCE = 0.5` constant
- Two trigger paths in `calculate_average()`:
  1. **High-confidence single frame**: If any frame >= 0.5, return it immediately
     - 0.5 is above the 0.45 trigger threshold and far above noise
     - The silence gate already blocks RMS < 0.0005
     - The model produces <0.01 on non-wake speech
  2. **Smoothed multi-frame**: Otherwise, require 2+ frames above threshold
     and return their average (filters transient noise spikes)
- Logging: `wake: high-confidence single-frame trigger (avg=X, prob=Y)`

### Why prem224k's approach is better than prem22k's
- prem22k: Lowers `MIN_POSITIVE_DETECTIONS` from 2.0 to 1.0 (ALL single frames
  above threshold trigger — more false wakes from noise)
- prem224k: Keeps 2.0 but adds a high-confidence bypass (only 0.5+ single
  frames trigger instantly — precise, low false-positive rate)

---

## 7. State-Dependent Hotkey

### Problem (prem224k)
The original hotkey always did both: dismiss sidebar + wake NEXUS. This meant
pressing the hotkey to close the sidebar also woke NEXUS, which was
unintuitive.

### Implementation (`src-tauri/src/hotkey.rs`)
- Check if sidebar is currently visible (`window.is_visible()`)
- If sidebar visible → close sidebar only (do NOT wake NEXUS)
- If sidebar hidden → wake NEXUS (do NOT touch sidebar)
- Result: Press hotkey twice (with sidebar visible) to first close sidebar,
  then wake NEXUS on the second press

### Merge with prem22k's multi-hotkey
prem22k adds multiple hotkeys (Ctrl+Shift+Space, Ctrl+Alt+Space, Alt+Space)
for Linux GNOME Wayland compatibility. The merge combines both:
- Multiple hotkeys registered
- Each hotkey has state-dependent behavior

---

## 8. CSP for Silero VAD CDN

### Problem (prem224k)
Silero VAD loads its WASM model from `cdn.jsdelivr.net`. The original CSP
didn't allow this, causing CSP violations and VAD failures.

### Implementation (`src-tauri/tauri.conf.json`)
```json
"csp": "default-src 'self'; connect-src 'self' wss: https: ipc: http://ipc.localhost;
  media-src 'self' blob: data:; img-src 'self' data: blob:;
  script-src 'self' 'unsafe-inline' 'unsafe-eval' https://cdn.jsdelivr.net;
  style-src 'self' 'unsafe-inline'; font-src 'self' data:;
  worker-src 'self' blob:;"
```
- Added `'unsafe-eval'` and `https://cdn.jsdelivr.net` to `script-src`
- Added `worker-src 'self' blob:` for VAD Web Worker

---

## 9. Multi-Voice TTS Engine (prem22k)

### Implementation (`frontend/src/audio/ttsPlayer.ts`)
- 6 curated voices:
  - **Gemini Flash** (Google AI) — `gemini-3.1-flash-tts-preview`
  - **Ethan** (Fish Audio) — `s2.1-pro` model
  - **Jarvis** (ElevenLabs) — British male, executive assistant
  - **Nova** (ElevenLabs) — American female, conversational
  - **Echo** (ElevenLabs) — Australian male, tech companion
  - **Onyx** (ElevenLabs) — Deep baritone, commanding
- Multi-tier fallback: Gemini → Fish Audio → WebSpeech
- `CURATED_VOICES` array with voice preview support
- Settings: `tts_provider`, `elevenlabs_api_key`, `fish_audio_api_key`,
  `gemini_api_key` in `NexusSettings` (Rust) with env var fallback

### Privacy note
Gemini STT (3.5 Transcribe) sends audio to Google. This is kept as an opt-in
fallback only — local faster-whisper remains the default to preserve the
"audio never leaves the device" privacy model.

---

## 10. Non-Activating Floating Overlay (prem22k)

### Implementation (`src-tauri/src/window_manager.rs`)
- `configure_non_activating_overlay()`: Sets `always_on_top` + `focusable(false)`
- `position_orb()`: Positions orb at bottom-center, just above taskbar/dock
  - Platform-specific dock offsets: macOS 70px, Windows 48px, Linux 36px
- Impact: NEXUS wakes without stealing keyboard focus from IDEs/terminals

---

## 11. Linux D-Bus MPRIS Media Control (prem22k)

### Implementation (`src-tauri/src/mpris.rs`)
- Uses `zbus` 5.x for direct D-Bus session bus communication
- `send_mpris_command()`: PlayPause, Play, Pause, Next, Previous, Stop
- `send_native_notification()`: Desktop toast notifications
- Zero-latency, no sub-shell spawning
- Only compiled on Linux (`#[cfg(target_os = "linux")]`)

---

## 12. Dual-Phase VAD Post-TTS Mute Gate (prem22k)

### Problem
TTS output can echo back into the microphone and self-trigger the wake word,
creating infinite loops.

### Implementation
- 300ms mic mute after TTS finishes
- `last_tts_active` timer in `wakeword_oww.rs` chunk processing
- 300ms delay in frontend `main.tsx` `startListening()`
- Allows speaker hardware decay and room acoustic reflections to clear

---

## 13. GitHub Actions CI/CD (prem22k)

### Implementation (`.github/workflows/build-windows.yml`)
- Triggers on push to `prem22k` or `main`
- Runs on `windows-latest`
- Steps: checkout → setup Node 20 → install Rust → cache Cargo deps
  (swatinem/rust-cache) → npm install → npm run build → tauri-action
  (NSIS bundle) → upload artifact
- Uses `--bundles nsis --features custom-protocol`
- Output: `NEXUS_0.1.0_x64-setup.exe`

---

## 14. Setup Wizard Redesign (prem22k)

### Implementation (`frontend/src/setup/SetupApp.tsx`)
- 3-step wizard: Persona & Voice → Preferences → Accounts
- Voice persona selection with preview playback
- API key entry (ElevenLabs, Fish Audio, Gemini)
- Hotkey customization
- Wake word enable/disable toggle
- Autostart toggle
- Removed `framer-motion` dependency (replaced with CSS transitions)

---

## 15. Architecture Decisions

### Serverless architecture preserved
```
NEXUS laptop → HTTP POST → Cloudflare Worker → D1 + APIs → text response
```
- No sidecar, no n8n, no Ollama in the client path
- Audio stays local (faster-whisper on 127.0.0.1:39217)
- Only transcript text crosses the network

### Sidebar content delivery
- Original: Tauri events between separate WebView contexts (unreliable)
- Final: Rust directly evaluates JavaScript in the sidebar WebView DOM
  via `eval()` — sets query text, response HTML, visibility, scrolling

### `done` event timing
- Original: Rust emitted `done` immediately after result → frontend reset/TTS
  cancellation
- Final: `done` only on error/cancel paths. Normal flow: frontend completes
  reset/hide after TTS callback

### Intent classifier enhancement
- Added `PR <number>` + `in/of/from <something>` pattern → `github_analyse`
  even without the "analyse" keyword (handles STT mishearing "analyse" as
  "unless")

---

## 16. Conflict Resolution Strategy

### 3 files changed in both branches:

| File | prem22k approach | prem224k approach | Merge decision |
|---|---|---|---|
| `hotkey.rs` | Multi-hotkey + non-activating overlay | State-dependent (sidebar-aware) | **Both**: Multi-hotkey with state-dependent logic + non-activating overlay |
| `wakeword_oww.rs` | `MIN_POSITIVE_DETECTIONS = 1.0` | `SINGLE_FRAME_HIGH_CONFIDENCE = 0.5` | **prem224k**: Keep 2.0 threshold + high-confidence bypass (more precise) |
| `tauri.conf.json` | `visible: true` (Linux fix) | CSP for Silero VAD CDN | **Both**: visible:true + CSP changes (different lines) |

---

## Files Changed (Complete List)

### prem224k (this session's work):
- `server/worker/src/index.ts` — Fuzzy repo matching, intent classifier, PR analysis
- `server/stt_server.py` — Dynamic hotwords, initial_prompt
- `frontend/src/audio/recorder.ts` — STT correction, long-running query ack, captureInProgress fix
- `frontend/src/net/wsBridge.ts` — Sidebar result handler, orb visibility management
- `frontend/src/sidebar/sidebar.css` — Word fade-in animation
- `src-tauri/src/commands.rs` — show_sidebar_with_content (direct DOM injection)
- `src-tauri/src/network.rs` — HTTP timeout fix, error logging
- `src-tauri/src/hotkey.rs` — State-dependent hotkey
- `src-tauri/src/wakeword_oww.rs` — Single-frame high-confidence trigger
- `src-tauri/src/lib.rs` — Command registration
- `src-tauri/tauri.conf.json` — CSP for Silero VAD
- `frontend/tsconfig.json` — Exclude vitest tests from production build
- `.gitignore` — Training notebooks, research PDFs, CDP scripts

### prem22k (merged from branch):
- `frontend/src/audio/ttsPlayer.ts` — Multi-voice TTS engine (580 lines)
- `frontend/src/audio/stt.ts` — Gemini 3.5 Transcribe integration
- `frontend/src/setup/SetupApp.tsx` — Setup wizard redesign
- `frontend/src/settings/SettingsApp.tsx` — Settings UI for voice selection
- `src-tauri/src/mpris.rs` — Linux D-Bus MPRIS media control (new file)
- `src-tauri/src/window_manager.rs` — Non-activating overlay
- `src-tauri/src/command_executor.rs` — MPRIS command execution
- `.github/workflows/build-windows.yml` — CI/CD pipeline (new file)
- `src-tauri/installer/nexus-installer.nsi` — Auto-launch setup wizard
- `CHANGELOG_PREM22K.md` — Branch changelog
- `env.example` — Environment variable template
- `install.sh` / `scripts/build.sh` — Linux build scripts

---

## Test Results Summary

| Test | Result |
|---|---|
| Worker health check | OK |
| GitHub OAuth status | Connected, scopes: repo read:org workflow |
| servx repo resolution | Found via user's GitHub account |
| PR 1 in servx | 5,184-char analysis |
| PR 5 in servx | 12,817-char analysis |
| Latest PR (no number) | 12,515-char analysis |
| Fuzzy: "service" → "servx" | Matched, 4,162-char analysis |
| Fuzzy: "weeks" → "servx" | Not matched (expected — too different) |
| STT correction: "unless PR 5 in servx" | → "analyse PR 5 in servx" → full analysis |
| Voice: "On it sir" ack | Spoken immediately, orb hides |
| Voice: "Here is the analysis sir" | Spoken when result arrives, sidebar shows |
| Sidebar animation | Words fade in left-to-right, top-to-bottom |
| Sidebar spacing | Even, natural (no gaps) |
| Sidebar dismissal | Hotkey closes sidebar |
| Wake-word: single 0.5+ frame | Triggers immediately |
| captureInProgress fix | Subsequent commands work during Worker wait |
