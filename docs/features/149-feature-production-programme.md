# 149 - Feature production programme: every feature to 80% / four stars

Status: PLAN. Date 2026-10-20. Companion to `docs/research/feature-readiness-scorecard-2026-10-20.md` and `docs/testing/scorecard-data.json` (regenerate with `python scripts/scorecard.py report`).

## 1. Goal and definition of done

Every feature reaches **>= 80% and >= 4 stars** on the scorecard, one feature at a time, weakest first.

A feature is DONE only when all of these hold (so a score cannot be reached by polishing one axis):

1. Score >= 80% (stars = score / 20, so 80% is exactly four stars; 85% is 4.5).
2. **Live proof L >= 70**: the feature ran in the real app or on real hardware, and the run is recorded in the proof ledger (section 4).
3. Floors on the other axes: **B >= 90, T >= 70, S >= 80, I >= 60**.
4. Every safety rule has a test that fails when the rule is removed (mutation-checked), and the failure text is honest.
5. The scorecard row, `AGENTS.md`, and the change record are updated in the same commit.

## 2. The loop for each feature

1. **Read** the code and the last evidence; write the gap list in the work card (section 7).
2. **Tests first** for each gap; watch them fail.
3. **Fix** with narrow, anchored patches (other sessions edit `orchestrator.rs`, `intent_parser.rs`, `center.rs`).
4. **Mutation-check** each safety rule (revert the rule, a test must fail).
5. **Prove it for real** (section 3): BOT proof by me on this laptop, OWNER proof by you with the checklist, EXT/DATA when something outside the repo is needed.
6. **Record** the run in the ledger (date, commit, what was run, result, log lines).
7. **Re-score** with `python scripts/scorecard.py set <feature> L=.. T=.. ...`, run `report`, update `AGENTS.md`, commit.

## 3. Four kinds of proof (this is what limits the speed)

| Proof | Who | Example | What it unlocks |
|---|---|---|---|
| **BOT** | me, on this laptop | spawn real windows and tile them; real git in a temp repo; real hook script into the real webhook; `powercfg /requests`; replay harness | L up to about 70 without you |
| **OWNER** | you, with the checklist | speak a phrase, watch the screen, hear the voice, read the log | L 70-90; the only way to prove voice, audio, and feel |
| **EXT** | outside the repo | second laptop, a Mac, a bridge, an account, an approval | rows that cannot finish alone |
| **DATA** | recordings / labels | 10 h of ambient audio for the wake word | quality (I) rows |

Installed on this laptop (so BOT hook proofs are possible): Claude Code, Codex, Antigravity (`agy`), Cursor, Kilo, Aider, Copilot CLI, `gh`, `git`, Node, Python, Rust. **Not installed:** Devin, Windsurf, OpenCode, Gemini CLI. A release `nexus.exe` exists (built today 12:57) but predates the newest commit, so every OWNER walkthrough starts with `nexus build`.

## 4. Proof ledger

New file `docs/testing/live-proof-ledger.md`, one line per run: `date | commit | feature | proof (BOT/OWNER) | what was run | result | evidence (log lines or path)`. A row's L score may only be raised for a run that has a ledger line.

## 5. Decisions (my defaults; flip any of them)

| # | Decision | Default |
|---|---|---|
| D1 | macOS / Linux parity | Out of the Windows 1.0 target, tracked as its own programme (no Mac here) |
| D2 | Administrator control and the admin Qwen brain | Out of the target: you closed administrator access by design; admin brain is compiled out of family builds |
| D3 | Speaker verification | In scope: it is the real fix for TV / side talk, needs about 30 min of your recordings |
| D4 | Deepgram streaming captions | Finish it or remove it from the scorecard (optional feature) |
| D5 | Agent-watch sidecar | Keep only if still needed once hooks exist; otherwise delete |

Without D1 and D2 the macOS and Administrator rows cannot reach 80% from this machine, so they are the only rows I would not promise.

## 6. Work queue (weakest first)

Effort: S about half a session, M one to two, L several. `Floors` lists axes under the definition-of-done floors (B 90, T 70, S 80, I 60).

| # | Feature | Now | Stars | Proof | Effort | Floors to fix | Needs |
|---|---|---|---|---|---|---|---|
| 1 | macOS / Linux parity | 23% | ★☆☆☆☆ | EXT | L | B, T, L, S, I | D1 |
| 2 | Administrator control | 30% | ★½☆☆☆ | OWNER+EXT | L | B, T, L, I | D2 |
| 3 | Activity Manager (app-only history) | 35% | ★½☆☆☆ | BOT+OWNER | M | B, T, L, I | - |
| 4 | Speaker verification / owner check | 37% | ★★☆☆☆ | DATA+OWNER | L | B, T, L, S, I | D3 |
| 5 | Leave-by reminders | 44% | ★★☆☆☆ | OWNER | S | T, L, S, I | Google connected |
| 6 | Streaming partial captions (Deepgram, optional) | 45% | ★★☆☆☆ | EXT | M | B, T, L, S | D4 |
| 7 | Installer / release pipeline | 47% | ★★½☆☆ | BOT+OWNER | M | B, T, L, S, I | - |
| 8 | Music control (Spotify / Apple Music) | 49% | ★★½☆☆ | OWNER | S | B, T, L, S, I | - |
| 9 | Admin brain (Qwen, admin only) | 50% | ★★½☆☆ | BOT | S | B, T, L, S, I | D2 |
| 10 | WhatsApp priority people (read-only) | 52% | ★★½☆☆ | OWNER+EXT | M | B, T, L, I | WhatsApp bridge running |
| 11 | Window tiling + display off | 53% | ★★½☆☆ | BOT | S | T, L, I | - |
| 12 | Vision-grounded click | 53% | ★★½☆☆ | BOT+OWNER | M | B, T, L, S, I | - |
| 13 | Calendar read / add | 54% | ★★½☆☆ | OWNER | S | T, L | Google connected |
| 14 | Live mode: WhatsApp / browser flows | 55% | ★★½☆☆ | OWNER | M | B, T, L, S, I | - |
| 15 | Overnight record + morning briefing | 55% | ★★½☆☆ | BOT+OWNER | S | T, L | - |
| 16 | Session snapshot / restore | 55% | ★★½☆☆ | BOT | M | T, L, I | - |
| 17 | Agent watch sidecar (spawn + watch CLIs) | 55% | ★★★☆☆ | BOT | S | B, T, L, S | D5 |
| 18 | Proactive-speech policy | 56% | ★★★☆☆ | OWNER | M | L, I | - |
| 19 | Wake-up sound | 57% | ★★★☆☆ | OWNER | S | T, L | - |
| 20 | Boot hello / shutdown flow / resume hello | 57% | ★★★☆☆ | OWNER | S | T, L, S | rebuilt app |
| 21 | Gated commit + draft PR | 58% | ★★★☆☆ | BOT+OWNER | S | L | - |
| 22 | Track all agents (announce each, summary) | 59% | ★★★☆☆ | BOT+OWNER | S | L | - |
| 23 | Profile, corrections, routines, review, curator | 59% | ★★★☆☆ | OWNER | M | L | time |
| 24 | Important-mail watcher + triage | 60% | ★★★☆☆ | OWNER | S | L | Google connected |
| 25 | NLU fallback (BERT-Mini ONNX) | 60% | ★★★☆☆ | BOT | M | T, L, S | - |
| 26 | Endpointing / VAD / turn detection | 60% | ★★★☆☆ | DATA | M | T, L, S, I | - |
| 27 | Screen analysis / narrated tour | 61% | ★★★☆☆ | BOT+OWNER | M | L, S, I | - |
| 28 | Device Links (phone / laptop pairing) | 61% | ★★★☆☆ | EXT | M | B, L | second device |
| 29 | Boot briefing + resume points | 62% | ★★★☆☆ | OWNER | S | L | rebuilt app |
| 30 | Google accounts + OAuth | 62% | ★★★☆☆ | OWNER | S | T, L | - |
| 31 | Directed-speech gate (TV / side-talk filter) | 62% | ★★★☆☆ | DATA+OWNER | M | T, L, S, I | - |
| 32 | Action Kernel (voice file control, quarantine delete, undo) | 63% | ★★★☆☆ | BOT+OWNER | S | L | - |
| 33 | Keep-awake | 64% | ★★★☆☆ | BOT | S | T, L, S | - |
| 34 | Mishearing normaliser + learned corrections | 64% | ★★★☆☆ | BOT | S | T, L, S, I | - |
| 35 | Maps / Places / Directions | 65% | ★★★☆☆ | BOT+OWNER | S | T, L, S | - |
| 36 | Dictation (type what I say) | 65% | ★★★☆☆ | BOT | M | T, L, I | - |
| 37 | Timetable from an image + reminders | 66% | ★★★½☆ | OWNER | S | T, L | - |
| 38 | Local STT fallback (Moonshine) | 66% | ★★★½☆ | BOT | S | T, L, S | - |
| 39 | TTS offline (Kokoro-82M) | 66% | ★★★½☆ | BOT+OWNER | M | L, I | - |
| 40 | Main centre / 17 sub-centres validation | 68% | ★★★½☆ | OWNER | S | T, L | - |
| 41 | Compound commands ("X then Y") | 68% | ★★★½☆ | BOT+OWNER | M | L, I | - |
| 42 | Architecture mapper | 68% | ★★★½☆ | BOT | S | T, S | - |
| 43 | Orb UI and turn lifecycle | 68% | ★★★½☆ | OWNER | S | T, L, S | - |
| 44 | Hand mode (camera pointer) | 69% | ★★★½☆ | OWNER | M | L, I | - |
| 45 | CI gates: dead code, IPC, turn-end, hygiene | 69% | ★★★½☆ | BOT+OWNER | S | L | approval to push |
| 46 | Shortcut map + action router | 69% | ★★★½☆ | BOT | S | T, L | - |
| 47 | Captions (word-by-word, black capsule) | 70% | ★★★½☆ | OWNER | S | T, L, S | - |
| 48 | MCP connect system (Swiggy, Amazon, WhatsApp) | 70% | ★★★½☆ | EXT | M | L | bridge / approval |
| 49 | Global hotkey Ctrl+Space (call / cancel) | 72% | ★★★½☆ | BOT | S | T | - |
| 50 | Wake word "NEXUS" (openWakeWord, own model) | 72% | ★★★½☆ | DATA+OWNER | L | L, S, I | - |
| 51 | Ghost mode (voice drives the PC) | 72% | ★★★½☆ | OWNER | S | T, L, I | - |
| 52 | Command Hub / settings UI | 72% | ★★★½☆ | BOT | S | T, L, S | - |
| 53 | Voice personas + offline twin swap | 74% | ★★★½☆ | OWNER | S | L | - |
| 54 | Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) | 74% | ★★★½☆ | BOT+EXT | M | L | - |
| 55 | TTS volume lease / restore | 74% | ★★★½☆ | BOT | S | L | - |
| 56 | Ask-first shutdown / sleep / restart | 75% | ★★★½☆ | OWNER | S | L | - |
| 57 | Security: keychain, PII filter, sealed memory | 75% | ★★★★☆ | BOT | S | L | - |
| 58 | Resource footprint (RAM / CPU / battery) | 75% | ★★★★☆ | BOT | S | T, S | - |
| 59 | 9Router (Cerebras > Groq > Gemini > Worker) | 76% | ★★★★☆ | BOT | S | T, S | - |
| 60 | Open / close / focus apps | 77% | ★★★★☆ | BOT | S | T, S | - |
| 61 | Memory core (encrypted store, ranked recall, context pack) | 78% | ★★★★☆ | BOT+OWNER | S | L | - |

61 features are below 80%. Rough size: about 64 working sessions in total, of which the wake word alone is about 4 and most S rows are quick once the proof harness exists.

## 7. Work cards (one per feature, in queue order)

Each card: what must be true to count as DONE for that feature. Tests and proof are in addition to the standard loop.

### 1. macOS / Linux parity - 23% ★☆☆☆☆

- Now: B30 T25 L0 S50 I30. Gap on record: Windows-first; hand mode live, tiling, display-off are Windows only.
- Proof: **EXT**; effort **L**; needs: D1
- D1 recommendation: take it OUT of the Windows 1.0 target and track it as its own programme (needs a Mac; none here).
- If kept in scope: port hotkey, display-off, window tiling, keep-awake, shortcut map, UIA-free grounding to macOS Accessibility; CI matrix on macos-latest; a live walkthrough on a real Mac.

### 2. Administrator control - 30% ★½☆☆☆

- Now: B25 T45 L0 S90 I20. Gap on record: Policy and provenance only; every executor is closed. Do not present as ready.
- Proof: **OWNER+EXT**; effort **L**; needs: D2
- D2 recommendation: stays CLOSED by your own decision; exclude from the 80% target until you open it.
- If opened: enrolment, signed bootstrap, voice/face/challenge verifier, session opener, executor, delivery, red-team corpus; each is a security-critical build with its own review.

### 3. Activity Manager (app-only history) - 35% ★½☆☆☆

- Now: B70 T10 L0 S85 I40. Gap on record: Off by default.
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Tests: 30-day purge, encryption at rest, off-by-default, foreground filter, no title capture for protected windows (target 12+ tests).
- Real 1-hour run on Windows; read the sealed store back; Command Hub toggle + clear button; document what is stored.

### 4. Speaker verification / owner check - 37% ★★☆☆☆

- Now: B60 T50 L10 S50 I30. Gap on record: Wired, off by default, fail-open; enrolment model (G1) not built.
- Proof: **DATA+OWNER**; effort **L**; needs: D3
- Enrolment by voice ('enrol my voice', 5 phrases), stored sealed; threshold set from your recordings; measure false-accept / false-reject with you, one other voice, and a TV.
- Make fail-open a visible setting; add the enrolled-owner signal to the directed gate. Needs about 30 minutes of recordings from you.

### 5. Leave-by reminders - 44% ★★☆☆☆

- Now: B90 T35 L0 S75 I55. Gap on record: Needs a real calendar event.
- Proof: **OWNER**; effort **S**; needs: Google connected
- Mock Calendar + Routes tests for: no home saved, no address, traffic spike, event moved, already left (target 12 tests).
- Live: a real event 2 hours ahead with a street address; confirm one spoken 'time to leave' and no repeats.

### 6. Streaming partial captions (Deepgram, optional) - 45% ★★☆☆☆

- Now: B80 T25 L20 S60 I60. Gap on record: Optional, needs a key; stream module has no tests.
- Proof: **EXT**; effort **M**; needs: D4
- D4 recommendation: it is optional; either finish (fake WebSocket server tests, Groq fallback on failure, live with a key) or remove it from the scorecard.
- Finishing = 8+ tests on stt_stream and one live run with a Deepgram key.

### 7. Installer / release pipeline - 47% ★★½☆☆

- Now: B70 T30 L35 S60 I50. Gap on record: NSIS broken locally; MSI built; start-menu shortcut points at a stale exe.
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Fix the NSIS bundle, produce MSI + exe, run install -> first launch -> uninstall in Windows Sandbox on a clean profile.
- Fix the Start-menu shortcut that points at an old exe; autostart task; keychain/permission prompts on first run; checksum + version in the artifact name.

### 8. Music control (Spotify / Apple Music) - 49% ★★½☆☆

- Now: B85 T35 L25 S60 I55. Gap on record: Windows media keys and deep links.
- Proof: **OWNER**; effort **S**; needs: -
- Tests for every phrase -> media key / deep link mapping (target 12 tests); live Spotify and Apple Music on Windows; honest reply when the player is not installed.

### 9. Admin brain (Qwen, admin only) - 50% ★★½☆☆

- Now: B80 T30 L35 S70 I50. Gap on record: Compiled out of family builds.
- Proof: **BOT**; effort **S**; needs: D2
- Family builds compile it out, so score it only for the admin build: fake-server tests for classify / phrasings / pronunciation (target 10), a health + timeout path, and a live admin run.

### 10. WhatsApp priority people (read-only) - 52% ★★½☆☆

- Now: B85 T65 L5 S90 I50. Gap on record: OFF by default; read-receipt side effects unknown.
- Proof: **OWNER+EXT**; effort **M**; needs: WhatsApp bridge running
- Run docs/testing/whatsapp-read-receipt-test.md first; if the bridge changes read state, keep it OFF and document why.
- Live with the real bridge: chat list, priority learning on a real week of counts, VIP / mute by voice; keep the read-only deny-list tests.

### 11. Window tiling + display off - 53% ★★½☆☆

- Now: B100 T55 L5 S85 I50. Gap on record: No real window moved, display_off never called.
- Proof: **BOT**; effort **S**; needs: -
- BOT proof: spawn 2, 3, 4 Notepad windows, tile, read back GetWindowRect and assert equal columns; minimised and maximised start states; a shared-terminal case; an elevated window is skipped.
- Display-off: confirm the guard (idle > 60 s, no meeting), wake on input, and `powercfg /requests` while off. Needs your OK because it turns your screen off for a few seconds.

### 12. Vision-grounded click - 53% ★★½☆☆

- Now: B85 T60 L25 S70 I40. Gap on record: No measured accuracy on real screens; calibration results never recorded.
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Run the existing calibration suite on 30 real targets across 3 apps; record hit-rate and distance error in the ledger.
- Set a confidence floor: below it NEXUS asks instead of clicking. Target: >= 70% hit on the 5 most-used apps, 0 wrong clicks in protected windows.

### 13. Calendar read / add - 54% ★★½☆☆

- Now: B90 T50 L15 S85 I60. Gap on record: live proof missing
- Proof: **OWNER**; effort **S**; needs: Google connected
- Live: read today / tomorrow, add an event, overlap warning, confirm gate. Add tests for time-zone and all-day events (target 14 tests).

### 14. Live mode: WhatsApp / browser flows - 55% ★★½☆☆

- Now: B85 T35 L45 S70 I45. Gap on record: WhatsApp UIA unverified on current build; opener bounded to 3 candidates.
- Proof: **OWNER**; effort **M**; needs: -
- Re-verify against the current WhatsApp build (UIA vs OCR header check); state-machine tests (Idle -> AppOpen -> ChatActive -> TextTyped) to 15 tests; honest failure lines for every branch.

### 15. Overnight record + morning briefing - 55% ★★½☆☆

- Now: B100 T65 L0 S85 I60. Gap on record: Not run across a real shutdown.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- BOT: integration test save -> clear process -> boot flow speaks it once -> mark told -> asks to reopen apps.
- OWNER: dry-run rehearsal, then one `on` run with a deliberately failing agent; read the next morning's briefing.

### 16. Session snapshot / restore - 55% ★★½☆☆

- Now: B90 T60 L15 S80 I55. Gap on record: Brave/Edge restore flag not verified.
- Proof: **BOT**; effort **M**; needs: -
- Test restore on this machine for Brave, Edge, VS Code, Explorer, Notepad; verify the browser restore flag; never reopen sensitive windows.

### 17. Agent watch sidecar (spawn + watch CLIs) - 55% ★★★☆☆

- Now: B80 T30 L45 S70 I60. Gap on record: Real PTY spawn verified once (Claude); others not installed here.
- Proof: **BOT**; effort **S**; needs: D5
- D5 recommendation: keep only if still needed now that hooks exist. Otherwise fold the CLI spawn into hooks and delete the Node sidecar. If kept: node tests for server.cjs (target 8).

### 18. Proactive-speech policy - 56% ★★★☆☆

- Now: B90 T70 L10 S85 I55. Gap on record: Thresholds are hypotheses; nothing emits a Critical alert yet.
- Proof: **OWNER**; effort **M**; needs: -
- Add real Critical sources (power countdown, security / PR-failed alert); hook the `proactive:nudge` UI; tune thresholds from a week of use; timeline tests for each new source.

### 19. Wake-up sound - 57% ★★★☆☆

- Now: B100 T65 L10 S80 I60. Gap on record: Nothing audible has been heard yet.
- Proof: **OWNER**; effort **S**; needs: -
- Listen to all five styles; check the echo guard with your real mic; read the `[CUE]` latency lines; Bluetooth check.

### 20. Boot hello / shutdown flow / resume hello - 57% ★★★☆☆

- Now: B100 T50 L25 S70 I60. Gap on record: Built in two rounds; nothing run live after rebuild.
- Proof: **OWNER**; effort **S**; needs: rebuilt app
- Reboot and sleep/resume tests; read the stage-ready gating log; confirm one hello, not two.

### 21. Gated commit + draft PR - 58% ★★★☆☆

- Now: B100 T80 L0 S90 I60. Gap on record: Never run against a real repo or GitHub.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- BOT: real git against a temp repo with a local bare remote (branch, commit, push); a real `.env` is refused; nothing-to-commit is spoken; detached HEAD / rebase refused.
- OWNER: approve creating one throwaway private GitHub repo so the draft-PR call runs against real GitHub.

### 22. Track all agents (announce each, summary) - 59% ★★★☆☆

- Now: B100 T70 L5 S90 I65. Gap on record: Built today; no multi-agent live run.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- BOT: scripted timeline through the real webhook and hook script with 3 stand-in agents (staggered finish, one failing, one silent); assert announcements merged and one summary.
- OWNER: run it once with 2-3 real agents (Claude, Codex, Antigravity are installed).

### 23. Profile, corrections, routines, review, curator - 59% ★★★☆☆

- Now: B95 T75 L10 S85 I60. Gap on record: Review never run against a real model; routines have no history yet.
- Proof: **OWNER**; effort **M**; needs: time
- Review: one real run with the PII filter assertions; routines need about a week of command history; curator needs a full profile. Time-gated.

### 24. Important-mail watcher + triage - 60% ★★★☆☆

- Now: B95 T75 L10 S85 I65. Gap on record: No Google account connected at test time.
- Proof: **OWNER**; effort **S**; needs: Google connected
- Send yourself a deadline mail, a GitHub security mail and a promotion; confirm one alert for the first two, none for the third, 'that's not important' mutes.

### 25. NLU fallback (BERT-Mini ONNX) - 60% ★★★☆☆

- Now: B90 T55 L40 S65 I60. Gap on record: Candidate model 0.9004 test intent; shipped model may lag; OTA update path built, never run end to end.
- Proof: **BOT**; effort **M**; needs: -
- Promote the 0.9004 candidate after re-measuring on the locked test set; calibrate confidence; run the OTA publish -> download -> hot-swap path against a staging Worker once.

### 26. Endpointing / VAD / turn detection - 60% ★★★☆☆

- Now: B90 T45 L55 S60 I50. Gap on record: Adaptive 240-800 ms; age-bias (older speakers cut off) unmeasured.
- Proof: **DATA**; effort **M**; needs: -
- Record slow / hesitant / quiet fixtures; measure cut-off rate vs the 700 ms baseline; tune and pin the numbers in tests.

### 27. Screen analysis / narrated tour - 61% ★★★☆☆

- Now: B95 T70 L30 S75 I50. Gap on record: Gemini box accuracy and 125/150% scaling not verified.
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Test at 100 / 125 / 150% scaling; three apps; compare pointer boxes to ground truth; measure first-callout latency.

### 28. Device Links (phone / laptop pairing) - 61% ★★★☆☆

- Now: B75 T70 L35 S85 I60. Gap on record: Local Wrangler + D1 verified; no live SMS or two-laptop run.
- Proof: **EXT**; effort **M**; needs: second device
- Two-laptop pairing run; optional SMS gateway check; keep the Worker test suite as is.

### 29. Boot briefing + resume points - 62% ★★★☆☆

- Now: B95 T70 L25 S80 I60. Gap on record: Real reboot never tested.
- Proof: **OWNER**; effort **S**; needs: rebuilt app
- Reboot test: delay, presence gate, spoken delivery once a day; recorder window-title quality across 5 apps.

### 30. Google accounts + OAuth - 62% ★★★☆☆

- Now: B95 T45 L40 S80 I65. Gap on record: Needs the user's own OAuth client; Testing-mode token expires in 7 days.
- Proof: **OWNER**; effort **S**; needs: -
- Fresh sign-in, forced token expiry, reconnect button, remove account; document the 7-day Testing-mode limit and the publish path.

### 31. Directed-speech gate (TV / side-talk filter) - 62% ★★★☆☆

- Now: B100 T65 L35 S75 I50. Gap on record: Replay ignored 72/96 of a real noise log (75%); rules written after reading the same log, so optimistic. No live TV test.
- Proof: **DATA+OWNER**; effort **M**; needs: -
- 30-minute live TV / Echo test; record false accepts and false rejects; replace the hand-made label set with real held-out turns.

### 32. Action Kernel (voice file control, quarantine delete, undo) - 63% ★★★☆☆

- Now: B100 T75 L10 S95 I70. Gap on record: 20/20 mutations killed; never run by voice.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- BOT: drive the voice grammar -> kernel path on temp folders (delete, undo, move, copy, refuse 'wipe my drive'); tests for journal and ipc.
- OWNER: one spoken delete + undo.

### 33. Keep-awake - 64% ★★★☆☆

- Now: B100 T65 L35 S70 I65. Gap on record: powercfg behaviour on this Modern-Standby laptop not checked.
- Proof: **BOT**; effort **S**; needs: -
- `powercfg /requests` while a lease is held and after a hard kill; add a system-only mode so the screen can turn off while the PC stays awake; heartbeat tests.

### 34. Mishearing normaliser + learned corrections - 64% ★★★☆☆

- Now: B100 T60 L45 S70 I55. Gap on record: Real recall of learned corrections unmeasured.
- Proof: **BOT**; effort **S**; needs: -
- Replay the real missed-intent log and measure how many are rescued; add tests for each learned alias class.

### 35. Maps / Places / Directions - 65% ★★★☆☆

- Now: B95 T45 L55 S70 I65. Gap on record: Both keys verified once.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- Live directions / places / ETA; without-key and quota paths; origin spoken vs remembered home.

### 36. Dictation (type what I say) - 65% ★★★☆☆

- Now: B95 T65 L40 S85 I55. Gap on record: 29 tests vs a planned 150-pair corpus.
- Proof: **BOT**; effort **M**; needs: -
- Build the planned 150-pair text corpus (spoken -> expected text) and gate on it.

### 37. Timetable from an image + reminders - 66% ★★★½☆

- Now: B90 T65 L45 S80 I60. Gap on record: Real Gemini read all 5 slots of a generated timetable.
- Proof: **OWNER**; effort **S**; needs: -
- Three real formats (table screenshot, photo, handwriting); section pick; reminders fire once.

### 38. Local STT fallback (Moonshine) - 66% ★★★½☆

- Now: B90 T40 L60 S75 I75. Gap on record: 6.65-7.84% WER; lazy sidecar; only used offline.
- Proof: **BOT**; effort **S**; needs: -
- Airplane-mode walkthrough; measure WER on the fixture set; restart-after-crash test.

### 39. TTS offline (Kokoro-82M) - 66% ★★★½☆

- Now: B90 T70 L50 S80 I50. Gap on record: Real-time on CPU only (RTF 0.7-2.4); deliberately NOT the live voice.
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Offline walkthrough, auto-return to Edge only after the proof sequence; listening check of all 10 voices; real word timing instead of estimated.

### 40. Main centre / 17 sub-centres validation - 68% ★★★½☆

- Now: B100 T65 L45 S80 I60. Gap on record: Typed ActionSpec, shared policy; live walkthrough pending.
- Proof: **OWNER**; effort **S**; needs: -
- One live phrase for each of the 17 centres; log lines `[MAIN-CMD]` / `[SUB-ROUTE]` match expectation.

### 41. Compound commands ("X then Y") - 68% ★★★½☆

- Now: B100 T75 L40 S85 I55. Gap on record: GitHub step inside a compound drops the remaining steps (known).
- Proof: **BOT+OWNER**; effort **M**; needs: -
- Fix: a GitHub step inside a compound must resume the remaining steps; 5 live compounds including one with a confirm gate.

### 42. Architecture mapper - 68% ★★★½☆

- Now: B100 T35 L70 S70 I65. Gap on record: 2,991 lines, 6 tests.
- Proof: **BOT**; effort **S**; needs: -
- Tests for the three phases, enrichment failure, empty repo, rate limit (target 20 tests).

### 43. Orb UI and turn lifecycle - 68% ★★★½☆

- Now: B100 T55 L60 S65 I65. Gap on record: 'Stuck orb' class of bug recurred 6+ times; each fix partly unverified live.
- Proof: **OWNER**; effort **S**; needs: -
- 50-turn soak with the orb log; any stuck state is a failed run.

### 44. Hand mode (camera pointer) - 69% ★★★½☆

- Now: B95 T70 L55 S85 I45. Gap on record: Pinch 'almost perfect' per owner; gestures, multi-monitor, macOS not run; camera 15 fps on battery.
- Proof: **OWNER**; effort **M**; needs: -
- Field test 6: pinch, drag, scroll, gestures, second monitor, battery fps; tune thresholds from the `[HAND]` lines.

### 45. CI gates: dead code, IPC, turn-end, hygiene - 69% ★★★½☆

- Now: B95 T70 L40 S85 I75. Gap on record: Gates fail on unrelated concurrent items; the GitHub workflow itself never run.
- Proof: **BOT+OWNER**; effort **S**; needs: approval to push
- Run the GitHub workflow on a branch; fix baselines; make the lib test suite deterministic (quarantine timing tests).

### 46. Shortcut map + action router - 69% ★★★½☆

- Now: B90 T65 L45 S95 I70. Gap on record: Wrong-window = nothing sent; tested on Notepad only.
- Proof: **BOT**; effort **S**; needs: -
- Cover VS Code, Explorer, terminals, two browsers; voice phrases for the 23 actions; tests to 30.

### 47. Captions (word-by-word, black capsule) - 70% ★★★½☆

- Now: B100 T60 L55 S70 I70. Gap on record: Estimated (not real) word timing on the offline voice.
- Proof: **OWNER**; effort **S**; needs: -
- Check timing against audio on both voices; fix the estimated-timing margin.

### 48. MCP connect system (Swiggy, Amazon, WhatsApp) - 70% ★★★½☆

- Now: B100 T75 L40 S85 I70. Gap on record: Swiggy production needs approval; bridges not running here.
- Proof: **EXT**; effort **M**; needs: bridge / approval
- Live WhatsApp pairing with QR rotation; Swiggy sandbox if approved; keep the audit-line tests.

### 49. Global hotkey Ctrl+Space (call / cancel) - 72% ★★★½☆

- Now: B100 T10 L85 S80 I80. Gap on record: Used daily and works; no automated test.
- Proof: **BOT**; effort **S**; needs: -
- Extract the toggle decision into a pure function and test it (idle / capturing / speaking / ghost).

### 50. Wake word "NEXUS" (openWakeWord, own model) - 72% ★★★½☆

- Now: B100 T90 L55 S75 I40. Gap on record: Works live, but the honest held-out measure is recall 63.2% (quiet 3/10, far 15/25) and ~12 false fires/h on real background; the 98.6% figure was self-scored on training data. Target is >=95% at <=0.1/h.
- Proof: **DATA+OWNER**; effort **L**; needs: -
- THE biggest item. Record >= 10 h ambient (TV, talk, music, silence) and the labelled review set; retrain with the cascade (speaker check + verify only in the uncertain band).
- Gate: held-out recall >= 90% and <= 1 false fire per hour first, then the 95% / 0.1 per hour target. Use `nexus wake replay --release` for every claim.

### 51. Ghost mode (voice drives the PC) - 72% ★★★½☆

- Now: B100 T65 L60 S85 I55. Gap on record: Keyboard-first is where the industry is reliable (>=89%); cursor/vision clicks sit in a 20-55% band.
- Proof: **OWNER**; effort **S**; needs: -
- Run the scripted session (open app, new tab, search, type, WhatsApp draft, Esc); every refusal path once.

### 52. Command Hub / settings UI - 72% ★★★½☆

- Now: B100 T60 L65 S70 I70. Gap on record: Visual match to design not verified at 400 px.
- Proof: **BOT**; effort **S**; needs: -
- Component tests per page; 400 px reflow check in the built-in browser; light / dark.

### 53. Voice personas + offline twin swap - 74% ★★★½☆

- Now: B95 T75 L55 S85 I70. Gap on record: Pinned hashes, latest-wins queue. Audition by ear not done.
- Proof: **OWNER**; effort **S**; needs: -
- Audition all 10 voices and their offline twins; re-verify the pinned hashes.

### 54. Agent hooks (Claude, Codex, Antigravity, Cursor, Gemini, OpenCode, Aider, Devin, Windsurf, Kilo, Copilot) - 74% ★★★½☆

- Now: B100 T90 L40 S90 I70. Gap on record: Live: Antigravity start/stop, Claude prompt hook. Docs only: Codex, Devin, Windsurf, Kilo, Copilot, Cursor, Gemini, OpenCode, Aider.
- Proof: **BOT+EXT**; effort **M**; needs: -
- BOT: live-run every CLI that is installed here (Claude Stop, Codex after /hooks trust, Antigravity global hook, Cursor, Kilo, Aider, Copilot) and read the `[WATCH-POWER] agent event` line.
- Not installed here: Devin, Windsurf, OpenCode, Gemini CLI -> install them or mark them 'documented, unverified' in the scorecard.

### 55. TTS volume lease / restore - 74% ★★★½☆

- Now: B100 T70 L55 S85 I75. Gap on record: live proof missing
- Proof: **BOT**; effort **S**; needs: -
- Long-reply and hard-kill checks; baseline restore on next start.

### 56. Ask-first shutdown / sleep / restart - 75% ★★★½☆

- Now: B100 T75 L50 S95 I70. Gap on record: Graceful close; a save prompt stops it.
- Proof: **OWNER**; effort **S**; needs: -
- Dry run, then one `on` run (cancel by voice, by key, by meeting).

### 57. Security: keychain, PII filter, sealed memory - 75% ★★★★☆

- Now: B100 T70 L60 S85 I70. Gap on record: Keys still also in settings.json in places.
- Proof: **BOT**; effort **S**; needs: -
- Remove the duplicate plaintext keys from settings.json; secrets scan in CI; threat-model tests for the webhook token and the egress log.

### 58. Resource footprint (RAM / CPU / battery) - 75% ★★★★☆

- Now: B100 T40 L80 S75 I80. Gap on record: Idle 104-269 MB; Kokoro +368 MB when loaded.
- Proof: **BOT**; effort **S**; needs: -
- Budget test on the release build: idle RAM, CPU, wakeups per second; fail CI above budget.

### 59. 9Router (Cerebras > Groq > Gemini > Worker) - 76% ★★★★☆

- Now: B100 T55 L70 S75 I85. Gap on record: ~242 ms vs 600-2100 ms through the Worker.
- Proof: **BOT**; effort **S**; needs: -
- Cascade-failure tests (each provider down, all down); latency report.

### 60. Open / close / focus apps - 77% ★★★★☆

- Now: B100 T55 L80 S75 I70. Gap on record: Used constantly; duplicate-window bug fixed.
- Proof: **BOT**; effort **S**; needs: -
- More tests: ambiguous names, duplicate windows, protected windows, UWP vs Win32.

### 61. Memory core (encrypted store, ranked recall, context pack) - 78% ★★★★☆

- Now: B100 T90 L55 S90 I70. Gap on record: Paraphrase recall 19/19 on dev set (tuned), 17/20 held-out.
- Proof: **BOT+OWNER**; effort **S**; needs: -
- Grow the held-out paraphrase set to 60; first-launch migration on your real data.

## 8. Owner walkthroughs (one sitting lifts many rows)

Each walkthrough starts with `nexus build` then `nexus start`, and you paste back the log lines named. Order matters: do W1 first, it covers the weakest rows.

**W1 - Agents and power (about 30 min).** Rows: hooks, track all agents, watch then power off, ask-first shutdown, overnight record, keep-awake, tiling, display-off.

1. Say 'hook up all agents'; approve; read back which agents were hooked.
2. Say 'practice unattended shutdown'. Start tasks in two agents (for example Claude and Codex). Say 'track all the agents and tell me when they are done'.
3. Let both finish; expect one spoken line per agent, one summary, windows in two equal columns, display off only if you stayed away.
4. Say 'watch Antigravity, commit and raise a PR, then shut down my laptop'; answer yes to the git question; use a throwaway repo.
5. Switch to `on` ('allow unattended shutdown'), run a task that fails on purpose, let the laptop shut down; next morning read the briefing and the reopen question.
6. Paste the `[WATCH-POWER]` lines from the run.

**W2 - Voice front end (about 30 min).** Rows: wake sound, boot hello, endpointing, directed gate, captions, orb, personas, offline voice, hotkey.

1. Reboot; confirm one hello. Sleep and resume; confirm the resume hello.
2. Say 20 short commands with the wake word at normal, quiet, and far distance; note misses.
3. Play a TV show for 10 minutes in Ghost mode; count unwanted reactions.
4. Switch each of the 5 wake sounds; switch the voice to 3 personas; go offline for 2 minutes and back.
5. Do 50 turns watching for a stuck orb.

**W3 - Desktop control (about 30 min).** Rows: ghost, hand, shortcuts, dictation, action kernel, screen tour, vision click, WhatsApp flows.

1. 'ghost mode': open Brave, new tab, search, type a line, draft a WhatsApp message (do not send), Esc.
2. 'hand mode': palm-fist-palm-fist, click, drag, scroll, 3-finger tab flick; exit.
3. 'delete the pdfs in downloads' (use a folder with test files), confirm, then 'undo that'.
4. 'analyse my screen' on a web page, a video, and an app at 125% scaling.
5. 'click the <label>' in three apps for the vision calibration run.

**W4 - Memory and proactive (about 30 min, Google connected).** Rows: mail watcher, calendar, leave-by, briefing, restore, timetable, routines, maps.

1. Send yourself a deadline mail and a GitHub security mail; expect one alert each.
2. 'what is on my calendar today'; 'add lunch tomorrow at 1pm'; create an event 2 hours ahead with an address and wait for the leave-by line.
3. 'directions to <place>', 'nearest pharmacy'.
4. 'analyse this and add section 2 to my timetable' with a timetable screenshot.
5. Reboot; read the briefing and the reopen offer.

## 9. Order of attack

1. **Foundations (before row 1):** this plan, the scorecard script and data (done), the proof ledger file, a fresh `nexus build`, and a green baseline (the foreign failing parser test is fixed or quarantined so every run is clean).
2. **Rows 1-12 (the weakest):** decisions D1-D5 first, then the cheap builds (Activity Manager, Leave-by, Music, Installer), then tiling and display-off which I can fully prove myself.
3. **Rows 13-30:** the new agent features and the memory / proactive rows; run W1 and W4 here.
4. **Rows 31-45:** the quality rows (directed gate, endpointing, NLU, dictation corpus); W2 and W3 here.
5. **Rows 46-61:** the nearly-there rows, which are mostly tests plus one live run each.
6. **The wake word** runs in parallel from day one because it needs recordings, which take calendar time.
7. **Finish:** re-score everything, regenerate the scorecard, tag the build.

## 10. Risks stated up front

- Live proof is the bottleneck: I can prove about half the rows alone; the rest need you in four sittings of about 30 minutes.
- The wake word cannot reach 80% by engineering alone; it needs the ambient recordings and honest held-out numbers.
- Rows that depend on third-party behaviour (agent hooks for tools not installed, WhatsApp bridge, Swiggy) can fail for reasons outside the repo.
- The percentages are a rubric; the floors in the definition of done are there so the number cannot be gamed.

## 11. Progress log

The queue above is the plan; this table is the truth. A row is DONE only when it meets section 1.

| Date | Row | Before | After | What changed | What is still needed to reach 80% |
|---|---|---|---|---|---|
| 2026-10-20 | Foundations | - | - | Proof ledger, `scripts/scorecard.py` (+ `docs/testing/scorecard-data.json`), baseline lib run green (2022 passed) | - |
| 2026-10-20 | D1, D2 applied | - | - | macOS/Linux, Administrator control and the admin brain are listed as *out of the current target* in the scorecard | Reopen them by flipping `scope` in the data file |
| 2026-10-20 | 3 Activity Manager | 35% | 68% | Privacy list now matches variants (`keepassxc` was being recorded), rollback on failed save, 13 tests, 8 mutations killed, real foreground read verified | A real 1-hour opted-in run by the owner (W1) |
| 2026-10-20 | 5 Leave-by reminders | 44% | 56% | Logic split into pure tested functions (12 tests, 5 mutations), failed route lookups no longer retried every minute | A real calendar event with an address (W4) |
| 2026-10-20 | 8 Music control | 49% | 67% | Fixed `R&B` / multi-word titles, removed the false "playing" claim and the stray pause key, honest "not installed", bare `play <title>` understood, 10 tests | A spoken Spotify run (W3) |

Observation after the first three rows: **tests and fixes move a row from about 50% to about 68%; the last 12 points are the live proof (L axis, 30% weight).** The four owner walkthroughs in section 8 are therefore the critical path for the whole programme, not a final polish step. Schedule W1 as soon as the agent rows are built.

### 2026-10-20 (second batch, coded by Antigravity/Gemini, reviewed and verified by the engineer)

| Row | Before | After | Real proof run | Real bugs found and fixed |
|---|---|---|---|---|
| Global hotkey | 72% | 85% (Production) | decision checked for all 16 states | stale design comment rewritten |
| Window tiling + display off | 53% | 76% | 3 real windows tiled into exact 640 px columns; `display_off` returned Ok | none; the proof harness itself needed a real-windows approach (console windows do not appear under `cargo test`) |
| Gated commit + PR | 58% | 75% | real git against a local bare remote (5 tests) | none |
| Keep-awake | 64% | 78% | execution state `0x3` held, `0x0` released | a stale lease drop re-cleared the state |
| Action Kernel | 63% | 77% | spoken delete, confirm, undo on real folders | none |
| Architecture mapper | 68% | 80% | 23 new pure tests | `.git` suffix kept in repo names; `pages/api` was treated as UI; unstable tie order |
| Overnight record | 55% | 65% | store round trip, expiry, corruption | none |
| Calendar read / add | 54% | 66% | local mock server | the real flow now uses the tested helpers (specific spoken errors); evening/afternoon times; all-day events no longer conflicts |
| Dictation | 65% | 72% | 156-pair corpus | known quirks recorded ("one dollar" -> "1 dollars") |
| Live mode / WhatsApp | 55% | 65% | 25+ pure tests | name cleaning no longer drops the spoken name |
| Session restore | 55% | 64% | injectable launcher, 22 tests | UNC paths could be launched on restore (NTLM risk); "reopen them" was not understood as a yes |
| Mishearing normaliser | 64% | 72% | owner's real log replayed: 31 of 194 unique misses now understood | added openbread / open brief / command hop |
| Security (keys) | 75% | 77% | audit found 5 plaintext keys in the owner's settings.json | keys now move to the keychain and are blanked at startup; import replaces; readers keychain-first |

Method note: the coding was delegated to the Antigravity CLI (`agy`, Gemini 3.8 Flash) from written specs; every report was checked against the diff. Two things to watch for in delegated work, both seen here: (1) a helper written only for its own tests that production never calls (calendar), fixed by wiring production to it; (2) helpers that exist only for tests must be `#[cfg(test)]` or the dead-code gate fails.
