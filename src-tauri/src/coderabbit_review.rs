//! CodeRabbit review pipeline for a pull request: what CodeRabbit already posted, then (free only) a server review.
//!
//! FREE ONLY. A review is started only from the included reviews of the owner's plan (`coderabbit usage --agent`:
//! `includedReviews.remaining > 0`); `--use-credits` is never passed, whatever the billing status says.
//! A repository that is NEXUS's own checkout is reviewed LOCALLY (a temporary git worktree of the PR head; works without
//! CodeRabbit knowing the GitHub organization), any other with the server-side `--remote` review.
//! Output is normalised into one `Report` with CodeRabbit's own two axes: SEVERITY (critical, major, minor, trivial,
//! info; `unknown` when it is not stated) and TYPE (potential issue, refactor suggestion, nitpick). The CLI gives no
//! type, so a derived one is flagged `kind_inferred` and shown as inferred, never as official.
//! Parsers are pure and tolerant (unknown record types and fields are ignored); the runners at the bottom are thin.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Critical,
    Major,
    Minor,
    Trivial,
    Info,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    PotentialIssue,
    Refactor,
    Nitpick,
    Unknown,
}

pub fn parse_severity(s: &str) -> Severity {
    match s.trim().to_lowercase().as_str() {
        "critical" => Severity::Critical,
        "major" => Severity::Major,
        "minor" => Severity::Minor,
        "trivial" => Severity::Trivial,
        "info" | "informational" => Severity::Info,
        _ => Severity::Unknown,
    }
}

/// (type, severity) from the first lines of a CodeRabbit PR comment, e.g. `_⚠️ Potential issue_ | _🟠 Major_`.
pub fn parse_header(body: &str) -> (Kind, Severity) {
    let head: String = body.lines().take(2).collect::<Vec<_>>().join(" ").to_lowercase();
    let kind = if head.contains("potential issue") {
        Kind::PotentialIssue
    } else if head.contains("refactor suggestion") {
        Kind::Refactor
    } else if head.contains("nitpick") {
        Kind::Nitpick
    } else {
        Kind::Unknown
    };
    let severity = ["critical", "major", "minor", "trivial"]
        .iter()
        .find(|w| head.contains(*w))
        .map(|w| parse_severity(w))
        .unwrap_or(Severity::Unknown);
    (kind, severity)
}

/// A type for a finding that has none (CLI output). The result is always marked as inferred by the caller.
pub fn infer_kind(severity: Severity, title: &str, comment: &str) -> Kind {
    let text = format!("{title} {comment}").to_lowercase();
    let risky = ["security", "vulnerab", "injection", "leak", "race condition", "panic", "crash", "null", "overflow", "unsafe", "bug"];
    if matches!(severity, Severity::Critical | Severity::Major) || risky.iter().any(|w| text.contains(w)) {
        Kind::PotentialIssue
    } else if severity == Severity::Trivial {
        Kind::Nitpick
    } else {
        Kind::Refactor
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub kind: Kind,
    pub kind_inferred: bool,
    pub file: String,
    pub start_line: Option<u32>,
    pub end_line: Option<u32>,
    pub title: String,
    pub comment: String,
    pub suggestions: Vec<String>,
    pub agent_instructions: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gate {
    pub threshold: Option<String>,
    pub status: Option<String>,
    pub blocking: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    GithubBot,
    RemoteReview,
    LocalReview,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub source: Source,
    pub summary: String,
    pub findings: Vec<Finding>,
    pub prompt_for_agents: Option<String>,
    pub gate: Option<Gate>,
    pub unreviewed_files: u32,
    pub reused: bool,
    /// Why there is no review / what the owner can do (exact, never a guess).
    pub notes: Vec<String>,
    pub usage: Option<Usage>,
}

impl Report {
    pub fn empty(note: impl Into<String>) -> Self {
        Report {
            source: Source::None,
            summary: String::new(),
            findings: Vec::new(),
            prompt_for_agents: None,
            gate: None,
            unreviewed_files: 0,
            reused: false,
            notes: vec![note.into()],
            usage: None,
        }
    }
}

fn strings(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| match x {
                Value::String(s) => Some(s.clone()),
                Value::Object(_) => x.get("code").or_else(|| x.get("text")).or_else(|| x.get("command")).and_then(Value::as_str).map(str::to_string),
                _ => None,
            })
            .collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn line_no(v: Option<&Value>) -> Option<u32> {
    v.and_then(Value::as_u64).map(|n| n as u32)
}

/// Parses `coderabbit review --agent` NDJSON. An `error` record without any finding is an `Err` with CodeRabbit's
/// own message; unknown record types and fields are ignored.
pub fn parse_review_ndjson(stdout: &str) -> Result<Report, String> {
    let mut report = Report::empty("");
    report.notes.clear();
    report.source = Source::RemoteReview;
    let mut error: Option<String> = None;
    for line in stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        match v.get("type").and_then(Value::as_str) {
            Some("finding") => {
                let severity = v.get("severity").and_then(Value::as_str).map(parse_severity).unwrap_or(Severity::Unknown);
                let comment = v.get("comment").and_then(Value::as_str).unwrap_or("").to_string();
                let title = v
                    .get("title")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| comment.lines().next().unwrap_or("CodeRabbit finding").trim_matches('*').chars().take(140).collect());
                let comment = strip_title(&comment, &title);
                let kind = infer_kind(severity, &title, &comment);
                report.findings.push(Finding {
                    id: format!("cr-{}", report.findings.len() + 1),
                    severity,
                    kind,
                    kind_inferred: true,
                    file: v.get("fileName").and_then(Value::as_str).unwrap_or("").to_string(),
                    start_line: line_no(v.get("startLine")),
                    end_line: line_no(v.get("endLine")),
                    title,
                    comment,
                    suggestions: strings(v.get("suggestions")),
                    agent_instructions: v.get("codegenInstructions").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string),
                });
            }
            Some("complete") => {
                report.unreviewed_files = v.get("unreviewedFileCount").and_then(Value::as_u64).unwrap_or(0) as u32;
                report.reused = v.get("reused").and_then(Value::as_bool).unwrap_or(false);
                if let Some(g) = v.get("gate") {
                    report.gate = Some(Gate {
                        threshold: g.get("threshold").and_then(Value::as_str).map(str::to_string),
                        status: g.get("status").and_then(Value::as_str).map(str::to_string),
                        blocking: g.get("blockingFindings").and_then(Value::as_u64).map(|n| n as u32),
                    });
                }
                match v.get("status").and_then(Value::as_str) {
                    Some("review_skipped") => report.notes.push(v.get("message").and_then(Value::as_str).unwrap_or("CodeRabbit skipped this review").to_string()),
                    _ => {
                        if let Some(m) = v.get("message").and_then(Value::as_str).filter(|_| report.findings.is_empty()) {
                            report.summary = m.to_string();
                        }
                    }
                }
                if v.get("outcome").and_then(Value::as_str) == Some("failed") {
                    error = Some("CodeRabbit reported that the review failed".to_string());
                }
            }
            Some("error") => {
                error = Some(v.get("message").and_then(Value::as_str).unwrap_or("CodeRabbit returned an error").to_string());
            }
            _ => {}
        }
    }
    if report.findings.is_empty() {
        if let Some(e) = error {
            return Err(e);
        }
    }
    if report.summary.is_empty() {
        report.summary = format!("CodeRabbit found {} item(s).", report.findings.len());
    }
    Ok(report)
}

/// CodeRabbit repeats the title as `**title**` at the start of the comment; the card already shows it.
pub fn strip_title(comment: &str, title: &str) -> String {
    let bold = format!("**{}**", title.trim());
    match comment.trim_start().strip_prefix(bold.as_str()) {
        Some(rest) => rest.trim_start().to_string(),
        None => comment.to_string(),
    }
}

/// `owner/repo` of a git remote URL (https or ssh, with or without `.git`), lower-case only for comparison by callers.
pub fn repo_from_remote_url(url: &str) -> Option<String> {
    let u = url.trim().trim_end_matches('/').trim_end_matches(".git");
    let tail = u.split("github.com").nth(1)?.trim_start_matches([':', '/']);
    let mut parts = tail.split('/');
    let (owner, name) = (parts.next()?, parts.next()?);
    (parts.next().is_none() && !owner.is_empty() && !name.is_empty()).then(|| format!("{owner}/{name}"))
}

/// Argument lists of the local review, as pure data: fetch the PR head and the base, review the committed difference.
/// None for anything that is not a plain number / branch name. Neither list can contain `--use-credits`.
pub fn local_review_plan(pr: u32, base: &str) -> Option<(Vec<String>, Vec<String>, Vec<String>)> {
    if pr == 0 || !safe_token(base, &[]) {
        return None;
    }
    let fetch_pr = ["fetch", "--quiet", "origin", &format!("pull/{pr}/head")].iter().map(|s| s.to_string()).collect();
    let fetch_base = ["fetch", "--quiet", "origin", base].iter().map(|s| s.to_string()).collect();
    let review = ["review", "--agent", "--base", &format!("origin/{base}"), "--committed"].iter().map(|s| s.to_string()).collect();
    Some((fetch_pr, fetch_base, review))
}

/// Where a repository's owner stands with the signed-in CodeRabbit account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    /// The owner is the active organization: `pullrequest` and reviews can reach it.
    ActiveOrg,
    /// The owner is one of the account's other organizations: switch with `coderabbit auth org`.
    OtherOrg,
    /// The owner is not an organization of this CodeRabbit account at all.
    UnknownOrg,
}

pub fn classify_access(owner: &str, active: &str, orgs: &[String]) -> Access {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    if same(owner, active) {
        Access::ActiveOrg
    } else if orgs.iter().any(|o| same(o, owner)) {
        Access::OtherOrg
    } else {
        Access::UnknownOrg
    }
}

/// The exact explanation and fix for an owner CodeRabbit cannot read.
pub fn access_note(owner: &str, access: Access) -> Option<String> {
    match access {
        Access::ActiveOrg => None,
        Access::OtherOrg => Some(format!(
            "CodeRabbit knows {owner}, but another organization is active. Run `coderabbit auth org` and choose {owner}."
        )),
        Access::UnknownOrg => Some(format!(
            "{owner} is not an organization of your CodeRabbit account. Add it at app.coderabbit.ai (Add repositories) to get reviews."
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub state: String,
    pub limit: u32,
    pub remaining: u32,
    pub billing_status: String,
    pub resets_at: Option<String>,
    /// Milliseconds until the whole included allowance is back (CodeRabbit's `fullCapacityInMs`; 0 when full).
    pub full_capacity_ms: u64,
}

/// `coderabbit usage --agent` (last JSON line).
pub fn parse_usage(stdout: &str) -> Option<Usage> {
    let v = stdout.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).last()?;
    let inc = v.get("includedReviews")?;
    Some(Usage {
        state: inc.get("state").and_then(Value::as_str).unwrap_or("unknown").to_string(),
        limit: inc.get("limit").and_then(Value::as_u64).unwrap_or(0) as u32,
        remaining: inc.get("remaining").and_then(Value::as_u64).unwrap_or(0) as u32,
        billing_status: v.pointer("/billingPeriod/usageBillingStatus").and_then(Value::as_str).unwrap_or("unknown").to_string(),
        resets_at: v.pointer("/billingPeriod/resetsAt").and_then(Value::as_str).map(str::to_string),
        full_capacity_ms: inc.get("fullCapacityInMs").and_then(Value::as_u64).unwrap_or(0),
    })
}

/// Free reviews only: the plan's included reviews must be available with some left. Credits are never an option.
pub fn free_review_allowed(u: &Usage) -> bool {
    u.state == "available" && u.remaining > 0
}

/// "12 min" / "under a minute" for a millisecond count.
pub fn minutes_text(ms: u64) -> String {
    match (ms + 59_999) / 60_000 {
        0 => "under a minute".to_string(),
        1 => "1 min".to_string(),
        m => format!("{m} min"),
    }
}

pub fn free_review_note(u: &Usage) -> String {
    let back = if u.full_capacity_ms > 0 { format!(", back to {} in about {}", u.limit, minutes_text(u.full_capacity_ms)) } else { String::new() };
    format!("No free CodeRabbit review left right now ({} of {} included reviews remaining{back}). NEXUS never uses paid credits.", u.remaining, u.limit)
}

/// What the owner is asked before a review is spent: the count first, then the question.
pub fn consent_line(u: &Usage, pr: u32, repo: &str) -> String {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    format!("You have {} of {} free CodeRabbit reviews left, sir. Use one on PR {pr} in {name}?", u.remaining, u.limit)
}

/// Stock lines spoken the moment a long review starts (rotated, never the same twice in a row).
pub const WORKING_LINES: [&str; 5] = ["On it, sir.", "Give me a minute, sir.", "I'll get back to you, sir.", "Running CodeRabbit now, sir.", "One moment, sir. This takes a few minutes."];

pub fn working_line(counter: usize) -> &'static str {
    WORKING_LINES[counter % WORKING_LINES.len()]
}

/// The line spoken when a review ends.
pub fn finish_line(pr: u32, r: &Report) -> String {
    match (r.source, r.findings.len()) {
        (Source::None, _) => format!("CodeRabbit could not review PR {pr}, sir. {}", r.notes.first().cloned().unwrap_or_default()).trim().to_string(),
        (_, 0) => format!("CodeRabbit has nothing to flag on PR {pr}, sir."),
        (_, 1) => format!("CodeRabbit finished: 1 finding on PR {pr}, sir."),
        (_, n) => format!("CodeRabbit finished: {n} findings on PR {pr}, sir."),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    Go,
    AlreadyRunning,
    /// No free review left or the usage could not be read: nothing is started, the text says why.
    Refused(String),
}

/// Whether a review may be started now. Spending needs a free review; a running review for the same PR is reused.
pub fn start_decision(running: bool, usage: Option<&Usage>) -> Start {
    if running {
        return Start::AlreadyRunning;
    }
    match usage {
        None => Start::Refused("I could not read your CodeRabbit usage, so I did not start a review.".to_string()),
        Some(u) if !free_review_allowed(u) => Start::Refused(free_review_note(u)),
        Some(_) => Start::Go,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Running,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub state: JobState,
    pub started_ms: u64,
    pub finished_ms: Option<u64>,
    pub report: Option<Report>,
}

fn jobs() -> &'static std::sync::Mutex<HashMap<(String, u32), Job>> {
    static J: OnceLock<std::sync::Mutex<HashMap<(String, u32), Job>>> = OnceLock::new();
    J.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// True while a review of this PR is running.
pub fn is_running(repo: &str, pr: u32) -> bool {
    jobs().lock().unwrap_or_else(|e| e.into_inner()).get(&(repo.to_lowercase(), pr)).is_some_and(|j| j.state == JobState::Running)
}

/// The next spoken "I'm working on it" line (rotates, never the same twice in a row).
pub fn next_working_line() -> &'static str {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    working_line(N.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}

/// The PR the sidebar is showing (set by the UI), so "this PR" can be resolved from a spoken command.
fn context() -> &'static std::sync::Mutex<Option<(String, u32)>> {
    static C: OnceLock<std::sync::Mutex<Option<(String, u32)>>> = OnceLock::new();
    C.get_or_init(|| std::sync::Mutex::new(None))
}

pub fn current_context() -> Option<(String, u32)> {
    context().lock().unwrap_or_else(|e| e.into_inner()).clone()
}

#[tauri::command]
pub fn coderabbit_set_context(repo: String, pr_number: u32) {
    if repo.split_once('/').is_some() && pr_number > 0 {
        *context().lock().unwrap_or_else(|e| e.into_inner()) = Some((repo, pr_number));
    }
}

/// IPC: the free-review allowance (None when the CLI is missing or signed out).
#[tauri::command]
pub async fn coderabbit_usage() -> Option<Usage> {
    account().await.and_then(|a| a.2)
}

/// IPC: the job for this PR, if one was started in this session (the UI polls it while a review runs).
#[tauri::command]
pub fn coderabbit_job(repo: String, pr_number: u32) -> Option<Job> {
    jobs().lock().unwrap_or_else(|e| e.into_inner()).get(&(repo.to_lowercase(), pr_number)).cloned()
}

/// Starts a free review in the background (the ONLY place a review is spent). Returns the immediate answer: the job,
/// or the reason nothing was started. The caller has already obtained the owner's consent.
pub async fn start_review<R: tauri::Runtime>(app: tauri::AppHandle<R>, repo: String, pr: u32) -> Result<Job, String> {
    if repo.split_once('/').is_none() || pr == 0 {
        return Err("Which repository and pull request?".to_string());
    }
    let key = (repo.to_lowercase(), pr);
    let running = jobs().lock().unwrap_or_else(|e| e.into_inner()).get(&key).is_some_and(|j| j.state == JobState::Running);
    let usage = if running { None } else { account().await.and_then(|a| a.2) };
    match start_decision(running, usage.as_ref()) {
        Start::Refused(why) => return Err(why),
        Start::AlreadyRunning => return jobs().lock().unwrap_or_else(|e| e.into_inner()).get(&key).cloned().ok_or_else(|| "No review running.".to_string()),
        Start::Go => {}
    }
    let job = Job { state: JobState::Running, started_ms: now_ms(), finished_ms: None, report: None };
    jobs().lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone(), job.clone());
    let task_id = format!("coderabbit:{}#{pr}", repo.to_lowercase());
    crate::background_tasks::start(&app, &task_id, "coderabbit", &format!("CodeRabbit is reviewing PR {pr}"));
    tauri::async_runtime::spawn(async move {
        let report = report_for_pr(&repo, pr, true).await;
        crate::background_tasks::finish(&app, &task_id);
        let state = if report.source == Source::None { JobState::Failed } else { JobState::Done };
        let line = finish_line(pr, &report);
        {
            let mut g = jobs().lock().unwrap_or_else(|e| e.into_inner());
            if let Some(j) = g.get_mut(&key) {
                j.state = state;
                j.finished_ms = Some(now_ms());
                j.report = Some(report);
            }
        }
        crate::proactive_policy::submit(&app, format!("coderabbit-{}-{pr}-{}", repo.replace('/', "_"), now_ms()), line, crate::google::types::AlertUrgency::Medium);
    });
    Ok(job)
}

/// IPC: start the free review after the owner said yes (button or voice). Same function either way.
#[tauri::command]
pub async fn coderabbit_review_start<R: tauri::Runtime>(app: tauri::AppHandle<R>, repo: String, pr_number: u32) -> Result<Job, String> {
    let r = start_review(app, repo, pr_number).await;
    // A voice "yes" and a click can both arrive; whichever is second must not ask again.
    crate::memcore::offer::clear();
    r
}

fn safe_token(s: &str, extra: &[char]) -> bool {
    !s.is_empty() && s.len() <= 200 && !s.starts_with('-') && !s.contains("..") && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c) || extra.contains(&c))
}

/// CLI arguments of a server-side review of `repo` (`owner/name`) from `source` into `base`. None for anything that
/// could be read as a flag or is not a plain repo/ref. `--use-credits` is never present.
pub fn remote_review_args(repo: &str, base: &str, source: &str) -> Option<Vec<String>> {
    let (owner, name) = repo.split_once('/')?;
    if !(safe_token(owner, &[]) && safe_token(name, &[]) && !name.contains('/') && !owner.contains('/')) || !safe_token(base, &[]) || !safe_token(source, &[]) {
        return None;
    }
    Some(
        ["review", "--agent", "--remote", repo, "--base", base, "--source-branch", source]
            .iter()
            .map(|s| s.to_string())
            .collect(),
    )
}

/// Reports already fetched, by (repo, PR number, head SHA): one review per commit, ever (also across re-opens).
#[derive(Default)]
pub struct ReviewCache {
    map: HashMap<(String, u32, String), Report>,
    order: Vec<(String, u32, String)>,
}

const CACHE_CAP: usize = 40;

impl ReviewCache {
    pub fn get(&self, repo: &str, pr: u32, sha: &str) -> Option<&Report> {
        self.map.get(&(repo.to_lowercase(), pr, sha.to_string()))
    }

    pub fn put(&mut self, repo: &str, pr: u32, sha: &str, r: Report) {
        let key = (repo.to_lowercase(), pr, sha.to_string());
        if self.map.insert(key.clone(), r).is_none() {
            self.order.push(key);
            while self.order.len() > CACHE_CAP {
                let old = self.order.remove(0);
                self.map.remove(&old);
            }
        }
    }
}

/// Runs are serialized (one CodeRabbit process at a time) and share the cache.
fn state() -> &'static tokio::sync::Mutex<ReviewCache> {
    static S: OnceLock<tokio::sync::Mutex<ReviewCache>> = OnceLock::new();
    S.get_or_init(|| tokio::sync::Mutex::new(ReviewCache::default()))
}

/// CodeRabbit says reviews usually take 2-6 minutes and up to 15; stopping early discards the review.
async fn git(dir: &std::path::Path, args: &[&str], timeout: Duration) -> Option<String> {
    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("-C").arg(dir).args(args).kill_on_drop(true);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.as_std_mut().creation_flags(0x0800_0000);
    }
    let out = tokio::time::timeout(timeout, cmd.output()).await.ok()?.ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// NEXUS's own git checkout when it is a clone of `repo` (found from the running exe, then the working directory).
async fn local_checkout_for(repo: &str) -> Option<std::path::PathBuf> {
    let mut starts: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        starts.extend(exe.parent().map(|p| p.to_path_buf()));
    }
    starts.extend(std::env::current_dir().ok());
    for start in starts {
        let Some(top) = git(&start, &["rev-parse", "--show-toplevel"], QUICK_TIMEOUT).await else { continue };
        let top = std::path::PathBuf::from(top);
        let Some(url) = git(&top, &["remote", "get-url", "origin"], QUICK_TIMEOUT).await else { continue };
        if repo_from_remote_url(&url).is_some_and(|r| r.eq_ignore_ascii_case(repo)) {
            return Some(top);
        }
    }
    None
}

fn refs(v: &[String]) -> Vec<&str> {
    v.iter().map(String::as_str).collect()
}

/// Free local review of a PR in a temporary worktree of its head (the owner's working tree is never touched).
async fn run_local_review(cli: &std::path::PathBuf, checkout: &std::path::Path, repo: &str, pr: u32, base: &str) -> Result<Report, String> {
    let (fetch_pr, fetch_base, review) = local_review_plan(pr, base).ok_or("The base branch name is not a plain ref.")?;
    git(checkout, &refs(&fetch_pr), Duration::from_secs(120)).await.ok_or("Could not fetch the pull request from GitHub.")?;
    let head = git(checkout, &["rev-parse", "FETCH_HEAD"], QUICK_TIMEOUT).await.ok_or("Could not read the fetched pull request.")?;
    git(checkout, &refs(&fetch_base), Duration::from_secs(120)).await.ok_or("Could not fetch the base branch.")?;
    let dir = std::env::temp_dir().join("nexus-coderabbit").join(format!("{}-{pr}-{}", repo.replace('/', "_"), &head[..head.len().min(8)]));
    let _ = git(checkout, &["worktree", "remove", "--force", &dir.to_string_lossy()], QUICK_TIMEOUT).await;
    git(checkout, &["worktree", "add", "--detach", &dir.to_string_lossy(), &head], Duration::from_secs(120))
        .await
        .ok_or("Could not prepare a temporary checkout of the pull request.")?;
    let result = crate::coderabbit::run_cli_at(cli, &refs(&review), REVIEW_TIMEOUT, Some(&dir)).await;
    let _ = git(checkout, &["worktree", "remove", "--force", &dir.to_string_lossy()], QUICK_TIMEOUT).await;
    let out = result?;
    let mut report = parse_review_ndjson(&String::from_utf8_lossy(&out.stdout))?;
    report.source = Source::LocalReview;
    Ok(report)
}

const REVIEW_TIMEOUT: Duration = Duration::from_secs(16 * 60);
const QUICK_TIMEOUT: Duration = Duration::from_secs(20);

/// Signed-in account and usage, from the CLI (empty when it is not installed or signed in).
pub async fn account() -> Option<(String, Vec<String>, Option<Usage>)> {
    let cli = crate::coderabbit::cli_path()?;
    let auth = crate::coderabbit::run_cli(&cli, &["auth", "status", "--agent"], QUICK_TIMEOUT).await.ok()?;
    let v = String::from_utf8_lossy(&auth.stdout).lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).last()?;
    if v.get("authenticated").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let active = v.pointer("/currentOrg/name").and_then(Value::as_str)?.to_string();
    let orgs = v.get("organizations").and_then(Value::as_array).map(|a| a.iter().filter_map(|o| o.get("name").and_then(Value::as_str).map(str::to_string)).collect()).unwrap_or_default();
    let usage = crate::coderabbit::run_cli(&cli, &["usage", "--agent"], QUICK_TIMEOUT).await.ok().and_then(|o| parse_usage(&String::from_utf8_lossy(&o.stdout)));
    Some((active, orgs, usage))
}

/// The PR's CodeRabbit report, cheapest source first: (1) what CodeRabbit already posted on GitHub (free, no CLI),
/// (2) a server review from the included free reviews when `auto_review` is on. Never blocks on failure: the reason
/// and the fix come back as `notes`.
pub async fn report_for_pr(repo: &str, pr: u32, auto_review: bool) -> Report {
    let posted = crate::coderabbit::coderabbit_review_pr_data(pr, Some(repo.to_string())).await;
    let (head_sha, base, source) = match &posted {
        Ok(r) => (r.head_sha.clone(), r.base_branch.clone(), r.head_branch.clone()),
        Err(e) => return Report::empty(format!("Could not read the pull request: {e}")),
    };
    let posted = posted.ok();
    if let Some(r) = posted.as_ref().filter(|r| r.verdict != "NOT_REVIEWED") {
        let mut rep = crate::coderabbit::report_from_review(r);
        rep.usage = account().await.and_then(|a| a.2);
        return rep;
    }
    let mut notes = Vec::new();
    let owner = repo.split('/').next().unwrap_or("");
    let acct = account().await;
    let Some((active, orgs, usage)) = acct else {
        return Report::empty("CodeRabbit has not reviewed this pull request, and the CodeRabbit CLI is not installed or not signed in (Settings, Accounts).");
    };
    if let Some(n) = access_note(owner, classify_access(owner, &active, &orgs)) {
        notes.push(n);
    }
    let mut rep = Report::empty("CodeRabbit has not posted a review on this pull request yet.");
    rep.notes.extend(notes);
    rep.usage = usage.clone();
    if !auto_review {
        rep.notes.push("Automatic CodeRabbit reviews are switched off (coderabbitAutoReview).".into());
        return rep;
    }
    let Some(u) = usage else {
        rep.notes.push("Could not read your CodeRabbit usage, so no review was started.".into());
        return rep;
    };
    if !free_review_allowed(&u) {
        rep.notes.push(free_review_note(&u));
        return rep;
    }
    let local = local_checkout_for(repo).await;
    let remote_args = if local.is_some() { None } else { remote_review_args(repo, &base, &source) };
    if local.is_none() && remote_args.is_none() {
        rep.notes.push("The branch names are not plain refs, so no review was started.".into());
        return rep;
    }
    let mut guard = state().lock().await;
    if let Some(hit) = guard.get(repo, pr, &head_sha) {
        return hit.clone();
    }
    let Some(cli) = crate::coderabbit::cli_path() else {
        return rep;
    };
    tracing::info!(
        "[CODERABBIT] free {} review of {repo} PR {pr} ({} of {} included left)",
        if local.is_some() { "local" } else { "server" },
        u.remaining,
        u.limit
    );
    let result = match (&local, &remote_args) {
        (Some(dir), _) => run_local_review(&cli, dir, repo, pr, &base).await,
        (None, Some(args)) => {
            let argv: Vec<&str> = args.iter().map(String::as_str).collect();
            match crate::coderabbit::run_cli(&cli, &argv, REVIEW_TIMEOUT).await {
                Ok(out) => parse_review_ndjson(&String::from_utf8_lossy(&out.stdout)),
                Err(e) => Err(e),
            }
        }
        (None, None) => Err("No review method is available.".to_string()),
    };
    match result {
        Ok(mut r) => {
            r.usage = Some(u);
            r.notes.extend(rep.notes);
            guard.put(repo, pr, &head_sha, r.clone());
            r
        }
        Err(e) => {
            tracing::warn!("[CODERABBIT] review failed: {e}");
            rep.notes.push(format!("CodeRabbit could not review it: {e}"));
            rep
        }
    }
}

/// IPC: what CodeRabbit ALREADY has for this pull request (read-only; never spends a review). Spending goes through
/// `coderabbit_review_start` after the owner's consent. `coderabbitAutoReview` (default OFF) restores the old
/// automatic behaviour for owners who want it.
#[tauri::command]
pub async fn coderabbit_report<R: tauri::Runtime>(app: tauri::AppHandle<R>, repo: String, pr_number: u32) -> Result<Report, String> {
    if repo.split_once('/').is_none() || pr_number == 0 {
        return Err("Which repository and pull request?".to_string());
    }
    let auto = tauri::Manager::path(&app).app_data_dir().map(|d| crate::memcore::flag(&d, "coderabbitAutoReview", false)).unwrap_or(false);
    Ok(report_for_pr(&repo, pr_number, auto).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REVIEW: &str = r#"{"type":"review_context","remote":true,"currentBranch":"feat/x","baseBranch":"main"}
{"type":"heartbeat"}
{"type":"finding","severity":"major","fileName":"src/pool.rs","title":"Connection leak on error","comment":"The lease is dropped without recycle.","codegenInstructions":"Call pool.recycle(conn) before returning.","suggestions":["pool.recycle(conn);"],"startLine":40,"endLine":42,"newField":1}
{"type":"finding","severity":"trivial","fileName":"src/a.rs","comment":"Prefer Duration over u64 milliseconds."}
{"type":"someFutureRecord","x":1}
{"type":"complete","status":"review_completed","findings":2,"unreviewedFileCount":1,"reused":true,"gate":{"threshold":"major","status":"blocked","blockingFindings":1}}"#;

    #[test]
    fn the_review_stream_becomes_one_report_with_official_severities() {
        let r = parse_review_ndjson(REVIEW).unwrap();
        assert_eq!(r.source, Source::RemoteReview);
        assert_eq!(r.findings.len(), 2);
        let f = &r.findings[0];
        assert_eq!((f.severity, f.file.as_str(), f.start_line, f.end_line), (Severity::Major, "src/pool.rs", Some(40), Some(42)));
        assert_eq!(f.title, "Connection leak on error");
        assert_eq!(f.suggestions, vec!["pool.recycle(conn);".to_string()]);
        assert_eq!(f.agent_instructions.as_deref(), Some("Call pool.recycle(conn) before returning."));
        assert_eq!(r.findings[1].title, "Prefer Duration over u64 milliseconds.", "title falls back to the comment's first line");
        assert_eq!((r.unreviewed_files, r.reused), (1, true));
        let g = r.gate.unwrap();
        assert_eq!((g.status.as_deref(), g.threshold.as_deref(), g.blocking), (Some("blocked"), Some("major"), Some(1)));
    }

    const REAL_LOCAL: &str = include_str!("../tests/fixtures/coderabbit/local_review_real.ndjson");
    const REAL_REMOTE_FAIL: &str = include_str!("../tests/fixtures/coderabbit/remote_review_not_available.ndjson");

    #[test]
    fn a_real_local_review_recorded_from_cli_0_9_0_parses() {
        let r = parse_review_ndjson(REAL_LOCAL).unwrap();
        assert_eq!(r.findings.len(), 1);
        let f = &r.findings[0];
        assert_eq!((f.severity, f.file.as_str(), f.start_line, f.end_line), (Severity::Minor, "frontend/src/stage/handOverlayModel.ts", Some(108), Some(108)));
        assert_eq!(f.title, "Do not use a previous click position for a scroll ripple.");
        assert!(f.comment.starts_with("After a click, `action_pos` remains set."), "the repeated bold title is removed: {}", f.comment);
        assert!(f.agent_instructions.as_deref().unwrap().contains("Review comment at @frontend/src/stage/handOverlayModel.ts at line 108"));
        assert!(f.suggestions.is_empty());
    }

    #[test]
    fn the_real_not_available_error_is_surfaced_as_coderabbits_own_message() {
        let e = parse_review_ndjson(REAL_REMOTE_FAIL).unwrap_err();
        assert!(e.contains("not available in the active CodeRabbit organization"), "{e}");
    }

    #[test]
    fn remote_urls_give_the_repo_and_the_local_plan_is_plain() {
        for u in ["https://github.com/Engine-NEXUS/NEXUS-Agent.git", "git@github.com:Engine-NEXUS/NEXUS-Agent.git", "https://github.com/Engine-NEXUS/NEXUS-Agent/"] {
            assert_eq!(repo_from_remote_url(u).as_deref(), Some("Engine-NEXUS/NEXUS-Agent"), "{u}");
        }
        assert_eq!(repo_from_remote_url("https://gitlab.com/a/b.git"), None);
        assert_eq!(repo_from_remote_url("https://github.com/only-owner"), None);
        let (a, b, c) = local_review_plan(34, "main").unwrap();
        assert_eq!(a, ["fetch", "--quiet", "origin", "pull/34/head"]);
        assert_eq!(b, ["fetch", "--quiet", "origin", "main"]);
        assert_eq!(c, ["review", "--agent", "--base", "origin/main", "--committed"]);
        assert!(![a, b, c].concat().iter().any(|s| s.contains("credit")), "no credits, ever");
        assert!(local_review_plan(0, "main").is_none());
        assert!(local_review_plan(5, "--output=x").is_none());
        assert!(local_review_plan(5, "a b").is_none());
        assert_eq!(strip_title("**T**\n\nbody", "T"), "body");
        assert_eq!(strip_title("body only", "T"), "body only");
    }

    #[test]
    fn a_derived_type_is_always_marked_inferred() {
        let r = parse_review_ndjson(REVIEW).unwrap();
        assert!(r.findings.iter().all(|f| f.kind_inferred));
        assert_eq!(r.findings[0].kind, Kind::PotentialIssue, "major leak");
        assert_eq!(r.findings[1].kind, Kind::Nitpick, "trivial style");
        assert_eq!(infer_kind(Severity::Minor, "Rename variable", "clarity"), Kind::Refactor);
        assert_eq!(infer_kind(Severity::Info, "Possible SQL injection", ""), Kind::PotentialIssue);
    }

    #[test]
    fn an_error_without_findings_is_the_error_and_with_findings_it_is_ignored() {
        let e = r#"{"type":"error","errorType":"review","message":"This pull request is not available in the active CodeRabbit organization.","recoverable":false}"#;
        assert_eq!(parse_review_ndjson(e).unwrap_err(), "This pull request is not available in the active CodeRabbit organization.");
        let both = format!("{REVIEW}\n{e}");
        assert_eq!(parse_review_ndjson(&both).unwrap().findings.len(), 2);
        let skipped = r#"{"type":"complete","status":"review_skipped","message":"No changes detected","findings":0}"#;
        let r = parse_review_ndjson(skipped).unwrap();
        assert!(r.findings.is_empty() && r.notes == vec!["No changes detected".to_string()]);
        assert!(parse_review_ndjson("not json at all").unwrap().findings.is_empty());
    }

    #[test]
    fn official_type_and_severity_come_from_the_comment_header() {
        assert_eq!(parse_header("_⚠️ Potential issue_ | _🟠 Major_\n\n**x**"), (Kind::PotentialIssue, Severity::Major));
        assert_eq!(parse_header("_🛠️ Refactor suggestion_ | _🟡 Minor_"), (Kind::Refactor, Severity::Minor));
        assert_eq!(parse_header("_🧹 Nitpick_ | _🔵 Trivial_"), (Kind::Nitpick, Severity::Trivial));
        assert_eq!(parse_header("_⚠️ Potential issue_ | _🔴 Critical_").1, Severity::Critical);
        assert_eq!(parse_header("just a comment"), (Kind::Unknown, Severity::Unknown), "no guessing");
        assert_eq!(parse_header("Potentially useful note, not a CodeRabbit label").0, Kind::Unknown, "only the exact label counts");
        assert_eq!(parse_severity("INFO"), Severity::Info);
        assert_eq!(parse_severity("praise"), Severity::Unknown);
    }

    #[test]
    fn access_tells_the_exact_fix() {
        let orgs: Vec<String> = ["ChitkulLakshya", "zync-meet", "Servx-lab"].iter().map(|s| s.to_string()).collect();
        assert_eq!(classify_access("chitkullakshya", "ChitkulLakshya", &orgs), Access::ActiveOrg);
        assert_eq!(classify_access("Zync-Meet", "ChitkulLakshya", &orgs), Access::OtherOrg);
        assert_eq!(classify_access("Engine-NEXUS", "ChitkulLakshya", &orgs), Access::UnknownOrg);
        assert!(access_note("zync-meet", Access::OtherOrg).unwrap().contains("coderabbit auth org"));
        assert!(access_note("Engine-NEXUS", Access::UnknownOrg).unwrap().contains("not an organization"));
        assert_eq!(access_note("x", Access::ActiveOrg), None);
    }

    const USAGE: &str = r#"{"type":"status","phase":"usage","status":"complete","organization":"ChitkulLakshya","includedReviews":{"state":"available","repository":"Engine-NEXUS/NEXUS-Agent","fullCapacityInMs":2972403,"limit":10,"remaining":3,"rollingWindowMs":3600000},"billingPeriod":{"state":"available","scope":"user","usageBillingStatus":"inactive","user":"ChitkulLakshya","reviewsCount":0,"resetsAt":"2026-11-28T00:00:00.000Z"}}"#;

    #[test]
    fn only_included_free_reviews_ever_allow_a_review() {
        let u = parse_usage(USAGE).unwrap();
        assert_eq!((u.limit, u.remaining, u.billing_status.as_str()), (10, 3, "inactive"));
        assert!(free_review_allowed(&u));
        assert!(!free_review_allowed(&Usage { remaining: 0, ..u.clone() }), "none left: no review, no credits");
        assert!(!free_review_allowed(&Usage { state: "exhausted".into(), ..u.clone() }));
        let paid = Usage { billing_status: "active".into(), remaining: 0, ..u.clone() };
        assert!(!free_review_allowed(&paid), "an active credit plan does not turn credits on");
        assert!(free_review_note(&paid).contains("never uses paid credits"));
        assert_eq!(parse_usage("garbage"), None);
    }

    fn usage(remaining: u32) -> Usage {
        Usage { remaining, ..parse_usage(USAGE).unwrap() }
    }

    #[test]
    fn the_time_until_the_allowance_is_full_again_is_read_and_shown() {
        let u = parse_usage(USAGE).unwrap();
        assert_eq!(u.full_capacity_ms, 2_972_403);
        assert_eq!(minutes_text(2_972_403), "50 min");
        assert_eq!(minutes_text(60_000), "1 min");
        assert_eq!(minutes_text(1), "1 min", "never rounds a wait down to nothing");
        assert_eq!(minutes_text(0), "under a minute");
        let none_left = Usage { remaining: 0, ..u };
        let note = free_review_note(&none_left);
        assert!(note.contains("0 of 10") && note.contains("back to 10 in about 50 min") && note.contains("never uses paid credits"), "{note}");
    }

    #[test]
    fn the_consent_question_states_the_count_first() {
        let q = consent_line(&usage(7), 34, "zync-meet/Zync");
        assert_eq!(q, "You have 7 of 10 free CodeRabbit reviews left, sir. Use one on PR 34 in Zync?");
    }

    #[test]
    fn a_review_starts_only_with_a_free_one_left_and_a_second_request_reuses_the_running_one() {
        assert_eq!(start_decision(false, Some(&usage(3))), Start::Go);
        assert!(matches!(start_decision(false, Some(&usage(0))), Start::Refused(m) if m.contains("No free CodeRabbit review left")));
        assert!(matches!(start_decision(false, None), Start::Refused(m) if m.contains("could not read")));
        assert_eq!(start_decision(true, Some(&usage(0))), Start::AlreadyRunning, "a running review is never started twice and costs nothing");
        let exhausted = Usage { state: "exhausted".into(), ..usage(5) };
        assert!(matches!(start_decision(false, Some(&exhausted)), Start::Refused(_)));
    }

    #[test]
    fn spoken_lines_rotate_and_the_finish_line_says_what_happened() {
        for i in 0..20 {
            assert_ne!(working_line(i), working_line(i + 1), "never the same line twice in a row");
        }
        let mut r = Report::empty("");
        r.source = Source::LocalReview;
        assert_eq!(finish_line(34, &r), "CodeRabbit has nothing to flag on PR 34, sir.");
        let f = parse_review_ndjson(REVIEW).unwrap();
        assert_eq!(finish_line(34, &f), "CodeRabbit finished: 2 findings on PR 34, sir.");
        r.findings = f.findings.into_iter().take(1).collect();
        assert_eq!(finish_line(34, &r), "CodeRabbit finished: 1 finding on PR 34, sir.");
        let failed = Report::empty("This repository is not available in the active CodeRabbit organization.");
        assert_eq!(finish_line(9, &failed), "CodeRabbit could not review PR 9, sir. This repository is not available in the active CodeRabbit organization.");
    }

    #[test]
    fn the_sidebar_pr_context_is_remembered_and_junk_is_ignored() {
        coderabbit_set_context("o/r".into(), 5);
        assert_eq!(current_context(), Some(("o/r".to_string(), 5)));
        coderabbit_set_context("no-slash".into(), 6);
        coderabbit_set_context("o/r".into(), 0);
        assert_eq!(current_context(), Some(("o/r".to_string(), 5)), "an invalid context never replaces a valid one");
    }

    #[test]
    fn the_review_command_is_plain_and_never_spends_credits() {
        let a = remote_review_args("Engine-NEXUS/NEXUS-Agent", "main", "feat/x").unwrap();
        assert_eq!(a[..3], ["review", "--agent", "--remote"]);
        assert!(!a.iter().any(|s| s.contains("credit")), "no --use-credits, ever");
        assert!(remote_review_args("o/r", "--use-credits", "x").is_none());
        assert!(remote_review_args("o/r", "main", "-x").is_none());
        assert!(remote_review_args("o/r/extra", "main", "x").is_none());
        assert!(remote_review_args("o/..", "main", "x").is_none());
        assert!(remote_review_args("o/r", "main", "a b").is_none());
        assert!(remote_review_args("noslash", "main", "x").is_none());
    }

    #[test]
    fn one_report_per_commit_and_the_cache_is_bounded() {
        let mut c = ReviewCache::default();
        c.put("O/R", 5, "sha1", Report::empty("one"));
        assert_eq!(c.get("o/r", 5, "sha1").unwrap().notes, vec!["one".to_string()], "repo is case-insensitive");
        assert!(c.get("o/r", 5, "sha2").is_none(), "a new commit is a new review");
        assert!(c.get("o/r", 6, "sha1").is_none());
        for i in 0..60u32 {
            c.put("o/r", i, "s", Report::empty("x"));
        }
        assert!(c.get("o/r", 59, "s").is_some() && c.get("o/r", 0, "s").is_none(), "oldest evicted");
    }
}
