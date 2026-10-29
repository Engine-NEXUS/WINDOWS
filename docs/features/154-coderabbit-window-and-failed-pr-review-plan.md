# 154 — CodeRabbit window and failed-PR review (research + plan, 2026-10-28)

Status: PLAN ONLY. Nothing here is built.

## What the owner asked for

1. A NEXUS window with the same dimensions and the same interface as the CodeRabbit app (narrow tall panel, ~595 x 975 in the screenshots), no top bar showing "CodeRabbit", no left/right dock buttons.
2. Move the window left/right with the keyboard arrow keys while it is selected.
3. All the information the CodeRabbit app shows, using the CodeRabbit sign-in the owner already has.
4. When a PR fails, run the review and show CodeRabbit's own analysis of that PR and its logs, exactly as CodeRabbit gives it.

## Findings (verified)

| Fact | Evidence |
|---|---|
| The current NEXUS CodeRabbit dashboard is **mock data**. `coderabbit_review_pr_data` falls back to a fabricated PR 24 review; `show_coderabbit_hub` lists `get_active_prs_mock()`; stats are hard-coded (3 files, +120/-34). | `src-tauri/src/coderabbit.rs` lines ~339-510, `commands.rs` ~1370, `frontend/src/sidebar/CodeRabbitDashboard.tsx` |
| Sign-in already works through the official CLI (`coderabbit auth login/status/logout --agent`, OAuth in the browser, region us/eu). NEXUS stores no CodeRabbit password or key. | `docs/changes/137-coderabbit-settings-connection.md`, `coderabbit.rs` |
| The CLI can return real data: `coderabbit pullrequest <n> --show-threads --agent` (the inline review threads CodeRabbit already posted on a PR, as JSON), `--show-prompts` (its consolidated "prompt for AI agents"), `review --agent` (findings for local changes: type `finding` with severity, fileName, title, comment, startLine, endLine, suggestions, codegenInstructions), `usage --agent`, `code ls/show --agent` (the cloud coding tasks shown under "Recents"), `auth org --agent`. | docs.coderabbit.ai/cli/reference, /cli/agent-mode |
| The exact JSON field names of `pullrequest`, `usage`, `code ls` are **not documented**; they must be read from real output. | same pages |
| The REST API (`api.coderabbit.ai`, header `x-coderabbitai-api-key`) is for metrics/users/roles/audit logs; "other APIs require the enterprise plan". Only the Deep Scan findings API is on all plans. | docs.coderabbit.ai/api |
| The owner's account shows **"No Plan"** and the onboarding "Try CodeRabbit for free" is not started. | screenshots |
| **CodeRabbit has never commented on a PR in `Engine-NEXUS/WINDOWS`.** Of 36 PRs, the only bot review on #36 is `copilot-pull-request-reviewer`. So today there is no CodeRabbit review to display. | `gh api repos/Engine-NEXUS/WINDOWS/issues/36/comments` (empty), `/pulls/36/reviews` |
| The web app's own screens (Triage, Analytics, New coding task, Run deep scan, Recents, Personal scope) have no public API on a free account; Triage and Personal scope are locked in the screenshot. | screenshots, API docs |

## Options for "exact same interface with all the information"

A. **Embed the real app** in a NEXUS top-level window (like the Google Maps window, `dyn_windows::open_external_window`): `https://app.coderabbit.ai`, 595 x 975, arrow-key docking, no NEXUS chrome. Pixel-exact and shows every screen, but it is the website (own login cookie in the NEXUS WebView2 profile, cannot be restyled or driven by voice, cannot auto-open a failed PR except by URL).
B. **Native rebuild** in NEXUS React: same layout (sidebar panel: Search, Triage, New coding task, Run deep scan, Analytics, Recents, Quick access, account footer) filled from the CLI + GitHub. Controllable by voice and by the PR-failure trigger, but only shows what the CLI/API exposes (no Analytics/Triage on a free plan).
C. **Hybrid (recommended):** B for what NEXUS can read and act on (failed-PR review, Recents/tasks, usage, account), plus an "Open in CodeRabbit" button that opens A for everything else.

## Plan (recommended: C)

### P0 — Remove the mock data (must come first)
- Delete `get_active_prs_mock` and the synthesized review fallback; an unavailable source shows an honest empty state ("CodeRabbit has not reviewed this PR yet"), never invented findings.
- Parse real `pullrequest --show-threads --agent` / `--show-prompts` output. Capture real samples from the owner's machine first (record them under `src-tauri/tests/fixtures/coderabbit/`) because the schema is undocumented.
- Fetch PR title, branches, file/addition/deletion counts from GitHub (existing token and `github_data.rs`), not constants.
- Second source for the same text: GitHub PR comments/reviews by `coderabbitai[bot]` (walkthrough, summary, actionable comments, committable suggestions, "Prompt for AI Agents"), read with the existing GitHub token. This is CodeRabbit's own wording and works without the CLI once the app is installed on the repo.

### P1 — Make CodeRabbit actually review the owner's PRs (owner action)
- Start the free trial / pick a plan and install the CodeRabbit GitHub app on `Engine-NEXUS` repos (`Add repositories` in the CodeRabbit panel). Without this nothing can be shown for PRs. Public repos can use the free tier; private ones need the trial.
- Fallback that needs no GitHub app: `coderabbit review --agent --base <branch> --remote owner/repo --source-branch <ref>` produces CodeRabbit findings for a failing PR on demand (uses review credits; availability on "No Plan" must be checked live).

### P2 — The window
- New view `coderabbit` in the unified sidebar with fixed size 595 x 975 logical px (clamped to the monitor), centred by default; header row removed (no "CodeRabbit" title bar, no dock buttons, no close button; Esc closes). Reuse `sidebar_geometry` ("coderabbit" arm in `commands.rs`) and `set_sidebar_dock`.
- Arrow keys: Left/Right move the window to the left/right screen edge (with a 10 px margin, like the existing docks), Left from the left edge or Right from the right edge does nothing, Up/Down not used. Handled in the view's `keydown` (same place Esc is handled in `SidebarApp.tsx`); calls `set_sidebar_dock("left"|"right"|"center")`. Only when no text field has focus.
- Layout and tokens copied from the app screenshots: dark surface, 12 px rounded panel, account header (avatar, name, plan chip), search row with `Ctrl K`, nav rows, Recents card, Quick access list, account footer. Window opacity guard `scripts/check-window-opacity.mjs` and registry stay satisfied (solid surface).
- Screens NEXUS cannot fill from real data are shown locked/disabled exactly like CodeRabbit does for the plan, or open the real app (option A).

### P3 — Failed PR → CodeRabbit analysis
- Trigger: `memcore/github_watch.rs` already detects a failed PR check (entity `github:<repo>:pr`). On the first spoken failure, call the review fetch for that PR (P0 sources, newest-first), then say "PR failed in zync, sir. CodeRabbit has 3 findings." and open the window on that PR with the failing check's log summary (GitHub Actions job log tail via the existing token) above CodeRabbit's threads.
- Voice: "show the CodeRabbit review for PR 34 in zync" (owner-less repo resolution already exists, `github_repo_index.rs`).
- Quiet by default: one window opening per failed PR run, never on repeat failures (reuse the notification state machine), switch `coderabbitAutoOpen` in the Hub.

### P4 — Tests and verification
- Parser tests on the recorded real samples; mock-removal test (no fabricated data reachable); arrow-key move tests (pure geometry in Rust, key handler in vitest); trigger test with the injectable clock.
- Live (owner): sign in, install the GitHub app, push a PR that fails CI, confirm the window opens on it with CodeRabbit's real review.

## Decisions (owner, 2026-10-28)

1. **Hybrid (C)** — native NEXUS window in the CodeRabbit layout, plus "Open in CodeRabbit" for the rest.
2. **The owner installs the CodeRabbit GitHub app** (trial + `Engine-NEXUS` repositories). Until then the window shows an honest "CodeRabbit has not reviewed this PR yet" state, never invented findings.

## Sources

- CLI reference: https://docs.coderabbit.ai/cli/reference
- Agent mode (NDJSON records): https://docs.coderabbit.ai/cli/agent-mode.md
- API overview and plan limits: https://docs.coderabbit.ai/api.md
- Existing connection work: `docs/changes/137-coderabbit-settings-connection.md`
