> **Partially superseded (2026-10-17).** The Python sidecar and its `/apikeys/*` routes mentioned here no longer exist. OAuth tokens are held by the Cloudflare Worker (D1) and the on-device vault; third-party API keys live in the OS keychain (a legacy plaintext copy in `settings.json` still exists and is being removed — see `docs/memory-core/06-limits-risks-and-roadmap.md`). Read the rest as design history.

# Google Integrations

> Which Google APIs NEXUS uses, how they're authenticated, and what each one is for.

---

## Two Auth Methods for Google

Google APIs split into two authentication categories:

### OAuth (User's Private Data)
- **Gmail** — read user's email
- **Calendar** — read/manage user's calendar
- **Drive** — read user's files
- **Meet** — meeting integration

These require user consent. NEXUS uses OAuth2 PKCE (see [02-oauth-flow.md](./02-oauth-flow.md)).

### API Key (Public / Project-Owned Data)
- **YouTube Data API v3** — search videos, manage playlists
- **Custom Search API** — programmatic Google search
- **Maps API** — geocoding, directions, places
- **Translate API** — text translation

These use API keys stored in the sidecar (see [03-api-keys.md](../archive/credentials/03-api-keys.md)).

---

## OAuth Scopes

Requested during the Google OAuth flow:

| Scope | API | What it allows |
|-------|-----|----------------|
| `gmail.readonly` | Gmail API | Read email messages and metadata |
| `calendar` | Google Calendar API | Read, create, modify calendar events |
| `drive.readonly` | Google Drive API | Read files and metadata |
| `meetings` | Google Meet | Meeting integration |
| `openid` | OpenID Connect | Identity verification |
| `email` | OpenID Connect | User's email address |
| `profile` | OpenID Connect | User's profile info |

**Minimal scope principle:** Only request what NEXUS needs. Adding more scopes requires re-consent (`prompt=consent`).

---

## OAuth vs API Key: When to Use Which

| Scenario | Auth Method | Why |
|----------|-------------|-----|
| "Summarize my email" | OAuth (Gmail) | User's private email |
| "What's on my calendar today?" | OAuth (Calendar) | User's private calendar |
| "Search YouTube for cats" | API key (YouTube Data) | Public search results |
| "Search Google for quantum computing" | API key (Custom Search) | Programmatic search |
| "How do I get to the airport?" | API key (Maps) | Directions (project-owned data) |
| "Translate 'hello' to Japanese" | API key (Translate) | Translation (project-owned data) |

---

## YouTube Data API

**Purpose:** Search videos, get video details, manage playlists.

**Key format:** `AIzaSy...` (39 characters)

**Setup:**
1. Go to [Google Cloud Console](https://console.cloud.google.com/).
2. Create a project (or use existing).
3. Enable **YouTube Data API v3**.
4. Create credentials → API key.
5. Paste the key in NEXUS setup → API Keys → provider: `youtube`.

**Quota:** YouTube Data API has a default quota of 10,000 units/day. Each search costs 100 units. That's ~100 searches/day on the free tier.

---

## Custom Search API

**Purpose:** Programmatic Google Search results (for "search Google for X" commands).

**Key format:** `AIzaSy...`

**Setup:**
1. Enable **Custom Search API** in Google Cloud Console.
2. Create API key.
3. Create a **Custom Search Engine** at [cse.google.com](https://cse.google.com/) — configure it to search the entire web.
4. Get the **Search Engine ID** (`cx` parameter).
5. Store the API key as provider `customsearch`.
6. Store the Search Engine ID as provider `searchengine` (it's not a secret, but we store it alongside for convenience).

**Quota:** 100 queries/day free, $5/1000 queries after that.

---

## Maps API

**Purpose:** Geocoding, directions, places.

**Key format:** `AIzaSy...`

**Setup:**
1. Enable **Maps JavaScript API** + **Geocoding API** + **Directions API**.
2. Create API key.
3. Store as provider `maps`.

**Billing:** Maps API requires billing enabled. Has a $200/month free tier.

---

## Translate API

**Purpose:** Text translation.

**Key format:** `AIzaSy...`

**Setup:**
1. Enable **Cloud Translation API**.
2. Create API key.
3. Store as provider `translate`.

**Billing:** Translation API requires billing enabled. Has a free tier of 500,000 chars/month.

---

## Service Accounts: Not Used

**Service accounts** are a third Google auth method (machine-to-machine, no user context). They're appropriate for:
- Server-to-server API calls.
- Accessing project-owned data (e.g. a shared Google Sheet).

They're **NOT appropriate** for:
- Reading a user's personal Gmail.
- Accessing a user's personal Calendar.
- Accessing a user's personal Drive.

For user data, OAuth is the correct choice. NEXUS does not use service accounts.

---

## Refresh Token Split

The conversation history noted that combining YouTube + Drive scopes in a single OAuth authorization caused a scope-combination error in the Google OAuth Playground. The solution was to split into separate authorization flows:

- **Flow 1 (OAuth):** Gmail, Calendar, Drive, Meet — user's private data.
- **Flow 2 (API key):** YouTube, Custom Search, Maps, Translate — public/project data.

This split is reflected in the current architecture: OAuth handles private data, API keys handle public data. No scope combination issue.

---

## Environment Variables (Sidecar `.env`)

```env
# OAuth (client secret stays server-side)
GOOGLE_CLIENT_ID=xxxxxxxx.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=GOCSPX-xxxxxxxx

# API Keys (stored in SQLite, but can be pre-loaded via env)
YOUTUBE_API_KEY=AIzaSyxxxxxxxx
CUSTOM_SEARCH_API_KEY=AIzaSyxxxxxxxx
MAPS_API_KEY=AIzaSyxxxxxxxx
TRANSLATE_API_KEY=AIzaSyxxxxxxxx
SEARCH_ENGINE_ID=xxxxx:xxxxx

# Encryption key for API keys at rest
NEXUS_ENCRYPTION_KEY=xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
```

**Security:** These should be in the sidecar's `.env` file, which is gitignored. They should **never** be committed to the repository or pasted into chat.
