# 8. GitHub by voice without owner

Scorecard before: Built 90, Tested 80, Live 0, Safety 80, Industry 30 (2.5 stars).

## What it is

"Analyse zync", "analyse the PR in zync", "analyse PR 34" work without saying the owner. `github_repo_index.rs` resolves a bare repo name across the user's repos and every organisation (10-minute cache): an exact name first, then a prefix of three or more letters, then an edit distance of one or two for names of four or more letters. Several matches become a spoken question. "PR 34" with no repo searches every repo (`FindPr`). `github_cmd.rs` runs the commands; destructive ones (merge, close, revert, collaborator and organisation changes) ask first.

## Verification (what was found)

Baseline: 83 tests pass (resolution tiers, ranking, qualification, PR search, the confirmation prompts).

The advisor's concern was that a **prefix or edit-distance match resolves silently to one repo**, so "merge PR 5 in zinc" could merge in `zync` without the user hearing which. Checked in the code: `execute_command` resolves a bare name to the full `owner/repo` **before** it builds the confirmation prompt, and every destructive prompt prints that resolved name ("Are you sure you want to squash merge PR #5 in Engine-NEXUS/zync?"). So the full name **is** read back whenever a destructive command runs: the concern is answered, but nothing had pinned it.

## What was built

`github_cmd::confirmation_names_the_resolved_repo_tests` (2 tests): after a loose match the prompt for merge, close and revert contains the full `owner/repo`; and in `execute_command` the resolution comes before the prompt is built (a source-order scan). No behaviour changed.

## Not done, and why

* **A spoken confirmation that says what was *heard* as well as what was *understood*** ("I heard zinc; that is Engine-NEXUS/zync") when the match was not exact is not built: `Resolution::One` does not carry which tier matched, and adding it touches seven call sites. Planned, small.
* **Live behaviour against real accounts** (an organisation behind SSO, a name under two owners, a private repo) needs the owner's token. The unauthenticated GitHub API cannot list a user's repos.
* A real-payload contract fixture for the repo list and PR search (as was done for Actions runs in document 3) needs an authenticated call; only public endpoints were used here.

## Owner checklist

1. "Analyse zync" (a repo you own), "analyse the PR in zync" (a list opens in the sidebar), "analyse PR 34" (each repo with an open #34 is listed).
2. A name under two owners: NEXUS asks which.
3. "Merge PR 5 in zinc" (a deliberate mis-hearing): the confirmation must read `owner/zync` in full; say no.
4. An organisation that needs SSO approval: expect a spoken reason, not a crash.
