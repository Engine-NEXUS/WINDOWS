# NEXUS Home Server: 24/7 Multi-User Agent Mediator (Future Plan)

Status: **PLAN ONLY.** Nothing here is built, tested on the server, or committed to a schedule.
Date: 2026-11-06
Supersedes (in direction, not by deletion): [features/121](../features/121-nexus-cloud-multi-agent-mediator-and-telephony-plan.md), which assumed agents run only on friends' laptops and a local manager model does the planning.
Related: [features/156](../features/156-place-data-mediator-supabase-home-server-plan.md) (home server + Supabase + Cloudflare), [features/122](../features/122-agent-completion-watcher-and-keep-awake-architecture.md) (agent_watch), `docs/changes/144-cloudflare-free-device-links-foundation.md` (device links), `docs/changes/139-...` (watch-then-power).

Provenance: this document records an analysis conversation with Claude about the owner's repo and server idea. Statements about CLI config variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, XDG paths, `HOME`) come from general knowledge and were **not tested on the server**. Every one must be confirmed during Phase 0. Pricing/licensing statements were not verified on current vendor pages.

---

## 1. Goal in one paragraph

One always-on Windows machine (64 GB RAM, 12 GB VRAM GPU, 1 TB SSD) is both the owner's gaming PC and a private server for the owner's connected devices and a short list of enrolled users. A user gives a command on their laptop ("finish the login page and fix the tests"). The server plans the work with the **strongest model that user has access to**, hands the coding to that user's **cheaper fast model**, works in that user's own isolated environment, reviews and tests the result with the strong model, pushes it, and notifies the user on all their devices. If the laptop closes mid-task, work continues on the server through a git branch handoff.

## 2. Differences from plan 121

| Topic | Plan 121 | This plan |
|---|---|---|
| Who plans | Local model on the server (e.g. Qwen3 14B 4-bit, ~9-10 GB VRAM) | The user's own strongest cloud model |
| Who codes | Agent on the friend's laptop | User's cheap/fast model, on laptop or server |
| Where work runs | Friend laptops only | Laptop **and** server (git handoff) |
| GPU role | Central (manager model) | Almost none; optional small local model |
| Gaming conflict | Large (VRAM contention) | Small (game mode nearly unnecessary) |
| Cost to owner | GPU + power | Power only; each user pays their own model usage |

## 3. Target hardware and the OS decision

### 3.1 Windows edition: Windows 11 Pro, officially licensed

Why Pro over Home:
- **Update control.** Home restarts for updates on its own schedule, cutting users off at random. Pro allows deferral and active hours via Group Policy.
- **Remote Desktop host.** Only Pro can be the RDP host. Useful once the box sits headless.
- **Hyper-V and Windows Sandbox** for walling server parts off from the gaming setup and personal files.
- **BitLocker** drive encryption. The box will hold other people's tokens and logs.
- Gaming performance is the same on Home and Pro. Docker Desktop and WSL2 work on both.

Why replace the unofficial copy: the machine will hold other people's credentials and be reachable from the internet. Cracked images may carry tampered files or activator malware and may lose security updates. Acceptable for a gaming-only PC, not for this.

Licensing notes:
- Very cheap keys (roughly ₹500-1,500 / $10-15 on resale listings) are usually resold OEM or volume keys that can be blocked later. Buy from the Microsoft Store (microsoft.com/en-in) or a retail box from a known seller.
- If a valid Home licence already exists, the in-place Home-to-Pro upgrade (Settings > Activation) is cheaper than a full Pro licence.
- Current official India price was **not verified**; check the store.

Note: the dev laptop currently reports Windows 11 Home Single Language. That is the laptop, not the server; the recommendation concerns the server machine.

### 3.2 Server settings checklist
- Never sleep; high-performance power plan; display may sleep.
- BIOS: **restore on AC power loss = Power On** so the server returns after an outage.
- Auto-logon only if needed, otherwise services as Windows services/scheduled tasks at boot.
- Docker/WSL2 components set to start at boot.
- Active hours and update deferral via Group Policy; scheduled maintenance window.
- BitLocker on, recovery key stored off the machine.
- A UPS if affordable (protects SSD and in-flight git state).

### 3.3 Capacity sketch
- 64 GB RAM: each agent CLI is a Node/Rust process mostly waiting on the cloud, roughly 200-500 MB RAM and negligible GPU. 10+ simultaneous agents plus gaming is plausible. **Estimate, not measured.**
- 12 GB VRAM: free for games when planning/coding are cloud-side. Optional local model (e.g. gpt-oss-20b class) for cheap jobs only (summarising notifications, picking which model to use). If a local model is used it must unload when a game starts.
- CPU and disk are the real shared resources (Valhalla tile builds, git operations, test runs, container I/O). Apply per-container CPU/RAM limits.

## 4. Task lifecycle

```
Laptop (user)                        Server (per-user sandbox)                  Cloud models
─────────────                        ─────────────────────────                  ────────────
1. command spoken/typed
2. NEXUS creates task id, branch
   nexus/<task>, commits WIP, pushes
3. task request ───────────────────▶ 4. pull branch into user's repo dir
                                      5. PLAN: strongest planner model ──────▶ (user's account)
                                      6. CODE: cheap fast model follows plan ─▶ (user's account)
                                      7. REVIEW + RUN TESTS: planner model ───▶
                                      8. commit, push branch, mark done
9. notify on ALL user devices ◀──────
10. laptop opens → NEXUS pulls branch, tells user what was done
```

### 4.1 Model roles per user (examples)
- User A (Claude, Antigravity): plan/review with Claude's top model; code with an Antigravity flash-tier model.
- User B (Antigravity, OpenCode, Codex): plan/review with the strongest of their three (for example Codex at high reasoning); code with a flash-tier model through Antigravity or OpenCode.

### 4.2 Ownership rules
- **One owner per task at a time**: laptop or server, never both.
- If the user opens the laptop while the server is mid-task, NEXUS asks: let the server finish, or pause the server and pull the work back.
- State machine (proposal): `queued → planning → coding → reviewing → testing → done | failed | needs_login | needs_user | paused`. Reuse agent_watch signals for `needs_login` and completion.

### 4.3 Git handoff details to design
- Branch naming `nexus/<user>/<task-id>`.
- WIP commit must include untracked files that belong to the task; never commit secrets (`.env` excluded by a server-side check).
- Conflict policy when the laptop and server both touched the branch (should be prevented by the single-owner rule; detect and refuse anyway).
- The server pushes with a **per-user, per-repo-scoped token** (see §6).
- Squash/cleanup is the user's choice; never force-push.

## 5. Running separate CLI agents per user

The core claim: two Claude Code CLIs with different credentials can run side by side on one server, because each CLI reads its login from a configuration folder that can be redirected per process or per user.

| CLI | Default login location | Per-user override (to verify) | API-key alternative |
|---|---|---|---|
| Claude Code | `~/.claude` | `CLAUDE_CONFIG_DIR` | `ANTHROPIC_API_KEY` |
| Codex | `~/.codex` | `CODEX_HOME` | `OPENAI_API_KEY` |
| OpenCode | `~/.local/share/opencode` (+ config dir) | `XDG_DATA_HOME` / `XDG_CONFIG_HOME` | provider keys |
| Gemini CLI / Antigravity (`agy`) | `~/.gemini` | separate `HOME`/`USERPROFILE` | `GEMINI_API_KEY` |

Repo evidence: `src-tauri/src/agent_hooks.rs` and the AGENTS.md notes already record the `~/.gemini` and OpenCode plugin paths.

### 5.1 Environment-variable separation (minimum viable)
```
users/
  userA/ home/(.claude, .gemini)  repos/  logs/
  userB/ home/(.gemini, .codex, .local/share/opencode)  repos/  logs/
```
Example (illustrative, not tested here):
```bash
CLAUDE_CONFIG_DIR=/srv/nexus/userA/.claude claude -p "fix the login bug"   # in userA/repo
CLAUDE_CONFIG_DIR=/srv/nexus/userB/.claude claude -p "add dark mode"        # in userB/repo, at the same time
```
Weakness: all processes run as one OS user and can read every folder. A misbehaving agent could read another user's files.

### 5.2 Recommended: one sandbox per user
Docker container, WSL distro, or separate Windows user per person:
- Their CLIs, logins, repos and toolchains exist only inside it.
- User A's agent cannot see User B's files.
- Offboarding = delete the container/volume, wiping credentials.
- Per-container CPU/RAM caps so a user cannot starve gaming.
- Rate limits are per user account, so users do not slow each other.

### 5.3 Known snags
- **First login.** Subscription logins (Claude, Google for Antigravity) need a one-time browser sign-in. Do it inside the user's sandbox via a device-code or sign-in link. After that the CLI refreshes itself. API keys skip this.
- **Antigravity is primarily a desktop app.** Whether `agy` runs headless on Linux in a container is **unknown**. Fallback: a dedicated Windows user account on the server for that user's Antigravity tasks. Phase 0 must test this.
- Hook installation: `agent_hooks.rs` templates assume the owner's own home. Per-user sandboxes need hook installation into each sandbox's config dir.

## 6. Credentials, terms and isolation

### 6.1 Risks to decide on
1. **Storing someone's subscription login on the owner's machine** means the owner could technically act as them. Subscription plans may not permit running on another person's server. **Check each provider's current terms before enrolling anyone on a subscription login.**
2. **API keys are safer**: scope-limited, spend-capped, revocable by the friend at any time.
3. Either way: per-user encrypted credential store, never plaintext, never in logs.

### 6.2 Isolation requirements (must hold for every user)
- Separate filesystem roots, credentials, and logs.
- No shared agent memory or learning data across users (plan 121's warning on per-friend learning isolation stays).
- Server-wide audit log of everything sent on a user's behalf; user can read their own entries only.
- Revocation: owner can revoke a user or a single device token; revocation kills that user's running agents and offers credential wipe.

### 6.3 What a task needs on the server
- That user's GitHub access: a fine-grained token limited to their repos.
- Project secrets (`.env`) supplied by the user per project, stored in the user's encrypted store.
- Toolchains for the project's tests (Node, Python, Rust, etc.) installed inside the user's sandbox. **Each project needs one-time setup.**

### 6.4 Access control (carried from plan 121)
Enrolled people only: admin-issued key + TOTP (authenticator app) + per-device revocable tokens. Voice may *start* a revoke but a second factor must confirm. Phone-channel callers (if telephony is built) get a limited command set and must repeat back to confirm.

## 7. Reuse from the existing repo

| Need | Existing piece | Gap |
|---|---|---|
| Detect agent finish / needs login | `agent_watch.rs`, `agent_hooks.rs`, `agent_lifecycle` receipts (2026-11-05 reliability pass) | Runs on laptop only; no sandbox-aware installer |
| Access control, device tokens, revocation | Cloudflare Worker device links (`docs/changes/144`), approved-device list, revocation | No TOTP enrollment yet |
| Task transport laptop ↔ server | Worker signalling Durable Object, device links | No task-queue protocol or presence |
| Notifications | Worker SMS outbox + Termux gateway, NEXUS proactive policy, notify_hub | No multi-device fan-out per user |
| Unattended finish actions | `watch_runner.rs` watch-then-power | Not server-aware |
| Server scaffolding | `server/docker-compose.yml` (sidecar 8443, n8n 5678, Ollama commented) | See §8 |

Per the AGENTS.md notes, agent-watch has three known edge gaps (busy siblings before watch creation, provisional done hiding exit, OpenCode/Kilo resumed activity). The server flow inherits them until fixed.

## 8. Exposure and networking

Observed in the repo analysis:
- `server/docker-compose.yml` publishes sidecar 8443 and n8n 5678 on **all interfaces**, while the routing adapter correctly binds loopback only. The n8n webhook points at a Tailscale address (100.71.60.31), so something may already be running. **Not verified.**
- Plan 156 says Cloudflare Tunnel with **no open router ports**. Make the older stack match: bind to 127.0.0.1 and publish only through the tunnel (or Tailscale for owner-only admin).
- Prefer the Worker's existing device-token transport for task messages over a new WebSocket server on the box.

## 9. Gaming coexistence

- Cloud-side planning/coding means the GPU is mostly free; no "game mode" is required for the core design.
- Telephony and routing services do not use the GPU.
- If a local model is added: **game mode** = unload on game start (Ollama keep-alive 0 or a script), queue or reroute to a free cloud model until the game exits. Detect games via process list/fullscreen state.
- Cap sandbox CPU shares so test runs do not stutter games. Heavy jobs (tile builds, large test suites) can be scheduled outside gaming hours.
- Never run a user's untrusted code with access to the gaming user's files.

## 10. Optional components carried over from plan 121

- **Telephony at $0/month**: Jio SIM in a USB GSM modem (about $15, e.g. Huawei E173) with Asterisk `chan_dongle`; one call at a time, 2G/3G quality; Android bridging and eSIM were ruled out in the earlier research. STT/TTS via free cloud services so the GPU is not involved. Treat as optional and low priority; setup time is the open cost.
- **Presence**: each laptop holds a connection so the owner can see who is online.
- **Per-user learning**: only with strict isolation (§6.2).

## 11. Open decisions (owner)

1. **API keys only, or also subscription logins** (Claude Pro/Max, Google login for Antigravity)? Recommendation: API keys first; subscription logins only after reading provider terms.
2. **Handoff trigger**: always move to server when the laptop closes, or only on "continue on the server"? Recommendation: explicit opt-in per task first, automatic later.
3. **Notification channels**: NEXUS on other devices, SMS via Termux gateway, WhatsApp, or all? Recommendation: NEXUS device links first (already built), SMS second.
4. **Model selection**: user-chosen planner/coder, or NEXUS auto-picks from a strongest-to-cheapest ranking? Recommendation: auto-pick with per-user override.
5. Sandbox technology: Docker containers vs WSL distros vs Windows users (depends on Phase 0 `agy` test).
6. Whether to keep the optional local model at all.

## 12. Phased build plan

**Phase 0: verification spikes (no feature code)**
- Confirm each CLI's per-user config override actually isolates logins (run two Claude Code instances with different `CLAUDE_CONFIG_DIR` and different accounts in parallel; repeat for Codex, OpenCode, Gemini/Antigravity).
- Test whether `agy` runs headless in a Linux container; else define the Windows-user fallback.
- Inventory what is actually running on the server (OS build, GPU, Docker/WSL state, n8n/sidecar, Tailscale).
- Read provider terms on server-side use of subscription logins.
- Measure real RAM/CPU per idle and active agent.

**Phase 1: single-user server loop**
- One sandbox, one user (the owner), one agent. Git handoff laptop → server → laptop. Planner/coder split. Completion notification.

**Phase 2: multi-user isolation**
- Per-user sandboxes, encrypted credential store, scoped GitHub tokens, per-user logs, resource limits, offboarding wipe.

**Phase 3: enrollment and access**
- Admin key + TOTP, device tokens via Worker, revocation flow, voice-start/second-factor-confirm.

**Phase 4: orchestration quality**
- Model ranking and auto-pick, review/test gate definition ("done and tested"), failure handling, ownership conflict prompts, presence.

**Phase 5: optional extras**
- Local small model with game mode, telephony modem, per-user learning with isolation.

Each phase needs its own verification record in `docs/testing/` before the next begins, following the project's existing rule that source-tested work is not production-ready.

## 13. Risks summary

| Risk | Severity | Mitigation |
|---|---|---|
| Subscription credentials on shared host (terms, impersonation ability) | High | API keys first, per-user encrypted store, read provider terms |
| Cross-user leakage | High | One sandbox per user, separate logs/creds, no shared memory |
| Exposed ports | High | Loopback binding + Cloudflare Tunnel/Tailscale, no router ports |
| Gaming stutter from CPU/disk load | Medium | Container limits, scheduling, optional game mode |
| `agy` not headless | Medium | Windows-user fallback, Phase 0 test |
| Unverified CLI override variables | Medium | Phase 0 tests before design commits |
| Cracked OS | High | Official Windows 11 Pro |
| Single-owner rule violated (laptop and server editing same branch) | Medium | Task ownership state + refusal on conflict |
| Agent-watch edge gaps | Low-Medium | Fix per the 2026-11-06 cross-check before relying on server completion signals |

## 14. Not claimed

No performance numbers, capacity limits, vendor terms, prices, or CLI behaviours in this document were measured or verified on the server. The existing project rule applies: no readiness-score change and no queue advancement from planning documents.
