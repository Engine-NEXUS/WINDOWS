# Deep links and desktop-app handoff: GitHub, deploy platforms, coding agents

## 1. GitHub web URLs and how to get their ids from the REST API

### Takeaway
The REST API gives you everything needed to build job-level and step-level GitHub links (`run_id`, `job.id`, `run_attempt`, `steps[].number`, plus a ready-made `html_url` on jobs, runs, PRs and review comments). The safest approach is to use the `html_url` the API returns and append anchors only as a best-effort extra. The `#step:N:L` anchor is real but undocumented.

### Cited Findings
- Jobs endpoint `GET /repos/{owner}/{repo}/actions/runs/{run_id}/jobs` returns per job: `id`, `run_id`, `run_attempt`, `html_url`, and `steps[]` with `number`, `name`, `conclusion`. `filter=latest` is the default and `filter=all` includes earlier attempts. `per_page` max is 100. — [GitHub REST: workflow jobs](https://docs.github.com/en/rest/actions/workflow-jobs)
- Per-attempt jobs: `GET /repos/{owner}/{repo}/actions/runs/{run_id}/attempts/{attempt_number}/jobs`. — [GitHub REST: workflow jobs](https://docs.github.com/en/rest/actions/workflow-jobs)
- Job logs: `GET /repos/{owner}/{repo}/actions/jobs/{job_id}/logs` returns a 302 whose `Location` is a plain-text log URL that expires after 1 minute. Single step logs: `GET .../actions/jobs/{job_id}/steps/{step_number}/logs`, also a 302 with a 1-minute link. Private repos need the `repo` scope. — [GitHub REST: workflow jobs](https://docs.github.com/en/rest/actions/workflow-jobs)
- Check run object: `id`, `details_url` ("URL of the integrator's site that has the full details of the check"), and `html_url` (listed in the schema, with no description of where it points). The docs do not explain how these map to Actions job URLs. — [GitHub REST: check runs](https://docs.github.com/en/rest/checks/runs)
- Review comment objects have a required `html_url` (web URL of the comment) and `pull_request_url` (API URL). The docs do not describe the `html_url` format. — [GitHub REST: PR review comments](https://docs.github.com/en/rest/pulls/comments)
- A GitHub issue shows a real job link ending in `#step:5:5`, i.e. the `#step:<step number>:<line>` anchor. It also notes that annotations from newer workflow commands link only to the job, while deprecated commands return step-level links. No official documentation was found. — [joshjohanning/github-actions-log-warning-checker#3 (surfaced via search)](https://github.com/joshjohanning/github-actions-log-warning-checker/issues/3)
- GitHub docs say you can get a permalink to a specific log line by clicking the step's line number and copying the address bar. No anchor syntax is documented. — [GitHub Docs: use workflow run logs](https://docs.github.com/en/actions/how-tos/monitor-workflows/use-workflow-run-logs)

### Inferences
- From my background knowledge, not verified in this session (no fetchable source found): the web patterns are `https://github.com/<o>/<r>/actions/runs/<run_id>`, `.../actions/runs/<run_id>/job/<job_id>`, `.../actions/runs/<run_id>/attempts/<n>`, PR `.../pull/<n>`, `.../pull/<n>/files`, comment anchors `#issuecomment-<id>`, `#discussion_r<id>`, `#pullrequestreview-<id>`. Treat these as high-likelihood but unconfirmed here. The `html_url` fields are the verified way to get exact URLs, and review comment `html_url` values are widely known to include the `#discussion_r<id>` anchor.
- Recommended build: take `job.html_url` and, for the first failed step (`steps[].conclusion == "failure"`), append `#step:<number>:1`. Since the anchor is undocumented, a miss should degrade gracefully to the job page. Forgejo (not GitHub) shows this fallback shape: a missing line still expands the step. — [Forgejo commit via search](https://git.fb1.uni-bremen.de/davrot/forgejo_backup/commit/640a88fa099302919320ab242aa21a2ded1c4760)
- Check-run `html_url` for Actions-created check runs most likely equals the job URL, but this was not confirmed. Prefer the Actions jobs endpoint.

### Gaps
- No official source for the `/job/<id>` and `/attempts/<n>` URL patterns, `#step:N:L` semantics, security alert URLs, or commit status link formats.
- Whether a nonexistent step/line number in the anchor fails gracefully on github.com was not verified.

## 2. Desktop apps for GitHub items (GitHub Desktop, VS Code PR extension, gh CLI)

### Takeaway
Desktop apps can open repos, branches and PRs (VS Code extension), but none was shown to open an Actions run or job log. For failed-job logs the browser (or `gh run view --log-failed` in a terminal) is the realistic target. For a PR, the VS Code extension has a verified URI handler, but it is the only app with one.

### Cited Findings
- VS Code GitHub Pull Requests extension URI handler paths (from `src/common/uri.ts`): `/open-issue-webview`, `/open-pull-request-webview`, `/checkout-pull-request`, `/open-pull-request-changes`. The authority is the extension ID (`github.vscode-pull-request-github`, matched case-insensitively). — [vscode-pull-request-github uri.ts](https://raw.githubusercontent.com/microsoft/vscode-pull-request-github/main/src/common/uri.ts)
- Two query formats are accepted: `uri=https://github.com/<owner>/<repo>/pull/<n>` (issues: `.../issues/<n>`), or legacy JSON with `owner`, `repo`, `pullRequestNumber` (issues: `issueNumber`). All three values are required, and parse or validation failures are silently swallowed. — [uri.ts](https://raw.githubusercontent.com/microsoft/vscode-pull-request-github/main/src/common/uri.ts)
- Real example (Insiders): `vscode-insiders://github.vscode-pull-request-github/checkout-pull-request?%7B%22owner%22%3A%22microsoft%22%2C%22repo%22%3A%22vscode%22%2C%22pullRequestNumber%22%3A987654321%7D`. A nonexistent PR number silently did nothing, with the error only in the Output window. — [microsoft/vscode#275358](https://github.com/microsoft/vscode/issues/275358)
- GitHub Desktop: `x-github-client://openRepo/https://github.com/user/repo?branch=branch` (third-party source). A third-party library also offers an `openFile` helper. — [Protocol Launcher / DEV Community summary](https://dev.to/zhensherlock/protocol-launcher-series-one-click-github-desktop-collaboration-4j29), [protocol-launcher docs](https://zhensherlock.github.io/protocol-launcher/apps/github-desktop.html)
- The Linux desktop entry registers `x-scheme-handler/x-github-client`. A user-reported Windows issue says a manual registry command needed `"GitHubDesktop.exe" --protocol-launcher -- "x-github-client://openRepo/"`. — [lightrun summary of desktop/desktop issue](https://lightrun.com/answers/desktop-desktop-github-desktop-mess-with-x-github-desktop-protocol)
- GitHub's bounty program lists RCE via `x-github-client://` protocol handlers as in scope, so treat the scheme as sensitive and only pass URLs you built. — [GitHub bounty targets](https://bounty.github.com/targets/github-desktop.html)

### Inferences
- Capability matrix (inferred from the sources above):

| Target | Repo | Branch | PR | PR changes | Review comment | Actions run/job log |
|---|---|---|---|---|---|---|
| Browser | yes | yes | yes | yes | yes (anchor) | yes |
| GitHub Desktop (`x-github-client`) | yes | yes | not shown | no | no | no |
| VS Code + PR extension | no | no | yes (webview/checkout) | yes (`open-pull-request-changes`) | not shown | no |
| `gh run view <id> --log-failed` (terminal) | n/a | n/a | n/a | n/a | n/a | yes (text) |

- Suggested fallback order: for PR clicks, VS Code extension URI if installed (and the user has that repo open or accepts a checkout prompt, note `checkout-pull-request` changes local state, so prefer `open-pull-request-webview`), else browser. For workflow failures: always browser at the job URL. Expose "View log in terminal" as a secondary action.
- Detecting installs on Windows (inferred, not sourced): check the protocol handler registration under `HKCU\Software\Classes\<scheme>\shell\open\command` (also HKLM and `HKCR`) for `vscode`, `x-github-client`, `cursor`; or `where code`, `where gh`.

### Gaps
- GitHub Desktop's own protocol docs (the two doc URLs tried returned 404); the exact set of supported actions and whether PR or file params exist was not confirmed from primary sources.
- `gh run view --log-failed` and `gh browse` flags were not fetched or verified here.
- Whether `open-pull-request-webview` works without the repo being open locally was not tested.

## 3. Deployment platforms: Vercel, Render, Netlify, Supabase

### Takeaway
Only Vercel has a verified API field for a dashboard link (`inspectorUrl`). Render's deploy object has no dashboard URL field, and Netlify's `admin_url` points to the site (not the deploy). Dashboard deep links for Render, Netlify deploys and Supabase deployments could not be verified and must be constructed from known identifiers with a fall-back to the project page.

### Cited Findings
- Vercel `GET /v13/deployments/{idOrUrl}` (accepts deployment id or hostname; optional `teamId` or `slug`) has an `inspectorUrl` (string, nullable) in the response schema. — [Vercel REST API: get a deployment](https://vercel.com/docs/rest-api/reference/endpoints/deployments/get-a-deployment-by-id-or-url)
- Render deploy object fields: `id`, `buildId`, `commit`, `image`, `status` (`build_failed`, `update_failed`, `pre_deploy_failed`, `live`, `canceled`, ...), `trigger`, timestamps. No dashboard/console URL field. Retrieve endpoint is `/services/{serviceId}/deploys/{deployId}`. — [Render API: retrieve deploy](https://api-docs.render.com/reference/retrieve-deploy)
- Netlify deploy object example: `id`, `deploy_url` (`<deploy_id>.<site-domain>` style), `admin_url` (example points to the site API URL, not a deploy page), `state`. Documented dashboard patterns are project/team settings only: `https://app.netlify.com/projects/{site_name}/configuration/general` and `https://app.netlify.com/teams/{team_name}/settings/general`. — [Netlify API guide](https://docs.netlify.com/api-and-cli-guides/api-guides/get-started-with-api/)
- Third-party connector docs show a Netlify `admin_url` like `https://app.netlify.com/sites/example-site`. — [withone.ai Netlify connector doc (surfaced via search, third-party)](https://www.withone.ai/knowledge/netlify/conn_mod_def%3A%3AGJ5BNHj0f6U%3A%3A1neMKLL4SYWcUf1z4n7Oug/md)
- Supabase dashboard project paths follow `https://supabase.com/dashboard/project/<project-ref>/...`; an example is `.../functions/<function-name>/logs`, and docs use `_` as a placeholder ref, e.g. `/dashboard/project/_/logs/edge-logs`. — [Supabase logs docs (surfaced via search)](https://supabase.com/docs/guides/monitoring-and-debugging/logs.md)

### Inferences
- From background knowledge, unverified here: Vercel inspect pages look like `https://vercel.com/<team-slug>/<project>/<deployment-id-without-dpl_>`; Render deploys like `https://dashboard.render.com/web/srv-<id>/deploys/dep-<id>`; Netlify like `https://app.netlify.com/projects/<site>/deploys/<deploy_id>` (older `/sites/<site>/deploys/<id>`). Use `inspectorUrl` for Vercel; construct the others and fall back to the service/project page if they 404.
- Required identifiers: Vercel needs the deployment record only (use `inspectorUrl`); Render needs `serviceId` + `deployId` (both known from API polling); Netlify needs site name (or slug) + deploy id; Supabase needs project ref (+ section path).
- Render ids having `srv-`/`dep-` prefixes was not confirmed by the API docs.

### Gaps
- No primary-source confirmation of Render dashboard deploy URL, Netlify deploy-page URL, or any Supabase deployment/branch-history URL. Supabase Management API link fields were not checked.
- Netlify `log_access_attributes` / `build_id` fields not confirmed.

## 4. AI coding agents: focus an existing session from a toast

### Takeaway
No documented URL scheme or CLI was found that focuses an existing Claude Code, Codex CLI or Antigravity terminal session. The realistic option is activating the window by process/title (Win32) or opening the IDE that hosts the agent, not jumping to a specific session.

### Cited Findings
- Cursor registers `cursor://`; `cursor://file/{path}:{line}` was suggested on the forum for opening files. — [Cursor forum: unique open scheme](https://forum.cursor.com/t/does-cursor-have-a-unique-open-scheme/3659)
- Remote workspace links use `cursor://vscode-remote/wsl+Ubuntu/...`; in a 2026 bug report these opened a new window rather than reusing the existing one (Cursor 3.1.15). — [Cursor forum bug report](https://forum.cursor.com/t/cursor-deep-links-to-wsl-files-no-longer-reuse-the-existing-cursor-wsl-window-and-now-open-a-new-window/158160)
- On Linux `xdg-open cursor://...` misbehaved (ran the agent in one report); `cursor --open-url "<url>"` was a workaround. — [Cursor forum](https://forum.cursor.com/t/xdg-open-cursor-runs-agent/135397), [Cursor forum](https://forum.cursor.com/t/bug-in-extension-url-handler-on-linux/48100)
- Cursor deeplinks for MCP installs were flagged as a security attack surface. — [pulse.bot coverage (secondary)](https://www.pulse.bot/cybersecurity/news/cursorjack-attack-path-exposes-code-execution-risk-in-ai-development-environment-657fe4dc-8159-4eda-922c-8023ebddd39b/)

### Inferences
- Because `vscode://`-style schemes can only open files/folders/extension handlers, a toast click can at best open the project folder or file in the IDE; it cannot select a specific agent terminal.
- Practical approach for a Windows app (from general knowledge, unverified): enumerate top-level windows, match by process name and title (Windows Terminal, Code, Cursor), `SetForegroundWindow` with the usual foreground-lock workaround, and fall back to launching the IDE with the project path. NEXUS already stores the agent's `cwd` from hook payloads, which lets it pick the right project window by title. Windows Terminal's `wt.exe` focus semantics were not researched.

### Gaps
- No evidence found for Claude Code (`vscode://anthropic.claude-code/...`), Codex CLI or Antigravity URL schemes or session-focus CLIs; the search for the Claude Code URI returned nothing relevant.
- `wt.exe` tab/window focus options and Antigravity's protocol handler were not researched (tool-call budget).
