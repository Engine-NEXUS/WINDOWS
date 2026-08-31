# NEXUS — Voice-First AI Desktop Assistant & Architecture Mapper

A cross-platform, Siri-like floating overlay assistant designed specifically for software engineers. NEXUS combines a **native desktop client** (Tauri v2 + Rust + React/TS) with a **serverless edge backend** (Cloudflare Workers + D1) for blazing-fast, privacy-respecting AI interactions.

NEXUS was built to answer the ultimate developer question: *"If I change this, what breaks?"* — and was submitted for the **Autonomous Codebase Architecture Mapper** Hackathon.

> **Hackathon Jury:** Please read our official submission document at [docs/HACKATHON_SUBMISSION.md](docs/HACKATHON_SUBMISSION.md).

---

## Key Features

### Autonomous Architecture Mapper (Hackathon Highlight)
* **Instant Layering:** Analyzes any GitHub repo and clusters files into architectural layers (Frontend, Backend, DB, etc.) in under 10 seconds.
* **Deep Dependency Graph:** Clones the repo, parses AST imports in parallel via rayon, and builds a directed petgraph mapping out every file dependency.
* **Impact Analysis (Blast Radius):** Runs sub-10ms Reverse-BFS to show you exactly which files break if you modify a target file, reconstructing the shortest dependency paths.
* **Risk Scoring:** Automatically detects circular dependencies (via Tarjan's SCC) and architectural hotspots (in_degree centrality).

### Advanced Voice Pipeline
* **Local Wake Word:** openWakeWord (ONNX in Rust) detects "NEXUS" locally with <0.5% CPU overhead. No audio is streamed until you wake it.
* **In-Process STT:** Moonshine Tiny (ONNX) runs inside the Rust process — no Python sidecar, no IPC latency.
* **Smart VAD:** Silero Voice Activity Detection with a dynamic silence gate and Automatic Gain Control (AGC) ensures perfect cutoffs.
* **Multi-Voice TTS:** Fish Audio (s2.1-pro-free) with Jarvis/Ethan/Nova voices, Web Speech API fallback.
* **NLU Fallback:** BERT-Mini ONNX intent classifier (lazy Python sidecar) backs up the deterministic parser.

### Developer-First Integrations
* **Voice-Triggered PR Reviews:** *"NEXUS, analyze PR 5 in servx."* Fetches diffs, commits, and comments, returning a Senior-Engineer grade review right to your sidebar.
* **Fuzzy Repo Matching:** Intelligent Levenshtein matching on the edge catches STT mishearings (e.g., "service" instead of "servx").
* **Linux MPRIS:** Native D-Bus media controls integrated directly via zbus.

### Stunning Native UI
* **Non-Activating Overlay:** Floats above your IDE without stealing keyboard focus.
* **Liquid Frosted Glass:** Screenshot-capture blur backdrop for a genuine frosted-glass look.
* **Streaming Text Animations:** Cursor-style text rendering with sequential word fade-ins.

---

## Architecture Overview

NEXUS is fully **Serverless**:

```
NEXUS laptop -> HTTP POST -> Cloudflare Worker -> APIs -> Text Response
                              |
                              -> D1 Database (OAuth tokens, Device registration)
                              -> Workers AI (Intent classification)
```

No sidecar, no n8n, no heavy local LLMs required.

> **Read the full feature log:** [docs/NEXUS_FEATURES_IMPLEMENTED.md](docs/NEXUS_FEATURES_IMPLEMENTED.md)

---

## Quick Start (One Command — All Platforms)

NEXUS ships with a unified cross-platform developer command (`nexus`).
It auto-installs all prerequisites (Rust, Node.js, Python, system libs)
and builds the app in one step on **Windows, macOS, and Linux**.

### First time (clone + install + build + global command)

```bash
git clone <repo-url> ULTRON
cd ULTRON

# Windows:
nexus install

# macOS / Linux:
./nexus install
```

`nexus install` will:
1. Detect your OS and install missing tools (Rust, Node, Python, LLVM, system libs)
2. Install frontend + Worker + NLU Python dependencies
3. Build the release binary
4. Install a **global `nexus` command** so you can run it from any directory

After `nexus install`, the `nexus` command is available everywhere — no need to `cd` into the repo.

### Start the app

```bash
nexus start      # all platforms (after install)
```

### Develop with hot reload

```bash
nexus dev
```

### Rebuild after changes

```bash
nexus build
```

### All commands

| Command | What it does |
|---------|-------------|
| `nexus install` | Install prerequisites + build + global `nexus` command (first time) |
| `nexus setup` | Install prerequisites + build (no global command) |
| `nexus build` | Build frontend + Rust release binary |
| `nexus dev` | Tauri dev mode (hot reload via Vite) |
| `nexus start` | Launch the built release binary (alias: `nexus run`) |
| `nexus check` | Diagnostics (tools, frontend, Rust, NLU, Worker) |
| `nexus clean` | Remove build artifacts |
| `nexus worker` | Deploy the Cloudflare Worker (optional, self-host backend) |
| `nexus help` | Show help |

### Prerequisites (if you prefer manual install)

* **Node.js** 20+
* **Rust** toolchain (https://rustup.rs)
* **Python** 3.12+ (for the NLU server)
* **Windows:** LLVM/libclang (for bindgen), MSVC C++ Build Tools
* **macOS:** Xcode Command Line Tools
* **Linux:** `libwebkit2gtk-4.1-dev libgtk-3-dev libasound2-dev libssl-dev pkg-config` (apt) or equivalents

---

## Production Build (Installers)

For signed installers (NSIS .exe, notarized .dmg, AppImage + .deb):

```bash
# Windows:
pwsh ./scripts/build.ps1

# macOS / Linux:
./scripts/build.sh
```

Set `NEXUS_SERVER_URL` before building to bake in the Cloudflare Worker URL:

```bash
export NEXUS_SERVER_URL="https://nexus-worker.your-subdomain.workers.dev"
./scripts/build.sh   # or pwsh ./scripts/build.ps1
```

---

## Cloudflare Worker Backend (Optional)

NEXUS uses a Cloudflare Worker for intent classification, OAuth, and API calls.
Contributors can either:
1. **Use the shared Worker URL** (set `NEXUS_SERVER_URL` in your env), or
2. **Self-deploy** their own Worker:

```bash
cd server/worker
npm install
npx wrangler login
npx wrangler d1 create nexus-db       # paste ID into wrangler.toml
npx wrangler d1 execute nexus-db --file=schema.sql --remote
npx wrangler secret put GOOGLE_CLIENT_ID
npx wrangler secret put GOOGLE_CLIENT_SECRET
npx wrangler secret put GITHUB_CLIENT_ID
npx wrangler secret put GITHUB_CLIENT_SECRET
npx wrangler deploy
```

See [server/worker/README.md](server/worker/README.md) for full details.

---

## License

Proprietary (c) 2026 NEXUS.
