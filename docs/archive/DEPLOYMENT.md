> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS Deployment Guide

This guide covers deploying the server-side components (sidecar, STT, n8n, Ollama)
on a GPU server, and building the Tauri client for distribution.

## Server Components

### 1. STT (faster-whisper)

```bash
cd server
pip install faster-whisper fastapi uvicorn python-multipart
uvicorn stt_server:app --host 0.0.0.0 --port 8000
```

Environment:
- `WHISPER_MODEL=large-v3` (or `distil-large-v3` for lower VRAM)
- `WHISPER_DEVICE=cuda`
- `WHISPER_COMPUTE=int8_float16`

VRAM usage (11GB GPU):
- large-v3 + int8_float16: ~3.1GB
- distil-large-v3 + int8_float16: ~1.5GB
- Leaves ~8GB for Ollama

### 2. Ollama

```bash
# Install Ollama: https://ollama.ai
ollama pull qwen2.5:1.5b-instruct-q4_k_m   # intent router (~1GB)
ollama pull llama3.1:8b-instruct-q4_k_m    # heavy LLM (~5GB)
```

Ollama runs on port 11434 by default.

### 3. n8n

```bash
# Install n8n: https://docs.n8n.io/hosting/installation/
npm install -g n8n
n8n
```

n8n runs on port 5678 by default.

Import the blueprints:
1. Open n8n UI at http://localhost:5678
2. Import each blueprint from `server/n8n/`:
   - `master_supervisor.blueprint.json`
   - `email_summarize.blueprint.json`
   - `github_pr_check.blueprint.json`
   - `calendar_peek.blueprint.json`
   - `general_chat.blueprint.json`
3. Note the workflow IDs for each imported workflow
4. Update `master_supervisor.blueprint.json`:
   - Replace `EMAIL_SUMMARY_WF_ID` with the email.summarize workflow ID
   - Replace `GITHUB_PR_WF_ID` with the github.pr_check workflow ID
   - Replace `CALENDAR_WF_ID` with the calendar.peek workflow ID
5. Re-import the updated master supervisor

### 4. Sidecar

```bash
cd server/sidecar
pip install -r requirements.txt
cp env.example .env
# Edit .env with your values
uvicorn sidecar:app --host 0.0.0.0 --port 8443
```

Required environment variables (see `env.example`):
- `ELEVENLABS_API_KEY` + `ELEVENLABS_VOICE_ID` — TTS
- `GOOGLE_CLIENT_ID` + `GOOGLE_CLIENT_SECRET` — Google OAuth
- `GITHUB_CLIENT_ID` + `GITHUB_CLIENT_SECRET` — GitHub OAuth
- `STT_URL` — faster-whisper endpoint
- `N8N_SUPERVISOR_URL` — n8n webhook URL

### 5. Production: Caddy reverse proxy (TLS)

```Caddyfile
NEXUS.yourdomain.com {
    reverse_proxy localhost:8443
}
```

This terminates TLS and proxies to the sidecar on localhost:8443.

## Client Build

### Prerequisites

- Node.js 20+
- Rust (stable)
- Platform-specific deps (see Tauri docs)

### Build

```bash
# Windows (NSIS installer)
pwsh ./scripts/build.ps1 -Bundles nsis

# macOS (DMG)
pwsh ./scripts/build.ps1 -Bundles dmg

# macOS ARM (DMG for Apple Silicon)
pwsh ./scripts/build.ps1 -Target aarch64-apple-darwin -Bundles dmg

# Linux (AppImage + deb)
pwsh ./scripts/build.ps1 -Bundles "appimage,deb"
```

Artifacts are in `src-tauri/target/release/bundle/`.

### CI/CD

The GitHub Actions workflow (`.github/workflows/release.yml`) builds for all
platforms on tag push (`v*`). Set these secrets in GitHub:

- `TAURI_SIGNING_PRIVATE_KEY` + `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` — Windows signing
- `APPLE_CERTIFICATE` + `APPLE_CERTIFICATE_PASSWORD` — macOS signing
- `APPLE_ID` + `APPLE_PASSWORD` + `APPLE_TEAM_ID` — macOS notarization

The CI workflow (`.github/workflows/ci.yml`) runs on every PR:
- TypeScript check + frontend build
- Cargo check (default + mock-wake features)
- Python parse check for sidecar modules
- JSON validation for n8n blueprints

## Manual Setup (Google + GitHub OAuth)

### Google Cloud Console

1. Create a project at https://console.cloud.google.com
2. Enable APIs: Gmail, Google Calendar, Google Drive
3. Configure OAuth consent screen (External, add your scopes)
4. Create OAuth client (Web application)
5. Add redirect URI: `NEXUS://oauth/callback`
6. Copy Client ID + Client Secret to sidecar `.env`

### GitHub OAuth App

1. Go to https://github.com/settings/developers → OAuth Apps → New OAuth App
2. Authorization callback URL: `NEXUS://oauth/callback`
3. Copy Client ID + Client Secret to sidecar `.env`

### ElevenLabs

1. Create account at https://elevenlabs.io
2. Get API key from Settings
3. Get Voice ID from the Voice Lab
4. Set `ELEVENLABS_API_KEY` and `ELEVENLABS_VOICE_ID` in sidecar `.env`
