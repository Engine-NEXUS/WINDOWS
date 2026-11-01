//! "Analyse this PR": which pull request is the user looking at?
//!
//! The screen is the first source (a github.com page, the title of the Gmail message or GitHub window, and as a last
//! resort a vision read of the window). Whatever is found is checked against the user's own GitHub by the caller; this
//! module is the pure part: pulling "owner/repo" and a PR number out of text, the prompt for the vision model, and the
//! decision between "analyse that one", "take the latest of this repo" and "ask which one".

use serde_json::Value;

/// What the screen shows. `owner` is `None` when only a bare repository name was seen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub owner: Option<String>,
    pub repo: String,
    pub number: Option<u64>,
}

/// A `Seen` that the caller has matched to a real repository of the user's (or a real github.com page).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub owner: String,
    pub repo: String,
    pub number: Option<u64>,
}

impl Resolved {
    fn same_repo(&self, owner: &str, repo: &str) -> bool {
        self.owner.eq_ignore_ascii_case(owner) && self.repo.eq_ignore_ascii_case(repo)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Analyse this exact pull request.
    Analyse { owner: String, repo: String, number: u64 },
    /// Analyse the latest pull request of this repository; `notice` is said first when that was a fallback.
    Latest { owner: String, repo: String, notice: Option<String> },
    /// Say this and wait for the user.
    Ask(String),
}

// ─── text -> what it shows ───────────────────────────────────────────

fn slug(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')
}

/// `[owner/repo]` (GitHub's e-mail subjects) or the first plain `owner/repo` token that is not part of a URL.
fn repo_in(text: &str) -> Option<(String, String)> {
    if let (Some(a), Some(b)) = (text.find('['), text.find(']')) {
        if a < b {
            if let Some(full) = crate::architect::split_full_name(text[a + 1..b].trim()) {
                return Some(full);
            }
        }
    }
    for w in text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | '"' | '\'')) {
        if w.contains("://") || w.contains("github.com") || w.contains('@') {
            continue;
        }
        let w = w.trim_end_matches(|c: char| !slug(c));
        // "owner/repo#70" -> owner/repo
        let w = w.split('#').next().unwrap_or(w);
        if let Some((o, r)) = w.split_once('/') {
            if !o.is_empty() && !r.is_empty() && o.chars().all(slug) && r.chars().all(slug) && !r.contains('/') {
                if let Some(full) = crate::architect::split_full_name(&format!("{o}/{r}")) {
                    return Some(full);
                }
            }
        }
    }
    None
}

/// "PR #70", "Pull request #70", "owner/repo#70", "/pull/70", or a bare "#70" next to a repository.
fn number_in(text: &str, have_repo: bool) -> Option<u64> {
    let lower = text.to_lowercase();
    let b = lower.as_bytes();
    let digits_after = |mut i: usize| -> Option<u64> {
        while i < b.len() && (b[i] == b' ' || b[i] == b'#') {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        (i > start && i - start <= 6).then(|| lower[start..i].parse().ok()).flatten().filter(|n| *n > 0)
    };
    for key in ["pull request", "pull/", "pr #", "pr ", "pr#", "pr-"] {
        let mut from = 0;
        while let Some(p) = lower[from..].find(key) {
            let at = from + p;
            // "pr" must be a whole word: not the end of "improve" or the start of "pro"
            let word_ok = !key.starts_with("pr") || at == 0 || !b[at - 1].is_ascii_alphanumeric();
            if word_ok {
                if let Some(n) = digits_after(at + key.len()) {
                    return Some(n);
                }
            }
            from = at + key.len();
        }
    }
    // "owner/repo#70" or a lone "#70" beside a repository name
    if let Some(p) = lower.find('#') {
        if have_repo || lower[..p].ends_with(|c: char| slug(c)) {
            if let Some(n) = digits_after(p) {
                return Some(n);
            }
        }
    }
    None
}

/// What one line of text (a window title, a Gmail subject) says about a pull request. `None` = nothing about one.
pub fn from_text(text: &str) -> Option<Seen> {
    let repo = repo_in(text);
    let number = number_in(text, repo.is_some());
    match (repo, number) {
        (Some((owner, repo)), number) => Some(Seen { owner: Some(owner), repo, number }),
        (None, _) => None,
    }
}

/// A github.com URL: a pull request page, or a repository page (no number).
pub fn from_url(url: &str) -> Option<Seen> {
    if let Some((owner, repo, n)) = crate::orchestrator::github_pr_from_url(url) {
        return Some(Seen { owner: Some(owner), repo, number: Some(n) });
    }
    crate::architect::extract_github_repo_from_url(url).map(|id| Seen { owner: Some(id.owner), repo: id.repo, number: None })
}

// ─── the vision read ─────────────────────────────────────────────────

pub fn build_prompt() -> &'static str {
    "You are reading a screenshot. List the GitHub pull requests it shows, as written. \
Reply with JSON only: {\"prs\": [{\"repo\": \"owner/repo or just the repository name\", \"number\": 70}]}. \
Rules: only pull requests that are clearly visible (a GitHub page, a GitHub notification or e-mail, a chat message about \
a specific PR); \"number\" is the PR number or null when it is not shown; never invent a repo or a number; use [] when none."
}

/// The model's answer -> up to four distinct sightings.
pub fn parse_prs(text: &str) -> Vec<Seen> {
    let Ok(v) = serde_json::from_str::<Value>(crate::vision::strip_json_fences(text)) else { return Vec::new() };
    let mut out: Vec<Seen> = Vec::new();
    for item in v.get("prs").and_then(|p| p.as_array()).into_iter().flatten() {
        let Some(raw) = item.get("repo").and_then(|r| r.as_str()) else { continue };
        let number = item.get("number").and_then(|n| n.as_u64()).filter(|n| *n > 0 && *n < 1_000_000);
        let seen = match crate::repo_hint::classify(raw) {
            Some(crate::repo_hint::Candidate::Full(o, r)) => Seen { owner: Some(o), repo: r, number },
            Some(crate::repo_hint::Candidate::Bare(b)) => Seen { owner: None, repo: b, number },
            None => continue,
        };
        if !out.contains(&seen) {
            out.push(seen);
        }
        if out.len() >= 4 {
            break;
        }
    }
    out
}

// ─── the decision ────────────────────────────────────────────────────

fn distinct(found: &[Resolved]) -> Vec<&Resolved> {
    let mut out: Vec<&Resolved> = Vec::new();
    for r in found {
        if !out.iter().any(|o| o.same_repo(&r.owner, &r.repo) && o.number == r.number) {
            out.push(r);
        }
    }
    out
}

fn repos_of<'a>(found: &'a [&'a Resolved]) -> Vec<(&'a str, &'a str)> {
    let mut out: Vec<(&str, &str)> = Vec::new();
    for r in found {
        if !out.iter().any(|(o, n)| r.same_repo(o, n)) {
            out.push((r.owner.as_str(), r.repo.as_str()));
        }
    }
    out
}

/// Pure. `mentioned` = the repository the user named (already matched to a real one), if any. `want_latest` = they said
/// "latest" without a repo.
pub fn decide(found: &[Resolved], mentioned: Option<(&str, &str)>, want_latest: bool) -> Decision {
    let found = distinct(found);
    // The user named a repository: a pull request of THAT repository on the screen wins, otherwise its latest one.
    if let Some((mo, mr)) = mentioned {
        let on_screen = found.iter().find(|r| r.same_repo(mo, mr) && r.number.is_some());
        return match on_screen {
            Some(r) => Decision::Analyse { owner: r.owner.clone(), repo: r.repo.clone(), number: r.number.unwrap_or(0) },
            None => Decision::Latest { owner: mo.to_string(), repo: mr.to_string(), notice: None },
        };
    }
    let numbered: Vec<&&Resolved> = found.iter().filter(|r| r.number.is_some()).collect();
    let repos = repos_of(&found);
    if want_latest {
        // "the latest PR": the repository on the screen decides which repository
        if let [(o, r)] = repos.as_slice() {
            return Decision::Latest { owner: o.to_string(), repo: r.to_string(), notice: None };
        }
    } else if let [one] = numbered.as_slice() {
        return Decision::Analyse { owner: one.owner.clone(), repo: one.repo.clone(), number: one.number.unwrap_or(0) };
    }
    if numbered.len() > 1 && !want_latest {
        let list: Vec<String> = numbered.iter().take(4).map(|r| format!("number {} in {}", r.number.unwrap_or(0), r.repo)).collect();
        return Decision::Ask(format!("I can see more than one pull request, sir: {}. Which one?", list.join(", and ")));
    }
    match repos.as_slice() {
        [(o, r)] => Decision::Latest {
            owner: o.to_string(),
            repo: r.to_string(),
            notice: Some(format!("I can't see a specific pull request on the screen, so I'm taking the latest one in {r}, sir.")),
        },
        [] => Decision::Ask(
            "I can't see a pull request on your screen, sir. Which one do you mean? Tell me the repository, and the number if you have it."
                .to_string(),
        ),
        many => {
            let names: Vec<&str> = many.iter().take(3).map(|(_, r)| *r).collect();
            Decision::Ask(format!("I can see more than one repository, sir: {}. Which pull request do you mean?", names.join(" and ")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(o: &str, r: &str, n: Option<u64>) -> Resolved {
        Resolved { owner: o.into(), repo: r.into(), number: n }
    }

    #[test]
    fn a_github_email_subject_gives_the_repo_and_the_number() {
        let t = "Gmail - Re: [Leddger-AI/LedgerAI] Align email campaigns and mobile landing for release (PR #70) - official.lakshya.chitkul@gmail.com - Gmail";
        assert_eq!(from_text(t), Some(Seen { owner: Some("Leddger-AI".into()), repo: "LedgerAI".into(), number: Some(70) }));
    }

    #[test]
    fn other_ways_a_title_names_a_pull_request() {
        let n = |t: &str| from_text(t).and_then(|s| s.number);
        assert_eq!(n("Pull Request #12 · acme/zync"), Some(12));
        assert_eq!(n("Fix login by prem · Pull Request #345 · acme/zync · GitHub"), Some(345));
        assert_eq!(n("acme/zync#9 Add retries"), Some(9));
        assert_eq!(n("GitHub Desktop - acme/zync - PR 21"), Some(21));
        // a repo without a number is still a sighting (a repo page or a list of PRs)
        assert_eq!(from_text("acme/zync: Pull requests"), Some(Seen { owner: Some("acme".into()), repo: "zync".into(), number: None }));
        // nothing about a repository or pull request
        assert_eq!(from_text("Inbox (3) - someone@gmail.com - Gmail"), None);
        assert_eq!(from_text("improve 12 things you can do"), None);
        // "pr" inside a word is not a pull request
        assert_eq!(from_text("acme/zync improve 12 things").and_then(|s| s.number), None);
    }

    #[test]
    fn urls_give_a_pull_request_or_just_the_repo() {
        assert_eq!(from_url("https://github.com/acme/zync/pull/34/files").and_then(|s| s.number), Some(34));
        let page = from_url("https://github.com/acme/zync/pulls").unwrap();
        assert_eq!((page.repo.as_str(), page.number), ("zync", None));
        assert_eq!(from_url("https://mail.google.com/mail/u/0/#inbox"), None);
    }

    #[test]
    fn the_vision_answer_is_cleaned() {
        let got = parse_prs("```json\n{\"prs\":[{\"repo\":\"acme/zync\",\"number\":7},{\"repo\":\"zync\",\"number\":null},{\"repo\":\"\",\"number\":3},{\"repo\":\"acme/zync\",\"number\":7}]}\n```");
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0], Seen { owner: Some("acme".into()), repo: "zync".into(), number: Some(7) });
        assert_eq!(got[1], Seen { owner: None, repo: "zync".into(), number: None });
        assert!(parse_prs("not json").is_empty());
    }

    #[test]
    fn this_pr_with_one_pr_on_screen_analyses_it() {
        let d = decide(&[res("Leddger-AI", "LedgerAI", Some(70))], None, false);
        assert_eq!(d, Decision::Analyse { owner: "Leddger-AI".into(), repo: "LedgerAI".into(), number: 70 });
    }

    #[test]
    fn a_named_repo_prefers_its_pr_on_screen_then_its_latest() {
        let screen = [res("acme", "zync", Some(5)), res("acme", "other", Some(9))];
        // case 1: the screen shows a PR of the named repo
        assert_eq!(decide(&screen, Some(("acme", "zync")), false), Decision::Analyse { owner: "acme".into(), repo: "zync".into(), number: 5 });
        // case 2: nothing of that repo on screen -> its latest PR (the other repo's PR is not used)
        assert_eq!(decide(&screen, Some(("acme", "servx")), false), Decision::Latest { owner: "acme".into(), repo: "servx".into(), notice: None });
        assert_eq!(decide(&[], Some(("acme", "zync")), false), Decision::Latest { owner: "acme".into(), repo: "zync".into(), notice: None });
        // only the repo (no number) on screen is not "a PR of that repo": latest
        assert!(matches!(decide(&[res("acme", "zync", None)], Some(("acme", "zync")), false), Decision::Latest { .. }));
    }

    #[test]
    fn several_pull_requests_are_a_question_and_so_is_an_empty_screen() {
        let two = [res("a", "zync", Some(1)), res("a", "servx", Some(2))];
        let Decision::Ask(q) = decide(&two, None, false) else { panic!() };
        assert!(q.contains("number 1 in zync") && q.contains("number 2 in servx") && q.ends_with("Which one?"), "{q}");
        let Decision::Ask(q) = decide(&[], None, false) else { panic!() };
        assert!(q.contains("can't see a pull request") && q.contains("Which one"), "{q}");
        // the same PR seen twice (URL and title) is one
        let twice = [res("a", "zync", Some(1)), res("A", "ZYNC", Some(1))];
        assert!(matches!(decide(&twice, None, false), Decision::Analyse { number: 1, .. }));
    }

    #[test]
    fn a_repo_without_a_pr_number_gives_its_latest_and_says_why() {
        let Decision::Latest { repo, notice, .. } = decide(&[res("a", "zync", None)], None, false) else { panic!() };
        assert_eq!(repo, "zync");
        assert!(notice.unwrap().contains("taking the latest one in zync"));
        // two repos and no PR: ask
        assert!(matches!(decide(&[res("a", "zync", None), res("a", "servx", None)], None, false), Decision::Ask(_)));
    }

    #[test]
    fn the_latest_pr_uses_the_repo_on_screen() {
        let d = decide(&[res("a", "zync", Some(3))], None, true);
        assert_eq!(d, Decision::Latest { owner: "a".into(), repo: "zync".into(), notice: None });
        assert!(matches!(decide(&[], None, true), Decision::Ask(_)));
    }
}
