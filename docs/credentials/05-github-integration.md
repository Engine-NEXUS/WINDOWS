> **Partially superseded (2026-10-17).** The Python sidecar and its `/apikeys/*` routes mentioned here no longer exist. OAuth tokens are held by the Cloudflare Worker (D1) and the on-device vault; third-party API keys live in the OS keychain (a legacy plaintext copy in `settings.json` still exists and is being removed — see `docs/memory-core/06-limits-risks-and-roadmap.md`). Read the rest as design history.

# GitHub Integration

> How NEXUS connects to GitHub via OAuth and what it can do with the access token.

**Source files:**
- `server/sidecar/oauth.py` — GitHub token exchange
- `frontend/src/setup/oauth.ts` — `connectOAuth("github", userId)`

---

## OAuth Flow

GitHub OAuth Apps don't support PKCE, but the flow is similar to Google:

```
1. User clicks "Connect GitHub" in setup
2. Client generates state (user_id) — no PKCE verifier needed
3. GET /oauth/auth-url?provider=github&user_id=lakshya
4. Sidecar builds URL:
     https://github.com/login/oauth/authorize
       ?client_id=GITHUB_CLIENT_ID
       &redirect_uri=nexus://oauth/callback
       &scope=repo read:org workflow
       &state=lakshya
5. Client opens URL in system browser
6. User authorizes on GitHub
7. GitHub redirects to: nexus://oauth/callback?code=XXX&state=lakshya
8. Tauri deep-link catches redirect
9. POST /oauth/exchange {provider:"github", code, user_id}
10. Sidecar: POST https://github.com/login/oauth/access_token {
      client_id, client_secret, code
    }
    → Returns {access_token: "gho_..."}
11. db.store_oauth_token(user_id, "github", access_token, None, 0, scopes)
    (No refresh token — GitHub tokens don't expire by default)
12. UI shows "Connected"
```

---

## Scopes

| Scope | What it allows |
|-------|----------------|
| `repo` | Full repository access (read/write code, issues, PRs, actions) |
| `read:org` | Read organization membership (see which orgs the user belongs to) |
| `workflow` | Trigger and manage GitHub Actions workflows |

**Why these scopes?**
- `repo` — needed for "check PR #76", "review my code", "create an issue".
- `read:org` — needed to list repos across orgs.
- `workflow` — needed for "trigger the deploy workflow".

**Minimal scope principle:** We don't request `admin:org`, `delete_repo`, or other destructive scopes.

---

## Token Characteristics

| Property | Value |
|----------|-------|
| Format | `gho_...` (OAuth app token) |
| Expiry | No expiry by default (until revoked) |
| Refresh | Not supported (GitHub doesn't issue refresh tokens for OAuth Apps) |
| Revocation | User can revoke at https://github.com/settings/applications |

**No refresh logic needed.** The sidecar's `get_valid_credentials()` skips the refresh step for GitHub:

```python
if provider == "google":
    refreshed = await _refresh_google(token["refresh_token"])
    # ...
else:
    # GitHub tokens don't expire by default; skip refresh.
    result[provider] = {"access_token": token["access_token"]}
```

---

## What NEXUS Can Do With GitHub

With the `repo read:org workflow` scopes, n8n sub-canvas workflows can:

| Command | GitHub API |
|---------|------------|
| "Check PR #76" | `GET /repos/{owner}/{repo}/pulls/76` |
| "Review my code" | `GET /repos/{owner}/{repo}/pulls` + diff analysis |
| "Create an issue" | `POST /repos/{owner}/{repo}/issues` |
| "List my repos" | `GET /user/repos` |
| "Trigger the deploy workflow" | `POST /repos/{owner}/{repo}/actions/workflows/{workflow_id}/dispatches` |
| "What's the status of the CI?" | `GET /repos/{owner}/{repo}/actions/runs` |

---

## Environment Variables

```env
GITHUB_CLIENT_ID=Ov2.xxxxxxxx
GITHUB_CLIENT_SECRET=xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

**Setup:**
1. Go to [GitHub Settings → Developer settings → OAuth Apps](https://github.com/settings/developers).
2. Create a new OAuth App.
3. Set Authorization callback URL to your Cloudflare Worker URL (e.g., `https://nexus-worker.<your-subdomain>.workers.dev/oauth/callback`).
4. Copy the Client ID and Client Secret into the Cloudflare Worker secrets.

---

## Disconnect

```
DELETE /oauth/disconnect {user_id, provider: "github"}
→ db.delete_oauth_token(user_id, "github")
→ Token removed from SQLite
```

**Note:** This doesn't revoke the token at GitHub's end. The user should also revoke at https://github.com/settings/applications.
