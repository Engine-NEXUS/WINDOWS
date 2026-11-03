//! Repositories named on screen (an email, a notification, an issue, a chat) rather than shown as a GitHub page.
//! Pure: the prompt for the vision model, the cleaning of its answer, and the split into "owner/repo" or a bare name.
//! The runner in `orchestrator` captures the window, calls the model and resolves bare names against the user's own
//! GitHub (`github_repo_index`), so nothing the model returns is trusted until it has matched a real repository.

use serde_json::Value;

const MAX_NAMES: usize = 3;
const MAX_LEN: usize = 100;

pub fn build_prompt() -> &'static str {
    "You are reading a screenshot. List the GitHub repositories it names, as written, even when there is no link. \
Reply with JSON only: {\"repos\": [\"owner/repo\", \"or just the repository name\"]}. \
Rules: include only names that clearly refer to a code repository (for example in a GitHub notification, an issue, \
a pull request, a README or a message about a project); never invent a name; use [] when the screen names none."
}

/// What a cleaned name is.
#[derive(Debug, Clone, PartialEq)]
pub enum Candidate {
    /// `owner/repo`.
    Full(String, String),
    /// A repository name with no owner.
    Bare(String),
}

fn plain(s: &str) -> bool {
    !s.is_empty() && s.len() <= MAX_LEN && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// One name as the model wrote it -> a clean candidate (URL prefix, `@`, `.git` and trailing slashes removed).
pub fn classify(raw: &str) -> Option<Candidate> {
    let mut s = raw.trim().trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ',' | ';' | ':' | ')' | '(' | '[' | ']')).trim();
    for p in ["https://github.com/", "http://github.com/", "github.com/", "www.github.com/"] {
        if let Some(r) = s.strip_prefix(p) {
            s = r;
            break;
        }
    }
    let s = s.trim_start_matches('@').trim_end_matches('/');
    let s = s.strip_suffix(".git").unwrap_or(s);
    if s.contains('/') {
        let mut parts = s.split('/');
        let (o, r) = (parts.next()?, parts.next()?);
        // A URL may carry more path ("/pull/3"); the first two parts are the repository.
        let full = crate::architect::split_full_name(&format!("{o}/{r}"))?;
        return Some(Candidate::Full(full.0, full.1));
    }
    // A bare name must look like a repository name: at least two characters, not all dots, not only digits.
    if plain(s) && s.len() >= 2 && !s.chars().all(|c| c == '.') && !s.chars().all(|c| c.is_ascii_digit()) {
        Some(Candidate::Bare(s.to_string()))
    } else {
        None
    }
}

/// The model's answer -> up to three distinct usable names, in the order given.
pub fn parse_names(text: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<Value>(crate::vision::strip_json_fences(text)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    if let Some(arr) = v.get("repos").and_then(|r| r.as_array()) {
        for item in arr {
            let Some(raw) = item.as_str() else { continue };
            let name = match classify(raw) {
                Some(Candidate::Full(o, r)) => format!("{o}/{r}"),
                Some(Candidate::Bare(b)) => b,
                None => continue,
            };
            if !out.iter().any(|x| x.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
            if out.len() >= MAX_NAMES {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_splits_full_and_bare_names_and_rejects_junk() {
        assert_eq!(classify("zync-meet/Zync"), Some(Candidate::Full("zync-meet".into(), "Zync".into())));
        assert_eq!(classify("https://github.com/Engine-NEXUS/WINDOWS/pull/39"), Some(Candidate::Full("Engine-NEXUS".into(), "WINDOWS".into())));
        assert_eq!(classify("@owner/repo.git"), Some(Candidate::Full("owner".into(), "repo".into())));
        assert_eq!(classify("`zync`"), Some(Candidate::Bare("zync".into())));
        for bad in ["", "a", "..", "123", "has space", "a/b/../c", "../x", "x;rm -rf /", "http://evil.example/a/b"] {
            assert!(matches!(classify(bad), None | Some(Candidate::Full(..))) && !matches!(classify(bad), Some(Candidate::Bare(_))), "{bad}");
        }
        assert_eq!(classify("http://evil.example/a/b"), None);
        assert_eq!(classify("../x"), None);
        // A trailing slash is only decoration on a bare name.
        assert_eq!(classify("owner/"), Some(Candidate::Bare("owner".into())));
    }

    #[test]
    fn parse_names_keeps_three_distinct_clean_names() {
        let t = "```json\n{\"repos\": [\"zync-meet/Zync\", \"ZYNC-MEET/zync\", \"servx\", \"not a name\", \"https://github.com/a/b\", \"extra\"]}\n```";
        assert_eq!(parse_names(t), vec!["zync-meet/Zync".to_string(), "servx".to_string(), "a/b".to_string()]);
    }

    #[test]
    fn parse_names_survives_bad_answers() {
        assert!(parse_names("I cannot see any repository").is_empty());
        assert!(parse_names("{\"repos\": []}").is_empty());
        assert!(parse_names("{\"repos\": [1, null, {}]}").is_empty());
        assert!(parse_names("{\"other\": [\"a/b\"]}").is_empty());
    }
}
