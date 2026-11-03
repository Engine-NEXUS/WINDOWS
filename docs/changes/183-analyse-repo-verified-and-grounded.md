# 183 - "Analyse this repository": the repo is checked against your GitHub and the review is read, not guessed (2026-11-01)

Live log 2026-11-01 21:31: with a Gmail message open ("[Leddger-AI/LedgerAI] Align email campaigns ... (PR #70)"), "analyse this repository" produced a review that used the name but was not based on the user's GitHub.

## Causes (from the log and code)
1. `architect::get_active_repo_url` falls back to the window title; the email subject's `owner/repo` token was accepted and used without checking it against the signed-in GitHub account.
2. After the rewrite to `analyse Leddger-AI/LedgerAI`, `router::can_route` (phrase list only) allowed the fast chat path, so Cerebras/Groq/Gemini described the repo from its name; nothing was fetched.
3. The map window asked the cloud Worker for a token ("device isn't linked") although a local login exists. NOT changed (plan item C, cosmetic: the deep scan clones the repo and works).

## Built
- `github_repo_index::verify_repo_identity` / `Identity` / `identity_notice` / `entries_for_check` (live list, else the cached 92-repo list; works with no token). Verified, Corrected (misspelling fixed and said aloud), Ambiguous (the existing "Which one?"), NotFound, Unverifiable.
- `orchestrator::pointed_decision`: a real github.com page may be anyone's public repo; a name from a title/email must be in your GitHub, otherwise "I see X on the screen, sir, but it isn't in your GitHub, so I won't guess about it." Names read off the screen by vision are checked the same way (`resolve_screen_repos`).
- Analysis intents never use the chat fast path (`is_repo_analysis_label`), and `can_route` refuses any "analyse/review owner/repo" sentence.
- `repo_review.rs`: reads description, languages, top-level files, README (4 KB), 10 commits, open PR count with your own login (vault, then `gh auth token`), then the model writes only from that evidence and a provenance line is appended. If the repo cannot be read here the Worker is tried; if that fails the user hears the real reason.
- The spoken line now names the repo that was analysed ("Here is the analysis for Leddger-AI/LedgerAI, sir.", it said "for this").

## Verify
Rust: github_repo_index 16, router + orchestrator + repo_review groups 149 pass; a mutation (accept title names) is killed. Probe: the deterministic parser handles the analyse-this-repo variants. NOT run live: the real GitHub fetch, the model write-up, the Gmail scenario, offline behaviour, a rebuild (not done yet).
