# Windows 1.0 production-readiness queue — 2026-10-26

**Active work:** speaker verification remains **41% (2.05/5)**. Frozen source `f0988d9` passes all shared gates and 40 mutations; release `d846dd346c470b60` is built and provenance-verified. The owner now controls runtime/builds; assistant work is edits only. Previous release `0b10fd3a1416a540` passed shipping settings checks but failed the requested owner voice action. Neutral decoding and native local execution repairs require a fresh live check. [Current assessment](runs/windows1-20261026-neutral-local-assessment.json), [change167](../changes/167-neutral-decoding-and-native-local-execution.md).

P3 source-only tooling now includes [enrollment-isolated embedding scores](../changes/169-enrollment-isolated-embedding-scores.md): 63 Python script tests and 18 offline mutations pass. These edits have no application-build or corpus/live evidence; the frozen release checks above remain separately attributed.

[P4/P5/P6 source checkpoint](../changes/170-voice-trial-accounting-and-phase-closeout.md): current permitted suites and79 mutations pass; legacy CSV qualification is blocked and richer declared-trial evidence is separate from readiness. [All unmet phase gates](voice-lock-phase-closeout.md) remain explicit.

Source: `docs/testing/scorecard-data.json`. 65 in-scope features; none is declared production ready. Scores are provisional rubric estimates, not measured accuracy. The older 80% classification and previous running-process notes are historical; failures/build-attribution corrections remain in the [ledger](live-proof-ledger.md).

Production ready requires raw score at least 90% (4.5/5), B>=90, T>=70, L>=70, S>=80, I>=60, safety mutations, passing shared gates on the assessed revision, fresh release provenance, real-device proof and no contradictory known failure.

Do not advance to the next feature while the speaker [work card](speaker-verification-work-card.md) lacks its required owner/non-owner/replay, enrollment, profile-recovery and capture-accounting proof. Recalculate queue order after a completed feature.

| Order | Feature | Raw score | Stars / 5 | Tests / evidence | Minimum-axis gaps | Observed gap / next proof |
|---:|---|---:|---:|---|---|---|
| 1 | Speaker verification / owner check | 41.0% | 2.05 | Frozen f0988d9 shared gates: Rust 2537 library + 19 integration (38 ignored), frontend 687 + TypeScript/build, Worker 145, Python 60, NLU/repository guards, dead-code 148 known/none new; 40/40 mutations. Release d846dd346c470b60 built and provenance-verified; repaired owner action proof pending. Runtime/builds are owner-controlled. Separate P3 tooling: 63 script tests / 18 offline mutations; no model/corpus/live proof (change169). | B 75/90, T 55/70, L 10/70, S 50/80, I 30/60 | In progress at 41% (2.05/5). Prior release 0b10fd3a1416a540 passed shipping settings checks, but the owner Notepad action failed despite verified ownership. One consented same-clip experiment reproduced command duplication with the owner vocabulary (2/2), absent with neutral decoding (2/2). Native local execution was also missing. Both fixes are frozen at f0988d9 / d7b49eeb; all shared gates pass, release d846dd346c470b60 is built and provenance-verified. The owner now controls runtime and rebuilds; edits only. No formal owner/impostor rate; no promotion. Stage heartbeat, replacement voice action, ten-start accounting, assessed-build enrollment repetition, real missing/corrupt/deleted-profile recovery and owner/non-owner/TV/replay proof remain required. Earlier failures and attribution corrections are retained in the live-proof ledger. See docs/changes/167-neutral-decoding-and-native-local-execution.md and docs/testing/runs/windows1-20261026-neutral-local-assessment.json. |
| 2 | Installer / release pipeline | 47.0% | 2.35 | - | B 70/90, T 30/70, L 35/70, S 60/80, I 50/60 | NSIS broken locally; MSI built; start-menu shortcut points at a stale exe. |
| 3 | Reverse engineer website/app | 48.0% | 2.40 | 28 scanner + 15 app + 21 scroll + 12 runner tests | B 85/90, T 65/70, L 0/70, I 40/60 | Built, never run live: stack scanner (private-address safe; a disguised-IPv6 hole was found by tests and fixed), app inspector, Tech Stack sidebar view (previewed light and dark), scrolling tour runner (window move, scroll and restore are untested). Needs the owner walkthrough on two real sites. |
| 4 | Window theming (solid panels) | 51.5% | 2.58 | guard script over 11 CSS files + self-test | L 0/70, S 70/80 | About 20 translucent or blurred layers removed from panel windows; a guard script fails CI if they return. Not seen in the real app yet; the guard only covers the surfaces it lists. |
| 5 | WhatsApp priority people (read-only) | 52.5% | 2.62 | 17+8 tests | B 85/90, T 65/70, L 5/70, I 50/60 | OFF by default; read-receipt side effects unknown. |
| 6 | Vision-grounded click | 54.2% | 2.71 | 43 tests | B 85/90, T 66/70, L 25/70, S 70/80, I 40/60 | Measurement harness written (UIA ground truth vs the vision model, kept in the working tree with other uncommitted vision work); NOT run: it sends a screenshot of the whole screen to a cloud provider, so it needs a clean desktop. Found: Groq vision models are decommissioned, so auto uses Gemini only. |
| 7 | Agent watch sidecar (spawn + watch CLIs) | 55.0% | 2.75 | 2 tests | B 80/90, T 30/70, L 45/70, S 70/80 | Real PTY spawn verified once (Claude); others not installed here. |
| 8 | Leave-by reminders | 56.0% | 2.80 | 12 tests, 5 mutations | L 0/70, I 55/60 | Logic split into pure tested functions; route lookups no longer retried every minute after a failure. Still needs a real calendar event with an address (W4). |
| 9 | Proactive-speech policy | 56.0% | 2.80 | 15 tests | L 10/70, I 55/60 | Thresholds are hypotheses; nothing emits a Critical alert yet. |
| 10 | Boot hello / shutdown flow / resume hello | 57.0% | 2.85 | mixed | T 50/70, L 25/70, S 70/80 | Built in two rounds; nothing run live after rebuild. |
| 11 | Wake-up sound | 57.0% | 2.85 | 11 tests | T 65/70, L 10/70 | Nothing audible has been heard yet. |
| 12 | Profile, corrections, routines, review, curator | 58.8% | 2.94 | 59 tests | L 10/70 | Review never run against a real model; routines have no history yet. |
| 13 | Track all agents (announce each, summary) | 58.8% | 2.94 | 4+37 tests | L 5/70 | Built today; no multi-agent live run. |
| 14 | Important-mail watcher + triage | 59.5% | 2.98 | 22 tests | L 10/70 | No Google account connected at test time. |
| 15 | NLU fallback (BERT-Mini ONNX) | 59.8% | 2.99 | 13 tests | T 55/70, L 40/70, S 65/80 | Candidate model 0.9004 test intent; shipped model may lag; OTA update path built, never run end to end. |
| 16 | Endpointing / VAD / turn detection | 60.0% | 3.00 | 5 tests | T 45/70, L 55/70, S 60/80, I 50/60 | Adaptive 240-800 ms; age-bias (older speakers cut off) unmeasured. |
| 17 | Device Links (phone / laptop pairing) | 61.2% | 3.06 | 145 Worker tests share | B 75/90, L 35/70 | Local Wrangler + D1 verified; no live SMS or two-laptop run. |
| 18 | Boot briefing + resume points | 63.1% | 3.16 | 31 tests | L 25/70 | Real reboot never tested. |
| 19 | Session snapshot / restore | 63.5% | 3.17 | 36 | L 20/70 | Fixed: UNC paths could be launched on restore (NTLM leak risk); injectable launcher; 22 new tests; 'reopen them' is now understood as a yes. Still needs a real reboot (W4). |
| 20 | Screen analysis / narrated tour | 64.2% | 3.21 | 60 Rust tour tests, 11 drawing styles previewed in a browser harness | L 30/70, S 78/80, I 55/60 | Vision call now has a 20 s per-request limit (was 4 s, which made every tour fall back to OCR); text-only fallback explains the screen instead of reading junk; the model picks how to draw each item and states the intent; sidebar screenshots zoom. NOT run live: real Gemini latency, 125/150% scaling alignment, speech/callout timing. |
| 21 | Overnight record + morning briefing | 64.8% | 3.24 | 10 | L 20/70 | Save, restart, tell once, expiry, corrupt record and new-record reset tested against the real sealed store. Still needs: a real shutdown and the next-morning briefing (W1). |
| 22 | Live mode: WhatsApp / browser flows | 65.2% | 3.26 | 45 | L 45/70, I 45/60 | State machine, safety layer and WhatsApp name/number logic tested (case-insensitive tool matching, name cleaning keeps the spoken name as a last candidate). Still needs the current WhatsApp build run (W3). |
| 23 | Timetable from an image + reminders | 65.5% | 3.27 | 16 tests | T 65/70, L 45/70 | Real Gemini read all 5 slots of a generated timetable. |
| 24 | Calendar read / add | 66.3% | 3.32 | 22 | L 25/70 | Production flow now uses the tested helpers (specific spoken errors for expired sign-in, quota, network, timeout); evening/afternoon times understood; all-day events no longer treated as conflicts. Still needs: a real Google account run (W4). |
| 25 | TTS offline (Kokoro-82M) | 66.5% | 3.33 | 27 tests | L 50/70, I 50/60 | Real-time on CPU only (RTF 0.7-2.4); deliberately NOT the live voice. |
| 26 | Music control (Spotify / Apple Music) | 67.0% | 3.35 | 10 tests, 5 mutations | L 35/70 | Fixed R&B / multi-word titles (cmd start quoting), no more false 'playing' claim or stray pause key, honest 'not installed', bare 'play <title>' now understood. Still needs a spoken Spotify run (W3). |
| 27 | Directed-speech gate (TV / side-talk filter) | 67.0% | 3.35 | 27 + 145-line corpus | L 35/70, I 55/60 | 145-line corpus: noise rejected 98.7%; fixed 'tell him I'll call back' being accepted (tell now needs me/us/nexus/an agent). Corpus was written by the engineer, so still optimistic. Needs the 30-minute live TV test. |
| 28 | Main centre / 17 sub-centres validation | 67.5% | 3.38 | 16 tests | T 65/70, L 45/70 | Typed ActionSpec, shared policy; live walkthrough pending. |
| 29 | Compound commands ("X then Y") | 68.0% | 3.40 | 31 tests | L 40/70, I 55/60 | GitHub step inside a compound drops the remaining steps (known). |
| 30 | Activity Manager (app-only history) | 68.0% | 3.40 | 13 tests, 8 mutations killed | L 40/70, I 55/60 | Hardened (substring privacy list incl. keepassxc, rollback on failed save, tested sealed round-trip/tamper/expiry); real foreground+idle read verified. Still needs: a real 1-hour opted-in run and the Hub toggle used by the owner (W1). |
| 31 | CI gates: dead code, IPC, turn-end, hygiene | 69.0% | 3.45 | scripts | L 40/70 | Gates fail on unrelated concurrent items; the GitHub workflow itself never run. |
| 32 | Hand mode (camera pointer) | 69.0% | 3.45 | 80+ tests | L 55/70, I 45/60 | Pinch 'almost perfect' per owner; gestures, multi-monitor, macOS not run; camera 15 fps on battery. |
| 33 | MCP connect system (Swiggy, Amazon, WhatsApp) | 70.2% | 3.51 | 28 tests | L 40/70 | Swiggy production needs approval; bridges not running here. |
| 34 | Google accounts + OAuth | 71.0% | 3.55 | 45 | L 40/70 | Found and fixed: the sign-in callback never checked the OAuth state (code injection/CSRF) and treated /favicon.ico as a callback; disconnecting the last account left the legacy token. Not run: a real sign-in with the owner's client. |
| 35 | Captions (word-by-word, black capsule) | 71.5% | 3.58 | vitest | L 55/70, S 70/80 | Estimated (not real) word timing on the offline voice. |
| 36 | Dictation (type what I say) | 71.5% | 3.58 | 20 + 156-pair corpus | L 40/70 | 156-pair spoken->written corpus gates the cleaner. Known quirks: 'one dollar' -> '1 dollars', proper nouns stay lowercase. Still needs real dictation into apps (W3). |
| 37 | Wake word "NEXUS" (openWakeWord, own model) | 71.8% | 3.59 | 64 tests, replay harness | L 55/70, S 75/80, I 40/60 | Works live, but the honest held-out measure is recall 63.2% (quiet 3/10, far 15/25) and ~12 false fires/h on real background; the 98.6% figure was self-scored on training data. Target is >=95% at <=0.1/h. |
| 38 | Orb UI and turn lifecycle | 71.9% | 3.60 | vitest | L 60/70, S 65/80 | Store transitions, thinking dwell and watchdog inputs now tested; the 50-turn soak is still needed. |
| 39 | Ghost mode (voice drives the PC) | 72.0% | 3.60 | 16+ tests | T 65/70, L 60/70, I 55/60 | Keyboard-first is where the industry is reliable (>=89%); cursor/vision clicks sit in a 20-55% band. |
| 40 | Mishearing normaliser + learned corrections | 72.0% | 3.60 | 45 | L 55/70, S 75/80 | Real log replay: 31 of 194 unique missed commands are now understood; added openbread/open brief/command hop. 163 still missed (see ledger). |
| 41 | Local STT fallback (Moonshine) | 72.5% | 3.62 | python sidecar | L 60/70, S 75/80 | Fallback decisions tested; airplane-mode walkthrough still to do. |
| 42 | Maps / Places / Directions | 72.7% | 3.64 | 20 | L 55/70 | Fixed: # and % and non-ASCII were not encoded in map URLs (apartment #4 truncated the query); empty/long addresses and HTTP errors now give speakable errors. |
| 43 | Shortcut map + action router | 73.2% | 3.66 | 55 | L 45/70 | 16 new tests: seed validity, precedence for 5 real app cases, terminals never inherit Ctrl+C/Z/S/X/A/Y, router gates and call order. No wrong chord found in the seed. Tested on Notepad only live. |
| 44 | Voice personas + offline twin swap | 73.8% | 3.69 | 31 tests | L 55/70 | Pinned hashes, latest-wins queue. Audition by ear not done. |
| 45 | Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) | 74.0% | 3.70 | 48 tests | L 40/70 | Live: Antigravity start/stop, Claude prompt hook. Docs only: Codex, Devin, Windsurf, Kilo, Copilot, Cursor, Gemini, OpenCode, Aider. |
| 46 | TTS volume lease / restore | 74.5% | 3.73 | 14 tests | L 55/70 |  |
| 47 | Ask-first shutdown / sleep / restart | 74.8% | 3.74 | 23 tests | L 50/70 | Graceful close; a save prompt stops it. |
| 48 | Gated commit + draft PR | 75.0% | 3.75 | 30 (5 real git) | L 45/70 | Real git against a local bare remote: branch, commit, push, ignored files left out, secrets/detached/merge/hold refused, push failure keeps the commit. Still needs: a real GitHub draft PR from a throwaway repo (owner approval). |
| 49 | Resource footprint (RAM / CPU / battery) | 75.2% | 3.76 | measured | T 40/70, S 75/80 | Idle 104-269 MB; Kokoro +368 MB when loaded. |
| 50 | Window tiling + display off | 75.7% | 3.79 | 21 pure + 2 live | L 55/70 | Real proof: 3 own windows tiled into exact 640px columns (0-640-1280-1920) and display_off returned Ok. Still needs: the finish sequence on real agent windows, the screen actually going dark seen by the owner, an elevated window skip (W1). |
| 51 | 9Router (Cerebras > Groq > Gemini > Worker) | 76.0% | 3.80 | 18 tests | T 55/70, S 75/80 | ~242 ms vs 600-2100 ms through the Worker. |
| 52 | Command Hub / settings UI | 76.1% | 3.80 | frontend 507 total | L 65/70, S 70/80 | Hub models, store and boundary tests added (about 140 frontend tests this session). No component render tests: the repo has no React testing library. |
| 53 | Open / close / focus apps | 76.8% | 3.84 | 15+ tests | T 55/70, S 75/80 | Used constantly; duplicate-window bug fixed. |
| 54 | Action Kernel (voice file control, quarantine delete, undo) | 77.0% | 3.85 | 53 | L 45/70 | Spoken grammar -> confirm -> trash -> undo on real folders (unicode, long names, clashes, forbidden places, wrong origin). Still needs: one spoken delete and undo by voice (W3). |
| 55 | Security: keychain, PII filter, sealed memory | 77.3% | 3.87 | 30 | L 50/70 | Plaintext API keys (5 found in settings.json) are now moved to the keychain and blanked at startup, verified by read-back; readers are keychain-first; import replaces. Still needs the first real startup scrub and a restart check. |
| 56 | Keep-awake | 78.0% | 3.90 | 12 + 1 live | L 65/70 | Real proof: execution state 0x3 while a lease is held, 0x0 after release. Fixed a stale-drop re-clear. Still needs: a long reply on battery and the system-only mode. |
| 57 | Memory core (encrypted store, ranked recall, context pack) | 78.5% | 3.92 | 62 tests | L 55/70 | Paraphrase recall 19/19 on dev set (tuned), 17/20 held-out. |
| 58 | Architecture mapper | 79.9% | 4.00 | 42 | S 78/80 | 23 new pure tests; fixed .git suffix handling, API routes classified as server code (pages/api), deterministic tie-break. Network error mapping is still inline. |
| 59 | Deterministic intent parser (regex tier) | 81.0% | 4.05 | 236 tests | S 75/80, I 55/60 | Strongest tested module. Early P0 measure: bare command 43% correct / 12% wrong action; since improved but not re-measured. |
| 60 | GitHub commands (PRs, merge, review) | 81.2% | 4.06 | 61 tests | Axis minima met; release/live gates still require review |  |
| 61 | Watch one agent, then power off | 81.2% | 4.06 | ~100 tests | L 65/70 | Two practice-mode runs, verdict engine behaves; a real shutdown never run. |
| 62 | Cloudflare Worker | 82.5% | 4.12 | 145 tests | Axis minima met; release/live gates still require review |  |
| 63 | Global hotkey Ctrl+Space (call / cancel) | 85.2% | 4.26 | 5 | Axis minima met; release/live gates still require review | Cancel/call decision extracted and checked for all 16 states; stale doc comment rewritten. |
| 64 | TTS online (Edge, persistent connection) | 85.8% | 4.29 | 44 tests | Axis minima met; release/live gates still require review | 409 ms median per short line, 290 ms after wake pre-warm; ~2.6 s end of speech to first audio for a cloud reply. |
| 65 | Cloud STT (Groq Whisper turbo) + hallucination filter | 87.7% | 4.38 | 60 | Axis minima met; release/live gates still require review | Routing extracted and tested (privacy mode never reaches a cloud step); fixed empty-buffer upload and NaN energy; Deepgram key now honoured by the main entry point; provider-side audio rejection still falls back to local. |

Recalculate this ascending order from the data after each completed feature. A blocked or failed live run keeps the current feature in progress.

Administrator control, the admin brain, macOS/Linux parity, and optional Deepgram captions remain outside this Windows queue; their omission is not a readiness claim.

## Owner enrollment interaction result

Owner live report on candidate 031caf5848621e18: all ten enrollment clips completed without waking NEXUS. Enrollment wake isolation passes this interaction check. Normal wake recovery, cancellation recovery, owner/non-owner accuracy and profile recovery remain pending. These ten calibration/enrollment clips count as zero acceptance trials. Score remains 41% (2.05/5); queue stays on speaker verification.

### Post-enrollment wake recovery

Owner confirmed normal waking with Nexus, Hey Nexus and OK Nexus after the ten-clip enrollment on release 031caf5848621e18, with Voice Lock off. Enrollment isolation and post-completion wake recovery have passed owner interaction checks. Cancellation recovery and locked owner/non-owner/TV accuracy remain pending. No accuracy trials counted; score stays 41%.

### Cancellation interaction result

Owner reports both enrollment cancellation checks passed on 031caf5848621e18: navigation during countdown and during recording stopped recording and restored normal waking, with Voice Lock off. Pending-permission navigation is not live-verified. Locked owner/non-owner/TV accuracy, restart and profile recovery remain pending. Score stays 41%; no accuracy trials counted.

## Live build attribution correction

Build attribution correction: the owner supplied console logs showing nexus start replaced the assessed candidate with D: release 6f4e1063a38683a3 at 16:54, followed by 3d1932b68b2c0f15 at 17:36. Earlier owner enrollment/wake/cancellation reports remain observations, but exact assessed-build attribution is unconfirmed and must be repeated. The Notepad turn at 17:37:41 was owner=unenrolled with Voice Lock off; it is excluded from locked accuracy counts. Current device setting was confirmed false; a profile file exists. Candidate 031caf5848621e18 was restored with unique logs, PID 28952. No score increase or queue advancement.
