# 167 — Vercel, Render and Netlify deploy watchers (2026-10-27)

Built for Task P3 (Vercel, Render and Netlify deploy watchers, repo `D:\ULTRON`).

## 1. Goal and Problem

Developers deploying applications across multiple modern hosting platforms (Vercel, Render, and Netlify) need spoken awareness when production deployments break or recover, without recurring noise, without routine success announcements, and without spending AI credits.

This task adds an automatic deploy watcher (`src-tauri/src/memcore/deploy_watch.rs`) for the user's connected deploy services. It queries the providers' production deployments, maps terminal states into structured observations, and feeds them into the notification state machine (`notify_brain.rs`):
- First failure of a production deploy is announced once (e.g. "Vercel deploy failed in zync, sir.").
- Subsequent failures for the same ongoing issue stay silent.
- Recovery is announced once ("zync is live again, sir.").
- Baseline state at startup or when first connecting a service is recorded quietly without speaking.
- When no token is present for a service or when the setting is off, the service is completely idle.

## 2. What Is Watched Per Service

- **Vercel (`https://api.vercel.com`):**
  - Personal account deployments via `GET /v6/deployments?limit=20&target=production`.
  - Team deployments via `GET /v2/teams?limit=5` (capped at at most 3 teams), followed by `GET /v6/deployments?limit=20&target=production&teamId=<id>` for each team. If `/v2/teams` returns 403 or 404, it is treated as a personal-only token and continues with personal deployments only.
  - Deployment fields evaluated: `uid` (or fallback `id`), `name` (project), `state` (or older field `readyState`), `target`, and `created` (epoch ms).
  - Terminal state mapping: `READY` maps to `Obs::Fixed`; `ERROR` maps to `Obs::Failed`; non-terminal states (`BUILDING`, `QUEUED`, `INITIALIZING`, `CANCELED`, etc.) are ignored.
  - Per project, the newest deployment by `created` mapping to an observation is kept; deployments older than 7 days are ignored as stale.

- **Render (`https://api.render.com`):**
  - Active web and background services queried via `GET /v1/services?limit=20` (capped at at most 10 services). Suspended services and cron jobs are filtered out.
  - Completed deploys queried via `GET /v1/services/{id}/deploys?limit=5`.
  - Deploy fields evaluated: `id`, `status`, and `createdAt` (RFC3339 timestamp).
  - Terminal state mapping: `live` maps to `Obs::Fixed`; `build_failed`, `update_failed`, and `pre_deploy_failed` map to `Obs::Failed`; non-terminal states (`canceled`, `deactivated`, `created`, `build_in_progress`, `update_in_progress`, `queued`, etc.) are ignored.
  - Per project, the newest deploy by `createdAt` mapping to an observation is kept; deploys older than 7 days are ignored.

- **Netlify (`https://api.netlify.com/api/v1`):**
  - User sites queried via `GET /sites?per_page=20&filter=all` (capped at at most 10 sites).
  - Production deploys queried via `GET /sites/{id}/deploys?per_page=5`. Only deploys with `context == "production"` are evaluated.
  - Deploy fields evaluated: `id`, `state`, and `created_at` (RFC3339 timestamp).
  - Terminal state mapping: `ready` maps to `Obs::Fixed`; `error` maps to `Obs::Failed`; preview and non-terminal states are ignored.
  - Per project, the newest deploy by `created_at` mapping to an observation is kept; deploys older than 7 days are ignored.

## 3. What Is Announced vs What Is Never Announced

### Announced
- **First failure:** Spoken once per broken deploy streak:
  - `{Service} deploy failed in {project}, sir.` (e.g. `Vercel deploy failed in zync, sir.`)
- **Recovery (live again):** Spoken once when a previously failing project successfully deploys:
  - `{project} is live again, sir.`
- **Persistent failures:** If an ongoing failure streak continues past 6 hours (`STILL_FAILING_MS`), a gentle reminder is spoken:
  - `{Service} deploy still failing in {project}, {count} deploys, sir.` (count > 2 adds count)
- **Auth expiration:** If a provider returns `401 Unauthorized`, NEXUS speaks at most once per day:
  - `{Service} needs a new token, sir.` (e.g. `Vercel needs a new token, sir.`)

### Never Announced
- **Old news at startup / connect:** Baseline poll runs quietly; existing failures or green states are recorded without speech.
- **Routine successes:** Routine successful deploys when a project is already ok or unknown are never spoken.
- **Subsequent failures:** Further failure deploys during an active failure streak within the 6-hour lull remain silent.
- **Preview / branch deploys:** Ignored on purpose (only production deploys are tracked).
- **Cancelled / aborted deploys:** Cancelled deploys never trigger speech or hide older terminal deploys.
- **In-progress deploys:** Deploys currently building or queued are ignored until they reach a terminal state.
- **Deploys older than 7 days:** Stale historical deploys are dropped.

## 4. API Cost and Rate Limiting

- **Call volume per cycle (300 s nominal interval):**
  - Vercel: 1 teams listing + 1 personal deployments listing + up to 3 team deployments listings (at most 5 requests per 5-minute cycle).
  - Render: 1 services listing + up to 10 deploys listings (at most 11 requests per 5-minute cycle).
  - Netlify: 1 sites listing + up to 10 deploys listings (at most 11 requests per 5-minute cycle).
- **Rate limit safety:**
  - Response headers: parses `x-ratelimit-remaining` and checks `retry-after`.
  - Precise rate-limit rule: detects HTTP 429 (`TOO_MANY_REQUESTS`) or HTTP 403 with `remaining == 0` or a `retry-after` header via `github_watch::is_rate_limited`.
  - Backs off to 15 minutes (`900 s`) if rate limited or if `remaining <= 50`.
  - An unreadable project/site/service returning 403, 404, or 410 is skipped without failing the rest of the poll.
  - On network errors, backoff starts at 600 s and scales exponentially up to a 30-minute cap.

## 5. Privacy and Zero AI Credits

- **Zero AI credits:** All parsing, observation extraction, state transitions, and spoken alerts use deterministic logic and tail grammar.
- **Zero code or artifact data stored:** No commit messages, environment variables, source code, build logs, or file hashes are stored in SQLite. Only minimal entity records (`service: "vercel"|"render"|"netlify"`, `project`, `kind: "deploy"`, `run_ref: id`, timestamp) are kept in `entity_state`.
- **Credential hygiene:** Auth tokens are retrieved from OS credential storage via `auth_vault::get_token(service)`. Log lines are prefixed with `[DEPLOY-WATCH]` and NEVER print or leak tokens.

## 6. Settings, Status and Command Hub Switch

- **Rust Settings (`NexusSettings`):**
  - Added `pub memcore_deploy_watch: bool` (camelCase `memcoreDeployWatch`), default `true`.
  - Initialized in `NexusSettings::default()`.
  - Unit test `memcore_deploy_watch_defaults_to_true_and_round_trips` verifies defaults and JSON round-trip serialization.
- **Memory Core Status (`memcore::Status`):**
  - Added `pub deploy_watch: bool` read from `flag(app_data_dir, "memcoreDeployWatch", true)`.
  - Updated all `Status` literals in `memcore/mod.rs` and unit tests.
- **Frontend Hub Switch (`MemoryPage.tsx`, `memoryModel.ts`, `SettingsSidebarApp.tsx`):**
  - Added `deploy_watch?: boolean` to `MemoryStatus`.
  - Added `"memcoreDeployWatch"` to `MemoryFlag` union.
  - Added `memcoreDeployWatch?: boolean` to `Settings` interface in `SettingsSidebarApp.tsx`.
  - Added `SwitchCard` "Tell me when deploys fail" with description under Memory Page right after the "Tell me when builds fail" card, bound to `status.deploy_watch !== false`.
  - Added frontend test in `memoryModel.test.ts`.

## 7. Files Changed

- `src-tauri/src/notify_brain.rs`:
  - Added `Kind::Deploy` with `as_str() -> "deploy"`.
  - Added `pub fn render_for(service: &str, line: &Line, address: Option<&str>) -> String` implementing Deploy wording (`{Service} deploy failed in {project}`, `{Service} deploy still failing in {project}`, `{project} is live again`).
  - Preserved `render` signature as a shim calling `render_for("github", line, address)`.
  - Extended `say<R: Runtime>(app, service, line)` with dynamic service prefix in entity ID and event key.
  - Extended word-count tests with Deploy lines for vercel, render, and netlify (all <= 12 words) and exact string checks.
- `src-tauri/src/memcore/deploy_watch.rs` (new):
  - Shared `DeployRun` struct with `Debug, Clone, PartialEq, Eq`.
  - Pure parsers: `parse_vercel`, `parse_teams_vercel`, `parse_render`, `parse_services_render`, `parse_netlify`, `parse_sites_netlify`.
  - Pure aggregation: `facts_for(service, runs, now_ms)` with 7-day cutoff, newest selection, and tie-breaking.
  - Dynamic backoff helper `next_delay_secs(fails, rate_limited, remaining)` with 300 s base and 1800 s cap.
  - Async network poller `poll_service` handling Vercel, Render, and Netlify with precise rate-limiting, 401 auth mapping, and unreadable project skipping.
  - Background loop `start(app)` with 30 s tick, Focus Mode check, credential vault retrieval, daily auth notification, and `notify_brain` dispatch.
  - Unit tests: backoff schedule, realistic parser fixtures for all three providers, non-terminal deploy filtering, staleness filtering, tie-breaker ordering, mock HTTP server flow tests (Vercel, Render, Netlify), 401 auth error test, and 403 unreadable project skip test.
- `src-tauri/src/memcore/test_http.rs` (new):
  - Extracted shared `start_mock_server` helper for HTTP mocking in test environments.
- `src-tauri/src/memcore/github_watch.rs`:
  - Made `is_rate_limited` `pub(crate)` to share with `deploy_watch.rs`.
  - Replaced duplicate `start_mock_server` with import from `super::test_http::start_mock_server`.
  - Updated `notify_brain::say` call to pass `"github"`.
- `src-tauri/src/memcore/mailwatch.rs`:
  - Updated `notify_brain::say` calls to pass `"github"`.
- `src-tauri/src/memcore/mod.rs`:
  - Registered `pub mod deploy_watch;` and `#[cfg(test)] pub(crate) mod test_http;`.
  - Added `pub deploy_watch: bool` to `Status`.
  - Added `deploy_watch` flag retrieval and updated all `Status` literals.
- `src-tauri/src/commands.rs`:
  - Added `pub memcore_deploy_watch: bool` to `NexusSettings`.
  - Added default in `NexusSettings::default()`.
  - Added unit test `memcore_deploy_watch_defaults_to_true_and_round_trips`.
- `src-tauri/src/lib.rs`:
  - Spawned `memcore::deploy_watch::start(app.handle().clone())` right after `memcore::github_watch::start`.
- `frontend/src/settings-sidebar/hub/memoryModel.ts`:
  - Added `deploy_watch?: boolean` to `MemoryStatus`.
  - Added `"memcoreDeployWatch"` to `MemoryFlag`.
- `frontend/src/settings-sidebar/SettingsSidebarApp.tsx`:
  - Added `memcoreDeployWatch?: boolean` to `Settings`.
- `frontend/src/settings-sidebar/hub/MemoryPage.tsx`:
  - Added SwitchCard for "Tell me when deploys fail".
- `frontend/src/settings-sidebar/hub/memoryModel.test.ts`:
  - Added unit test verifying `deploy_watch` status field and `"memcoreDeployWatch"` flag key.

## 8. What Was NOT Run Live

- `cargo` and `git commit` were not run (per strict task rules; reviewer will compile and run tests).
- A real failing production deploy and its subsequent fix on live Vercel.
- A real failing production deploy and its subsequent fix on live Render.
- A real failing production deploy and its subsequent fix on live Netlify.
- A team account deployment flow against live Vercel.
- A NEXUS application restart while a project is in an active failing state.

## Follow-up (2026-10-27): PR-list race and queue entries

- `sidebar:set_view` could arrive after the list it announces (the list rides `orchestrator:event`) and reset a pinned list to the feed. New store action `showFeedUnlessPinned` keeps a pinned list that names a repo; the set_view handler and the mount fallback use it. 3 new tests; mutation (guard disabled) fails 2 of them.
- `docs/testing/scorecard-data.json` has three new queue entries (Focus Mode, GitHub by voice without owner, Notification brain), all with live score 0 until the owner's proofs. Work cards regenerated (68 cards, drift check passes); stale old-numbered card files were removed because the generator does not prune.
- Frontend: tsc clean, vitest 696. Nothing committed; no runtime or build action.

## A4 measurement tool (2026-10-27)

`scripts/hand-focus-report.mjs <console log>` compares Hand Mode with and without Focus Mode from the `[HAND]` / `[FOCUS] on|off` lines (median fps, capture-to-rust and capture-to-landmarks p95, infer p50, busy_skips, gpu_busy share, verdict per metric). `--selftest` passes. Protocol: record one Hand Mode session with `handFocusMode` off and one with it on under the same load (video + build), run the script on both logs; keep Focus Mode only if p95 and busy_skips improve. Not run on real logs (none exist yet).

## Hand Mode H2: camera-stall release (2026-10-27)

Doc 150 phase H2 was not built: a camera freeze during a drag left the button held, because the engine only releases on frames. The 80 ms watchdog now disarms an armed session after 2 s with no frame at all (`frames_stalled`, `FRAME_STALL_DISARM`), which runs the engine's existing drag release (`external_disarm_releases_a_held_drag`). Frames arrive at least every few hundred ms even with no hand in view, so a healthy session is not affected. `cargo test --lib hand::` 87 passed; dead-code ratchet 148, none new. Only the pure decision is unit-tested; the watchdog wiring and a real camera freeze are not run live. H3 (bounded output queue, UIA off the output thread) and H4 are still open.

## Hand Mode H3: bounded output replay and off-thread snapping (2026-10-27)

- The output thread now takes up to 256 queued messages at once and `coalesce`s them: runs of moves collapse to the newest, clicks/presses/releases/scrolls/gestures keep their order, and a stop drops everything except releases (a held button is always let go). A stall can no longer replay an old cursor path.
- Click snapping (UI Automation) runs in a `hand-snap` thread with a 150 ms deadline (`snapped_within`); a hung provider costs one deadline, a late answer is dropped, and only one lookup runs at a time (`SNAP_BUSY`). The shared 2.5 s voice lookup budget is unchanged.
- `cargo test --lib hand::` 90 passed (3 new: coalescing order, stop keeps releases, busy lookup returns at once); `cargo check --lib` clean; dead-code ratchet 148, none new. Not run with a real hung UIA provider or live clicks. The queue itself is still an unbounded channel (the thread drains it in bounded batches); the full newest-slot/ordered-queue split and generation tags remain open (H3 remainder, H4).

## Hand Mode H4 (part): frame sequence and stale-frame rejection (2026-10-27)

Each frame from the hand window now carries a counter (`seq`, frontend `frameSeq`). Rust `accept_frame` rejects duplicates, out-of-order frames and frames whose capture age exceeds 3 s before they touch the engine or the stall clock; a restarted window begins again at 1; frames without `seq` (older page) pass. Rejections are counted and appended to the 5 s `[HAND] fps=` line as `stale=N`. Tests: Rust `hand::` 91 passed (mutation `n > last_seq` -> true fails the new test), vitest hand 20 passed, tsc clean, dead-code 148 none new, IPC and hygiene gates OK. **Not done (needs the camera):** the 640x480 / 480x360 / 320x240 comparison, the corrected constraint call with `getCapabilities()`/`getSettings()`, and adaptive mode switching. The 3 s age limit is a conservative guess, not a measurement.

## Supabase project watcher (2026-10-27)

`deploy_watch.rs` now also polls `GET https://api.supabase.com/v1/projects` with the vault `supabase` token (same 5-minute loop, 401 -> once-a-day "needs a new token", rate-limit rules, quiet baseline, same switch `memcoreDeployWatch`). Only `ACTIVE_HEALTHY` (good) and `INIT_FAILED` / `RESTORE_FAILED` / `PAUSE_FAILED` (failed) are announced; paused, pausing, coming-up and upgrading states are never spoken (a free-tier pause is the owner's/the platform's normal behaviour and the mail watcher already reports pause notices). The API has no status-change time, so the status text is the run reference. Lines: "Supabase project down in zync, sir." / "Supabase project still down in zync, 3 checks" / "zync is healthy again". Tests: `deploy_watch` 18 passed (parser mapping, flow against the mock server), `notify_brain` 13 passed, dead-code 148 none new. Not run against a real Supabase account; the exact status strings come from the public Management API reference and are unverified on a live project.
