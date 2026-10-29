# NEXUS Assistant — Feature Implementation Index

Plans, specifications and records for every feature, from the original `prem224k`/`prem22k` merge (2026-08) through the Action Kernel (2026-10). The tables below grew by phase, so they are grouped by period rather than strictly numeric.

> **Numbering note.** Two numbering sequences were used at different times, so some numbers appear twice (01–16, 25, 28, 29, 40–43, 73, 75, 87, 94, 96, 98, 101, 126, 129). Files are referenced by number from `AGENTS.md` and `docs/changes/`, so they are not renamed; use the full filename. Items marked *(superseded)* are kept for history. For current behaviour, `AGENTS.md` at the repository root is authoritative.
>
> Files that predate this index but were missing from it are listed in **Additional documents** at the bottom.

---
## Proposed notification-to-sidebar interaction

- [Detailed N0–N6 implementation plan](notification-to-sidebar-implementation-plan.md) — exact-event routing, compact top card, sidebar handoff, work-area geometry, policy, native fallback and acceptance tests. Proposal only.
- [Reusable source-only coding prompt](notification-to-sidebar-implementation-prompt.md) — first assignment limited to N0/N1; GLM editor and supervisor review/testing; owner retains runtime/build control.
- [Platform research and source investigation](../research/notification-to-sidebar/README.md).

## Current Hand Mode refactor

- [150 — Hand Mode production refactor implementation plan](150-hand-mode-production-refactor-implementation-plan.md) — ordered safety, feedback, frame freshness, output, tuning, and real-hardware release gates. Status: plan only.
- [Hand Mode production refactor research](../research/hand-mode/02-production-refactor-research-2026-10-21.md) — source-backed defects, uncertain performance claims, and measurement contract.

## Focus Mode, owner-less GitHub commands and notification state (2026-10-25/26)

- [153 — Focus Mode, owner-less GitHub commands and the general notification brain](153-focus-mode-github-resolution-and-notification-programme.md) — plan, owner decisions, as-built design for four tracks (Hand Mode Focus Mode with exact restore, repo/PR resolution across user and org repos, failed/still failing/fixed notification state machine with a GitHub Actions watcher, 65 generated feature work cards), deviations, open work and the live verification protocol. Status: built and unit-tested, not live-proven; Vercel/Render/Netlify watchers designed, not built. Evidence: [research record](../research/focus-github-notification-research-2026-10-26.md); as-built index: [session overview](../changes/session-2026-10-25-focus-github-notifications-overview.md).

## Notification brain (planning 2026-10-23; Phase A+B built 2026-10-24)

- [151 — Notification brain: event keys, cooldown and short notifications](151-notification-brain-event-key-cooldown-and-short-notifications-plan.md) — code-verified root causes of repeated/verbose notifications; event-key + 5-min cooldown + hold-and-merge design through `proactive_policy`; all-category short templates; zero-AI-credit audit; borrowed patterns from ntfy/Apprise/Apple Intelligence. Status: plan only (owner-reviewed 2026-10-23); phases A+B built: [changes/157](../changes/157-notification-brain-phase-a-b.md), phase D built: [changes/158](../changes/158-notification-brain-phase-d-one-liners.md), phase E built: [changes/159](../changes/159-notification-brain-phase-e-funnel.md).

## Feature Index

- [152 — Voice Lock evidence and reliability](152-voice-lock-evidence-and-reliability-plan.md) — P1 capture accounting building first; subsequent native-state UI, model benchmarking, transcription diagnosis and real-device acceptance gates. Research is a local application of ElevenLabs Architect guidance, not a remote Architect opinion.

### Voice & PR Analysis (prem224k — this session)

| # | Feature | File | Description |
|---|---|---|---|
| 01 | GitHub PR Analysis via Voice | [01-github-pr-analysis.md](01-github-pr-analysis.md) | Say "analyse PR 5 in servx" → GLM-4.7-Flash code review in sidebar |
| 02 | STT Mishearing Fixes | [02-stt-mishearing-fixes.md](02-stt-mishearing-fixes.md) | Post-processing corrections + dynamic hotwords for tiny.en |
| 03 | Sidebar Streaming Text Animation | [03-sidebar-streaming-animation.md](03-sidebar-streaming-animation.md) | ChatGPT/Gemini-style word fade-in, left-to-right, top-to-bottom |
| 04 | On-It-Sir Flow | [04-on-it-sir-flow.md](04-on-it-sir-flow.md) | Immediate ack for long queries → orb hides → "Here is the analysis" |
| 05 | Worker Fuzzy Repo Matching | [05-worker-fuzzy-repo-matching.md](05-worker-fuzzy-repo-matching.md) | Levenshtein distance matching for misheard repo names |

### Wake-Word & Hotkey (prem224k)

| # | Feature | File | Description |
|---|---|---|---|
| 06 | Wake-Word Reliability | [06-wake-word-reliability.md](06-wake-word-reliability.md) | Single-frame high-confidence trigger (0.5+ bypass) |
| 07 | State-Dependent Hotkey | [07-state-dependent-hotkey.md](07-state-dependent-hotkey.md) | Sidebar-aware: close sidebar OR wake, not both |
| 08 | CSP for Silero VAD | [08-csp-silero-vad.md](08-csp-silero-vad.md) | CDN script-src + worker-src blob for VAD WASM |

### Voice Engines & Cross-Platform (prem22k)

| # | Feature | File | Description |
|---|---|---|---|
| 09 | Multi-Voice TTS Engine | [09-multi-voice-tts.md](09-multi-voice-tts.md) | Gemini Flash, Fish Audio Ethan, ElevenLabs Jarvis/Nova/Echo/Onyx |
| 10 | Non-Activating Overlay | [10-non-activating-overlay.md](10-non-activating-overlay.md) | Orb doesn't steal keyboard focus from IDEs/terminals |
| 11 | Linux D-Bus MPRIS | [11-linux-mpris.md](11-linux-mpris.md) | Native media control via zbus (PlayPause, Next, Previous) |
| 12 | VAD Post-TTS Mute Gate | [12-vad-post-tts-mute.md](12-vad-post-tts-mute.md) | 300ms mic mute after TTS to prevent echo self-triggering |

### CI/CD & Setup (prem22k)

| # | Feature | File | Description |
|---|---|---|---|
| 13 | GitHub Actions CI/CD | [13-github-actions-cicd.md](13-github-actions-cicd.md) | Auto-build Windows NSIS .exe installer on push |
| 14 | Setup Wizard Redesign | [14-setup-wizard-redesign.md](14-setup-wizard-redesign.md) | Voice persona selection, API keys, preferences |

### Architecture & Merge

| # | Feature | File | Description |
|---|---|---|---|
| 15 | Architecture Decisions | [15-architecture-decisions.md](15-architecture-decisions.md) | Serverless model, sidebar delivery, done event timing |
| 16 | Conflict Resolution | [16-conflict-resolution.md](16-conflict-resolution.md) | How 3 overlapping files were merged from both branches |

### AK Repo Port (2026-08-29 — this session)

| # | Feature | File | Description |
|---|---|---|---|
| 17 | Mic Baton Pass | [17-ak-port-mic-baton-pass.md](17-ak-port-mic-baton-pass.md) | Pause/resume cpal stream around getUserMedia to fix Intel SST mic lock |
| 18 | Cancel Hotkey + Double Wake Fix | [18-ak-port-cancel-hotkey-double-wake-fix.md](18-ak-port-cancel-hotkey-double-wake-fix.md) | Ctrl+Space cancel hotkey + fix triple event emission causing "on it sir" twice |
| 19 | Audio Volume + Multi-Turn VAD | [19-ak-port-audio-volume-multi-turn-vad.md](19-ak-port-audio-volume-multi-turn-vad.md) | RMS volume tracking for avatar reactivity + "didn't catch that" retry (max 3) |
| 20 | STT Fix + Wake Reliability | [20-stt-fix-wakeword-reliability.md](20-stt-fix-wakeword-reliability.md) | STT server missing __main__ block + wake word model assessment |
| 21 | Liquid Glass Sidebar | [21-liquid-glass-sidebar.md](21-liquid-glass-sidebar.md) | Screenshot-capture blur (GDI BitBlt + Rust blur) for non-activating windows + pending content pattern for dynamic windows |
| 22 | Worker AI Latency Optimization | [22-worker-ai-latency-optimization-plan.md](22-worker-ai-latency-optimization-plan.md) | Plan to reduce Worker AI response from 40s to 3-4s via prompt truncation, model tiering, GitHub API caching, and SSE streaming |
| 23 | TTS Voice Research | [23-tts-voice-research-elevenlabs-vs-fish.md](23-tts-voice-research-elevenlabs-vs-fish.md) | ElevenLabs vs Fish Audio free tier analysis — Fish Audio s2.1-pro-free recommended (free API, ~100ms TTFA, voice cloning) |
| 24 | TTS Deep Research (All Providers) | [24-tts-deep-research-all-providers.md](24-tts-deep-research-all-providers.md) | 10 TTS providers compared — Kokoro (self-host, Apache 2.0, unlimited), Fish Audio, Google Cloud, Polly, Piper, Azure, HuggingFace, Coqui, ElevenLabs, TTSMaker |
| 25 | Rich Repo Analysis Dashboard | [25-rich-repo-analysis-dashboard.md](25-rich-repo-analysis-dashboard.md) | GLM-4.7-flash (free) + GitHub languages API + pie charts (languages/frameworks) + databases + features + top bar heading |
| 26 | Wake-Word Training Guide | [26-wake-word-training-guide.md](26-wake-word-training-guide.md) | How to train a custom openWakeWord ONNX model for "nexus" with TTS samples + negative samples |
| 27 | Loading Indicator Overlay | [27-loading-indicator-overlay.md](27-loading-indicator-overlay.md) | Transparent click-through Lottie animation at top-right corner, shown during Worker processing after "On it sir" |
| 28 | System Volume Control Research | [28-system-volume-control-research.md](28-system-volume-control-research.md) | Deep research on programmatic volume control across Windows (Core Audio COM), macOS (CoreAudio), and Linux (PipeWire/PulseAudio/ALSA) — permissions, APIs, Rust integration |
| 29 | TTS Auto-Volume Plan | [29-tts-auto-volume-plan.md](29-tts-auto-volume-plan.md) | Plan: before NEXUS speaks, set system volume to configured level (default 75%), restore after TTS completes — Windows COM, macOS CoreAudio, Linux shell |

### NLU Training & Voice Collection (2026-09-12)

| # | Feature | File | Description |
|---|---|---|---|
| 51 | NLU Training & Voice Collection | [51-nlu-training-and-voice-collection.md](51-nlu-training-and-voice-collection.md) | Complete guide for `nexus collect` (voice sample collector) and `nexus train` (full BERT-Mini retraining pipeline) — 52 intents, 45 slot types, data flow, troubleshooting, admin brain integration |

### Research System (2026-09-01)

| # | Feature | File | Description |
|---|---|---|---|
| R01 | Research Sources | [research/01-research-sources.md](research/01-research-sources.md) | 9 ad-free research sources (Wikipedia, Wikidata, DDG, knowledgelib, SearchX, Tavily, Google CSE, Serper, Wolfram, Semantic Scholar) |
| R02 | LLM Cascade | [research/02-llm-cascade.md](research/02-llm-cascade.md) | Gemini Flash Lite (1,500/day) → Groq Qwen 3.8 (14,400/day) → Cloudflare llama-3.2-3b (~200/day) |
| R03 | API Keys & Secrets | [research/03-api-keys-and-secrets.md](research/03-api-keys-and-secrets.md) | Every API key, where to get it, free tier details, Cloudflare secret setup |
| R04 | Cascade Architecture | [research/04-cascade-architecture.md](research/04-cascade-architecture.md) | Full retrieval + synthesis flow, decision trees, prompt construction |
| R05 | Capacity Analysis | [research/05-capacity-analysis-10-users.md](research/05-capacity-analysis-10-users.md) | Whether each free tier survives 10 active users — 153x LLM headroom |
| R06 | Latency Benchmarks | [research/06-latency-benchmarks.md](research/06-latency-benchmarks.md) | Measured end-to-end latency for every command type, from live tests |
| R07 | Deployment Guide | [research/07-deployment-guide.md](research/07-deployment-guide.md) | Step-by-step deploy: secrets, D1 schema, KV namespace, wrangler config |
| R-NLU-01 | External NLU Data Sources | [../research/external-nlu-data-sources-and-acquisition-plan-2026-09-13.md](../research/external-nlu-data-sources-and-acquisition-plan-2026-09-13.md) | Ranked comparison of public datasets, opt-in user telemetry, custom speech vendors, scraping/legal constraints, privacy architecture, ingestion quality gates, and phased NEXUS data plan |
| R-NLU-02 | BERT-Mini Deep Audit | [../research/nlu-model-and-dataset-deep-audit-2026-09-14.md](../research/nlu-model-and-dataset-deep-audit-2026-09-14.md) | Structural and ONNX audit, before/after metrics, repaired slot training, remaining OOS/live-intent gaps, and prioritized retraining plan |
| R-NLU-03 | Code vs Research Accuracy Gap | [../research/nlu-intent/nlu-wakeword-code-vs-research-accuracy-gap-2026-09-14.md](../research/nlu-intent/nlu-wakeword-code-vs-research-accuracy-gap-2026-09-14.md) | NLU and wake implementation compared with primary calibration, OOS, joint-SLU, and keyword-spotting research; ranked low-RAM improvements |
| R-NLU-LIVE | Latest NLU Audit | [../research/nlu-model-data-audit-latest.md](../research/nlu-model-data-audit-latest.md) | Generated by `nexus audit` with current dataset and model metrics |
| T-NLU-01 | NLU Future Testing Playbook | [../testing/nlu-future-testing-and-model-promotion-playbook.md](../testing/nlu-future-testing-and-model-promotion-playbook.md) | Permanent test procedure: baseline metrics, intent/slot/OOS/safety/voice suites, external-data gates, candidate comparison, model promotion, rollback, and report templates |
| R08 | Testing Results | [research/08-testing-results.md](research/08-testing-results.md) | 13 live test queries with timings, provider routing, and output quality |
| R09 | Intent Routing Fixes | [research/09-intent-routing-fixes.md](research/09-intent-routing-fixes.md) | Bug fixes: "research" keyword, isSearchQuestion, isAcademicQuery, reasoning leakage |
| R10 | Future Improvements | [research/10-future-improvements.md](research/10-future-improvements.md) | Pending keys, streaming, parallel racing, cache versioning, scaling to 100+ users |

---

## Quick Summary

### prem224k (2 commits + this session's work)
- GitHub PR analysis via voice commands
- STT post-processing for misheard words
- Sidebar streaming text animation
- "On it sir" → "Here is the analysis" flow
- Fuzzy repo name matching (Levenshtein)
- Wake-word single-frame high-confidence trigger
- State-dependent hotkey
- CSP for Silero VAD CDN

### prem22k (20 commits)
- Multi-voice TTS engine (6 voices, 3 providers)
- Non-activating floating overlay window
- Linux D-Bus MPRIS media control
- Dual-phase VAD post-TTS mute gate
- GitHub Actions CI/CD pipeline
- Setup wizard redesign
- Pop!_OS/WebKitGTK fixes
- Multi-hotkey binding for Linux
- Single-instance daemon lock
- NSIS installer auto-launch setup

### Merge conflicts (3 files, all resolved)
- `hotkey.rs` — Combined multi-hotkey + state-dependent logic
- `wakeword_oww.rs` — Took prem224k's precise high-confidence approach
- `tauri.conf.json` — Combined visible:true + CSP changes

### AK repo port (4 features, all implemented and tested)
- Mic baton pass (pause/resume cpal stream for Intel SST compatibility)
- Cancel hotkey (Ctrl+Space) + double "on it sir" fix (triple event emission)
- Audio volume RMS tracking + multi-turn VAD resume + "didn't catch that" retry
- STT server missing `__main__` block fix + wake word reliability assessment

### Architecture Mapper Polish (2026-09-02)

| # | Feature | File | Description |
|---|---|---|---|
| 33 | Architecture Mapper Voice → Loading Flow | [33-architecture-mapper-voice-loading-flow.md](33-architecture-mapper-voice-loading-flow.md) | "Open architecture mapper" → immediate "On it sir" → orb hides → loading.json at top-right → background analysis → window opens when ready |
| 34 | Architecture Mapper Layout Redesign | [34-architecture-mapper-layout-redesign.md](34-architecture-mapper-layout-redesign.md) | 3 view tabs (Files/Hotspots/Cycles) in header, full-width map, 2-row bottom section (file inspector + analytics) |
| 35 | Custom-Protocol Build Requirement | [35-custom-protocol-build-requirement.md](35-custom-protocol-build-requirement.md) | Frontend changes require Rust binary rebuild (`cargo build --release --features custom-protocol`) — frontend is embedded in the binary |

### Settings Sidebar Research (2026-09-06)

| # | Feature | File | Description |
|---|---|---|---|
| 49 | Settings Sidebar Research | [49-settings-sidebar-research.md](49-settings-sidebar-research.md) | Research for unified settings sidebar: Google/GitHub OAuth, Gemini/Groq API keys, TTS volume slider, orb position/size sliders |
| 50 | Orb Position + Size Persistence Plan | [50-orb-position-persistence-plan.md](50-orb-position-persistence-plan.md) | Detailed plan for orb position/size sliders with 100% persistence assurance — saves to settings.json, overwrites on new save |

### Interactive Plan Approval & Voice Confirmation (2026-09-21)

| # | Feature | File | Description |
|---|---|---|---|
| 54 | Interactive Voice Approval & Confirmation Sidebar | [54-interactive-voice-approval-and-confirmation-sidebar.md](54-interactive-voice-approval-and-confirmation-sidebar.md) | Automatic 5s voice approval listening window with early-exit instant execution (<2s), persistent Response Sidebar confirmation card, and dual voice/mouse controls |
| 55 | NLU Data Foundation, STT Conditioning & Voice Scaling | [55-nlu-data-perfection-voice-scaling-and-mcp-bridge-research.md](55-nlu-data-perfection-voice-scaling-and-mcp-bridge-research.md) | STT Whisper prompt conditioning (`temperature=0.0`), decoupled acoustic vs NLU text pipeline, 70/30 data principle, and OTA model distribution |
| 56 | Targeted Intent Training & MCP Data Promotion | [56-targeted-intent-training-and-mcp-data-promotion.md](56-targeted-intent-training-and-mcp-data-promotion.md) | Category-specific interactive voice collection (`nexus collect -c -i`), full 55-intent coverage, zero-quarantine promotion, and aligned intent mastery |
| 57 | Apex Wake Word Evolution & Hardware Adaptation | [57-apex-wake-word-evolution-and-hardware-adaptation.md](57-apex-wake-word-evolution-and-hardware-adaptation.md) | Spectral microphone prober, chassis resonance auto-tuning (128.3 Hz HPF), dynamic AGC pre-gain, phantom trigger elimination, and multilingual negative augmentation |
| 62 | Full-Codebase Audit & Top-5 Critical Fixes | [62-full-codebase-audit-and-top-5-critical-fixes.md](62-full-codebase-audit-and-top-5-critical-fixes.md) | 5-agent audit (6 critical / 5 high / ~20 medium); fixed serde wire tags, NLU OTA pipeline, Moonshine production STT, root package.json, ghostwriter abort, worker quota double-count |
| 63 | Single-Stage Shell & Blackout Policy | [63-single-stage-shell-and-blackout-policy.md](63-single-stage-shell-and-blackout-policy.md) | Transparent overlay window, pixel-identical geometry mapping, dynamic hitbox toggling, and watchdog blackout auto-recovery |
| 64 | Ghost Mode Windows Control Plan | [64-ghost-mode-plan.md](64-ghost-mode-plan.md) | AI drives the real Windows cursor while user speaks, stage ring visualization, keyboard-first Win-search flows, takeover rails |
| 65 | Dual-Target Wake Word Balancing & Engine Alignment | [65-dual-target-wake-word-balancing-and-rust-engine-alignment.md](65-dual-target-wake-word-balancing-and-rust-engine-alignment.md) | Balanced positive sample training, 1.5s cold-boot grace, VAD onset chopping fix, and SHA-256 manifest synchronization |
| 66 | Hey Jarvis vs. NEXUS Benchmark & Multi-Syllabic Parity | [66-hey-jarvis-vs-nexus-comparative-acoustic-benchmark-and-multi-syllabic-parity.md](66-hey-jarvis-vs-nexus-comparative-acoustic-benchmark-and-multi-syllabic-parity.md) | Empirical comparison vs openWakeWord Jarvis, 22.05 kHz ingestion bug resolution, universal resampling, and 0.00% FA multi-syllabic parity |
| 67 | Instant Neural Wake Alignment & Clean Console UX | [67-instant-neural-wake-alignment-and-clean-console-ux.md](67-instant-neural-wake-alignment-and-clean-console-ux.md) | Bypassed redundant Stage-2 STT verification, eliminated 500ms post-hit delay, quad-mic active stereo downmixer, fail-open onset VAD, and minimal trigger banner console |
| 68 | Live Audio Telemetry, Trigger Debouncing & Diagnostics Parity | [68-live-audio-telemetry-and-connection-diagnostics-parity.md](68-live-audio-telemetry-and-connection-diagnostics-parity.md) | Real-time 12-slice speaker waveform and status meter in `nexus start`, atomic 1.5s neural trigger debouncing, and startup cloud connection diagnostics box |
| 69 | NLU Phonetic Alias Map & Persistent Missed-Intent Logging | [69-nlu-phonetic-alias-map-and-missed-intent-logging.md](69-nlu-phonetic-alias-map-and-missed-intent-logging.md) | Deterministic soundalike phrase normalizer for Ghost Mode and apps, stop phrases expansion, and persistent `%APPDATA%/com.nexus.assistant/missed_intents.jsonl` audit logging |
| 70 | **Ghost Mode Complete Program** | [70-ghost-mode-complete.md](70-ghost-mode-complete.md) | One-stop guide: phases 0→4 + voice integration (entry split, waves orb, hot-mic, in-session app/message routing with confirm-gated send), takeover leash, hardening, honest remainders |
| A1 | Ghost Architecture Reference | [ghost-mode-architecture-reference.md](ghost-mode-architecture-reference.md) | Symbol map (ghost.rs state, runners, grounding), wiring call-graph, and the Wry-typed Send invariant (why runtime erasure lives in one place) |
| 71 | Personalized & Neural-Augmented Wake-Word Training | [71-personalized-and-neural-augmented-wakeword-training.md](71-personalized-and-neural-augmented-wakeword-training.md) | 432 positive sample retraining (94 human mic + 88 SAPI + 250 Microsoft Neural) with Binary Focal Loss across 41,183 windows, 98.6% recall at 0.68, 0.00% FA on 186 fast multi-syllabic negatives |
| 72 | STT Hallucination Mitigation, Strict Wave Motion Rule & Architect Routing Fix | [72-stt-hallucination-mitigation-wave-motion-and-architect-routing-fix.md](72-stt-hallucination-mitigation-wave-motion-and-architect-routing-fix.md) | Acoustic pre-gate (<0.005 RMS), expanded Whisper hallucination filter, strict resting wave baseline (0.15 flat, Lottie paused), eliminated truncated "open" architect false triggers, demoted WhatsApp bridge probe logs |
| 73 | Ghost Waves, Browser Search & Typing, TTS Latency, and Intent Isolation | [73-ghost-waves-browser-search-typing-and-intent-isolation.md](73-ghost-waves-browser-search-typing-and-intent-isolation.md) | Deep root-cause resolution for wake/ghost wave persistence, local browser search & typing (Ctrl+L / dictation), app-open TTS latency elimination (<5ms cached), and complete GitHub intent isolation |
| 74 | Main Center + Per-Action Sub-Centers (plan) | [74-main-center-with-per-action-sub-centers.md](74-main-center-with-per-action-sub-centers.md) | Decision-tree router (one sub-center per action family), Alexa-mapped validity gate with spoken repeat-backs, single UI/STT/TTS directors; P0–P5 phased plan |
| 75 | Ghost-Session UX, Animation Timing & STT Gating (plan) | [75-ghost-session-ux-animation-timing-and-stt-gating.md](75-ghost-session-ux-animation-timing-and-stt-gating.md) | Orb persistence in ghost, speaking visibility, Esc/Ctrl+Space exits, n× repetition + Latin-gibberish STT gates, loading timing; P-A–P-E phased plan |
| 75 | GoogleCenter Hierarchical Domain Architecture & Proactive Sentinel | [75-google-center-hierarchical-domain-architecture-and-proactive-sentinel.md](75-google-center-hierarchical-domain-architecture-and-proactive-sentinel.md) | Domain sub-services (mail, calendar, maps, photos) unit-tested in isolation, unified GoogleCenter hub with OAuth2 PKCE, proactive background deadline sentinel, and Main Center UI/TTS integration |
| 76 | Vision Mode Screen Email Scanning & Proactive Watch Memory | [76-vision-mode-screen-email-scanning-and-proactive-watch-memory.md](76-vision-mode-screen-email-scanning-and-proactive-watch-memory.md) | VLM screen semantic extraction for open emails, dedicated active watch memory store (watches.json), and background Sentinel proactive deadline alerts |
| 77 | Multi-Email Google Accounts, Direct Native OAuth & Settings Refactor | [77-multi-email-google-accounts-and-direct-oauth-settings.md](77-multi-email-google-accounts-and-direct-oauth-settings.md) | Direct native desktop OAuth (RFC 8252) with loopback listener, multi-account storage in Windows Credential Manager, Settings accounts refactor, and Brave URL extraction fix |
| 78 | Sentinel Orb-Landing Event & Tracking Layer | [78-sentinel-orb-landing-event-and-tracking-layer.md](78-sentinel-orb-landing-event-and-tracking-layer.md) | Structured `orchestrator:sentinel-alert` event per detected change (spec wire shape) + lifecycle tracking store; landing animation deferred to a later phase |
| 79 | Cloned Agent-Subsystem Integration | [79-cloned-agent-subsystem-integration.md](79-cloned-agent-subsystem-integration.md) | Clone → extract → delete (mcp-agent, Memento, agent-workflow-mcp; LedgerMind dropped as Linux-only); provenance SHAs + corrected source map |
| 80 | Production Orb via Lottie Creator-MCP & Five-State Matrix | [80-production-orb-creator-mcp-pipeline.md](80-production-orb-creator-mcp-pipeline.md) | Creator-MCP editing pipeline (persistent relay, batch runner, export audits), wakeup markers + asset rotation, 3-bar waves prune, final state matrix (idle freeze / thinking loading / listening+speaking zoom), duplicate `pulse-listen` last-wins fix |
| 96 | Ghost Mode Window Focus Polling, Contextual Search & Human Cursor Motion | [96-ghost-mode-focus-search-and-human-motion-architecture.md](96-ghost-mode-focus-search-and-human-motion-architecture.md) | UFO-style 150ms focus retry loop with Win32 Alt-key lock bypass, context-aware WhatsApp search drill routing, sub-20ms local UIA Edit control grounding, and Fitts's Law 60 FPS humanized cursor gliding |
| 98 | Screen Analysis Celebrity Recognition & Zero-Delay Pipeline | [98-screen-analysis-celebrity-recognition-and-zero-delay-spec.md](98-screen-analysis-celebrity-recognition-and-zero-delay-spec.md) | Eliminates 30s dead air down to ~1.2s via healthy Gemini model pruning (gemini-3.5-flash-lite / gemini-3.5-flash), kills 3-engine cascading retry, unblocks face recognition for public figures, and provides pooled HTTP/2 client |
| 126 | Universal Agent Watcher & Cross-Platform Music Control | [126-universal-agent-watcher-and-cross-platform-music-control-plan.md](126-universal-agent-watcher-and-cross-platform-music-control-plan.md) | Universal agent completion monitoring (Claude, Codex, Antigravity, Cursor, Trae, Windsurf, generic CLI/MCP), implicit foreground window targeting, OS keep-awake leasing, proactive voice alerting, and cross-platform native Apple Music & Spotify control |
| 126 | Questions, Answer Capture & Question Memory | [126-questions-answer-capture-and-question-memory-live-log-analysis-and-plan.md](126-questions-answer-capture-and-question-memory-live-log-analysis-and-plan.md) | Live-log analysis of unanswered questions and the plan for answer capture + question memory |
| 127 | One Voice Everywhere / Carbon-Copy TTS (superseded) | [127-one-voice-everywhere-carbon-copy-tts-analysis-and-plan.md](127-one-voice-everywhere-carbon-copy-tts-analysis-and-plan.md) | Superseded by 128/129; bug findings still stand |
| 128 | Exact Microsoft Voice Locally + Settings Hub Architecture | [128-exact-microsoft-voice-locally-and-settings-hub-architecture-audit-and-plan.md](128-exact-microsoft-voice-locally-and-settings-hub-architecture-audit-and-plan.md) | Where Ava/Ryan can legally be run locally; how the settings hub drifted |
| 129 | Free Open-Source One Voice Everywhere | [129-free-open-source-one-voice-everywhere-research-and-plan.md](129-free-open-source-one-voice-everywhere-research-and-plan.md) | Free/open-source constraint analysis (Kokoro as the one engine) |
| 130 | RAM Usage by State (measured + estimated) | [130-ram-usage-by-state-measured-and-estimated.md](130-ram-usage-by-state-measured-and-estimated.md) | Idle/active RAM tables |
| 131 | Voice Quality, Variety & RAM Reduction (measured) | [131-voice-quality-variety-and-ram-reduction-plan-measured.md](131-voice-quality-variety-and-ram-reduction-plan-measured.md) | Kokoro/NLU memory breakdown, arena-off speed cost |
| 132 | RAM Budget 600-700 MB | [132-ram-budget-600-700-mb-for-all-tasks-plan.md](132-ram-budget-600-700-mb-for-all-tasks-plan.md) | Memory governor design and eviction order |
| 133 | Coordination With Ghost (no-vision plan) | [133-coordination-with-ghost-no-vision-plan-impact-and-file-ownership.md](133-coordination-with-ghost-no-vision-plan-impact-and-file-ownership.md) | File ownership rules between sessions |
| 134 | Edge First, Local Standby, Latency Lanes | [134-edge-first-local-standby-connectivity-and-latency-lanes-plan.md](134-edge-first-local-standby-connectivity-and-latency-lanes-plan.md) | Connectivity state machine, bank, standby, two latency lanes |
| 135 | Smart Fallback Timing & Spoken Connection Notices | [135-smart-fallback-timing-and-spoken-connection-notices-plan.md](135-smart-fallback-timing-and-spoken-connection-notices-plan.md) | Switch timing, reactive notices, varied phrases |
| 136 | Voice / Connectivity / RAM Master Compendium | [136-voice-connectivity-and-ram-master-compendium.md](136-voice-connectivity-and-ram-master-compendium.md) | One-file review of 126-135 + the owner decisions |
| 137 | Free Open-Source Low-Memory Production Plan (GPT) | [137-free-open-source-low-memory-production-plan-2026-10-17.md](137-free-open-source-low-memory-production-plan-2026-10-17.md) | Free-provider contract, license, memory gates; single-local-voice default rejected |
| 138 | Shutdown Never Explained; Misheard Power Commands; How-To in Sidebar | [138-shutdown-never-explain-misheard-power-commands-and-howto-in-sidebar-plan.md](138-shutdown-never-explain-misheard-power-commands-and-howto-in-sidebar-plan.md) | Loose power match, learned phrases, steps card, sidebar delivery |
| 140 | Agent-Grade Computer Control: Research + NEXUS Action Kernel | [140-agent-grade-computer-control-research-and-nexus-action-kernel-plan.md](140-agent-grade-computer-control-research-and-nexus-action-kernel-plan.md) | How Claude Code/Codex/UFO2 control a PC safely (typed tools, policy, sandbox, undo); risk classes, quarantine delete, provenance; phased plan |

---

## Additional documents

Listed automatically from the folder on 2026-10-17; descriptions are the documents' own titles.

| File | Title |
|---|---|
| [01-wake-word.md](01-wake-word.md) | Feature: Wake Word Detection |
| [02-tier3-commands.md](02-tier3-commands.md) | Feature: Tier 3 Direct Command Classification |
| [03-meeting-privacy-mode.md](03-meeting-privacy-mode.md) | Feature: Meeting / Privacy Mode |
| [04-boot-greeting.md](04-boot-greeting.md) | Feature: Boot & Wake Greeting |
| [05-sidecar-manager.md](05-sidecar-manager.md) | Feature: Sidecar Manager |
| [06-mic-permissions.md](06-mic-permissions.md) | Feature: Microphone Permission Handler |
| [07-app-registry.md](07-app-registry.md) | Feature: App Registry (Pre-Indexed Launcher + Resolution Cache) |
| [08-voice-enrollment.md](08-voice-enrollment.md) | Feature: Voice Enrollment & Speaker Verification |
| [09-audio-pipeline.md](09-audio-pipeline.md) | Feature: Local Audio Pipeline (VAD → STT → TTS) |
| [10-window-overlay.md](10-window-overlay.md) | Feature: Window Overlay & Click-Through |
| [100-memory-core-plan.md](100-memory-core-plan.md) | NEXUS Memory Core — priority-based, on-device, proactive life assistant (plan) |
| [101-claude-prompt-google-maps-routes-window.md](101-claude-prompt-google-maps-routes-window.md) | One-Time Claude Prompt: Google Maps & Routes Navigation Window Integration in NEXUS |
| [101-google-maps-routes-window-plan.md](101-google-maps-routes-window-plan.md) | Feature Plan: 101 — Google Maps & Routes Navigation Window |
| [102-keyboard-first-shortcut-brain-plan.md](102-keyboard-first-shortcut-brain-plan.md) | 102 — Keyboard-First Shortcut Brain + Zero-Delay Mouse Fallback (PLAN) |
| [103-ghost-command-understanding-and-keyboard-first-implementation-plan.md](103-ghost-command-understanding-and-keyboard-first-implementation-plan.md) | 103 — Ghost Mode: Command Understanding + Keyboard-First Execution — Combined Implementation Plan |
| [104-ghost-whatsapp-direct-chat-and-command-relation-plan.md](104-ghost-whatsapp-direct-chat-and-command-relation-plan.md) | 104 — Ghost Mode: "Open <name> chat in WhatsApp" directly, and "replace the old command only if it's a retry — never if it's a continuation" (PLAN ONLY) |
| [105-pure-black-letter-by-letter-caption-plan.md](105-pure-black-letter-by-letter-caption-plan.md) | Feature Plan: 105 — Pure Black Letter-by-Letter Voice Captions |
| [106-pebble-wakeup-orb-capsule-plan.md](106-pebble-wakeup-orb-capsule-plan.md) | Feature Plan: 106 — Pebble Wakeup Design for Voice Orb |
| [107-wake-word-production-plan.md](107-wake-word-production-plan.md) | 107 - Wake word "NEXUS": production plan (Alexa/Siri-level accuracy with real live clips) |
| [108-timed-social-contact-blocking-and-memory-leases.md](108-timed-social-contact-blocking-and-memory-leases.md) | Feature 108: Timed Social Contact Blocking & Memory Action Leases |
| [109-whatsapp-document-downloader-and-analyzer.md](109-whatsapp-document-downloader-and-analyzer.md) | Feature 109: WhatsApp Chat Document Downloader & Analyzer |
| [11-system-tray.md](11-system-tray.md) | Feature: System Tray & Manual Controls |
| [110-universal-screen-grounding-and-localhost-dev-runner.md](110-universal-screen-grounding-and-localhost-dev-runner.md) | Feature 110: Universal Screen Grounding & Localhost Dev Runner |
| [111-universal-autonomous-coding-agent-and-ide-adapter.md](111-universal-autonomous-coding-agent-and-ide-adapter.md) | Feature 111: Universal Autonomous Coding Agent & AI IDE Adapter |
| [112-college-email-internship-radar.md](112-college-email-internship-radar.md) | Feature 112: College Email Internship Radar |
| [113-alexa-style-voice-reminders-and-proactivity-center.md](113-alexa-style-voice-reminders-and-proactivity-center.md) | Feature 113: Alexa-Style Voice Reminders & Proactivity Center |
| [114-camera-attention-and-hand-gestures-plan.md](114-camera-attention-and-hand-gestures-plan.md) | 114 — Camera: "is the user talking to NEXUS?" and hand gestures (PLAN ONLY) |
| [115-camera-hand-mouse-plan.md](115-camera-hand-mouse-plan.md) | 115 — Camera as a mouse: hand moves the cursor, pinch = click (PLAN ONLY) |
| [116-hand-gestures-window-and-tab-switching-plan.md](116-hand-gestures-window-and-tab-switching-plan.md) | 116 — Hand gestures for switching windows and tabs, like the touchpad (PLAN ONLY) |
| [117-cross-os-screen-adaptation-and-intent-plan.md](117-cross-os-screen-adaptation-and-intent-plan.md) | 117 — Every OS, every screen, and understanding what the user means (STATUS + PLAN) |
| [118-hand-mode-master-compendium.md](118-hand-mode-master-compendium.md) | 118 — Hand Mode: the complete record (plans, decisions, field tests, architecture, status) |
| [119-instant-logon-readiness-voice-parity-and-intelligibility-plan.md](119-instant-logon-readiness-voice-parity-and-intelligibility-plan.md) | Feature Plan: 119 — Instant-at-Logon Readiness, Voice Parity & Intelligibility (PLAN ONLY) |
| [12-settings-window.md](12-settings-window.md) | Feature 12 — Settings Window |
| [120-subcommand-connection-contract-plan.md](120-subcommand-connection-contract-plan.md) | Feature Plan: 120 — Subcommand Connection Contract (Main Center never breaks) |
| [121-nexus-cloud-multi-agent-mediator-and-telephony-plan.md](121-nexus-cloud-multi-agent-mediator-and-telephony-plan.md) | Feature 121: NEXUS Cloud — Multi-Friend Agent Mediator + Free SIM Telephony |
| [122-agent-completion-watcher-and-keep-awake-architecture.md](122-agent-completion-watcher-and-keep-awake-architecture.md) | Feature 122: Keep-Awake + Agent-Completion Watcher — Architecture Reference |
| [123-boot-to-first-word-latency-analysis-and-plan.md](123-boot-to-first-word-latency-analysis-and-plan.md) | 123 — Power-on to first spoken word: measured analysis + plan (PLAN ONLY, nothing built) |
| [124-questions-keep-mic-open-and-follow-up-memory-audit-and-plan.md](124-questions-keep-mic-open-and-follow-up-memory-audit-and-plan.md) | 124 — When NEXUS asks a question: mic stays open, and "do it" still works later (AUDIT + PLAN, nothing built) |
| [125-response-delay-and-whatsapp-learning-analysis-and-plan.md](125-response-delay-and-whatsapp-learning-analysis-and-plan.md) | 125 — Slow replies ("Ok sir" after 7 s) and the WhatsApp agent that never learns (ANALYSIS + PLAN, nothing built) |
| [129-ghost-mode-reliable-desktop-control-implementation-plan.md](129-ghost-mode-reliable-desktop-control-implementation-plan.md) | Ghost Mode: reliable desktop control without a vision model |
| [13-response-sidebar.md](13-response-sidebar.md) | Feature 13 — Response Sidebar |
| [139-production-return-review-and-agent-task-reconciliation-plan.md](139-production-return-review-and-agent-task-reconciliation-plan.md) | 139 — Production return review and agent task reconciliation |
| [14-nsis-installer.md](14-nsis-installer.md) | Feature 14 — NSIS Installer |
| [15-setup-wizard.md](15-setup-wizard.md) | Feature 15 — Setup Wizard |
| [16-architecture-mapper.md](16-architecture-mapper.md) | 16 — Architecture Mapper |
| [25-multi-worker-optimization-plan.md](25-multi-worker-optimization-plan.md) | NEXUS Multi-Worker AI, Search, Storage & Optimization Plan |
| [28-linux-wayland-compatibility.md](28-linux-wayland-compatibility.md) | Linux & Wayland Compatibility Research |
| [29-overlay-architecture-analysis.md](29-overlay-architecture-analysis.md) | Overlay Architecture Analysis: Is Tauri Multi-Window the Best Approach? |
| [30-auto-fetch-github-url-plan.md](30-auto-fetch-github-url-plan.md) | Auto-Fetch GitHub Repo URL — Architecture Mapper |
| [31-architecture-mapper-ai-cost-research.md](31-architecture-mapper-ai-cost-research.md) | NEXUS Architecture Mapper — AI Cost & Sponsorship Research |
| [32-capacity-plan-1500-requests-day.md](32-capacity-plan-1500-requests-day.md) | NEXUS — 1,500 Requests/Day Capacity Plan (10 Users × 150 Each) |
| [36-tts-engine-evolution.md](36-tts-engine-evolution.md) | TTS Engine Evolution — Kokoro → Piper Migration |
| [37-stt-engine-evolution.md](37-stt-engine-evolution.md) | STT Engine Evolution — Moonshine → faster-whisper Journey |
| [38-loading-indicator-evolution.md](38-loading-indicator-evolution.md) | Loading Indicator Evolution — Separate Window → In-Orb |
| [39-voice-ack-pipeline-evolution.md](39-voice-ack-pipeline-evolution.md) | Voice Acknowledgement Pipeline Evolution — Validate-First + Cached TTS |
| [40-ram-optimization-plan-450mb.md](40-ram-optimization-plan-450mb.md) | RAM Optimization Plan — 800 MB → 450 MB |
| [40-self-learning-stt.md](40-self-learning-stt.md) | Self-Learning STT Correction System |
| [41-ci-cd-evolution.md](41-ci-cd-evolution.md) | CI/CD Evolution — Cross-Platform Installer Pipeline |
| [41-extreme-research-cloud-vs-local-ram-latency-storage.md](41-extreme-research-cloud-vs-local-ram-latency-storage.md) | Extreme Research: Cloud vs Local — RAM, Latency, Storage Optimization |
| [42-complete-options-comparison-all-engines.md](42-complete-options-comparison-all-engines.md) | Complete Options Comparison: Every STT, TTS, and NLU Engine |
| [42-ram-optimization-journey.md](42-ram-optimization-journey.md) | RAM Optimization Journey — 1,644 MB → 104 MB (94% Reduction) |
| [43-architectural-timeline.md](43-architectural-timeline.md) | NEXUS Architectural Timeline — Master Evolution Document |
| [43-phase2-implementation-plan-groq-piper.md](43-phase2-implementation-plan-groq-piper.md) | Phase 2 Implementation Plan — Groq STT + edge-tts TTS + Fallbacks |
| [44-pr-branch-query-commands-plan.md](44-pr-branch-query-commands-plan.md) | PR & Branch Query Commands — Research, Plan & Implementation |
| [45-central-orchestrator.md](45-central-orchestrator.md) | 45 — Central Orchestrator |
| [46-github-oauth-connect.md](46-github-oauth-connect.md) | 46 — GitHub OAuth Connect |
| [47-loading-indicator-ownership.md](47-loading-indicator-ownership.md) | 47 — Loading Indicator Ownership |
| [48-github-subcommand-system.md](48-github-subcommand-system.md) | 48 — GitHub Sub-Command System (Phase 2A) |
| [52-loading-choreography-and-llm-cascade.md](52-loading-choreography-and-llm-cascade.md) | Loading Choreography + LLM Cascade Refresh (2026-09-19) |
| [53-mic-stability-meet-parity.md](53-mic-stability-meet-parity.md) | Mic Stability + Meet Parity (2026-09-19) |
| [58-multi-source-noise-hardening-and-hardware-invariance.md](58-multi-source-noise-hardening-and-hardware-invariance.md) | Multi-Source Noise Hardening & Hardware-Invariant Microphone DSP |
| [59-vocal-friction-hardening-and-throat-gargle-rejection.md](59-vocal-friction-hardening-and-throat-gargle-rejection.md) | Feature 59: Vocal Friction Hardening & Industrial Throat/Gargle Rejection |
| [60-silence-phantom-elimination-and-comfort-streaming.md](60-silence-phantom-elimination-and-comfort-streaming.md) | Silence Phantom Elimination, Speech Onset Preservation & Dual-Buffer Comfort Streaming Architecture Specification |
| [61-training-data-poison-elimination-and-focal-multi-syllabic-hardening.md](61-training-data-poison-elimination-and-focal-multi-syllabic-hardening.md) | Feature 61: Training Data Poison Elimination, Focal Loss & Multi-Syllabic Speech Hardening |
| [73-system-voice-lock-ghost-orb-waves-and-win32-barge-in.md](73-system-voice-lock-ghost-orb-waves-and-win32-barge-in.md) | 73 — System Voice Lock, Ghost Orb Waves & Win32 Barge-In Architecture |
| [81-dual-grammar-modal-partitioning-and-foreground-grounding.md](81-dual-grammar-modal-partitioning-and-foreground-grounding.md) | Feature 81 — Dual-Grammar Modal Partitioning, Foreground Dynamic Grounding & Vision Screen Analysis |
| [82-interactive-animation-calibration-and-command-hub-overlay.md](82-interactive-animation-calibration-and-command-hub-overlay.md) | Feature 82 — Interactive Animation Calibration HUD & Command Hub Overlay |
| [83-top-10-iconic-voices-and-single-slot-dynamic-offline-swapping.md](83-top-10-iconic-voices-and-single-slot-dynamic-offline-swapping.md) | Feature 83 — Top 10 Iconic Voices & Single-Slot Dynamic Offline Swapping |
| [84-liquid-glass-frosted-desktop-widgets-architecture.md](84-liquid-glass-frosted-desktop-widgets-architecture.md) | Feature 84 — Liquid Glass Frosted Desktop Widgets & Backdrop Blur Engine |
| [85-live-passthrough-frosted-glass-and-automatic-luminance-adaptation.md](85-live-passthrough-frosted-glass-and-automatic-luminance-adaptation.md) | Feature 85 — Live Optical Pass-Through Frosted Glass & Automatic Luminance Adaptation |
| [86-spatial-vision-and-screen-ocr-command-center.md](86-spatial-vision-and-screen-ocr-command-center.md) | Feature 86: Spatial Vision & Screen OCR Command Center |
| [87-screen-annotation-architecture-plan.md](87-screen-annotation-architecture-plan.md) | Feature 87 — Screen Annotation & Live Architecture Drawing (Implementation Plan) |
| [87-shipnotes-webgl-voice-orb-integration.md](87-shipnotes-webgl-voice-orb-integration.md) | Feature 87: Ship Notes WebGL Voice Orb Integration |
| [88-admin-gated-worker-identity-and-byok.md](88-admin-gated-worker-identity-and-byok.md) | Feature 88 — Admin-Gated Worker Identity & Per-Laptop BYOK |
| [89-orb-screen-wide-entrance-and-tts-beat-sync.md](89-orb-screen-wide-entrance-and-tts-beat-sync.md) | Feature 89 — Orb Screen-Wide Entrance Burst & Real TTS-Audio Beat Sync |
| [90-nexus-production-hardening-plan.md](90-nexus-production-hardening-plan.md) | NEXUS Production-Hardening Implementation Plan (2026-10-05) |
| [91-memory-first-plan.md](91-memory-first-plan.md) | Memory-First Plan: How NEXUS Remembers & Learns About You (2026-10-05) |
| [92-selective-memory-self-learning-plan.md](92-selective-memory-self-learning-plan.md) | Selective Memory & Self-Learning Plan (2026-10-06) |
| [93-friendship-humor-personality-plan.md](93-friendship-humor-personality-plan.md) | Friendship, Humor & Understanding Plan (2026-10-06) |
| [94-command-hub-accounts-carbon-copy-plan.md](94-command-hub-accounts-carbon-copy-plan.md) | Command Hub Accounts Redesign — Carbon-Copy Plan (2026-10-06) |
| [94-fullscreen-overlay-dismissal-and-webview-resilience-plan.md](94-fullscreen-overlay-dismissal-and-webview-resilience-plan.md) | Architecture & Feature Plan 94: Fullscreen Overlay Dismissal, Escape Hatches & WebView2 Resilience |
| [95-memory-m3-ranked-recall-plan.md](95-memory-m3-ranked-recall-plan.md) | Memory M3 — Ranked Recall & Full-Path Injection Plan (2026-10-07) |
| [96-installer-connect-and-relevance-plan.md](96-installer-connect-and-relevance-plan.md) | Installer Connect UX + 50% Profile Bootstrap + Relevance Engine Plan (2026-10-07) |
| [97-screen-analysis-zero-delay-plan.md](97-screen-analysis-zero-delay-plan.md) | Screen-Analysis Zero-Delay Plan (2026-10-07) |
| [98-volume-baseline-restore-plan.md](98-volume-baseline-restore-plan.md) | Speaker Volume Baseline Restore Plan (2026-10-07) |
| [99-dual-engine-stt-nlu-alignment-and-accuracy-fix-spec.md](99-dual-engine-stt-nlu-alignment-and-accuracy-fix-spec.md) | Feature Spec 99: Dual-Engine STT ↔ BERT-Mini NLU Alignment & 50% Accuracy Fix |
| [146-foreground-review-and-recreation.md](146-foreground-review-and-recreation.md) | Foreground app/website review followed by reference-bound recreation; release gates and current implementation state |
| [150-hand-mode-production-refactor-implementation-plan.md](150-hand-mode-production-refactor-implementation-plan.md) | Hand Mode safety and latency refactor, with measurable Windows live release gates (plan only) |
