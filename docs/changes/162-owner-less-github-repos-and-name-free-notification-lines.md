# 162 — Owner-less GitHub repo resolution, "the PR in <repo>" lists PRs, and name-free notification lines (2026-10-25)

Built by Antigravity from written briefs, reviewed and corrected by hand (see "Review findings"). Plan: `C:\Users\Chitkul Lakshya\.claude\plans\cryptic-questing-wadler.md` (tracks B and C, phase P0).

## GitHub (track B, steps B1-B3)

- `github_repo_index.rs` (new): cached index of your repos, collaborator repos and every organisation's repos (10-minute TTL, up to 3 pages of user repos and 2 pages per org, 403/404 orgs skipped with a `[GITHUB-INDEX]` log). Pure `resolve_in` with exact, prefix and edit-distance tiers, the sound-alias map (zinc/zink/sync -> zync), no substring matching of short queries, and a `Many` result when the same name exists under several owners.
- "analyse zync": `orchestrator.rs` (marked `feature 161 repo resolution`) resolves the bare name first and sends `analyse owner/repo` to the Worker; several owners -> a spoken question and no Worker call; no match -> unchanged. `center.rs` rates a cached single match as Strong evidence so an unenrolled voice is no longer asked "Which repository?".
- "analyse the PR in zync" now lists zync's open PRs (`ListPrs`); "latest/newest/last/recent PR" and "PR by <author>" still analyse (`AnalyseLatestPr`).
- `github_cmd.rs` resolves bare repo names for `ListPrs` (merged list across owners when ambiguous) and for merge/approve/close/comment/check-branch. `fuzzy_match_repo_name` uses the live repo names when cached.
- NOT done yet (next task): "analyse PR 34" with no repo still defaults to `ultron`; the `FindPr` command, the pinned sidebar list and owner-qualified Analyse/Merge buttons.

## Notification wording (track C, P0)

- `memcore/selfnames.rs` (new): the owner's own names (profile name, GitHub login and name, every Google account name and e-mail) so spoken lines never name them.
- `mailwording.rs`: tail-address grammar (`PR failed in zync, sir.`, `Build failed in windows, sir.`, `Review requested in zync, sir.`, `Security alert in zync, sir.`); CI and deploy events never name an actor; person-to-person lines name the actor only if it is not the owner; whole-word check for `ci`; the cloud prompt gets "the owner" instead of the name. `mailtriage.rs` template fallback uses "you" for the owner and clips subjects at 60 characters. `proactive_policy::progress_text` supports tail lines. `agent_watch.rs` no longer speaks raw error text.

## Review findings (hand-fixes after Antigravity)

- `proactive_policy.rs` runtime glue was rewritten from imagination (removed `start_ticker` and the `proactive_snooze` command, invented `tts::speak_proactive_alert`, `cancel_alerts_for_request`): restored from git, edits kept.
- Compile errors in `intent_parser.rs` (type path, f32/f64), a dead "superseded" parser copy left in `parse_latest_pr_analyse`, duplicate module-level helpers in `selfnames.rs`, a test shadowing `force_release` in `focus_mode.rs`, a clip test off by the ellipsis, wrappers only used by tests (now `#[cfg(test)]`), and three items the dead-code ratchet rejected (`current_epoch`, `merge_agent_alerts_into_return`, `drop_pending_with_prefix`: removed).
- `focus_mode::force_release` is now called on app exit (`lib.rs`).

## Verification

`cargo check --lib` clean, no warnings. `cargo test --lib -- --test-threads=1`: 2566 passed, 0 failed (31 ignored), run before the last small deletions; affected suites re-run after. Frontend `tsc` clean, vitest 656 passed. Gates: turn-end, hygiene, IPC contract and window-opacity pass. **Not run live:** any spoken GitHub command, a real org with SSO, a real failing workflow, Focus Mode with a real camera (see record 161).

## Track B steps B4-B5: Find open PR across repos and pinned sidebar PR list

- `src-tauri/src/intent_parser.rs`: bare PR number patterns ("analyse pr 34", "analyse the pr 34", "analyse pull request 34") no longer default to repo `"ultron"`. They return `ParsedIntent::GitHubCommand { command: GitHubCommand::FindPr { pr_number } }` with deterministic source and 1.0 confidence.
- `src-tauri/src/github_repo_index.rs`: added `pub async fn entries() -> Vec<RepoEntry>` that refreshes stale cache reusing the internal refresh helper.
- `src-tauri/src/github_cmd.rs`:
  - Added `FindPr { pr_number: u64 }` to `GitHubCommand` enum.
  - Implemented `execute_find_pr(client, pr_number)`: reads repos from `github_repo_index::entries()`, filters for `open_issues > 0`, sorts descending by `open_issues`, caps at 60 candidates, queries `GET /repos/{owner}/{name}/pulls/{n}` with concurrency 10 using `JoinSet`, keeps only `state == "open"`, skips 404 silently, catches 403/429 to return speakable rate limit/access error when no hits found, and returns `GitHubResult::PrList { repo: "PR #<n>", state: "open", prs }`.
  - Added pure helper `find_pr_summary(pr_number, prs)` producing spoken lines: 0 hits -> `No open PR <n> in your repositories, sir.`, 1 hit -> `PR <n> is open in <repo>`, 2 hits -> `PR <n> is open in <repo1> and <repo2>`, 3+ hits -> `PR <n> is open in <repo1>, <repo2> and <N> more`. Unit-tested for 0, 1, 2, 3, and 5 hits.
  - Updated `is_destructive` to treat `FindPr` as non-destructive, and wired `execute` to `execute_find_pr`.
- `src-tauri/src/center.rs`: added `FindPr` to read-only commands in `is_side_effect` and matched alongside `ListPrs` for `CommandEvidence::Strong` in `command_evidence`. Added unit test.
- `src-tauri/src/orchestrator.rs`: updated `PrList` handler branches (lines 2606 and 9487) to use `find_pr_summary` when `repo` starts with `"PR #"`.
- `frontend/src/pr-list/prListStore.ts`: added `pinned: boolean` and `title: string` to store. `derivePrListTitle(repo)` maps empty/"all repositories" -> `"Open pull requests"`, "PR #" -> `"<repo> in your repositories"`, other -> `"Open PRs in <repo>"`. `showPrList` sets `pinned: true`, title, replaces PR list even when empty, and switches to `pull_requests` subpage. `applyDashboardData` preserves PRs while pinned. `setSubpage` unpins when navigating to other subpages, and `clearPinned` resets pinning.
- `frontend/src/pr-list/PrListApp.tsx`: `allPullRequests` uses `prs` directly when pinned; renders title and "Show all" unpin link above PR cards; renders empty state `No open pull requests found.` when pinned and empty; `handleAnalyse` and `handleMerge` qualify repository as `owner/repo` when owner is available.
- `frontend/src/pr-list/prListStore.test.ts`: added tests for title derivation across the 3 cases, pinned PR preservation across dashboard updates, empty pinned result replacement, and unpin on subpage change.
- **Not run live:** spoken voice command invocation on physical microphone, real GitHub account querying across live org repos for PR #34, real 429 rate limit replay.
