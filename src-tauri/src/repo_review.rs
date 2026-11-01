//! A repository review that is written from what was actually READ from the repository.
//!
//! 2026-11-01: "analyse this repository" was answered by the general chat model from the repo's NAME alone (nothing was
//! fetched), so the review sounded right and was not based on the user's GitHub. This module fetches the real facts
//! (description, languages, top-level files, README, recent commits, open PR count) with the user's own login and lets
//! the model write only from that evidence. If the facts cannot be read, the caller says so; nothing is guessed.

use serde_json::Value;
use std::time::Duration;

const README_CHARS: usize = 4_000;
const COMMITS: usize = 10;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub full_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub default_branch: String,
    pub pushed_at: Option<String>,
    pub stars: u64,
    pub open_issues_and_prs: u64,
    pub open_prs: Option<usize>,
    /// (language, bytes), largest first
    pub languages: Vec<(String, u64)>,
    /// top-level entries, directories end with `/`
    pub files: Vec<String>,
    pub readme: Option<String>,
    /// (date, first line of the message)
    pub commits: Vec<(String, String)>,
}

// ─── parsing (pure) ──────────────────────────────────────────────────

pub fn parse_repo(v: &Value, facts: &mut Facts) {
    facts.full_name = v["full_name"].as_str().unwrap_or("").to_string();
    facts.description = v["description"].as_str().filter(|d| !d.trim().is_empty()).map(String::from);
    facts.private = v["private"].as_bool().unwrap_or(false);
    facts.default_branch = v["default_branch"].as_str().unwrap_or("").to_string();
    facts.pushed_at = v["pushed_at"].as_str().map(String::from);
    facts.stars = v["stargazers_count"].as_u64().unwrap_or(0);
    facts.open_issues_and_prs = v["open_issues_count"].as_u64().unwrap_or(0);
}

pub fn parse_languages(v: &Value) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = v.as_object().map(|o| o.iter().filter_map(|(k, b)| b.as_u64().map(|b| (k.clone(), b))).collect()).unwrap_or_default();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

pub fn parse_contents(v: &Value) -> Vec<String> {
    let mut out: Vec<String> = v
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|e| {
                    let name = e["name"].as_str()?;
                    Some(if e["type"].as_str() == Some("dir") { format!("{name}/") } else { name.to_string() })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub fn parse_commits(v: &Value) -> Vec<(String, String)> {
    v.as_array()
        .map(|a| {
            a.iter()
                .take(COMMITS)
                .filter_map(|c| {
                    let msg = c["commit"]["message"].as_str()?.lines().next().unwrap_or("").trim().to_string();
                    let date = c["commit"]["author"]["date"].as_str().unwrap_or("").get(..10).unwrap_or("").to_string();
                    (!msg.is_empty()).then_some((date, msg))
                })
                .collect()
        })
        .unwrap_or_default()
}

// ─── what the model is given (pure) ──────────────────────────────────

/// The evidence block. Everything the review may say comes from here.
pub fn evidence(f: &Facts) -> String {
    let mut s = format!("Repository: {}\n", f.full_name);
    s.push_str(&format!("Visibility: {}\n", if f.private { "private" } else { "public" }));
    match &f.description {
        Some(d) => s.push_str(&format!("Description: {d}\n")),
        None => s.push_str("Description: (none set)\n"),
    }
    if !f.languages.is_empty() {
        let total: u64 = f.languages.iter().map(|l| l.1).sum::<u64>().max(1);
        let parts: Vec<String> = f.languages.iter().take(6).map(|(n, b)| format!("{n} {}%", b * 100 / total)).collect();
        s.push_str(&format!("Languages: {}\n", parts.join(", ")));
    }
    if !f.files.is_empty() {
        s.push_str(&format!("Top-level entries ({}): {}\n", f.files.len(), f.files.iter().take(40).cloned().collect::<Vec<_>>().join(", ")));
    }
    s.push_str(&format!("Default branch: {}\n", f.default_branch));
    if let Some(p) = &f.pushed_at {
        s.push_str(&format!("Last push: {p}\n"));
    }
    s.push_str(&format!("Stars: {}\n", f.stars));
    match f.open_prs {
        Some(n) => s.push_str(&format!("Open pull requests: {n}\n")),
        None => s.push_str("Open pull requests: (not read)\n"),
    }
    s.push_str(&format!("Open issues plus PRs (GitHub counter): {}\n", f.open_issues_and_prs));
    if !f.commits.is_empty() {
        s.push_str("Recent commits (newest first):\n");
        for (d, m) in &f.commits {
            s.push_str(&format!("- {d} {m}\n"));
        }
    }
    match &f.readme {
        Some(r) => s.push_str(&format!("README (first {} characters):\n{}\n", README_CHARS, r)),
        None => s.push_str("README: (none found)\n"),
    }
    s
}

pub const SYSTEM: &str = "You review a GitHub repository for its owner. Use ONLY the evidence block you are given. \
Never use outside knowledge of this project or guess from its name. If something is not in the evidence, say it is not shown. \
Write 120 to 200 words of markdown with short headings: What it is, Stack, Recent activity, Worth a look (only items the \
evidence supports, such as an open PR count, a missing README or description). Do not invent file names, features or numbers.";

/// The line under the review that says where it came from.
pub fn provenance(f: &Facts) -> String {
    let last = f.commits.first().map(|c| c.0.as_str()).filter(|d| !d.is_empty()).unwrap_or("unknown");
    format!(
        "*Read from github.com/{}: {} top-level entries, README {}, {} recent commits (latest {}).*",
        f.full_name,
        f.files.len(),
        if f.readme.is_some() { "read" } else { "not found" },
        f.commits.len(),
        last
    )
}

/// Nothing usable was read: the model is not asked at all.
pub fn has_enough(f: &Facts) -> bool {
    !f.full_name.is_empty() && (f.readme.is_some() || !f.files.is_empty() || !f.commits.is_empty())
}

// ─── fetching ────────────────────────────────────────────────────────

async fn get_json(client: &reqwest::Client, token: &Option<String>, url: &str) -> Result<Value, String> {
    let mut req = client.get(url).header("User-Agent", "nexus-assistant").header("Accept", "application/vnd.github+json");
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await.map_err(|e| format!("network: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(match status.as_u16() {
            401 | 403 => "GitHub refused the request (sign-in or rate limit)".to_string(),
            404 => "GitHub says that repository does not exist or is not visible to this login".to_string(),
            c => format!("GitHub answered {c}"),
        });
    }
    resp.json::<Value>().await.map_err(|e| format!("reply: {e}"))
}

pub async fn fetch(owner: &str, repo: &str) -> Result<Facts, String> {
    let token = crate::github_data::resolve_token(None, None).await.ok();
    let client = reqwest::Client::builder().timeout(Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    let base = format!("https://api.github.com/repos/{owner}/{repo}");
    let meta = get_json(&client, &token, &base).await?;
    let mut facts = Facts::default();
    parse_repo(&meta, &mut facts);

    let readme_req = {
        let mut r = client.get(format!("{base}/readme")).header("User-Agent", "nexus-assistant").header("Accept", "application/vnd.github.raw+json");
        if let Some(t) = &token {
            r = r.bearer_auth(t);
        }
        r.send()
    };
    let (u_langs, u_contents, u_commits, u_pulls) = (
        format!("{base}/languages"),
        format!("{base}/contents"),
        format!("{base}/commits?per_page={COMMITS}"),
        format!("{base}/pulls?state=open&per_page=100"),
    );
    let (langs, contents, commits, pulls, readme) = tokio::join!(
        get_json(&client, &token, &u_langs),
        get_json(&client, &token, &u_contents),
        get_json(&client, &token, &u_commits),
        get_json(&client, &token, &u_pulls),
        readme_req,
    );
    facts.languages = langs.map(|v| parse_languages(&v)).unwrap_or_default();
    facts.files = contents.map(|v| parse_contents(&v)).unwrap_or_default();
    facts.commits = commits.map(|v| parse_commits(&v)).unwrap_or_default();
    facts.open_prs = pulls.ok().and_then(|v| v.as_array().map(|a| a.len()));
    if let Ok(r) = readme {
        if r.status().is_success() {
            if let Ok(text) = r.text().await {
                let t: String = text.chars().take(README_CHARS).collect();
                if !t.trim().is_empty() {
                    facts.readme = Some(t);
                }
            }
        }
    }
    Ok(facts)
}

/// The review as markdown, or the plain reason it could not be written. Never a guess.
pub async fn grounded_review<R: tauri::Runtime>(app: &tauri::AppHandle<R>, owner: &str, repo: &str) -> Result<String, String> {
    let facts = fetch(owner, repo).await.map_err(|e| format!("I couldn't read {owner}/{repo} from GitHub, sir: {e}."))?;
    if !has_enough(&facts) {
        return Err(format!("I could reach {owner}/{repo} but found nothing to read in it, sir, so I won't describe it."));
    }
    let keys = crate::router::read_provider_keys(app);
    let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build().map_err(|e| e.to_string())?;
    let user = format!("Evidence:\n{}", evidence(&facts));
    let reply = crate::router::route_with_system(SYSTEM, &user, 700, &keys, &client)
        .await
        .ok_or_else(|| format!("I read {owner}/{repo}, sir, but none of my writing models answered, so I have no review to give."))?;
    let text = reply.text.trim();
    if text.is_empty() {
        return Err(format!("I read {owner}/{repo}, sir, but the model returned nothing."));
    }
    Ok(format!("## {}\n\n{}\n\n---\n{}", facts.full_name, text, provenance(&facts)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Facts {
        let mut f = Facts::default();
        parse_repo(
            &json!({"full_name":"Leddger-AI/LedgerAI","description":"Accounting API","private":true,"default_branch":"main","pushed_at":"2026-10-30T10:00:00Z","stargazers_count":2,"open_issues_count":5}),
            &mut f,
        );
        f.languages = parse_languages(&json!({"Python":9000,"HTML":1000}));
        f.files = parse_contents(&json!([{"name":"app","type":"dir"},{"name":"README.md","type":"file"},{"name":"requirements.txt","type":"file"}]));
        f.commits = parse_commits(&json!([{"commit":{"message":"Add invoices\n\nbody","author":{"date":"2026-10-30T09:00:00Z"}}}]));
        f.readme = Some("# LedgerAI\nAccounts API".into());
        f.open_prs = Some(3);
        f
    }

    #[test]
    fn the_evidence_contains_only_what_was_read() {
        let e = evidence(&sample());
        for must in ["Leddger-AI/LedgerAI", "Accounting API", "Python 90%", "HTML 10%", "README.md, app/, requirements.txt", "2026-10-30 Add invoices", "Open pull requests: 3", "# LedgerAI"] {
            assert!(e.contains(must), "missing {must}: {e}");
        }
        assert!(!e.contains("body"), "only the first line of a commit message");
    }

    #[test]
    fn missing_parts_are_stated_as_missing_not_filled_in() {
        let mut f = sample();
        f.readme = None;
        f.description = None;
        f.open_prs = None;
        let e = evidence(&f);
        assert!(e.contains("README: (none found)") && e.contains("Description: (none set)") && e.contains("Open pull requests: (not read)"), "{e}");
    }

    #[test]
    fn the_provenance_line_says_what_was_read() {
        let p = provenance(&sample());
        assert!(p.contains("github.com/Leddger-AI/LedgerAI") && p.contains("3 top-level entries") && p.contains("README read") && p.contains("latest 2026-10-30"), "{p}");
    }

    #[test]
    fn an_empty_repository_is_not_described() {
        let mut f = Facts::default();
        parse_repo(&json!({"full_name":"a/b"}), &mut f);
        assert!(!has_enough(&f), "no files, README or commits: nothing to write from");
        assert!(has_enough(&sample()));
        assert!(!has_enough(&Facts::default()));
    }

    #[test]
    fn the_prompt_forbids_guessing() {
        assert!(SYSTEM.contains("ONLY the evidence") && SYSTEM.contains("Never use outside knowledge") && SYSTEM.contains("not shown"));
    }

    #[test]
    fn parsers_survive_garbage() {
        assert!(parse_languages(&json!("x")).is_empty());
        assert!(parse_contents(&json!({"message":"Not Found"})).is_empty());
        assert!(parse_commits(&json!(null)).is_empty());
    }
}
