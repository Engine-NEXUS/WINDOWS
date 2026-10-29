//! NEXUS — GitHub Repo Index and Owner-less Resolution (Feature 161)
//!
//! Provides fast in-memory resolution of bare repository names to full
//! `owner/repo` identifiers. Normalises sound aliases (zinc -> zync),
//! matches by prefix and edit distance, resolves ambiguity across owners,
//! and maintains a cached index of user and organization repositories.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached repository entry.
#[derive(Clone, Debug, PartialEq)]
pub struct RepoEntry {
    pub full_name: String,
    pub owner: String,
    pub name: String,
    pub owner_type: String,
    pub open_issues: u32,
}

/// Result of resolving a repository name or query.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    One(RepoEntry),
    Many(Vec<RepoEntry>),
    None,
}

/// Cache TTL: 10 minutes.
const CACHE_TTL: Duration = Duration::from_secs(600);

/// In-memory cache of repositories with timestamp.
static CACHE: Mutex<Option<(Instant, Vec<RepoEntry>)>> = Mutex::new(None);

/// Clean and compact a string by removing `-`, `_`, `.`, and spaces, and lowercasing.
fn clean_token(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && *c != '_' && *c != '.' && !c.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

/// Pure resolution against an explicit slice of repository entries.
pub fn resolve_in(entries: &[RepoEntry], query: &str) -> Resolution {
    let trimmed = query.trim();
    if trimmed.is_empty() || entries.is_empty() {
        return Resolution::None;
    }

    let canon = crate::intent_parser::canonical_repo_name(trimmed);

    // If the query contains '/', match full_name exactly (case-insensitive) -> One/None.
    if trimmed.contains('/') || canon.contains('/') {
        let matches: Vec<RepoEntry> = entries
            .iter()
            .filter(|e| {
                e.full_name.eq_ignore_ascii_case(&canon)
                    || e.full_name.eq_ignore_ascii_case(trimmed)
            })
            .cloned()
            .collect();

        if matches.is_empty() {
            return Resolution::None;
        }
        return rank_and_resolve(matches);
    }

    let q_clean = clean_token(&canon);
    if q_clean.is_empty() {
        return Resolution::None;
    }

    // Tier 1: Exact name match (ignoring `-`, `_`, `.`, and spaces)
    let tier1: Vec<RepoEntry> = entries
        .iter()
        .filter(|e| clean_token(&e.name) == q_clean)
        .cloned()
        .collect();
    if !tier1.is_empty() {
        return rank_and_resolve(tier1);
    }

    // Tier 2: Name starts with query (query length >= 3)
    if q_clean.len() >= 3 {
        let tier2: Vec<RepoEntry> = entries
            .iter()
            .filter(|e| clean_token(&e.name).starts_with(&q_clean))
            .cloned()
            .collect();
        if !tier2.is_empty() {
            return rank_and_resolve(tier2);
        }
    }

    // Tier 3: Edit distance (reuse levenshtein). Never substring-match short queries.
    // Short queries (< 4 chars) are not fuzzy-matched to prevent noisy collisions like "ync" -> "zync".
    if q_clean.len() >= 4 {
        let mut tier3 = Vec::new();
        for e in entries {
            let name_clean = clean_token(&e.name);
            if name_clean.len() < 4 {
                continue;
            }
            let max_dist = if name_clean.len() <= 5 || q_clean.len() <= 5 {
                1
            } else {
                2
            };
            let dist = crate::intent_parser::levenshtein(&q_clean, &name_clean);
            if dist <= max_dist {
                tier3.push(e.clone());
            }
        }
        if !tier3.is_empty() {
            return rank_and_resolve(tier3);
        }
    }

    Resolution::None
}

/// Deduplicate matches by `full_name`, rank them, and return `One` or `Many`.
fn rank_and_resolve(matches: Vec<RepoEntry>) -> Resolution {
    let mut deduped: Vec<RepoEntry> = Vec::new();
    for e in matches {
        if !deduped
            .iter()
            .any(|d| d.full_name.eq_ignore_ascii_case(&e.full_name))
        {
            deduped.push(e);
        }
    }

    if deduped.is_empty() {
        return Resolution::None;
    }
    if deduped.len() == 1 {
        return Resolution::One(deduped.remove(0));
    }

    // Sort: repos the user owns come first, then by open_issues descending.
    deduped.sort_by(|a, b| {
        let a_is_user = a.owner_type.eq_ignore_ascii_case("user")
            || a.owner_type.eq_ignore_ascii_case("owner");
        let b_is_user = b.owner_type.eq_ignore_ascii_case("user")
            || b.owner_type.eq_ignore_ascii_case("owner");
        match (a_is_user, b_is_user) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => b
                .open_issues
                .cmp(&a.open_issues)
                .then_with(|| a.full_name.cmp(&b.full_name)),
        }
    });

    Resolution::Many(deduped)
}

/// Synchronous peek into the in-memory cache, falling back to disk cache if available.
pub fn peek(query: &str) -> Option<Resolution> {
    {
        let guard = CACHE.lock().ok()?;
        if let Some((_, ref entries)) = *guard {
            return Some(resolve_in(entries, query));
        }
    }

    // Disk fallback if app data directory exists without AppHandle
    if let Some(dir) = crate::app_dirs::nexus_data_dir() {
        let cache_file = dir.join("github_dashboard_cache.json");
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(repos) = data.get("repos").and_then(|v| v.as_array()) {
                    let mut entries = Vec::new();
                    for r in repos {
                        let full_name = r
                            .get("full_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = r
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let owner = r
                            .get("owner_login")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let owner_type = r
                            .get("owner_type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("User")
                            .to_string();
                        let open_issues = r
                            .get("open_issues_count")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;

                        if !full_name.is_empty() && !name.is_empty() {
                            entries.push(RepoEntry {
                                full_name,
                                owner,
                                name,
                                owner_type,
                                open_issues,
                            });
                        }
                    }
                    if !entries.is_empty() {
                        let res = resolve_in(&entries, query);
                        if let Ok(mut guard) = CACHE.lock() {
                            *guard = Some((Instant::now(), entries));
                        }
                        return Some(res);
                    }
                }
            }
        }
    }

    None
}

/// Currently cached repository names for fuzzy matching.
pub fn cached_names() -> Vec<String> {
    if let Ok(guard) = CACHE.lock() {
        if let Some((_, ref entries)) = *guard {
            let mut names: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
            names.sort();
            names.dedup();
            return names;
        }
    }

    if let Some(dir) = crate::app_dirs::nexus_data_dir() {
        let cache_file = dir.join("github_dashboard_cache.json");
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(repos) = data.get("repos").and_then(|v| v.as_array()) {
                    let mut names = Vec::new();
                    for r in repos {
                        if let Some(name) = r.get("name").and_then(|v| v.as_str()) {
                            if !name.is_empty() {
                                names.push(name.to_string());
                            }
                        }
                    }
                    names.sort();
                    names.dedup();
                    return names;
                }
            }
        }
    }

    Vec::new()
}

/// Fetch user and organization repositories over the GitHub API.
async fn refresh_cache() -> Result<Vec<RepoEntry>, String> {
    let token = crate::github_data::resolve_token(None, None).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let mut all_repos = Vec::new();

    // 1. Fetch user repos (pages 1..=3)
    for page in 1..=3 {
        let url = format!(
            "https://api.github.com/user/repos?per_page=100&page={}&affiliation=owner,collaborator,organization_member",
            page
        );
        let resp = client
            .get(&url)
            .header("User-Agent", "nexus-assistant")
            .header("Accept", "application/vnd.github+json")
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| format!("fetch user repos: {e}"))?;

        if !resp.status().is_success() {
            break;
        }

        let items: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
        let count = items.len();
        for r in items {
            if let (Some(full_name), Some(name)) = (
                r.get("full_name").and_then(|v| v.as_str()),
                r.get("name").and_then(|v| v.as_str()),
            ) {
                let owner = r
                    .get("owner")
                    .and_then(|o| o.get("login"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let owner_type = r
                    .get("owner")
                    .and_then(|o| o.get("type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("User")
                    .to_string();
                let open_issues = r
                    .get("open_issues_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                if !name.is_empty() && !full_name.is_empty() {
                    all_repos.push(RepoEntry {
                        full_name: full_name.to_string(),
                        owner,
                        name: name.to_string(),
                        owner_type,
                        open_issues,
                    });
                }
            }
        }
        if count < 100 {
            break;
        }
    }

    // 2. Fetch user orgs
    let orgs_resp = client
        .get("https://api.github.com/user/orgs?per_page=100")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(&token)
        .send()
        .await;

    let mut org_logins = Vec::new();
    if let Ok(resp) = orgs_resp {
        if resp.status().is_success() {
            if let Ok(orgs) = resp.json::<Vec<serde_json::Value>>().await {
                for o in orgs {
                    if let Some(login) = o.get("login").and_then(|v| v.as_str()) {
                        if !login.is_empty() {
                            org_logins.push(login.to_string());
                        }
                    }
                }
            }
        }
    }

    // 3. Concurrently fetch org repos with limit of 5
    if !org_logins.is_empty() {
        use std::sync::Arc;
        use tokio::sync::Semaphore;

        let sem = Arc::new(Semaphore::new(5));
        let mut join_set = tokio::task::JoinSet::new();

        for org in org_logins {
            let client = client.clone();
            let token = token.clone();
            let sem = sem.clone();
            join_set.spawn(async move {
                let _permit = sem.acquire().await.ok();
                let mut org_repos = Vec::new();
                for page in 1..=2 {
                    let url = format!(
                        "https://api.github.com/orgs/{}/repos?per_page=100&page={}",
                        org, page
                    );
                    let resp = client
                        .get(&url)
                        .header("User-Agent", "nexus-assistant")
                        .header("Accept", "application/vnd.github+json")
                        .bearer_auth(&token)
                        .send()
                        .await;

                    match resp {
                        Ok(r) => {
                            let status = r.status();
                            if status == reqwest::StatusCode::FORBIDDEN
                                || status == reqwest::StatusCode::NOT_FOUND
                            {
                                println!("[GITHUB-INDEX] org {} returned {}, skipping", org, status);
                                break;
                            }
                            if !status.is_success() {
                                break;
                            }
                            if let Ok(items) = r.json::<Vec<serde_json::Value>>().await {
                                let count = items.len();
                                for item in items {
                                    if let (Some(full_name), Some(name)) = (
                                        item.get("full_name").and_then(|v| v.as_str()),
                                        item.get("name").and_then(|v| v.as_str()),
                                    ) {
                                        let owner = item
                                            .get("owner")
                                            .and_then(|o| o.get("login"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or(&org)
                                            .to_string();
                                        let owner_type = item
                                            .get("owner")
                                            .and_then(|o| o.get("type"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Organization")
                                            .to_string();
                                        let open_issues = item
                                            .get("open_issues_count")
                                            .and_then(|v| v.as_u64())
                                            .unwrap_or(0) as u32;

                                        if !name.is_empty() && !full_name.is_empty() {
                                            org_repos.push(RepoEntry {
                                                full_name: full_name.to_string(),
                                                owner,
                                                name: name.to_string(),
                                                owner_type,
                                                open_issues,
                                            });
                                        }
                                    }
                                }
                                if count < 100 {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        Err(e) => {
                            println!("[GITHUB-INDEX] org {} fetch error: {}, skipping", org, e);
                            break;
                        }
                    }
                }
                org_repos
            });
        }

        while let Some(res) = join_set.join_next().await {
            if let Ok(repos) = res {
                all_repos.extend(repos);
            }
        }
    }

    // Dedupe by full_name
    let mut deduped: Vec<RepoEntry> = Vec::new();
    for r in all_repos {
        if !deduped
            .iter()
            .any(|d| d.full_name.eq_ignore_ascii_case(&r.full_name))
        {
            deduped.push(r);
        }
    }

    Ok(deduped)
}

/// Ensure the cache is refreshed if stale.
async fn refresh_cache_if_stale() {
    let is_stale = {
        let guard = CACHE.lock().ok();
        match guard.as_ref().and_then(|g| g.as_ref()) {
            Some((time, _)) => time.elapsed() > CACHE_TTL,
            None => true,
        }
    };

    if is_stale {
        if let Ok(fresh) = refresh_cache().await {
            if !fresh.is_empty() {
                if let Ok(mut guard) = CACHE.lock() {
                    *guard = Some((Instant::now(), fresh));
                }
            }
        }
    }
}

/// Asynchronously resolve a repository query, refreshing the cache if stale.
pub async fn resolve_name(query: &str) -> Resolution {
    refresh_cache_if_stale().await;

    if let Ok(guard) = CACHE.lock() {
        if let Some((_, ref entries)) = *guard {
            return resolve_in(entries, query);
        }
    }

    if let Some(res) = peek(query) {
        return res;
    }

    Resolution::None
}

/// Return all cached repository entries, refreshing the cache if stale.
pub async fn entries() -> Vec<RepoEntry> {
    refresh_cache_if_stale().await;

    if let Ok(guard) = CACHE.lock() {
        if let Some((_, ref entries)) = *guard {
            return entries.clone();
        }
    }

    let _ = peek("");
    if let Ok(guard) = CACHE.lock() {
        if let Some((_, ref entries)) = *guard {
            return entries.clone();
        }
    }

    Vec::new()
}

/// Seed the in-memory cache for unit tests.
#[cfg(test)]
pub fn seed_cache_for_test(entries: Vec<RepoEntry>) {
    if let Ok(mut guard) = CACHE.lock() {
        if let Some((_, ref mut existing)) = *guard {
            existing.extend(entries);
        } else {
            *guard = Some((Instant::now(), entries));
        }
    }
}

/// Clear the in-memory cache for unit tests.
#[cfg(test)]
pub fn clear_cache_for_test() {
    if let Ok(mut guard) = CACHE.lock() {
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entries() -> Vec<RepoEntry> {
        vec![
            RepoEntry {
                full_name: "chitkul/zync".into(),
                owner: "chitkul".into(),
                name: "zync".into(),
                owner_type: "User".into(),
                open_issues: 3,
            },
            RepoEntry {
                full_name: "acme/zync".into(),
                owner: "acme".into(),
                name: "zync".into(),
                owner_type: "Organization".into(),
                open_issues: 12,
            },
            RepoEntry {
                full_name: "chitkul/ultron".into(),
                owner: "chitkul".into(),
                name: "ultron".into(),
                owner_type: "User".into(),
                open_issues: 5,
            },
            RepoEntry {
                full_name: "chitkul/ledger-ai".into(),
                owner: "chitkul".into(),
                name: "ledger-ai".into(),
                owner_type: "User".into(),
                open_issues: 1,
            },
            RepoEntry {
                full_name: "chitkul/servx".into(),
                owner: "chitkul".into(),
                name: "servx".into(),
                owner_type: "User".into(),
                open_issues: 0,
            },
        ]
    }

    #[test]
    fn test_exact_match() {
        let entries = sample_entries();
        let res = resolve_in(&entries, "ultron");
        match res {
            Resolution::One(e) => {
                assert_eq!(e.full_name, "chitkul/ultron");
            }
            other => panic!("expected One(chitkul/ultron), got {:?}", other),
        }
    }

    #[test]
    fn test_sound_alias() {
        let entries = sample_entries();
        // zinc -> zync via sound alias table
        let res = resolve_in(&entries, "zinc");
        match res {
            Resolution::Many(v) => {
                assert_eq!(v.len(), 2);
                assert_eq!(v[0].full_name, "chitkul/zync"); // User repo first
            }
            other => panic!("expected Many for zync across owners, got {:?}", other),
        }
    }

    #[test]
    fn test_prefix_match() {
        let entries = sample_entries();
        // "ult" has length 3 and matches ultron
        let res = resolve_in(&entries, "ult");
        match res {
            Resolution::One(e) => assert_eq!(e.full_name, "chitkul/ultron"),
            other => panic!("expected One(chitkul/ultron), got {:?}", other),
        }
    }

    #[test]
    fn test_edit_distance() {
        let entries = sample_entries();
        // "ultrn" is edit distance 1 from "ultron" (length 5 <= 5)
        let res = resolve_in(&entries, "ultrn");
        match res {
            Resolution::One(e) => assert_eq!(e.full_name, "chitkul/ultron"),
            other => panic!("expected One(chitkul/ultron), got {:?}", other),
        }
    }

    #[test]
    fn test_no_substring_for_short_queries() {
        let entries = sample_entries();
        // "ync" is length 3, not a prefix of "zync", and short queries must not match substring
        let res = resolve_in(&entries, "ync");
        assert_eq!(res, Resolution::None);
    }

    #[test]
    fn test_ambiguity_across_owners() {
        let entries = sample_entries();
        let res = resolve_in(&entries, "zync");
        match res {
            Resolution::Many(v) => {
                assert_eq!(v.len(), 2);
                // User owner must be first despite fewer open issues (3 vs 12)
                assert_eq!(v[0].owner, "chitkul");
                assert_eq!(v[0].owner_type, "User");
                assert_eq!(v[1].owner, "acme");
                assert_eq!(v[1].owner_type, "Organization");
            }
            other => panic!("expected Many(2), got {:?}", other),
        }
    }

    #[test]
    fn test_owner_repo_form() {
        let entries = sample_entries();
        let res = resolve_in(&entries, "acme/zync");
        match res {
            Resolution::One(e) => assert_eq!(e.full_name, "acme/zync"),
            other => panic!("expected One(acme/zync), got {:?}", other),
        }

        let res_missing = resolve_in(&entries, "nonexistent/repo");
        assert_eq!(res_missing, Resolution::None);
    }

    #[test]
    fn test_empty_list_none() {
        let res = resolve_in(&[], "zync");
        assert_eq!(res, Resolution::None);
    }

    #[test]
    fn test_dedupe_same_full_name() {
        let duplicate_entries = vec![
            RepoEntry {
                full_name: "acme/zync".into(),
                owner: "acme".into(),
                name: "zync".into(),
                owner_type: "Organization".into(),
                open_issues: 12,
            },
            RepoEntry {
                full_name: "acme/zync".into(),
                owner: "acme".into(),
                name: "zync".into(),
                owner_type: "Organization".into(),
                open_issues: 12,
            },
        ];
        let res = resolve_in(&duplicate_entries, "acme/zync");
        match res {
            Resolution::One(e) => assert_eq!(e.full_name, "acme/zync"),
            other => panic!("expected One(acme/zync), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_entries_returns_cached_repos() {
        clear_cache_for_test();
        seed_cache_for_test(sample_entries());
        let e = entries().await;
        assert_eq!(e.len(), 5);
        assert_eq!(e[0].full_name, "chitkul/zync");
        clear_cache_for_test();
    }
}
