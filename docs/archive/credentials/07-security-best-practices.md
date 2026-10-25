> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# Security Best Practices

> How to keep NEXUS credentials secure, and what to do if they're exposed.

---

## The Threat Model

| Threat | Mitigation |
|--------|------------|
| Client secret leaked from client app | PKCE — secret stays in sidecar `.env`, never in the client |
| API key intercepted in transit | HTTPS (Caddy/Tailscale) between client and sidecar |
| API keys stolen from database | Fernet encryption at rest (`NEXUS_ENCRYPTION_KEY`) |
| OAuth token stolen from database | Tokens are opaque; can be revoked at provider; stored server-side only |
| Mic audio leaked | Text-only protocol; audio goes to `127.0.0.1:8000` only, never to the server |
| Third-party content accesses mic | Permission handler only allows NEXUS-owned origins |
| CSRF on OAuth redirect | `state` parameter (user_id) verified on redirect |
| Replay of authorization code | PKCE `code_verifier` — code is useless without it |
| Stale tokens used after revocation | `is_token_expired()` with 60s buffer; refresh on each request |

---

## Secret Hygiene Rules

### 1. Never commit secrets to git

The `.gitignore` should include:
```
server/sidecar/.env
frontend/.env.local
*.key
*.pem
NEXUS_credentials.db
```

**Verify:** `git log --all --full-history -- server/sidecar/.env` should return nothing.

### 2. Never paste secrets into chat

If secrets are pasted into a conversation (as happened in this project's history), they must be treated as **compromised** and rotated:

| Secret Type | How to Rotate |
|-------------|---------------|
| Google OAuth client secret | Google Cloud Console → APIs & Services → Credentials → Edit OAuth client → Reset client secret |
| GitHub OAuth client secret | GitHub Settings → Developer settings → OAuth App → Generate new client secret |
| Google refresh token | Revoke at https://myaccount.google.com/permissions → re-authorize in NEXUS |
| API keys (YouTube, Maps, etc.) | Google Cloud Console → Credentials → Delete old key → Create new key |
| `NEXUS_ENCRYPTION_KEY` | Generate new Fernet key → re-encrypt all stored API keys (or delete and re-add them) |
| `N8N_API_TOKEN` | n8n settings → API → regenerate |

### 3. Use environment variables, not hardcoded values

All secrets are loaded from environment variables:
```python
GOOGLE_CLIENT_ID = os.getenv("GOOGLE_CLIENT_ID", "")
GOOGLE_CLIENT_SECRET = os.getenv("GOOGLE_CLIENT_SECRET", "")
```

If the env var is not set, the value is empty — the feature is disabled, not broken with a hardcoded key.

### 4. Encrypt sensitive data at rest

- **API keys:** Fernet-encrypted in SQLite.
- **OAuth tokens:** Stored as-is (they're opaque and revocable).
- **Voice profiles:** Stored as JSON (biometric data, but local-only and not a credential).

### 5. Least privilege

- OAuth scopes are minimal (only what NEXUS needs).
- Tauri capabilities (`capabilities/main.json`) grant only the scopes needed.
- CSP restricts `connect-src` to the backend host + `ipc:`.
- Permission handler only allows NEXUS-owned origins for mic/camera.

### 6. Don't return secrets to the client

- `/apikeys/list` returns only provider names, never the keys.
- `/config/check` returns only boolean flags (`configured: true/false`), never the values.
- OAuth tokens are never sent to the client — only "connected: true/false".

---

## Production Deployment Checklist

- [ ] `NEXUS_ENCRYPTION_KEY` set in sidecar `.env` (not ephemeral).
- [ ] `NEXUS_SIDECAR_TOKEN` set (WSS authentication enabled).
- [ ] HTTPS/WSS via Caddy or Tailscale (no plaintext).
- [ ] `.env` file is gitignored and not committed.
- [ ] OAuth client secrets rotated if ever exposed.
- [ ] API keys rotated if ever exposed.
- [ ] `N8N_API_TOKEN` set (n8n webhook authentication).
- [ ] SQLite DB file (`NEXUS_credentials.db`) is backed up and access-restricted.
- [ ] CSP in `tauri.conf.json` restricts `connect-src` to the backend host.
- [ ] Tauri capabilities grant only the minimum scopes needed.

---

## Incident Response: If Secrets Are Exposed

1. **Identify what was exposed:**
   - Client ID? (Not secret — it's in the client app anyway.)
   - Client secret? (Rotate immediately.)
   - Refresh token? (Revoke at provider, re-authorize.)
   - API key? (Delete and recreate.)
   - Encryption key? (Generate new, re-encrypt all stored keys.)

2. **Rotate the exposed secret** (see table above).

3. **Update the sidecar `.env`** with the new secret.

4. **Restart the sidecar** so it picks up the new `.env`.

5. **Re-authorize** if OAuth tokens were exposed (disconnect + reconnect in setup page).

6. **Audit** `git log` to ensure the secret was never committed. If it was, use `git filter-branch` or BFG Repo-Cleaner to purge it from history.

7. **Document** the incident and the rotation in your security log.

---

## The Text-Only Protocol as a Security Property

The WebSocket between client and sidecar carries **only JSON text frames**. Binary frames are explicitly rejected:

```python
# sidecar.py
data = msg.get("bytes")
if data is not None:
    log.warning("session sent binary frame — REJECTED (text-only protocol)")
    await _send_error(ws, "binary frames not supported — text only")
    continue  # Do NOT buffer or process the binary data
```

This means:
- **No audio frames** can be sent to the server (even by accident).
- **No binary blobs** can be smuggled through the WebSocket.
- The attack surface is limited to JSON text frames with a 64 KB size cap.
- `NEXUS_SIDECAR_TOKEN` (if set) requires a `Bearer` header on the WSS connection.

---

## WebView2 Permission Handler as a Security Property

The mic/camera permission handler only auto-allows **NEXUS-owned origins**:

```rust
const ALLOWED_ORIGIN_PREFIXES: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "http://localhost",
    "https://localhost",
    "ipc://localhost",
];
```

If any external origin ever loads in the webview (e.g. via a bug or XSS), it will **not** get automatic mic/camera access — WebView2's default dialog will appear, alerting the user.
