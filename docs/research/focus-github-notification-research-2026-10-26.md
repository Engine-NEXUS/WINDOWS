# Research record: Hand Mode load, owner-less GitHub commands, notifications beyond e-mail (2026-10-25/26)

Status: research only. Every statement was read from the working tree on `D:\ULTRON` on 2026-10-25 unless marked **(external)**, which means it comes from general knowledge of a provider's public API and must be re-verified before relying on it. The plan built from it is [feature 153](../features/153-focus-mode-github-resolution-and-notification-programme.md); the as-built record is [the session overview](../changes/session-2026-10-25-focus-github-notifications-overview.md).

Three read-only research passes (Hand Mode and background work; GitHub repo/PR voice commands; notification sources) plus a read of the existing scorecard produced this record. No `[HAND]` log lines existed on the machine, so no measured Hand Mode numbers were available.

---

## 1. Hand Mode under load

### 1.1 How it runs
- `hand::start` (`src-tauri/src/hand/mod.rs`) creates the `hand` WebView2 window (hidden 8x8, or 300x250 with `handShowCamera`). The page (`frontend/src/hand/main.ts`) opens the camera at 640x480/30 fps ideal, reads frames from `MediaStreamTrackProcessor` (fallback `requestVideoFrameCallback`), runs MediaPipe `detectForVideo` synchronously on the page thread (GPU or CPU delegate chosen by a 4-frame warm-up and 25-frame trial) and sends one `invoke("hand_frame")` at a time (extra frames are skipped and counted as `busy_skips`).
- Rust `hand_frame` takes the `SESSION` lock, runs the pure engine, emits `hand:state` every 50 ms and logs `[HAND]` metrics every 5 s. Output goes through an unbounded channel to the `hand-out` thread (enigo, `SendInput` absolute coordinates, 150 ms foreground-guard cache, UI Automation click snapping on the same thread). An 80 ms tokio watchdog checks window death, physical mouse takeover, monitor changes.
- Shared resources: the WebView2 GPU process is shared with the always-on stage window (WebGL orb); camera driver rate (15 fps on battery, 30 on AC, 6 in dim light per earlier field tests); EcoQoS is enabled for the whole `nexus.exe` (`power.rs`); nothing sets thread or process priority anywhere.

### 1.2 Documented causes of lag (docs 115/118/150, AGENTS.md)
Camera delivery rate; GPU contention with a third app (Brave tripled `busy_skips` and `infer_p50`); UI Automation on the output thread (up to the shared 2,500 ms budget); an unbounded output queue that replays stale input; no safety release when frames stop (12 s warning only); one in-flight `invoke` (bottleneck unproven); EcoQoS effect on the WebView2 renderer is an open hypothesis with no recorded A/B. Doc 150 already plans H0–H7 for most of these.

### 1.3 What reacted to Hand Mode before this work
Only the wake classifier (`hand::suppresses_voice`, read at `wakeword_oww.rs`). No poller consulted `hand::active()`. The single contention heuristic (`hand/mod.rs`, `infer_p50` against the calibrated baseline, 2x for two windows) only reported, halved the frontend frame rate and lowered the orb particle cap; it paused no background work and changed no priority.

### 1.4 Inventory of NEXUS's own background work
Always on: window cache refresh (every 2 s; EnumWindows plus a full sysinfo process refresh, scales with the number of running processes); foreground tracker (2 s); activity sampler (2 s); meeting/mic detection (2 s); stage hitbox thread (250 ms idle, 16 ms near the orb) and stage watchdog (2 s, rebuilds the stage if its heartbeat is older than 10 s); proactive ticker (2 s); keep-awake poller (3 s); session snapshot (60 s; process refresh and a PowerShell call for Explorer folders); resume sampler (30 s; UI Automation for browser URLs); mail/WhatsApp/leave-by/scheduler/lease pollers (30–60 s ticks); curator, reviewer, routine suggester; Google sentinel (15 s); vault monitor (90 s); TTS network probe (30–60 s); STT diagnostics sweep (60 s). Lazy sidecars (STT, NLU, agent-watch) already die when idle. One-shot heavy jobs: TTS cache refresh (42–51 s of CPU per doc 125), offline voice sync, NLU update check, voice swap downloads, daily app scan.

Must not be paused: wake audio stream, meeting detection, the stage watchdog/heartbeat and hitbox thread, keep-awake, webhook listener. The watch-power countdown is cancelled by any mouse movement (so a moving hand cancels it, by design).

### 1.5 Reusable machinery and what is missing
Reusable: the lease with epochs, heartbeat watchdog and durable baseline file (`volume.rs`, `keep_awake.rs`); `proactive_policy` snooze (held alerts stay fresh because expiry counts only speakable time); the `memcore::flag` gates already on most memcore loops; `MeetingState` atomics; lazy sidecar idle-kill; `session.rs` snapshot and `restore.rs` for "reopen what was open". Missing: any priority/EcoQoS change on other processes, any system CPU/GPU sampler, any capture of window placement, media state or downloads, and a shared registry of background loops. "Exact same point" therefore means different things per target: NEXUS timers keep their own clocks, sidecars cannot be frozen, and user apps cannot be restored by reopening; hence the decision to lower priority instead of closing anything (feature 153, decision D1).

---

## 2. GitHub repo and PR voice commands

### 2.1 Findings (before this work)
- `analyse zync` parsed to `AnalyseRepo{owner:None, repo:"zync"}` but Rust never resolved the owner. The Worker's `resolveRepo` (`server/worker/src/index.ts`) did: first 100 repos by recent update, no pagination, no `/user/orgs`, substring matching, silently picks the first match.
- The typed path (`github_cmd.rs`, octocrab) had no resolver: `split_repo("zync")` gave an empty owner and `list prs in zync` called `/repos//zync/...`.
- A bare `analyse PR 34` was parsed with a hard-coded repo `"ultron"` (Rust parser, the legacy TypeScript parser, and `coderabbit.rs`).
- "the PR in zync" was pinned by 16 tests to `AnalyseLatestPr` (newest PR), not a list.
- `fuzzy_match_repo_name` compared against a hard-coded `KNOWN_REPOS` list of six names.
- `center.rs` rated a bare `AnalyseRepo` as Medium evidence, so an unenrolled voice got "Which repository should I use, sir?" even when the repo was spoken.
- Sidebar: `PrListApp` can show several repos, but `applyDashboardData` and the `github:dashboard_updated` listener overwrote any filtered list; empty results kept stale PRs; Analyse/Merge sent a bare repo name.
- Tokens: Rust resolves vault, then `gh auth token`, then `GITHUB_TOKEN`, then the Worker; the Worker uses its own OAuth token; scopes requested are `repo read:org workflow`. The two can disagree about visibility.
- Caveats **(external)**: organisation OAuth-app restrictions hide private org repos silently; SAML SSO needs per-org token authorisation (403); REST allows 5,000 requests per hour; search is limited to 30 per minute.

### 2.2 Smallest viable design (chosen)
One cached index of the user's repos plus every org's repos; a pure resolver (exact, prefix, edit distance, sound aliases, no substring for short queries, `Many` when ambiguous); resolve in Rust and hand `owner/repo` to the Worker; bare "PR in X" lists open PRs; a number-only request searches all indexed repos for that open PR number; the sidebar list is pinned so refreshes cannot overwrite it.

---

## 3. Notification sources beyond e-mail

### 3.1 What existed
`notify.rs` (event keys), `proactive_policy.rs` (tiers, cooldown), `memcore/mailwording.rs` / `mailtriage.rs` / `mailwatch.rs`, `memcore/wa.rs`, the legacy `google/sentinel.rs`, `agent_watch.rs`, loopback-only `webhook.rs`. **No provider watcher existed**: a repo-wide search found no calls to the Vercel, Render, Netlify, Supabase, Stripe, Slack or GitHub notifications/Actions APIs. Tokens for those services are stored in the OS vault (`auth_vault::VAULT_SERVICES`) but nothing read them; the MCP panel's "deployment tracking" text described features that were not implemented. Static tokens are never validated ("live" only means present).

### 3.2 Where the owner's name entered speech
`sender_label()` (the e-mail `From` display name; for GitHub Actions mail that is the actor, i.e. the owner) in `github_plain`, `category_plain`, the template `alert_text`, `batch_text` and the cloud rewrite prompt. The owner's names are available locally: user profile, `github_profile.json`, connected Google accounts. `github_plain` also used a `contains("ci")` substring test and said "A GitHub check failed", naming no workflow or PR/push kind.

### 3.3 Provider API options **(external, re-verify before building)**
| Service | Polling endpoint | Auth | Notes |
|---|---|---|---|
| GitHub Actions | `GET /repos/{o}/{r}/actions/runs?status=completed` (`conclusion`, `event`) | token with `repo` | 5,000/h; ETag 304 is free; webhooks need a public URL |
| GitHub notifications | `GET /notifications` | classic/OAuth token | honour `X-Poll-Interval`; subject for CI is a CheckSuite |
| Vercel | `GET /v6/deployments?target=production` (`state`: READY/ERROR/BUILDING…) | account token (+`teamId`) | signed webhooks exist (plan-dependent) |
| Render | `GET /v1/services`, `/v1/services/{id}/deploys` (`status`: live/build_failed/update_failed…) | API key | workspace webhooks exist |
| Netlify | `GET /api/v1/sites/{id}/deploys` (`state`: ready/error…, `context`) | personal access token | per-site hooks exist |
| Supabase | Management API project status | `sbp_` token | database webhooks are table-level only |
| Stripe / Slack | events API / Socket Mode | restricted key / app token | webhooks or Socket Mode needed for push; do not speak amounts or names |

Polling with tokens already in the vault needs no server. Webhooks would need a public endpoint; the Worker has no inbound event inbox and the Durable Object only relays pairing frames, so webhooks are deferred.

### 3.4 State and recovery
The cooldown keyed on the event would have muted a "fixed" notice (state is not in the key) and the key ring is in memory only. Recovery therefore needs a small persisted per-entity state: ok/failing, first-failed time, failure count, last run id, whether the failure was spoken, last spoken time. Privacy: store only service, project slug, state, counters, run reference and timestamps.

---

## 4. Feature programme
`docs/testing/scorecard-data.json` and `windows1-production-queue.md` already rank 65 in-scope features by a rubric (B 20 %, T 20 %, L 30 %, S 15 %, I 15 %; production needs raw ≥ 90 % and gates B≥90, T≥70, L≥70, S≥80, I≥60). The queue rule is "do not start the next feature while the active card is pending"; speaker verification is the active card. What was missing was one repeatable work card per feature and a map of which processes each feature runs as.

---

## 5. Process finding: delegating code to Antigravity
`agy --print … --mode accept-edits` cannot run unattended: headless mode auto-denies every shell command, so a first run produced nothing. With `--sandbox --dangerously-skip-permissions` (explicitly approved by the owner) it works, but review found recurring faults, all caught by compiling and by the repository's dead-code ratchet: it rewrote `proactive_policy.rs` runtime glue from memory (dropping `start_ticker` and the `proactive_snooze` command, inventing `tts::speak_proactive_alert`); left dead "superseded" function copies behind `#[expect]`; dropped a `#[test]` attribute; wrote code that did not compile (type paths, f32/f64) and tests that did not compile (missing `Debug`/`PartialEq` derives); and treated every HTTP 403 as a rate limit. Briefs now carry strict editing rules, forbid cargo/commit, and every result is diffed, compiled, tested and mutation-checked by hand.
