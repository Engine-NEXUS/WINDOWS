# 153 — Focus Mode, owner-less GitHub commands and the general notification brain (PLAN + AS-BUILT)

Date: 2026-10-25/26. Evidence: [research record](../research/focus-github-notification-research-2026-10-26.md). What was actually built, with files and verification: [session overview](../changes/session-2026-10-25-focus-github-notifications-overview.md). Builds on [feature 151](151-notification-brain-event-key-cooldown-and-short-notifications-plan.md) and [feature 150](150-hand-mode-production-refactor-implementation-plan.md).

**Status:** tracks A, B, C (P0–P2) and D are built and unit-tested. **None of it is live-proven.** Track C P3 (Vercel/Render/Netlify) is designed and its brief written, not built.

## 1. Owner requests

1. Hand Mode must not glitch or lag when many processes run; NEXUS should stop its own unneeded background work, lower other heavy apps, and put everything back exactly on exit.
2. "analyse zync" must find the repo among the user's repos and organisations with no owner; "analyse the PR in zync" lists zync's open PRs in the sidebar; "analyse PR 34" with no repo lists every repo that has open PR #34.
3. Notifications from every connected service must be short, smart and non-repeating, and must never speak the account holder's own name ("PR failed in zync, sir." not "CI failed for chitkul lakshya in repo windows").
4. Analyse all features so each can be driven to production one by one.

## 2. Owner decisions

| # | Decision | Taken |
|---|---|---|
| D1 | Hand Mode scope | NEXUS's own tasks plus lowering other apps' CPU priority; **nothing is closed or frozen**, so restoring "exactly as it was" is literal |
| D2 | Order of work | Build the four items in parallel tracks on different files; speaker verification stays the active work card |
| D3 | Cloud e-mail rewrite | Default OFF everywhere; repeats never call a model (feature 151 review) |
| D4 | High-urgency repeats | Short fixed guard (2 min), escalation and Critical break through |
| D5 | Delegation | Code written by Antigravity from written briefs; decisions, review, compile, tests and fixes by Claude; auto-approve flag approved by the owner |

## 3. Design

### Track A — Focus Mode
- `focus_mode.rs`: a lease (epoch counter, heartbeat watchdog 10 s, atomic mirror `paused()`) modelled on `volume.rs`/`keep_awake.rs`; Hand Mode acquires it in `start` and releases it on every exit path; `RunEvent::Exit` force-releases.
- Own work: about 17 loops skip their tick while paused (window cache, session snapshot, resume, activity sampler, curator, reviewer, routines, vault monitor, STT diagnostics, TTS network probe, sentinel, foreground tracker); heavy one-shots wait (TTS cache refresh, voice sync, NLU update check, voice swap download, daily app scan). Never gated: wake stream, meeting detection, stage watchdog/hitbox, keep-awake, webhook, watch-power, mail/WhatsApp/leave-by pollers (their speech is held).
- `proactive_policy`: `Signals.focus` makes Medium alerts wait and stops focus time counting towards expiry, so held alerts are released fresh. High and Critical are unchanged.
- `focus_throttle.rs`: while paused and `handFocusLowerApps` is on it samples per-process CPU (sysinfo, twice about 1 s apart), picks at most 6 user apps above 15 % and lowers each to BELOW_NORMAL plus EcoQoS, raises NEXUS to ABOVE_NORMAL. Never touched: the foreground process family, NEXUS and descendants (WebView2), elevated or other-user processes, system/shell, security software, call/meeting/capture/media apps, `claude.exe`/`agy.exe`; no lowering at all during a meeting; a process already at idle/below-normal is skipped. Originals (priority class, throttle control and state, creation time to defeat pid reuse) are written to `focus_baseline.json` before the first change and restored in reverse order; `recover_on_start` repairs a crash; `restore_now` runs at exit.
- Honest limit: CPU priority only. It cannot change another app's GPU share; the GPU-priority API is an experiment to try only after measurement.

### Track B — GitHub resolution
- `github_repo_index.rs`: cached entries (full name, owner, type, open issue count) from `/user/repos` (3 pages), `/user/orgs` and `/orgs/{org}/repos` (2 pages, 5 concurrent), 10-minute TTL, seeded from the dashboard cache; pure `resolve_in` with exact, prefix (≥ 3 chars), edit-distance tiers, sound aliases, no substring match, `Many` when ambiguous; unreadable orgs are skipped with a log.
- "analyse zync": the orchestrator resolves first and sends `analyse owner/repo`; several owners produce a spoken question; no match keeps the old path. `center.rs` treats a cached single match as Strong evidence.
- "analyse the PR in zync" -> `ListPrs{open}`; "latest/newest/last/recent PR" or "by <author>" still analyse. `github_cmd` qualifies bare repo names for list/merge/approve/close/comment/check-branch.
- "analyse PR 34" -> `FindPr`: repos with open issues, newest-active first, capped at 60, `GET /repos/{r}/pulls/{n}` with concurrency 10; always a list in the sidebar; speaks "PR 34 is open in zync and ledger-ai".
- Sidebar: pinned list with a title and empty state, owner-qualified Analyse/Merge, "Show all" link.

### Track C — Notification brain
- P0 wording (`memcore/selfnames.rs`, tail-address grammar `<What> <state> in <project>, sir.`, no actor on CI/deploy events, whole-word `ci`, no raw error text).
- P1 `notify_brain.rs`: per-entity state `github:<repo>:<pr|build>`; first failure spoken once, repeats silent, recovery spoken once only if the failure was heard, "still failing" after 6 h, failures unconfirmed for 12 h forgotten, quiet baseline at connect/start; persisted in the sealed table `entity_state` (30-day prune, wiped with memory); GitHub CI e-mails feed the same book.
- P2 `memcore/github_watch.rs`: latest completed Actions runs of the 5 most recently pushed repos every 2 min, per PR/push bucket, ETag 304s free, 403/404/410 repos skipped, rate limit only when 429 or 403 with `remaining == 0`/`retry-after`; once-a-day sign-in notice; setting `memcoreGithubWatch` (default on, inert without a token).
- P3 (designed, not built) `memcore/deploy_watch.rs`: Vercel (`/v6/deployments?target=production`, teams capped at 3), Render (services capped at 10, deploys), Netlify (sites capped at 10, production context), same state machine with `Kind::Deploy`, 5-minute interval, tokens from the vault, one switch `memcoreDeployWatch`, preview deploys ignored. Brief: kept with the session scratch notes; relaunch it from this section.

### Track D — Feature work cards
`scripts/make-work-cards.mjs` reads `docs/testing/scorecard-data.json` and generates 65 cards (queue order, axis gaps against the gates, how to close each, shared gates, live-script template, done checklist) plus the [feature process map](../testing/feature-process-map.md); `--check` fails on drift.

## 4. Deviations from the approved plan
- The plan's "restore the missing `merge_agent_alerts_into_return`" was dropped: the function has no compiled caller (`return_review.rs` is not declared in `lib.rs`) and the dead-code ratchet rejects it; see [change 160](../changes/160-notification-brain-review-pass.md).
- Track C P1/P2 were built before B-4/B-5 UI polish and A2's live measurement; the A4 measurement gate (Hand Mode matrix with and without Focus Mode) is **not done**.
- Per-service "Watching" status in the MCP panel was not changed (deferred with P3).
- The `sidebar:set_view` race that can reset the PR list to the feed after a list arrives is documented but not fixed.
- Document numbers collided with another session: change records 163 and 165 exist twice (`163-focus-mode-lowers-other-apps` / `163-voice-capture-accounting`, `165-notification-state-machine` / `165-voice-lock-live-verification-fixes`). Not renamed because other documents reference them by file name; refer to them by full file name.

## 5. Open work (in order)
1. Live proofs (section 6) — nothing here counts as production-ready before them.
2. P3 deploy watchers.
3. A4 measurement of Focus Mode under real load; decide on the GPU-priority experiment.
4. Fix the PR-list `sidebar:set_view` race; add queue entries for these items to `scorecard-data.json`.
5. Hub grouping of notifications and a Worker webhook inbox only if polling proves insufficient.

## 6. Live verification protocol (owner)
- **Focus Mode:** start Hand Mode with a video playing and a build running; Task Manager should show those processes at Below normal with efficiency mode, NEXUS at Above normal; `[FOCUS] lowered N apps: …` in the log; leave Hand Mode; priorities return to the recorded originals; kill NEXUS during Hand Mode, restart, and confirm `[FOCUS] recovered N apps`.
- **GitHub:** say "analyse zync", "analyse the PR in zync", "analyse PR 34" (and a repo name that exists under two owners); an org with SSO.
- **Notifications:** connect GitHub, break a workflow: one line; push another failing commit: silence; fix it: "<repo> is green again, sir."; restart while failing: no repeat. Confirm no spoken line contains the owner's name.

## 7. Verification state at the end of the session
Rust library 2,623 passed / 0 failed (31 ignored); frontend `tsc` clean, vitest 692; turn-end, hygiene, IPC, window-opacity gates pass; dead-code ratchet 148 known / none new; work-card drift check passes; mutation checks killed (PID-reuse check, meeting guard, self-name match, forget window, fixed-only-if-heard, 403 handling).
