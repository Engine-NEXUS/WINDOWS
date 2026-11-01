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
    /// Analyse the latest pull request of this repository (the user named it and the screen shows none of it, or asked
    /// for "the latest" and the screen shows exactly one repository).
    Latest { owner: String, repo: String },
    /// Not sure which: put the open pull requests in the sidebar (this repository's, or `None` = all the user's) and ask.
    List { repo: Option<(String, String)>, text: String },
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

fn list_one(owner: &str, repo: &str) -> Decision {
    Decision::List {
        repo: Some((owner.to_string(), repo.to_string())),
        text: format!("Which pull request in {repo} do you mean, sir?"),
    }
}

fn list_all() -> Decision {
    Decision::List { repo: None, text: "I can't tell which pull request you mean, sir. Which one?".to_string() }
}

/// Pure. The screen is read first. `mentioned` = the repository the user named (already matched to a real one), if any.
/// `want_latest` = they said "latest" without a repo.
///   * a pull request of the named repository on the screen -> that one; else the named repository's latest (case 2);
///   * no repo named: exactly one pull request on the screen -> that one;
///     one repository only -> its open pull requests are listed (or its latest, when "latest" was asked);
///     anything else, or nothing -> all the user's open pull requests are listed.
pub fn decide(found: &[Resolved], mentioned: Option<(&str, &str)>, want_latest: bool) -> Decision {
    let found = distinct(found);
    if let Some((mo, mr)) = mentioned {
        return match found.iter().find(|r| r.same_repo(mo, mr) && r.number.is_some()) {
            Some(r) => Decision::Analyse { owner: r.owner.clone(), repo: r.repo.clone(), number: r.number.unwrap_or(0) },
            None => Decision::Latest { owner: mo.to_string(), repo: mr.to_string() },
        };
    }
    let numbered: Vec<&&Resolved> = found.iter().filter(|r| r.number.is_some()).collect();
    let repos = repos_of(&found);
    if want_latest {
        if let [(o, r)] = repos.as_slice() {
            return Decision::Latest { owner: o.to_string(), repo: r.to_string() };
        }
    } else if let [one] = numbered.as_slice() {
        return Decision::Analyse { owner: one.owner.clone(), repo: one.repo.clone(), number: one.number.unwrap_or(0) };
    }
    match repos.as_slice() {
        [(o, r)] => list_one(o, r),
        _ => list_all(),
    }
}

/// What a PR NUMBER (no repository) means. `on_screen` = repositories shown on the screen that really have this PR;
/// `elsewhere` = every repository of the user's with this PR open.
#[derive(Debug, Clone, PartialEq)]
pub enum NumberDecision {
    Analyse { owner: String, repo: String },
    /// The same number is open in several repositories: ask which (the sidebar lists them).
    AskRepo(String),
    NotFound(String),
}

pub fn decide_number(number: u64, on_screen: &[(String, String)], elsewhere: &[(String, String)]) -> NumberDecision {
    let pick = |v: &[(String, String)]| -> Option<NumberDecision> {
        match v {
            [(o, r)] => Some(NumberDecision::Analyse { owner: o.clone(), repo: r.clone() }),
            _ => None,
        }
    };
    // the screen first: one repository shown there has it
    if let Some(d) = pick(on_screen) {
        return d;
    }
    if let Some(d) = pick(elsewhere) {
        return d;
    }
    if elsewhere.is_empty() {
        return NumberDecision::NotFound(format!("There is no open pull request {number} in your repositories, sir."));
    }
    let names: Vec<&str> = elsewhere.iter().take(4).map(|(_, r)| r.as_str()).collect();
    NumberDecision::AskRepo(format!("Pull request {number} is open in {}, sir. Which repository?", names.join(" and ")))
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
        assert_eq!(from_text("acme/zync: Pull requests"), Some(Seen { owner: Some("acme".into()), repo: "zync".into(), number: None }));
        assert_eq!(from_text("Inbox (3) - someone@gmail.com - Gmail"), None);
        assert_eq!(from_text("improve 12 things you can do"), None);
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
        // the same PR seen twice (URL and title) is one
        let twice = [res("a", "zync", Some(1)), res("A", "ZYNC", Some(1))];
        assert!(matches!(decide(&twice, None, false), Decision::Analyse { number: 1, .. }));
    }

    #[test]
    fn a_named_repo_prefers_its_pr_on_screen_then_its_latest() {
        let screen = [res("acme", "zync", Some(5)), res("acme", "other", Some(9))];
        assert_eq!(decide(&screen, Some(("acme", "zync")), false), Decision::Analyse { owner: "acme".into(), repo: "zync".into(), number: 5 });
        // case 2: nothing of that repo on screen -> its latest PR (the other repo's PR is not used)
        assert_eq!(decide(&screen, Some(("acme", "servx")), false), Decision::Latest { owner: "acme".into(), repo: "servx".into() });
        assert_eq!(decide(&[], Some(("acme", "zync")), false), Decision::Latest { owner: "acme".into(), repo: "zync".into() });
        assert!(matches!(decide(&[res("acme", "zync", None)], Some(("acme", "zync")), false), Decision::Latest { .. }));
    }

    #[test]
    fn not_knowing_which_lists_the_prs_of_the_known_repo_or_all_of_them() {
        // several PRs in different repos on screen, two repos with no PR number, or nothing: all open PRs
        let two = [res("a", "zync", Some(1)), res("a", "servx", Some(2))];
        assert!(matches!(decide(&two, None, false), Decision::List { repo: None, .. }));
        assert!(matches!(decide(&[], None, false), Decision::List { repo: None, .. }));
        assert!(matches!(decide(&[res("a", "zync", None), res("a", "servx", None)], None, false), Decision::List { repo: None, .. }));
        // one repo known (no PR number): that repo's open PRs, and the question names it
        let Decision::List { repo, text } = decide(&[res("a", "zync", None)], None, false) else { panic!() };
        assert_eq!(repo, Some(("a".into(), "zync".into())));
        assert!(text.contains("pull request in zync"), "{text}");
        // several PRs in the SAME repo: that repo's list
        let same = [res("a", "zync", Some(1)), res("a", "zync", Some(2))];
        assert!(matches!(decide(&same, None, false), Decision::List { repo: Some(_), .. }));
    }

    #[test]
    fn the_latest_pr_uses_the_repo_on_screen() {
        assert_eq!(decide(&[res("a", "zync", Some(3))], None, true), Decision::Latest { owner: "a".into(), repo: "zync".into() });
        assert!(matches!(decide(&[], None, true), Decision::List { repo: None, .. }));
    }

    fn pair(o: &str, r: &str) -> (String, String) {
        (o.to_string(), r.to_string())
    }

    #[test]
    fn a_pr_number_alone_uses_the_screen_then_a_unique_match_else_asks_which_repo() {
        // the screen's repo has it
        assert_eq!(decide_number(70, &[pair("a", "zync")], &[pair("a", "zync"), pair("a", "servx")]), NumberDecision::Analyse { owner: "a".into(), repo: "zync".into() });
        // nothing on screen, one repo has it
        assert_eq!(decide_number(70, &[], &[pair("a", "servx")]), NumberDecision::Analyse { owner: "a".into(), repo: "servx".into() });
        // the same number in two repos and the screen does not settle it: ask which repo
        let NumberDecision::AskRepo(q) = decide_number(70, &[], &[pair("a", "zync"), pair("a", "servx")]) else { panic!() };
        assert!(q.contains("zync and servx") && q.ends_with("Which repository?"), "{q}");
        // two on screen have it: still ask
        assert!(matches!(decide_number(70, &[pair("a", "zync"), pair("a", "servx")], &[pair("a", "zync"), pair("a", "servx")]), NumberDecision::AskRepo(_)));
        // nobody has it
        assert!(matches!(decide_number(70, &[], &[]), NumberDecision::NotFound(t) if t.contains("no open pull request 70")));
    }
}
