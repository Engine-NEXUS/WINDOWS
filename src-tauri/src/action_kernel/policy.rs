//! Policy engine (docs/features/140, P1): data, not code.
//!
//! Evaluation order, strictest first: Forbidden locations -> outside the allowed roots ->
//! provenance (text NEXUS read can never delete) -> item caps -> user rules (deny, then ask, then
//! allow; first match wins inside each effect) -> the mode default for the tool's risk class.
//! Everything is decided on canonical paths (see `paths.rs`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths::{self, is_within};
use super::{Origin, Risk, Tool};

pub const POLICY_FILE: &str = "action_policy.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Reads and undoable actions are instant, anything destructive asks first (default).
    Smart,
    /// Everything except reading asks first.
    Ask,
    /// Destructive actions are refused outright.
    NeverDestructive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Deny,
    Ask,
    Allow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub effect: Effect,
    /// A tool name (`delete`, `move`, ...) or `*`.
    pub tool: String,
    /// `*` and `?` glob. Without a path separator it is matched against the file name, otherwise
    /// against the whole canonical path. Case-insensitive.
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Policy {
    pub version: u32,
    /// Master switch for voice file commands.
    pub enabled: bool,
    pub mode: Mode,
    /// Downloads, Desktop and Documents of the current user.
    pub use_default_roots: bool,
    /// Extra folders the user added in the Hub (project folders and so on).
    pub extra_roots: Vec<String>,
    /// Extra locations that are never touched.
    pub protected_paths: Vec<String>,
    pub rules: Vec<Rule>,
    /// Hard cap on how many items one voice command may act on.
    pub max_items: usize,
    /// A folder holding more files than this always needs a spoken confirmation.
    pub max_folder_files: u64,
    /// How long quarantined items are kept before the age purge.
    pub quarantine_days: i64,
    /// How long audit lines are kept.
    pub audit_days: i64,
    /// What a coding agent started by voice may do on its own (see `safe_agent_preset`).
    pub agent_approval: String,
}

fn rule(effect: Effect, tool: &str, pattern: &str) -> Rule {
    Rule { effect, tool: tool.into(), pattern: pattern.into() }
}

impl Default for Policy {
    fn default() -> Self {
        let mut rules = Vec::new();
        // secrets and keys are never moved, renamed, copied or deleted by voice
        for p in ["*.kdbx", "*.pem", "*.key", "*.pfx", "*.p12", "id_rsa*", "id_ed25519*", ".env", "*.env", "*.ppk"] {
            for t in ["delete", "move", "rename", "copy"] {
                rules.push(rule(Effect::Deny, t, p));
            }
        }
        // programs and installers: always read the name back first, even if a user rule allows it
        for p in ["*.exe", "*.msi", "*.dll", "*.bat", "*.cmd", "*.ps1"] {
            rules.push(rule(Effect::Ask, "move", p));
            rules.push(rule(Effect::Ask, "rename", p));
        }
        Policy {
            version: 1,
            enabled: true,
            mode: Mode::Smart,
            use_default_roots: true,
            extra_roots: Vec::new(),
            protected_paths: Vec::new(),
            rules,
            max_items: 50,
            max_folder_files: 200,
            quarantine_days: 30,
            audit_days: 90,
            agent_approval: "edit".into(),
        }
    }
}

/// The only approval presets a voice-started coding agent may run with. "edit" (the default) lets
/// it read and edit files in its working folder but not run commands, browse or start sub-agents;
/// "standard" and "permissive" make it stop and ask. There is deliberately no "autonomous": it
/// maps to flags like --dangerously-skip-permissions / --full-auto. Anything else falls back to edit.
pub fn safe_agent_preset(requested: &str) -> &'static str {
    match requested {
        "readonly" => "readonly",
        "standard" => "standard",
        "permissive" => "permissive",
        _ => "edit",
    }
}

pub fn load(dir: &Path) -> Policy {
    std::fs::read_to_string(dir.join(POLICY_FILE))
        .ok()
        .and_then(|s| serde_json::from_str::<Policy>(&s).ok())
        .unwrap_or_default()
}

pub fn save(dir: &Path, policy: &Policy) -> Result<(), String> {
    let tmp = dir.join(format!("{POLICY_FILE}.tmp"));
    let text = serde_json::to_string_pretty(policy).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, dir.join(POLICY_FILE)).map_err(|e| e.to_string())
}

/// What the machine looks like, detected once; tests build their own.
#[derive(Debug, Clone)]
pub struct PolicyEnv {
    pub home: PathBuf,
    pub users_dir: Option<PathBuf>,
    pub system_dirs: Vec<PathBuf>,
    /// NEXUS's own data directory (settings, vault, memory, quarantine manifest).
    pub data_dir: PathBuf,
}

impl PolicyEnv {
    pub fn detect(data_dir: &Path) -> Self {
        let canon = |p: PathBuf| paths::canonical(&p).unwrap_or(p);
        let home = canon(dirs_next::home_dir().unwrap_or_default());
        let users_dir = home.parent().map(|p| p.to_path_buf());
        let mut system_dirs = Vec::new();
        for var in ["SystemRoot", "windir", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432", "ProgramData"] {
            if let Some(v) = std::env::var_os(var) {
                system_dirs.push(canon(PathBuf::from(v)));
            }
        }
        if !cfg!(windows) {
            for p in ["/etc", "/usr", "/bin", "/sbin", "/var", "/boot", "/sys", "/proc", "/dev", "/System", "/Library"] {
                system_dirs.push(PathBuf::from(p));
            }
        }
        PolicyEnv { home, users_dir, system_dirs, data_dir: canon(data_dir.to_path_buf()) }
    }
}

/// Validate and normalise a policy edited in the Hub before it is saved. Extra folders must exist,
/// are stored canonical, and may not be a drive, the user profile, the folder of all profiles,
/// Windows/Program Files or NEXUS's own data: the same places the kernel refuses anyway, refused at
/// the door so the page can say why.
pub fn sanitize(mut p: Policy, env: &PolicyEnv) -> Result<Policy, String> {
    p.version = 1;
    p.agent_approval = safe_agent_preset(&p.agent_approval).to_string();
    p.max_items = p.max_items.clamp(1, 200);
    p.max_folder_files = p.max_folder_files.clamp(1, 100_000);
    p.quarantine_days = p.quarantine_days.clamp(1, 365);
    p.audit_days = p.audit_days.clamp(7, 3650);
    if p.extra_roots.len() > 20 || p.rules.len() > 200 || p.protected_paths.len() > 100 {
        return Err("That's more folders or rules than I can keep, sir.".into());
    }
    let mut roots: Vec<String> = Vec::new();
    for r in &p.extra_roots {
        let c = paths::canonical(Path::new(r.trim())).map_err(|e| format!("{r}: {}", e.spoken()))?;
        if !c.is_dir() {
            return Err(format!("{r} is not a folder that exists."));
        }
        if let Some(why) = forbidden_reason(&c, env, &p, &[], true) {
            return Err(format!("{r} can't be a work folder: {why}."));
        }
        let s = c.to_string_lossy().to_string();
        if !roots.iter().any(|x| paths::key(Path::new(x)) == paths::key(&c)) {
            roots.push(s);
        }
    }
    p.extra_roots = roots;
    let mut protected = Vec::new();
    for r in &p.protected_paths {
        let t = r.trim();
        if t.is_empty() {
            continue;
        }
        protected.push(t.to_string());
    }
    p.protected_paths = protected;
    for r in &mut p.rules {
        r.pattern = r.pattern.trim().to_string();
        let known = ["*", "delete", "move", "copy", "rename", "create_folder"];
        if r.pattern.is_empty() || r.pattern.len() > 200 || !r.pattern.chars().any(|c| c.is_alphanumeric()) {
            return Err("A rule needs a pattern with at least one letter or digit.".into());
        }
        if !known.contains(&r.tool.as_str()) {
            return Err(format!("{} is not an action I know.", r.tool));
        }
    }
    Ok(p)
}

/// Folders the voice layer may work in, canonical and existing.
pub fn effective_roots(policy: &Policy) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: Option<PathBuf>| {
        if let Some(p) = p {
            if let Ok(c) = paths::canonical(&p) {
                if c.is_dir() && !out.iter().any(|o| paths::key(o) == paths::key(&c)) {
                    out.push(c);
                }
            }
        }
    };
    if policy.use_default_roots {
        push(dirs_next::download_dir());
        push(dirs_next::desktop_dir());
        push(dirs_next::document_dir());
    }
    for r in &policy.extra_roots {
        push(Some(PathBuf::from(r)));
    }
    out
}

/// What the kernel knows about something it was asked to act on (canonical, measured).
#[derive(Debug, Clone)]
pub struct Target {
    pub path: PathBuf,
    pub is_dir: bool,
    /// Files below it (0 for a file); capped, see `truncated`.
    pub files: u64,
    pub bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Needs a spoken yes; the string is why (read back in the preview).
    Ask(String),
    Deny(String),
}

/// A location that is never touched, whatever the tool, mode or rule. `None` = fine.
///
/// `as_dest`: the location only *receives* things (a destination folder), so one of the work
/// folders themselves is fine there; their ancestors still are not.
pub fn forbidden_reason(path: &Path, env: &PolicyEnv, policy: &Policy, roots: &[PathBuf], as_dest: bool) -> Option<String> {
    if paths::is_root(path) {
        return Some("that is a whole drive".into());
    }
    if env.system_dirs.iter().any(|s| is_within(path, s)) {
        return Some("that is part of Windows or the installed programs".into());
    }
    if let Some(users) = &env.users_dir {
        if is_within(path, users) && !is_within(path, &env.home) {
            return Some("that belongs to another user profile".into());
        }
        if paths::key(path) == paths::key(users) {
            return Some("that is the folder holding every profile".into());
        }
    }
    if paths::key(path) == paths::key(&env.home) {
        return Some("that is your whole profile folder".into());
    }
    if is_within(path, &env.data_dir) {
        return Some("that is my own data".into());
    }
    if paths::has_component(path, &[".git", ".ssh", ".gnupg", ".aws", ".nexus-trash"])
        || is_within(path, &env.home.join("AppData"))
    {
        return Some("that is a protected location".into());
    }
    let equal = |r: &PathBuf| paths::key(r) == paths::key(path);
    if roots.iter().any(|r| (equal(r) && !as_dest) || (is_within(r, path) && !equal(r))) {
        return Some("that is one of the folders I work inside, not something inside it".into());
    }
    for p in &policy.protected_paths {
        if let Ok(c) = paths::canonical(Path::new(p)) {
            if is_within(path, &c) {
                return Some("you marked that location as protected".into());
            }
        }
    }
    if paths::is_reparse(path) {
        return Some("that is a shortcut link, not a real folder".into());
    }
    None
}

/// Forbidden + inside-the-roots check for one location (also used by undo).
pub fn guard_path(path: &Path, env: &PolicyEnv, policy: &Policy, roots: &[PathBuf]) -> Result<(), String> {
    guard(path, env, policy, roots, false)
}

/// The same for a folder that only receives things.
pub fn guard_dest(path: &Path, env: &PolicyEnv, policy: &Policy, roots: &[PathBuf]) -> Result<(), String> {
    guard(path, env, policy, roots, true)
}

fn guard(path: &Path, env: &PolicyEnv, policy: &Policy, roots: &[PathBuf], as_dest: bool) -> Result<(), String> {
    if let Some(why) = forbidden_reason(path, env, policy, roots, as_dest) {
        return Err(format!("I won't touch that, sir, {why}."));
    }
    if !roots.iter().any(|r| is_within(path, r)) {
        return Err("That's outside the folders I'm allowed to work in, sir. You can add one in the Command Hub.".into());
    }
    Ok(())
}

/// Simple `*` / `?` glob, case-insensitive, whole-string.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

fn rule_hits(rule: &Rule, tool: Tool, path: &Path) -> bool {
    if rule.tool != "*" && !rule.tool.eq_ignore_ascii_case(tool.name()) {
        return false;
    }
    if rule.pattern.contains('\\') || rule.pattern.contains('/') {
        glob_match(&rule.pattern.replace('/', "\\"), &path.to_string_lossy().replace('/', "\\"))
    } else {
        path.file_name().map_or(false, |n| glob_match(&rule.pattern, &n.to_string_lossy()))
    }
}

/// The first rule of `effect` that matches any location in `paths`.
fn first_rule<'a>(policy: &'a Policy, effect: Effect, tool: Tool, paths: &[&Path]) -> Option<&'a Rule> {
    policy
        .rules
        .iter()
        .filter(|r| r.effect == effect)
        .find(|r| paths.iter().any(|p| rule_hits(r, tool, p)))
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub decision: Decision,
    pub risk: Risk,
}

/// Decide whether `tool` may act on `sources` (and write into `dest`, when it has one).
pub fn evaluate(
    policy: &Policy,
    env: &PolicyEnv,
    roots: &[PathBuf],
    tool: Tool,
    sources: &[Target],
    dest: Option<&Path>,
    origin: Origin,
) -> Verdict {
    let risk = tool.risk();
    let deny = |why: String, risk: Risk| Verdict { decision: Decision::Deny(why), risk };
    let mut all: Vec<&Path> = sources.iter().map(|t| t.path.as_path()).collect();
    if let Some(d) = dest {
        all.push(d);
    }
    // 1 + 2: forbidden locations and the allowed roots
    for (i, p) in all.iter().enumerate() {
        let as_dest = dest.is_some() && i == all.len() - 1;
        if let Err(why) = guard(p, env, policy, roots, as_dest) {
            let forbidden = forbidden_reason(p, env, policy, roots, as_dest).is_some();
            return deny(why, if forbidden { Risk::Forbidden } else { risk });
        }
    }
    // 3: provenance. Text NEXUS read (screen, mail, web, WhatsApp, a webhook) never deletes.
    if origin == Origin::Derived && risk >= Risk::Destructive {
        return deny(
            "That request didn't come from your voice, so I won't delete anything because of it, sir.".into(),
            risk,
        );
    }
    // 4: caps
    if sources.len() > policy.max_items {
        return deny(
            format!("That's {} items, sir, which is too many for one voice command. Narrow it down.", sources.len()),
            risk,
        );
    }
    // 5: user rules, deny -> ask -> allow
    let mut ask_reason: Option<String> = None;
    if let Some(r) = first_rule(policy, Effect::Deny, tool, &all) {
        return deny(
            format!("Your rules don't let me {} items like {}, sir.", tool.name(), r.pattern),
            risk,
        );
    }
    if let Some(r) = first_rule(policy, Effect::Ask, tool, &all) {
        ask_reason = Some(format!("it matches your rule for {}", r.pattern));
    }
    let explicit_allow = first_rule(policy, Effect::Allow, tool, &all).is_some();
    // 6: the mode default
    let mode_ask = match (policy.mode, risk) {
        (Mode::NeverDestructive, r) if r >= Risk::Destructive => {
            return deny("Destructive actions are switched off in your settings, sir.".into(), risk)
        }
        (_, Risk::Read) => false,
        (Mode::Ask, _) => true,
        (_, r) if r >= Risk::Destructive => true,
        _ => false,
    };
    if mode_ask && !explicit_allow && ask_reason.is_none() {
        ask_reason = Some(if risk >= Risk::Destructive { "it deletes".into() } else { "you asked me to check first".into() });
    }
    // a big folder always asks, even when a rule allows it
    if risk >= Risk::Destructive {
        if let Some(big) = sources.iter().find(|t| t.is_dir && (t.files > policy.max_folder_files || t.truncated)) {
            ask_reason = Some(format!(
                "{} holds {}{} files",
                big.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
                big.files,
                if big.truncated { "+" } else { "" }
            ));
        }
    }
    match ask_reason {
        Some(r) => Verdict { decision: Decision::Ask(r), risk },
        None => Verdict { decision: Decision::Allow, risk },
    }
}

#[cfg(test)]
pub(crate) fn test_env(root: &Path) -> (PolicyEnv, Policy, Vec<PathBuf>) {
    let home = root.join("home");
    let users = root.to_path_buf();
    let data = root.join("nexusdata");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    let docs = home.join("Documents");
    std::fs::create_dir_all(&docs).unwrap();
    let env = PolicyEnv {
        home,
        users_dir: Some(users),
        system_dirs: vec![root.join("Windows"), root.join("Program Files")],
        data_dir: data,
    };
    let policy = Policy { use_default_roots: false, extra_roots: vec![docs.to_string_lossy().to_string()], ..Policy::default() };
    let roots = effective_roots(&policy);
    (env, policy, roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_kernel::paths::test_dir;

    fn file_target(p: PathBuf) -> Target {
        Target { path: p, is_dir: false, files: 0, bytes: 10, truncated: false }
    }

    fn setup() -> (PathBuf, PolicyEnv, Policy, Vec<PathBuf>) {
        let root = test_dir("policy");
        let (env, policy, roots) = test_env(&root);
        (root, env, policy, roots)
    }

    fn eval(tool: Tool, p: &Policy, e: &PolicyEnv, r: &[PathBuf], paths: &[&Path], origin: Origin) -> Decision {
        let targets: Vec<Target> = paths.iter().map(|p| file_target(p.to_path_buf())).collect();
        evaluate(p, e, r, tool, &targets, None, origin).decision
    }

    #[test]
    fn glob_basics() {
        assert!(glob_match("*.PDF", "report.pdf"));
        assert!(glob_match("id_rsa*", "id_rsa.pub"));
        assert!(glob_match("a?c", "abc") && !glob_match("a?c", "abbc"));
        assert!(glob_match("*", "") && !glob_match("*.pdf", "pdf"));
        assert!(glob_match("*\\secrets\\*", "d:\\x\\secrets\\a.txt"));
    }

    #[test]
    fn smart_mode_reads_and_reversible_are_instant_delete_asks() {
        let (root, env, policy, roots) = setup();
        let f = roots[0].join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(eval(Tool::List, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Allow);
        assert_eq!(eval(Tool::Move, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Allow);
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Ask(_)));
        let _ = root;
    }

    #[test]
    fn ask_mode_asks_for_everything_but_reads_and_never_mode_refuses_delete() {
        let (_r, env, mut policy, roots) = setup();
        let f = roots[0].join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        policy.mode = Mode::Ask;
        assert_eq!(eval(Tool::Find, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Allow);
        assert!(matches!(eval(Tool::Copy, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Ask(_)));
        policy.mode = Mode::NeverDestructive;
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Deny(_)));
        assert_eq!(eval(Tool::Move, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Allow);
    }

    #[test]
    fn derived_text_can_never_delete_but_may_do_reversible_things() {
        let (_r, env, policy, roots) = setup();
        let f = roots[0].join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::Derived), Decision::Deny(_)));
        assert_eq!(eval(Tool::Copy, &policy, &env, &roots, &[&f], Origin::Derived), Decision::Allow);
    }

    #[test]
    fn forbidden_locations_are_refused_in_every_mode_and_origin() {
        let (root, env, mut policy, roots) = setup();
        std::fs::create_dir_all(root.join("Windows").join("System32")).unwrap();
        std::fs::create_dir_all(root.join("other").join("Documents")).unwrap();
        std::fs::create_dir_all(roots[0].join("proj").join(".git")).unwrap();
        std::fs::write(roots[0].join("proj").join(".git").join("config"), b"x").unwrap();
        let bad: Vec<PathBuf> = vec![
            root.join("Windows").join("System32"),
            root.join("other").join("Documents"),
            env.home.clone(),
            env.data_dir.clone(),
            roots[0].clone(),
            root.clone(),
            roots[0].join("proj").join(".git").join("config"),
        ];
        for mode in [Mode::Smart, Mode::Ask, Mode::NeverDestructive] {
            policy.mode = mode;
            for tool in [Tool::List, Tool::Move, Tool::Delete, Tool::Rename, Tool::Copy] {
                for origin in [Origin::UserVoice, Origin::Derived] {
                    for p in &bad {
                        let d = eval(tool, &policy, &env, &roots, &[p.as_path()], origin);
                        assert!(matches!(d, Decision::Deny(_)), "{mode:?} {tool:?} {origin:?} {p:?} -> {d:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn outside_the_roots_is_refused_even_for_reads() {
        let (root, env, policy, roots) = setup();
        let outside = root.join("somewhere");
        std::fs::create_dir_all(&outside).unwrap();
        let f = outside.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(matches!(eval(Tool::List, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Deny(_)));
    }

    #[test]
    fn a_destination_outside_the_roots_is_refused_too() {
        let (root, env, policy, roots) = setup();
        let f = roots[0].join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let outside = root.join("elsewhere");
        std::fs::create_dir_all(&outside).unwrap();
        let v = evaluate(&policy, &env, &roots, Tool::Move, &[file_target(f)], Some(&outside), Origin::UserVoice);
        assert!(matches!(v.decision, Decision::Deny(_)));
    }

    #[test]
    fn secret_files_are_denied_by_the_default_rules() {
        let (_r, env, policy, roots) = setup();
        for name in ["vault.kdbx", "id_rsa", "id_rsa.pub", "server.pem", ".env", "prod.env", "wallet.key"] {
            let f = roots[0].join(name);
            std::fs::write(&f, b"x").unwrap();
            for tool in [Tool::Delete, Tool::Move, Tool::Rename, Tool::Copy] {
                assert!(
                    matches!(eval(tool, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Deny(_)),
                    "{name} {tool:?}"
                );
            }
        }
    }

    #[test]
    fn deny_beats_ask_beats_allow_and_allow_skips_only_the_mode_prompt() {
        let (_r, env, mut policy, roots) = setup();
        let f = roots[0].join("scratch.tmp");
        std::fs::write(&f, b"x").unwrap();
        policy.rules.push(Rule { effect: Effect::Allow, tool: "delete".into(), pattern: "*.tmp".into() });
        assert_eq!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Allow);
        policy.rules.push(Rule { effect: Effect::Ask, tool: "delete".into(), pattern: "scratch.*".into() });
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Ask(_)));
        policy.rules.push(Rule { effect: Effect::Deny, tool: "*".into(), pattern: "*.tmp".into() });
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::UserVoice), Decision::Deny(_)));
        // an allow rule never lifts the forbidden/roots/provenance gates
        policy.rules.retain(|r| r.effect == Effect::Allow);
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &[&f], Origin::Derived), Decision::Deny(_)));
    }

    #[test]
    fn caps_and_big_folders() {
        let (_r, env, mut policy, roots) = setup();
        policy.max_items = 2;
        let many: Vec<PathBuf> = (0..3).map(|i| roots[0].join(format!("f{i}.txt"))).collect();
        for p in &many {
            std::fs::write(p, b"x").unwrap();
        }
        let refs: Vec<&Path> = many.iter().map(|p| p.as_path()).collect();
        assert!(matches!(eval(Tool::Delete, &policy, &env, &roots, &refs, Origin::UserVoice), Decision::Deny(_)));
        policy.max_items = 50;
        policy.rules.push(Rule { effect: Effect::Allow, tool: "delete".into(), pattern: "*".into() });
        let dir = Target { path: roots[0].join("bigdir"), is_dir: true, files: 500, bytes: 1, truncated: false };
        std::fs::create_dir_all(&dir.path).unwrap();
        let v = evaluate(&policy, &env, &roots, Tool::Delete, &[dir], None, Origin::UserVoice);
        assert!(matches!(v.decision, Decision::Ask(ref r) if r.contains("500")));
    }

    /// Each guard must hold on its own: these tests widen the roots (and drop the other guards) so a
    /// location is refused by exactly one rule.
    fn isolated(tag: &str) -> (PathBuf, PolicyEnv, Policy, Vec<PathBuf>) {
        let root = test_dir(tag);
        let (mut env, mut policy, _) = test_env(&root);
        env.users_dir = None;
        policy.use_default_roots = false;
        policy.extra_roots = vec![root.to_string_lossy().to_string()];
        let roots = effective_roots(&policy);
        (root, env, policy, roots)
    }

    #[test]
    fn windows_and_program_folders_are_refused_by_their_own_rule() {
        let (root, env, policy, roots) = isolated("iso_sys");
        let sys = root.join("Windows").join("System32");
        std::fs::create_dir_all(&sys).unwrap();
        let d = eval(Tool::Delete, &policy, &env, &roots, &[&sys], Origin::UserVoice);
        assert!(matches!(d, Decision::Deny(ref w) if w.contains("Windows")), "{d:?}");
        let ok = root.join("plain");
        std::fs::create_dir_all(&ok).unwrap();
        assert!(!matches!(eval(Tool::Move, &policy, &env, &roots, &[&ok], Origin::UserVoice), Decision::Deny(_)));
    }

    #[test]
    fn other_user_profiles_are_refused_by_their_own_rule() {
        let (root, mut env, policy, roots) = isolated("iso_users");
        env.users_dir = Some(root.clone());
        let other = root.join("someone_else").join("Documents");
        std::fs::create_dir_all(&other).unwrap();
        let d = eval(Tool::List, &policy, &env, &roots, &[&other], Origin::UserVoice);
        assert!(matches!(d, Decision::Deny(ref w) if w.contains("another user")), "{d:?}");
        // the user's own profile (home) is not "another user"
        let mine = env.home.join("Documents").join("x.txt");
        std::fs::write(&mine, b"x").unwrap();
        assert!(!matches!(eval(Tool::List, &policy, &env, &roots, &[&mine], Origin::UserVoice), Decision::Deny(_)));
    }

    #[test]
    fn nexus_own_data_is_refused_by_its_own_rule() {
        let (_root, env, policy, roots) = isolated("iso_data");
        let vault = env.data_dir.join("memory").join("memcore.enc");
        std::fs::create_dir_all(vault.parent().unwrap()).unwrap();
        std::fs::write(&vault, b"sealed").unwrap();
        let d = eval(Tool::Delete, &policy, &env, &roots, &[&vault], Origin::UserVoice);
        assert!(matches!(d, Decision::Deny(ref w) if w.contains("my own data")), "{d:?}");
    }

    #[test]
    fn outside_the_roots_is_refused_by_the_roots_rule_alone() {
        let root = test_dir("iso_roots");
        let (mut env, mut policy, _) = test_env(&root);
        env.users_dir = None;
        let work = root.join("work");
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&work).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        policy.use_default_roots = false;
        policy.extra_roots = vec![work.to_string_lossy().to_string()];
        let roots = effective_roots(&policy);
        let f = elsewhere.join("a.txt");
        std::fs::write(&f, b"x").unwrap();
        let d = eval(Tool::List, &policy, &env, &roots, &[&f], Origin::UserVoice);
        assert!(matches!(d, Decision::Deny(ref w) if w.contains("outside the folders")), "{d:?}");
        let inside = work.join("a.txt");
        std::fs::write(&inside, b"x").unwrap();
        assert_eq!(eval(Tool::List, &policy, &env, &roots, &[&inside], Origin::UserVoice), Decision::Allow);
    }

    #[test]
    fn sanitize_refuses_dangerous_work_folders_and_normalises_the_rest() {
        let root = test_dir("sanitize");
        let (env, base, _) = test_env(&root);
        let good = root.join("projects");
        std::fs::create_dir_all(&good).unwrap();
        let mk = |roots: Vec<String>| Policy { extra_roots: roots, ..base.clone() };
        // (the temp folder stands in for the profiles folder, so the happy path drops that guard)
        let env_ok = PolicyEnv { users_dir: None, ..env.clone() };
        let ok = sanitize(mk(vec![good.to_string_lossy().to_string(), good.to_string_lossy().to_string()]), &env_ok).unwrap();
        assert_eq!(ok.extra_roots.len(), 1, "duplicates collapse");
        let sys = root.join("Windows");
        std::fs::create_dir_all(&sys).unwrap();
        for bad in [
            env.home.clone(),
            sys,
            env.data_dir.clone(),
            root.clone(),
            root.join("missing"),
            PathBuf::from("relative"),
            PathBuf::from(r"\\server\share\x"),
        ] {
            assert!(sanitize(mk(vec![bad.to_string_lossy().to_string()]), &env).is_err(), "{bad:?}");
        }
        let mut p = base.clone();
        p.max_items = 100_000;
        p.quarantine_days = 0;
        let c = sanitize(p, &env).unwrap();
        assert_eq!((c.max_items, c.quarantine_days), (200, 1));
        let mut p = base.clone();
        p.rules.push(Rule { effect: Effect::Deny, tool: "format".into(), pattern: "*.x".into() });
        assert!(sanitize(p, &env).is_err());
        let mut p = base.clone();
        p.rules.push(Rule { effect: Effect::Deny, tool: "delete".into(), pattern: "***".into() });
        assert!(sanitize(p, &env).is_err());
    }

    #[test]
    fn voice_started_agents_can_never_get_a_bypass_preset() {
        for ok in ["readonly", "standard", "permissive", "edit"] {
            assert_eq!(safe_agent_preset(ok), ok);
        }
        for bad in ["autonomous", "yolo", "full-auto", "", "EDIT", "--dangerously-skip-permissions"] {
            assert_eq!(safe_agent_preset(bad), "edit", "{bad:?}");
        }
        assert_eq!(Policy::default().agent_approval, "edit");
        let root = test_dir("preset");
        let (env, base, _) = test_env(&root);
        let p = Policy { agent_approval: "autonomous".into(), ..base };
        assert_eq!(sanitize(p, &env).unwrap().agent_approval, "edit");
    }

    #[test]
    fn policy_round_trips_and_a_corrupt_file_falls_back_to_defaults() {
        let d = test_dir("policy_io");
        let mut p = Policy::default();
        p.mode = Mode::Ask;
        p.extra_roots.push("D:\\projects".into());
        save(&d, &p).unwrap();
        let back = load(&d);
        assert_eq!(back.mode, Mode::Ask);
        assert_eq!(back.extra_roots, p.extra_roots);
        std::fs::write(d.join(POLICY_FILE), "{ not json").unwrap();
        assert_eq!(load(&d).mode, Mode::Smart);
        std::fs::write(d.join(POLICY_FILE), r#"{"mode":"ask"}"#).unwrap();
        assert_eq!(load(&d).max_items, 50, "missing fields take defaults");
    }
}
