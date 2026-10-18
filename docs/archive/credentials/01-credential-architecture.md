> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# NEXUS — Credential Architecture

> Where every secret lives, who reads it, and how it flows through the system.
> This is the master document for all credential-related concepts.

---

## The Three Credential Types

NEXUS deals with three fundamentally different kinds of credentials. Understanding the distinction is critical.

### 1. OAuth Tokens (Google, GitHub)

| Aspect | Detail |
|--------|--------|
| **What they are** | Access tokens + refresh tokens issued by Google/GitHub after user consent |
| **Purpose** | Let NEXUS read your Gmail, Calendar, Drive, GitHub repos, etc. on your behalf |
| **How obtained** | User clicks "Connect Google" in setup → browser login → redirect → token exchange |
| **Where stored** | Sidecar SQLite DB (`oauth_tokens` table), per-user, per-provider |
| **Encrypted at rest?** | No (OAuth tokens are already opaque; encryption is for API keys) |
| **Expires?** | Yes — Google access tokens expire in ~1 hour. Refresh tokens are long-lived. GitHub tokens don't expire by default. |
| **Who reads them** | Sidecar's `get_valid_credentials()` at request time → injected into n8n webhook payload |
| **Client secret** | Stored in sidecar `.env` file, **never** in the client app |

### 2. API Keys (Claude, Devin, Antigravity, YouTube Data API, etc.)

| Aspect | Detail |
|--------|--------|
| **What they are** | Simple string keys (`sk-...`, `AIza...`, `ghp_...`) for services that don't support OAuth |
| **Purpose** | Authenticate API calls to third-party services |
| **How obtained** | User pastes the key in setup page → POST to sidecar `/apikeys/add` |
| **Where stored** | Sidecar SQLite DB (`api_keys` table), per-user, per-provider |
| **Encrypted at rest?** | **Yes** — Fernet symmetric encryption (`NEXUS_ENCRYPTION_KEY` env var) |
| **Expires?** | Depends on the provider. Most don't expire until revoked. |
| **Who reads them** | Sidecar's `get_valid_credentials()` at request time → injected into n8n webhook payload |
| **Listed back to client?** | **No** — `/apikeys/list` returns only provider names, never the keys themselves |

### 3. Device Tokens

| Aspect | Detail |
|--------|--------|
| **What they are** | Per-device authentication tokens issued by the server |
| **Purpose** | Identify which device is making a request (for rate limiting, audit, revocation) |
| **How obtained** | Generated during first-run setup → POST to sidecar `/device/register` |
| **Where stored** | Sidecar SQLite DB (`user_devices` table) + client config (`nexus-config.json`) |
| **Encrypted at rest?** | No (the token is a simple identifier) |
| **Expires?** | No (until manually revoked) |
| **Who reads them** | Sent in the WSS `start` frame as `deviceId` |

---

## Where Secrets Live

```
┌─────────────────────────────────────────────────────────────────────┐
│  CLIENT (per device) — NO secrets stored here                       │
│                                                                     │
│  nexus-config.json (app data dir):                                  │
│    serverUrl: ws://127.0.0.1:49152/ws                               │
│    userId: local-user                                               │
│    deviceId: local-device                                           │
│                                                                     │
│  voice_profile.json (app data dir):                                 │
│    speaker embeddings (NOT a secret — biometric, but local only)    │
│                                                                     │
│  frontend/.env.local (build-time fallbacks only):                   │
│    VITE_SERVER_URL, VITE_DEVICE_TOKEN                               │
│    (These are NOT secrets — just config fallbacks)                  │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│  SIDECAR (server-side) — ALL secrets stored here                    │
│                                                                     │
│  .env file (server/sidecar/.env):                                   │
│    GOOGLE_CLIENT_ID=...                                             │
│    GOOGLE_CLIENT_SECRET=...                                         │
│    GITHUB_CLIENT_ID=...                                             │
│    GITHUB_CLIENT_SECRET=...                                         │
│    NEXUS_ENCRYPTION_KEY=...  (Fernet key for API key encryption)    │
│    N8N_SUPERVISOR_URL=http://localhost:5678/webhook/supervisor      │
│    N8N_API_TOKEN=...                                                │
│    NEXUS_SIDECAR_TOKEN=...  (optional WSS auth)                     │
│    YOUTUBE_API_KEY=...                                              │
│    CUSTOM_SEARCH_API_KEY=...                                        │
│    MAPS_API_KEY=...                                                 │
│    TRANSLATE_API_KEY=...                                            │
│    SEARCH_ENGINE_ID=...                                             │
│                                                                     │
│  SQLite DB (NEXUS_credentials.db):                                  │
│    oauth_tokens(user_id, provider, access_token, refresh_token,     │
│                expires_at, scopes, created_at)                      │
│    api_keys(user_id, provider, key_encrypted, created_at)           │
│    user_devices(user_id, device_id, device_token, created_at)       │
└─────────────────────────────────────────────────────────────────────┘
```

**The client never sees the client secret, API keys, or OAuth tokens.** It only sees "connected: true/false" status.

---

## The Credential Flow at Request Time

```
1. User says "summarize my email"
2. Local STT → transcript: "summarize my email"
3. WSS to sidecar: {type:"transcript", data:"summarize my email"}
4. Sidecar calls get_valid_credentials(user_id):
   a. Load OAuth tokens from SQLite
   b. If Google token expired → refresh using refresh_token
   c. Load API keys from SQLite (Fernet decrypt)
   d. Return: {google: {access_token, scopes}, github: {...}, api_keys: {youtube: "AIza...", ...}}
5. Sidecar calls n8n supervisor:
   POST /supervisor {
     transcript: "summarize my email",
     sessionId: "...",
     userId: "...",
     deviceId: "...",
     credentials: {google: {access_token, scopes}, api_keys: {...}}
   }
6. n8n routes to email.summarize sub-canvas
7. Sub-canvas uses credentials.google.access_token to call Gmail API
8. Result text returned → sidecar → WSS → client → local TTS
```

**The credentials travel sidecar → n8n → sub-canvas → Google API.** They never touch the client.

---

## Security Properties

| Property | How it's enforced |
|----------|-------------------|
| Client secret never in client app | OAuth uses PKCE; secret stays in sidecar `.env` |
| API keys encrypted at rest | Fernet symmetric encryption in SQLite |
| API keys never returned to client | `/apikeys/list` returns only provider names |
| OAuth tokens scoped | Minimal scopes requested (see [02-oauth-flow.md](./02-oauth-flow.md)) |
| WSS can be token-authed | `NEXUS_SIDECAR_TOKEN` env var → `Bearer` header check |
| Text-only protocol | No audio frames, no binary data, reduces attack surface |
| CSP restricts connect-src | `tauri.conf.json` limits to backend host + `ipc:` |

---

## What to Read Next

| If you want to understand… | Read |
|----------------------------|------|
| The OAuth PKCE flow step-by-step | [02-oauth-flow.md](./02-oauth-flow.md) |
| API key management (add/remove/list) | [03-api-keys.md](./03-api-keys.md) |
| Which Google APIs and why | [04-google-integrations.md](./04-google-integrations.md) |
| GitHub OAuth scopes | [05-github-integration.md](./05-github-integration.md) |
| Device registration | [06-device-registration.md](./06-device-registration.md) |
| Security best practices + rotation | [07-security-best-practices.md](./07-security-best-practices.md) |
| Setup page UI walkthrough | [08-setup-page-guide.md](./08-setup-page-guide.md) |
