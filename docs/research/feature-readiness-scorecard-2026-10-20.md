# Feature readiness scorecard — NEXUS (2026-10-20)

Purpose: one table to cross-check how complete, tested, and proven each feature really is. Built from the repository (test counts, module sizes), `AGENTS.md` verification records (what was and was not run live), and the existing research (`docs/research/ai-assistant-landscape-audit-2026-09-26.md`, `docs/research/deployment/01-...`, `docs/research/wakeword/...`).

## How to read the numbers (please read this first)

The **% is a rubric estimate I computed from evidence, not a measured accuracy.** Only the figures in the *Measured numbers* section are real measurements. The rubric is fixed so two reviewers get the same answer from the same evidence:

| Axis | Weight | 100 means | 0 means |
|---|---|---|---|
| **B** Built and wired end to end | 20% | reachable by the user in the shipping path, no dead code | not reachable |
| **T** Automated test depth | 20% | many tests, mutation-checked, integration tests | no tests |
| **L** Live-verified on the real app / hardware | 30% | used repeatedly by the owner or run on real hardware | never run outside unit tests |
| **S** Safety and failure handling | 15% | gated, confirms before acting, fails closed, honest on failure | acts blindly |
| **I** Industry parity / measured quality | 15% | at or above published reference numbers | far below / unmeasured |

Stars = % / 20 in half steps. Verdict: **Production** >= 80, **Beta** 65-79, **Alpha** 50-64, **Prototype** < 50. Live verification carries the most weight on purpose: the project's own history shows features that passed unit tests and still failed in a real session.

## Headline

- 64 features scored. Average **64%**.
- Production: **6**, Beta: **26**, Alpha: **26**, Prototype: **6**.
- Repository size: 191 Rust modules, ~134,700 lines, 2,029 Rust test annotations (23 ignored dev benches), 362 frontend test cases in 45 files, 11 Worker test files, 9 Python test files, 176 change records, 180 feature docs, 382 commits.
- The dominant weakness across the whole project is not missing code, it is *unverified in the running app* (see the L column).

## Summary by area

| Area | Features | Avg % | Stars | Avg live-verified (L) |
|---|---|---|---|---|
| Voice front end | 15 | 65% | ★★★½☆ | 50% |
| Understanding | 6 | 69% | ★★★½☆ | 52% |
| Desktop control | 11 | 62% | ★★★☆☆ | 38% |
| Agent control | 8 | 65% | ★★★½☆ | 30% |
| Memory and proactive | 10 | 59% | ★★★☆☆ | 19% |
| Integrations | 8 | 66% | ★★★½☆ | 48% |
| Platform and quality | 6 | 69% | ★★★½☆ | 56% |

## Full table

### Voice front end

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| TTS online (Edge, persistent connection) | 44 tests | 100 | 70 | 90 | 80 | 85 | **86%** | ★★★★½ | Production | 409 ms median per short line, 290 ms after wake pre-warm; ~2.6 s end of speech to first audio for a cloud reply. |
| Cloud STT (Groq Whisper turbo) + hallucination filter | 29 tests | 100 | 75 | 85 | 80 | 80 | **84%** | ★★★★☆ | Production | Sits on the industry curve (~4-5% WER clean English). ~240 ms end to end. |
| Voice personas + offline twin swap | 31 tests | 95 | 75 | 55 | 85 | 70 | **74%** | ★★★½☆ | Beta | Pinned hashes, latest-wins queue. Audition by ear not done. |
| Wake word "NEXUS" (openWakeWord, own model) | 64 tests, replay harness | 100 | 90 | 55 | 75 | 40 | **72%** | ★★★½☆ | Beta | Works live, but the honest held-out measure is recall 63.2% (quiet 3/10, far 15/25) and ~12 false fires/h on real background; the 98.6% figure was self-scored on training data. Target is >=95% at <=0.1/h. |
| Global hotkey Ctrl+Space (call / cancel) | 0 tests | 100 | 10 | 85 | 80 | 80 | **72%** | ★★★½☆ | Beta | Used daily and works; no automated test. |
| Captions (word-by-word, black capsule) | vitest | 100 | 60 | 55 | 70 | 70 | **70%** | ★★★½☆ | Beta | Estimated (not real) word timing on the offline voice. |
| Orb UI and turn lifecycle | vitest | 100 | 55 | 60 | 65 | 65 | **68%** | ★★★½☆ | Beta | 'Stuck orb' class of bug recurred 6+ times; each fix partly unverified live. |
| Local STT fallback (Moonshine) | python sidecar | 90 | 40 | 60 | 75 | 75 | **66%** | ★★★½☆ | Beta | 6.65-7.84% WER; lazy sidecar; only used offline. |
| TTS offline (Kokoro-82M) | 27 tests | 90 | 70 | 50 | 80 | 50 | **66%** | ★★★½☆ | Beta | Real-time on CPU only (RTF 0.7-2.4); deliberately NOT the live voice. |
| Directed-speech gate (TV / side-talk filter) | 15 tests | 100 | 65 | 35 | 75 | 50 | **62%** | ★★★☆☆ | Alpha | Replay ignored 72/96 of a real noise log (75%); rules written after reading the same log, so optimistic. No live TV test. |
| Endpointing / VAD / turn detection | 5 tests | 90 | 45 | 55 | 60 | 50 | **60%** | ★★★☆☆ | Alpha | Adaptive 240-800 ms; age-bias (older speakers cut off) unmeasured. |
| Wake-up sound | 11 tests | 100 | 65 | 10 | 80 | 60 | **57%** | ★★★☆☆ | Alpha | Nothing audible has been heard yet. |
| Boot hello / shutdown flow / resume hello | mixed | 100 | 50 | 25 | 70 | 60 | **57%** | ★★★☆☆ | Alpha | Built in two rounds; nothing run live after rebuild. |
| Streaming partial captions (Deepgram, optional) | 3 tests | 80 | 25 | 20 | 60 | 60 | **45%** | ★★☆☆☆ | Prototype | Optional, needs a key; stream module has no tests. |
| Speaker verification / owner check | 14 tests | 60 | 50 | 10 | 50 | 30 | **37%** | ★★☆☆☆ | Prototype | Wired, off by default, fail-open; enrolment model (G1) not built. |

### Understanding

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Deterministic intent parser (regex tier) | 236 tests | 100 | 95 | 75 | 75 | 55 | **81%** | ★★★★☆ | Production | Strongest tested module. Early P0 measure: bare command 43% correct / 12% wrong action; since improved but not re-measured. |
| 9Router (Cerebras > Groq > Gemini > Worker) | 18 tests | 100 | 55 | 70 | 75 | 85 | **76%** | ★★★★☆ | Beta | ~242 ms vs 600-2100 ms through the Worker. |
| Compound commands ("X then Y") | 31 tests | 100 | 75 | 40 | 85 | 55 | **68%** | ★★★½☆ | Beta | GitHub step inside a compound drops the remaining steps (known). |
| Main centre / 17 sub-centres validation | 16 tests | 100 | 65 | 45 | 80 | 60 | **68%** | ★★★½☆ | Beta | Typed ActionSpec, shared policy; live walkthrough pending. |
| Mishearing normaliser + learned corrections | 34 tests | 100 | 60 | 45 | 70 | 55 | **64%** | ★★★☆☆ | Alpha | Real recall of learned corrections unmeasured. |
| NLU fallback (BERT-Mini ONNX) | 13 tests | 90 | 55 | 40 | 65 | 60 | **60%** | ★★★☆☆ | Alpha | Candidate model 0.9004 test intent; shipped model may lag; OTA update path built, never run end to end. |

### Desktop control

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Open / close / focus apps | 15+ tests | 100 | 55 | 80 | 75 | 70 | **77%** | ★★★★☆ | Beta | Used constantly; duplicate-window bug fixed. |
| Ghost mode (voice drives the PC) | 16+ tests | 100 | 65 | 60 | 85 | 55 | **72%** | ★★★½☆ | Beta | Keyboard-first is where the industry is reliable (>=89%); cursor/vision clicks sit in a 20-55% band. |
| Shortcut map + action router | 28 tests | 90 | 65 | 45 | 95 | 70 | **69%** | ★★★½☆ | Beta | Wrong-window = nothing sent; tested on Notepad only. |
| Hand mode (camera pointer) | 80+ tests | 95 | 70 | 55 | 85 | 45 | **69%** | ★★★½☆ | Beta | Pinch 'almost perfect' per owner; gestures, multi-monitor, macOS not run; camera 15 fps on battery. |
| Dictation (type what I say) | 24 tests | 95 | 65 | 40 | 85 | 55 | **65%** | ★★★☆☆ | Beta | 29 tests vs a planned 150-pair corpus. |
| Action Kernel (voice file control, quarantine delete, undo) | 43 tests | 100 | 75 | 10 | 95 | 70 | **63%** | ★★★☆☆ | Alpha | 20/20 mutations killed; never run by voice. |
| Screen analysis / narrated tour | 48 tests | 95 | 70 | 30 | 75 | 50 | **61%** | ★★★☆☆ | Alpha | Gemini box accuracy and 125/150% scaling not verified. |
| Live mode: WhatsApp / browser flows | 11 tests | 85 | 35 | 45 | 70 | 45 | **55%** | ★★½☆☆ | Alpha | WhatsApp UIA unverified on current build; opener bounded to 3 candidates. |
| Vision-grounded click | 43 tests | 85 | 60 | 25 | 70 | 40 | **53%** | ★★½☆☆ | Alpha | No measured accuracy on real screens; calibration results never recorded. |
| Window tiling + display off | 21 tests | 100 | 55 | 5 | 85 | 50 | **53%** | ★★½☆☆ | Alpha | No real window moved, display_off never called. |
| Music control (Spotify / Apple Music) | 3 tests | 85 | 35 | 25 | 60 | 55 | **49%** | ★★½☆☆ | Prototype | Windows media keys and deep links. |

### Agent control

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Watch one agent, then power off | ~100 tests | 100 | 85 | 65 | 95 | 70 | **81%** | ★★★★☆ | Production | Two practice-mode runs, verdict engine behaves; a real shutdown never run. |
| Ask-first shutdown / sleep / restart | 23 tests | 100 | 75 | 50 | 95 | 70 | **75%** | ★★★½☆ | Beta | Graceful close; a save prompt stops it. |
| Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) | 48 tests | 100 | 90 | 40 | 90 | 70 | **74%** | ★★★½☆ | Beta | Live: Antigravity start/stop, Claude prompt hook. Docs only: Codex, Devin, Windsurf, Kilo, Copilot, Cursor, Gemini, OpenCode, Aider. |
| Keep-awake | 6 tests | 100 | 65 | 35 | 70 | 65 | **64%** | ★★★☆☆ | Alpha | powercfg behaviour on this Modern-Standby laptop not checked. |
| Track all agents (announce each, summary) | 4+37 tests | 100 | 70 | 5 | 90 | 65 | **59%** | ★★★☆☆ | Alpha | Built today; no multi-agent live run. |
| Gated commit + draft PR | 25 tests | 100 | 80 | 0 | 90 | 60 | **58%** | ★★★☆☆ | Alpha | Never run against a real repo or GitHub. |
| Agent watch sidecar (spawn + watch CLIs) | 2 tests | 80 | 30 | 45 | 70 | 60 | **55%** | ★★★☆☆ | Alpha | Real PTY spawn verified once (Claude); others not installed here. |
| Overnight record + morning briefing | 23 tests | 100 | 65 | 0 | 85 | 60 | **55%** | ★★½☆☆ | Alpha | Not run across a real shutdown. |

### Memory and proactive

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Memory core (encrypted store, ranked recall, context pack) | 62 tests | 100 | 90 | 55 | 90 | 70 | **78%** | ★★★★☆ | Beta | Paraphrase recall 19/19 on dev set (tuned), 17/20 held-out. |
| Timetable from an image + reminders | 16 tests | 90 | 65 | 45 | 80 | 60 | **66%** | ★★★½☆ | Beta | Real Gemini read all 5 slots of a generated timetable. |
| Boot briefing + resume points | 31 tests | 95 | 70 | 25 | 80 | 60 | **62%** | ★★★☆☆ | Alpha | Real reboot never tested. |
| Important-mail watcher + triage | 22 tests | 95 | 75 | 10 | 85 | 65 | **60%** | ★★★☆☆ | Alpha | No Google account connected at test time. |
| Profile, corrections, routines, review, curator | 59 tests | 95 | 75 | 10 | 85 | 60 | **59%** | ★★★☆☆ | Alpha | Review never run against a real model; routines have no history yet. |
| Proactive-speech policy | 15 tests | 90 | 70 | 10 | 85 | 55 | **56%** | ★★★☆☆ | Alpha | Thresholds are hypotheses; nothing emits a Critical alert yet. |
| Session snapshot / restore | 18 tests | 90 | 60 | 15 | 80 | 55 | **55%** | ★★½☆☆ | Alpha | Brave/Edge restore flag not verified. |
| Calendar read / add | 8 tests | 90 | 50 | 15 | 85 | 60 | **54%** | ★★½☆☆ | Alpha |  |
| WhatsApp priority people (read-only) | 17+8 tests | 85 | 65 | 5 | 90 | 50 | **52%** | ★★½☆☆ | Alpha | OFF by default; read-receipt side effects unknown. |
| Leave-by reminders | 5 tests | 90 | 35 | 0 | 75 | 55 | **44%** | ★★☆☆☆ | Prototype | Needs a real calendar event. |

### Integrations

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Cloudflare Worker | 145 tests | 100 | 80 | 75 | 85 | 75 | **82%** | ★★★★☆ | Production |  |
| GitHub commands (PRs, merge, review) | 61 tests | 100 | 85 | 70 | 85 | 70 | **81%** | ★★★★☆ | Production |  |
| MCP connect system (Swiggy, Amazon, WhatsApp) | 28 tests | 100 | 75 | 40 | 85 | 70 | **70%** | ★★★½☆ | Beta | Swiggy production needs approval; bridges not running here. |
| Architecture mapper | 6 tests | 100 | 35 | 70 | 70 | 65 | **68%** | ★★★½☆ | Beta | 2,991 lines, 6 tests. |
| Maps / Places / Directions | 8 tests | 95 | 45 | 55 | 70 | 65 | **65%** | ★★★☆☆ | Alpha | Both keys verified once. |
| Google accounts + OAuth | ~40 tests | 95 | 45 | 40 | 80 | 65 | **62%** | ★★★☆☆ | Alpha | Needs the user's own OAuth client; Testing-mode token expires in 7 days. |
| Device Links (phone / laptop pairing) | 145 Worker tests share | 75 | 70 | 35 | 85 | 60 | **61%** | ★★★☆☆ | Alpha | Local Wrangler + D1 verified; no live SMS or two-laptop run. |
| Activity Manager (app-only history) | 1 test | 70 | 10 | 0 | 85 | 40 | **35%** | ★½☆☆☆ | Prototype | Off by default. |

### Platform and quality

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Security: keychain, PII filter, sealed memory | ~30 tests | 100 | 70 | 60 | 85 | 70 | **75%** | ★★★★☆ | Beta | Keys still also in settings.json in places. |
| Resource footprint (RAM / CPU / battery) | measured | 100 | 40 | 80 | 75 | 80 | **75%** | ★★★★☆ | Beta | Idle 104-269 MB; Kokoro +368 MB when loaded. |
| TTS volume lease / restore | 14 tests | 100 | 70 | 55 | 85 | 75 | **74%** | ★★★½☆ | Beta |  |
| Command Hub / settings UI | vitest | 100 | 60 | 65 | 70 | 70 | **72%** | ★★★½☆ | Beta | Visual match to design not verified at 400 px. |
| CI gates: dead code, IPC, turn-end, hygiene | scripts | 95 | 70 | 40 | 85 | 75 | **69%** | ★★★½☆ | Beta | Gates fail on unrelated concurrent items; the GitHub workflow itself never run. |
| Installer / release pipeline | - | 70 | 30 | 35 | 60 | 50 | **47%** | ★★½☆☆ | Prototype | NSIS broken locally; MSI built; start-menu shortcut points at a stale exe. |

## Measured numbers (the only hard measurements)

| What | Measured | Reference / target | Source |
|---|---|---|---|
| Wake word recall, held-out owner session | 63.2% (quiet 3/10, far 15/25) | >=95% | wake P0 replay baseline |
| Wake word recall on training data | 98.6% (self-scored, optimistic) | - | same, shown for contrast |
| Wake false fires on real background | ~12 per hour (0.42 h sample) | <=0.1 per hour | wake P0 |
| Spoken soundalikes that wake it | 10 of 15 | 0 | wake P0 |
| Early parser accuracy, bare command | 43% correct, 12% WRONG action | - | doc 103 P0 baseline (since improved, not re-measured) |
| NLU candidate intent accuracy | 0.9004 | Rhasspy-style pipelines 92.8-94.8% | phase-11 dataset |
| Cloud STT | ~240 ms, WER on the Whisper curve (~4-5% EN) | Wispr Flow ~700 ms | STT research 2026-10-07 |
| Edge TTS first audio | 409 ms median (290 ms warm) | streaming TTS 75-250 ms | change 132 |
| Reply latency end of speech to first audio | ~2.6 s | - | voice plan 136 |
| Directed-speech gate on real noise log | 72 of 96 ignored (75%), ~4 false accepts | - | change 82 (optimistic) |
| Vision click accuracy on real screens | NOT MEASURED | ScreenSpot-v2 ~90% easy, OSWorld-G ~50-54% | deployment paper |
| Idle RAM | 104-269 MB | Jan ~200 MB, LocalAI ~500 MB | landscape audit / doc 131 |
| Hand camera rate on battery | ~15 fps (driver halves it) | 30 fps | hand field test notes |

## Industry comparison (same features, nearest reference)

| Capability | NEXUS | Nearest reference | Where NEXUS is ahead | Where NEXUS is behind |
|---|---|---|---|---|
| Wake word | Custom openWakeWord, 63% held-out recall | Alexa / Siri / Google (multi-stage, cloud verify, years of tuning) | Personalised, hardware-invariance tests | Recall and false-wake rate far from product grade |
| Command understanding | Regex > BERT-Mini > Qwen > cloud | Leon, Rhasspy, Home Assistant | More tiers, <1 ms primary path | Accuracy 0.90 vs 0.93-0.95 pipelines |
| Voice latency | 240 ms STT, 409 ms TTS start | Wispr Flow ~700 ms, Pipecat ~500 ms | Faster STT path | Reply ~2.6 s because of cloud LLM |
| Computer control | Keyboard-first Ghost, UIA, vision fallback | Open Interpreter OS mode, OSWorld agents (20-55%) | Safety design (Esc, task abort, protected windows) | Click accuracy unmeasured |
| Memory | Encrypted local store, provenance, egress log | Hermes / Letta designs, ChatGPT memory | Local, auditable, user-editable | Retrieval is lexical + synonyms, not semantic |
| Agent watching | Hooks + verdict engine + unconditional/conditional shutdown | Nothing comparable in open source | Unique feature | Most agent hooks unverified; one session id per agent |
| RAM footprint | 104-269 MB idle | Jan ~200 MB | Lazy loading | Kokoro +368 MB when active |
| Platform reach | Windows | Home Assistant / Open Interpreter cross-platform | Deep Windows integration | macOS/Linux largely unbuilt |

## Weakest 12 (where the next effort should go)

| Feature | Score | Main reason |
|---|---|---|
| Activity Manager (app-only history) | 35% | Off by default. |
| Speaker verification / owner check | 37% | Wired, off by default, fail-open; enrolment model (G1) not built. |
| Leave-by reminders | 44% | Needs a real calendar event. |
| Streaming partial captions (Deepgram, optional) | 45% | Optional, needs a key; stream module has no tests. |
| Installer / release pipeline | 47% | NSIS broken locally; MSI built; start-menu shortcut points at a stale exe. |
| Music control (Spotify / Apple Music) | 49% | Windows media keys and deep links. |
| WhatsApp priority people (read-only) | 52% | OFF by default; read-receipt side effects unknown. |
| Window tiling + display off | 53% | No real window moved, display_off never called. |
| Vision-grounded click | 53% | No measured accuracy on real screens; calibration results never recorded. |
| Calendar read / add | 54% | low live proof |
| Live mode: WhatsApp / browser flows | 55% | WhatsApp UIA unverified on current build; opener bounded to 3 candidates. |
| Overnight record + morning briefing | 55% | Not run across a real shutdown. |

## Out of the current target (decision recorded)

| Feature | Score | Why it is out |
|---|---|---|
| Admin brain (Qwen, admin only) | 50% | D2: compiled out of family builds; scored only for the admin build |
| Administrator control | 30% | D2: administrator access is closed by the owner until enrolment and verification are built |
| macOS / Linux parity | 23% | D1: Windows 1.0 target only; macOS/Linux is its own programme and needs a Mac |

## What would move the numbers fastest

1. **One rebuilt-app voice walkthrough** of the newest features (tracker, tiling, display-off, commit/PR, overnight record, wake cue, boot hello). It lifts the L axis on about 20 rows at once; no new code needed.
2. **Wake word**: record the >=10 h of ambient audio the plan asks for and run the session-separated replay; the 63% / 12-per-hour baseline is the single biggest product risk.
3. **Vision click accuracy**: run the existing calibration suite on real screens and record the result; today that number does not exist.
4. **Real shutdown rehearsal**: `dry`, then one `on` run with a deliberately failing agent (verifies unconditional shutdown plus the morning briefing).
5. **Live hook runs** for Codex, Devin, Windsurf, Kilo, Copilot (one agent at a time, read the `[WATCH-POWER] agent event` line).

## Caveats

- Scores for items I did not re-run today come from the project's own verification notes in `AGENTS.md`; I did not re-verify them live.
- Test counts are `#[test]` annotations per module, not pass counts; a few suites flake under parallel load (timing tests).
- Weighting is a judgement. Change the weights in the rubric and the table recomputes the same way; the raw B/T/L/S/I columns are given so you can re-score any row.
