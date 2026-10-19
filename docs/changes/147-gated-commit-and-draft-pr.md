# 147 — Gated commit + draft PR after an agent finishes (plan P5)

Status: module built and unit-tested; **not wired**, **never run against a real repository or GitHub**.

## What was built

`src-tauri/src/agent_followup.rs` (plus `pub(crate) mod agent_followup;` in `lib.rs`). Nothing else was touched; the tracker/runner that calls it is another session's work.

- `FollowUp::CommitAndPr { dir, agent, stamp, message_hint, base }`. `agent` and `stamp` (`yyyymmdd-hhmm`, `default_stamp()`) were added to the shape in the brief so the planner stays pure and the branch is `nexus/<agent>-<stamp>`. Empty `base` means `main`.
- Pure planner `plan_followup(&Verdict, &FollowUp, &GitStatus) -> Decision` (`Run(Steps)` / `Skip(SkipReason)`). It runs **only on `Verdict::Go`**.
  - `Hold` → "Commit stopped because errors were found in the response: <reasons>. The line was: "<failure line>"." (reuses `session_verdict::spoken_reasons` and the quoted `FailureMarker` line).
  - `Unsure` → "Commit stopped because I could not confirm the response was clean: <reasons>." This is deliberately **not** the "errors were found" wording: an Unsure verdict is not proof of errors.
  - Other skips, each with its own sentence: repository mid merge/rebase/cherry-pick/revert/bisect, detached HEAD, no commits yet, unresolved conflicts, nothing changed, more than 200 changed files, unreadable repository, unusable base branch.
  - **Secrets refusal**: any changed path (or the old name of a rename) that matches `.env*`, `*.pem`, `*.key`, `*.pfx`, `*.p12`, `*.ppk`, `*.jks`, `*.keystore`, `id_rsa*`/`id_dsa*`/`id_ecdsa*`/`id_ed25519*`, `credentials*`, `.netrc`, `.pgpass`, anything under `.ssh/`, `.aws/`, `.gnupg/`, or with `token` / `secret` in the file name refuses the WHOLE commit and the files are named. This is strict on purpose: it also refuses ordinary source files such as `tokenizer.rs` or `.env.example`. A skipped commit is cheap; a leaked key is not.
- `parse_porcelain` (both `-z` and newline form, quoted paths, renames, branch line for detached / unborn).
- `trait GitRunner` (`Send + Sync`) with `RealGit` (argument arrays only, 60 s kill timeout on every call, `GIT_TERMINAL_PROMPT=0`, no console window) and a fake in the tests.
- Calls, in order: read-only `rev-parse --show-toplevel --absolute-git-dir` and `status --porcelain=v1 -z --untracked-files=all -b` (only for `Go`), then `checkout -b nexus/<agent>-<stamp>`, `add -- <each listed path>` (never a blanket add), `commit -m <subject>`, `push -u origin <branch>`, `remote get-url origin`. Git runs from the repository root so paths from `status` resolve even when `dir` is a sub folder. Commit subject = first line of the agent's final text, markdown stripped, max 72 chars; else `NEXUS: changes from <agent>`.
- Draft PR through the existing `github_cmd::GitHubCommand::CreatePr` with `draft: true` via `github_cmd::execute_command(.., confirmed = false)` (CreatePr is non-destructive). No new GitHub client; nothing merges.
- Push failure (no remote, no credentials, timeout) is reported plainly, credentials in URLs are scrubbed from the sentence, and the local commit is left in place.
- `describe(&Decision)` and `describe_outcome(&GitOutcome, Option<&PrOutcome>) -> Report` give what NEXUS says.

## What the tracker should call

```rust
let step = agent_followup::FollowUp::CommitAndPr {
    dir, agent: "Claude Code".into(), stamp: agent_followup::default_stamp(),
    message_hint: final_text, base: "main".into(),
};
let report = agent_followup::run_followup(
    std::sync::Arc::new(agent_followup::RealGit::default()), step, verdict, &worker_url, &user_id,
).await;
// speak report.spoken; report.pr_url / pr_opened / committed / pushed / skipped are for the sidebar and the overnight record
```

Call it once per finished agent, with that agent's own verdict. The shutdown decision must not depend on `report`. For a "what will you do" read-back before the unattended run, call `agent_followup::describe(..)` on a `Decision` from `prepare(&RealGit::default(), &step, &verdict)` (read-only git calls, only for `Go`).

Side effect to know about: the new branch is checked out in the user's working tree and stays checked out afterwards. Everything changed in the repository (including changes that were not the agent's) is committed, because git cannot tell them apart.

## Verification

- `cargo test --lib --features custom-protocol,admin-brain -- agent_followup`: 25 passed, 0 failed, no warnings from the file. `cargo check --features custom-protocol,admin-brain` finished clean. (A first attempt was blocked for a while by another session's mid-edit `watch_runner.rs` test code; in that window the module was also run in a scratch crate with the real `session_verdict.rs` and stub `github_cmd` / `pii_filter`: 25 passed.)
- Mutation checks (each applied to the file, tests run, file restored; every one made tests fail):
  - Go-only gate removed from `plan_followup` → 3 failures; removed from `prepare` (the zero-git-call gate) → `hold_and_unsure_never_call_the_runner` fails.
  - Secrets refusal disabled → 2 failures.
  - Staging changed to a blanket add → 3 failures (exact-sequence, specific-paths, and the source grep).
  - Push/step failure ignored → 3 failures.
- Tests cover: exact git argument sequence on a fake for Go; zero runner calls for Hold and Unsure; secrets refused and named (including a rename's old name) with no mutating call; empty / detached / unborn / conflicts / in-progress / too many files / bad base skip; push failure keeps the commit and scrubs URL credentials; a failing step stops the rest; runner error (timeout text); draft PR command; remote URL parsing; commit message and branch name cleaning; a source grep that production code has no shell string, no `format!` into Command and no blanket add.
- **Dead-code gate (`node scripts/dead-code-audit.mjs`) will list ~51 `agent_followup.rs` items as new until the tracker calls `run_followup` (or `prepare` + `execute_git`).** The gate force-warns through `expect(dead_code)`, so the module-level `#![expect(dead_code, ...)]` only silences the compiler. Wire it, then the list disappears; remove the `expect` line at that point (it becomes an unfulfilled expectation).

## NOT run

- Nothing was run against a real git repository, a real remote, or GitHub. `RealGit` (spawn, timeout kill, no-window flag, env), `gather_status` against real `git`, `open_draft_pr` / the Worker token path, and the async `run_followup` have no live test. The unit tests use a fake runner.
- Not wired into the orchestrator, `command_center`, or any tracker; the module is `expect(dead_code)` until then.
- The `.git` marker check for rebases uses the real file system and is tested with a temp directory only.
- Hooks, signing and missing `user.name` surface as a plain commit failure; the behaviour with a real pre-commit hook was not exercised.
