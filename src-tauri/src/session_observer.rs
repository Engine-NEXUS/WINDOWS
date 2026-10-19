//! Watching a coding session from the outside (doc 142, phase W3): turns what can be seen into the
//! `Evidence` that `session_verdict::judge` decides on.
//!
//! What is seen, and how much each is worth:
//!   * the agent's process tree: idle (almost no CPU, no new child process, no disk traffic) for a long
//!     time. WEAK on its own: an agent waiting for its model's reply is idle too, which is exactly why a
//!     second signal is always required and a "busy" mark on the window outranks it;
//!   * the window's text: no spinner / "esc to interrupt", and unchanged for a while;
//!   * for a session NEXUS started itself, the sidecar's own task-complete plus an output that stopped moving.
//!
//! Everything here is pure except `ProcSampler` (a thin sysinfo wrapper). Nothing here powers anything
//! off, and nothing reads a window yet: a window-text source is supplied by the caller, and without one the
//! window signal is simply missing, so the verdict can never reach "go" on process idleness alone.

use crate::session_verdict::{prepare_tail, scan_tail, Evidence, ModelVerdict, Signal};
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

/// Average CPU of the whole tree above this means it is working.
pub const CPU_ACTIVE_PCT: f32 = 3.0;
/// More disk traffic than this between two samples means it is working.
pub const IO_ACTIVE_BYTES: u64 = 64 * 1024;
/// How long the process tree must be quiet to count as idle (an agent can wait for its model that long
/// without CPU, which is why this is only ever one of two signals).
pub const IDLE_NEEDED_MS: u64 = 45_000;
/// How long the window text must stay unchanged, and how long a sidecar output must stop moving.
pub const QUIET_NEEDED_MS: u64 = 20_000;
/// Fewer samples than this never count as idle (CPU needs two samples to mean anything).
pub const MIN_SAMPLES: u32 = 3;
/// How long the agent's Stop must stay its latest event before that quiet counts as a second sign.
pub const HOOK_QUIET_NEEDED_MS: u64 = 45_000;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcSnap {
    pub pid: u32,
    pub ppid: u32,
    /// lowercase process name
    pub name: String,
    /// lowercase command line
    pub cmd: String,
    pub cpu_pct: f32,
    /// total bytes read + written since the process started
    pub io_total: u64,
}

// ─── Finding the agent ───────────────────────────────────────────────

/// Words that identify an agent's processes (matched in the name or the command line).
fn agent_words(agent: &str) -> &'static [&'static str] {
    match agent {
        "claude" => &["claude"],
        "codex" => &["codex"],
        "antigravity" => &["antigravity"],
        "cursor" => &["cursor"],
        "trae" => &["trae"],
        "windsurf" => &["windsurf"],
        "aider" => &["aider"],
        "opencode" => &["opencode"],
        "hermes" => &["hermes"],
        "gemini" => &["gemini"],
        "vscode" => &["code.exe", "visual studio code", "/code "],
        _ => &[],
    }
}

/// The processes that ARE the agent (never NEXUS itself): the roots of the matching processes.
/// With no agent named, the foreground window's process is the root (a terminal or an IDE).
pub fn find_roots(agent: Option<&str>, all: &[ProcSnap], foreground_pid: Option<u32>, own_pid: u32) -> Vec<u32> {
    let ours = descendants(&[own_pid], all);
    let Some(agent) = agent else {
        return foreground_pid.filter(|p| !ours.contains(p)).into_iter().collect();
    };
    let words = agent_words(agent);
    if words.is_empty() {
        return Vec::new();
    }
    let matching: BTreeSet<u32> = all
        .iter()
        .filter(|p| !ours.contains(&p.pid) && words.iter().any(|w| p.name.contains(w) || p.cmd.contains(w)))
        .map(|p| p.pid)
        .collect();
    // a root = a match whose parent is not also a match
    all.iter()
        .filter(|p| matching.contains(&p.pid) && !matching.contains(&p.ppid))
        .map(|p| p.pid)
        .collect()
}

/// Every process that descends from `roots` (the roots included).
pub fn descendants(roots: &[u32], all: &[ProcSnap]) -> BTreeSet<u32> {
    let mut seen: BTreeSet<u32> = roots.iter().copied().collect();
    let mut changed = true;
    while changed {
        changed = false;
        for p in all {
            if !seen.contains(&p.pid) && seen.contains(&p.ppid) {
                seen.insert(p.pid);
                changed = true;
            }
        }
    }
    seen
}

pub fn tree_snaps(roots: &[u32], all: &[ProcSnap]) -> Vec<ProcSnap> {
    let set = descendants(roots, all);
    all.iter().filter(|p| set.contains(&p.pid)).cloned().collect()
}

// ─── Is the process tree idle? ───────────────────────────────────────

#[derive(Debug, Default)]
pub struct IdleTracker {
    prev_pids: Option<BTreeSet<u32>>,
    prev_io: Option<u64>,
    quiet_since: Option<u64>,
    samples: u32,
    active_now: bool,
    ever_seen: bool,
    gone: bool,
}

impl IdleTracker {
    /// Feed one sample of the agent's whole tree (empty = the tree is gone).
    pub fn feed(&mut self, now_ms: u64, tree: &[ProcSnap]) {
        self.samples += 1;
        if tree.is_empty() {
            self.gone = self.ever_seen;
            self.active_now = false;
            self.quiet_since.get_or_insert(now_ms);
            return;
        }
        self.ever_seen = true;
        self.gone = false;
        let cpu: f32 = tree.iter().map(|p| p.cpu_pct).sum();
        let pids: BTreeSet<u32> = tree.iter().map(|p| p.pid).collect();
        let io: u64 = tree.iter().map(|p| p.io_total).sum();
        let new_child = self.prev_pids.as_ref().map_or(false, |prev| pids.difference(prev).next().is_some());
        let io_delta = self.prev_io.map_or(0, |prev| io.saturating_sub(prev));
        self.active_now = cpu > CPU_ACTIVE_PCT || new_child || io_delta > IO_ACTIVE_BYTES;
        self.prev_pids = Some(pids);
        self.prev_io = Some(io);
        if self.active_now {
            self.quiet_since = None;
        } else {
            self.quiet_since.get_or_insert(now_ms);
        }
    }

    /// The latest sample showed work.
    pub fn active_now(&self) -> bool {
        self.active_now
    }

    /// The tree existed and has now disappeared (the agent exited).
    pub fn gone(&self) -> bool {
        self.gone
    }

    pub fn is_idle(&self, now_ms: u64) -> bool {
        self.samples >= MIN_SAMPLES
            && self.ever_seen
            && !self.active_now
            && self.quiet_since.map_or(false, |q| now_ms.saturating_sub(q) >= IDLE_NEEDED_MS)
    }
}

// ─── Reading a window's text ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowRead {
    /// A spinner, "esc to interrupt" or a "working…" line: it is NOT over.
    pub busy: bool,
    /// A permission prompt / y-n question / choice waits for the user.
    pub waiting: bool,
    pub login: bool,
}

const BUSY_MARKERS: &[&str] = &[
    "esc to interrupt", "ctrl+c to interrupt", "ctrl-c to interrupt", "press esc to interrupt", "esc to stop", "thinking…", "thinking...",
    "working…", "working...", "generating…", "generating...", "running…", "running...", "executing…", "executing...",
];
const LOGIN_MARKERS: &[&str] = &[
    "please run /login", "run /login", "not logged in", "login required", "please log in", "please sign in", "sign in to continue",
    "invalid api key", "authentication failed", "authentication_error", "your session has expired", "token has expired",
];

/// Braille spinner frames and the star spinners the CLIs animate.
fn has_spinner_glyph(text: &str) -> bool {
    text.chars().any(|c| ('\u{2800}'..='\u{28FF}').contains(&c) || matches!(c, '✻' | '✶' | '✳' | '✢' | '✽' | '◐' | '◓' | '◑' | '◒'))
}

pub fn classify_window_text(text: &str) -> WindowRead {
    let lower = text.to_lowercase();
    let scan = scan_tail(text);
    WindowRead {
        busy: BUSY_MARKERS.iter().any(|m| lower.contains(m)) || has_spinner_glyph(text),
        waiting: scan.question.is_some(),
        login: LOGIN_MARKERS.iter().any(|m| lower.contains(m)),
    }
}

/// How long the text has stayed the same.
#[derive(Debug, Default)]
pub struct TextStability {
    hash: Option<u64>,
    since_ms: u64,
}

impl TextStability {
    pub fn feed(&mut self, now_ms: u64, text: &str) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.trim().hash(&mut h);
        let h = h.finish();
        if self.hash != Some(h) {
            self.hash = Some(h);
            self.since_ms = now_ms;
        }
    }

    pub fn stable_for(&self, now_ms: u64) -> u64 {
        if self.hash.is_some() {
            now_ms.saturating_sub(self.since_ms)
        } else {
            0
        }
    }
}

// ─── Putting it together ─────────────────────────────────────────────

pub struct Inputs<'a> {
    pub now_ms: u64,
    /// The agent's whole process tree this sample (empty when it is gone or could not be found).
    pub tree: &'a [ProcSnap],
    /// The text of the watched window, when a reader exists and could read it.
    pub window_text: Option<&'a str>,
    /// The agent's own Stop hook fired (and no new prompt since).
    pub hook_stopped: bool,
    /// The agent's final message from that Stop hook, when it sent one (data for the verdict).
    pub hook_tail: Option<&'a str>,
    /// A permission prompt is waiting for the user (from the agent's Notification hook).
    pub hook_needs_user: bool,
    /// The agent's Stop event says it stopped on an error or an abort (Antigravity, Cursor).
    pub hook_failed: bool,
    /// The agent's Stop event says background work is still running (Antigravity's `fullyIdle: false`).
    pub hook_not_idle: bool,
    /// How long the agent's Stop has been its latest event (no start, notice or message since); `None` = not so.
    pub hook_quiet_ms: Option<u64>,
    /// An IDE agent (Antigravity, Cursor): its process tree is never quiet, so CPU, child processes and disk traffic
    /// say nothing about whether it is working. Its hooks and its own stop report decide instead.
    pub ignore_process_activity: bool,
}

#[derive(Debug, Default)]
pub struct Observer {
    idle: IdleTracker,
    stability: TextStability,
}

impl Observer {
    pub fn step(&mut self, i: Inputs) -> Evidence {
        self.idle.feed(i.now_ms, i.tree);
        let mut signals = Vec::new();
        if i.hook_stopped {
            signals.push(Signal::AgentHook);
        }
        if i.hook_stopped && !i.hook_not_idle && i.hook_quiet_ms.map_or(false, |q| q >= HOOK_QUIET_NEEDED_MS) {
            signals.push(Signal::HookQuiet);
        }
        if self.idle.is_idle(i.now_ms) || self.idle.gone() {
            signals.push(Signal::ProcessIdle);
        }
        let mut read = WindowRead::default();
        let mut tail = i.hook_tail.map(prepare_tail).unwrap_or_default();
        if let Some(text) = i.window_text {
            self.stability.feed(i.now_ms, text);
            read = classify_window_text(text);
            tail = prepare_tail(text);
            if !read.busy && self.stability.stable_for(i.now_ms) >= QUIET_NEEDED_MS {
                signals.push(Signal::WindowQuiet);
            }
        }
        Evidence {
            signals,
            tail,
            exit_ok: if i.hook_failed { Some(false) } else { None },
            waiting_for_user: read.waiting || i.hook_needs_user,
            login_required: read.login,
            still_busy: read.busy || (!i.ignore_process_activity && self.idle.active_now()) || i.hook_not_idle,
            model: ModelVerdict::Unavailable,
        }
    }
}

/// Evidence from a session NEXUS started itself through the sidecar (`GET /watch/{id}` JSON).
/// `output_quiet_ms` = how long its `output` has stayed the same, measured by the caller between polls.
/// (Reserved for "ask claude to X and shut down when it's perfect"; compiled for tests only until that is wired.)
#[cfg(test)]
pub fn evidence_from_watch_json(json: &serde_json::Value, output_quiet_ms: u64) -> Evidence {
    let status = json.get("status").and_then(|v| v.as_str()).unwrap_or("working");
    let output = json.get("output").and_then(|v| v.as_str()).unwrap_or("");
    let mut signals = Vec::new();
    if status == "done" {
        signals.push(Signal::AdapterTaskComplete);
    }
    if output_quiet_ms >= QUIET_NEEDED_MS {
        signals.push(Signal::OutputQuiet);
    }
    Evidence {
        signals,
        tail: prepare_tail(output),
        exit_ok: if status == "error" { Some(false) } else { None },
        waiting_for_user: status == "blocked",
        login_required: status == "login_required",
        still_busy: matches!(status, "starting" | "working"),
        model: ModelVerdict::Unavailable,
    }
}

// ─── The only impure part: sampling real processes ───────────────────

pub struct ProcSampler {
    sys: sysinfo::System,
}

impl ProcSampler {
    pub fn new() -> ProcSampler {
        ProcSampler { sys: sysinfo::System::new() }
    }

    /// Every process now. CPU is only meaningful from the second call on.
    pub fn sample(&mut self) -> Vec<ProcSnap> {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::everything());
        let mut out: HashMap<u32, ProcSnap> = HashMap::new();
        for (pid, p) in self.sys.processes() {
            let du = p.disk_usage();
            out.insert(
                pid.as_u32(),
                ProcSnap {
                    pid: pid.as_u32(),
                    ppid: p.parent().map(|x| x.as_u32()).unwrap_or(0),
                    name: p.name().to_string_lossy().to_lowercase(),
                    cmd: p.cmd().iter().map(|c| c.to_string_lossy().to_lowercase()).collect::<Vec<_>>().join(" "),
                    cpu_pct: p.cpu_usage(),
                    io_total: du.total_read_bytes.saturating_add(du.total_written_bytes),
                },
            );
        }
        let mut v: Vec<ProcSnap> = out.into_values().collect();
        v.sort_by_key(|p| p.pid);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_verdict::{judge, Reason, Verdict};

    fn p(pid: u32, ppid: u32, name: &str, cmd: &str, cpu: f32, io: u64) -> ProcSnap {
        ProcSnap { pid, ppid, name: name.into(), cmd: cmd.into(), cpu_pct: cpu, io_total: io }
    }

    fn world() -> Vec<ProcSnap> {
        vec![
            p(1, 0, "system", "", 0.0, 0),
            p(10, 1, "windowsterminal.exe", "windowsterminal.exe", 0.2, 10),
            p(11, 10, "pwsh.exe", "pwsh.exe", 0.0, 5),
            p(12, 11, "node.exe", "node c:/tools/claude/cli.js --resume", 1.0, 100),
            p(13, 12, "node.exe", "node test-runner.js", 0.0, 50),
            p(20, 1, "cursor.exe", "c:/cursor/cursor.exe", 0.5, 20),
            p(21, 20, "cursor.exe", "c:/cursor/cursor.exe --type=renderer", 0.1, 20),
            p(30, 1, "nexus.exe", "nexus.exe", 0.3, 5),
            p(31, 30, "node.exe", "node agent_watch_sidecar/server.cjs claude", 0.0, 5),
        ]
    }

    #[test]
    fn the_agents_processes_are_found_and_nexus_itself_is_never_one() {
        let all = world();
        assert_eq!(find_roots(Some("claude"), &all, None, 30), vec![12]);
        assert_eq!(find_roots(Some("cursor"), &all, None, 30), vec![20]);
        assert!(find_roots(Some("codex"), &all, None, 30).is_empty());
        assert!(find_roots(Some("unknown"), &all, None, 30).is_empty());
        // the sidecar's own command line mentions claude: it is under NEXUS and must not count
        assert!(!find_roots(Some("claude"), &all, None, 30).contains(&31));
        // no agent named: the window in front, unless that is NEXUS
        assert_eq!(find_roots(None, &all, Some(10), 30), vec![10]);
        assert!(find_roots(None, &all, Some(31), 30).is_empty());
        assert!(find_roots(None, &all, None, 30).is_empty());
    }

    #[test]
    fn a_tree_includes_every_descendant() {
        let all = world();
        let t: Vec<u32> = tree_snaps(&[10], &all).iter().map(|x| x.pid).collect();
        assert_eq!(t, vec![10, 11, 12, 13]);
        let t: Vec<u32> = tree_snaps(&[12], &all).iter().map(|x| x.pid).collect();
        assert_eq!(t, vec![12, 13]);
    }

    fn quiet_tree() -> Vec<ProcSnap> {
        vec![p(12, 11, "node.exe", "node claude", 0.5, 1000), p(13, 12, "node.exe", "node child", 0.0, 500)]
    }

    #[test]
    fn idle_needs_enough_samples_and_a_long_quiet_and_any_work_resets_it() {
        let mut t = IdleTracker::default();
        let quiet = quiet_tree();
        t.feed(0, &quiet);
        t.feed(5_000, &quiet);
        assert!(!t.is_idle(5_000), "too few samples and too short");
        t.feed(10_000, &quiet);
        assert!(!t.is_idle(10_000), "quiet for only 10 s");
        t.feed(45_000, &quiet);
        assert!(t.is_idle(45_000));
        // CPU burst
        let mut busy = quiet_tree();
        busy[0].cpu_pct = 40.0;
        t.feed(50_000, &busy);
        assert!(!t.is_idle(50_000) && t.active_now());
        t.feed(60_000, &quiet);
        assert!(!t.is_idle(60_000), "the quiet clock restarted");
        t.feed(105_000, &quiet);
        assert!(t.is_idle(105_000));
        // a new child process
        let mut kids = quiet_tree();
        kids.push(p(99, 12, "cargo.exe", "cargo test", 0.0, 0));
        t.feed(110_000, &kids);
        assert!(t.active_now() && !t.is_idle(110_000));
        // disk traffic
        let mut t2 = IdleTracker::default();
        t2.feed(0, &quiet);
        let mut writes = quiet_tree();
        writes[0].io_total += IO_ACTIVE_BYTES + 1;
        t2.feed(1_000, &writes);
        assert!(t2.active_now());
    }

    #[test]
    fn a_vanished_tree_counts_only_after_it_was_seen() {
        let mut t = IdleTracker::default();
        t.feed(0, &[]);
        assert!(!t.gone(), "never seen: could be the wrong match");
        t.feed(1_000, &quiet_tree());
        t.feed(2_000, &[]);
        assert!(t.gone());
    }

    #[test]
    fn window_text_is_read_for_busy_waiting_and_login() {
        assert!(classify_window_text("✻ Thinking… (12s · esc to interrupt)").busy);
        assert!(classify_window_text("⠋ Installing packages").busy);
        assert!(classify_window_text("Running... please wait").busy);
        assert!(!classify_window_text("All done.\n> ").busy);
        assert!(classify_window_text("Do you want to proceed? (y/n)").waiting);
        assert!(!classify_window_text("Implemented the change.").waiting);
        assert!(classify_window_text("Please run /login to continue").login);
        assert!(classify_window_text("Invalid API key").login);
        assert!(!classify_window_text("logged in as admin").login);
    }

    #[test]
    fn text_stability_resets_on_any_change() {
        let mut s = TextStability::default();
        assert_eq!(s.stable_for(100), 0);
        s.feed(0, "a");
        s.feed(10_000, "a ");
        assert_eq!(s.stable_for(10_000), 10_000, "trailing blanks are not a change");
        s.feed(15_000, "b");
        assert_eq!(s.stable_for(16_000), 1_000);
    }

    fn run(obs: &mut Observer, now: u64, tree: &[ProcSnap], text: Option<&str>, hook: bool) -> Evidence {
        obs.step(Inputs {
            now_ms: now,
            tree,
            window_text: text,
            hook_stopped: hook,
            hook_tail: None,
            hook_needs_user: false,
            hook_failed: false,
            hook_not_idle: false,
            hook_quiet_ms: None,
            ignore_process_activity: false,
        })
    }

    /// A hook that says the agent stopped on an error / abort, or that background work is still running (Antigravity
    /// `fullyIdle:false`), turns an otherwise clean finish into "not a go".
    #[test]
    fn a_stop_that_reports_an_error_or_background_work_is_never_a_go() {
        let tree = quiet_tree();
        let done = "Implemented it.\nAll 12 tests passed.\nDone.";
        for (failed, not_idle, want_go) in [(false, false, true), (true, false, false), (false, true, false), (true, true, false)] {
            let mut o = Observer::default();
            let mut ev = None;
            for s in 0..=8 {
                ev = Some(o.step(Inputs {
                    now_ms: s * 10_000,
                    tree: &tree,
                    window_text: None,
                    hook_stopped: true,
                    hook_tail: Some(done),
                    hook_needs_user: false,
                    hook_failed: failed,
                    hook_not_idle: not_idle,
                    hook_quiet_ms: None,
                    ignore_process_activity: false,
                }));
            }
            let ev = ev.unwrap();
            assert_eq!(judge(&ev, true) == Verdict::Go, want_go, "failed={failed} not_idle={not_idle}: {:?}", judge(&ev, true));
            assert_eq!(ev.exit_ok, if failed { Some(false) } else { None });
            assert_eq!(ev.still_busy, not_idle);
        }
    }

    /// An IDE agent (Electron: renderers, language server, file watchers) never has a quiet process tree. Its own Stop,
    /// still the latest event 45 s later, is the second sign; the busy tree must not veto it.
    #[test]
    fn an_ide_agent_ends_on_its_stop_and_a_busy_process_tree_cannot_veto_it() {
        let busy_tree = vec![p(12, 11, "antigravity.exe", "antigravity", 35.0, 5_000), p(13, 12, "antigravity.exe", "antigravity --type=renderer", 20.0, 9_000)];
        let done = "Implemented it.\nAll 12 tests passed.\nDone.";
        let feed = |ignore: bool, quiet: Option<u64>, not_idle: bool, hook: bool| {
            let mut o = Observer::default();
            let mut ev = None;
            for s in 0..=8u64 {
                let mut tree = busy_tree.clone();
                tree[0].io_total += s * 200_000; // always writing
                ev = Some(o.step(Inputs {
                    now_ms: s * 10_000,
                    tree: &tree,
                    window_text: None,
                    hook_stopped: hook,
                    hook_tail: Some(done),
                    hook_needs_user: false,
                    hook_failed: false,
                    hook_not_idle: not_idle,
                    hook_quiet_ms: quiet,
                    ignore_process_activity: ignore,
                }));
            }
            ev.unwrap()
        };
        // IDE: stop + 45 s of quiet, busy tree ignored -> ended and a go
        let ev = feed(true, Some(HOOK_QUIET_NEEDED_MS), false, true);
        assert!(ev.signals.contains(&Signal::AgentHook) && ev.signals.contains(&Signal::HookQuiet), "{:?}", ev.signals);
        assert!(!ev.still_busy);
        assert_eq!(judge(&ev, true), Verdict::Go);
        // terminal agent: the same busy tree DOES veto
        let ev = feed(false, Some(HOOK_QUIET_NEEDED_MS), false, true);
        assert!(ev.still_busy);
        assert_eq!(judge(&ev, true), Verdict::Unsure(vec![Reason::StillBusy]));
        // not quiet long enough -> only one sign
        let ev = feed(true, Some(HOOK_QUIET_NEEDED_MS - 1), false, true);
        assert!(!ev.signals.contains(&Signal::HookQuiet));
        assert_eq!(judge(&ev, true), Verdict::Unsure(vec![Reason::NotEnoughSignals]));
        // the agent says background work is still running -> busy, and no quiet credit
        let ev = feed(true, Some(HOOK_QUIET_NEEDED_MS), true, true);
        assert!(ev.still_busy && !ev.signals.contains(&Signal::HookQuiet));
        // no stop at all -> the quiet counts for nothing
        let ev = feed(true, Some(HOOK_QUIET_NEEDED_MS), false, false);
        assert!(!ev.signals.contains(&Signal::HookQuiet) && !ev.signals.contains(&Signal::AgentHook));
        assert!(!matches!(judge(&ev, true), Verdict::Go));
    }

    #[test]
    fn process_idleness_alone_never_reaches_a_go() {
        let mut o = Observer::default();
        let tree = quiet_tree();
        let mut ev = run(&mut o, 0, &tree, None, false);
        for s in 1..=10 {
            ev = run(&mut o, s * 10_000, &tree, None, false);
        }
        assert!(ev.signals.contains(&Signal::ProcessIdle));
        assert_eq!(ev.signals.len(), 1);
        assert_eq!(judge(&ev, true), Verdict::Unsure(vec![Reason::NotEnoughSignals, Reason::NothingToJudge]));
    }

    #[test]
    fn a_quiet_process_plus_a_quiet_window_is_a_go_but_a_spinner_overrules_both() {
        let mut o = Observer::default();
        let tree = quiet_tree();
        let done = "Implemented it.\nAll 12 tests passed.\nDone.";
        let mut ev = run(&mut o, 0, &tree, Some(done), false);
        for s in 1..=6 {
            ev = run(&mut o, s * 10_000, &tree, Some(done), false);
        }
        assert!(ev.signals.contains(&Signal::ProcessIdle) && ev.signals.contains(&Signal::WindowQuiet), "{:?}", ev.signals);
        assert_eq!(judge(&ev, true), Verdict::Go);
        // the agent starts working again: the spinner alone blocks it
        let ev = run(&mut o, 70_000, &tree, Some("✻ Working… (3s · esc to interrupt)"), false);
        assert!(ev.still_busy);
        assert_eq!(judge(&ev, true), Verdict::Unsure(vec![Reason::StillBusy, Reason::NotEnoughSignals]));
    }

    #[test]
    fn the_agents_own_stop_hook_counts_as_a_signal_and_a_failure_in_the_window_still_holds() {
        let mut o = Observer::default();
        let tree = quiet_tree();
        let text = "Ran the build\nerror[E0432]: unresolved import\nStopped.";
        let ev = run(&mut o, 0, &tree, Some(text), true);
        assert!(ev.signals.contains(&Signal::AgentHook));
        // hook + (not yet quiet window / idle) is one signal
        assert_eq!(judge(&ev, true), Verdict::Hold(vec![Reason::FailureMarker("error[e0432]".into(), "error[E0432]: unresolved import".into()), Reason::NotEnoughSignals]));
    }

    #[test]
    fn a_login_wall_or_a_question_in_the_window_is_carried_into_the_evidence() {
        let mut o = Observer::default();
        let ev = run(&mut o, 0, &quiet_tree(), Some("Please run /login"), false);
        assert!(ev.login_required);
        let ev = run(&mut o, 1_000, &quiet_tree(), Some("Overwrite the file? (y/n)"), false);
        assert!(ev.waiting_for_user);
    }

    #[test]
    fn sidecar_sessions_are_judged_from_their_status_and_output() {
        use serde_json::json;
        let done = json!({"status": "done", "output": "Implemented.\nAll tests passed.\nDone."});
        let ev = evidence_from_watch_json(&done, QUIET_NEEDED_MS);
        assert_eq!(ev.signals, vec![Signal::AdapterTaskComplete, Signal::OutputQuiet]);
        assert_eq!(judge(&ev, true), Verdict::Go);
        // done but the output is still moving: one signal only
        let ev = evidence_from_watch_json(&done, 1_000);
        assert_eq!(judge(&ev, true), Verdict::Unsure(vec![Reason::NotEnoughSignals]));
        for (status, check) in [("working", 0), ("starting", 0), ("blocked", 1), ("login_required", 2), ("error", 3)] {
            let ev = evidence_from_watch_json(&json!({"status": status, "output": "x"}), 60_000);
            match check {
                0 => assert!(ev.still_busy, "{status}"),
                1 => assert!(ev.waiting_for_user, "{status}"),
                2 => assert!(ev.login_required, "{status}"),
                _ => assert_eq!(ev.exit_ok, Some(false), "{status}"),
            }
            assert!(!matches!(judge(&ev, true), Verdict::Go), "{status}");
        }
        // missing fields never produce a go
        assert!(!matches!(judge(&evidence_from_watch_json(&json!({}), 0), true), Verdict::Go));
    }

    #[test]
    fn one_or_two_samples_are_never_idle_however_long_the_gap() {
        let mut t = IdleTracker::default();
        t.feed(0, &quiet_tree());
        t.feed(50_000, &quiet_tree());
        assert!(!t.is_idle(50_000), "two samples are not enough");
        t.feed(60_000, &quiet_tree());
        assert!(t.is_idle(60_000));
    }

    #[test]
    fn a_window_that_still_shows_a_spinner_is_never_quiet_however_long_it_stays_the_same() {
        let mut o = Observer::default();
        let tree = quiet_tree();
        let busy = "Reading files\n\u{2733} Working... (esc to interrupt)";
        let mut ev = run(&mut o, 0, &tree, Some(busy), false);
        for s in 1..=8 {
            ev = run(&mut o, s * 10_000, &tree, Some(busy), false);
        }
        assert!(ev.still_busy);
        assert!(!ev.signals.contains(&Signal::WindowQuiet), "{:?}", ev.signals);
    }

    #[test]
    fn work_in_the_process_tree_marks_it_busy_even_when_no_window_is_read() {
        let mut o = Observer::default();
        let mut tree = quiet_tree();
        tree[0].cpu_pct = 60.0;
        let ev = run(&mut o, 0, &tree, None, false);
        assert!(ev.still_busy);
        let calm = quiet_tree();
        let ev = run(&mut o, 2_000, &calm, None, false);
        assert!(!ev.still_busy);
    }

    #[test]
    fn a_session_that_is_still_working_is_not_task_complete_just_because_its_output_stopped() {
        use serde_json::json;
        for status in ["working", "starting", "blocked", "login_required", "error", "stopped"] {
            let ev = evidence_from_watch_json(&json!({"status": status, "output": "x"}), 60_000);
            assert_eq!(ev.signals, vec![Signal::OutputQuiet], "{status}");
        }
    }

    #[test]
    fn the_real_sampler_sees_this_very_process_and_its_parent_chain() {
        let mut s = ProcSampler::new();
        let _ = s.sample();
        std::thread::sleep(std::time::Duration::from_millis(250));
        let all = s.sample();
        let me = std::process::id();
        let mine = all.iter().find(|p| p.pid == me).expect("own process listed");
        assert!(!mine.name.is_empty());
        assert!(all.iter().any(|p| p.pid == mine.ppid), "parent listed");
        assert!(descendants(&[mine.ppid], &all).contains(&me));
        // NEXUS's own tree is excluded from any agent search
        assert!(!find_roots(Some("claude"), &all, None, me).iter().any(|r| descendants(&[me], &all).contains(r)));
    }
}
