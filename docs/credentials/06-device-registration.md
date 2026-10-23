> **Partially superseded (2026-10-17).** The Python sidecar and its `/apikeys/*` routes mentioned here no longer exist. OAuth tokens are held by the Cloudflare Worker (D1) and the on-device vault; third-party API keys live in the OS keychain (a legacy plaintext copy in `settings.json` still exists and is being removed — see `docs/memory-core/06-limits-risks-and-roadmap.md`). Read the rest as design history.

# Device Registration

> How NEXUS registers devices and validates them for server access.

**Source files:**
- `server/sidecar/oauth.py` — `/device/register` and `/device/validate` endpoints
- `server/sidecar/db.py` — `register_device()`, `validate_device()`
- `src-tauri/src/commands.rs` — `save_server_config` / `get_server_config` (stores deviceId locally)

---

## Purpose

Device registration lets the server know **which devices** are authorized to make requests for a given user. This enables:

1. **Rate limiting** — limit requests per device.
2. **Audit trail** — know which device made which request.
3. **Revocation** — revoke a specific device (e.g. lost laptop) without affecting others.
4. **Device tracking** — see how many devices a user has connected.

---

## The Flow

### Registration (First-Run Setup)

```
1. User opens NEXUS for the first time
2. Setup page asks for:
   - Server URL (ws://127.0.0.1:49152/ws)
   - User ID (e.g. "lakshya")
   - Device ID (e.g. "laptop-abc123" — auto-generated or user-entered)
3. User clicks "Save & Continue"
4. Frontend calls invoke("save_server_config", {serverUrl, userId, deviceId})
   → Rust writes nexus-config.json to app data dir
5. (Future) Frontend calls POST /device/register {
     user_id: "lakshya",
     device_id: "laptop-abc123",
     device_token: null  // or an existing token if re-registering
   }
6. Sidecar: db.register_device(user_id, device_id, device_token)
   → INSERT OR REPLACE INTO user_devices VALUES (...)
7. Returns {ok: true, user_id, device_id}
```

### Validation (At Request Time)

```
1. WSS connect: {type:"start", sessionId, userId, deviceId}
2. (Future) Sidecar validates: db.validate_device(userId, deviceId)
   → SELECT 1 FROM user_devices WHERE user_id=? AND device_id=?
   → If not found → close WSS with policy violation
3. If valid → proceed with session
```

**Currently, validation is implemented but not enforced** — the sidecar accepts any `userId`/`deviceId` in the WSS `start` frame. Enforcing validation is a future hardening step.

---

## Database Schema

```sql
CREATE TABLE user_devices (
    user_id       TEXT NOT NULL,
    device_id     TEXT NOT NULL,
    device_token  TEXT,
    created_at    REAL NOT NULL,
    PRIMARY KEY (user_id, device_id)
);
```

- **Primary key:** `(user_id, device_id)` — one row per device per user.
- `device_token` — optional, for future token-based auth.
- `created_at` — when the device was first registered.

---

## Local Config

The client stores its identity in `nexus-config.json` (app data dir):

```json
{
  "serverUrl": "ws://127.0.0.1:49152/ws",
  "userId": "local-user",
  "deviceId": "local-device"
}
```

On first launch, if no config exists, NEXUS auto-creates one with sensible defaults:

```rust
let default_config = serde_json::json!({
    "serverUrl": "ws://127.0.0.1:49152/ws",
    "userId": "local-user",
    "deviceId": "local-device",
});
```

This avoids showing the setup window on first launch (which confused users into thinking there was a connection error). The setup window can be opened later via tray → Settings.

---

## Endpoints

### POST /device/register

```json
// Request
{
  "user_id": "lakshya",
  "device_id": "laptop-abc123",
  "device_token": "optional-existing-token"
}

// Response
{
  "ok": true,
  "user_id": "lakshya",
  "device_id": "laptop-abc123"
}
```

### GET /device/validate

```
GET /device/validate?user_id=lakshya&device_id=laptop-abc123

// Response
{
  "valid": true
}
```

---

## Future Hardening

Currently, device registration is **informational** — the sidecar doesn't reject unknown devices. Future improvements:

1. **Enforce validation** — reject WSS connections from unregistered devices.
2. **Device tokens** — issue a token at registration, require it in the WSS `Authorization` header.
3. **Token rotation** — allow the user to rotate a device token from the setup page.
4. **Device list UI** — show all registered devices in the setup page, with "Revoke" buttons.
5. **Per-device rate limiting** — limit requests per device per hour.
