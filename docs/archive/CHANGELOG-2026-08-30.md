> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — Major Update: Rich Repo Analysis, Live Glass Sidebar, STT/TTS Pipeline, RAM Optimization

**Date:** 2026-08-30
**Scope:** Full-stack changes across Worker (backend), Tauri/Rust (desktop client), and React/TypeScript (frontend)

---

## Table of Contents

1. [Rich Repository Analysis Dashboard](#1-rich-repository-analysis-dashboard)
2. [Live Liquid-Glass Sidebar Blur](#2-live-liquid-glass-sidebar-blur)
3. [STT Pipeline Fixes](#3-stt-pipeline-fixes)
4. [TTS Deep Research & Provider Support](#4-tts-deep-research--provider-support)
5. [RAM Optimization — Lazy Windows & Lazy STT](#5-ram-optimization--lazy-windows--lazy-stt)
6. [Wake Word Model & Mic Silence Recovery](#6-wake-word-model--mic-silence-recovery)
7. [GitHub OAuth & Token Management](#7-github-oauth--token-management)
8. [Architecture Mapper — Phase 1 Latency Optimization](#8-architecture-mapper--phase-1-latency-optimization)
9. [NEXUS CLI](#9-nexus-cli)
10. [Connection Diagnostics](#10-connection-diagnostics)
11. [Sidebar UI Redesign](#11-sidebar-ui-redesign)
12. [File Change Summary](#12-file-change-summary)

---

## 1. Rich Repository Analysis Dashboard

### Overview

Replaced the old 5-line spoken summary with a full structured analysis
dashboard rendered in the NEXUS sidebar. The user says `analyse zync` or
`analyse owner/repo`, and the sidebar displays a rich dashboard with pie
charts, language breakdowns, framework detection, database detection,
features, architecture, and CI/Docker status.

### Worker-Side Changes (`server/worker/src/index.ts`)

**`handleFastAnalyse()` — complete rewrite:**

- Parses `analyse owner/repo` and shorthand `analyse repo`.
- Resolves shorthand names using the authenticated user's repository list.
- Fetches repository metadata via GitHub REST API (`GET /repos/{owner}/{repo}`).
- Fetches language breakdown (`GET /repos/{owner}/{repo}/languages`).
- Fetches the full Git tree for file inventory and key file detection.
- Fetches key manifest/config files: `README.md`, `package.json`,
  `Cargo.toml`, `pyproject.toml`, `requirements.txt`, `go.mod`,
  `tsconfig.json`, `vite.config.ts`, `Dockerfile`, `docker-compose.yml`,
  `.github/workflows/ci.yml`, and more.
- Detects:
  - **Languages** — from GitHub's `/languages` API with byte counts and percentages.
  - **Frameworks** — from package manifests (React, Next.js, Vue, Svelte,
    Express, FastAPI, Flask, Django, Tauri, Axum, Actix, Tailwind, Mantine,
    Prisma, Jest, Vitest, etc.).
  - **Build tools** — Vite, Webpack, Cargo, pip, go build, etc.
  - **Databases** — Prisma, MongoDB, PostgreSQL, Redis, SQLite, Supabase
    (detected from config files, Docker Compose, and package dependencies).
  - **Tests** — presence of test directories and test frameworks.
  - **CI** — GitHub Actions workflows.
  - **Docker** — Dockerfile and docker-compose.yml detection.
- Extracts features from README headings and GitHub topics.
- Generates a natural spoken summary via `@cf/mistral/mistral-small-3.1-24b-instruct`
  (mistral was chosen over GLM-4.7-flash because GLM leaked reasoning text
  into the output; mistral produces clean direct responses).
- Returns a structured `analysis` object in the Worker response alongside
  the spoken `reply_text`.

**Response shape:**

```json
{
  "request_id": "...",
  "reply_text": "Ok sir, zync-meet/Zync is a TypeScript repository...",
  "intent": "fast_analyse",
  "analysis": {
    "repo": "zync-meet/Zync",
    "visibility": "public",
    "description": "...",
    "stars": 17,
    "forks": 1,
    "totalFiles": 623,
    "defaultBranch": "main",
    "languages": [
      { "name": "TypeScript", "bytes": 123456, "percentage": 62.6 },
      ...
    ],
    "frameworks": [
      { "name": "React", "category": "frontend" },
      { "name": "Tailwind CSS", "category": "styling" },
      ...
    ],
    "databases": [
      { "name": "Prisma", "evidence": "prisma/schema.prisma" },
      ...
    ],
    "features": ["collaboration", "realtime", "yjs", ...],
    "tests": true,
    "ci": "GitHub Actions",
    "docker": true,
    "architecture": "Frontend (React) + Database (Prisma)"
  }
}
```

**Token expiry handling:**

- `getValidGithubToken()` supports both classic non-expiring tokens and
  GitHub App-style expiring tokens with refresh.
- 401 responses produce actionable spoken messages instructing the user to
  reconnect GitHub via the setup wizard.
- Public repositories fall back to unauthenticated API requests when the
  token is invalid.

### Frontend Changes

**New files:**

- `frontend/src/sidebar/AnalysisDashboard.tsx` — renders the full dashboard:
  - Overview card (visibility, stars, forks, files, default branch)
  - Description
  - Languages section with half-donut chart
  - Frameworks section with equal-slice donut chart
  - Databases section with evidence files
  - Features bullet list
  - Architecture summary
  - Quality & CI grid (tests, CI, Docker status)

- `frontend/src/sidebar/Charts.tsx` — two chart components:
  - `LanguageChart` — half-donut (semi-circle) pie chart using
    `react-minimal-pie-chart`, with GitHub-style language colors and a
    legend showing language names and percentages.
  - `FrameworkChart` — full donut with equal slices (frameworks don't have
    natural byte-based percentages), color-coded by category (frontend,
    backend, styling, testing, build, database, etc.).

**Modified files:**

- `frontend/src/sidebar/sidebarStore.ts` — added `RepoAnalysis` type,
  `analysisData` state field, and `showAnalysis()` action.
- `frontend/src/sidebar/SidebarApp.tsx`:
  - Removed the temporary text input bar (was a mic workaround).
  - Removed the X close button (closing via Esc / Ctrl+Shift+Space only).
  - Added a formatted heading in the top bar (e.g. `Analysis: zync-meet/Zync`).
  - Renders `AnalysisDashboard` when `analysisData` is present, otherwise
    falls back to markdown rendering.
  - Listens for `sidebar:analysis` event for structured data from the Worker.
  - Fetches pending content (including analysis) on mount via
    `get_pending_sidebar_content`.

### Rust Network Layer (`src-tauri/src/network.rs`)

- Extended `ServerEvent` struct with an optional `analysis: Option<serde_json::Value>` field.
- Added `result_with_analysis()` constructor.
- The response handler now checks for `data["analysis"]` and emits it
  alongside the reply text.

### Rust Commands (`src-tauri/src/commands.rs`)

- Added `show_sidebar_with_analysis` Tauri command — like
  `show_sidebar_with_content` but also stores the analysis JSON in the
  pending content static, so the sidebar can render the dashboard
  race-free even on fresh window creation.
- Extended `PendingSidebar` struct with an `analysis` field.
- `get_pending_sidebar_content` now returns the analysis data too.

### Dependency

- `react-minimal-pie-chart` added to `frontend/package.json`.

### Testing

- `analyse zync` → resolves to `zync-meet/Zync`, returns full structured
  analysis in ~4 seconds.
- Language percentages match GitHub's `/languages` API.
- Frameworks detected from `package.json` dependencies.
- Databases detected from Prisma schema and Docker Compose.
- Features extracted from README headings and GitHub topics.
- Architecture summary generated by the LLM.

---

## 2. Live Liquid-Glass Sidebar Blur

### Problem

The sidebar uses a "fake blur" approach because native DWM Acrylic/Mica
cannot render on non-activating windows (see AGENTS.md for full details).
The original implementation took a single screenshot before the window
appeared, which didn't update when windows moved behind the sidebar.

A 200ms (5 FPS) live capture loop was added to fix this, but it created a
"buffering video" effect — each frame required the full
capture→blur→JPEG→base64→event→repaint pipeline (~50-100ms), making 5 FPS
look stuttery rather than smooth like real glass.

### Solution: 1 FPS + Change Detection + CSS Crossfade

**`src-tauri/src/sidebar_backdrop.rs`:**

- `frame_hash(bgra: &[u8]) -> u64` — lightweight rolling hash that samples
  every 16th byte of the captured BGRA buffer. Cost: ~1ms. Used to detect
  whether the background actually changed before running the expensive
  blur→encode pipeline.
- `capture_region_bgra_public()` — exposes the raw capture so the loop can
  hash first, then decide whether to blur.
- `blur_bgra_to_jpeg()` — blurs an already-captured BGRA buffer without
  re-capturing the screen (reuses bytes from the hash step).
- `capture_and_blur_jpeg()` — existing JPEG encoder (quality 60, fast).

**`src-tauri/src/commands.rs` (live loop in `show_sidebar_inner`):**

- Changed interval from 200ms → 1000ms (1 FPS).
- Added `LAST_FRAME_HASH` static `Mutex<Option<u64>>`.
- Each cycle: capture raw BGRA → hash → compare to previous → only run
  blur+emit if the hash differs.
- First frame after show always emits (hash reset to `None` on exit).
- When nothing moves behind the sidebar: zero blur/encode/emit — just a
  1ms hash check per second.
- `WDA_EXCLUDEFROMCAPTURE` flag prevents the hall-of-mirrors effect.

**`frontend/src/sidebar/sidebar.css`:**

- Moved the backdrop image from `.sidebar-card`'s `background-image` to a
  `::after` pseudo-element.
- Added `transition: opacity 0.3s ease-out` on `::after` for smooth
  crossfading between frames.
- Added `position: relative; z-index: 2` to `.sidebar-header`,
  `.sidebar-response`, and `.sidebar-footer` so content sits above the
  backdrop layer.

### Performance Impact

| Metric | Before (200ms loop) | After (1 FPS + hash) |
|--------|---------------------|----------------------|
| Capture frequency | 5/sec | 1/sec |
| CPU when idle | ~5-10% | ~0% (1ms hash only) |
| CPU when active | ~15-25% | ~5-10% (only on change) |
| Visual smoothness | Stuttery/buffering | Gentle crossfade |

---

## 3. STT Pipeline Fixes

### Root cause of "didn't catch that sir"

Four bugs were found and fixed in the STT (Speech-to-Text) pipeline:

1. **`lazy_stt.rs` path bug:** `stt_script_path()` was missing one
   `.parent()` level. It looked for `src-tauri/server/stt_server.py` but
   the file is at `ULTRON/server/stt_server.py`. Fixed by adding the
   correct path traversal.

2. **`ensure_stt_running()` not called on hotkey:** Only the wake-word
   path called `ensure_stt_running()`. The hotkey handler in `hotkey.rs`
   and the `transcribe_audio` command in `stt.rs` did not. Fixed by adding
   `ensure_stt_running()` calls to both.

3. **`is_stt_responsive()` used tokio runtime:** The health check used
   `tokio::runtime::Handle::try_current()` which fails on non-tokio
   threads. Fixed by using a raw TCP connection instead.

4. **STT idle timeout too aggressive:** 60 seconds. The server was killed
   before the user could speak again. Increased to 5 minutes.

### Whisper hallucination filter (`stt.rs`)

Added a hallucination filter in `transcribe_audio` that catches common
faster-whisper tiny.en hallucinations on noisy/silent audio:
- "thank you for watching", "you", "bye", "okay", etc.
- Text with < 2 alphabetic characters
- Filtered text is replaced with empty string, triggering retry logic.

**Files changed:** `src-tauri/src/lazy_stt.rs`, `src-tauri/src/stt.rs`,
`src-tauri/src/hotkey.rs`

---

## 4. TTS Deep Research & Provider Support

### Research

Researched and documented multiple TTS providers:
- Web Speech API (browser fallback, no API key needed)
- Google Cloud TTS (default provider, voice: `algenib`)
- Fish Audio
- ElevenLabs
- Kokoro
- Piper

### Implementation (`frontend/src/audio/ttsPlayer.ts`)

- Multi-provider TTS player with automatic fallback.
- Google Cloud TTS as default (configured via `google_cloud_api_key` in
  settings).
- Web Speech API as universal fallback.
- Voice selection and provider configuration in settings UI.
- TTS control button in the sidebar header (play/stop reading aloud).

**Files changed:** `frontend/src/audio/ttsPlayer.ts`,
`src-tauri/src/commands.rs` (added `google_cloud_api_key`),
`frontend/src/settings/SettingsApp.tsx`

**Documentation:** `docs/features/24-tts-deep-research-all-providers.md`

---

## 5. RAM Optimization — Lazy Windows & Lazy STT

### Result: 77% RAM reduction (1,644 MB → 385 MB idle)

### Lazy Window Creation (`src-tauri/src/dyn_windows.rs`)

- Only the `main` (orb) window is created at startup in `tauri.conf.json`.
- `setup`, `settings`, `sidebar`, `architect` windows are created on-demand
  by `dyn_windows::get_or_create_window()` when first needed.
- `hide_sidebar` / `close_setup_window` / `close_settings_window` now
  **destroy** the window (not `hide()`) — kills the WebView2 process tree
  and frees ~250 MB per window.
- Platform effects (DWM corners, macOS vibrancy) applied at creation time.

### Lazy STT Server (`src-tauri/src/lazy_stt.rs`)

- STT server (faster-whisper tiny.en) is NOT started at boot.
- `lazy_stt::ensure_stt_running()` is called when the wake word fires.
- `lazy_stt::mark_stt_request()` resets the idle timer on each transcription.
- A background thread kills the STT server after 5 minutes of no requests.
- If an external STT server is already running on port 39217, the lazy
  manager detects it and skips spawning its own.

### Measured RAM

| Component          | Before   | After    |
|--------------------|----------|----------|
| NEXUS.exe (Rust)   | 47.9 MB  | 40.8 MB  |
| WebView2 (1 window)| 870 MB   | 344 MB   |
| STT server         | 339 MB   | 0 MB     |
| **TOTAL**          | **1,644 MB** | **385 MB** |

**Files changed:** `src-tauri/tauri.conf.json`, `src-tauri/src/dyn_windows.rs`
(new), `src-tauri/src/lazy_stt.rs` (new), `src-tauri/src/lib.rs`,
`src-tauri/src/commands.rs`, `src-tauri/src/architect.rs`,
`src-tauri/src/hotkey.rs`, `src-tauri/src/tray.rs`,
`src-tauri/src/wakeword_oww.rs`, `src-tauri/src/stt.rs`, `scripts/run.ps1`

---

## 6. Wake Word Model & Mic Silence Recovery

### Wake Word Model (`src-tauri/resources/oww/nexus.onnx`)

- Custom-trained ONNX wake word model for "NEXUS".
- Tested with the exact Rust pipeline (mel → normalize → slice[4:80] →
  embedding → classifier).
- 100% recall on TTS samples, 0% false positives on 20 negative samples.
- The model is NOT the problem — the Intel SST mic driver is.

### Mic Silence Recovery (`src-tauri/src/wakeword_oww.rs`)

Added a background thread that monitors the audio callback counter and
automatically restarts the cpal stream when the mic goes silent (Intel
Smart Sound Technology driver bug).

**Settings (tuned for Intel SST bursty audio):**
- Poll interval: 5s
- Silence threshold: 165 callbacks (~5s)
- Restart method: `try_device_silent` (no 5s probe)
- Nuclear option: every 12 restarts (~60s of silence), restarts the
  Windows Audio service (`net stop/start Audiosrv`)
- Confirmation RMS threshold lowered from 0.01 to 0.002

**Files changed:** `src-tauri/src/wakeword_oww.rs`,
`src-tauri/resources/oww/nexus.onnx`

---

## 7. GitHub OAuth & Token Management

### Token refresh support (`server/worker/src/index.ts`)

- `getValidGithubToken()` supports both:
  - Classic OAuth-style non-expiring access tokens.
  - GitHub App-style expiring user tokens with refresh tokens.
- Automatic refresh when tokens are within the configured refresh buffer.
- Falls back to the old token if refresh fails.
- OAuth callback stores actual values (`access_token`, `refresh_token`,
  `expires_in`) rather than hardcoded `null`/`0`.

### 401 error handling

- `githubErrorMessage()` maps status codes to actionable spoken messages.
- Applied to all GitHub operations: PR reading/listing/analysis/merge/
  approve/close/comment, issue creation/closure, PR creation, fast
  repository analysis.
- Public repositories continue to work through unauthenticated fallback.
- Private repositories produce a reconnect message.

**Example:** "Your GitHub token has expired or been revoked, sir. Please
reconnect GitHub in the NEXUS setup wizard..."

---

## 8. Architecture Mapper — Phase 1 Latency Optimization

Phase 1 now uses a hybrid approach for 3-4s first response:

1. **Parallelized GitHub API calls** (`tokio::join!`): metadata + recursive
   tree fetched concurrently using the symbolic ref `HEAD`.
2. **Instant Rust heuristic clustering** for first paint (~5ms).
3. **Async LLM enrichment** (`enrich_phase1` command): after first paint,
   the client POSTs heuristic layers to the Worker's `phase1_enrich`
   intent. The LLM (Mistral 24B) rewrites generic labels into
   repo-specific ones. Result streams back via the
   `architect:phase1-enriched` event ~2-3s later and merges in-place.
   Never blocks first paint.

**Files changed:** `src-tauri/src/architect.rs`,
`src-tauri/src/network.rs`, `server/worker/src/index.ts`,
`frontend/src/architect/architectStore.ts`,
`frontend/src/architect/ArchitectApp.tsx`

---

## 9. NEXUS CLI

A command-line interface for NEXUS:

```
nexus start       Start NEXUS with unified console
nexus stop        Stop NEXUS and STT server
nexus status      Check if NEXUS is running
nexus logs        Tail NEXUS logs in real-time
nexus build       Rebuild NEXUS
nexus diagnostics Check all service connections
nexus help        Show help
```

Added to user PATH. Available from any terminal.

**Files:** `scripts/nexus.bat`, `scripts/nexus.cmd`,
`scripts/diagnostics.ps1`

---

## 10. Connection Diagnostics

`src-tauri/src/diagnostics.rs` — checks 5 services on startup:

| Service | Check method | Expected |
|---------|-------------|----------|
| STT | TCP connect to port 39217 + HTTP GET /health | OK if running |
| TTS | Check settings.json for provider keys | Always OK (Web Speech fallback) |
| Cloudflare Worker | HTTPS GET to /health | OK if reachable |
| GitHub | HTTPS GET to Worker /oauth/status | OK if OAuth connected |
| Google | HTTPS GET to Worker /oauth/status | OK if OAuth connected |

Available as: Tauri command `nexus_diagnostics`, CLI `nexus diagnostics`,
and auto-logged 5s after boot.

---

## 11. Sidebar UI Redesign

### Changes

- **Removed text input bar** — the sidebar is an analysis view, not a chat
  interface. The text input was a temporary mic workaround.
- **Removed X close button** — closing is via `Esc` or `Ctrl+Shift+Space`
  only.
- **Added formatted heading** in the top bar alongside the speaker button:
  - `analyse zync` → `Analysis: zync-meet/Zync`
  - `analyse PR 24 servx` → `Analysis: PR 24 Servx`
- **Speaker (TTS) button** retained in the top bar.
- **Rich markdown rendering** with GFM tables, syntax-highlighted code
  blocks, image lightbox, GitHub callout cards, and safe external links.
- **Font size adjustment** (sm/md/lg/xl).
- **Copy full response** button.
- **Scroll to top** floating button.

### CSS (`frontend/src/sidebar/sidebar.css`)

- 419 lines of new/modified CSS for the analysis dashboard, charts,
  language/framework legends, database lists, feature lists, activity
  grid, and the `::after` backdrop crossfade layer.
- Glass-morphism styling with lit bezel box-shadow stack.
- DWM corner rounding via `dwm_corners.rs` to match CSS border-radius.

---

## 12. File Change Summary

### New Files

| File | Purpose |
|------|---------|
| `frontend/src/sidebar/AnalysisDashboard.tsx` | Rich repo analysis dashboard component |
| `frontend/src/sidebar/Charts.tsx` | Half-donut language chart + framework donut chart |
| `src-tauri/src/dyn_windows.rs` | Lazy window creation/destruction (RAM optimization) |
| `src-tauri/src/lazy_stt.rs` | Lazy STT server manager |
| `src-tauri/src/sidebar_backdrop.rs` | Fake-blur backdrop capture + frame hashing |
| `src-tauri/src/diagnostics.rs` | Connection diagnostics for 5 services |
| `src-tauri/src/dwm_corners.rs` | DWM window corner rounding |
| `scripts/nexus.bat` / `scripts/nexus.cmd` | NEXUS CLI |
| `scripts/diagnostics.ps1` | Diagnostics CLI script |
| `docs/features/21-liquid-glass-sidebar.md` | Liquid glass sidebar docs |
| `docs/features/22-worker-ai-latency-optimization-plan.md` | Worker latency plan |
| `docs/features/23-tts-voice-research-elevenlabs-vs-fish.md` | TTS research |
| `docs/features/24-tts-deep-research-all-providers.md` | TTS deep research |
| `docs/features/25-rich-repo-analysis-dashboard.md` | Repo analysis dashboard plan |

### Modified Files

| File | Lines changed | Key changes |
|------|--------------|-------------|
| `server/worker/src/index.ts` | +1017 | Rich repo analysis, token refresh, 401 handling, phase1 enrich |
| `src-tauri/src/architect.rs` | +1078 | Parallel fetch, enrichment, dynamic windows |
| `src-tauri/src/commands.rs` | +427 | Lazy windows, show_sidebar_with_analysis, live blur loop |
| `frontend/src/sidebar/sidebar.css` | +419 | Analysis dashboard styles, backdrop crossfade, chart styles |
| `frontend/src/sidebar/SidebarApp.tsx` | +213 | Remove text input/X, add heading, render dashboard |
| `src-tauri/src/wakeword_oww.rs` | +292 | Silence recovery, confirmation threshold |
| `frontend/src/audio/ttsPlayer.ts` | +185 | Multi-provider TTS with fallback |
| `frontend/src/audio/recorder.ts` | +102 | Mic recording improvements |
| `frontend/src/audio/stt.ts` | +90 | STT improvements |
| `src-tauri/src/lib.rs` | +113 | Module registration, window config |
| `src-tauri/tauri.conf.json` | +76 | Removed 4 startup windows |
| `frontend/src/net/wsBridge.ts` | +64 | Module-level listener, analysis event handling |
| `src-tauri/src/network.rs` | +32 | ServerEvent analysis field, result_with_analysis |
| `frontend/src/sidebar/sidebarStore.ts` | +42 | RepoAnalysis type, showAnalysis action |
| `AGENTS.md` | +334 | Project documentation |
| Other files | ~300 | Various fixes and improvements |
| **Total** | **+4,361 / -670** | |

### Dependencies Added

- `react-minimal-pie-chart` (frontend) — lightweight SVG pie/donut charts
- `image` crate `jpeg` feature (Rust) — fast JPEG encoding for live blur
- `base64` crate (Rust) — base64 encoding for data URIs

---

## Architecture

```
User says "analyse zync"
        │
        ▼
  NEXUS desktop app (Rust/Tauri)
  ├── Wake word / hotkey triggers
  ├── Local microphone capture (cpal)
  ├── Local STT (faster-whisper tiny.en, lazy-started)
  └── Sends transcript text via HTTPS POST
        │
        ▼
  Cloudflare Worker (serverless backend)
  ├── Intent detection (llama-3.2-1b-instruct)
  ├── GitHub API (OAuth token, public/private repos)
  │   ├── Repository metadata
  │   ├── Language breakdown
  │   ├── Git tree + key files
  │   └── Framework/database/CI/Docker detection
  ├── LLM summary (mistral-small-3.1-24b-instruct)
  └── Returns { reply_text, analysis }
        │
        ▼
  NEXUS desktop app
  ├── Rust network.rs — parses response, emits assistant:server event
  ├── wsBridge.ts — receives event, calls show_sidebar_with_analysis
  ├── Sidebar renders AnalysisDashboard with pie charts
  └── Local TTS speaks the short summary
```

---

## Privacy Model

- Microphone audio stays local — only transcript text crosses the network.
- No server-generated audio returned — TTS is local.
- GitHub OAuth tokens stored in Cloudflare D1 (encrypted at rest).
- Token values never exposed in logs, UI, or summaries.
- `WDA_EXCLUDEFROMCAPTURE` prevents the sidebar from capturing itself.
