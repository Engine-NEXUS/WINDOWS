# Eight weak features: verification first, then the plan (2026-11-24)

Owner asked to focus on the eight features that score 2.0 to 2.5 stars, to have Opus 5.5 advise while Claude Code does the coding, to verify before planning the implementation, and to write it all down in `docs/`. This folder is that record.

| # | Feature | Stars before | Document |
|---|---|---|---|
| 1 | Vision-grounded click | 2.5 | [01-vision-grounded-click.md](01-vision-grounded-click.md) |
| 2 | Admin brain (Qwen) | 2.5 | [02-admin-brain.md](02-admin-brain.md) |
| 3 | Notification brain and GitHub / deploy watchers | 2.5 | [03-notification-brain-and-watchers.md](03-notification-brain-and-watchers.md) |
| 4 | Streaming partial captions | 2.5 | [04-streaming-partial-captions.md](04-streaming-partial-captions.md) |
| 5 | WhatsApp priority people | 2.5 | [05-whatsapp-priority-people.md](05-whatsapp-priority-people.md) |
| 6 | Reverse engineer website / app | 2.5 | [06-reverse-engineer.md](06-reverse-engineer.md) |
| 7 | Focus Mode | 2.5 | [07-focus-mode.md](07-focus-mode.md) |
| 8 | GitHub by voice without owner | 2.5 | [08-github-by-voice.md](08-github-by-voice.md) |

(The numbering follows the order the work is done in, not the order the owner listed them.)

## Method (what "verification first" meant here)

1. **Baseline.** The existing tests of each feature were run unchanged on the merged branch (e20bf84). All green: 134 reverse-engineer, 21 Focus Mode, 83 GitHub-by-voice, 53 notification brain, 25 WhatsApp, 81 vision, 4 Deepgram, 10 brain client = **411 passed, 0 failed**. A green baseline says the code does what its tests say; it does not say the tests ask the right questions.
2. **Read the code for what the tests do not ask.** For every feature the question was: where could a test pass while the real behaviour is wrong (mock-only tests, assertions written from the code's own constants, a path nothing calls)?
3. **Independent review.** An Opus 5.5 advisor, read-only, re-checked every finding against the code (file and line) and looked for more. It confirmed all the main findings, corrected one (Focus Mode's real-process test already existed in my working tree) and added a dozen defects I had missed. Its report is folded into the per-feature documents.
4. **Only then a plan**, ordered by real-world value per hour, with a list of what must not be attempted offline.

## The rules this work runs under

* The owner runs the real NEXUS. Nothing here starts, stops or changes it.
* Offline means: unit and contract tests, mock servers on localhost, real Win32 calls on **child processes the test spawns itself**, fixtures captured from **public unauthenticated** endpoints, and mutation checks (change one rule, a test must fail).
* **Not attempted offline** (each would use the owner's account, key, phone or screen): any live Deepgram, Groq or Gemini call, uploading a screenshot, the real WhatsApp bridge, GitHub merge / approve / close, Vercel / Render / Netlify with tokens, running Qwen, starting NEXUS, and claiming any "Live-proven" score. The Live column stays where it is until the owner runs the checklists at the end of each document.
* No Python is installed on this machine, so Python sidecars (brain server, NLU, STT) cannot be run here.

## Findings, ranked (after the advisor's review)

| Rank | Finding | Where | Feature |
|---|---|---|---|
| 1 | The vision click sends a screenshot of the **whole primary screen** to Groq or Gemini with no blocked-window, password or protected-window check; in race mode to **both** providers | `vision.rs:1007-1020` | Vision |
| 2 | "Clicked X, sir." is spoken when `SendInput` returned. No recheck that the foreground window is still the one captured, after a multi-second cloud call | `orchestrator.rs:9582` | Vision |
| 3 | Compound steps ("X then Y") accept the brain's label with **no confidence gate, no sanitising, no window-open cap** | `command_center.rs:513` | Admin brain |
| 4 | The GitHub watcher takes the newest completed run across **all workflows and all branches**: a passing docs workflow announces "green again" while CI is still red; a failing feature branch announces "build failed" | `memcore/github_watch.rs:86-103` | Notification brain |
| 5 | Streaming captions are **never produced**: the only caller of the stream, `captureUntilSilence`, has no callers; the real mic path is native Rust | `recorder.ts:440,568` | Streaming |
| 6 | The stream ignores `localSttOnly` (privacy mode) and would send live audio to Deepgram | `stt_stream.rs:31-34` | Streaming |
| 7 | The brain's local port is trusted if anything accepts a TCP connection; its output is then routed as commands | `lazy_brain.rs:156-159` | Admin brain |
| 8 | `admin.json`'s `min_confidence` (0.90) is never used; the orchestrator hard-codes 0.5 | `orchestrator.rs:604` | Admin brain |
| 9 | Read-receipt wording promises something unproven ("the sender will not see blue ticks") | `wa.rs:521` | WhatsApp |
| 10 | `keyterm=` is sent with `model=nova-2`; Deepgram documents keyterm as Nova-3 only (Nova-2 uses `keywords`) | `stt_deepgram.rs:10,105` | Streaming |
| 11 | The scanner's HTTP client is built without `.no_proxy()`: under a system proxy the DNS-pinned SSRF guard could be bypassed (to be checked) | `site_probe.rs:1491` | Reverse engineer |
| 12 | Prefix and edit-distance repo matches resolve silently to one repo | `github_repo_index.rs:85-117` | GitHub by voice |

## Order of work (advisor's order, adopted)

1. Vision privacy gate, pre-click recheck, honest wording.
2. Brain gates (compound path, `min_confidence`, port, admin check, trust handshake).
3. GitHub watcher grouping, written red-first on the real captured payload.
4. Streaming: privacy gate, stream generation, double-start, `is_final`, CloseStream, KeepAlive, pill timeout; wiring native capture behind a default-off setting.
5. WhatsApp wording. Reverse-engineer real-site capture with a held-out half.
6. Optional: deploy parser fails closed, `no_proxy`.

Each item carries a mutation check: the rule is removed, a test must fail.

## What the stars can honestly do

Built and Tested go up with this work; Safety goes up where a real hole was closed. **Live-proven does not move** and carries 30 % of the score, so no feature here reaches 4 stars until the owner runs its checklist. Each document ends with that checklist and with the exact log line or screen state that proves it.

## Result (2026-11-24)

**Tests.** Rust lib 3516 passed, 0 failed (was 3480; +36). Frontend 1282 passed (was 1280), tsc clean, build ok. All repository gates pass: window opacity, turn ends, repo hygiene, IPC contract (270 commands; three dead stream commands removed), hard-coded colours, dead-code ratchet (139 to 137 known: `min_confidence` is now used), work-card drift. The family build (`--features custom-protocol`, no admin brain) also compiles. The timing-based stream and real-process tests were run 5 times in a row and once at 4 threads: stable.

**Mutation checks.** 27 rules, each removed in turn, a test must fail: 25 killed first time. Two survived and were **equivalent mutants** (two guards backing each other up; a dropped sender that already ended the session). For the first one the survival showed a real hole in my test (the late message never actually arrived), so the guard logic was extracted into `deliver()` with a direct test, and both rules were re-checked with stronger mutations: **27 of 27 now killed**. Records: `docs/testing/runs/eight-weak-features-mutations-20261124.json` and `...-b-...json`; specs in `scripts/mutations-eight-weak-features*.json`.

**Stars (self-scored, from `scorecard-data.json`).** Built, Tested and Safety moved; **Live-proven did not move for any of the eight.**

| Feature | Before | After | What moved |
|---|---|---|---|
| Streaming partial captions | 45 / 2.5 | 60.8 / 3.0 | it now exists (was never called), privacy and generation rules, 14 real-WebSocket tests |
| Vision-grounded click | 54.2 / 2.5 | 58.3 / 3.0 | window vetted before upload, recheck before click, honest wording |
| Admin brain | 50.5 / 2.5 | 57.0 / 3.0 | one gate for single and compound steps, configured floor, port, admin check |
| Notification brain and watchers | 50.5 / 2.5 | 53.3 / 2.5 | grouping by workflow and branch, real payload fixture |
| WhatsApp priority people | 52.5 / 2.5 | 53.5 / 2.5 | honest wording only |
| Focus Mode | 48.0 / 2.5 | 50.0 / 2.5 | real-process proof of lower and restore |
| GitHub by voice | 50.5 / 2.5 | 51.1 / 2.5 | confirmation read-back pinned |
| Reverse engineer | 48.0 / 2.5 | 49.8 / 2.5 | real-site replay, VitePress and four generators, no_proxy |

Three features cross the 3-star line; none reaches 4. To get there each needs its owner checklist (the end of each document). The biggest single gain per hour is still the live session: for these eight, Live-proven is 0 to 35 and carries 30 %.

**What was changed that you may notice at run time**
* `admin.json` brain confidence floor is now honoured (default 0.90, was a fixed 0.5): brain labels below it fall back to the normal parser. Set `min_confidence` to 0.5 to restore the old behaviour.
* "Click X" by vision refuses a sensitive or unidentified foreground window aloud and does not click if the window changes while it thinks.
* The GitHub watcher says "green again" only when every workflow is green, and ignores feature-branch builds.
* Live captions exist but are off: `"sttLiveCaptions": true` in `settings.json` turns them on.
* The settings panel and chat card no longer promise "no blue ticks".
* The site scanner ignores system proxies.

**Decisions left for the owner**
1. A Hub switch for live captions, and whether race-mode vision (sends the screen to both providers) should say so in the UI.
2. Whether to keep the brain floor at 0.90 or set 0.5.
3. Whether to start the 50-site real-world capture for the reverse engineer.
4. The `brain_server.py` source is not in the repository; the trust handshake needs it.
