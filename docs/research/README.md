# NEXUS Research Index

All research documents organized by topic.

## Landscape & Audit
- [Vision-Grounded Gmail Engine & Proactive Watch Architecture](vision-grounded-gmail-engine-and-proactive-watch-architecture-2026-09-29.md) — 3-tier hybrid perception (browser URL hash + VLM fallback), native Gmail Engine watch state machine, and proactive deadline diffing
- [Voice & Ghost Mode Root Cause Analysis & Flow Hardening](voice-and-ghost-mode-root-cause-analysis-and-flow-hardening-2026-09-29.md) — Deep audit of ghost waves, strict motion invariants, browser search & typing (Ctrl+L/dictation), app-open TTS latency (<5ms), and complete GitHub intent isolation
- [AI Assistant Landscape Audit](ai-assistant-landscape-audit-2026-09-26.md) — 15 GitHub projects + YouTube landscape + claim verification vs academic papers

## Competitive Analysis
- [Wake Word Academic Analysis](wakeword-academic-analysis.md) — Wake word systems vs academic papers (arXiv:2304.03416, arXiv:2011.01460, arXiv:2307.14743)
- [Ghost Mode Competitive Analysis](ghost-mode-competitive-analysis.md) — AI desktop control comparison (Open Interpreter, AI Desktop, Everywhere, PyGPT)
- [NLU Competitive Analysis](nlu-competitive-analysis.md) — NLU systems comparison (Leon, Home Assistant, Almond, Rhasspy, Mycroft)
- [MCP Ecosystem Analysis](mcp-ecosystem-analysis.md) — MCP ecosystem study (arXiv:2607.25635, ACM DOI:10.1145/3796519)
- [Voice Assistant Comparison](voice-assistant-comparison.md) — Full feature-by-feature comparison table

## Identity & Access
- **[Administrator-Privilege Research Suite](admin-privilege/README.md)** — 9-doc research suite: live voice candidate → camera face/PAD → fresh spoken challenge → action policy. Its early blanket personal-domain deny-list is historical where it conflicts with the later [Feature 143 product decisions](../features/143-administrator-review-access.md). The active build sequence is [Feature 145](../features/145-administrator-control-implementation.md); the earlier Device Links foundation is [change 144](../changes/144-cloudflare-free-device-links-foundation.md).
- [01 · Overview & Verdict](admin-privilege/01-admin-privilege-any-device-research-2026-10-19.md) — voice-only verdict (NIST 800-63B-4 forbids), R1-R8 decoded, core evidence tables, licensing, P0-P6
- [02 · Face Enrollment Without Storing Raw Images](admin-privilege/02-face-enrollment-template-only-pipeline-research-2026-10-19.md) — detect→align→embed→verify anatomy, zero-raw retention, template-inversion risk literature, E0-E5
- [03 · Voice Identity — Firewall 1](admin-privilege/03-voice-identity-firewall-f1-research-2026-10-19.md) — embeddings + EERs (sherpa-onnx/wespeaker/SpeechBrain), cross-device reality, AS-Norm calibration, ASVspoof 5 anti-spoof numbers, V0-V5
- [04 · Standards & Compliance](admin-privilege/04-standards-nist-compliance-research-2026-10-19.md) — NIST SP 800-63B-4 as design checklist, ISO 30107-3 PAD vocabulary, GDPR Art. 9, honest claim ladder
- [05 · Face Verification — Firewall 2](admin-privilege/05-face-verification-firewall-f2-research-2026-10-19.md) — YuNet/SFace benchmarks, PAD ensemble (MiniFASNet, rPPG, lip-sync), CelebA-Spoof/CASIA-SURF, cross-camera gap, F0-F5
- [06 · Multimodal Fusion & Deepfakes](admin-privilege/06-multimodal-fusion-deepfakes-research-2026-10-19.md) — POI-Forensics/AVFF/AV-Deepfake1M, lip-sync correspondence, fusion decision semantics, in-session re-verification
- [07 · Threat Model & Capability Policy](admin-privilege/07-threat-model-capability-policy-research-2026-10-19.md) — attack-vs-firewall matrix, capability tokens + personal-domain deny-list, session-token design, audit contract
- [08 · UX, Session Flow & Phrasing](admin-privilege/08-ux-session-flow-phrasing-research-2026-10-19.md) — session state machine with timings, phrase catalog (role/sir styles), enrollment script, test-ready string keys
- [09 · Master Implementation Plan](admin-privilege/09-master-implementation-plan-2026-10-19.md) — 17-phase consolidated roadmap, module map, RAM/latency budgets, red-team corpora, AGENTS.md template

## Tools & Observability
- **[REA Teardown Research Suite](rea-teardown/README.md)** — 5-doc suite (index, reading order): REA reverse-engineering kit (github.com/morluto/rea, MIT) for **websites only**, installed pinned outside the project, soft-imported teardown bay + `/teardown/diag`, observe-only LAW verified against REA's own non-goals
- [01 · Tool Verification & Pinned Install](rea-teardown/01-rea-tool-verification-install-research-2026-10-19.md) — npm registry identity evidence (repository.url → morluto/rea), pin `rea-agents@6.7.0`, `--ignore-scripts` + frida postinstall analysis, PATH-independent `node rea.mjs COMMAND --json`, exact install commands
- [02 · Website Observation & LAW Compliance](rea-teardown/02-browser-observation-law-compliance-research-2026-10-19.md) — CDP attach model, passive surfaces + data-minimization ledger, clause-by-clause observe-only law mapping, excluded Playwright scenario surface, session workflow
- [03 · Teardown Bay & /teardown/diag Design](rea-teardown/03-teardown-bay-diag-design-research-2026-10-19.md) — soft-import pattern on the FastAPI sidecar, 9-check diag ladder, JSON contract, Node/Chrome discovery ladders, in-character one-liners
- [04 · Build Plan T0-T5](rea-teardown/04-build-plan-research-2026-10-19.md) — install/pin → diag → law-enforcement → live VERIFY → hardening; negative-test matrix, budgets, AGENTS.md template
- [05 · Feature-Impact Study](rea-teardown/05-feature-impact-study-2026-10-19.md) — strategy + estimated % boost per existing feature (screen analysis ~30 %, text-evidence ~25 %, ghost grounding 10-15 %, PR/repo 5-10 %), learnings, honest zeros, build-first order

## Voice Lock and Architect-guided review

- [NEXUS review using ElevenLabs Architect guidance](elevenlabs-architect-guided-nexus-review.md) — local source/evidence review using the installed vendor playbooks; no remote MCP consultation claimed.
- [External Architect review brief](elevenlabs-architect-review-brief.md) — prepared questions and connection-attempt limitations.
- [Implementation plan](../features/152-voice-lock-evidence-and-reliability-plan.md) — ordered capture accounting, effective lock state, verifier benchmarking, command fidelity and real-device release gates.

## Planning
- [Improvement Plan](improvement-plan.md) — 19 prioritized improvements across 4 phases (A-D)

## Hand Mode
- [Original camera, pointer, and gesture research](hand-mode/01-camera-attention-hand-pointer-and-gestures-research.md) — earlier design and sources.
- [Production refactor research](hand-mode/02-production-refactor-research-2026-10-21.md) — current source audit, latency hypotheses, safety gaps, and measurement contract; [implementation plan](../features/150-hand-mode-production-refactor-implementation-plan.md).

## Existing Research (from AGENTS.md)
- [Wake Word Research](features/71-personalized-and-neural-augmented-wakeword-training.md)
- [NLU Phonetic Alias Map](features/69-nlu-phonetic-alias-map-and-missed-intent-logging.md)
- [STT Hallucination Fix](changes/50-stt-hallucination-verify-ring-prefix-pad-and-ghost-mode-vocabulary.md)
- [Ghost Mode Complete](features/70-ghost-mode-complete.md)
- [Ghost Architecture Reference](features/ghost-mode-architecture-reference.md)

## Claim Verification Summary
| Claim | Verdict | Source |
|-------|---------|--------|
| Whisper too slow for real-time | Exaggerated | arXiv:2307.14743 (Whisper-Streaming) |
| Local models 3-7x slower | Misleading | arXiv:2601.09527 (GPU benchmarks) |
| 0% false positive rate | Incorrect | openWakeWord docs, arXiv:2304.03416 |
| 104MB idle RAM | Optimistic | Tauri #5889, Microsoft WebView2 docs |
| Streaming TTS ~500ms | Conservative | arXiv:2509.15969 (VoXtream), arXiv:2111.09052 |
| Agent protocols reduce dev time | Unverified | arXiv:2607.25635, ACM DOI:10.1145/3796519 |

## Key Findings
- NEXUS is best-in-class in wake word (9.5/10), Ghost Mode safety (9.5/10), MCP (9/10), OTA updates (10/10)
- Critical gap: No persistent memory (2/10) — Leon has 5-layer memory system
- High-ROI improvements: persistent memory, streaming TTS, speaker verification wiring, Ghost Mode vision
- 19 improvements prioritized across 4 phases (A-D), ~6 months total

- [Focus Mode, owner-less GitHub commands and notification sources (2026-10-26)](focus-github-notification-research-2026-10-26.md) — Hand Mode load and background-task inventory, GitHub repo/PR resolution gaps, provider API options for deploy/CI notifications, state and privacy design, Antigravity delegation findings.
