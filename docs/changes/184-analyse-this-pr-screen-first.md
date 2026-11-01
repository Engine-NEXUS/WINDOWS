# 184 - "Analyse this PR": the screen first, then the repo's latest PR, otherwise ask (2026-11-01)

Owner request: with Gmail or the GitHub app open, "analyse this pr" must read the screen. If the repo is named: a PR of that repo on the screen first (case 1), else that repo's latest PR (case 2). If nothing matches, ask which one.

## Built
- `intent_parser::parse_analyse_pr_pointer` (runs before the CodeRabbit shortcuts): "analyse/review/explain/check this|the|that|my|latest pr|pull request [for me|please|on screen]" -> `AnalyseRepo { repo: "this pr" | "latest pr" }`; with a repo ("analyse the latest pr in zync", "analyse this pr in zync", "...of Org/Repo") -> `AnalyseLatestPr`. Numbers, authors and plain "analyse the pr in zync" (kept as the older list reading) are untouched. Fixes "analyse the latest pr of Org/Repo" being read as an AI-review request and "this pr for me" becoming a repo name.
- `pr_screen.rs` (pure): PR/repo from a window title (`[Org/Repo] ... (PR #70)`, `Pull Request #12 · acme/zync`, `acme/zync#9`), from a github.com URL, from a vision answer (`{"prs":[{"repo","number"}]}`), and `decide()`: one PR on screen -> analyse it; several -> "Which one?" naming them; a repo but no number -> its latest PR and says so; two repos -> ask; nothing -> "I can't see a pull request on your screen, sir. Which one do you mean?...". With a named repo: its PR on screen, else its latest, never another repo's PR.
- `orchestrator::resolve_pr_target`: sources in order URL, window titles (foreground and last non-NEXUS), vision read only if no number found; every repo is checked against the user's GitHub (same rule as change 183: a real github.com page may be anyone's public repo, an email/notification name must be in your GitHub); a PR number GitHub reports as 404 is dropped (`repo_review::pr_exists`). The spoken line names what was analysed ("PR #70 in Leddger-AI/LedgerAI", "the latest PR in ...").
- Evidence: a clear pointed PR phrase is Strong (an unenrolled voice is not asked "which repository").

## Verify
Rust lib 2931 pass (new: 1 parser test, 9 pr_screen tests). NOT run live: the Gmail / GitHub app title case, the vision fallback, the GitHub 404 check, the spoken question. Needs a rebuild.
