//! Deploy watchers for Vercel, Render and Netlify (task P3).
//!
//! Watches the latest production deploy of each project for each connected service.
//! Feeds the notification state machine (`notify_brain`):
//! - First failure -> announced once ("{Service} deploy failed in {project}, sir.").
//! - Further failures -> silent.
//! - Recovery -> announced once ("{project} is live again, sir.").
//! - Baseline (connect/startup) -> recorded quietly without speaking.
//!
//! Deterministic logic, 0 AI credits, tokens never logged.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use crate::notify_brain::{Fact, Kind, Obs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployRun {
    pub project: String,
    pub id: String,
    pub obs: Option<Obs>,
    pub at_ms: i64,
}

fn parse_ts_ms(val: &serde_json::Value) -> Option<i64> {
    if let Some(n) = val.as_i64() {
        return Some(n);
    }
    if let Some(f) = val.as_f64() {
        return Some(f as i64);
    }
    if let Some(s) = val.as_str() {
        if let Ok(n) = s.parse::<i64>() {
            return Some(n);
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp_millis());
        }
    }
    None
}

fn obs_of_vercel(state: &str) -> Option<Obs> {
    match state.to_ascii_uppercase().as_str() {
        "READY" => Some(Obs::Fixed),
        "ERROR" => Some(Obs::Failed),
        _ => None,
    }
}

pub fn parse_teams_vercel(json: &str) -> Vec<String> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val.get("teams").and_then(|v| v.as_array()).or_else(|| val.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            out.push(id.to_string());
            if out.len() >= 3 {
                break;
            }
        }
    }
    out
}

pub fn parse_vercel(json: &str) -> Vec<DeployRun> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val
        .get("deployments")
        .and_then(|v| v.as_array())
        .or_else(|| val.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let Some(id) = item
            .get("uid")
            .or_else(|| item.get("id"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        let Some(project) = item
            .get("name")
            .or_else(|| item.get("project"))
            .and_then(|v| v.as_str())
        else {
            continue;
        };
        if let Some(target) = item.get("target").and_then(|v| v.as_str()) {
            if target != "production" {
                continue;
            }
        }
        let Some(at_ms) = item.get("created").and_then(parse_ts_ms) else {
            continue;
        };
        let state = item
            .get("state")
            .or_else(|| item.get("readyState"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let obs = obs_of_vercel(state);
        out.push(DeployRun {
            project: project.to_string(),
            id: id.to_string(),
            obs,
            at_ms,
        });
    }
    out
}

fn obs_of_render(status: &str) -> Option<Obs> {
    match status.to_ascii_lowercase().as_str() {
        "live" => Some(Obs::Fixed),
        "build_failed" | "update_failed" | "pre_deploy_failed" => Some(Obs::Failed),
        _ => None,
    }
}

pub fn parse_services_render(json: &str) -> Vec<(String, String)> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val.as_array().or_else(|| val.get("services").and_then(|v| v.as_array()));
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let obj = item.get("service").unwrap_or(item);
        let suspended = obj
            .get("suspended")
            .map(|v| v.as_bool() == Some(true) || v.as_str() == Some("suspended"))
            .unwrap_or(false);
        if suspended {
            continue;
        }
        let svc_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if svc_type.to_lowercase().contains("cron") {
            continue;
        }
        let Some(id) = obj.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(name) = obj.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !id.is_empty() && !name.is_empty() {
            out.push((id.to_string(), name.to_string()));
            if out.len() >= 10 {
                break;
            }
        }
    }
    out
}

pub fn parse_render(project: &str, json: &str) -> Vec<DeployRun> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val
        .get("deploys")
        .and_then(|v| v.as_array())
        .or_else(|| val.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let deploy = item.get("deploy").unwrap_or(item);
        let Some(id) = deploy.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let proj = if !project.is_empty() {
            project.to_string()
        } else {
            deploy
                .get("name")
                .or_else(|| deploy.get("project"))
                .or_else(|| item.get("name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let Some(at_ms) = deploy
            .get("createdAt")
            .or_else(|| deploy.get("created_at"))
            .and_then(parse_ts_ms)
        else {
            continue;
        };
        let status = deploy.get("status").and_then(|v| v.as_str()).unwrap_or("");
        let obs = obs_of_render(status);
        out.push(DeployRun {
            project: proj,
            id: id.to_string(),
            obs,
            at_ms,
        });
    }
    out
}

fn obs_of_netlify(state: &str) -> Option<Obs> {
    match state.to_ascii_lowercase().as_str() {
        "ready" => Some(Obs::Fixed),
        "error" => Some(Obs::Failed),
        _ => None,
    }
}

pub fn parse_sites_netlify(json: &str) -> Vec<(String, String)> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val.as_array().or_else(|| val.get("sites").and_then(|v| v.as_array()));
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        let Some(id) = item.get("id").or_else(|| item.get("site_id")).and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(name) = item.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        if !id.is_empty() && !name.is_empty() {
            out.push((id.to_string(), name.to_string()));
            if out.len() >= 10 {
                break;
            }
        }
    }
    out
}

/// Supabase project status: healthy is the good state, the three *_FAILED states are failures.
/// Everything else (COMING_UP, PAUSING, PAUSED, INACTIVE, UPGRADING ...) is transitional or a
/// choice of the owner and is never announced here.
fn obs_of_supabase(status: &str) -> Option<Obs> {
    match status.to_ascii_uppercase().as_str() {
        "ACTIVE_HEALTHY" => Some(Obs::Fixed),
        "INIT_FAILED" | "RESTORE_FAILED" | "PAUSE_FAILED" => Some(Obs::Failed),
        _ => None,
    }
}

/// `GET /v1/projects`: one entry per project, no status-change time, so `now_ms` stands in and the
/// status text is the run reference (a repeated failing status stays silent).
pub fn parse_supabase(json: &str, now_ms: i64) -> Vec<DeployRun> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str()?.trim();
            let status = p.get("status")?.as_str()?;
            if name.is_empty() {
                return None;
            }
            Some(DeployRun {
                project: name.to_string(),
                id: status.to_ascii_uppercase(),
                obs: obs_of_supabase(status),
                at_ms: now_ms,
            })
        })
        .collect()
}

pub fn parse_netlify(project: &str, json: &str) -> Vec<DeployRun> {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let arr = val
        .get("deploys")
        .and_then(|v| v.as_array())
        .or_else(|| val.as_array());
    let Some(arr) = arr else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let context = item.get("context").and_then(|v| v.as_str()).unwrap_or("");
        if context != "production" {
            continue;
        }
        let Some(id) = item.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        let proj = if !project.is_empty() {
            project.to_string()
        } else {
            item.get("name")
                .or_else(|| item.get("site_name"))
                .or_else(|| item.get("project"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let Some(at_ms) = item
            .get("created_at")
            .or_else(|| item.get("createdAt"))
            .and_then(parse_ts_ms)
        else {
            continue;
        };
        let state = item.get("state").and_then(|v| v.as_str()).unwrap_or("");
        let obs = obs_of_netlify(state);
        out.push(DeployRun {
            project: proj,
            id: id.to_string(),
            obs,
            at_ms,
        });
    }
    out
}

pub fn facts_for(service: &'static str, runs: &[DeployRun], now_ms: i64) -> Vec<Fact> {
    const MAX_AGE_MS: i64 = 7 * 24 * 3600 * 1000;
    let mut projects: Vec<String> = runs.iter().map(|r| r.project.clone()).collect();
    projects.sort();
    projects.dedup();

    let mut facts = Vec::new();
    for project in projects {
        let candidate = runs
            .iter()
            .filter(|r| r.project == project)
            .filter(|r| r.obs.is_some())
            .filter(|r| r.at_ms > 0 && now_ms.saturating_sub(r.at_ms) <= MAX_AGE_MS)
            .max_by(|a, b| a.at_ms.cmp(&b.at_ms).then_with(|| a.id.cmp(&b.id)));

        if let Some((run, obs)) = candidate.and_then(|r| r.obs.map(|o| (r, o))) {
            facts.push(Fact {
                service,
                project: project.clone(),
                kind: Kind::Deploy,
                obs,
                run_ref: run.id.clone(),
            });
        }
    }
    facts
}

pub(crate) fn next_delay_secs(fails: u32, rate_limited: bool, remaining: Option<u32>) -> u64 {
    if rate_limited || remaining.map_or(false, |r| r <= 50) {
        return 15 * 60;
    }
    if fails == 0 {
        return 300;
    }
    (300u64.saturating_mul(1u64 << fails.min(6))).min(30 * 60)
}

#[derive(Debug, PartialEq, Eq)]
pub enum PollError {
    Auth,
    RateLimited,
    Other(String),
}

#[derive(Default)]
pub struct WatchState {
    pub seen: HashSet<String>,
    pub rate_limit_remaining: Option<u32>,
}

fn update_rate_limit(resp: &reqwest::Response, state: &mut WatchState) {
    if let Some(rem) = resp
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.parse::<u32>().ok())
    {
        state.rate_limit_remaining = Some(rem);
    }
}

pub async fn poll_service(
    client: &reqwest::Client,
    base: &str,
    service: &str,
    token: &str,
    state: &mut WatchState,
    now_ms: i64,
) -> Result<Vec<(Fact, bool)>, PollError> {
    let base = base.trim_end_matches('/');
    let mut all_runs = Vec::new();

    match service {
        "vercel" => {
            let teams_url = format!("{base}/v2/teams?limit=5");
            let teams_req = client
                .get(&teams_url)
                .header("User-Agent", "nexus-assistant")
                .header("Authorization", format!("Bearer {token}"));

            let teams_resp = teams_req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
            update_rate_limit(&teams_resp, state);
            let status = teams_resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                return Err(PollError::Auth);
            }
            if crate::memcore::github_watch::is_rate_limited(
                status,
                state.rate_limit_remaining,
                teams_resp.headers().contains_key("retry-after"),
            ) {
                return Err(PollError::RateLimited);
            }

            let mut team_ids = Vec::new();
            if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::NOT_FOUND {
                // Personal-only token
                tracing::debug!("[DEPLOY-WATCH] vercel teams not accessible ({status}); personal only");
            } else if status.is_success() {
                let text = teams_resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
                team_ids = parse_teams_vercel(&text);
            }

            let mut endpoints = vec![format!("{base}/v6/deployments?limit=20&target=production")];
            for tid in team_ids {
                endpoints.push(format!("{base}/v6/deployments?limit=20&target=production&teamId={tid}"));
            }

            for url in endpoints {
                let req = client
                    .get(&url)
                    .header("User-Agent", "nexus-assistant")
                    .header("Authorization", format!("Bearer {token}"));
                let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
                update_rate_limit(&resp, state);
                let st = resp.status();
                if st == reqwest::StatusCode::UNAUTHORIZED {
                    return Err(PollError::Auth);
                }
                if crate::memcore::github_watch::is_rate_limited(
                    st,
                    state.rate_limit_remaining,
                    resp.headers().contains_key("retry-after"),
                ) {
                    return Err(PollError::RateLimited);
                }
                if matches!(
                    st,
                    reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
                ) {
                    tracing::debug!("[DEPLOY-WATCH] vercel deployments skipped ({st})");
                    continue;
                }
                if !st.is_success() {
                    return Err(PollError::Other(format!("vercel deployments returned {st}")));
                }
                let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
                all_runs.extend(parse_vercel(&text));
            }
        }
        "render" => {
            let services_url = format!("{base}/v1/services?limit=20");
            let req = client
                .get(&services_url)
                .header("User-Agent", "nexus-assistant")
                .header("Authorization", format!("Bearer {token}"));
            let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
            update_rate_limit(&resp, state);
            let st = resp.status();
            if st == reqwest::StatusCode::UNAUTHORIZED {
                return Err(PollError::Auth);
            }
            if crate::memcore::github_watch::is_rate_limited(
                st,
                state.rate_limit_remaining,
                resp.headers().contains_key("retry-after"),
            ) {
                return Err(PollError::RateLimited);
            }
            if !st.is_success() {
                return Err(PollError::Other(format!("render services returned {st}")));
            }
            let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
            let services = parse_services_render(&text);

            for (service_id, service_name) in services {
                let deploys_url = format!("{base}/v1/services/{service_id}/deploys?limit=5");
                let req = client
                    .get(&deploys_url)
                    .header("User-Agent", "nexus-assistant")
                    .header("Authorization", format!("Bearer {token}"));
                let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
                update_rate_limit(&resp, state);
                let st = resp.status();
                if st == reqwest::StatusCode::UNAUTHORIZED {
                    return Err(PollError::Auth);
                }
                if crate::memcore::github_watch::is_rate_limited(
                    st,
                    state.rate_limit_remaining,
                    resp.headers().contains_key("retry-after"),
                ) {
                    return Err(PollError::RateLimited);
                }
                if matches!(
                    st,
                    reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
                ) {
                    tracing::debug!("[DEPLOY-WATCH] render deploys for {service_name} skipped ({st})");
                    continue;
                }
                if !st.is_success() {
                    return Err(PollError::Other(format!("render deploys for {service_name} returned {st}")));
                }
                let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
                all_runs.extend(parse_render(&service_name, &text));
            }
        }
        "netlify" => {
            let sites_url = format!("{base}/sites?per_page=20&filter=all");
            let req = client
                .get(&sites_url)
                .header("User-Agent", "nexus-assistant")
                .header("Authorization", format!("Bearer {token}"));
            let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
            update_rate_limit(&resp, state);
            let st = resp.status();
            if st == reqwest::StatusCode::UNAUTHORIZED {
                return Err(PollError::Auth);
            }
            if crate::memcore::github_watch::is_rate_limited(
                st,
                state.rate_limit_remaining,
                resp.headers().contains_key("retry-after"),
            ) {
                return Err(PollError::RateLimited);
            }
            if !st.is_success() {
                return Err(PollError::Other(format!("netlify sites returned {st}")));
            }
            let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
            let sites = parse_sites_netlify(&text);

            for (site_id, site_name) in sites {
                let deploys_url = format!("{base}/sites/{site_id}/deploys?per_page=5");
                let req = client
                    .get(&deploys_url)
                    .header("User-Agent", "nexus-assistant")
                    .header("Authorization", format!("Bearer {token}"));
                let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
                update_rate_limit(&resp, state);
                let st = resp.status();
                if st == reqwest::StatusCode::UNAUTHORIZED {
                    return Err(PollError::Auth);
                }
                if crate::memcore::github_watch::is_rate_limited(
                    st,
                    state.rate_limit_remaining,
                    resp.headers().contains_key("retry-after"),
                ) {
                    return Err(PollError::RateLimited);
                }
                if matches!(
                    st,
                    reqwest::StatusCode::FORBIDDEN | reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::GONE
                ) {
                    tracing::debug!("[DEPLOY-WATCH] netlify deploys for {site_name} skipped ({st})");
                    continue;
                }
                if !st.is_success() {
                    return Err(PollError::Other(format!("netlify deploys for {site_name} returned {st}")));
                }
                let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
                all_runs.extend(parse_netlify(&site_name, &text));
            }
        }
        "supabase" => {
            let url = format!("{base}/v1/projects");
            let req = client
                .get(&url)
                .header("User-Agent", "nexus-assistant")
                .header("Authorization", format!("Bearer {token}"));
            let resp = req.send().await.map_err(|e| PollError::Other(e.to_string()))?;
            update_rate_limit(&resp, state);
            let st = resp.status();
            if st == reqwest::StatusCode::UNAUTHORIZED {
                return Err(PollError::Auth);
            }
            if crate::memcore::github_watch::is_rate_limited(
                st,
                state.rate_limit_remaining,
                resp.headers().contains_key("retry-after"),
            ) {
                return Err(PollError::RateLimited);
            }
            if !st.is_success() {
                return Err(PollError::Other(format!("supabase projects returned {st}")));
            }
            let text = resp.text().await.map_err(|e| PollError::Other(e.to_string()))?;
            all_runs.extend(parse_supabase(&text, now_ms));
        }
        _ => return Err(PollError::Other(format!("unknown deploy service: {service}"))),
    }

    let static_service: &'static str = match service {
        "vercel" => "vercel",
        "render" => "render",
        "netlify" => "netlify",
        "supabase" => "supabase",
        _ => "deploy",
    };
    let facts = facts_for(static_service, &all_runs, now_ms);
    let mut out = Vec::with_capacity(facts.len());
    for fact in facts {
        let key = format!("{}:{}", service, fact.project.to_lowercase());
        let is_first = !state.seen.contains(&key);
        if is_first {
            state.seen.insert(key);
        }
        out.push((fact, is_first));
    }

    Ok(out)
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

fn capitalize_service(service: &str) -> String {
    let mut chars = service.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn note_auth_state<R: Runtime>(app: &AppHandle<R>, dir: &std::path::Path, service: &str, needs_token: bool) {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let meta_key = format!("deploy_needs_signin_{service}");
    let alert_day_key = format!("deploy_signin_alert_day_{service}");
    let say = crate::memcore::with_store(dir, |s| {
        let was = s.meta_get(&meta_key).map(|v| !v.is_empty()).unwrap_or(false);
        if !needs_token {
            if was {
                s.meta_set(&meta_key, "");
            }
            return false;
        }
        s.meta_set(&meta_key, "1");
        let told = s.meta_get(&alert_day_key).as_deref() == Some(today.as_str());
        if !told {
            s.meta_set(&alert_day_key, &today);
        }
        !told
    })
    .unwrap_or(false);
    if say {
        let addr = speak_address(app);
        let svc_cap = capitalize_service(service);
        let raw = format!("{svc_cap} needs a new token");
        let text = crate::memcore::mailwording::tail(addr.as_deref(), &raw);
        crate::proactive_policy::submit(
            app,
            format!("deploy_signin_{service}"),
            text,
            crate::google::types::AlertUrgency::Medium,
        );
    }
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let mut next_due: HashMap<String, Instant> = HashMap::new();
        let mut failures: HashMap<String, u32> = HashMap::new();
        let mut noted_auth: HashMap<String, bool> = HashMap::new();
        let mut logged_no_token: HashSet<String> = HashSet::new();
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
            if !crate::memcore::enabled(&dir) || !crate::memcore::flag(&dir, "memcoreDeployWatch", true) {
                continue;
            }
            if crate::focus_mode::paused() {
                crate::focus_mode::defer_log("deploy_watch");
                continue;
            }

            for service in ["vercel", "render", "netlify", "supabase"] {
                let due = next_due.get(service).copied().unwrap_or_else(Instant::now);
                if Instant::now() < due {
                    continue;
                }

                let token = match crate::auth_vault::get_token(service) {
                    Some(t) if !t.trim().is_empty() => {
                        logged_no_token.remove(service);
                        t
                    }
                    _ => {
                        if !logged_no_token.contains(service) {
                            tracing::info!("[DEPLOY-WATCH] {service}: no token");
                            logged_no_token.insert(service.to_string());
                        }
                        next_due.insert(service.to_string(), Instant::now() + Duration::from_secs(300));
                        continue;
                    }
                };

                let base_url = match service {
                    "vercel" => "https://api.vercel.com",
                    "render" => "https://api.render.com",
                    "netlify" => "https://api.netlify.com/api/v1",
                    "supabase" => "https://api.supabase.com",
                    _ => continue,
                };

                let now_ms = chrono::Utc::now().timestamp_millis();
                let poll_res = poll_service(&client, base_url, service, &token, &mut state, now_ms).await;
                match poll_res {
                    Ok(items) => {
                        failures.insert(service.to_string(), 0);
                        noted_auth.insert(service.to_string(), false);
                        note_auth_state(&app, &dir, service, false);
                        for (fact, baseline) in items {
                            if let crate::notify_brain::Decision::Say(line) =
                                crate::notify_brain::observe_fact(&app, &fact, baseline)
                            {
                                crate::notify_brain::say(&app, service, line);
                            }
                        }
                        let delay = next_delay_secs(0, false, state.rate_limit_remaining);
                        next_due.insert(service.to_string(), Instant::now() + Duration::from_secs(delay));
                    }
                    Err(PollError::Auth) => {
                        let fails = failures.entry(service.to_string()).or_insert(0);
                        *fails += 1;
                        note_auth_state(&app, &dir, service, true);
                        if !noted_auth.get(service).copied().unwrap_or(false) {
                            noted_auth.insert(service.to_string(), true);
                            tracing::warn!("[DEPLOY-WATCH] {service}: authentication failed; token needed");
                        }
                        let delay = next_delay_secs(*fails, false, state.rate_limit_remaining);
                        next_due.insert(service.to_string(), Instant::now() + Duration::from_secs(delay));
                    }
                    Err(PollError::RateLimited) => {
                        let fails = failures.entry(service.to_string()).or_insert(0);
                        *fails += 1;
                        tracing::warn!("[DEPLOY-WATCH] {service}: rate limit reached; backing off");
                        let delay = next_delay_secs(*fails, true, state.rate_limit_remaining);
                        next_due.insert(service.to_string(), Instant::now() + Duration::from_secs(delay));
                    }
                    Err(PollError::Other(e)) => {
                        let fails = failures.entry(service.to_string()).or_insert(0);
                        *fails += 1;
                        tracing::debug!("[DEPLOY-WATCH] {service}: poll error: {e}");
                        let delay = next_delay_secs(*fails, false, state.rate_limit_remaining);
                        next_due.insert(service.to_string(), Instant::now() + Duration::from_secs(delay));
                    }
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
    fn test_next_delay_secs() {
        assert_eq!(next_delay_secs(0, false, None), 300);
        assert_eq!(next_delay_secs(0, false, Some(100)), 300);
        assert_eq!(next_delay_secs(0, false, Some(50)), 900);
        assert_eq!(next_delay_secs(0, false, Some(10)), 900);
        assert_eq!(next_delay_secs(0, true, None), 900);
        assert_eq!(next_delay_secs(1, false, None), 600);
        assert_eq!(next_delay_secs(2, false, None), 1200);
        assert_eq!(next_delay_secs(3, false, None), 1800);
        assert_eq!(next_delay_secs(10, false, None), 1800);
    }

    #[test]
    fn test_parse_vercel_realistic() {
        let json = r#"{
            "deployments": [
                {
                    "uid": "d_101",
                    "name": "zync",
                    "state": "READY",
                    "target": "production",
                    "created": 1729850000000
                },
                {
                    "uid": "d_102",
                    "name": "zync",
                    "state": "ERROR",
                    "target": "production",
                    "created": 1729851000000
                },
                {
                    "uid": "d_103",
                    "name": "nexus-web",
                    "readyState": "BUILDING",
                    "target": "production",
                    "created": 1729852000000
                },
                {
                    "uid": "d_104",
                    "name": "preview-app",
                    "state": "READY",
                    "target": "preview",
                    "created": 1729853000000
                },
                {
                    "uid": "d_105",
                    "name": "old-app",
                    "state": "CANCELED",
                    "target": "production",
                    "created": 1729854000000
                }
            ]
        }"#;

        let runs = parse_vercel(json);
        assert_eq!(runs.len(), 4, "preview target must be skipped");
        assert_eq!(runs[0].project, "zync");
        assert_eq!(runs[0].id, "d_101");
        assert_eq!(runs[0].obs, Some(Obs::Fixed));

        assert_eq!(runs[1].project, "zync");
        assert_eq!(runs[1].id, "d_102");
        assert_eq!(runs[1].obs, Some(Obs::Failed));

        assert_eq!(runs[2].project, "nexus-web");
        assert_eq!(runs[2].obs, None, "BUILDING must map to None");

        assert_eq!(runs[3].project, "old-app");
        assert_eq!(runs[3].obs, None, "CANCELED must map to None");
    }

    #[test]
    fn test_parse_teams_vercel() {
        let json = r#"{
            "teams": [
                {"id": "team_1", "name": "Team One"},
                {"id": "team_2", "name": "Team Two"},
                {"id": "team_3", "name": "Team Three"},
                {"id": "team_4", "name": "Team Four"}
            ]
        }"#;
        let teams = parse_teams_vercel(json);
        assert_eq!(teams, vec!["team_1", "team_2", "team_3"], "teams cap at 3");
    }

    #[test]
    fn test_parse_render_realistic() {
        let json = r#"[
            {
                "deploy": {
                    "id": "dep_1",
                    "status": "live",
                    "createdAt": "2026-10-25T12:00:00Z"
                }
            },
            {
                "deploy": {
                    "id": "dep_2",
                    "status": "build_failed",
                    "createdAt": "2026-10-25T12:10:00Z"
                }
            },
            {
                "deploy": {
                    "id": "dep_3",
                    "status": "update_failed",
                    "createdAt": "2026-10-25T12:20:00Z"
                }
            },
            {
                "deploy": {
                    "id": "dep_4",
                    "status": "pre_deploy_failed",
                    "createdAt": "2026-10-25T12:30:00Z"
                }
            },
            {
                "deploy": {
                    "id": "dep_5",
                    "status": "build_in_progress",
                    "createdAt": "2026-10-25T12:40:00Z"
                }
            },
            {
                "deploy": {
                    "id": "dep_6",
                    "status": "canceled",
                    "createdAt": "2026-10-25T12:50:00Z"
                }
            }
        ]"#;

        let runs = parse_render("zync", json);
        assert_eq!(runs.len(), 6);
        assert_eq!(runs[0].project, "zync");
        assert_eq!(runs[0].id, "dep_1");
        assert_eq!(runs[0].obs, Some(Obs::Fixed));

        assert_eq!(runs[1].id, "dep_2");
        assert_eq!(runs[1].obs, Some(Obs::Failed));

        assert_eq!(runs[2].id, "dep_3");
        assert_eq!(runs[2].obs, Some(Obs::Failed));

        assert_eq!(runs[3].id, "dep_4");
        assert_eq!(runs[3].obs, Some(Obs::Failed));

        assert_eq!(runs[4].id, "dep_5");
        assert_eq!(runs[4].obs, None, "build_in_progress must map to None");

        assert_eq!(runs[5].id, "dep_6");
        assert_eq!(runs[5].obs, None, "canceled must map to None");
    }

    #[test]
    fn test_parse_services_render() {
        let json = r#"[
            {"service": {"id": "srv_1", "name": "web-api", "type": "web_service", "suspended": "not_suspended"}},
            {"service": {"id": "srv_2", "name": "worker", "type": "background_worker", "suspended": "not_suspended"}},
            {"service": {"id": "srv_3", "name": "cron-daily", "type": "cron_job", "suspended": "not_suspended"}},
            {"service": {"id": "srv_4", "name": "old-site", "type": "static_site", "suspended": "suspended"}},
            {"service": {"id": "srv_5", "name": "paused-app", "type": "web_service", "suspended": true}}
        ]"#;
        let svcs = parse_services_render(json);
        assert_eq!(svcs.len(), 2);
        assert_eq!(svcs[0], ("srv_1".to_string(), "web-api".to_string()));
        assert_eq!(svcs[1], ("srv_2".to_string(), "worker".to_string()));
    }

    #[test]
    fn test_parse_netlify_realistic() {
        let json = r#"[
            {
                "id": "net_1",
                "state": "ready",
                "context": "production",
                "created_at": "2026-10-25T10:00:00Z"
            },
            {
                "id": "net_2",
                "state": "error",
                "context": "production",
                "created_at": "2026-10-25T10:30:00Z"
            },
            {
                "id": "net_3",
                "state": "building",
                "context": "production",
                "created_at": "2026-10-25T11:00:00Z"
            },
            {
                "id": "net_4",
                "state": "ready",
                "context": "deploy-preview",
                "created_at": "2026-10-25T11:30:00Z"
            }
        ]"#;

        let runs = parse_netlify("zync", json);
        assert_eq!(runs.len(), 3, "deploy-preview context must be ignored");
        assert_eq!(runs[0].project, "zync");
        assert_eq!(runs[0].id, "net_1");
        assert_eq!(runs[0].obs, Some(Obs::Fixed));

        assert_eq!(runs[1].id, "net_2");
        assert_eq!(runs[1].obs, Some(Obs::Failed));

        assert_eq!(runs[2].id, "net_3");
        assert_eq!(runs[2].obs, None, "building must map to None");
    }

    #[test]
    fn test_parse_sites_netlify() {
        let json = r#"[
            {"id": "site_1", "name": "zync-prod"},
            {"id": "site_2", "name": "landing-page"},
            {"id": "site_3", "name": "docs"}
        ]"#;
        let sites = parse_sites_netlify(json);
        assert_eq!(sites.len(), 3);
        assert_eq!(sites[0], ("site_1".to_string(), "zync-prod".to_string()));
        assert_eq!(sites[1], ("site_2".to_string(), "landing-page".to_string()));
        assert_eq!(sites[2], ("site_3".to_string(), "docs".to_string()));
    }

    #[test]
    fn test_facts_for_running_and_cancelled_handling() {
        let now = 1_000_000_000i64;
        let runs = vec![
            DeployRun {
                project: "zync".into(),
                id: "d1".into(),
                obs: Some(Obs::Failed),
                at_ms: now - 3600 * 1000,
            },
            DeployRun {
                project: "zync".into(),
                id: "d2".into(),
                obs: None, // newest deploy cancelled: must not hide older terminal d1
                at_ms: now - 1800 * 1000,
            },
            DeployRun {
                project: "nexus".into(),
                id: "n1".into(),
                obs: None, // running deploy: never counts
                at_ms: now - 600 * 1000,
            },
        ];

        let facts = facts_for("vercel", &runs, now);
        assert_eq!(facts.len(), 1, "nexus running deploy must yield no facts");
        assert_eq!(facts[0].service, "vercel");
        assert_eq!(facts[0].project, "zync");
        assert_eq!(facts[0].obs, Obs::Failed);
        assert_eq!(facts[0].run_ref, "d1");
    }

    #[test]
    fn test_facts_for_staleness() {
        let now = 1_000_000_000i64;
        let eight_days_ms = 8 * 24 * 3600 * 1000;
        let runs = vec![DeployRun {
            project: "zync".into(),
            id: "d_old".into(),
            obs: Some(Obs::Fixed),
            at_ms: now - eight_days_ms,
        }];

        let facts = facts_for("render", &runs, now);
        assert!(facts.is_empty(), "runs older than 7 days must be ignored as stale");
    }

    #[test]
    fn test_facts_for_ties_and_ordering() {
        let now = 1_000_000_000i64;
        let ts = now - 1000;
        let runs = vec![
            DeployRun {
                project: "zync".into(),
                id: "dep_1".into(),
                obs: Some(Obs::Failed),
                at_ms: ts,
            },
            DeployRun {
                project: "zync".into(),
                id: "dep_2".into(),
                obs: Some(Obs::Fixed),
                at_ms: ts, // same timestamp: tie break by id dep_2
            },
            DeployRun {
                project: "app".into(),
                id: "app_1".into(),
                obs: Some(Obs::Fixed),
                at_ms: ts,
            },
        ];

        let facts = facts_for("netlify", &runs, now);
        assert_eq!(facts.len(), 2);
        // Sorted by project: app, then zync
        assert_eq!(facts[0].project, "app");
        assert_eq!(facts[0].run_ref, "app_1");

        assert_eq!(facts[1].project, "zync");
        assert_eq!(facts[1].run_ref, "dep_2");
        assert_eq!(facts[1].obs, Obs::Fixed);
    }

    #[test]
    fn supabase_statuses_map_to_failed_fixed_or_nothing() {
        let json = r#"[{"id":"a","name":"zync","status":"ACTIVE_HEALTHY"},{"id":"b","name":"ledger","status":"INIT_FAILED"},{"id":"c","name":"old","status":"PAUSED"},{"id":"d","name":"up","status":"COMING_UP"},{"name":"","status":"INIT_FAILED"}]"#;
        let runs = parse_supabase(json, 1000);
        let by = |n: &str| runs.iter().find(|r| r.project == n).unwrap().obs;
        assert_eq!(runs.len(), 4);
        assert_eq!(by("zync"), Some(Obs::Fixed));
        assert_eq!(by("ledger"), Some(Obs::Failed));
        assert_eq!(by("old"), None);
        assert_eq!(by("up"), None);
        assert!(parse_supabase("not json", 1).is_empty());
    }

    #[tokio::test]
    async fn test_poll_service_supabase_flow() {
        let cycle = Arc::new(Mutex::new(0usize));
        let cycle_clone = cycle.clone();
        let (base, _requests, stop) = start_mock_server(move |req| {
            if req.starts_with("GET /v1/projects") {
                let body = if *cycle_clone.lock().unwrap() == 0 {
                    r#"[{"id":"p1","name":"zync","status":"ACTIVE_HEALTHY"}]"#
                } else {
                    r#"[{"id":"p1","name":"zync","status":"RESTORE_FAILED"}]"#
                };
                return (200, vec![("Content-Type", "application/json".into())], body.into());
            }
            (404, vec![], "{}".into())
        });
        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let f1 = poll_service(&client, &base, "supabase", "sbp_tok", &mut state, 5000).await.unwrap();
        assert_eq!(f1.len(), 1);
        assert!(f1[0].1, "first sight is the quiet baseline");
        assert_eq!(f1[0].0.service, "supabase");
        *cycle.lock().unwrap() = 1;
        let f2 = poll_service(&client, &base, "supabase", "sbp_tok", &mut state, 6000).await.unwrap();
        assert_eq!(f2[0].0.obs, Obs::Failed);
        assert_eq!(f2[0].0.run_ref, "RESTORE_FAILED");
        assert!(!f2[0].1);
        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_service_vercel_flow() {
        let cycle = Arc::new(Mutex::new(0usize));
        let cycle_clone = cycle.clone();

        let (base, _requests, stop) = start_mock_server(move |req| {
            if req.starts_with("GET /v2/teams") {
                return (200, vec![("Content-Type", "application/json".into())], r#"{"teams":[]}"#.into());
            }
            if req.starts_with("GET /v6/deployments") {
                let c = *cycle_clone.lock().unwrap();
                if c == 0 {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"{"deployments":[{"uid":"v1","name":"zync","state":"READY","target":"production","created":1729850000000}]}"#.into(),
                    );
                } else {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"{"deployments":[{"uid":"v2","name":"zync","state":"ERROR","target":"production","created":1729851000000}]}"#.into(),
                    );
                }
            }
            (404, vec![], "{}".into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = 1729852000000i64;

        // 1. First call: baseline == true
        let facts1 = poll_service(&client, &base, "vercel", "tok_v", &mut state, now_ms)
            .await
            .expect("first poll should succeed");
        assert_eq!(facts1.len(), 1);
        assert_eq!(facts1[0].0.project, "zync");
        assert_eq!(facts1[0].0.obs, Obs::Fixed);
        assert!(facts1[0].1, "first call must set baseline == true");

        // 2. Second call with newer failing deploy: baseline == false
        *cycle.lock().unwrap() = 1;
        let facts2 = poll_service(&client, &base, "vercel", "tok_v", &mut state, now_ms)
            .await
            .expect("second poll should succeed");
        assert_eq!(facts2.len(), 1);
        assert_eq!(facts2[0].0.project, "zync");
        assert_eq!(facts2[0].0.obs, Obs::Failed);
        assert_eq!(facts2[0].0.run_ref, "v2");
        assert!(!facts2[0].1, "subsequent call must set baseline == false");

        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_service_render_flow() {
        let cycle = Arc::new(Mutex::new(0usize));
        let cycle_clone = cycle.clone();

        let (base, _requests, stop) = start_mock_server(move |req| {
            if req.starts_with("GET /v1/services?") || req == "GET /v1/services" {
                return (
                    200,
                    vec![("Content-Type", "application/json".into())],
                    r#"[{"service":{"id":"srv_1","name":"zync","type":"web_service","suspended":"not_suspended"}}]"#.into(),
                );
            }
            if req.starts_with("GET /v1/services/srv_1/deploys") {
                let c = *cycle_clone.lock().unwrap();
                if c == 0 {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"[{"deploy":{"id":"r1","status":"live","createdAt":"2026-10-25T12:00:00Z"}}]"#.into(),
                    );
                } else {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"[{"deploy":{"id":"r2","status":"build_failed","createdAt":"2026-10-25T12:30:00Z"}}]"#.into(),
                    );
                }
            }
            (404, vec![], "{}".into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-10-25T13:00:00Z").unwrap().timestamp_millis();

        // 1. First call: baseline == true
        let facts1 = poll_service(&client, &base, "render", "tok_r", &mut state, now_ms)
            .await
            .expect("first render poll should succeed");
        assert_eq!(facts1.len(), 1);
        assert_eq!(facts1[0].0.project, "zync");
        assert_eq!(facts1[0].0.obs, Obs::Fixed);
        assert!(facts1[0].1, "first call must set baseline == true");

        // 2. Second call with newer failing deploy: baseline == false
        *cycle.lock().unwrap() = 1;
        let facts2 = poll_service(&client, &base, "render", "tok_r", &mut state, now_ms)
            .await
            .expect("second render poll should succeed");
        assert_eq!(facts2.len(), 1);
        assert_eq!(facts2[0].0.project, "zync");
        assert_eq!(facts2[0].0.obs, Obs::Failed);
        assert_eq!(facts2[0].0.run_ref, "r2");
        assert!(!facts2[0].1, "subsequent call must set baseline == false");

        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_service_netlify_flow() {
        let cycle = Arc::new(Mutex::new(0usize));
        let cycle_clone = cycle.clone();

        let (base, _requests, stop) = start_mock_server(move |req| {
            if req.starts_with("GET /sites?") || req == "GET /sites" {
                return (
                    200,
                    vec![("Content-Type", "application/json".into())],
                    r#"[{"id":"site_1","name":"zync"}]"#.into(),
                );
            }
            if req.starts_with("GET /sites/site_1/deploys") {
                let c = *cycle_clone.lock().unwrap();
                if c == 0 {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"[{"id":"n1","state":"ready","context":"production","created_at":"2026-10-25T12:00:00Z"}]"#.into(),
                    );
                } else {
                    return (
                        200,
                        vec![("Content-Type", "application/json".into())],
                        r#"[{"id":"n2","state":"error","context":"production","created_at":"2026-10-25T12:30:00Z"}]"#.into(),
                    );
                }
            }
            (404, vec![], "{}".into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-10-25T13:00:00Z").unwrap().timestamp_millis();

        // 1. First call: baseline == true
        let facts1 = poll_service(&client, &base, "netlify", "tok_n", &mut state, now_ms)
            .await
            .expect("first netlify poll should succeed");
        assert_eq!(facts1.len(), 1);
        assert_eq!(facts1[0].0.project, "zync");
        assert_eq!(facts1[0].0.obs, Obs::Fixed);
        assert!(facts1[0].1, "first call must set baseline == true");

        // 2. Second call with newer failing deploy: baseline == false
        *cycle.lock().unwrap() = 1;
        let facts2 = poll_service(&client, &base, "netlify", "tok_n", &mut state, now_ms)
            .await
            .expect("second netlify poll should succeed");
        assert_eq!(facts2.len(), 1);
        assert_eq!(facts2[0].0.project, "zync");
        assert_eq!(facts2[0].0.obs, Obs::Failed);
        assert_eq!(facts2[0].0.run_ref, "n2");
        assert!(!facts2[0].1, "subsequent call must set baseline == false");

        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_service_auth_error() {
        let (base, _, stop) = start_mock_server(|_| {
            (401, vec![("Content-Type", "application/json".into())], r#"{"message":"Bad credentials"}"#.into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = 1_000_000_000i64;

        let err = poll_service(&client, &base, "vercel", "bad_tok", &mut state, now_ms)
            .await
            .expect_err("should return auth error on 401");
        assert_eq!(err, PollError::Auth);
        stop.store(true, Ordering::Relaxed);
    }

    #[tokio::test]
    async fn test_poll_service_unreadable_project_skipped() {
        let (base, _, stop) = start_mock_server(|req| {
            if req.starts_with("GET /sites?") || req == "GET /sites" {
                return (
                    200,
                    vec![("Content-Type", "application/json".into())],
                    r#"[{"id":"site_unreadable","name":"secret"},{"id":"site_ok","name":"zync"}]"#.into(),
                );
            }
            if req.starts_with("GET /sites/site_unreadable/deploys") {
                // 403 forbidden on unreadable site
                return (403, vec![("Content-Type", "application/json".into())], r#"{"message":"Forbidden"}"#.into());
            }
            if req.starts_with("GET /sites/site_ok/deploys") {
                return (
                    200,
                    vec![("Content-Type", "application/json".into())],
                    r#"[{"id":"n_ok","state":"error","context":"production","created_at":"2026-10-25T12:00:00Z"}]"#.into(),
                );
            }
            (404, vec![], "{}".into())
        });

        let client = reqwest::Client::new();
        let mut state = WatchState::default();
        let now_ms = chrono::DateTime::parse_from_rfc3339("2026-10-25T13:00:00Z").unwrap().timestamp_millis();

        let facts = poll_service(&client, &base, "netlify", "tok", &mut state, now_ms)
            .await
            .expect("an unreadable 403 site must be skipped without failing the whole poll");
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].0.project, "zync");
        assert_eq!(facts[0].0.obs, Obs::Failed);
        assert_eq!(facts[0].0.run_ref, "n_ok");

        stop.store(true, Ordering::Relaxed);
    }
}
