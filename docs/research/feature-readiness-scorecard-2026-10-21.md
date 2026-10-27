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

- 65 features scored. Average **68%**.
- Production: **7**, Beta: **37**, Alpha: **18**, Prototype: **3**.
- Repository size: 191 Rust modules, ~134,700 lines, 2,029 Rust test annotations (23 ignored dev benches), 362 frontend test cases in 45 files, 11 Worker test files, 9 Python test files, 176 change records, 180 feature docs, 382 commits.
- The dominant weakness across the whole project is not missing code, it is *unverified in the running app* (see the L column).

## Summary by area

| Area | Features | Avg % | Stars | Avg live-verified (L) |
|---|---|---|---|---|
| Voice front end | 14 | 69% | ★★★½☆ | 52% |
| Understanding | 6 | 71% | ★★★½☆ | 54% |
| Desktop control | 12 | 68% | ★★★½☆ | 43% |
| Agent control | 8 | 70% | ★★★½☆ | 42% |
| Memory and proactive | 10 | 62% | ★★★☆☆ | 20% |
| Integrations | 8 | 73% | ★★★½☆ | 53% |
| Platform and quality | 7 | 67% | ★★★½☆ | 46% |

## Full table

### Voice front end

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Cloud STT (Groq Whisper turbo) + hallucination filter | 60 | 100 | 85 | 85 | 88 | 80 | **88%** | ★★★★½ | Production | Routing extracted and tested (privacy mode never reaches a cloud step); fixed empty-buffer upload and NaN energy; Deepgram key now honoured by the main entry point; provider-side audio rejection still falls back to local. |
| TTS online (Edge, persistent connection) | 44 tests | 100 | 70 | 90 | 80 | 85 | **86%** | ★★★★½ | Production | 409 ms median per short line, 290 ms after wake pre-warm; ~2.6 s end of speech to first audio for a cloud reply. |
| Global hotkey Ctrl+Space (call / cancel) | 5 | 100 | 75 | 85 | 85 | 80 | **85%** | ★★★★½ | Production | Cancel/call decision extracted and checked for all 16 states; stale doc comment rewritten. |
| Voice personas + offline twin swap | 31 tests | 95 | 75 | 55 | 85 | 70 | **74%** | ★★★½☆ | Beta | Pinned hashes, latest-wins queue. Audition by ear not done. |
| Local STT fallback (Moonshine) | python sidecar | 90 | 70 | 60 | 75 | 75 | **72%** | ★★★½☆ | Beta | Fallback decisions tested; airplane-mode walkthrough still to do. |
| Orb UI and turn lifecycle | vitest | 100 | 72 | 60 | 65 | 65 | **72%** | ★★★½☆ | Beta | Store transitions, thinking dwell and watchdog inputs now tested; the 50-turn soak is still needed. |
| Wake word "NEXUS" (openWakeWord, own model) | 64 tests, replay harness | 100 | 90 | 55 | 75 | 40 | **72%** | ★★★½☆ | Beta | Works live, but the honest held-out measure is recall 63.2% (quiet 3/10, far 15/25) and ~12 false fires/h on real background; the 98.6% figure was self-scored on training data. Target is >=95% at <=0.1/h. |
| Captions (word-by-word, black capsule) | vitest | 100 | 70 | 55 | 70 | 70 | **72%** | ★★★½☆ | Beta | Estimated (not real) word timing on the offline voice. |
| Directed-speech gate (TV / side-talk filter) | 27 + 145-line corpus | 100 | 80 | 35 | 82 | 55 | **67%** | ★★★½☆ | Beta | 145-line corpus: noise rejected 98.7%; fixed 'tell him I'll call back' being accepted (tell now needs me/us/nexus/an agent). Corpus was written by the engineer, so still optimistic. Needs the 30-minute live TV test. |
| TTS offline (Kokoro-82M) | 27 tests | 90 | 70 | 50 | 80 | 50 | **66%** | ★★★½☆ | Beta | Real-time on CPU only (RTF 0.7-2.4); deliberately NOT the live voice. |
| Endpointing / VAD / turn detection | 5 tests | 90 | 45 | 55 | 60 | 50 | **60%** | ★★★☆☆ | Alpha | Adaptive 240-800 ms; age-bias (older speakers cut off) unmeasured. |
| Wake-up sound | 11 tests | 100 | 65 | 10 | 80 | 60 | **57%** | ★★★☆☆ | Alpha | Nothing audible has been heard yet. |
| Boot hello / shutdown flow / resume hello | mixed | 100 | 50 | 25 | 70 | 60 | **57%** | ★★★☆☆ | Alpha | Built in two rounds; nothing run live after rebuild. |
| Speaker verification / owner check | 14 tests | 75 | 55 | 10 | 50 | 30 | **41%** | ★★☆☆☆ | Prototype | Enrolment exists in the setup wizard (not in the Command Hub) and the gate is wired, but speakerVerification is off and no voice profile exists on this PC. Needs your enrolment and a false-accept/reject measurement. |

### Understanding

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Deterministic intent parser (regex tier) | 236 tests | 100 | 95 | 75 | 75 | 55 | **81%** | ★★★★☆ | Production | Strongest tested module. Early P0 measure: bare command 43% correct / 12% wrong action; since improved but not re-measured. |
| 9Router (Cerebras > Groq > Gemini > Worker) | 18 tests | 100 | 55 | 70 | 75 | 85 | **76%** | ★★★★☆ | Beta | ~242 ms vs 600-2100 ms through the Worker. |
| Mishearing normaliser + learned corrections | 45 | 100 | 75 | 55 | 75 | 62 | **72%** | ★★★½☆ | Beta | Real log replay: 31 of 194 unique missed commands are now understood; added openbread/open brief/command hop. 163 still missed (see ledger). |
| Compound commands ("X then Y") | 31 tests | 100 | 75 | 40 | 85 | 55 | **68%** | ★★★½☆ | Beta | GitHub step inside a compound drops the remaining steps (known). |
| Main centre / 17 sub-centres validation | 16 tests | 100 | 65 | 45 | 80 | 60 | **68%** | ★★★½☆ | Beta | Typed ActionSpec, shared policy; live walkthrough pending. |
| NLU fallback (BERT-Mini ONNX) | 13 tests | 90 | 55 | 40 | 65 | 60 | **60%** | ★★★☆☆ | Alpha | Candidate model 0.9004 test intent; shipped model may lag; OTA update path built, never run end to end. |

### Desktop control

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Action Kernel (voice file control, quarantine delete, undo) | 53 | 100 | 90 | 45 | 95 | 75 | **77%** | ★★★★☆ | Beta | Spoken grammar -> confirm -> trash -> undo on real folders (unicode, long names, clashes, forbidden places, wrong origin). Still needs: one spoken delete and undo by voice (W3). |
| Open / close / focus apps | 15+ tests | 100 | 55 | 80 | 75 | 70 | **77%** | ★★★★☆ | Beta | Used constantly; duplicate-window bug fixed. |
| Window tiling + display off | 21 pure + 2 live | 100 | 85 | 55 | 88 | 60 | **76%** | ★★★★☆ | Beta | Real proof: 3 own windows tiled into exact 640px columns (0-640-1280-1920) and display_off returned Ok. Still needs: the finish sequence on real agent windows, the screen actually going dark seen by the owner, an elevated window skip (W1). |
| Shortcut map + action router | 55 | 90 | 85 | 45 | 95 | 70 | **73%** | ★★★½☆ | Beta | 16 new tests: seed validity, precedence for 5 real app cases, terminals never inherit Ctrl+C/Z/S/X/A/Y, router gates and call order. No wrong chord found in the seed. Tested on Notepad only live. |
| Ghost mode (voice drives the PC) | 16+ tests | 100 | 65 | 60 | 85 | 55 | **72%** | ★★★½☆ | Beta | Keyboard-first is where the industry is reliable (>=89%); cursor/vision clicks sit in a 20-55% band. |
| Dictation (type what I say) | 20 + 156-pair corpus | 95 | 90 | 40 | 85 | 65 | **72%** | ★★★½☆ | Beta | 156-pair spoken->written corpus gates the cleaner. Known quirks: 'one dollar' -> '1 dollars', proper nouns stay lowercase. Still needs real dictation into apps (W3). |
| Hand mode (camera pointer) | 80+ tests | 95 | 70 | 55 | 85 | 45 | **69%** | ★★★½☆ | Beta | Pinch 'almost perfect' per owner; gestures, multi-monitor, macOS not run; camera 15 fps on battery. |
| Music control (Spotify / Apple Music) | 10 tests, 5 mutations | 95 | 75 | 35 | 85 | 65 | **67%** | ★★★½☆ | Beta | Fixed R&B / multi-word titles (cmd start quoting), no more false 'playing' claim or stray pause key, honest 'not installed', bare 'play <title>' now understood. Still needs a spoken Spotify run (W3). |
| Live mode: WhatsApp / browser flows | 45 | 90 | 75 | 45 | 80 | 45 | **65%** | ★★★½☆ | Beta | State machine, safety layer and WhatsApp name/number logic tested (case-insensitive tool matching, name cleaning keeps the spoken name as a last candidate). Still needs the current WhatsApp build run (W3). |
| Screen analysis / narrated tour | 60 Rust tour tests, 11 drawing styles previewed in a browser harness | 96 | 80 | 30 | 78 | 55 | **64%** | ★★★☆☆ | Alpha | Vision call now has a 20 s per-request limit (was 4 s, which made every tour fall back to OCR); text-only fallback explains the screen instead of reading junk; the model picks how to draw each item and states the intent; sidebar screenshots zoom. NOT run live: real Gemini latency, 125/150% scaling alignment, speech/callout timing. |
| Vision-grounded click | 43 tests | 85 | 66 | 25 | 70 | 40 | **54%** | ★★½☆☆ | Alpha | Measurement harness written (UIA ground truth vs the vision model, kept in the working tree with other uncommitted vision work); NOT run: it sends a screenshot of the whole screen to a cloud provider, so it needs a clean desktop. Found: Groq vision models are decommissioned, so auto uses Gemini only. |
| Reverse engineer website/app | 28 scanner + 15 app + 21 scroll + 12 runner tests | 85 | 65 | 0 | 80 | 40 | **48%** | ★★½☆☆ | Prototype | Built, never run live: stack scanner (private-address safe; a disguised-IPv6 hole was found by tests and fixed), app inspector, Tech Stack sidebar view (previewed light and dark), scrolling tour runner (window move, scroll and restore are untested). Needs the owner walkthrough on two real sites. |

### Agent control

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Watch one agent, then power off | ~100 tests | 100 | 85 | 65 | 95 | 70 | **81%** | ★★★★☆ | Production | Two practice-mode runs, verdict engine behaves; a real shutdown never run. |
| Keep-awake | 12 + 1 live | 100 | 80 | 65 | 80 | 70 | **78%** | ★★★★☆ | Beta | Real proof: execution state 0x3 while a lease is held, 0x0 after release. Fixed a stale-drop re-clear. Still needs: a long reply on battery and the system-only mode. |
| Gated commit + draft PR | 30 (5 real git) | 100 | 90 | 45 | 92 | 65 | **75%** | ★★★★☆ | Beta | Real git against a local bare remote: branch, commit, push, ignored files left out, secrets/detached/merge/hold refused, push failure keeps the commit. Still needs: a real GitHub draft PR from a throwaway repo (owner approval). |
| Ask-first shutdown / sleep / restart | 23 tests | 100 | 75 | 50 | 95 | 70 | **75%** | ★★★½☆ | Beta | Graceful close; a save prompt stops it. |
| Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) | 48 tests | 100 | 90 | 40 | 90 | 70 | **74%** | ★★★½☆ | Beta | Live: Antigravity start/stop, Claude prompt hook. Docs only: Codex, Devin, Windsurf, Kilo, Copilot, Cursor, Gemini, OpenCode, Aider. |
| Overnight record + morning briefing | 10 | 100 | 85 | 20 | 85 | 60 | **65%** | ★★★☆☆ | Alpha | Save, restart, tell once, expiry, corrupt record and new-record reset tested against the real sealed store. Still needs: a real shutdown and the next-morning briefing (W1). |
| Track all agents (announce each, summary) | 4+37 tests | 100 | 70 | 5 | 90 | 65 | **59%** | ★★★☆☆ | Alpha | Built today; no multi-agent live run. |
| Agent watch sidecar (spawn + watch CLIs) | 2 tests | 80 | 30 | 45 | 70 | 60 | **55%** | ★★★☆☆ | Alpha | Real PTY spawn verified once (Claude); others not installed here. |

### Memory and proactive

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Memory core (encrypted store, ranked recall, context pack) | 62 tests | 100 | 90 | 55 | 90 | 70 | **78%** | ★★★★☆ | Beta | Paraphrase recall 19/19 on dev set (tuned), 17/20 held-out. |
| Calendar read / add | 22 | 98 | 80 | 25 | 90 | 65 | **66%** | ★★★½☆ | Beta | Production flow now uses the tested helpers (specific spoken errors for expired sign-in, quota, network, timeout); evening/afternoon times understood; all-day events no longer treated as conflicts. Still needs: a real Google account run (W4). |
| Timetable from an image + reminders | 16 tests | 90 | 65 | 45 | 80 | 60 | **66%** | ★★★½☆ | Beta | Real Gemini read all 5 slots of a generated timetable. |
| Session snapshot / restore | 36 | 90 | 85 | 20 | 90 | 60 | **64%** | ★★★☆☆ | Alpha | Fixed: UNC paths could be launched on restore (NTLM leak risk); injectable launcher; 22 new tests; 'reopen them' is now understood as a yes. Still needs a real reboot (W4). |
| Boot briefing + resume points | 31 tests | 95 | 78 | 25 | 80 | 60 | **63%** | ★★★☆☆ | Alpha | Real reboot never tested. |
| Important-mail watcher + triage | 22 tests | 95 | 75 | 10 | 85 | 65 | **60%** | ★★★☆☆ | Alpha | No Google account connected at test time. |
| Profile, corrections, routines, review, curator | 59 tests | 95 | 75 | 10 | 85 | 60 | **59%** | ★★★☆☆ | Alpha | Review never run against a real model; routines have no history yet. |
| Proactive-speech policy | 15 tests | 90 | 70 | 10 | 85 | 55 | **56%** | ★★★☆☆ | Alpha | Thresholds are hypotheses; nothing emits a Critical alert yet. |
| Leave-by reminders | 12 tests, 5 mutations | 95 | 80 | 0 | 85 | 55 | **56%** | ★★★☆☆ | Alpha | Logic split into pure tested functions; route lookups no longer retried every minute after a failure. Still needs a real calendar event with an address (W4). |
| WhatsApp priority people (read-only) | 17+8 tests | 85 | 65 | 5 | 90 | 50 | **52%** | ★★½☆☆ | Alpha | OFF by default; read-receipt side effects unknown. |

### Integrations

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Cloudflare Worker | 145 tests | 100 | 80 | 75 | 85 | 75 | **82%** | ★★★★☆ | Production |  |
| GitHub commands (PRs, merge, review) | 61 tests | 100 | 85 | 70 | 85 | 70 | **81%** | ★★★★☆ | Production |  |
| Architecture mapper | 42 | 100 | 85 | 70 | 78 | 68 | **80%** | ★★★★☆ | Beta | 23 new pure tests; fixed .git suffix handling, API routes classified as server code (pages/api), deterministic tie-break. Network error mapping is still inline. |
| Maps / Places / Directions | 20 | 95 | 75 | 55 | 80 | 68 | **73%** | ★★★½☆ | Beta | Fixed: # and % and non-ASCII were not encoded in map URLs (apartment #4 truncated the query); empty/long addresses and HTTP errors now give speakable errors. |
| Google accounts + OAuth | 45 | 95 | 80 | 40 | 92 | 68 | **71%** | ★★★½☆ | Beta | Found and fixed: the sign-in callback never checked the OAuth state (code injection/CSRF) and treated /favicon.ico as a callback; disconnecting the last account left the legacy token. Not run: a real sign-in with the owner's client. |
| MCP connect system (Swiggy, Amazon, WhatsApp) | 28 tests | 100 | 75 | 40 | 85 | 70 | **70%** | ★★★½☆ | Beta | Swiggy production needs approval; bridges not running here. |
| Activity Manager (app-only history) | 13 tests, 8 mutations killed | 90 | 80 | 40 | 92 | 55 | **68%** | ★★★½☆ | Beta | Hardened (substring privacy list incl. keepassxc, rollback on failed save, tested sealed round-trip/tamper/expiry); real foreground+idle read verified. Still needs: a real 1-hour opted-in run and the Hub toggle used by the owner (W1). |
| Device Links (phone / laptop pairing) | 145 Worker tests share | 75 | 70 | 35 | 85 | 60 | **61%** | ★★★☆☆ | Alpha | Local Wrangler + D1 verified; no live SMS or two-laptop run. |

### Platform and quality

| Feature | Tests | B | T | L | S | I | Score | Stars | Verdict | Evidence / gap |
|---|---|---|---|---|---|---|---|---|---|---|
| Security: keychain, PII filter, sealed memory | 30 | 100 | 82 | 50 | 95 | 78 | **77%** | ★★★★☆ | Beta | Plaintext API keys (5 found in settings.json) are now moved to the keychain and blanked at startup, verified by read-back; readers are keychain-first; import replaces. Still needs the first real startup scrub and a restart check. |
| Command Hub / settings UI | frontend 507 total | 100 | 78 | 65 | 70 | 70 | **76%** | ★★★★☆ | Beta | Hub models, store and boundary tests added (about 140 frontend tests this session). No component render tests: the repo has no React testing library. |
| Resource footprint (RAM / CPU / battery) | measured | 100 | 40 | 80 | 75 | 80 | **75%** | ★★★★☆ | Beta | Idle 104-269 MB; Kokoro +368 MB when loaded. |
| TTS volume lease / restore | 14 tests | 100 | 70 | 55 | 85 | 75 | **74%** | ★★★½☆ | Beta |  |
| CI gates: dead code, IPC, turn-end, hygiene | scripts | 95 | 70 | 40 | 85 | 75 | **69%** | ★★★½☆ | Beta | Gates fail on unrelated concurrent items; the GitHub workflow itself never run. |
| Window theming (solid panels) | guard script over 11 CSS files + self-test | 90 | 70 | 0 | 70 | 60 | **52%** | ★★½☆☆ | Alpha | About 20 translucent or blurred layers removed from panel windows; a guard script fails CI if they return. Not seen in the real app yet; the guard only covers the surfaces it lists. |
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
| Speaker verification / owner check | 41% | Enrolment exists in the setup wizard (not in the Command Hub) and the gate is wired, but speakerVerification is off and no voice profile exists on this PC. Needs your enrolment and a false-accept/reject measurement. |
| Installer / release pipeline | 47% | NSIS broken locally; MSI built; start-menu shortcut points at a stale exe. |
| Reverse engineer website/app | 48% | Built, never run live: stack scanner (private-address safe; a disguised-IPv6 hole was found by tests and fixed), app inspector, Tech Stack sidebar view (previewed light and dark), scrolling tour runner (window move, scroll and restore are untested). Needs the owner walkthrough on two real sites. |
| Window theming (solid panels) | 52% | About 20 translucent or blurred layers removed from panel windows; a guard script fails CI if they return. Not seen in the real app yet; the guard only covers the surfaces it lists. |
| WhatsApp priority people (read-only) | 52% | OFF by default; read-receipt side effects unknown. |
| Vision-grounded click | 54% | Measurement harness written (UIA ground truth vs the vision model, kept in the working tree with other uncommitted vision work); NOT run: it sends a screenshot of the whole screen to a cloud provider, so it needs a clean desktop. Found: Groq vision models are decommissioned, so auto uses Gemini only. |
| Agent watch sidecar (spawn + watch CLIs) | 55% | Real PTY spawn verified once (Claude); others not installed here. |
| Proactive-speech policy | 56% | Thresholds are hypotheses; nothing emits a Critical alert yet. |
| Leave-by reminders | 56% | Logic split into pure tested functions; route lookups no longer retried every minute after a failure. Still needs a real calendar event with an address (W4). |
| Wake-up sound | 57% | Nothing audible has been heard yet. |
| Boot hello / shutdown flow / resume hello | 57% | Built in two rounds; nothing run live after rebuild. |
| Track all agents (announce each, summary) | 59% | Built today; no multi-agent live run. |

## Out of the current target (decision recorded)

| Feature | Score | Why it is out |
|---|---|---|
| Streaming partial captions (Deepgram, optional) | 45% | D4: optional streaming captions needing a paid key; revisit if wanted |
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
