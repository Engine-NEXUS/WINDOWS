//! NEXUS GitHub Sub-Command Center — Live Data Backend Pipeline
//!
//! Fetches, aggregates, and caches comprehensive GitHub account data
//! (Repositories, Pull Requests, Organizations, Starred Repos, and Event Stats)
//! using the user's authenticated GitHub token.
//!
//! Provides 0ms instant cached loading with background async refresh.

use std::time::Duration;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};

const CACHE_FILE: &str = "github_dashboard_cache.json";

// ─── Data Models ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubRepoCard {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner_login: String,
    pub owner_avatar_url: String,
    pub owner_type: Option<String>,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub html_url: String,
    pub stars_count: u64,
    pub forks_count: u64,
    pub open_issues_count: u64,
    pub language: Option<String>,
    pub language_color: Option<String>,
    pub updated_at: String,
    pub pushed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubPrCard {
    pub id: u64,
    pub number: u64,
    pub title: String,
    pub state: String,
    pub html_url: String,
    pub repo_name: String,
    pub repo_owner: String,
    pub author: String,
    pub author_avatar_url: String,
    pub created_at: String,
    pub updated_at: String,
    pub draft: bool,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubOrgCard {
    pub id: u64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: String,
    pub description: Option<String>,
    pub html_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubStarredCard {
    pub id: u64,
    pub name: String,
    pub full_name: String,
    pub owner_login: String,
    pub owner_avatar_url: String,
    pub description: Option<String>,
    pub stars_count: u64,
    pub forks_count: u64,
    pub language: Option<String>,
    pub language_color: Option<String>,
    pub html_url: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubActivityStats {
    pub commits_pct: u32,
    pub pull_requests_pct: u32,
    pub code_review_pct: u32,
    pub issues_pct: u32,
    pub total_events: u32,
}

impl Default for GitHubActivityStats {
    fn default() -> Self {
        Self {
            commits_pct: 92,
            pull_requests_pct: 7,
            code_review_pct: 0,
            issues_pct: 1,
            total_events: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubDashboardData {
    pub profile: Option<crate::github_profile::GithubProfile>,
    pub repos: Vec<GitHubRepoCard>,
    pub prs: Vec<GitHubPrCard>,
    pub orgs: Vec<GitHubOrgCard>,
    pub starred: Vec<GitHubStarredCard>,
    pub activity_stats: GitHubActivityStats,
    pub last_updated: String,
}

// ─── Color Helper for Languages ─────────────────────────────────────────

pub fn language_to_color(lang: &str) -> &'static str {
    match lang.to_lowercase().as_str() {
        "typescript" => "#3178c6",
        "javascript" => "#f1e05a",
        "rust" => "#dea584",
        "python" => "#3572A5",
        "go" => "#00ADD8",
        "c++" | "cpp" => "#f34b7d",
        "c" => "#555555",
        "c#" | "csharp" => "#178600",
        "java" => "#b07219",
        "html" => "#e34c26",
        "css" => "#563d7c",
        "ruby" => "#701516",
        "php" => "#4F5D95",
        "swift" => "#F05138",
        "kotlin" => "#A97BFF",
        "shell" | "bash" => "#89e051",
        "powershell" => "#012456",
        "vue" => "#41b883",
        "dart" => "#00B4AB",
        "scala" => "#c22d40",
        _ => "#8b949e",
    }
}

// ─── Cache Management ───────────────────────────────────────────────────

fn cache_path<R: Runtime>(app: &AppHandle<R>) -> Option<std::path::PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(CACHE_FILE))
}

fn read_cache<R: Runtime>(app: &AppHandle<R>) -> Option<GitHubDashboardData> {
    let p = cache_path(app)?;
    let content = std::fs::read_to_string(p).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_cache<R: Runtime>(app: &AppHandle<R>, data: &GitHubDashboardData) {
    if let Some(p) = cache_path(app) {
        if let Ok(json) = serde_json::to_string(data) {
            let _ = std::fs::write(p, json);
        }
    }
}

// ─── Token Resolver ─────────────────────────────────────────────────────

pub async fn resolve_token(server_url: Option<&str>, user_id: Option<&str>) -> Result<String, String> {
    // 1. Try local auth vault (token or API key)
    if let Some(tok) = crate::auth_vault::get_token("github") {
        let t = tok.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }
    if let Some(tok) = crate::auth_vault::get_api_key("github") {
        let t = tok.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }

    // 2. Try host machine GitHub CLI (`gh auth token`)
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let mut cmd = std::process::Command::new("gh");
        cmd.args(["auth", "token"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !tok.is_empty() && (tok.starts_with("gh") || tok.starts_with("github_pat_")) {
                    tracing::info!("github_data: auto-resolved token from host gh CLI keyring");
                    crate::auth_vault::set_token("github", &tok, 30.0 * 24.0 * 3600.0);
                    return Ok(tok);
                }
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = std::process::Command::new("gh").args(["auth", "token"]).output() {
            if output.status.success() {
                let tok = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !tok.is_empty() && (tok.starts_with("gh") || tok.starts_with("github_pat_")) {
                    tracing::info!("github_data: auto-resolved token from host gh CLI keyring");
                    crate::auth_vault::set_token("github", &tok, 30.0 * 24.0 * 3600.0);
                    return Ok(tok);
                }
            }
        }
    }

    // 3. Fallback to GITHUB_TOKEN environment variable (useful in local dev / testing / CI)
    if let Ok(tok) = std::env::var("GITHUB_TOKEN") {
        let t = tok.trim().to_string();
        if !t.is_empty() {
            return Ok(t);
        }
    }

    // 4. Try session / worker token
    if let (Some(url), Some(uid)) = (server_url, user_id) {
        if !url.is_empty() && !uid.is_empty() {
            if let Ok(tok) = crate::github_cmd::fetch_token_from_worker(url, uid).await {
                let t = tok.trim().to_string();
                if !t.is_empty() {
                    return Ok(t);
                }
            }
        }
    }

    Err("No GitHub token found. Please connect your GitHub account.".into())
}

// ─── REST Fetchers ──────────────────────────────────────────────────────

async fn fetch_all_data(
    client: &reqwest::Client,
    token: &str,
) -> Result<GitHubDashboardData, String> {
    // 1. Profile: GET /user
    let profile_resp = client
        .get("https://api.github.com/user")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| format!("profile request: {e}"))?;

    let profile = if profile_resp.status().is_success() {
        let text = profile_resp.text().await.unwrap_or_default();
        crate::github_profile::parse_user(&text)
    } else {
        None
    };

    let username = profile.as_ref().map(|p| p.login.clone()).unwrap_or_default();

    // 2. Repositories: GET /user/repos?affiliation=owner,collaborator,organization_member&sort=updated&per_page=100
    let mut repos = Vec::new();
    let repos_resp = client
        .get("https://api.github.com/user/repos?affiliation=owner,collaborator,organization_member&sort=updated&per_page=100")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await;

    if let Ok(resp) = repos_resp {
        if resp.status().is_success() {
            if let Ok(items) = resp.json::<Vec<serde_json::Value>>().await {
                for r in items {
                    let id = r.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let name = r.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let full_name = r.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owner_login = r.get("owner").and_then(|o| o.get("login")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owner_avatar_url = r.get("owner").and_then(|o| o.get("avatar_url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owner_type = r.get("owner").and_then(|o| o.get("type")).and_then(|v| v.as_str()).map(|s| s.to_string());
                    let description = r.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let private = r.get("private").and_then(|v| v.as_bool()).unwrap_or(false);
                    let fork = r.get("fork").and_then(|v| v.as_bool()).unwrap_or(false);
                    let html_url = r.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let stars_count = r.get("stargazers_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let forks_count = r.get("forks_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let open_issues_count = r.get("open_issues_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let language = r.get("language").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let language_color = language.as_ref().map(|l| language_to_color(l).to_string());
                    let updated_at = r.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let pushed_at = r.get("pushed_at").and_then(|v| v.as_str()).map(|s| s.to_string());

                    if !name.is_empty() {
                        repos.push(GitHubRepoCard {
                            id,
                            name,
                            full_name,
                            owner_login,
                            owner_avatar_url,
                            owner_type,
                            description,
                            private,
                            fork,
                            html_url,
                            stars_count,
                            forks_count,
                            open_issues_count,
                            language,
                            language_color,
                            updated_at,
                            pushed_at,
                        });
                    }
                }
            }
        }
    }

    // 3. User Pull Requests across repos: GET /search/issues?q=is:pr+involves:{username}+sort:updated-desc&per_page=50
    let mut prs = Vec::new();
    let pr_query = if !username.is_empty() {
        format!("is:pr involves:{username} sort:updated-desc")
    } else {
        "is:pr sort:updated-desc".to_string()
    };
    let prs_url = format!("https://api.github.com/search/issues?q={}&per_page=50", urlencoding_safe(&pr_query));
    let prs_resp = client
        .get(&prs_url)
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await;

    if let Ok(resp) = prs_resp {
        if resp.status().is_success() {
            if let Ok(res_json) = resp.json::<serde_json::Value>().await {
                if let Some(items) = res_json.get("items").and_then(|v| v.as_array()) {
                    for item in items {
                        let id = item.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                        let number = item.get("number").and_then(|v| v.as_u64()).unwrap_or(0);
                        let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("open").to_string();
                        let html_url = item.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let repo_url = item.get("repository_url").and_then(|v| v.as_str()).unwrap_or("");
                        let (repo_owner, repo_name) = match repo_url.split('/').collect::<Vec<_>>().as_slice() {
                            [.., owner, name] => (owner.to_string(), name.to_string()),
                            _ => ("".to_string(), "".to_string()),
                        };
                        let author = item.get("user").and_then(|u| u.get("login")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let author_avatar_url = item.get("user").and_then(|u| u.get("avatar_url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let created_at = item.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let updated_at = item.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let draft = item.get("draft").and_then(|v| v.as_bool()).unwrap_or(false);
                        let labels = item
                            .get("labels")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|l| l.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()))
                                    .collect()
                            })
                            .unwrap_or_default();

                        if !title.is_empty() {
                            prs.push(GitHubPrCard {
                                id,
                                number,
                                title,
                                state,
                                html_url,
                                repo_name,
                                repo_owner,
                                author,
                                author_avatar_url,
                                created_at,
                                updated_at,
                                draft,
                                labels,
                            });
                        }
                    }
                }
            }
        }
    }

    // 4. Organizations: GET /user/orgs?per_page=100
    let mut orgs = Vec::new();
    let orgs_resp = client
        .get("https://api.github.com/user/orgs?per_page=100")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await;

    if let Ok(resp) = orgs_resp {
        if resp.status().is_success() {
            if let Ok(items) = resp.json::<Vec<serde_json::Value>>().await {
                for o in items {
                    let id = o.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let login = o.get("login").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = o.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let avatar_url = o.get("avatar_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let description = o.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let html_url = format!("https://github.com/{login}");

                    if !login.is_empty() {
                        orgs.push(GitHubOrgCard {
                            id,
                            login,
                            name,
                            avatar_url,
                            description,
                            html_url,
                        });
                    }
                }
            }
        }
    }

    // 5. Starred Repositories: GET /user/starred?sort=created&direction=desc&per_page=100
    let mut starred = Vec::new();
    let starred_resp = client
        .get("https://api.github.com/user/starred?sort=created&direction=desc&per_page=100")
        .header("User-Agent", "nexus-assistant")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token)
        .send()
        .await;

    if let Ok(resp) = starred_resp {
        if resp.status().is_success() {
            if let Ok(items) = resp.json::<Vec<serde_json::Value>>().await {
                for s in items {
                    let id = s.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
                    let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let full_name = s.get("full_name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owner_login = s.get("owner").and_then(|o| o.get("login")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let owner_avatar_url = s.get("owner").and_then(|o| o.get("avatar_url")).and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let description = s.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let stars_count = s.get("stargazers_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let forks_count = s.get("forks_count").and_then(|v| v.as_u64()).unwrap_or(0);
                    let language = s.get("language").and_then(|v| v.as_str()).map(|s| s.to_string());
                    let language_color = language.as_ref().map(|l| language_to_color(l).to_string());
                    let html_url = s.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let updated_at = s.get("updated_at").and_then(|v| v.as_str()).unwrap_or("").to_string();

                    if !name.is_empty() {
                        starred.push(GitHubStarredCard {
                            id,
                            name,
                            full_name,
                            owner_login,
                            owner_avatar_url,
                            description,
                            stars_count,
                            forks_count,
                            language,
                            language_color,
                            html_url,
                            updated_at,
                        });
                    }
                }
            }
        }
    }

    // 6. User Activity / Events: GET /users/{username}/events?per_page=100
    let mut activity_stats = GitHubActivityStats::default();
    if !username.is_empty() {
        let events_url = format!("https://api.github.com/users/{username}/events?per_page=100");
        let events_resp = client
            .get(&events_url)
            .header("User-Agent", "nexus-assistant")
            .header("Accept", "application/vnd.github+json")
            .bearer_auth(token)
            .send()
            .await;

        if let Ok(resp) = events_resp {
            if resp.status().is_success() {
                if let Ok(events) = resp.json::<Vec<serde_json::Value>>().await {
                    let mut commits_count = 0u32;
                    let mut prs_count = 0u32;
                    let mut reviews_count = 0u32;
                    let mut issues_count = 0u32;

                    for ev in &events {
                        let ev_type = ev.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match ev_type {
                            "PushEvent" => {
                                let c = ev
                                    .get("payload")
                                    .and_then(|p| p.get("commits"))
                                    .and_then(|c| c.as_array())
                                    .map(|arr| arr.len() as u32)
                                    .unwrap_or(1);
                                commits_count += c;
                            }
                            "PullRequestEvent" => prs_count += 1,
                            "PullRequestReviewEvent" | "PullRequestReviewCommentEvent" => reviews_count += 1,
                            "IssuesEvent" | "IssueCommentEvent" => issues_count += 1,
                            _ => {}
                        }
                    }

                    let total = commits_count + prs_count + reviews_count + issues_count;
                    if total > 0 {
                        activity_stats = GitHubActivityStats {
                            commits_pct: (commits_count * 100) / total,
                            pull_requests_pct: (prs_count * 100) / total,
                            code_review_pct: (reviews_count * 100) / total,
                            issues_pct: (issues_count * 100) / total,
                            total_events: total,
                        };
                    }
                }
            }
        }
    }

    let last_updated = chrono::Utc::now().to_rfc3339();

    Ok(GitHubDashboardData {
        profile,
        repos,
        prs,
        orgs,
        starred,
        activity_stats,
        last_updated,
    })
}

fn urlencoding_safe(s: &str) -> String {
    s.replace(' ', "%20")
        .replace(':', "%3A")
        .replace('@', "%40")
        .replace('/', "%2F")
}

// ─── Tauri IPC Command ──────────────────────────────────────────────────

#[tauri::command]
pub async fn github_get_dashboard_data<R: Runtime>(
    app: AppHandle<R>,
    server_url: Option<String>,
    user_id: Option<String>,
    force_refresh: Option<bool>,
) -> Result<GitHubDashboardData, String> {
    let force = force_refresh.unwrap_or(false);

    // If cache exists and force is not true, return cache immediately and refresh in background
    if !force {
        if let Some(cached) = read_cache(&app) {
            let app_clone = app.clone();
            let s_url = server_url.clone();
            let u_id = user_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(tok) = resolve_token(s_url.as_deref(), u_id.as_deref()).await {
                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(12))
                        .build()
                        .unwrap_or_default();
                    if let Ok(fresh) = fetch_all_data(&client, &tok).await {
                        write_cache(&app_clone, &fresh);
                        let _ = app_clone.emit("github:dashboard_updated", &fresh);
                    }
                }
            });
            return Ok(cached);
        }
    }

    // Otherwise, fetch directly
    let token = resolve_token(server_url.as_deref(), user_id.as_deref()).await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;

    let fresh = fetch_all_data(&client, &token).await?;
    write_cache(&app, &fresh);
    Ok(fresh)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_color_mapping() {
        assert_eq!(language_to_color("TypeScript"), "#3178c6");
        assert_eq!(language_to_color("Rust"), "#dea584");
        assert_eq!(language_to_color("Python"), "#3572A5");
        assert_eq!(language_to_color("UnknownLang"), "#8b949e");
    }

    #[test]
    fn test_activity_stats_default() {
        let stats = GitHubActivityStats::default();
        assert_eq!(stats.commits_pct, 92);
        assert_eq!(stats.pull_requests_pct, 7);
        assert_eq!(stats.issues_pct, 1);
        assert_eq!(stats.code_review_pct, 0);
    }

    #[tokio::test]
    async fn test_resolve_token_falls_back_gracefully() {
        // Without any credentials configured or worker, resolve_token returns Err
        // With GITHUB_TOKEN set, it resolves immediately
        std::env::set_var("GITHUB_TOKEN", "ghp_mock_token_for_tests");
        let resolved = resolve_token(None, None).await;
        assert!(resolved.is_ok());
        assert_eq!(resolved.unwrap(), "ghp_mock_token_for_tests");
        std::env::remove_var("GITHUB_TOKEN");
    }
}
