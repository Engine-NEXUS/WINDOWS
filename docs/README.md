# NEXUS Documentation Index

> NEXUS is a Windows-first voice desktop assistant: a Tauri v2 / Rust client with a React UI, a Cloudflare Worker backend, and on-demand local Python helpers (speech fallback, intent classifier).
> **Where the truth lives:** [`../AGENTS.md`](../AGENTS.md) is the engineering log — newest entries first, each with *Verify* and *Not run live* lines. These docs add design, research and plans around it. Where a document disagrees with `AGENTS.md` or the code, trust those.

---

## Start here

- **Notification-to-sidebar proposal (2026-10-28):** [platform research](research/notification-to-sidebar/README.md), [detailed phased plan](features/notification-to-sidebar-implementation-plan.md), and [reusable coding prompt](features/notification-to-sidebar-implementation-prompt.md). Planning only; no new UI, runtime or queue change.

- **Detailed work handover (2026-10-28):** [Windows readiness and Voice Lock research, plans, edits, file inventory, test/release history, failures and remaining gates](research/windows-readiness-voice-lock-handover-2026-10-28/README.md). Five linked documents; historical results are source-attributed and no new readiness claim is made.

- **Current Voice Lock work:** [implementation plan](features/152-voice-lock-evidence-and-reliability-plan.md), [Architect-guided local research](research/elevenlabs-architect-guided-nexus-review.md), and [live work card](testing/speaker-verification-work-card.md). The P1–P6 source work and evidence tools are documented; actual corpus, exact-release and device acceptance gates remain open.

1. [`../README.md`](../README.md) — what NEXUS is, what leaves the device, how to build.
2. [`../QUICKSTART.md`](../QUICKSTART.md) — install, first launch, things to try.
3. [`../AGENTS.md`](../AGENTS.md) — current state, pitfalls, and what has / has not been verified live.
4. [architecture/07-central-orchestrator.md](architecture/07-central-orchestrator.md) — the turn pipeline: one owner for request lifecycle, routing, cancellation.
   [architecture/12-command-center-alignment.md](architecture/12-command-center-alignment.md) records the current action-center contract and compound execution boundary.
5. [memory-core/README.md](memory-core/README.md) — the on-device memory, Google sign-in and WhatsApp watcher.
6. [research/deployment/](research/deployment/) — production-readiness paper, English-only execution plan, rollout rings runbook.

---

## Folder map

```
docs/
├── architecture/      # Current design docs 05–11 (state machine, orchestrator, OAuth, GitHub subcommands)
├── features/          # Plans and specs, numbered 01–140 (two numbering eras — see its README)
├── changes/           # Per-change records 01–135 and CHANGELOG.md
├── research/          # Research by topic: wake word, STT/TTS, NLU, ghost mode, hand mode, memory,
│                      #   deployment, jarvis-landscape, maps, captions, orb, performance, ...
├── memory-core/       # Memory Core guide (P0–P6), Google sign-in, limits and risks
├── wake-word/         # openWakeWord pipeline, training, validation
├── testing/           # Evaluation locks, NLU/wake experiment records, data-foundation gates
├── mcp/               # MCP servers, auth vault, connect cards
├── credentials/       # OAuth / device identity notes (02, 04, 05, 06 partly superseded)
├── 9router-research/  # LLM cascade (Cerebras → Groq → Gemini) research
├── commands/          # CLI (`nexus ...`) reference
├── meeting-protection/# Mic suppression during calls
├── phone/             # Phone-link notes
├── reviews/           # Reviews and retrospectives (historical)
├── archive/           # Superseded sidecar-era docs and old snapshots — history only
├── installer-testing-checklist.md   # Installer test checklist
└── privacy.html, terms.html         # Copies of the root legal pages
```

## Current vs. archived

The project began as a "thin client + fat server" design (n8n, Ollama, faster-whisper, Piper, a Python sidecar holding encrypted keys). It became serverless (Cloudflare Worker) with Groq speech-to-text, Edge text-to-speech, and on-device wake word, intent parsing and memory. Documents describing the first design live in [archive/](archive/README.md). Several other documents carry a banner saying they are partly superseded.

## Domain catalogs

| Domain | Entry point | Notes |
|---|---|---|
| Architecture | [architecture/README.md](architecture/README.md) | Docs 05–11 are current; 01–04 are archived |
| Features | [features/README.md](features/README.md) | Index of all feature docs, including late additions |
| Changes | [changes/CHANGELOG.md](changes/CHANGELOG.md) | Chronological; detailed records are numbered files |
| Memory Core | [memory-core/README.md](memory-core/README.md) | What is stored, what leaves the device, limits |
| Wake word | [wake-word/README.md](wake-word/README.md) | Pipeline and training; production plan is feature 107 |
| Testing | [testing/README.md](testing/README.md) | Evaluation locks and gates |
| MCP | [mcp/README.md](mcp/README.md) | Status page 00 is a 2026-09-18 snapshot |
| Research | [research/README.md](research/README.md) | Many sub-folders; start with deployment/ |
| Ghost mode | [research/ghost-mode/README.md](research/ghost-mode/README.md) and [features/70-ghost-mode-complete.md](features/70-ghost-mode-complete.md) | Desktop control program |
| Hand mode | [features/118-hand-mode-master-compendium.md](features/118-hand-mode-master-compendium.md), [production refactor plan](features/150-hand-mode-production-refactor-implementation-plan.md), [research](research/hand-mode/02-production-refactor-research-2026-10-21.md) | Current interaction record; planned safety, latency, and live release gates |
| Voice, connectivity, RAM | [features/136-voice-connectivity-and-ram-master-compendium.md](features/136-voice-connectivity-and-ram-master-compendium.md) | Owner decisions at the end |
| Action Kernel | [features/140-agent-grade-computer-control-research-and-nexus-action-kernel-plan.md](features/140-agent-grade-computer-control-research-and-nexus-action-kernel-plan.md) | Voice file control, quarantine and undo |
| Production readiness | [research/deployment/](research/deployment/) | SLOs, failure modes, rollout runbook |

## Runtime processes and ports

| Service | Address | Technology | Role |
|---|---|---|---|
| NEXUS app | — | `nexus.exe` (Rust + WebView2) | Wake word, hotkeys, intent parsing, memory, local actions, UI |
| Local STT fallback | `127.0.0.1:39217` | Moonshine (Python, started on demand) | Offline speech-to-text; Groq cloud STT is primary |
| Local NLU | `127.0.0.1:39218` | BERT-Mini ONNX (Python, on demand) | Intent classification fallback |
| Admin brain (admin builds) | `127.0.0.1:39219` | Qwen2.5-0.5B via llama.cpp | Classification, compound planning |
| Webhook trigger | `127.0.0.1:39220` | Rust, bearer token | External triggers |
| Agent-watch sidecar | `127.0.0.1:39222` | Node, token + Host check | Watches coding agents for completion |
| WhatsApp / Amazon bridges | `127.0.0.1:8765`, `:8766` | Local MCP servers | Optional |
| Cloudflare Worker | cloud | `server/worker/` | Device identity, OAuth tokens, quotas, cloud reasoning, OTA models |

Cloud services in use: Groq (speech-to-text, LLM), Deepgram (optional), Cerebras and Gemini (LLM, vision), Microsoft Edge TTS, GitHub and Google APIs. The full list of what is sent is in [`../privacy.html`](../privacy.html).
