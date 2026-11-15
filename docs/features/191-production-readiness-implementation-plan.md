# Production-readiness implementation plan (written 2026-11-11)

Companion to `docs/testing/production-readiness-plan-2026-11-10.md` (the rule and the order of work) and to the research
in `docs/research/session-2026-11-10/`. This file is the plan to *do* the work: scope, steps, pass rules, evidence, risks.

Starting point (from `docs/testing/feature-readiness-2026-11-03.md`): 87 features, 0 production-ready. Nothing in this
plan changes a readiness level until a live-proof ledger line exists for it.

## 0. The rule (restated)

A feature is ready only when, on the current main commit: tests and CI pass; the owner's hardware or account was used
for a recorded run (a line in `docs/testing/live-proof-ledger.md`: build id, date, steps, result); the run followed a
checklist agreed first, with a number where a number makes sense; a failed attempt left a log line that says why; and no
defect is open. "Merged" and "CI green" are not readiness.

## 1. Delivery discipline (before any feature work)

Why: in the last attempts the owner repeatedly ran builds that did not contain the fixes (wrong worktree, a shared output
folder), and features were handed over after fake-event tests only.

Steps:
1. One named build at a time. The build id (`nexus-build.json`) shown in the Command Hub About row and quoted in the
   hand-over message with the exact exe path and the commit/branch it contains.
2. Never share a cargo target folder between worktrees that hold different sources. One target folder per worktree
   (already used: `D:\ULTRON-pr\target`, `target-test` for tests).
3. A hand-over states what was tested live, what was not, and what the owner should try, in that order.
4. Real-use logging for gestures, agent events, announcements and orb transitions: the frontend `[ORB]`, `[SPEECH]`,
   `[TURN]` lines go into `nexus.log` (today they appear only with a debug port or `nexus start`).
5. Before merging a set of PRs: build the combination (`integration/<name>` branch), run all suites on it, run the live
   checks on it. This was done for PRs 52/53/54 (build 90fe404).

Pass: the owner can read, in the app, which commit they are running; every hand-over lists "not tested".

## 2. Agents (Claude Code, Codex, OpenCode, Antigravity, others)

Evidence so far: `docs/research/session-2026-11-10/01-...`, `03-...`.

Steps:
1. Guided live runs, one agent at a time, owner at the keyboard, NEXUS with a debug port and the log open:
   a trivial task, then a task longer than 45 s, then a task that needs a permission.
2. For each: Start seen, Stop seen, spoken line exactly once for a task and silent for a chat reply, Island card appears
   and clears, approval Allow/Deny reaches the agent, answering in the agent's own terminal clears the card.
3. Codex: pick a model the owner's account accepts (or let the owner run Codex themselves) so a real Stop can be seen;
   decide how to coexist with its own `notify` entry (today NEXUS skips it: `HasOwnNotify`).
4. OpenCode: use a model that finishes (the free models tried hung).
5. Record each in the ledger with the build id.

Pass rule per agent: 5 consecutive runs, announcement count equals task count, zero stale Island cards.
Risks: per-agent event shapes differ (Start per model call for Antigravity); account/model problems of the tools.

## 3. Voice: orb, caption, barge-in

Evidence: `docs/research/session-2026-11-10/04-...`.

Steps:
1. Owner speaks over a reply at normal volume and distance: expect duck within 400 ms and the owner's words captured.
2. Ten replies of varied length (one word to 20 s) with nobody speaking: expect zero barge onsets, zero orb flips,
   the orb hides within a few seconds of the end (the 6 s follow-up ring excepted).
3. Measure caption versus audio on the final build, not only the orb.
4. If the owner still sees the speaking shape in silence, capture the `[ORB]` transitions and the Rust log for that
   moment (item 4 of section 1 makes this routine) and trace which closer did not run.
5. Decide whether the 6 s follow-up listening ring should hide sooner; make it a setting if the answer differs by use.

Pass rule: 10 of 10 replies without a false barge-in or stuck orb; 5 of 5 real interruptions caught.
Risks: the gate is now stricter and may miss quiet interruptions; room speech and TV still produce `ref=none` onsets.

## 4. Icons and tiles

Evidence: `docs/research/session-2026-11-10/02-...`.

Steps:
1. See the Antigravity, Gemini, Kilo marks inside the running Island, a toast and the Hub on a build with the tile table.
2. Decide Devin and Netlify with the owner.
3. Replace the 73 "existing (...)" provenance records with a URL and date as each icon is next checked.
4. A dev page that renders island + toast + Hub tile for any agent id.

Pass rule: every id the Island, toasts or Hub can show is visible on its tile in the running app (screenshots kept).

## 5. Owner-hardware proofs, one feature per sitting

Order (from the readiness plan): hand mode (camera), wake word measurement, Ghost mode, dictation, WhatsApp, Spotify.
Each gets the checklist template (section 5 of the readiness plan) filled in before the sitting, a build id, a pass number
and the failure log lines that must explain a miss. Hand mode first because its thresholds were guessed and one live
log already contradicted them.

## 6. Accounts

Google sign-in (7-day Testing-mode token expiry), mail and calendar, GitHub checklist, deploy watchers (Vercel, Render,
Netlify, Supabase). Each needs a live run against the owner's real accounts and a ledger line.

## 7. Platform

- Memory store decryption defect (#42): find the cause (unknown since 2026-11-02) before any memory feature is called
  ready.
- Logon timeline (#73/#15/#79): measure from logon to first speech with `boot_timeline.jsonl`.
- Release path: NSIS installer, Start-menu shortcut that points at an old build, one green CI release.
- Resource footprint: record idle and active RAM against the 600-700 MB ceiling.
- CI: PRs 52/53/54 were opened without reading CI; read it before merging (PR 52 first failed on a hash recorded from
  CRLF content, found only in CI).

## 8. Maps (#56, #77)

Not run yet; the weaknesses below come from code and history, not from a reproduction.

- The keyless panel embeds Google's undocumented `/maps?...&output=embed` redirect, which can change.
- A route without an origin falls back to opening the real Maps site.
- A Routes API key was refused by the Embed API in testing (commit 96e0ed1).
- "Current location" depends on the Windows location service.

Steps: five scripted requests on the current build (a place, a route from the current location, a route between two
places, the nearest pharmacy, "where is"), a screenshot of each, a note of which fail and why; fix in order of failure;
keep the fallback to the real Maps site as the guaranteed path.

Pass rule: 5 of 5 requests show the right place or route, or fall back to the real site with a spoken reason.

## 9. Documentation duties

For every change: a `docs/changes/NNN-*.md` record with a *Verify* and a *Not verified* section; the `AGENTS.md` entry
with the same two lines; ledger lines only for live runs. Do not promote source-tested work to production-ready.

## 10. Open decisions for the owner

1. The follow-up listening ring: keep 6 s, shorten, or make it a setting?
2. Finish announcements: is "45 s task, 3 min cooldown" the right shape? Should short replies ever be announced?
3. Which Codex model/account to test with; which OpenCode model.
4. Devin and Netlify icon versions.
5. Merge order and timing for PRs 52, 53, 54 and the orb watchdog branch.
