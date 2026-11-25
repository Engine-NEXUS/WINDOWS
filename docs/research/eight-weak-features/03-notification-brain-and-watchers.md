# 3. Notification brain and the GitHub / deploy watchers

Scorecard before: Built 90, Tested 80, Live 0, Safety 80, Industry 30 (2.5 stars).

## What it is

`notify_brain.rs` keeps one state per entity ("github:zync:build"): the first failure is spoken once, more failures are silent, a recovery is spoken once, a failure that goes on is mentioned again only after 6 hours. `memcore/github_watch.rs` polls the latest completed GitHub Actions runs of the 5 most recently pushed repos every 2 minutes (ETag 304s are free, rate-limit aware). `memcore/deploy_watch.rs` does the same for Vercel, Render, Netlify and Supabase.

## Verification (what was found)

Baseline: 53 notification-brain tests, 19 + 18 watcher tests, all pass. They are built on hand-written JSON.

**The real defect was in the logic, not the parsing.** `facts_for_repo` picked the newest completed run of **any workflow on any branch** per "Build" bucket:

| Situation | What NEXUS said | What is true |
|---|---|---|
| CI failed 30 minutes ago, the docs workflow passed 5 minutes ago | "zync is green again" | CI is still red |
| A feature branch's build failed | "Build failed in zync" | `main` is fine |
| PR #5's check failed, PR #6's passed | "fixed" | PR #5 is still red |
| A workflow that fails and one that passes on alternate pushes | fail / fixed / fail / fixed, each spoken | one steady failure |

(Found by reading the code and confirmed by the Opus advisor. The real payload had several workflows, which is why a real fixture matters.)

Other findings, not changed: `job_hints` treats `cancelled` as failed unlike `obs_of` (`github_watch.rs`, only affects the extra detail line); `deploy_watch.rs:111` treats a Vercel deployment with no target as production (the request does ask for production, so low).

## Real data

`src-tauri/tests/fixtures/github/actions_runs_public_tokio.json` is **a real GitHub Actions response**, fetched without a token from the public `tokio-rs/tokio` repo on 2026-11-24 and trimmed to the fields the parser reads. 15 completed runs, 4 workflows, and the mix a real repo has: success, cancelled, skipped, `pull_request` and `pull_request_target`. A test parses all 15, checks workflows and branches come through, checks an all-green sample never reports a failure, and checks that turning one real run red makes the repo fail.

## What was built

`facts_for_repo_on(repo, runs, now, default_branch)`:

* the newest **decided** run of each (workflow, branch) pair;
* a kind is **failing while any pair is red**, **fixed only when every pair is green**; the fact names the newest red run (or, when fixed, the newest green one);
* **Build counts only the default branch** when it is known (`default_branch` comes from the same repo list the watcher already fetches); with it unknown nothing is dropped (the old behaviour);
* cancelled, skipped and neutral runs still decide nothing; runs older than 48 hours are still ignored.

## Verification of the fix

`memcore::github_watch::grouping_tests` (6 tests) plus the 20 existing ones, which pass unchanged: docs-passes-while-CI-red stays failed; fixed only when every workflow is green; a failing feature branch is not the build (and with no default known it is, the safe old behaviour); a red PR branch keeps the PR kind failing; cancelled/skipped/neutral decide nothing; the real captured payload. Three mutations (grouping removed, default-branch filter removed, green-over-red priority) are in the index.

## Not done, and why

* **No real failing workflow, deploy or restart-while-failing was run** (needs the owner's accounts). The Live score stays 0 until the checklist is done.
* Vercel / Render / Netlify have the same "one entity per project" shape; they are production-target-only and one deploy at a time, so the multi-workflow problem does not apply, but they were not changed.
* `job_hints` cancelled handling and the Vercel no-target parser are noted for a later pass.

## Owner checklist (the only thing that moves Live-proven)

1. Push a commit that fails CI on `main` of a repo with two workflows. Expected: one spoken "Build failed in <repo>, sir."
2. Let the other workflow pass. Expected: **silence** (CI is still red).
3. Fix CI. Expected: one spoken "<repo> is green again, sir."
4. Push a failing commit on a feature branch only. Expected: silence.
5. Restart NEXUS while a build is red. Expected: no repeat on start (state is persisted), no loss of the red state.
6. Log lines to read: `[GITHUB-WATCH]`.
