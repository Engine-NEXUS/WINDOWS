# Live checklist — 2026-10-27 (run in this order)

Prerequisite: the owner builds and starts NEXUS (`nexus build`, then `nexus start`). Nothing below has run on a device.
Record every result (pass / fail / what you saw) in `docs/testing/live-proof-ledger.md`. Do not commit until all pass.
Offline state at hand-over: Rust lib 2,635 + integration 20, frontend 697, Worker 145, Python 90, all gates pass, dead-code 147.

## 1. Startup sanity (2 min)
1. Start NEXUS. Expect: boot hello, no crash, no stage blackout.
2. Open Command Hub. Expect: switches for Focus Mode, Lower other apps, GitHub watch, Deploy watch are visible and ON.

## 2. GitHub by voice, no owner (5 min) — needs GitHub sign-in
| Say | Expect |
|---|---|
| "analyse zync" | Runs on `owner/zync` (no "which repository?" question). If you own two repos named zync, it asks which. |
| "analyse the PR in zync" | Sidebar lists zync's OPEN PRs (not an analysis of the newest). |
| "analyse the latest PR in zync" | Analyses the newest PR. |
| "analyse PR 34" | Sidebar lists every repo with open PR #34 (or analyses it if only one). Wrong number: "no repo has PR 34". |
| "coderabbit pr 24" | Same lookup across repos (no hidden "ultron" default). |
- Sidebar checks: the list stays (is not replaced by the feed within ~10 s); Analyse and Merge buttons act on the right repo; an empty result shows an empty state, not stale PRs.
- Failure case: an org that has not approved NEXUS should be said in plain words, not "not found".

## 3. Notification wording (3 min)
Trigger or wait for a GitHub CI mail / Actions failure. Expect "Build failed in zync, sir." or "PR failed in zync, sir." — short, project = repo name only, **your own name is never spoken**. A second failure of the same run stays silent; the fix says "zync is green again, sir."

## 4. Watchers (needs a real failure; do what you can)
1. **GitHub Actions**: break a workflow on a small repo, push. Expect one spoken failure within ~2 min; push the fix; expect one recovery line. Restart NEXUS while it is failing: no repeated announcement at start.
2. **Vercel / Render / Netlify**: needs the vault token. A failed production deploy gives one line ("Vercel deploy failed in zync, sir."), the fix gives "zync is live again, sir." A bad token gives one "needs a new token" line per day.
3. **Supabase**: a healthy project is silent on first sight. A paused project is silent. (Failed states are hard to trigger; if you cannot, mark "not run".)
- Hub switches off = silence.

## 5. Focus Mode (the lag claim) (15 min)
1. Open Task Manager (Details tab, add the Priority column). Start a heavy load: a long 4K YouTube video plus a build.
2. Set `handFocusMode` OFF. Start Hand Mode ("hand mode"), arm (palm, fist, palm, fist), move the cursor and click for 2 minutes. Save the console log.
3. Set `handFocusMode` ON. Repeat the same 2 minutes with the same load. Save the log.
4. Compare: `node scripts/hand-focus-report.mjs <log-off> <log-on>` is meant for ONE log with `[FOCUS] on/off` markers; simplest is to run it on the second log. Keep Focus Mode only if `captureToLandmarksP95` and `busySkips` improve.
5. While on: heavy apps show Below normal / Idle priority (never the app in front, never your meeting or music app). Exit Hand Mode: every priority returns to its original value. Kill NEXUS while it is on, restart: priorities are restored.
6. Spoken alerts during Hand Mode are held, then spoken after exit (not lost).

## 6. Hand Mode safety fixes (10 min)
1. **Camera freeze**: arm, hold a pinch drag, then cover/unplug the camera. Expect the drag to release and the mode to disarm within ~2 s (log `camera-stalled`).
2. **Fast hand**: flick the hand quickly; the cursor must not keep moving after you stop (queue coalescing).
3. **Click on a control edge**: clicks still land (snap has a 150 ms limit; a slow UI app falls back to the exact point).
4. Read the `[HAND] fps=` line: `stale=` should stay near 0.

## 7. Speaker verification (the active card) — separate session, see `docs/testing/speaker-verification-work-card.md`
Owner locked trials, a non-owner, a replay, ten starts, profile recovery. Do not mix with the items above.

## Report back
For each numbered item: PASS / FAIL / NOT RUN plus the log line or what you heard. I fix failures from that.
