> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# API Key Management

> How NEXUS stores, retrieves, and uses API keys for services that don't support OAuth (Claude, Devin, Antigravity, YouTube Data API, etc.).

**Source files:**
- `server/sidecar/oauth.py` — `/apikeys/*` endpoints
- `server/sidecar/db.py` — `store_api_key()`, `get_api_key()`, `get_all_api_keys()`, `delete_api_key()` (Fernet-encrypted)
- `frontend/src/setup/oauth.ts` — `addApiKey()`, `removeApiKey()`, `listApiKeys()` client functions
- `frontend/src/setup/SetupApp.tsx` — API key UI

---

## What Are API Keys For?

Some services don't support OAuth (or the user prefers a simple key):

| Provider | Example Key Format | Purpose |
|----------|-------------------|---------|
| `claude` | `sk-ant-...` | Anthropic Claude API for LLM calls |
| `devin` | `devin-...` | Devin AI API |
| `antigravity` | `ag-...` | Antigravity API |
| `youtube` | `AIza...` | YouTube Data API v3 (search, playlist management) |
| `customsearch` | `AIza...` | Google Custom Search API |
| `maps` | `AIza...` | Google Maps API (geocoding, directions) |
| `translate` | `AIza...` | Google Translate API |
| `searchengine` | (search engine ID, not a key) | Google Custom Search Engine ID |
| Any custom | (any string) | User-defined provider name + key |

**The provider name is free-text.** The user can type any provider name (e.g. "openai", "perplexity", "my_custom_api") and paste the key.

---

## The Three Endpoints

### 1. Add an API Key

```
POST /apikeys/add
Content-Type: application/json

{
  "user_id": "lakshya",
  "provider": "claude",
  "api_key": "sk-ant-xxx..."
}
```

**Sidecar:**
1. Validate required fields.
2. `db.store_api_key(user_id, "claude", "sk-ant-xxx...")`
3. Fernet encrypt the key: `ciphertext = Fernet.encrypt(key.encode())`
4. `INSERT OR REPLACE INTO api_keys (user_id, provider, key_encrypted, created_at) VALUES (...)`
5. Return `{ok: true, provider: "claude", stored: true}`

**The plaintext key is never logged.** Only the encrypted ciphertext is stored in SQLite.

### 2. Remove an API Key

```
DELETE /apikeys/remove
Content-Type: application/json

{
  "user_id": "lakshya",
  "provider": "claude"
}
```

**Sidecar:**
1. `db.delete_api_key(user_id, "claude")`
2. `DELETE FROM api_keys WHERE user_id=? AND provider=?`
3. Return `{ok: true, removed: "claude"}`

### 3. List Stored API Keys

```
GET /apikeys/list?user_id=lakshya
```

**Sidecar:**
1. `db.get_all_api_keys(user_id)` — returns `{provider: decrypted_key, ...}`
2. **Return only the provider names**, not the keys:
   ```json
   {"user_id": "lakshya", "providers": ["claude", "devin", "youtube"]}
   ```

**This is a security property.** The client can see *which* providers are configured, but never the actual key values. The keys only leave the sidecar when injected into an n8n webhook call.

---

## Encryption at Rest

API keys are encrypted with **Fernet** (symmetric authenticated encryption, AES-128-CBC + HMAC-SHA256):

```python
from cryptography.fernet import Fernet

ENCRYPTION_KEY = os.getenv("NEXUS_ENCRYPTION_KEY", "")
_fernet = Fernet(ENCRYPTION_KEY.encode())

def store_api_key(user_id, provider, api_key):
    encrypted = _fernet.encrypt(api_key.encode()).decode()
    # INSERT encrypted into SQLite

def get_api_key(user_id, provider):
    # SELECT key_encrypted FROM api_keys
    return _fernet.decrypt(row["key_encrypted"].encode()).decode()
```

**The encryption key** is set via the `NEXUS_ENCRYPTION_KEY` environment variable in the sidecar's `.env` file. Generate one with:
```python
from cryptography.fernet import Fernet
print(Fernet.generate_key().decode())
```

**If `NEXUS_ENCRYPTION_KEY` is not set**, the sidecar generates an ephemeral key at startup. This means API keys won't survive a sidecar restart. A warning is logged. **Set this in production.**

---

## How Keys Are Used at Request Time

When the user says "summarize my email":

```python
async def get_valid_credentials(user_id: str) -> dict:
    result = {"api_keys": {}}
    # ... OAuth tokens ...
    api_keys = db.get_all_api_keys(user_id)  # Fernet decrypt
    result["api_keys"] = api_keys  # {claude: "sk-ant-...", youtube: "AIza...", ...}
    return result

# In _process_transcript:
credentials = await get_valid_credentials(sess.user_id)
# credentials = {
#   google: {access_token: "ya29...", scopes: "..."},
#   github: {access_token: "gho_..."},
#   api_keys: {claude: "sk-ant-...", youtube: "AIza..."}
# }

# Sent to n8n:
payload = {
    transcript: "summarize my email",
    credentials: credentials,  # ← keys injected here
    ...
}
```

The n8n sub-canvas workflows receive the credentials and use them to call the respective APIs. The keys travel: **sidecar → n8n → sub-canvas → external API**. They never touch the client.

---

## Google API Keys vs OAuth

Some Google APIs use **API keys** (not OAuth):

| API | Auth Method | Why |
|-----|-------------|-----|
| YouTube Data API | API key | Public data (search, video info) — no user context needed |
| Custom Search API | API key | Programmatic search — no user context |
| Maps API | API key | Geocoding, directions — no user context |
| Translate API | API key | Translation — no user context |
| Gmail | OAuth | User's private email — requires user consent |
| Calendar | OAuth | User's private calendar — requires user consent |
| Drive | OAuth | User's private files — requires user consent |

**Rule of thumb:** If the API accesses **public data** or **project-owned data**, use an API key. If it accesses **user's private data**, use OAuth.

---

## Setup Page UI

The setup page (`SetupApp.tsx`) provides:

1. **A list of currently stored API keys** — shows provider name + "Remove" button.
2. **An add form** — two inputs (provider name, API key) + "Save Key" button.
3. **The API key input is `type="password"`** — masked as the user types.

```
┌─────────────────────────────────────────┐
│ API Keys                                │
│                                         │
│ Add API keys for services like Claude,  │
│ Devin, Antigravity, etc.                │
│                                         │
│ ┌───────────┐ ┌──────────────┐          │
│ │ claude    │ │ Remove       │          │
│ └───────────┘ └──────────────┘          │
│ ┌───────────┐ ┌──────────────┐          │
│ │ youtube   │ │ Remove       │          │
│ └───────────┘ └──────────────┘          │
│                                         │
│ [Provider name    ] [API key •••••••• ] │
│ [           Save Key                  ] │
└─────────────────────────────────────────┘
```
