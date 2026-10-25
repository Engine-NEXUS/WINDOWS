> **Partially superseded (2026-10-17).** The Python sidecar and its `/apikeys/*` routes mentioned here no longer exist. OAuth tokens are held by the Cloudflare Worker (D1) and the on-device vault; third-party API keys live in the OS keychain (a legacy plaintext copy in `settings.json` still exists and is being removed — see `docs/memory-core/06-limits-risks-and-roadmap.md`). Read the rest as design history.

# OAuth2 PKCE Flow (Google + GitHub)

> Step-by-step breakdown of how NEXUS connects a Google or GitHub account without ever shipping the client secret to the desktop app.

**Source files:**
- `frontend/src/setup/oauth.ts` — PKCE generation, browser open, redirect handling, code exchange
- `server/sidecar/oauth.py` — auth URL generation, token exchange, token refresh, status, disconnect
- `server/sidecar/db.py` — token storage in SQLite

---

## What Is OAuth2 PKCE?

**PKCE** (Proof Key for Code Exchange, RFC 7636) is an extension to OAuth2 that allows a **public client** (a desktop app with no server) to securely exchange an authorization code for tokens **without** having a client secret.

```
Client generates:
  code_verifier  = random 43-128 char string
  code_challenge = SHA256(code_verifier)  → base64url

Client sends code_challenge to auth server (in the authorization URL).
Auth server returns code.
Client sends code + code_verifier to token endpoint.
Token server hashes code_verifier, compares to stored code_challenge.
  Match → issue tokens.
  No match → reject.
```

**Why PKCE?** Even if an attacker intercepts the authorization code (e.g. from the redirect URL), they can't exchange it without the `code_verifier`, which never left the client.

---

## The Full Flow (Google)

```
  Step 1: Setup page (React)
  ──────────────────────────
  User clicks "Connect Google"
  → generateCodeVerifier()        // 32 random bytes → base64url
  → generateCodeChallenge(verifier)  // SHA256 → base64url
  → GET /oauth/auth-url?provider=google&user_id=lakshya&code_challenge=XXX

  Step 2: Sidecar builds auth URL
  ───────────────────────────────
  Sidecar constructs:
    https://accounts.google.com/o/oauth2/v2/auth
      ?client_id=GOOGLE_CLIENT_ID
      &redirect_uri=nexus://oauth/callback
      &response_type=code
      &scope=gmail.readonly calendar drive.readonly meetings openid email profile
      &code_challenge=XXX
      &code_challenge_method=S256
      &state=lakshya
      &access_type=offline         // needed for refresh token
      &prompt=consent              // force consent to get new refresh token
  → Returns {url: "...", redirect_uri: "nexus://oauth/callback"}

  Step 3: Client opens system browser
  ────────────────────────────────────
  → open(url) via @tauri-apps/plugin-shell
  → User logs into Google in their system browser
  → User grants consent for the requested scopes
  → Google redirects to: nexus://oauth/callback?code=AUTH_CODE&state=lakshya

  Step 4: Tauri deep-link catches the redirect
  ────────────────────────────────────────────
  → tauri-plugin-deep-link registers the "nexus://" scheme
  → On Windows/Linux: single-instance plugin passes the URL as a CLI arg
  → On macOS: deep-link plugin emits an event
  → Both paths emit: "deep-link://oauth-callback" with the full URL

  Step 5: Client extracts code + exchanges tokens
  ───────────────────────────────────────────────
  → handleOAuthRedirect(url)
  → Parse URL: code=AUTH_CODE, state=lakshya
  → POST /oauth/exchange {
      provider: "google",
      code: AUTH_CODE,
      code_verifier: PKCE_VERIFIER,    // the original verifier, not the challenge
      redirect_uri: "nexus://oauth/callback",
      user_id: "lakshya",
      state: "lakshya"
    }

  Step 6: Sidecar exchanges code for tokens
  ──────────────────────────────────────────
  → _exchange_google(code, code_verifier, redirect_uri)
  → POST https://oauth2.googleapis.com/token {
      client_id: GOOGLE_CLIENT_ID,
      client_secret: GOOGLE_CLIENT_SECRET,   // ← secret used HERE, server-side only
      code: AUTH_CODE,
      code_verifier: PKCE_VERIFIER,
      redirect_uri: "nexus://oauth/callback",
      grant_type: "authorization_code"
    }
  → Google returns: {access_token: "ya29...", refresh_token: "1//...", expires_in: 3600}

  Step 7: Sidecar stores tokens
  ──────────────────────────────
  → db.store_oauth_token(user_id, "google", access_token, refresh_token, 3600, scopes)
  → INSERT OR REPLACE INTO oauth_tokens VALUES (...)
  → Returns {ok: true, provider: "google", connected: true}

  Step 8: Client updates UI
  ──────────────────────────
  → refreshStatus()
  → GET /oauth/status?user_id=lakshya
  → Returns {google: {connected: true, expired: false, scopes: "..."}}
  → UI shows "Connected" badge
```

---

## GitHub Flow

GitHub OAuth Apps don't support PKCE, but the flow is similar:

```
  Step 1-2: Same as Google, but:
    URL: https://github.com/login/oauth/authorize
      ?client_id=GITHUB_CLIENT_ID
      &redirect_uri=nexus://oauth/callback
      &scope=repo read:org workflow
      &state=lakshya
    (No code_challenge — GitHub doesn't support PKCE for OAuth Apps)

  Step 3-4: Same as Google

  Step 5-6: Exchange:
    POST https://github.com/login/oauth/access_token {
      client_id: GITHUB_CLIENT_ID,
      client_secret: GITHUB_CLIENT_SECRET,
      code: AUTH_CODE,
      redirect_uri: "nexus://oauth/callback"
    }
    → Returns {access_token: "gho_..."}
    (No refresh token — GitHub tokens don't expire by default)
```

**CSRF protection:** Even without PKCE, the `state` parameter prevents CSRF attacks. The client sends `state=user_id`; the redirect must include the same `state`.

---

## Token Refresh

Google access tokens expire in ~1 hour. The sidecar refreshes them automatically:

```python
async def get_valid_credentials(user_id: str) -> dict:
    oauth_tokens = db.get_all_oauth_tokens(user_id)
    for provider, token in oauth_tokens.items():
        if db.is_token_expired(token) and token.get("refresh_token"):
            if provider == "google":
                refreshed = await _refresh_google(token["refresh_token"])
                # POST https://oauth2.googleapis.com/token {
                #   client_id, client_secret, refresh_token, grant_type: "refresh_token"
                # }
                db.store_oauth_token(user_id, "google", new_access, refresh_token, expires_in, scopes)
            # GitHub tokens don't expire — no refresh needed
    # ...
```

**60-second buffer:** `is_token_expired()` returns true if `now > expires_at - 60s`, so the refresh happens before the token actually expires.

---

## Disconnect

```
DELETE /oauth/disconnect {
  user_id: "lakshya",
  provider: "google"
}
→ db.delete_oauth_token(user_id, "google")
→ Tokens removed from SQLite
→ UI shows "Connect Google" button again
```

**Note:** This doesn't revoke the token at Google's end. The user should also revoke access at https://myaccount.google.com/permissions if they want full disconnection.

---

## Scopes Requested

### Google
```
gmail.readonly       — Read Gmail messages
calendar             — Read and manage Google Calendar
drive.readonly       — Read Google Drive files
meetings             — Google Meet integration
openid email profile — Basic identity (required by Google)
```

### GitHub
```
repo                 — Full repository access (read/write)
read:org             — Read organization membership
workflow             — Trigger GitHub Actions workflows
```

**Minimal scope principle:** We request only what NEXUS needs. If a future feature needs more scope, the user must re-consent (the `prompt=consent` flag forces this).
