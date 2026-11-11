# Production-readiness plan — 2026-11-10

Written after the owner reported that none of the recent attempts is production ready. It builds on `feature-readiness-2026-11-03.md` (87 features, **0 production-ready**) and `live-proof-ledger.md`. Read-only analysis: nothing here was built or run to write it.

## 1. What "production ready" means here (one rule for every feature)

A feature is **ready** only when all of these are true on the **current main commit**:

1. Unit and integration tests pass, and CI is green.
2. The owner's real hardware or account has been used for a recorded run (a line in `live-proof-ledger.md`: build id, date, what was done, result).
3. The run follows a written **acceptance checklist agreed before the work**, with a number (for example "18 of 20 attempts") where a number makes sense.
4. A failed attempt leaves a log line that says why (measurements, not just an outcome).
5. No known open defect.

"Merged" and "CI green" are not readiness. The build the owner tests is named in the app and in the message that hands it over.

## 2. Why the recent attempts failed (evidence from 2026-11-09)

- Tested with faked events; real camera/agent behavior was never exercised before handing over.
- The owner ran builds that did not contain the fixes (wrong worktree, shared output folder).
- Thresholds were guessed (palm hold 2 s → real log showed the hold never completed).
- An intermittent startup defect (a page missing its first event listener) was worked around, not explained.
- Rules were coded before the owner's wording was agreed ("while you were away").
- Hard-coded facts in diagnostics (15 sub-centers when 18 exist).

## 3. Order of work

Phase 0 — delivery discipline (before any feature work)
- One named build at a time; `nexus-build.json` id shown in the Hub; hand-over message states the exact exe and what it contains.
- Real-use logging for gestures, agent events and away/welcome decisions.

Phase 1 — real-agent proof (no hardware of the owner needed): Claude Code, Codex, OpenCode, Antigravity each run a trivial task; check hooks, island, finish wording, approval clearing, keep-awake release. (#34, #36, #39, #40, #76, Agent Island)

Phase 2 — owner-hardware proofs, one feature per sitting, each with its checklist: Hand mode (camera), wake word measurement, Ghost mode, dictation, WhatsApp, Spotify.

Phase 3 — accounts: Google sign-in, mail/calendar, GitHub checklist, deploy watchers.

Phase 4 — platform: memory store decryption (#42), logon timeline (#73/#15/#79), release path (NSIS, shortcut, one green CI), resource footprint.

Phase 5 — Maps (below) and the remaining level-2 rows from a single recorded 30-minute smoke session.

## 4. Maps (#56, #77): current state and plan

- Code: `maps_panel.rs` (keyless embed in a NEXUS panel), `location.rs` (current location), `google/maps.rs` (Routes/Places), intents for directions, "where is", "nearest …".
- Known weaknesses from the code and history: the keyless embed uses Google's undocumented `/maps?…&output=embed` redirect, which can change; a route without an origin falls back to the old window; a Routes API key was refused by the Embed API in testing (commit 96e0ed1); current location depends on the Windows location service; no recorded run exists.
- Plan: reproduce on the current build with five scripted requests (place, route from current location, route between two places, nearest pharmacy, "where is"), screenshot each, record which fails and why; then fix in order of what fails; keep the fallback to the real Maps site as the guaranteed path.

## 5. Per-feature acceptance checklist template

```
Feature:
Build id:
Setup (hardware/account):
Steps (numbered, what the owner does):
Pass rule (e.g. 18/20, latency < x s):
Log lines that must appear:
Failure log lines that explain a miss:
Result (date, build, numbers, pass/fail):
```
