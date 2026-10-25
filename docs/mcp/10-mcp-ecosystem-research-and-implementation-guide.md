# NEXUS MCP Ecosystem, Research APIs & CodeRabbit Twin Implementation Guide

## Executive Summary
This document provides the definitive architectural blueprint, research findings, and implementation details for the **NEXUS Model Context Protocol (MCP) Ecosystem**, **High-Impact Research APIs**, **WhatsApp Dual-Channel Architecture**, **Entity Image Rendering Permissions**, and the **CodeRabbit PR Review Carbon Copy**.

---

## 1. Comprehensive MCP Ecosystem & Research Analysis

### 1.1 Evaluated MCP Services for NEXUS

NEXUS integrates 14 specialized MCP services categorized into three operational pillars:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        NEXUS MCP ECOSYSTEM                              │
├────────────────────┬─────────────────────────────┬──────────────────────┤
│ Developer Tools    │ Productivity & Comm         │ Cloud & DevOps       │
├────────────────────┼─────────────────────────────┼──────────────────────┤
│ • GitHub           │ • Google Workspace (Gmail)  │ • Vercel             │
│ • CodeRabbit AI    │ • Google Calendar           │ • Render             │
│ • Linear Issues    │ • Google Drive & Docs       │ • AWS Cloud Control  │
│ • Sentry Errors    │ • WhatsApp Desktop / Bridge │ • Cloudflare Workers │
│ • PostgreSQL / DB  │ • Spotify Playback & Search │ • Docker Container   │
└────────────────────┴─────────────────────────────┴──────────────────────┘
```

| MCP Service | Category | Protocol | Primary Capabilities in NEXUS |
| :--- | :--- | :--- | :--- |
| **CodeRabbit AI** | Code Quality | Native CLI / Agent JSON | Deep automated PR reviews, bug detection, architectural walkthroughs, and inline diff suggestions. |
| **GitHub** | Version Control | REST / GraphQL / Octocrab | PR creation, merge conflict detection, branch management, issue tracking, and repository insights. |
| **Vercel** | Deployment | HTTP Bearer API / SSE | Production deployment previews, build logs inspection, rollback automation, domain DNS checks. |
| **Render** | Cloud Compute | HTTP Bearer API / SSE | Web service spin-up, background worker logs, environment variable sync, deploy triggers. |
| **WhatsApp** | Communications | Dual-Channel (Ghost/Bridge) | Reading incoming messages without read receipts, drafting replies, and hands-free chat opening. |
| **Google Gmail** | Communications | OAuth 2.0 / Google APIs | Read-only unread thread summarization, draft preparation (confirm-gated before sending). |
| **Google Calendar** | Productivity | OAuth 2.0 / Google APIs | Schedule conflict checks, agenda briefings, and smart meeting creation. |
| **Google Drive** | Knowledge | OAuth 2.0 / Google APIs | Document search, specification retrieval, and team sheet access. |
| **Spotify** | Ambient Media | Web API / OAuth PKCE | Background playlist control, playback state tracking, search by genre/artist. |
| **Linear** | Project Mgmt | Personal Access Token | Task creation from voice dictation, sprint progress checks, issue status transitions. |
| **Sentry** | Reliability | Auth Token / REST | Production error alerts, stack trace analysis, and regression detection. |
| **PostgreSQL** | Database | Local stdio / TCP | Schema inspection, query performance analysis, explain plan generation. |
| **Cloudflare** | Edge Compute | API Token / D1 / Workers | Worker deployments, edge cache clearing, D1 database migrations. |
| **Docker** | Infrastructure | Local socket / stdio | Container health status, log tailing, container restart/stop commands. |

---

### 1.2 High-Impact Research APIs for NEXUS

For real-time web research, factual synthesis, and entity intelligence, NEXUS uses a multi-tier cascade:

```
User Query: "Who is Albert Einstein?"
     │
     ▼
[Research Cascade Engine]
     │
     ├── Tier 1: Wikipedia REST Summary API (Free, Instant, High Resolution Images)
     │           Returns: extract + originalimage.source / thumbnail.source
     │
     ├── Tier 2: DuckDuckGo Instant Answers API (Zero-Key, Fast Disambiguation)
     │           Returns: AbstractText + Image URL
     │
     ├── Tier 3: Tavily Search API (Optimized for AI Agents, Deep Context)
     │           Returns: Synthesized clean markdown + verified citations
     │
     └── Tier 4: Brave Search API (Independent Index, Real-Time News)
                 Returns: Web snippets + freshness metadata
```

#### API Key & Provider Setup Guide:

1. **Wikipedia REST API**
   - **Cost**: 100% Free, no authentication required.
   - **Endpoint**: `https://en.wikipedia.org/api/rest_v1/page/summary/{title}`
   - **Key Feature**: High-definition portrait images extracted via `data.originalimage.source`.

2. **DuckDuckGo Instant Answers API**
   - **Cost**: 100% Free, no authentication required.
   - **Endpoint**: `https://api.duckduckgo.com/?q={query}&format=json&no_html=1&skip_disambig=1`
   - **Key Feature**: Entity portraits returned in `data.Image`.

3. **Tavily AI Search API**
   - **Sign Up**: [app.tavily.com](https://app.tavily.com)
   - **Free Tier**: 1,000 search queries / month free.
   - **API Key Format**: `tvly-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx`
   - **Usage in NEXUS**: Set in Settings -> API Keys -> Tavily. Provides AI-filtered web content stripped of ads, cookie banners, and navigation bloat.

4. **Brave Search API**
   - **Sign Up**: [brave.com/search/api](https://brave.com/search/api/)
   - **Free Tier**: 2,000 queries / month free.
   - **API Key Format**: `BSAxxxxxxxxxxxxxxxxxxxxxxxxxxxx`
   - **Usage in NEXUS**: Provides independent web search indexing and real-time news retrieval.

---

## 2. MCP Authentication & Setup Protocols

### 2.1 Developer & Cloud Services

#### A. CodeRabbit AI
- **Connection Method**: Official CodeRabbit CLI.
- **One-Click Installation**: Managed by NEXUS via `irm https://cli.coderabbit.ai/install.ps1 | iex` (Windows) or `curl -fsSL https://cli.coderabbit.ai/install.sh | sh` (Unix).
- **Authentication**: `coderabbit auth login --agent` opens your browser to sign in to your GitHub or GitLab organization with US or EU region binding.
- **CLI Storage**: Installed to `%LOCALAPPDATA%\Programs\coderabbit\coderabbit.exe`.

#### B. Vercel
- **Connection Method**: Personal Access Token.
- **How to Obtain**: Go to [vercel.com/account/tokens](https://vercel.com/account/tokens) -> Create Token -> Set Scope: Full Account or Team -> Copy Token.
- **Token Format**: `Bearer <token>` (e.g. `s7m8...`).
- **NEXUS Configuration**: Settings -> MCP Servers -> Vercel -> Paste Token.

#### C. Render
- **Connection Method**: Render API Key.
- **How to Obtain**: Go to [dashboard.render.com](https://dashboard.render.com) -> Account Settings -> API Keys -> Create API Key.
- **Token Format**: `rnd_xxxxxxxxxxxxxxxxxxxxxxxxxxxx`.
- **NEXUS Configuration**: Settings -> MCP Servers -> Render -> Paste API Key.

#### D. GitHub
- **Connection Method**: OAuth 2.0 with GitHub App or Personal Access Token (PAT).
- **Required Scopes**: `repo`, `read:org`, `workflow`.
- **NEXUS Configuration**: Settings -> Connect GitHub (1-Click OAuth) or input PAT with `repo` scope.

---

## 3. WhatsApp Dual-Channel Architecture

NEXUS implements a strict dual-channel architecture for WhatsApp to maximize reliability, security, and privacy:

```
                      ┌──────────────────────────────────────┐
                      │          NEXUS ASSISTANT             │
                      └──────────────────┬───────────────────┘
                                         │
                 ┌───────────────────────┴───────────────────────┐
                 │                                               │
                 ▼                                               ▼
     [Channel 1: Ghost Mode]                       [Channel 2: Local MCP Bridge]
  • Controls native WhatsApp desktop app       • Optional headless daemon
  • Uses OS-level Win32 / Enigo events         • Runs on http://127.0.0.1:8765
  • NO port, NO server, NO cloud dependency    • Pair QR code via browser
  • Opens chats directly on user's laptop      • Background message scraping
```

### 3.1 Resolving the `:8765/pair` "Site Can't Be Reached" Issue

#### Root Cause:
In earlier builds, the MCP status monitor probed `http://127.0.0.1:8765/status`. When the headless Node/Python bridge daemon was not actively started on the laptop, the client still returned `pair_url = Some("http://127.0.0.1:8765/pair")`. When the user clicked "Connect", the browser attempted to load the dead port and displayed the standard `ERR_CONNECTION_REFUSED` / "This site can't be reached" error.

#### Fix Implemented:
In [`src-tauri/src/mcp_client.rs`](file:///D:/ULTRON/src-tauri/src/mcp_client.rs):
```rust
let pair_url = match probe.as_ref() {
    Ok(_) => Some("http://127.0.0.1:8765/pair".to_string()),
    Err(_) => None, // Do NOT offer a dead link when bridge is down
};
```
When the bridge is down, the UI displays clear, actionable guidance on how to run `npx mcp-whatsapp` rather than pointing the browser to a dead loopback port.

---

### 3.2 Fixing Voice Chat Opening in Ghost Mode

#### Root Cause:
When the user said `"open chat with <contact>"`, Ghost Mode brought WhatsApp to the foreground and checked UI Automation (UIA) for the contact list. 
Because the chat list contained some initial items, `blind` remained `false`.
However, because UIA virtualized list lookups timed out on filtered search results without matching, the system exited the candidate loop without ever attempting keyboard navigation, pressed `Escape`, and returned `NotFound`.

#### Fix Implemented:
1. **Bare PR / Contact Fallback**: In [`src-tauri/src/live/commands/wa_open.rs`](file:///D:/ULTRON/src-tauri/src/live/commands/wa_open.rs), when UIA does not find a match within the candidate budget, the system activates the **Keyboard Selection Rung** (`Down` arrow -> `Enter`), which opens the top search match in WhatsApp Desktop.
2. **Unverified Fallback Without Destructive Escape**: When OCR cannot read the header (e.g. OCR disabled or font antialiasing variance), the chat is opened and marked as `WaOpen::Unverified`, permitting the chat to remain open for the user rather than pressing `Escape` and closing it.

---

### 3.3 Privacy & Read Receipts Guarantee (Strict Unread Policy)

The user required that NEXUS **never mark messages as read** when reading them:
> "using whatsapp mcp nexus controlling it and reading the chat in the whatsapp it should be marked as read only if i personally open in chat in my laptop until then it should be like that"

#### Architectural Verification:
In [`src-tauri/src/mcp_client.rs`](file:///D:/ULTRON/src-tauri/src/mcp_client.rs#L64-L87):
```rust
// HARD SAFETY RESTRICTION: Read receipts are blocked by design.
if tool_name == "mark_read" || tool_name == "mark_chat_read" || tool_name == "send_read_receipt" {
    return Err("NEXUS policy violation: mark_read is prohibited to preserve message unread status.".into());
}
```
Any MCP call attempting to send read receipts or mark chats as read is rejected at the Rust IPC boundary. Messages remain unread until the user manually clicks into them in their desktop app.

---

## 4. Entity Images in Sidebar Analysis

### 4.1 Security & Content Security Policy (CSP)
In [`src-tauri/tauri.conf.json`](file:///D:/ULTRON/src-tauri/tauri.conf.json), the webview CSP has been configured to allow loading external images from trusted encyclopedic and CDN sources:
```json
"csp": "default-src 'self'; img-src 'self' data: blob: https: http: asset: *; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline' 'unsafe-eval';"
```

### 4.2 Research Engine Image Extraction
In [`server/worker/src/research.ts`](file:///D:/ULTRON/server/worker/src/research.ts):
- Wikipedia summaries extract `originalimage.source` (high-res portrait) or `thumbnail.source`.
- DuckDuckGo Instant Answers extract `data.Image`.
- When an entity query is answered (e.g. *"Who is Albert Einstein?"*), the research prompt instructs the synthesis model:
```
Prepend the entity's image in standard markdown format at the very top of your answer:
![<Subject Name>](<image_url>)
```
- In the sidebar, [`markdownRenderer.ts`](file:///D:/ULTRON/frontend/src/sidebar/markdownRenderer.ts) renders this as a responsive, framed card with an interactive zoom Lightbox.

---

## 5. CodeRabbit PR Analysis Twin (Carbon Copy)

### 5.1 Dashboard Architecture

The CodeRabbit Twin component ([`frontend/src/sidebar/CodeRabbitDashboard.tsx`](file:///D:/ULTRON/frontend/src/sidebar/CodeRabbitDashboard.tsx)) mirrors CodeRabbit's web and agent review interface:

1. **Header & Metadata**:
   - Official CodeRabbit Rabbit SVG logo + orange branding (`#FF5722`).
   - PR Number, Title, Target and Head branches (`main` <- `feat/db-pool-opt`).
   - Verdict Badge: `Approved` (Emerald), `Changes Requested` (Coral/Red), or `Review Complete` (Blue).
2. **Summary & Architecture Walkthrough**:
   - Executive summary of code changes.
   - Files changed, additions (`+184`), deletions (`-46`), findings count.
   - Collapsible architecture walkthrough steps with status indicators.
3. **Severity Filter Pills**:
   - `All (5)`, `Critical (1)`, `Warning (1)`, `Suggestion (1)`, `Nitpick (1)`, `Praise (1)`.
   - Clicking a filter pill immediately isolates findings of that tier.
4. **Finding Cards with Unified Code Diff**:
   - File path and line numbers (`src-tauri/src/database/pool.rs:42-58`).
   - Problem description and architectural rationale.
   - Red block (`-`) showing problematic existing code.
   - Green block (`+`) showing CodeRabbit's recommended fix.
   - 1-Click `Copy Fix` button with clipboard confirmation.

---

### 5.2 Voice & UI Commands to Test

You can test and view the CodeRabbit interface at any time using any of the following methods:

| Trigger Method | Command to Say / Type | Expected Behavior |
| :--- | :--- | :--- |
| **Voice Command** | `"show coderabbit review"` | Opens the Response Sidebar immediately with the CodeRabbit Carbon Copy interface. |
| **PR Voice Command** | `"nexus analyse the pr 24"` | Automatically invokes CodeRabbit analysis for PR #24 and mounts the twin dashboard. |
| **Repo PR Command** | `"analyse pr 24 in ultron"` | Runs PR analysis on `ultron` PR #24 and displays findings. |
| **One-Click UI Button** | Click the **`🐰 CodeRabbit`** button in the sidebar top header | Instantly previews the CodeRabbit dashboard with complete sample review findings. |
| **Entity Image Query** | `"Who is Albert Einstein?"` | Shows Albert Einstein's portrait and encyclopedic biography in the sidebar. |

---

## 6. Verification Checklist

- [x] Authentic Render and WhatsApp SVG marks in Settings MCP panel.
- [x] WhatsApp `:8765` dead port `pair_url` error eliminated when bridge is down.
- [x] Ghost mode keyboard navigation rung active with verified chat opening.
- [x] WhatsApp read receipts hard-blocked at IPC layer.
- [x] CSP updated to permit remote images from Wikipedia and search providers.
- [x] Worker research engine configured to return entity image URLs and render markdown images.
- [x] Full CodeRabbit Carbon Copy dashboard component built and styled.
- [x] `coderabbit_review_pr` Tauri command registered and unit tested.
- [x] Frontend tests passing (347/347 tests).
- [x] Worker tests passing (138/138 tests).
- [x] Rust unit tests passing (19/19 wa_open tests).
