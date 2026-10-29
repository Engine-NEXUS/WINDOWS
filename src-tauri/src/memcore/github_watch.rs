//! GitHub Actions watcher (task P2).
//!
//! Watches the latest completed GitHub Actions runs of the user's most recently
//! pushed repositories. Feeds the notification state machine (`notify_brain`):
//! - First failure -> announced once ("Build failed in zync, sir." / "PR failed in zync, sir.").
//! - Further failures -> silent.
//! - Recovery -> announced once ("zync is green again, sir.").
//! - Baseline (connect/startup) -> recorded quietly without speaking.
//!
//! Pure deterministic logic, 0 AI credits, ETag caching with 304 Not Modified.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use crate::notify_brain::{Fact, Kind, Obs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub id: u64,
    pub event: String,
    pub conclusion: Option<String>,
    pub status: String,
    pub updated_at_ms: i64,
}

pub fn parse_runs(json: &str) -> Vec<Run> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val
        .get("workflow_runs")
        .and_then(|v| v.as_array())
        .or_else(|| val.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut runs = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(id) = item.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let Some(event) = item.get("event").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(status) = item.get("status").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(updated_at_str) = item.get("updated_at").and_then(|v| v.as_str()) else {
            continue;
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(updated_at_str) else {
            continue;
        };
        let conclusion = item
            .get("conclusion")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        runs.push(Run {
            id,
            event: event.to_string(),
            conclusion,
            status: status.to_string(),
            updated_at_ms: dt.timestamp_millis(),
        });
    }
    runs
}

fn obs_of(conclusion: &str) -> Option<Obs> {
    match conclusion {
        "success" => Some(Obs::Fixed),
        "failure" | "timed_out" | "startup_failure" => Some(Obs::Failed),
        _ => None,
    }
}

fn bucket(event: &str) -> Kind {
    match event {
        "pull_request" | "pull_request_target" => Kind::Pr,
        _ => Kind::Build,
    }
}

pub fn facts_for_repo(repo_name: &str, runs: &[Run], now_ms: i64) -> Vec<Fact> {
    const MAX_AGE_MS: i64 = 48 * 3600 * 1000;
    let mut facts = Vec::new();
    for target_kind in [Kind::Build, Kind::Pr] {
        let candidate = runs
            .iter()
            .filter(|r| bucket(&r.event) == target_kind)
            .filter(|r| r.status == "completed")
            .filter(|r| r.updated_at_ms > 0 && now_ms - r.updated_at_ms <= MAX_AGE_MS)
            .filter_map(|r| {
                let obs = r.conclusion.as_deref().and_then(obs_of)?;
                Some((r, obs))
            })
            .max_by(|(a, _), (b, _)| {
                a.updated_at_ms
                    .cmp(&b.updated_at_ms)
                    .then_with(|| a.id.cmp(&b.id))
            });

        if let Some((run, obs)) = candidate {
            facts.push(Fact {
                service: "github",
                project: repo_name.to_string(),
                kind: target_kind,
                obs,
                run_ref: run.id.to_string(),
            });
        }
    }
    facts
}

fn next_delay_secs(fails: u32, rate_limited: bool, remaining: Option<u32>) -> u64 {
    if rate_limited || remaining.map_or(false, |r| r <= 50) {
        return 15 * 60;
    }
    if fails == 0 {
        return 120;
    }
    (120u64.saturating_mul(1u64 << fails.min(6))).min(30 * 60)
}

fn pick_repos(json: &str, max: usize) -> Vec<(String, String)> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(arr) = val.as_array() else {
        return Vec::new();
    };
    let mut repos = Vec::new();
    for item in arr {
        let archived = item.get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
        if archived {
            continue;
        }
        let disabled = item.get("disabled").and_then(|v| v.as_bool()).unwrap_or(false);
        if disabled {
            continue;
        }
        let fork = item.get("fork").and_then(|v| v.as_bool()).unwrap_or(false);
        if fork {
            continue;
        }
        let full_name = item.get("full_name").and_then(|v| v.as_str()).unwrap_or("").trim();
        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("").trim();
        if !full_name.is_empty() && !name.is_empty() {
            repos.push((full_name.to_string(), name.to_string()));
            if repos.len() >= max {
                break;
            }
        }
    }
    repos
}

/// GitHub sends `x-ratelimit-remaining` on every response, so a 403 is only a rate limit when
/// the budget is really 0 or a `retry-after` is present; any other 403 means "not allowed here".
pub(crate) fn is_rate_limited(status: reqwest::StatusCode, remaining: Option<u32>, has_retry_after: bool) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN && (remaining == Some(0) || has_retry_after))
}

#[derive(Debug, PartialEq, Eq)]
enum PollError {
    Auth,
    RateLimited,
    Other(String),
}

#[derive(Default)]
struct WatchState {
    seen_repos: HashSet<String>,
    etags: HashMap<String, String>,
    rate_limit_remaining: Option<u32>,
}

async fn poll_once(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    state: &mut WatchState,
    now_ms: i64,
) -> Result<Vec<(Fact, bool)>, PollError> {
    let base = base.trim_end_matches('/');
    let repos_url = format!("{base}/user/repos?sort=pushed&per_page=10&affiliation=owner,collaborator,organization_member");
    let req = client
        .get(&repos_url)
        .header("User-Agent", "nexus-assistant")
        .header("Authorization", format!("Bearer {token}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");

    let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;

    if let Some(rem) = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
    {
        state.rate_limit_remaining = Some(rem);
    }

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(PollError::Auth);
    }
    if is_rate_limited(status, state.rate_limit_remaining, resp.headers().contains_key("retry-after")) {
        return Err(PollError::RateLimited);
    }
    if !status.is_success() {
        return Err(PollError::Other(format!("repos returned {status}")));
    }

    let repos_json = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
    let repos = pick_repos(&repos_json, 5);

    let mut out = Vec::new();
    for (full_name, name) in repos {
        let runs_url = format!("{base}/repos/{full_name}/actions/runs?per_page=15&status=completed");
        let mut req = client
            .get(&runs_url)
            .header("User-Agent", "nexus-assistant")
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");

        if let Some(etag) = state.etags.get(&full_name) {
            req = req.header("If-None-Match", etag);
        }

        let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;

        if let Some(rem) = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
        {
            state.rate_limit_remaining = Some(rem);
        }

        let status = resp.status();
        if status == reqwest::StatusCode::NOT_MODIFIED {
            continue;
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(PollError::Auth);
        }
        if is_rate_limited(status, state.rate_limit_remaining, resp.headers().contains_key("retry-after")) {
            return Err(PollError::RateLimited);
        }
        if matches!(
            status,
            reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
        ) {
            // Actions not readable for this repo (disabled, no access, SSO not authorised):
            // skip it, the other repos are still worth watching.
            tracing::debug!("[GITHUB-WATCH] {full_name}: runs not readable ({status})");
            continue;
        }
        if !status.is_success() {
            return Err(PollError::Other(format!("runs for {full_name} returned {status}")));
        }

        if let Some(etag) = resp.headers().get("etag").and_then(|h| h.to_str().ok()) {
            state.etags.insert(full_name.clone(), etag.to_string());
        }

        let runs_json = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
        let runs = parse_runs(&runs_json);
        let facts = facts_for_repo(&name, &runs, now_ms);

        let is_first_time = !state.seen_repos.contains(&full_name);
        if is_first_time {
            state.seen_repos.insert(full_name);
        }
        let baseline = is_first_time;
        for fact in facts {
            out.push((fact, baseline));
        }
    }

    Ok(out)
}

/// A PR check just failed and was announced: open the CodeRabbit panel on that PR so its review is in front of the owner.
/// Quiet when the repo or PR cannot be identified.
async fn open_failed_pr_review<R: Runtime>(app: &AppHandle<R>, project: &str, run_ref: &str, token: &str) {
    let crate::github_repo_index::Resolution::One(entry) = crate::github_repo_index::resolve_name(project).await else {
        return;
    };
    let Some(pr) = crate::coderabbit::pr_number_for_run(&entry.full_name, run_ref, token).await else {
        return;
    };
    if let Err(e) = crate::commands::show_coderabbit_pr(app.clone(), entry.full_name.clone(), pr).await {
        tracing::warn!("[GITHUB-WATCH] could not open the CodeRabbit review: {e}");
    }
}

fn speak_address<R: Runtime>(app: &AppHandle<R>) -> Option<String> {
    let friend = crate::persona::is_friend(&crate::commands::read_persona_mode(app));
    let name = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|d| crate::memory::read_user_profile(&d))
        .and_then(|p| p.name);
    crate::persona::address(name.as_deref(), friend)
}

fn note_auth_state<R: Runtime>(app: &AppHandle<R>, dir: &std::path::Path, needs_signin: bool) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let say = crate::memcore::with_store(dir, |s| {
        let was = s.meta_get("github_needs_signin").map(|v| !v.is_empty()).unwrap_or(false);
        if !needs_signin {
            if was {
                s.meta_set("github_needs_signin", "");
            }
            return false;
        }
        s.meta_set("github_needs_signin", "1");
        let told = s.meta_get("github_signin_alert_day").as_deref() == Some(today.as_str());
        if !told {
            s.meta_set("github_signin_alert_day", &today);
        }
        !told
    })
    .unwrap_or(false);
    if say {
        let addr = speak_address(app);
        let text = crate::memcore::mailwording::tail(addr.as_deref(), "GitHub needs you to sign in again");
        crate::proactive_policy::submit(
            app,
            "github_signin".to_string(),
            text,
            crate::google::types::AlertUrgency::Medium,
        );
    }
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut next_due = Instant::now();
        let mut failures: u32 = 0;
        let mut noted_auth = false;
        let mut logged_no_token = false;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        let mut state = WatchState::default();

        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let Ok(dir) = app.path().app_data_dir() else {
                continue;
            };
            if !crate::memcore::enabled(&dir) || !crate::memcore::flag(&dir, "memcoreGithubWatch", true) {
                continue;
            }
            if crate::focus_mode::paused() {
                crate::focus_mode::defer_log("github_watch");
                continue;
            }
            if Instant::now() < next_due {
                continue;
            }

            let (worker_url, user_id) = crate::network::get_session_info()
                .map(|(w, u, _)| (w, u))
                .unwrap_or_default();
            let token = match crate::github_cmd::get_github_token(&worker_url, &user_id).await {
                Ok(t) => {
                    logged_no_token = false;
                    t
                }
                Err(_) => {
                    if !logged_no_token {
                        tracing::info!("[GITHUB-WATCH] no token");
                        logged_no_token = true;
                    }
                    next_due = Instant::now() + Duration::from_secs(300);
                    continue;
                }
            };

            let now_ms = chrono::Utc::now().timestamp_millis();
            let poll_res = poll_once(&client, "https://api.github.com", &token, &mut state, now_ms).await;
            match poll_res {
                Ok(items) => {
                    failures = 0;
                    noted_auth = false;
                    note_auth_state(&app, &dir, false);
                    for (fact, baseline) in items {
                        if let crate::notify_brain::Decision::Say(line) =
                            crate::notify_brain::observe_fact(&app, &fact, baseline)
                        {
                            crate::notify_brain::say(&app, "github", line.clone());
                            // A clickable card only for a repository that resolves to exactly one owner.
                            if let crate::github_repo_index::Resolution::One(entry) =
                                crate::github_repo_index::resolve_name(&fact.project).await
                            {
                                crate::notify_hub::publish_github(&app, &entry.full_name, &fact, &line);
                            }
                            if fact.kind == Kind::Pr
                                && fact.obs == Obs::Failed
                                && crate::memcore::flag(&dir, "coderabbitAutoOpen", true)
                            {
                                let (app2, token2) = (app.clone(), token.clone());
                                let (project, run_ref) = (fact.project.clone(), fact.run_ref.clone());
                                tauri::async_runtime::spawn(async move {
                                    open_failed_pr_review(&app2, &project, &run_ref, &token2).await;
                                });
                            }
                        }
                    }
                    let delay = next_delay_secs(0, false, state.rate_limit_remaining);
                    next_due = Instant::now() + Duration::from_secs(delay);
                }
                Err(PollError::Auth) => {
                    failures += 1;
                    note_auth_state(&app, &dir, true);
                    if !noted_auth {
                        noted_auth = true;
                        tracing::warn!("[GITHUB-WATCH] authentication failed; sign-in needed");
                    }
                    let delay = next_delay_secs(failures, false, state.rate_limit_remaining);
                    next_due = Instant::now() + Duration::from_secs(delay);
                }
                Err(PollError::RateLimited) => {
                    failures += 1;
                    tracing::warn!("[GITHUB-WATCH] rate limit reached; backing off");
                    let delay = next_delay_secs(failures, true, state.rate_limit_remaining);
                    next_due = Instant::now() + Duration::from_secs(delay);
                }
                Err(PollError::Other(e)) => {
                    failures += 1;
                    tracing::debug!("[GITHUB-WATCH] poll error: {e}");
                    let delay = next_delay_secs(failures, false, state.rate_limit_remaining);
                    next_due = Instant::now() + Duration::from_secs(delay);
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memcore::test_http::start_mock_server;
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, Mutex};

    #[test]
    fn rate_limit_detection_is_precise() {
        use reqwest::StatusCode as S;
        assert!(is_rate_limited(S::TOO_MANY_REQUESTS, Some(100), false));
        assert!(is_rate_limited(S::FORBIDDEN, Some(0), false));
        assert!(is_rate_limited(S::FORBIDDEN, Some(4000), true));
        assert!(!is_rate_limited(S::FORBIDDEN, Some(4000), false), "an ordinary 403 is not a rate limit");
        assert!(!is_rate_limited(S::NOT_FOUND, Some(0), false));
        assert!(!is_rate_limited(S::OK, Some(0), false));
    }

    #[test]
    fn test_parse_runs_tolerant() {
        let json = r#"{
            "total_count": 5,
            "workflow_runs": [
                {
                    "id": 101,
                    "event": "push",
                    "status": "completed",
                    "conclusion": "failure",
                    "updated_at": "2026-10-25T12:00:00Z"
                },
                {
                    "id": 102,
                    "event": "pull_request",
                    "status": "completed",
                    "conclusion": "success",
                    "updated_at": "2026-10-25T12:05:00Z"
                },
                {
                    "id": 103,
                    "event": "push",
                    "status": "in_progress",
                    "conclusion": null,
                    "updated_at": "2026-10-25T12:10:00Z"
                },
                {
                    "id": 104,
                    "event": "push",
                    "status": "completed",
                    "conclusion": "failure"
                },
                {
                    "event": "push",
                    "status": "completed",
                    "conclusion": "success",
                    "updated_at": "2026-10-25T12:15:00Z"
                },
                {
                    "id": 105,
                    "event": "push",
                    "status": "completed",
                    "conclusion": "success",
                    "updated_at": "invalid-date"
                }
            ]
        }"#;

        let runs = parse_runs(json);
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].id, 101);
        assert_eq!(runs[0].event, "push");
        assert_eq!(runs[0].status, "completed");
        assert_eq!(runs[0].conclusion.as_deref(), Some("failure"));

        assert_eq!(runs[1].id, 102);
        assert_eq!(runs[1].event, "pull_request");
        assert_eq!(runs[1].status, "completed");
        assert_eq!(runs[1].conclusion.as_deref(), Some("success"));

        assert_eq!(runs[2].id, 103);
        assert_eq!(runs[2].status, "in_progress");
        assert_eq!(runs[2].conclusion, None);
    }

    #[test]
    fn test_obs_of() {
        assert_eq!(obs_of("success"), Some(Obs::Fixed));
        assert_eq!(obs_of("failure"), Some(Obs::Failed));
        assert_eq!(obs_of("timed_out"), Some(Obs::Failed));
        assert_eq!(obs_of("startup_failure"), Some(Obs::Failed));
        assert_eq!(obs_of("cancelled"), None);
        assert_eq!(obs_of("skipped"), None);
        assert_eq!(obs_of("neutral"), None);
        assert_eq!(obs_of("stale"), None);
        assert_eq!(obs_of("action_required"), None);
        assert_eq!(obs_of("unknown_conclusion"), None);
    }

    #[test]
    fn test_bucket() {
        assert_eq!(bucket("pull_request"), Kind::Pr);
        assert_eq!(bucket("pull_request_target"), Kind::Pr);
        assert_eq!(bucket("push"), Kind::Build);
        assert_eq!(bucket("schedule"), Kind::Build);
        assert_eq!(bucket("workflow_dispatch"), Kind::Build);
    }

    #[test]
    fn test_facts_for_repo_newest_wins() {
        let now = 1_000_000_000;
        let runs = vec![
            Run {
                id: 1,
                event: "push".into(),
                conclusion: Some("failure".into()),
                status: "completed".into(),
                updated_at_ms: now - 3600 * 1000,
            },
            Run {
                id: 2,
                event: "push".into(),
                conclusion: Some("success".into()),
                status: "completed".into(),
                updated_at_ms: now - 1800 * 1000, // newer
            },
        ];
        let facts = facts_for_repo("zync", &runs, now);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].project, "zync");
        assert_eq!(facts[0].kind, Kind::Build);
        assert_eq!(facts[0].obs, Obs::Fixed);
        assert_eq!(facts[0].run_ref, "2");
    }

    #[test]
    fn test_facts_for_repo_cancelled_ignored() {
        let now = 1_000_000_000;
        let runs = vec![
            Run {
                id: 1,
                event: "push".into(),
                conclusion: Some("failure".into()),
                status: "completed".into(),
                updated_at_ms: now - 3600 * 1000,
            },
            Run {
                id: 2,
                event: "push".into(),
                conclusion: Some("cancelled".into()),
                status: "completed".into(),
                updated_at_ms: now - 1800 * 1000, // newer, but cancelled
            },
        ];
        let facts = facts_for_repo("zync", &runs, now);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].kind, Kind::Build);
        assert_eq!(facts[0].obs, Obs::Failed);
        assert_eq!(facts[0].run_ref, "1");
    }

    #[test]
    fn test_facts_for_repo_pr_vs_push_independent() {
        let now = 1_000_000_000;
        let runs = vec![
            Run {
                id: 10,
                event: "push".into(),
                conclusion: Some("success".into()),
                status: "completed".into(),
                updated_at_ms: now - 1000,
            },
            Run {
                id: 20,
                event: "pull_request".into(),
                conclusion: Some("failure".into()),
                status: "completed".into(),
                updated_at_ms: now - 2000,
            },
        ];
        let facts = facts_for_repo("zync", &runs, now);
        assert_eq!(facts.len(), 2);
        let build = facts.iter().find(|f| f.kind == Kind::Build).unwrap();
        assert_eq!(build.obs, Obs::Fixed);
        assert_eq!(build.run_ref, "10");

        let pr = facts.iter().find(|f| f.kind == Kind::Pr).unwrap();
        assert_eq!(pr.obs, Obs::Failed);
        assert_eq!(pr.run_ref, "20");
    }

    #[test]
    fn test_facts_for_repo_stale_ignored() {
        let now = 1_000_000_000;
        let stale_ms = 49 * 3600 * 1000; // > 48h
        let runs = vec![Run {
            id: 1,
            event: "push".into(),
            conclusion: Some("failure".into()),
            status: "completed".into(),
            updated_at_ms: now - stale_ms,
        }];
        let facts = facts_for_repo("zync", &runs, now);
        assert!(facts.is_empty(), "runs older than 48h must be ignored");
    }

    #[test]
    fn test_facts_for_repo_empty() {
        let facts = facts_for_repo("zync", &[], 1_000_000_000);
        assert!(facts.is_empty());
    }

    #[test]
    fn test_facts_for_repo_tie_breaker_highest_id() {
        let now = 1_000_000_000;
        let ts = now - 1000;
        let runs = vec![
            Run {
                id: 10,
                event: "push".into(),
                conclusion: Some("failure".into()),
                status: "completed".into(),
                updated_at_ms: ts,
            },
            Run {
                id: 20,
                event: "push".into(),
                conclusion: Some("success".into()),
                status: "completed".into(),
                updated_at_ms: ts,
            },
        ];
        let facts = facts_for_repo("zync", &runs, now);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].run_ref, "20", "tie breaker must pick highest id");
        assert_eq!(facts[0].obs, Obs::Fixed);
    }

    #[test]
    fn test_next_delay_secs() {
        assert_eq!(next_delay_secs(0, false, None), 120);
        assert_eq!(next_delay_secs(0, false, Some(100)), 120);
        assert_eq!(next_delay_secs(0, false, Some(50)), 900);
        assert_eq!(next_delay_secs(0, false, Some(10)), 900);
        assert_eq!(next_delay_secs(0, true, None), 900);
        assert_eq!(next_delay_secs(1, false, None), 240);
        assert_eq!(next_delay_secs(2, false, None), 480);
        assert_eq!(next_delay_secs(3, false, None), 960);
        assert_eq!(next_delay_secs(4, false, None), 1800);
        assert_eq!(next_delay_secs(10, false, None), 1800);
    }

    #[test]
    fn test_pick_repos() {
        let json = r#"[
            {"full_name": "owner/archived_repo", "name": "archived_repo", "archived": true},
            {"full_name": "owner/disabled_repo", "name": "disabled_repo", "disabled": true},
            {"full_name": "owner/fork_repo", "name": "fork_repo", "fork": true},
            {"full_name": "owner/repo1", "name": "repo1", "archived": false, "fork": false},
            {"full_name": "org/repo2", "name": "repo2"},
            {"full_name": "org/repo3", "name": "repo3"}
        ]"#;

        let picks = pick_repos(json, 2);
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[0], ("owner/repo1".to_string(), "repo1".to_string()));
        assert_eq!(picks[1], ("org/repo2".to_string(), "repo2".to_string()));

        let picks_all = pick_repos(json, 5);
        assert_eq!(picks_all.len(), 3);
        assert_eq!(picks_all[2], ("org/repo3".to_string(), "repo3".to_string()));
    }

    #[tokio::test]
    async fn test_poll_once_mock_flow() {
        let run_cycle = Arc::new(Mutex::new(0usize));
        let run_cycle_clone = run_cycle.clone();

        let (base, requests, stop) = start_mock_server(move |req| {
            if req.starts_with("GET /user/repos") {
                return (
                    200,
                    vec![("Content-Type", "application/json".into()), ("x-ratelimit-remaining", "4999".into())],
                    r#"[{"full_name": "test-owner/zync", "name": "zync"}]"#.into(),
                );
            }
            if req.starts_with("GET /repos/test-owner/zync/actions/runs") {
                let c = *run_cycle_clone.lock().unwrap();
                if c == 0 {
                    // First call: 200 with initial success run and ETag tag-1
                    return (
                        200,
                        vec![
                            ("Content-Type", "application/json".into()),
                            ("ETag", "\"tag-1\"".into()),
                            ("x-ratelimit-remaining", "4998".into()),
                        ],
                        r#"{
                            "workflow_runs": [
                                {
                                    "id": 100,
                                    "event": "push",
                                    "status": "completed",
                                    "conclusion": "success",
                                    "updated_at": "2026-10-25T12:00:00Z"
                                }
                            ]
                        }"#.into(),
                    );
                } else if c == 1 {
                    // Second call: if If-None-Match: "tag-1" is present, return 304
                    if req.to_lowercase().contains("if-none-match: \"tag-1\"") {
                        return (304, vec![], "".into());
                    } else {
                        return (400, vec![], "missing if-none-match".into());
                    }
                } else {
                    // Third call: 200 with new failing run
                    return (
                        200,
                        vec![
                            ("Content-Type", "application/json".into()),
                            ("ETag", "\"tag-2\"".into()),
                            ("x-ratelimit-remaining", "4997".into()),
                        ],
                        r#"{
                            "workflow_runs": [
                                {
                                    "id": 101,
                                    "event": "push",
                                    "status": "completed",
                                    "conclusion": "failure",
                                    "updated_at": "2026-10-25T12:30:00Z"
                                }
                            ]
                        }"#.into(),
                    );
                }
            }
            (404, vec![], "{}".into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        // Pinned: the fixture runs are dated, and runs older than 48 h are ignored by design.
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-10-25T13:00:00Z")
            .unwrap()
            .timestamp_millis();

        // 1. First call: every fact has baseline == true
        let facts1 = poll_once(&client, &base, "tok_123", &mut state, now_ms)
            .await
            .expect("first poll should succeed");
        assert_eq!(facts1.len(), 1);
        assert_eq!(facts1[0].0.project, "zync");
        assert_eq!(facts1[0].0.obs, Obs::Fixed);
        assert!(facts1[0].1, "first call must set baseline == true");
        assert_eq!(state.etags.get("test-owner/zync").map(|s| s.as_str()), Some("\"tag-1\""));

        // 2. Second call with 304 -> no facts, and ETag was sent
        *run_cycle.lock().unwrap() = 1;
        let facts2 = poll_once(&client, &base, "tok_123", &mut state, now_ms)
            .await
            .expect("second poll 304 should succeed");
        assert!(facts2.is_empty(), "304 response must yield no facts");

        // Verify that If-None-Match was sent on second request
        let reqs = requests.lock().unwrap();
        let runs_req2 = reqs
            .iter()
            .filter(|r| r.starts_with("GET /repos/test-owner/zync/actions/runs"))
            .nth(1)
            .expect("should have second runs request");
        assert!(
            runs_req2.to_lowercase().contains("if-none-match: \"tag-1\""),
            "ETag must be sent in If-None-Match header: {runs_req2}"
        );
        drop(reqs);

        // 3. Third call: new failing run id -> baseline == false
        *run_cycle.lock().unwrap() = 2;
        let facts3 = poll_once(&client, &base, "tok_123", &mut state, now_ms)
            .await
            .expect("third poll should succeed");
        assert_eq!(facts3.len(), 1);
        assert_eq!(facts3[0].0.obs, Obs::Failed);
        assert_eq!(facts3[0].0.run_ref, "101");
        assert!(!facts3[0].1, "subsequent call must set baseline == false");

        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_once_auth_error() {
        let (base, _, stop) = start_mock_server(|_| {
            (401, vec![("Content-Type", "application/json".into())], r#"{"message":"Bad credentials"}"#.into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let err = poll_once(&client, &base, "bad_tok", &mut state, now_ms)
            .await
            .expect_err("should return auth error");
        assert_eq!(err, PollError::Auth);
        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_once_rate_limited() {
        let (base, _, stop) = start_mock_server(|_| {
            (
                403,
                vec![
                    ("Content-Type", "application/json".into()),
                    ("x-ratelimit-remaining", "0".into()),
                ],
                r#"{"message":"API rate limit exceeded"}"#.into(),
            )
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::Utc::now().timestamp_millis();

        let err = poll_once(&client, &base, "tok", &mut state, now_ms)
            .await
            .expect_err("should return rate limited error");
        assert_eq!(err, PollError::RateLimited);
        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn an_unreadable_repo_is_skipped_and_the_others_still_report() {
        let (base, _, stop) = start_mock_server(|req| {
            if req.starts_with("GET /user/repos") {
                return (
                    200,
                    vec![("Content-Type", "application/json".into()), ("x-ratelimit-remaining", "4999".into())],
                    r#"[{"full_name": "org/secret", "name": "secret"}, {"full_name": "me/zync", "name": "zync"}]"#.into(),
                );
            }
            if req.starts_with("GET /repos/org/secret/actions/runs") {
                // an ordinary 403 (SSO not authorised / Actions off): not a rate limit
                return (
                    403,
                    vec![("Content-Type", "application/json".into()), ("x-ratelimit-remaining", "4990".into())],
                    r#"{"message":"Resource protected by organization SAML enforcement"}"#.into(),
                );
            }
            (
                200,
                vec![("Content-Type", "application/json".into()), ("x-ratelimit-remaining", "4980".into())],
                r#"{"workflow_runs":[{"id":7,"event":"push","status":"completed","conclusion":"failure","updated_at":"2026-10-25T12:00:00Z"}]}"#.into(),
            )
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-10-25T13:00:00Z").unwrap().timestamp_millis();

        let facts = poll_once(&client, &base, "tok", &mut state, now_ms)
            .await
            .expect("a 403 on one repo must not fail the whole poll");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].0.project, "zync");
        assert!(facts[0].1, "first sight of the repo is a quiet baseline");
        stop.store(true, Ordering::Relaxed);
    }
}
