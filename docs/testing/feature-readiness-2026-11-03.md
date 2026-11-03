# Feature readiness — 2026-11-03

Evidence-based readiness of every NEXUS (Windows) feature. Read-only analysis: no build, no NEXUS run, no tests were executed to write this. Sources: `scorecard-data.json` (72 rows), `live-proof-ledger.md`, `windows1-production-queue.md`, `live-checklist-2026-10-27.md`, `voice-lock-phase-closeout.md`, `work-cards/*`, `feature-process-map.md`, `AGENTS.md` "Not run live / Pending / Needs the user" notes, `docs/changes/163`–`185`, `git log` 2026-10-28..11-02 and the uncommitted diff (`early_hello.rs`, `early_orb.rs`, `memcore/briefing.rs`, `memcore/store.rs`).

## Levels

| Level | Meaning |
|---|---|
| **1 Production-ready** | Unit/integration tests AND a recorded live proof (a line in `live-proof-ledger.md` on the current code) AND no known open defect |
| **2 Working, needs live proof** | Built and tested; no ledger line on current code (or only a partial/bot probe); no known defect beyond "not run live" |
| **3 Partial / in progress** | Built, but with a known defect, a measured shortfall, or a named missing piece |
| **4 Not started / planned** | Only a plan or a deferral exists |

Rules applied: scorecard `L` values written before the ledger rule (2026-10-20) do not count as live proof. A ledger run on older code does not count after the feature's code changed (ledger rule 3). "Works for the owner" in a note, without a ledger line, is not a proof.

## Summary

| Level | Count | Share |
|---|---:|---:|
| 1 Production-ready | **0** | 0 % |
| 2 Working, needs live proof | 45 | 52 % |
| 3 Partial / in progress | 33 | 38 % |
| 4 Not started / planned | 9 | 10 % |
| **Total** | **87** | |

**0 of 87 features are production-ready.** This agrees with `windows1-production-queue.md` ("none is declared production ready"). The ledger has only 9 BOT feature probes (keep-awake, tiling, display-off, mishearing replay, gated commit, security audit, activity manager, music probe, keyring) and the owner Voice Lock trials; none of them is both complete for its feature and on current code with no open defect.

Features 73–79 below were added from the last days' commits; 80–87 are planned items named in AGENTS.md/plans. Four out-of-scope scorecard rows (D1/D2/D4 decisions: streaming captions, admin brain, administrator, macOS/Linux) are still counted.

**About the "owner confirmed on 2026-11-03" status:** no evidence found for any of the four recent features (early hello, reopen question, analyse-from-screen, agent-watch keep-awake). There is no 2026-11-02/03 ledger line. The only owner/live observations for these features are FAILURES: the 2026-11-02 logons ("Good evening" at night, no orb, no speech volume, 15–20 s delay; change 185 updates b/d) and a comment in the uncommitted `early_hello.rs` diff saying that on 2026-11-03 the hello spoke at 10:48:46 while the lock screen was up (PIN entered at 10:48:55). So none of them is recorded as owner-confirmed.

---

## Voice / wake

| # | Feature | Level | Evidence | Blocker to level 1 |
|---|---|---|---|---|
| 1 | Wake word "NEXUS" (own OWW model) | 3 | 64 tests + replay harness; held-out recall **63.2 %**, ~12 false fires/h (AGENTS 2026-10-11); owner fast path woke on ordinary speech 2026-10-31 (now off, change 180); a film woke the orb at 0.92–0.98 on 2026-11-02 (guard added, change 185e). No ledger line | Record ambient + held-out set, `nexus wake score` → ≥95 % recall at ≤0.1 FA/h, retrain if needed, ledger line |
| 2 | Global hotkey Ctrl+Space | 2 | 5 tests, 16 states covered; no ledger line | One recorded call/cancel run in each state class |
| 6 | Endpointing / VAD / turn detection | 3 | 5 tests; age bias unmeasured; `wakeSeedOffsetMs`/400 ms limit never swept | `stt_eval --simulate-endpoint` sweep on real recordings + live run |
| 7 | Directed-speech gate (TV / side talk) | 2 | 27 tests + 145-line corpus (98.7 % noise rejected; corpus engineer-written) | 30-minute live TV test, ledger line |
| 14 | Wake-up sound | 2 | 11 tests, 5 mutations; "nothing audible heard yet" | Owner listens after rebuild; echo check with a real mic |
| 15 | Boot hello / shutdown flow / resume hello | 3 | Mixed tests; normal hello waits for the stage (stage took 21.4 s at the 2026-11-02 logon, change 185d) | Overlap stage/tray creation, then a recorded logon, shutdown and resume run |
| 73 | Early hello at logon with native orb + caption | 3 | Commits a7d538b..295561f; early_hello 8+ tests, rendered preview images; Rust lib 2961 pass (185e). Owner logons 2026-11-02 failed (no clip on disk, no orb, "Good evening"); lock-screen speech 2026-11-03 (comment in uncommitted diff). Lock-screen gate (`session_locked`, `logon_ui_running_in_my_session`) is uncommitted, untested by a logon | Commit the lock gate, rebuild, 3+ logons with boot timeline, owner sees orb+caption and hears the hello after the PIN; ledger line |

## Speaker lock

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 8 | Speaker verification / Voice Lock | 3 | 41 % (2.05/5). Frozen f0988d9: Rust 2537+19, frontend 687, Worker 145, Python 60, 40/40 mutations; release d846dd346c470b60 provenance-verified. Ledger: enrollment isolation, wake recovery, cancellation PASS (attribution unconfirmed); owner Notepad actions FAIL 2026-10-26; P2 settings PASS on older candidate. P3–P6 gates open (`voice-lock-phase-closeout.md`) | Repaired owner action on the fresh release, ten-start reconciliation, ≥95 % owner acceptance, 0 locked non-owner actions over ≥300 negatives, replay/TV, profile recovery |

## STT / NLU / understanding

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 3 | Cloud STT (Groq) + hallucination filter | 3 | 60 tests; owner vocabulary duplicated commands live ("open open open notebook", ledger 2026-10-26); neutral decoding fix f0988d9 not proven live | Live run on the repaired release showing single, correct transcripts |
| 4 | Local STT fallback (Moonshine) | 2 | Fallback decisions tested; no ledger line | Airplane-mode walkthrough |
| 5 | Streaming partial captions (Deepgram, out of scope D4) | 3 | 3 tests; stream module has no tests | Tests for the stream module + key + live run (or keep out of scope) |
| 16 | Deterministic intent parser | 3 | 236 tests; P0 measure 43 % bare-command correct / 12 % wrong action, not re-measured | Re-run the phrasing probe / held-out set, publish accuracy |
| 17 | NLU fallback (BERT-Mini) | 3 | 13 tests; candidate 0.9004 not promoted; OTA path never run end to end | Promote/retrain, one OTA publish→download→hot-swap run |
| 18 | Admin brain (Qwen, out of scope D2) | 3 | 0 client tests | Client tests + admin build live run |
| 19 | Compound commands | 3 | 31 tests; GitHub step in a compound drops remaining steps (known) | Fix GitHub resume, live run |
| 20 | Main centre / 17 sub-centres | 2 | 16 tests; change 172 audit (Rust 2813, vitest 751) | Live walkthrough |
| 21 | 9Router | 2 | 18 tests; ~242 ms measured (no ledger line) | Recorded live latency run |
| 22 | Mishearing normaliser + learned corrections | 3 | 45 tests; ledger BOT replay 2026-10-20: 31 of 194 rescued, 163 still missed | Reduce the missed set, re-run replay |

## TTS / orb / captions

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 9 | TTS online (Edge, persistent) | 2 | 44 tests; 409 ms median measured; no ledger line | Recorded latency run on current release |
| 10 | TTS offline (Kokoro-82M) | 2 | 27 tests; RTF 0.7–2.4 measured | Offline↔online switch run |
| 11 | Voice personas + twin swap | 2 | 31 tests; audition by ear not done | Owner audition + swap run |
| 12 | Captions (word-by-word) | 3 | vitest; offline voice uses estimated timing | Real Kokoro timing or accepted limitation + live run |
| 13 | Orb UI / turn lifecycle | 2 | vitest; change 172 (751 vitest) | 50-turn soak |
| 61 | TTS volume lease / restore | 2 | 14 tests | Recorded OS-volume probe across a Ghost session |

## Ghost / desktop control

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 23 | Ghost mode | 3 | 16+ tests; vision clicks in a 20–55 % band; G11 secondary items open | Scripted live run (open/new tab/search/stop) + ledger |
| 25 | Shortcut map + action router | 2 | 55 tests; Notepad only live (no ledger line) | Browser + VS Code live run |
| 26 | Dictation | 3 | 20 tests + 156-pair corpus; known quirks ("1 dollars", proper nouns lowercase) | Fix quirks, real dictation into apps (W3) |
| 27 | Action Kernel (file control, undo) | 2 | 53 tests, 20/20 mutations | One spoken delete + undo (W3) |
| 29 | Vision-grounded click | 3 | 43 tests; Groq vision decommissioned (Gemini only); measurement harness never run | Run the UIA-vs-vision harness on a clean desktop |
| 30 | Live mode WhatsApp/browser | 2 | 45 tests | Current WhatsApp build run (W3) |
| 31 | Open/close/focus apps | 2 | 15+ tests; used daily but no ledger line | One recorded run incl. repeat (no duplicate window) |
| 32 | Window tiling + display off | 2 | 21 pure tests; **ledger BOT 2026-10-20: 3 windows tiled to exact 640 px columns; display_off Ok but darkness not observed** | Owner sees the screen go dark; finish sequence on real agent windows; elevated-window skip |
| 33 | Music control | 2 | 10 tests, 5 mutations; ledger BOT probe (registry only) | Spoken Spotify run (W3) |

## Hand mode

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 24 | Hand mode (camera pointer) | 3 | 80+ tests (hand:: 91); owner: pinch "almost perfect"; gestures, Four threshold, multi-monitor not run; 15 fps on battery | Real-hand gesture run, camera-freeze test (checklist §6), ledger |
| 70 | Focus Mode (Hand Mode load control) | 2 | ~60 tests, 4 mutations; GPU share not controlled | Task Manager before/during/after + `hand-focus-report.mjs` (checklist §5) |

## Memory

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 42 | Memory core (sealed store, recall) | 3 | 62 tests; **`memcore.enc` could not be decrypted on every start since 2026-11-02 14:41 IST, cause unknown** (185d); set-aside recovery is uncommitted | Find the key mismatch cause, commit recovery, restart test with read-back |
| 43 | Profile, corrections, routines, review, curator | 2 | 59 tests; review never run against a real model | One review run + one routine offer live |
| 46 | Timetable from image + reminders | 2 | 16 tests; one real Gemini read (5/5 slots), no ledger line | Reminder → "yes" round trip |
| 47 | Boot briefing + resume points | 3 | 31 tests; depends on the broken memory store (185d) | Fix #42, real reboot |
| 48 | Session snapshot / restore | 3 | 36 tests; no snapshot could be saved while memory was unreadable | Fix #42, real reboot + restore |
| 74 | Reopen-last-session question + restore | 3 | Commit 295561f: five rotating questions, many yes/no wordings, `classify_reopen` isolation, retry-once restore; Rust lib 2961, vitest 788. Not run live; blocked by #42 | Fix #42, rebuild, logon → question → "yes" → apps reopen; ledger |
| 60 | Activity Manager | 2 | 13 tests, 8 mutations; ledger BOT foreground read 2026-10-20 | 1-hour opted-in run, Hub toggle |

## Notifications / watchers / proactive

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 55 | Google accounts + OAuth (user's own client) | 2 | 45 tests; OAuth state/CSRF fix; no real sign-in with the owner's client; Testing-mode tokens expire in 7 days | Real sign-in, re-auth after expiry, ledger |
| 44 | Important-mail watcher | 2 | 22 tests; no Google account at test time | Real account, real important mail |
| 45 | Calendar read / add | 2 | 22 tests | Real Google account run (W4) |
| 49 | Proactive-speech policy | 3 | 15 tests; nothing emits Critical; thresholds hypotheses | A real Critical source + live meeting test |
| 50 | WhatsApp priority people | 3 | 17+8 tests; OFF; read-receipt side effect unknown | `whatsapp-read-receipt-test.md` on a real bridge |
| 51 | Leave-by reminders | 2 | 12 tests, 5 mutations | Real calendar event with an address |
| 72 | Notification brain + GitHub Actions/Vercel/Render/Netlify/Supabase watchers | 2 | ~90 tests, 12 mutations; Supabase status strings unverified | Break/fix a workflow and a deploy (checklist §3–4) |

## GitHub / PR

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 53 | GitHub commands (PRs, merge, review) | 2 | 61 tests; no ledger line | Recorded signed-in run |
| 71 | GitHub by voice without owner | 2 | ~70 tests; set_view race fixed | Checklist §2 with real accounts, SSO org |
| 75 | Repo / PR analyse-from-screen | 2 | Changes 182, 183, 184: verify_repo_identity (github_repo_index 16), pr_screen 9, Rust lib 2931; a mutation killed. The 2026-11-01 21:31 live log was a FAILURE that led to change 183; nothing run live since | Gmail + GitHub-page scenarios after rebuild, ledger |
| 38 | Gated commit + draft PR | 2 | 30 tests; ledger BOT 2026-10-20: real git + local bare remote pass | Real GitHub draft PR on a throwaway repo |

## MCP / commerce

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 52 | MCP connect system (Swiggy, Amazon, WhatsApp) | 3 | 28 tests; Swiggy production needs Builders-Club approval; bridges not running | Running bridges + one confirmed tool call each |

## Architecture mapper

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 54 | Architecture mapper | 2 | 42 tests; own window beside the sidebar (36c8772, 49ebe77); `DEV_KEEP_WINDOWS_ALIVE` still true (change 180) | Flip the dev flag, live run at 125/150 % scaling |

## Screen tour / reverse engineer / maps

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 28 | Screen analysis / narrated tour | 2 | 60 tour tests | Real Gemini latency, scaling alignment, speech/callout timing |
| 68 | Reverse engineer website/app | 2 | 28+15+21+12 tests | Owner walkthrough on two real sites |
| 56 | Maps / Places / Directions | 2 | 20 tests | Recorded run |
| 77 | Keyless Maps panel + place card | 2 | Commits 9b7fad1..08c8573; screenshots in `docs/research/*.png` (untracked); no ledger line | Recorded where-is / route / place-card run |

## Agent control

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 34 | Agent hooks (11 agents) | 3 | 48 tests; live only Antigravity start/stop + Claude prompt hook; 9 agents docs-only; Codex final message ignored (182) | Live Stop payload for Claude, Codex, Cursor, Gemini, OpenCode |
| 35 | Watch one agent, then power off | 2 | ~100 tests; two practice runs (no ledger line); real shutdown never | Practice + `on` run with ledger lines |
| 36 | Track all agents | 2 | 4+37 tests | Multi-agent live run |
| 37 | Overnight record + morning briefing | 2 | 10 tests | Real shutdown + next-morning briefing |
| 39 | Agent watch sidecar | 3 | 2 tests; one Claude PTY spawn; Node not bundled | More tests, run 2+ CLIs |
| 40 | Keep-awake | 2 | 16 tests; ledger BOT 2026-10-20 (0x3/0x0) is on OLDER code: pinned holds added in change 182 | Re-run the live lease probe on current code + battery run |
| 76 | Agent-watch pinned keep-awake + needs-input | 3 | Change 182 (commit 9986339): 121 tests pass; needs-input only for agents with a signal (none for Codex/Antigravity/Cursor/Windsurf/Trae); events in memory only; sleep unverified (Modern Standby) | Real agent prompt → spoken needs-input; screen stays on through a slow check |
| 41 | Ask-first shutdown/sleep/restart | 2 | 23 tests | Recorded spoken shutdown with a save prompt |

## Settings / Command Hub / UI

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 64 | Command Hub / settings UI | 2 | frontend 788 vitest (185e); no render tests (no React testing library) | Recorded Hub walkthrough |
| 69 | Window theming (solid panels) | 2 | Opacity guard + self-test | Seen in the real app |
| 78 | UI theme tokens, contrast gate, accessibility (records 173–179) | 2 | Contrast gates (Architect, sidebar, HUD), forced-colors; no ledger line | Light/Dark/System + high-contrast run |
| 65 | Resource footprint | 2 | Measured idle 104–269 MB (pre-ledger) | Recorded measurement on current release vs 600–700 MB ceiling |

## Build / provenance / CI / security

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 62 | Security: keychain, PII, sealed memory | 3 | 30 tests; ledger audit found 5 plaintext keys; first real scrub not run; keys still in settings.json (change 78 deviation) | Real startup scrub + restart read-back |
| 63 | CI gates (dead code, IPC, turn-end, hygiene) | 3 | Scripts pass locally (dead-code 141); GitHub workflow never run | One green CI run on the repo |
| 67 | Installer / release pipeline | 3 | MSI built; NSIS broken locally; start-menu shortcut points at a stale exe; release provenance (`nexus-build.json`) built | Fix NSIS + shortcut, install on a clean account |
| 79 | Startup: no console at logon, boot marks, file log | 3 | Commits db476cd, dbc824e; stage 21.4 s / tray 7–19 s on the setup thread (185d) | Overlap stage/tray, measured logon timeline |
| 66 | macOS / Linux parity (out of scope D1) | 4 | Windows-only | Own programme |

## Cloudflare Worker / device links

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 57 | Cloudflare Worker | 2 | 145 tests | Recorded deployed `/health` + one real turn |
| 58 | Device Links | 3 | Local Wrangler + D1 verified; no SMS / two-laptop run; WebRTC transport not built | Two-laptop pairing run |

## Administrator

| # | Feature | Level | Evidence | Blocker |
|---|---|---|---|---|
| 59 | Administrator control (out of scope D2) | 3 | 7 tests; policy/provenance only; every executor closed | Enrollment, verifier, session opener |

## Planned / not started (level 4)

| # | Item | Source |
|---|---|---|
| 80 | Notification Worker webhook inbox | AGENTS 2026-10-25 "Not built" |
| 81 | Hub grouping of notifications | same |
| 82 | Device Links WebRTC file transport, activity sharing | change 144 |
| 83 | Early hello P1–P4: unlock/lid triggers, early question + mic buffer | plan 158, change 185 |
| 84 | LLM answer streaming into speech, connectivity state machine, Smart standby | AGENTS 2026-10-17 "Not built" |
| 85 | Hand mode head-pointer fallback, two-hand zoom, absolute mapping | AGENTS hand mode "NOT built" |
| 86 | Dictation hold-to-talk, voice "add to dictionary" | AGENTS ghost/dictation "Not built" |
| 87 | Supabase mediator / home server for place data | plan ea1faca |

(#66 macOS/Linux is the ninth level-4 item.)

---

## Top 10: what moves the most features to level 1

1. **One rebuild + one recorded 30-minute smoke session on the current commit** (owner builds; record each result in the ledger). Covers 10+ level-2 rows with no known defect: hotkey, TTS online, orb, wake sound, open/close apps, volume lease, Command Hub, theming, UI tokens, resource footprint, 9Router, main centre.
2. **Fix the memory-store decryption failure** (#42) and commit the set-aside recovery, then a restart read-back. This unblocks #42, #47, #48, #74 and the early hello's `last_hello_ts`.
3. **Commit the lock-screen gate and run 3 logons with the boot timeline** (#73, #15, #79). Also overlap stage and tray creation on the setup thread.
4. **Run the signed-in GitHub checklist §2 plus the Gmail/GitHub-page analyse cases** (#53, #71, #75, #54, #38 draft PR on a throwaway repo).
5. **Do one real agent session**: Claude Code with a needs-input prompt, then practice mode and `on` mode (#40 re-proof on current code, #76, #35, #36, #37, #34).
6. **Sign in with the owner's Google OAuth client** (#55, #45, #44, #51, then the briefing).
7. **Break and fix a workflow and a deploy on a throwaway repo** (#72, plus the notification wording).
8. **Do the W3 batch**: a spoken Spotify run, dictation into apps, a voice delete and undo, the WhatsApp flow (#33, #26, #27, #30, #25).
9. **Measure the wake word**: ambient + held-out recordings, `nexus wake score`, a 30-minute TV test (#1, #7, #6). This is the main voice-quality gap.
10. **Fix the release path**: NSIS, the stale start-menu shortcut, and one green GitHub CI run (#67, #63), so the proofs above are attributable to a shippable build. The Voice Lock work card (#8) continues in its own sessions and is the longest path.

## Live results recorded after this report (owner, 2026-11-03)

- **Early hello at logon (orb + caption + volume lease):** owner confirmed "great and perfect" on build 611e9b93; the 21:58 IST logon log shows the lock-screen gate, the orb painted before sound, the 75 % lease and the reopen question. Still level 3 until the sleep/wake and sign-out/in paths have a ledger line on the current build (e47e933b).
- **Reopen question:** spoken and armed at the 21:58 logon; the reopen itself (answer, apps launched) is not yet proven live.
- **Sleep/wake (build e47e933b):** resume-from-sleep barge-in and the overlay grace ran; the resume greeting needs >= 20 minutes away and has not been proven live.
- **Google "sign in again" alert:** the 2026-11-03 20:58 alert was a false alarm (no network right after a dark wake); fixed in this change, not yet seen live.
