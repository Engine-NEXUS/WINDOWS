> **SUPERSEDED — archived 2026-10-17.** This document describes an earlier design or a point-in-time snapshot (n8n/Ollama "fat server", faster-whisper, Piper, Fernet-in-SQLite sidecar, PR-era status). It no longer matches the code. For current behaviour read the repository `README.md`, `docs/README.md` and `AGENTS.md`. Kept for history only.

# Setup Page Guide

> A walkthrough of the NEXUS setup page — what each section does and how to use it.

**Source files:**
- `frontend/src/setup/SetupApp.tsx` — main setup UI
- `frontend/src/setup/VoiceEnrollment.tsx` — voice enrollment section
- `frontend/src/setup/oauth.ts` — OAuth + API key client functions

---

## When the Setup Page Appears

1. **First launch** — if no `nexus-config.json` exists. (Currently auto-created with defaults, so this is skipped — the user can open it later via tray → Settings.)
2. **Tray → Settings…** — opens the setup window at any time.
3. **After OAuth redirect** — the setup window may show status updates after a redirect.

---

## The Setup Page Layout

```
┌─────────────────────────────────────────────────────┐
│ NEXUS Setup                                         │
│ Connect your accounts to give NEXUS access to       │
│ your tools.                                         │
├─────────────────────────────────────────────────────┤
│                                                     │
│ ── Server ──────────────────────────────────────── │
│                                                     │
│ Server URL: [https://your-server.com:8443]          │
│ User ID:    [local-user               ]             │
│ Device ID:  [local-device             ]             │
│                                                     │
│ ── Google ──────────────────────────────────────── │
│                                                     │
│ Gmail · Calendar · Drive · Meet                     │
│                          [ Connect Google ]         │
│                                                     │
│ ── GitHub ──────────────────────────────────────── │
│                                                     │
│ Full repository read/write access                   │
│                          [ Connect GitHub ]         │
│                                                     │
│ ── API Keys ────────────────────────────────────── │
│                                                     │
│ Add API keys for services like Claude, Devin, etc.  │
│                                                     │
│ ┌──────────┐ ┌──────────┐                          │
│ │ claude   │ │ Remove   │                          │
│ └──────────┘ └──────────┘                          │
│                                                     │
│ [Provider name] [API key ••••••••] [Save Key]       │
│                                                     │
│ ── Voice Enrollment ────────────────────────────── │
│                                                     │
│ Status: Not enrolled                                │
│                         [ Start Enrollment ]        │
│                                                     │
│ ── Footer ──────────────────────────────────────── │
│                                                     │
│              [ Save & Continue ]                    │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## Section 1: Server

| Field | Purpose | Default |
|-------|---------|---------|
| Server URL | WebSocket URL of the sidecar | `ws://127.0.0.1:49152/ws` |
| User ID | Identifies the user (for credential lookup) | `local-user` |
| Device ID | Identifies the device (for device registration) | `local-device` |

**When you change the Server URL**, the page automatically refreshes the OAuth status, API key list, and config check from the new sidecar.

**On "Save & Continue":**
- Calls `invoke("save_server_config", {serverUrl, userId, deviceId})`.
- Rust writes `nexus-config.json` to the app data directory.
- Calls `invoke("close_setup_window")` to hide the setup window.

---

## Section 2: Google

Shows one of three states:

1. **"Connect Google" button** — Google is configured on the server and not yet connected for this user.
2. **"Connected" badge + "Disconnect" button** — OAuth tokens are stored and valid.
3. **"Connected (expired)" badge** — Tokens exist but are expired (refresh will be attempted on next request).
4. **"Not configured on server" badge** — The sidecar's `.env` doesn't have `GOOGLE_CLIENT_ID`/`GOOGLE_CLIENT_SECRET`.

**Clicking "Connect Google":**
1. Generates PKCE verifier + challenge.
2. Asks sidecar for the auth URL.
3. Opens the URL in the system browser.
4. User logs in and grants consent.
5. Browser redirects to `nexus://oauth/callback?code=XXX`.
6. Tauri deep-link catches the redirect.
7. Client sends code + verifier to sidecar `/oauth/exchange`.
8. Sidecar exchanges for tokens and stores them.
9. UI refreshes to show "Connected".

---

## Section 3: GitHub

Same flow as Google, but:
- No PKCE (GitHub OAuth Apps don't support it).
- Uses `state` for CSRF protection.
- Token doesn't expire (no refresh needed).

---

## Section 4: API Keys

**Adding a key:**
1. Type the provider name (e.g. `claude`, `youtube`, `devin`).
2. Paste the API key (input is `type="password"` — masked).
3. Click "Save Key".
4. POST to sidecar `/apikeys/add`.
5. Key is Fernet-encrypted and stored in SQLite.
6. UI refreshes to show the provider in the list.

**Removing a key:**
1. Click "Remove" next to the provider name.
2. DELETE to sidecar `/apikeys/remove`.
3. Key is deleted from SQLite.
4. UI refreshes.

**Listing keys:**
- GET `/apikeys/list?user_id=...` returns only provider names, never the keys.

---

## Section 5: Voice Enrollment

**Status display:**
- "Not enrolled" — no voice profile exists.
- "Enrolled (N clips)" — voice profile exists with N clips.

**Enrollment process:**
1. Click "Start Enrollment".
2. For each of 5 clips:
   - Countdown 3...2...1...
   - Record 3 seconds (say "NEXUS").
   - Stop.
3. All 5 clips are sent to Rust as `Vec<Vec<f32>>`.
4. Rust extracts speaker embeddings via sherpa-onnx.
5. Rust runs ASR on each clip to capture pronunciation variants.
6. Profile is saved to disk.
7. UI shows "Enrolled (5 clips)".

**Re-enrollment** appends new clips — it doesn't wipe the existing profile.

**Delete profile:**
- Click "Delete Voice Profile".
- Calls `invoke("delete_voice_profile")`.
- Profile JSON is deleted from disk.
- Speaker verification is disabled.

---

## Section 6: Save & Continue

- Validates that Server URL is not empty.
- Calls `invoke("save_server_config", {serverUrl, userId, deviceId})`.
- Calls `invoke("close_setup_window")`.
- Shows "Saved!" confirmation.

**The setup window is hidden, not destroyed.** It can be reopened via tray → Settings.

---

## Error Handling

Errors are shown in a red banner at the top of the page:
- "Can't reach server: ..." — sidecar is not running or URL is wrong.
- "Google connection failed: ..." — OAuth exchange failed.
- "Failed to save API key: ..." — sidecar rejected the key.
- "Failed to save: ..." — config save failed.

**The page is resilient** — if the sidecar is unreachable, the page still loads (with an error banner) and lets the user enter the Server URL. Once the sidecar is reachable, the status sections refresh automatically.
