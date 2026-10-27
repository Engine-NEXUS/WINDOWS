//! Gated follow-up after a watched agent finishes: commit its changes on a new branch, push, and open a DRAFT pull request (plan P5).
//!
//! The rule is one-directional: the follow-up runs ONLY on `Verdict::Go`. On `Hold` / `Unsure` nothing is read, staged or
//! committed and NEXUS says why. The laptop shutdown has its own rule and is never decided here.
//!
//! Layout:
//!   * pure: `parse_porcelain`, `plan_followup`, `commit_message`, `describe`, `Steps::git_calls`, `pr_command`, `parse_github_repo`;
//!   * git side effects go through the injectable `GitRunner` (real impl = `RealGit`, argument arrays only, 60 s timeout on every call);
//!   * the pull request goes through the existing `github_cmd::GitHubCommand::CreatePr` (`draft: true`), never a new GitHub client.
//!
//! Safety rules, each pinned by a test:
//!   * only listed paths are staged (never a blanket add); secrets-like paths refuse the WHOLE commit and are named;
//!   * no commit while a merge / rebase / cherry-pick is in progress, on a detached HEAD, with conflicts, or with nothing changed;
//!   * a new `nexus/<agent>-<yyyymmdd-hhmm>` branch is always created, so nothing is ever pushed to an existing branch;
//!   * a failed push (no remote, no credentials) is reported plainly and leaves the local commit in place;
//!   * nothing here merges.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::github_cmd::{GitHubCommand, GitHubResult};
use crate::session_verdict::{self, Reason, Verdict};

/// Every git call is killed after this long (a credential prompt or a dead remote must not hang the tracker).
pub const GIT_TIMEOUT: Duration = Duration::from_secs(60);
/// Git commit subject limit.
const MAX_SUBJECT_CHARS: usize = 72;
/// More changed files than this are not committed unattended.
const MAX_FILES: usize = 200;

// ─── Data ────────────────────────────────────────────────────────────

/// What to do once the agent has finished. `agent` and `stamp` (yyyymmdd-hhmm, see `default_stamp`) make the branch name; they
/// are inputs so the planner stays pure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FollowUp {
    CommitAndPr {
        dir: PathBuf,
        agent: String,
        stamp: String,
        /// The agent's final text; its first line becomes the commit subject.
        message_hint: Option<String>,
        /// The pull request target; empty means "main".
        base: String,
    },
}

/// Local time as `yyyymmdd-hhmm`.
pub fn default_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M").to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub x: char,
    pub y: char,
    pub path: String,
    /// The old name of a rename / copy.
    pub orig_path: Option<String>,
}

impl StatusEntry {
    pub fn is_unmerged(&self) -> bool {
        self.x == 'U' || self.y == 'U' || matches!((self.x, self.y), ('D', 'D') | ('A', 'A'))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    /// None on a detached HEAD.
    pub branch: Option<String>,
    pub detached: bool,
    /// No commit exists yet.
    pub unborn: bool,
    pub entries: Vec<StatusEntry>,
    /// The repository root (git reports paths relative to it).
    pub top: Option<PathBuf>,
    /// "merge" / "rebase" / ... when one is in progress.
    pub in_progress: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// The verdict was not Go (the sentence is already spoken-ready).
    NotClean(String),
    NotARepo(String),
    InProgress(String),
    Conflicts(Vec<String>),
    DetachedHead,
    NoCommits,
    NothingChanged,
    TooManyFiles(usize),
    Secrets(Vec<String>),
    BadBase(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Run(Steps),
    Skip(SkipReason),
}

/// Everything the executor needs; built only by `plan_followup`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Steps {
    pub dir: PathBuf,
    pub agent: String,
    pub branch: String,
    pub paths: Vec<String>,
    pub message: String,
    pub base: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Branch,
    Stage,
    Commit,
    Push,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCall {
    pub stage: Stage,
    pub args: Vec<String>,
}

impl Steps {
    /// The mutating git calls, in order. Argument arrays only; the add lists the exact paths after `--`.
    pub fn git_calls(&self) -> Vec<GitCall> {
        let mut add = vec!["add".to_string(), "--".to_string()];
        add.extend(self.paths.iter().cloned());
        vec![
            GitCall { stage: Stage::Branch, args: vec!["checkout".into(), "-b".into(), self.branch.clone()] },
            GitCall { stage: Stage::Stage, args: add },
            GitCall { stage: Stage::Commit, args: vec!["commit".into(), "-m".into(), self.message.clone()] },
            GitCall { stage: Stage::Push, args: vec!["push".into(), "-u".into(), "origin".into(), self.branch.clone()] },
        ]
    }

    pub fn pr_body(&self) -> String {
        let mut body = format!(
            "Opened by NEXUS after {} finished a session that looked clean. This is a draft: review it before merging.\n\nFiles:\n",
            self.agent
        );
        for p in self.paths.iter().take(30) {
            body.push_str(&format!("- {p}\n"));
        }
        if self.paths.len() > 30 {
            body.push_str(&format!("- ... and {} more\n", self.paths.len() - 30));
        }
        body
    }
}

// ─── Parsing git status ──────────────────────────────────────────────

/// Parse `git status --porcelain=v1 -b` output. Accepts the NUL-separated form (`-z`, what `gather_status` asks for: no
/// quoting, rename order `new\0old`) and the plain newline form (quoted paths, `old -> new`).
pub fn parse_porcelain(text: &str) -> GitStatus {
    let mut st = GitStatus::default();
    if text.contains('\0') {
        let mut parts = text.split('\0').filter(|p| !p.is_empty());
        while let Some(rec) = parts.next() {
            if let Some(b) = rec.strip_prefix("## ") {
                parse_branch_line(b, &mut st);
                continue;
            }
            let Some((x, y, path)) = split_xy(rec) else { continue };
            let orig_path = if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') { parts.next().map(str::to_string) } else { None };
            st.entries.push(StatusEntry { x, y, path: path.to_string(), orig_path });
        }
    } else {
        for line in text.lines() {
            if let Some(b) = line.strip_prefix("## ") {
                parse_branch_line(b, &mut st);
                continue;
            }
            let Some((x, y, rest)) = split_xy(line) else { continue };
            let (path, orig_path) = match rest.split_once(" -> ") {
                Some((old, new)) if matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C') => (unquote(new), Some(unquote(old))),
                _ => (unquote(rest), None),
            };
            st.entries.push(StatusEntry { x, y, path, orig_path });
        }
    }
    st
}

fn split_xy(rec: &str) -> Option<(char, char, &str)> {
    let b = rec.as_bytes();
    if b.len() < 4 || b[2] != b' ' || !b[0].is_ascii() || !b[1].is_ascii() {
        return None;
    }
    let (x, y) = (b[0] as char, b[1] as char);
    if x == '!' && y == '!' {
        return None; // ignored file
    }
    Some((x, y, &rec[3..]))
}

fn parse_branch_line(b: &str, st: &mut GitStatus) {
    if b.starts_with("HEAD (no branch)") {
        st.detached = true;
        return;
    }
    for p in ["No commits yet on ", "Initial commit on "] {
        if let Some(rest) = b.strip_prefix(p) {
            st.unborn = true;
            st.branch = Some(rest.trim().to_string());
            return;
        }
    }
    let name = b.split("...").next().unwrap_or(b).split(' ').next().unwrap_or("");
    if !name.is_empty() {
        st.branch = Some(name.to_string());
    }
}

fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else { return s.to_string() };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some(o) => {
                // Octal escapes (non-ASCII names) are not decoded: the path stays wrong, so `git add` fails loudly instead of guessing.
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ─── Secrets ─────────────────────────────────────────────────────────

/// A path that looks like it holds credentials. Strict on purpose: a false positive costs a skipped commit, a miss leaks a key.
/// (`token` / `secret` anywhere in the file name also catches ordinary source files such as `tokenizer.rs`.)
pub fn is_secret_path(path: &str) -> bool {
    let norm = path.replace('\\', "/").to_lowercase();
    let comps: Vec<&str> = norm.split('/').filter(|c| !c.is_empty()).collect();
    let Some((name, dirs)) = comps.split_last() else { return false };
    if dirs.iter().any(|d| matches!(*d, ".ssh" | ".aws" | ".gnupg")) {
        return true;
    }
    if name.starts_with(".env") || name.starts_with("credentials") {
        return true;
    }
    if ["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"].iter().any(|p| name.starts_with(p)) {
        return true;
    }
    if [".pem", ".key", ".pfx", ".p12", ".ppk", ".jks", ".keystore"].iter().any(|e| name.ends_with(e)) {
        return true;
    }
    if matches!(*name, ".netrc" | "_netrc" | ".pgpass") {
        return true;
    }
    name.contains("token") || name.contains("secret")
}

// ─── Naming ──────────────────────────────────────────────────────────

fn slug(s: &str, max: usize, allow_alpha: bool) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let keep = c.is_ascii_digit() || (allow_alpha && c.is_ascii_lowercase());
        if keep {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    trimmed.chars().take(max).collect::<String>().trim_matches('-').to_string()
}

/// `nexus/<agent>-<stamp>`; both parts reduced to `[a-z0-9-]`.
pub fn branch_name(agent: &str, stamp: &str) -> String {
    let a = slug(agent, 24, true);
    let a = if a.is_empty() { "agent".to_string() } else { a };
    let s = slug(stamp, 16, false);
    let s = if s.is_empty() { "now".to_string() } else { s };
    format!("nexus/{a}-{s}")
}

fn clean_label(s: &str) -> String {
    let c: String = s.chars().filter(|c| !c.is_control()).collect();
    let c = c.split_whitespace().collect::<Vec<_>>().join(" ");
    let c: String = c.chars().take(30).collect();
    if c.trim().is_empty() { "agent".to_string() } else { c.trim().to_string() }
}

/// First line of the agent's final text, cleaned, at most 72 characters; else "NEXUS: changes from <agent>".
pub fn commit_message(hint: Option<&str>, agent: &str) -> String {
    let fallback = format!("NEXUS: changes from {}", clean_label(agent));
    let Some(hint) = hint else { return fallback };
    let Some(raw) = hint.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("```")) else { return fallback };
    let mut line: String = raw.chars().filter(|c| !c.is_control()).collect();
    line = line.trim_start_matches(|c: char| matches!(c, '#' | '>' | '*' | '-' | '+' | '`') || c.is_whitespace()).to_string();
    line = line.replace(['`', '*'], "");
    line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().filter(|c| c.is_alphanumeric()).count() < 4 {
        return fallback;
    }
    if line.chars().count() > MAX_SUBJECT_CHARS {
        let cut: String = line.chars().take(MAX_SUBJECT_CHARS).collect();
        line = match cut.rfind(' ') {
            Some(i) if cut[..i].chars().count() >= 40 => cut[..i].to_string(),
            _ => cut,
        };
    }
    let line = line.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '-') || c.is_whitespace()).to_string();
    if line.chars().filter(|c| c.is_alphanumeric()).count() < 4 { fallback } else { line }
}

fn valid_ref(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && !s.chars().any(|c| c.is_whitespace() || c.is_control() || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

// ─── The planner (pure) ──────────────────────────────────────────────

fn not_clean_sentence(verdict: &Verdict) -> String {
    let why = session_verdict::spoken_reasons(verdict);
    match verdict {
        Verdict::Go => String::new(),
        Verdict::Hold(reasons) => {
            let quote = reasons
                .iter()
                .find_map(|r| match r {
                    Reason::FailureMarker(_, line) if !line.is_empty() => Some(format!(" The line was: \"{line}\".")),
                    _ => None,
                })
                .unwrap_or_default();
            format!("Commit stopped because errors were found in the response: {why}.{quote}")
        }
        // Not proof of errors, so it does not claim any.
        Verdict::Unsure(_) => format!("Commit stopped because I could not confirm the response was clean: {why}."),
    }
}

/// Decide whether to run the follow-up. Runs ONLY on `Verdict::Go`; every other path is a `Skip` with a spoken sentence.
pub fn plan_followup(verdict: &Verdict, step: &FollowUp, status: &GitStatus) -> Decision {
    let FollowUp::CommitAndPr { dir, agent, stamp, message_hint, base } = step;
    if !matches!(verdict, Verdict::Go) {
        return Decision::Skip(SkipReason::NotClean(not_clean_sentence(verdict)));
    }
    if let Some(what) = &status.in_progress {
        return Decision::Skip(SkipReason::InProgress(what.clone()));
    }
    if status.detached {
        return Decision::Skip(SkipReason::DetachedHead);
    }
    if status.unborn {
        return Decision::Skip(SkipReason::NoCommits);
    }
    let conflicts: Vec<String> = status.entries.iter().filter(|e| e.is_unmerged()).map(|e| e.path.clone()).collect();
    if !conflicts.is_empty() {
        return Decision::Skip(SkipReason::Conflicts(conflicts));
    }
    if status.entries.is_empty() {
        return Decision::Skip(SkipReason::NothingChanged);
    }
    let mut secrets: Vec<String> = Vec::new();
    for e in &status.entries {
        for p in std::iter::once(&e.path).chain(e.orig_path.iter()) {
            if is_secret_path(p) && !secrets.contains(p) {
                secrets.push(p.clone());
            }
        }
    }
    if !secrets.is_empty() {
        return Decision::Skip(SkipReason::Secrets(secrets));
    }
    if status.entries.len() > MAX_FILES {
        return Decision::Skip(SkipReason::TooManyFiles(status.entries.len()));
    }
    let base = if base.trim().is_empty() { "main".to_string() } else { base.trim().to_string() };
    if !valid_ref(&base) {
        return Decision::Skip(SkipReason::BadBase(base));
    }
    let mut paths: Vec<String> = Vec::new();
    for e in &status.entries {
        if !paths.contains(&e.path) {
            paths.push(e.path.clone());
        }
    }
    Decision::Run(Steps {
        dir: status.top.clone().unwrap_or_else(|| dir.clone()),
        agent: clean_label(agent),
        branch: branch_name(agent, stamp),
        paths,
        message: commit_message(message_hint.as_deref(), agent),
        base,
    })
}

fn name_files(files: &[String]) -> String {
    let shown: Vec<&str> = files.iter().take(5).map(String::as_str).collect();
    let mut s = shown.join(", ");
    if files.len() > shown.len() {
        s.push_str(&format!(" and {} more", files.len() - shown.len()));
    }
    s
}

fn plural(n: usize) -> String {
    if n == 1 { "1 file".to_string() } else { format!("{n} files") }
}

impl SkipReason {
    pub fn spoken(&self) -> String {
        match self {
            SkipReason::NotClean(s) => s.clone(),
            SkipReason::NotARepo(why) => format!("Commit skipped: I could not read the git status ({why})."),
            SkipReason::InProgress(w) => format!("Commit skipped: the repository is in the middle of a {w}."),
            SkipReason::Conflicts(f) => format!("Commit skipped: there are unresolved merge conflicts in {}.", name_files(f)),
            SkipReason::DetachedHead => "Commit skipped: the repository is on a detached HEAD, so there is no branch to start from.".to_string(),
            SkipReason::NoCommits => "Commit skipped: the repository has no commits yet.".to_string(),
            SkipReason::NothingChanged => "Nothing to commit: the repository has no changes.".to_string(),
            SkipReason::TooManyFiles(n) => format!("Commit skipped: {n} files changed, which is more than I commit unattended."),
            SkipReason::Secrets(f) => format!("Commit refused: these files look like secrets and I will not add them: {}.", name_files(f)),
            SkipReason::BadBase(b) => format!("Commit skipped: \"{b}\" is not a usable branch for the pull request."),
        }
    }
}

// ─── Running git ─────────────────────────────────────────────────────

pub trait GitRunner: Send + Sync {
    fn run(&self, dir: &Path, args: &[&str]) -> Result<Output, String>;
}

/// The real runner: `git` with an argument array (no shell), never prompting, killed after `timeout`.
pub struct RealGit {
    pub timeout: Duration,
}

impl Default for RealGit {
    fn default() -> Self {
        RealGit { timeout: GIT_TIMEOUT }
    }
}

impl GitRunner for RealGit {
    fn run(&self, dir: &Path, args: &[&str]) -> Result<Output, String> {
        let mut cmd = Command::new("git");
        cmd.args(args)
            .current_dir(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| format!("could not start git: {e}"))?;
        let mut so = child.stdout.take();
        let mut se = child.stderr.take();
        let t_out = std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(s) = so.as_mut() {
                let _ = s.read_to_end(&mut b);
            }
            b
        });
        let t_err = std::thread::spawn(move || {
            let mut b = Vec::new();
            if let Some(s) = se.as_mut() {
                let _ = s.read_to_end(&mut b);
            }
            b
        });
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        // The reader threads are left to end on their own: a credential helper may still hold the pipe.
                        return Err(format!("git {} timed out after {} s", args.first().copied().unwrap_or(""), self.timeout.as_secs()));
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => return Err(format!("could not wait for git: {e}")),
            }
        };
        let stdout = t_out.join().unwrap_or_default();
        let stderr = t_err.join().unwrap_or_default();
        Ok(Output { status, stdout, stderr })
    }
}

/// Remove `user:password@` from anything that looks like a URL, so a token in a remote never reaches speech or logs.
fn scrub_url_credentials(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("://") {
        out.push_str(&rest[..i + 3]);
        let after = &rest[i + 3..];
        let end = after.find(|c: char| c == '/' || c.is_whitespace()).unwrap_or(after.len());
        let authority = &after[..end];
        match authority.rfind('@') {
            Some(at) => out.push_str(&authority[at + 1..]),
            None => out.push_str(authority),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn failure_text(out: &Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr);
    let so = String::from_utf8_lossy(&out.stdout);
    let line = err
        .lines()
        .chain(so.lines())
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git failed without a message");
    let line = crate::pii_filter::sanitize(&scrub_url_credentials(line));
    line.chars().take(200).collect()
}

fn run_ok(runner: &dyn GitRunner, dir: &Path, args: &[String]) -> Result<String, String> {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let out = runner.run(dir, &refs)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(failure_text(&out))
    }
}

fn strs(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// A merge / rebase / cherry-pick / revert / bisect marker inside the git directory.
pub fn in_progress_marker(git_dir: &Path) -> Option<String> {
    [
        ("MERGE_HEAD", "merge"),
        ("rebase-merge", "rebase"),
        ("rebase-apply", "rebase"),
        ("CHERRY_PICK_HEAD", "cherry-pick"),
        ("REVERT_HEAD", "revert"),
        ("BISECT_LOG", "bisect"),
    ]
    .iter()
    .find(|(f, _)| git_dir.join(f).exists())
    .map(|(_, what)| what.to_string())
}

/// Read-only: repository root, git directory and `status`. Only called for a `Go` verdict.
pub fn gather_status(runner: &dyn GitRunner, dir: &Path) -> Result<GitStatus, String> {
    let roots = run_ok(runner, dir, &strs(&["rev-parse", "--show-toplevel", "--absolute-git-dir"]))?;
    let mut lines = roots.lines().map(str::trim).filter(|l| !l.is_empty());
    let (Some(top), Some(git_dir)) = (lines.next(), lines.next()) else {
        return Err("git did not report a repository root".to_string());
    };
    let top = PathBuf::from(top);
    let text = run_ok(runner, &top, &strs(&["status", "--porcelain=v1", "-z", "--untracked-files=all", "-b"]))?;
    let mut st = parse_porcelain(&text);
    st.in_progress = in_progress_marker(Path::new(git_dir));
    st.top = Some(top);
    Ok(st)
}

/// Gate + plan. For anything but `Go` this makes NO git call at all.
pub fn prepare(runner: &dyn GitRunner, step: &FollowUp, verdict: &Verdict) -> Decision {
    let FollowUp::CommitAndPr { dir, .. } = step;
    if !matches!(verdict, Verdict::Go) {
        return plan_followup(verdict, step, &GitStatus::default());
    }
    match gather_status(runner, dir) {
        Ok(status) => plan_followup(verdict, step, &status),
        Err(why) => Decision::Skip(SkipReason::NotARepo(why)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutcome {
    pub branch: String,
    pub file_count: usize,
    pub branch_created: bool,
    pub staged: bool,
    pub committed: bool,
    pub pushed: bool,
    /// "owner/repo" parsed from `origin`, only looked up after a successful push.
    pub repo: Option<String>,
    pub failure: Option<(Stage, String)>,
}

/// Run the branch / add / commit / push calls, stopping at the first failure. A push failure keeps the local commit.
pub fn execute_git(runner: &dyn GitRunner, steps: &Steps) -> GitOutcome {
    let mut o = GitOutcome {
        branch: steps.branch.clone(),
        file_count: steps.paths.len(),
        branch_created: false,
        staged: false,
        committed: false,
        pushed: false,
        repo: None,
        failure: None,
    };
    for call in steps.git_calls() {
        match run_ok(runner, &steps.dir, &call.args) {
            Ok(_) => match call.stage {
                Stage::Branch => o.branch_created = true,
                Stage::Stage => o.staged = true,
                Stage::Commit => o.committed = true,
                Stage::Push => o.pushed = true,
            },
            Err(why) => {
                o.failure = Some((call.stage, why));
                return o;
            }
        }
    }
    if let Ok(url) = run_ok(runner, &steps.dir, &strs(&["remote", "get-url", "origin"])) {
        o.repo = parse_github_repo(url.trim());
    }
    o
}

/// "owner/repo" from an https / ssh / scp-style GitHub remote; None for anything else.
pub fn parse_github_repo(url: &str) -> Option<String> {
    let clean = scrub_url_credentials(url.trim());
    let path = if let Some(r) = clean.strip_prefix("git@github.com:") {
        r
    } else {
        let after = clean.split_once("://").map(|(_, r)| r)?;
        let after = after.strip_prefix("git@").unwrap_or(after);
        after.strip_prefix("github.com/").or_else(|| after.strip_prefix("www.github.com/"))?
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut it = path.split('/');
    let (owner, repo) = (it.next()?, it.next()?);
    if it.next().is_some() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    let ok = |s: &str| s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    (ok(owner) && ok(repo)).then(|| format!("{owner}/{repo}"))
}

// ─── The pull request ────────────────────────────────────────────────

/// The existing GitHub command, as a DRAFT. Nothing here ever merges.
pub fn pr_command(steps: &Steps, repo: &str) -> GitHubCommand {
    GitHubCommand::CreatePr {
        repo: repo.to_string(),
        title: steps.message.clone(),
        head: steps.branch.clone(),
        base: steps.base.clone(),
        body: Some(steps.pr_body()),
        draft: true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrOutcome {
    Opened { url: Option<String> },
    Failed(String),
}

pub fn pr_outcome(result: &GitHubResult) -> PrOutcome {
    match result {
        GitHubResult::Text { text } => PrOutcome::Opened { url: text.split_once("View at: ").map(|(_, u)| u.trim().to_string()).filter(|u| !u.is_empty()) },
        GitHubResult::Error { message, .. } => PrOutcome::Failed(message.chars().take(200).collect()),
        _ => PrOutcome::Failed("GitHub asked for something I cannot answer unattended".to_string()),
    }
}

/// Create the draft PR through `github_cmd::execute_command` (not destructive, so `confirmed` stays false).
pub async fn open_draft_pr(worker_url: &str, user_id: &str, steps: &Steps, repo: &str) -> PrOutcome {
    let cmd = pr_command(steps, repo);
    let result = crate::github_cmd::execute_command(worker_url, user_id, &cmd, false).await;
    pr_outcome(&result)
}

// ─── What NEXUS says afterwards ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub spoken: String,
    pub skipped: bool,
    pub committed: bool,
    pub pushed: bool,
    pub pr_opened: bool,
    pub pr_url: Option<String>,
}

fn skip_report(r: &SkipReason) -> Report {
    Report { spoken: r.spoken(), skipped: true, committed: false, pushed: false, pr_opened: false, pr_url: None }
}

/// What NEXUS says about a decision (before anything runs, or instead of running).
pub fn describe(decision: &Decision) -> String {
    match decision {
        Decision::Skip(r) => r.spoken(),
        Decision::Run(s) => format!(
            "Committing {} to a new branch {}, then opening a draft pull request against {}.",
            plural(s.paths.len()),
            s.branch,
            s.base
        ),
    }
}

/// `pr` is None when no PR was attempted (the git side failed, or `origin` is not a GitHub repository).
pub fn describe_outcome(o: &GitOutcome, pr: Option<&PrOutcome>) -> Report {
    let n = plural(o.file_count);
    let b = &o.branch;
    let base = Report { spoken: String::new(), skipped: false, committed: o.committed, pushed: o.pushed, pr_opened: false, pr_url: None };
    if let Some((stage, why)) = &o.failure {
        let spoken = match stage {
            Stage::Branch => format!("Commit stopped: I could not create the branch {b}: {why}. Nothing was changed."),
            Stage::Stage => format!("I created the branch {b} but could not stage the files: {why}. Nothing was committed."),
            Stage::Commit => format!("I created the branch {b} and staged {n}, but the commit failed: {why}."),
            Stage::Push => format!(
                "I committed {n} on the branch {b} locally, but the push failed: {why}. The commit is still on your laptop and no pull request was opened."
            ),
        };
        return Report { spoken, ..base };
    }
    match (&o.repo, pr) {
        (None, _) => Report {
            spoken: format!("I committed {n} and pushed the branch {b}, but I could not tell which GitHub repository origin is, so no pull request was opened."),
            ..base
        },
        (Some(_), Some(PrOutcome::Opened { url })) => Report {
            spoken: format!("I committed {n} on the branch {b}, pushed it, and opened a draft pull request."),
            pr_opened: true,
            pr_url: url.clone(),
            ..base
        },
        (Some(_), Some(PrOutcome::Failed(why))) => Report {
            spoken: format!("I committed {n} and pushed the branch {b}, but opening the pull request failed: {why}."),
            ..base
        },
        (Some(_), None) => Report { spoken: format!("I committed {n} and pushed the branch {b}; the pull request was not attempted."), ..base },
    }
}

/// The one call for the tracker: gate on the verdict, commit, push, open the draft PR, return what to say. Git runs on a blocking
/// thread. `worker_url` / `user_id` are the same values the GitHub commands use elsewhere.
pub async fn run_followup(runner: Arc<dyn GitRunner>, step: FollowUp, verdict: Verdict, worker_url: &str, user_id: &str) -> Report {
    let r2 = runner.clone();
    let joined = tokio::task::spawn_blocking(move || {
        let decision = prepare(r2.as_ref(), &step, &verdict);
        tracing::info!("agent-followup: {}", describe(&decision));
        match decision {
            Decision::Skip(reason) => Err(reason),
            Decision::Run(steps) => {
                let outcome = execute_git(r2.as_ref(), &steps);
                Ok((steps, outcome))
            }
        }
    })
    .await;
    let (steps, outcome) = match joined {
        Ok(Ok(pair)) => pair,
        Ok(Err(reason)) => return skip_report(&reason),
        Err(e) => return skip_report(&SkipReason::NotARepo(format!("the git task failed: {e}"))),
    };
    let pr = match (&outcome.failure, &outcome.repo) {
        (None, Some(repo)) => Some(open_draft_pr(worker_url, user_id, &steps, repo).await),
        _ => None,
    };
    describe_outcome(&outcome, pr.as_ref())
}

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// What NEXUS says about a decision (before anything runs, or instead of running).
    fn describe(decision: &Decision) -> String {
        match decision {
            Decision::Skip(r) => r.spoken(),
            Decision::Run(s) => format!(
                "Committing {} to a new branch {}, then opening a draft pull request against {}.",
                plural(s.paths.len()),
                s.branch,
                s.base
            ),
        }
    }
    use std::process::ExitStatus;
    use std::sync::Mutex;

    #[cfg(windows)]
    fn status(code: i32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }
    #[cfg(unix)]
    fn status(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    fn out(code: i32, stdout: &str, stderr: &str) -> Output {
        Output { status: status(code), stdout: stdout.as_bytes().to_vec(), stderr: stderr.as_bytes().to_vec() }
    }

    type Handler = Box<dyn Fn(&[String]) -> Result<Output, String> + Send + Sync>;

    struct FakeGit {
        calls: Mutex<Vec<Vec<String>>>,
        handler: Handler,
    }

    impl FakeGit {
        fn new(handler: impl Fn(&[String]) -> Result<Output, String> + Send + Sync + 'static) -> FakeGit {
            FakeGit { calls: Mutex::new(Vec::new()), handler: Box::new(handler) }
        }
        fn calls(&self) -> Vec<Vec<String>> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl GitRunner for FakeGit {
        fn run(&self, _dir: &Path, args: &[&str]) -> Result<Output, String> {
            let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.calls.lock().unwrap().push(a.clone());
            (self.handler)(&a)
        }
    }

    const STATUS_Z: &str = "## feature/x...origin/feature/x [ahead 1]\0 M src/a.rs\0?? src/b.rs\0R  src/new.rs\0src/old.rs\0 D src/gone.rs\0";

    fn happy(status_text: &'static str) -> FakeGit {
        FakeGit::new(move |a| match a.first().map(String::as_str) {
            Some("rev-parse") => Ok(out(0, "D:/repo\n/nonexistent-nexus-test/.git\n", "")),
            Some("status") => Ok(out(0, status_text, "")),
            Some("remote") => Ok(out(0, "https://github.com/octo/widgets.git\n", "")),
            _ => Ok(out(0, "", "")),
        })
    }

    fn step() -> FollowUp {
        FollowUp::CommitAndPr {
            dir: PathBuf::from("D:/repo/sub"),
            agent: "Claude Code".into(),
            stamp: "20261020-1530".into(),
            message_hint: Some("## Added the follow-up planner.\nAll 12 tests passed.".into()),
            base: "main".into(),
        }
    }

    fn st(text: &str) -> GitStatus {
        parse_porcelain(text)
    }

    fn hold() -> Verdict {
        Verdict::Hold(vec![Reason::FailureMarker("error".into(), "error[E0432]: unresolved import".into())])
    }

    fn unsure() -> Verdict {
        Verdict::Unsure(vec![Reason::LowConfidence])
    }

    // ── parsing ──

    #[test]
    fn parses_nul_separated_status() {
        let s = st(STATUS_Z);
        assert_eq!(s.branch.as_deref(), Some("feature/x"));
        assert!(!s.detached && !s.unborn);
        let paths: Vec<&str> = s.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["src/a.rs", "src/b.rs", "src/new.rs", "src/gone.rs"]);
        assert_eq!(s.entries[2].orig_path.as_deref(), Some("src/old.rs"));
        assert_eq!((s.entries[0].x, s.entries[0].y), (' ', 'M'));
    }

    #[test]
    fn parses_newline_status_with_quotes_and_rename() {
        let s = st("## main\n M \"with space.txt\"\nR  old.rs -> new.rs\n?? plain.rs\n");
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(s.entries[0].path, "with space.txt");
        assert_eq!(s.entries[1].path, "new.rs");
        assert_eq!(s.entries[1].orig_path.as_deref(), Some("old.rs"));
        assert_eq!(s.entries.len(), 3);
    }

    #[test]
    fn parses_detached_unborn_and_unmerged() {
        assert!(st("## HEAD (no branch)\n M a.rs\n").detached);
        let u = st("## No commits yet on main\n?? a.rs\n");
        assert!(u.unborn);
        assert_eq!(u.branch.as_deref(), Some("main"));
        let c = st("## main\nUU conflict.rs\nAA both.rs\nDD gone.rs\n M fine.rs\n");
        assert_eq!(c.entries.iter().filter(|e| e.is_unmerged()).count(), 3);
        assert_eq!(st("## main\n!! ignored.log\n").entries.len(), 0);
    }

    // ── the gate ──

    #[test]
    fn hold_and_unsure_never_call_the_runner() {
        for v in [hold(), unsure()] {
            let fake = happy(STATUS_Z);
            let d = prepare(&fake, &step(), &v);
            assert!(matches!(d, Decision::Skip(SkipReason::NotClean(_))), "{d:?}");
            assert!(fake.calls().is_empty(), "git must not be touched: {:?}", fake.calls());
        }
    }

    #[test]
    fn plan_skips_hold_and_unsure_even_with_a_perfect_status() {
        let s = st(STATUS_Z);
        assert!(matches!(plan_followup(&hold(), &step(), &s), Decision::Skip(SkipReason::NotClean(_))));
        assert!(matches!(plan_followup(&unsure(), &step(), &s), Decision::Skip(SkipReason::NotClean(_))));
        assert!(matches!(plan_followup(&Verdict::Go, &step(), &s), Decision::Run(_)));
    }

    #[test]
    fn hold_sentence_quotes_the_failure_line() {
        let d = plan_followup(&hold(), &step(), &GitStatus::default());
        let said = describe(&d);
        assert!(said.starts_with("Commit stopped because errors were found in the response: "), "{said}");
        assert!(said.contains("the output mentions a failure (error)"), "{said}");
        assert!(said.contains("The line was: \"error[E0432]: unresolved import\"."), "{said}");
        let u = describe(&plan_followup(&unsure(), &step(), &GitStatus::default()));
        assert!(u.starts_with("Commit stopped because I could not confirm"), "{u}");
        assert!(u.contains("I am not confident enough"), "{u}");
    }

    // ── the happy path ──

    #[test]
    fn go_runs_the_exact_git_sequence() {
        let fake = happy(STATUS_Z);
        let Decision::Run(steps) = prepare(&fake, &step(), &Verdict::Go) else { panic!("expected Run") };
        assert_eq!(steps.dir, PathBuf::from("D:/repo"), "git runs from the repository root, not the sub folder");
        let outcome = execute_git(&fake, &steps);
        assert_eq!(outcome.failure, None);
        assert!(outcome.branch_created && outcome.staged && outcome.committed && outcome.pushed);
        assert_eq!(outcome.repo.as_deref(), Some("octo/widgets"));
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            fake.calls(),
            vec![
                v(&["rev-parse", "--show-toplevel", "--absolute-git-dir"]),
                v(&["status", "--porcelain=v1", "-z", "--untracked-files=all", "-b"]),
                v(&["checkout", "-b", "nexus/claude-code-20261020-1530"]),
                v(&["add", "--", "src/a.rs", "src/b.rs", "src/new.rs", "src/gone.rs"]),
                v(&["commit", "-m", "Added the follow-up planner"]),
                v(&["push", "-u", "origin", "nexus/claude-code-20261020-1530"]),
                v(&["remote", "get-url", "origin"]),
            ]
        );
        assert!(describe(&Decision::Run(steps)).contains("draft pull request against main"));
    }

    #[test]
    fn staging_lists_specific_paths_never_a_blanket_add() {
        let Decision::Run(steps) = plan_followup(&Verdict::Go, &step(), &st(STATUS_Z)) else { panic!("expected Run") };
        let add = steps.git_calls().into_iter().find(|c| c.stage == Stage::Stage).unwrap().args;
        assert_eq!(add[..2], ["add".to_string(), "--".to_string()]);
        assert_eq!(add.len(), 2 + steps.paths.len());
        for blanket in ["-A", "--all", ".", "-u", "*"] {
            assert!(!add.iter().any(|a| a == blanket), "blanket add `{blanket}` in {add:?}");
        }
        assert_eq!(&add[2..], steps.paths.as_slice());
    }

    // ── refusals ──

    #[test]
    fn secrets_refuse_the_whole_commit_and_name_the_files() {
        let s = st("## main\n M src/a.rs\n?? .env.local\n?? deploy/server.pem\n");
        let Decision::Skip(SkipReason::Secrets(files)) = plan_followup(&Verdict::Go, &step(), &s) else { panic!("expected Secrets") };
        assert_eq!(files, [".env.local", "deploy/server.pem"]);
        let said = describe(&Decision::Skip(SkipReason::Secrets(files)));
        assert!(said.contains(".env.local") && said.contains("server.pem") && said.contains("will not add"), "{said}");
        // a secret as the old name of a rename is caught too
        let r = st("## main\0R  fine.txt\0.env\0");
        assert!(matches!(plan_followup(&Verdict::Go, &step(), &r), Decision::Skip(SkipReason::Secrets(_))));
    }

    #[test]
    fn secrets_refusal_makes_no_mutating_call() {
        let fake = happy("## main\0?? .env\0 M a.rs\0");
        assert!(matches!(prepare(&fake, &step(), &Verdict::Go), Decision::Skip(SkipReason::Secrets(_))));
        assert!(fake.calls().iter().all(|c| matches!(c[0].as_str(), "rev-parse" | "status")), "{:?}", fake.calls());
    }

    #[test]
    fn secret_path_table() {
        for p in [
            ".env", ".env.production", "app/.env", "k.pem", "a/b/server.KEY", "id_rsa", "id_rsa.pub", "id_ed25519", "credentials.json",
            "aws/credentials", "x.pfx", "x.p12", "api_token.txt", "client_secret.json", ".ssh/config", "C:\\Users\\me\\.aws\\config", ".netrc",
        ] {
            assert!(is_secret_path(p), "{p} should be refused");
        }
        for p in ["src/main.rs", "README.md", "docs/environment.md","keyboard.rs", "src/monkey.ts"] {
            assert!(!is_secret_path(p), "{p} should be allowed");
        }
    }

    #[test]
    fn skips_for_empty_detached_unborn_conflicts_in_progress_and_too_many() {
        let go = Verdict::Go;
        assert_eq!(plan_followup(&go, &step(), &st("## main\n")), Decision::Skip(SkipReason::NothingChanged));
        assert_eq!(plan_followup(&go, &step(), &st("## HEAD (no branch)\n M a.rs\n")), Decision::Skip(SkipReason::DetachedHead));
        assert_eq!(plan_followup(&go, &step(), &st("## No commits yet on main\n?? a.rs\n")), Decision::Skip(SkipReason::NoCommits));
        let c = plan_followup(&go, &step(), &st("## main\nUU a.rs\n M b.rs\n"));
        assert_eq!(c, Decision::Skip(SkipReason::Conflicts(vec!["a.rs".into()])));
        let mut mid = st("## main\n M a.rs\n");
        mid.in_progress = Some("rebase".into());
        let d = plan_followup(&go, &step(), &mid);
        assert_eq!(d, Decision::Skip(SkipReason::InProgress("rebase".into())));
        assert!(describe(&d).contains("middle of a rebase"));
        let many: String = (0..=MAX_FILES).map(|i| format!(" M f{i}.rs\n")).collect();
        assert_eq!(plan_followup(&go, &step(), &st(&format!("## main\n{many}"))), Decision::Skip(SkipReason::TooManyFiles(MAX_FILES + 1)));
        for r in [SkipReason::NothingChanged, SkipReason::DetachedHead, SkipReason::NoCommits] {
            assert!(!r.spoken().is_empty());
        }
    }

    #[test]
    fn unreadable_repository_skips_without_mutating() {
        let fake = FakeGit::new(|_| Ok(out(128, "", "fatal: not a git repository (or any of the parent directories): .git")));
        let d = prepare(&fake, &step(), &Verdict::Go);
        let Decision::Skip(SkipReason::NotARepo(why)) = &d else { panic!("{d:?}") };
        assert!(why.contains("not a git repository"));
        assert_eq!(fake.calls().len(), 1);
    }

    #[test]
    fn bad_base_branch_is_refused() {
        let FollowUp::CommitAndPr { dir, agent, stamp, message_hint, .. } = step();
        let bad = FollowUp::CommitAndPr { dir: dir.clone(), agent: agent.clone(), stamp: stamp.clone(), message_hint: message_hint.clone(), base: "--delete".into() };
        assert!(matches!(plan_followup(&Verdict::Go, &bad, &st(STATUS_Z)), Decision::Skip(SkipReason::BadBase(_))));
        let empty = FollowUp::CommitAndPr { dir, agent, stamp, message_hint, base: "  ".into() };
        let Decision::Run(s) = plan_followup(&Verdict::Go, &empty, &st(STATUS_Z)) else { panic!() };
        assert_eq!(s.base, "main");
    }

    // ── failures while running ──

    #[test]
    fn push_failure_is_reported_plainly_and_keeps_the_local_commit() {
        let fake = FakeGit::new(|a| match a[0].as_str() {
            "rev-parse" => Ok(out(0, "D:/repo\n/nonexistent-nexus-test/.git\n", "")),
            "status" => Ok(out(0, STATUS_Z, "")),
            "push" => Ok(out(128, "", "fatal: unable to access 'https://user:ghp_secret@github.com/o/r.git/': could not read Username")),
            _ => Ok(out(0, "", "")),
        });
        let Decision::Run(steps) = prepare(&fake, &step(), &Verdict::Go) else { panic!() };
        let o = execute_git(&fake, &steps);
        assert!(o.committed && !o.pushed && o.repo.is_none());
        assert_eq!(o.failure.as_ref().map(|f| f.0), Some(Stage::Push));
        let r = describe_outcome(&o, None);
        assert!(r.spoken.contains("push failed") && r.spoken.contains("still on your laptop") && r.spoken.contains("no pull request"), "{}", r.spoken);
        assert!(!r.spoken.contains("ghp_secret") && !r.spoken.contains("user:"), "credentials leaked: {}", r.spoken);
        assert!(r.committed && !r.pushed && !r.pr_opened);
        // no `remote` lookup and nothing after the push
        assert_eq!(fake.calls().last().unwrap()[0], "push");
        // nothing was ever undone
        assert!(!fake.calls().iter().any(|c| matches!(c[0].as_str(), "reset" | "branch" | "checkout") && c.contains(&"-D".to_string())));
    }

    #[test]
    fn a_failing_step_stops_the_ones_after_it() {
        let fake = FakeGit::new(|a| match a[0].as_str() {
            "rev-parse" => Ok(out(0, "D:/repo\n/nonexistent-nexus-test/.git\n", "")),
            "status" => Ok(out(0, STATUS_Z, "")),
            "checkout" => Ok(out(128, "", "fatal: a branch named 'x' already exists")),
            _ => Ok(out(0, "", "")),
        });
        let Decision::Run(steps) = prepare(&fake, &step(), &Verdict::Go) else { panic!() };
        let o = execute_git(&fake, &steps);
        assert_eq!(o.failure.as_ref().map(|f| f.0), Some(Stage::Branch));
        assert!(!o.staged && !o.committed && !o.pushed);
        assert_eq!(fake.calls().len(), 3);
        assert!(describe_outcome(&o, None).spoken.contains("Nothing was changed"));
    }

    #[test]
    fn runner_error_such_as_a_timeout_is_a_plain_failure() {
        let fake = FakeGit::new(|a| match a[0].as_str() {
            "rev-parse" => Ok(out(0, "D:/repo\n/nonexistent-nexus-test/.git\n", "")),
            "status" => Ok(out(0, STATUS_Z, "")),
            "push" => Err("git push timed out after 60 s".into()),
            _ => Ok(out(0, "", "")),
        });
        let Decision::Run(steps) = prepare(&fake, &step(), &Verdict::Go) else { panic!() };
        let o = execute_git(&fake, &steps);
        assert!(o.failure.as_ref().unwrap().1.contains("timed out after 60 s"));
        assert!(describe_outcome(&o, None).spoken.contains("timed out"));
    }

    // ── pull request ──

    #[test]
    fn pr_is_a_draft_through_the_existing_command() {
        let Decision::Run(steps) = plan_followup(&Verdict::Go, &step(), &st(STATUS_Z)) else { panic!() };
        match pr_command(&steps, "octo/widgets") {
            GitHubCommand::CreatePr { repo, title, head, base, body, draft } => {
                assert!(draft, "the PR must be a draft");
                assert_eq!(repo, "octo/widgets");
                assert_eq!(title, "Added the follow-up planner");
                assert_eq!(head, "nexus/claude-code-20261020-1530");
                assert_eq!(base, "main");
                assert!(body.unwrap().contains("src/a.rs"));
            }
            other => panic!("wrong command {other:?}"),
        }
    }

    #[test]
    fn pr_results_map_to_outcomes_and_reports() {
        let ok = pr_outcome(&GitHubResult::Text { text: "Created PR #7: t in o/r. (draft) View at: https://github.com/o/r/pull/7".into() });
        assert_eq!(ok, PrOutcome::Opened { url: Some("https://github.com/o/r/pull/7".into()) });
        let bad = pr_outcome(&GitHubResult::Error { message: "A pull request already exists".into(), status: Some(422), is_auth_error: false });
        assert!(matches!(bad, PrOutcome::Failed(_)));
        let o = GitOutcome { branch: "nexus/x-1".into(), file_count: 3, branch_created: true, staged: true, committed: true, pushed: true, repo: Some("o/r".into()), failure: None };
        let good = describe_outcome(&o, Some(&ok));
        assert!(good.pr_opened && good.pr_url.is_some() && good.spoken.contains("draft pull request"), "{good:?}");
        let failed = describe_outcome(&o, Some(&bad));
        assert!(!failed.pr_opened && failed.pushed && failed.spoken.contains("opening the pull request failed"), "{failed:?}");
        let norepo = describe_outcome(&GitOutcome { repo: None, ..o }, None);
        assert!(norepo.spoken.contains("no pull request was opened"), "{norepo:?}");
    }

    #[test]
    fn github_remote_parsing() {
        for (u, want) in [
            ("https://github.com/octo/widgets.git", Some("octo/widgets")),
            ("https://github.com/octo/widgets", Some("octo/widgets")),
            ("https://user:tok@github.com/octo/widgets.git", Some("octo/widgets")),
            ("git@github.com:octo/widgets.git", Some("octo/widgets")),
            ("ssh://git@github.com/octo/widgets.git", Some("octo/widgets")),
            ("https://gitlab.com/octo/widgets.git", None),
            ("https://github.com/octo", None),
            ("https://github.com/a/b/c", None),
            ("/some/local/path.git", None),
        ] {
            assert_eq!(parse_github_repo(u).as_deref(), want, "{u}");
        }
    }

    // ── text helpers ──

    #[test]
    fn commit_messages_are_cleaned_and_capped() {
        assert_eq!(commit_message(Some("## **Added** the `parser`.\nmore"), "claude"), "Added the parser");
        assert_eq!(commit_message(Some("\n\n- Fixed the race in the queue.\n"), "claude"), "Fixed the race in the queue");
        let long = "Implemented a very long and winding explanation of everything that was changed across the repository today";
        let m = commit_message(Some(long), "claude");
        assert!(m.chars().count() <= MAX_SUBJECT_CHARS, "{m}");
        assert!(long.starts_with(&m) && m.len() > 40, "{m}");
        assert_eq!(commit_message(None, "Claude Code"), "NEXUS: changes from Claude Code");
        assert_eq!(commit_message(Some("   \n"), "codex"), "NEXUS: changes from codex");
        assert_eq!(commit_message(Some("ok"), "codex"), "NEXUS: changes from codex");
        assert_eq!(commit_message(Some("```\nok"), "codex"), "NEXUS: changes from codex", "a fence is skipped, `ok` is too short");
        assert_eq!(commit_message(Some("```\nFixed the bug"), "codex"), "Fixed the bug");
        assert_eq!(commit_message(Some("é".repeat(100).as_str()), "a").chars().count(), MAX_SUBJECT_CHARS);
    }

    #[test]
    fn branch_names_are_safe() {
        assert_eq!(branch_name("Claude Code", "20261020-1530"), "nexus/claude-code-20261020-1530");
        assert_eq!(branch_name("../evil; rm -rf", "20261020-1530"), "nexus/evil-rm-rf-20261020-1530");
        assert_eq!(branch_name("", ""), "nexus/agent-now");
        assert!(valid_ref(&branch_name("Antigravity (IDE)", "x y")));
    }

    #[test]
    fn url_credentials_are_scrubbed() {
        assert_eq!(scrub_url_credentials("fatal: https://u:p@github.com/o/r.git failed"), "fatal: https://github.com/o/r.git failed");
        assert_eq!(scrub_url_credentials("no url here"), "no url here");
    }

    #[test]
    fn in_progress_markers_are_found() {
        let dir = std::env::temp_dir().join(format!("nexus-followup-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(in_progress_marker(&dir), None);
        std::fs::write(dir.join("MERGE_HEAD"), "x").unwrap();
        assert_eq!(in_progress_marker(&dir).as_deref(), Some("merge"));
        std::fs::remove_file(dir.join("MERGE_HEAD")).unwrap();
        std::fs::create_dir_all(dir.join("rebase-merge")).unwrap();
        assert_eq!(in_progress_marker(&dir).as_deref(), Some("rebase"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every git call is an argument array: no shell, no formatted command line, no blanket add.
    #[test]
    fn production_code_never_builds_a_shell_command() {
        let src = include_str!("agent_followup.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        for needle in [
            ["cmd", " /C"].concat(),
            ["cmd", ".exe"].concat(),
            ["sh", " -c"].concat(),
            ["\"/", "C\""].concat(),
            ["Command::new(", "format!"].concat(),
            [".arg(", "format!"].concat(),
            [".args(", "format!"].concat(),
            ["\"-", "A\""].concat(),
            ["\"--", "all\""].concat(),
            ["\"add\", ", "\".\""].concat(),
        ] {
            assert!(!prod.contains(&needle), "forbidden `{needle}` in production code");
        }
        assert!(prod.contains("Command::new(\"git\")"));
    }

    // ───── real git, real repositories on disk (a bare local repository stands in for GitHub) ─────

    fn have_git() -> bool {
        std::process::Command::new("git").arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let o = std::process::Command::new("git").args(["-c", "core.quotepath=off"]).args(args).current_dir(dir).output().expect("git runs");
        assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
        String::from_utf8_lossy(&o.stdout).trim().to_string()
    }

    /// A working repository with one commit on `main`, pushed to a bare repository next to it.
    fn real_repo(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("nexus_followup_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (work, remote) = (base.join("work"), base.join("remote.git"));
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--bare", "-b", "main"]);
        git(&work, &["init", "-b", "main"]);
        git(&work, &["config", "user.name", "Nexus Test"]);
        git(&work, &["config", "user.email", "test@example.invalid"]);
        git(&work, &["config", "commit.gpgsign", "false"]);
        std::fs::write(work.join("README.md"), "first\n").unwrap();
        git(&work, &["add", "--", "README.md"]);
        git(&work, &["commit", "-m", "initial"]);
        git(&work, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&work, &["push", "-u", "origin", "main"]);
        (work, remote)
    }

    fn real_step(work: &Path) -> FollowUp {
        FollowUp::CommitAndPr {
            dir: work.to_path_buf(),
            agent: "Claude Code".into(),
            stamp: "20261020-1200".into(),
            message_hint: Some("Added the parser.\n\nMore detail the subject must not carry.".into()),
            base: "main".into(),
        }
    }

    #[test]
    fn real_git_branches_commits_and_pushes_only_the_changed_files() {
        if !have_git() { return; }
        let (work, remote) = real_repo("happy");
        std::fs::write(work.join("README.md"), "changed\n").unwrap();
        std::fs::write(work.join("new file \u{e9}.txt"), "x").unwrap();
        std::fs::write(work.join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(work.join("junk.log"), "ignored").unwrap();
        let runner = RealGit::default();
        let Decision::Run(steps) = prepare(&runner, &real_step(&work), &Verdict::Go) else { panic!("expected a plan") };
        assert!(!steps.paths.iter().any(|p| p.contains("junk.log")), "{:?}", steps.paths);
        let out = execute_git(&runner, &steps);
        assert!(out.failure.is_none() && out.branch_created && out.staged && out.committed && out.pushed, "{out:?}");
        // the remote has the new branch with one commit whose subject is the agent's first line
        assert!(git(&remote, &["branch", "--format=%(refname:short)"]).lines().any(|b| b == steps.branch), "{}", git(&remote, &["branch"]));
        assert_eq!(git(&remote, &["log", "-1", "--format=%s", &steps.branch]), "Added the parser");
        let files = git(&remote, &["show", "--name-only", "--format=", &steps.branch]);
        assert!(files.contains("README.md") && files.contains("new file \u{e9}.txt") && files.contains(".gitignore"), "{files}");
        assert!(!files.contains("junk.log"), "an ignored file must not be committed: {files}");
        // main is untouched on both sides
        assert_eq!(git(&remote, &["log", "--format=%s", "main"]), "initial");
        assert_eq!(git(&work, &["log", "--format=%s", "main"]), "initial");
        let _ = std::fs::remove_dir_all(work.parent().unwrap());
    }

    #[test]
    fn real_git_refuses_secrets_and_leaves_no_branch_behind() {
        if !have_git() { return; }
        let (work, remote) = real_repo("secrets");
        std::fs::write(work.join("README.md"), "changed\n").unwrap();
        std::fs::write(work.join(".env"), "TOKEN=abc\n").unwrap();
        std::fs::write(work.join("server.pem"), ["-----BEGIN", " PRIVATE KEY-----\n"].concat()).unwrap();
        let runner = RealGit::default();
        let d = prepare(&runner, &real_step(&work), &Verdict::Go);
        match d {
            Decision::Skip(SkipReason::Secrets(files)) => {
                assert!(files.iter().any(|f| f.contains(".env")) && files.iter().any(|f| f.contains("server.pem")), "{files:?}");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(git(&work, &["branch", "--format=%(refname:short)"]), "main", "no branch was created");
        assert_eq!(git(&remote, &["branch", "--format=%(refname:short)"]), "main");
        assert!(git(&work, &["status", "--porcelain"]).contains("README.md"), "the work is still uncommitted");
        let _ = std::fs::remove_dir_all(work.parent().unwrap());
    }

    #[test]
    fn real_git_nothing_changed_detached_head_and_unfinished_merge_are_skipped() {
        if !have_git() { return; }
        let runner = RealGit::default();
        // nothing changed
        let (work, _remote) = real_repo("clean");
        assert!(matches!(prepare(&runner, &real_step(&work), &Verdict::Go), Decision::Skip(SkipReason::NothingChanged)));
        // detached HEAD with a change
        std::fs::write(work.join("README.md"), "changed\n").unwrap();
        git(&work, &["checkout", "--detach"]);
        assert!(matches!(prepare(&runner, &real_step(&work), &Verdict::Go), Decision::Skip(SkipReason::DetachedHead)));
        git(&work, &["checkout", "-f", "main"]);
        // a merge in progress
        std::fs::write(work.join("README.md"), "changed again\n").unwrap();
        let head = git(&work, &["rev-parse", "HEAD"]);
        std::fs::write(work.join(".git").join("MERGE_HEAD"), format!("{head}\n")).unwrap();
        assert!(matches!(prepare(&runner, &real_step(&work), &Verdict::Go), Decision::Skip(SkipReason::InProgress(_))));
        let _ = std::fs::remove_dir_all(work.parent().unwrap());
    }

    #[test]
    fn real_git_a_hold_or_unsure_verdict_never_touches_the_repository() {
        if !have_git() { return; }
        let (work, remote) = real_repo("hold");
        std::fs::write(work.join("README.md"), "changed\n").unwrap();
        let before = git(&work, &["status", "--porcelain"]);
        let runner = RealGit::default();
        for v in [hold(), unsure()] {
            assert!(matches!(prepare(&runner, &real_step(&work), &v), Decision::Skip(SkipReason::NotClean(_))));
        }
        assert_eq!(git(&work, &["status", "--porcelain"]), before);
        assert_eq!(git(&work, &["branch", "--format=%(refname:short)"]), "main");
        assert_eq!(git(&remote, &["branch", "--format=%(refname:short)"]), "main");
        let _ = std::fs::remove_dir_all(work.parent().unwrap());
    }

    #[test]
    fn real_git_a_failed_push_keeps_the_local_commit_and_reports_the_push_stage() {
        if !have_git() { return; }
        let (work, remote) = real_repo("pushfail");
        std::fs::write(work.join("README.md"), "changed\n").unwrap();
        let runner = RealGit::default();
        let Decision::Run(steps) = prepare(&runner, &real_step(&work), &Verdict::Go) else { panic!("expected a plan") };
        std::fs::remove_dir_all(&remote).unwrap(); // the remote disappears
        let out = execute_git(&runner, &steps);
        assert!(out.committed && !out.pushed, "{out:?}");
        assert!(matches!(out.failure, Some((Stage::Push, _))), "{:?}", out.failure);
        assert_eq!(git(&work, &["log", "-1", "--format=%s", &steps.branch]), "Added the parser", "the local commit is kept");
        let said = describe_outcome(&out, None).spoken;
        assert!(!said.is_empty());
        let _ = std::fs::remove_dir_all(work.parent().unwrap());
    }
}
