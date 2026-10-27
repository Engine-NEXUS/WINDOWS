# 166 — GitHub Actions watcher: builds failed and recovery alerts (2026-10-25)

Built for Task P2 (GitHub Actions watcher — tells you once when a build fails and once when it is green again, repo `D:\ULTRON`).

## 1. Goal and Problem

Developers want immediate spoken awareness when a build breaks or when a broken build turns green again, without being flooded with recurring alerts, without speaking routine successes, and without spending AI credits.

This task adds an automatic background watcher (`src-tauri/src/memcore/github_watch.rs`) for the user's most recently pushed GitHub repositories. It queries GitHub Actions workflow runs, derives structured facts, and feeds them into the notification state machine (`notify_brain.rs`):
- First failure of a repo's builds or PR checks is spoken once ("Build failed in zync, sir." / "PR failed in zync, sir.").
- Subsequent failures for the same ongoing issue stay silent.
- Recovery is spoken once ("zync is green again, sir.").
- Old news at startup or first connection is recorded as a silent baseline.
- When no GitHub token is present or the setting is off, the poller stays completely idle.

## 2. What Is Watched

- **Repositories:** The 5 most recently pushed repositories the user has access to, queried via `GET /user/repos?sort=pushed&per_page=10&affiliation=owner,collaborator,organization_member`.
  - Archived repositories are skipped.
  - Disabled repositories are skipped.
  - Forked repositories are skipped.
  - The first 5 remaining repositories are selected.
- **Completed Runs:** For each selected repository, completed workflow runs are fetched via `GET /repos/{owner}/{repo}/actions/runs?per_page=15&status=completed`.
- **Buckets:** Runs are partitioned into two independent buckets per repository:
  - **PR Checks (`Kind::Pr`):** Event is `pull_request` or `pull_request_target`.
  - **Builds (`Kind::Build`):** All other triggers (e.g. `push`, `schedule`, `workflow_dispatch`).
- **Selection per bucket:**
  - The newest completed run with `status == "completed"` and conclusion mapping to an observation (`Obs::Fixed` for `success`; `Obs::Failed` for `failure`, `timed_out`, `startup_failure`).
  - Evaluated by highest `updated_at_ms`, with ties broken by highest run `id`.
  - Cancelled, skipped, neutral, stale, and action-required runs are ignored, allowing an older completed run to surface if the newer run was cancelled.
  - Runs older than 48 hours are ignored as stale.

## 3. What Is Announced vs What Is Never Announced

### Announced
- **First failure:** Spoken once per issue streak:
  - `Build failed in {repo}, sir.`
  - `PR failed in {repo}, sir.`
- **Recovery (green again):** Spoken once when a previously failing entity completes with `success`:
  - `{repo} is green again, sir.`
- **Persistent failures:** If an ongoing failure streak continues past 6 hours (`STILL_FAILING_MS`), a gentle reminder is spoken:
  - `Build still failing in {repo}, {count} runs, sir.` / `PR checks still failing in {repo}, {count} runs, sir.`
- **Auth expiration:** If GitHub returns `401 Unauthorized`, NEXUS speaks at most once per day:
  - `GitHub needs you to sign in again, sir.`

### Never Announced
- **Old news at startup / connect:** Baseline poll runs silently; existing failures or green states are recorded without speech.
- **Routine successes:** Routine green runs when a repo was already ok or unknown are never spoken.
- **Subsequent failures:** Further failure runs during an active failure streak within the 6-hour lull remain silent.
- **Cancelled / Skipped runs:** Runs with `conclusion` of `cancelled`, `skipped`, `neutral`, `stale`, or `action_required` never change state or trigger speech.
- **In-progress runs:** Runs that have not yet reached `status == "completed"` are ignored.
- **Runs older than 48 hours:** Stale historical runs are dropped.

## 4. API Cost and Rate Limiting

- **Call volume:** 1 repo listing (`/user/repos`) + up to 5 runs listings (`/actions/runs`) per 120-second polling cycle.
- **ETag caching:** Every `/actions/runs` request caches the HTTP `ETag` and sends `If-None-Match`. Unchanged workflow runs return `304 Not Modified`, which yields no facts and consumes zero GitHub API rate limit quota.
- **Rate limit safety:**
  - Reads `x-ratelimit-remaining` from GitHub API response headers.
  - If `x-ratelimit-remaining <= 50` or on HTTP 403 / 429 rate limit responses, the poller backs off to 15 minutes.
  - In normal conditions with active push activity, maximum API call rate is ~150 requests/hour even if zero 304s occur, well within GitHub's 5,000 requests/hour personal token limit.
  - On network/server errors, backoff starts at 240 s and scales exponentially up to a 30-minute cap.

## 5. Privacy and Zero AI Credits

- **Zero AI credits:** All observation extraction, state transitions, and spoken alerts use deterministic logic and tail grammar. No LLM or cloud vision model is invoked.
- **Zero code or commit data persisted:** No commit text, branch names, commit hashes, authors, code diffs, or actors are stored in SQLite. Only minimal entity records (`service: "github"`, `project: repo_name`, `kind: "build" | "pr"`, `run_ref: id`, timestamp) are kept in `entity_state`.
- **Sensitive data logging:** Log lines are prefixed with `[GITHUB-WATCH]` and never print or expose GitHub tokens.

## 6. Settings, Status and Command Hub Switch

- **Rust Settings (`NexusSettings`):**
  - Added `pub memcore_github_watch: bool` (camelCase `memcoreGithubWatch`), default `true`.
  - Added to `NexusSettings::default()` and verified via defaults and round-trip serialization tests.
- **Memory Core Status (`memcore::Status`):**
  - Added `pub github_watch: bool` read from `flag(app_data_dir, "memcoreGithubWatch", true)`.
  - Updated all `Status` constructions in `status()` and unit tests.
- **Frontend Hub Switch (`MemoryPage.tsx` & `memoryModel.ts`):**
  - Added `github_watch?: boolean` to `MemoryStatus`.
  - Added `"memcoreGithubWatch"` to `MemoryFlag` union.
  - Added SwitchCard `"Tell me when builds fail"` under Memory Page right after the plain-words email switch, using `status.github_watch !== false`.

## 7. Files Changed

- `src-tauri/src/memcore/github_watch.rs` (new):
  - Pure parsing and logic: `Run`, `parse_runs`, `obs_of`, `bucket`, `facts_for_repo`, `next_delay_secs`, `pick_repos`.
  - Async network poller: `poll_once` with ETag handling and rate-limit tracking.
  - Background tokio task loop: `start` with 30s tick, pause on Focus Mode, token caching, daily auth nag, and `notify_brain` dispatch.
  - Unit tests: 13 targeted tests covering parsing, all conclusions, event buckets, newest run selection, cancelled run filtering, PR vs push independence, stale run dropping, tie breakers, exponential backoff, repo filtering, and full mock HTTP server flow (first call baseline, 304 Not Modified, new failing run, 401 auth, 403 rate limit).
- `src-tauri/src/memcore/mod.rs`:
  - Added `pub mod github_watch;`.
  - Added `pub github_watch: bool` to `Status`.
  - Added `github_watch` reading to `status()` and updated all `Status` literals.
- `src-tauri/src/commands.rs`:
  - Added `pub memcore_github_watch: bool` to `NexusSettings`.
  - Initialized default in `NexusSettings::default()`.
  - Added `memcore_github_watch_defaults_to_true_and_round_trips` unit test.
- `src-tauri/src/lib.rs`:
  - Started `memcore::github_watch::start(app.handle().clone())` right next to `mailwatch::start`.
- `frontend/src/settings-sidebar/hub/memoryModel.ts`:
  - Added `github_watch?: boolean` to `MemoryStatus`.
  - Added `"memcoreGithubWatch"` to `MemoryFlag`.
- `frontend/src/settings-sidebar/hub/MemoryPage.tsx`:
  - Added SwitchCard for "Tell me when builds fail" with `status.github_watch !== false`.
- `frontend/src/settings-sidebar/hub/memoryModel.test.ts`:
  - Added unit test verifying `github_watch` status field and `"memcoreGithubWatch"` flag key.
- `docs/changes/166-github-actions-watcher.md` (this file).

## 8. What Was NOT Run Live

- `cargo` and `git commit` were not run (per strict task rules; reviewer will compile and run tests).
- A real failing GitHub Actions workflow on live GitHub.
- A real workflow fix to green on live GitHub.
- A NEXUS application restart while a repository is in an active failing state.
- Live repositories under GitHub organizations requiring SAML SSO authorization.

## Review findings (hand-fixes after Antigravity)

- A 403 was treated as a rate limit whenever GitHub sent `x-ratelimit-remaining` (it does on every response), so one repo whose Actions were unreadable (SSO not authorised, Actions off) would have aborted the whole poll. Now a rate limit is only 429, or 403 with `remaining == 0` or a `retry-after` header (`is_rate_limited`, tested); an unreadable repo (403/404/410) is skipped and the others still report (`an_unreadable_repo_is_skipped_and_the_others_still_report`).
- The new `Fact` type lacked `Debug`/`PartialEq` derives, so the tests did not compile; fixed. `Book::records` is `#[cfg(test)]` (only tests use it).

## Verification

`cargo test --lib -- --test-threads=1`: 2623 passed, 0 failed (31 ignored). Frontend `tsc --noEmit` clean, vitest 692 passed. Gates: turn-end, hygiene, IPC contract, window-opacity, dead-code ratchet (148 known, none new), work-card drift check. Mutations killed: forget window removed, "Fixed spoken even if the failure was never spoken", "any 403 is a rate limit".
