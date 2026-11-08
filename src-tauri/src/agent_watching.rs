//! Which running coding agents the user is already looking at, so the Agent Island can leave them out.
//!
//! An agent counts as WATCHED when the window that hosts it is on screen and mostly visible: it is the foreground
//! window, or at least half of it is not covered by the windows above it (so a split screen with two agents side by
//! side watches both, and an agent behind a maximised editor does not). A minimised window, a window on another
//! virtual desktop and an agent whose window cannot be found are NOT watched, so they still show.
//!
//! Split like `window_tile.rs`: every decision here is pure and tested; only `snapshot_windows` touches Win32.

use crate::session_observer::ProcSnap;
use crate::window_tile::{window_for_pid, Rect, TopWin};
use std::collections::{BTreeMap, BTreeSet};

/// At least this share of a window must be visible for its agent to count as watched.
pub(crate) const MIN_VISIBLE: f64 = 0.5;
/// A newly watched agent leaves the island after this long (so passing through a window does not flash the bar).
pub(crate) const WATCH_ON_MS: u64 = 500;
/// A no-longer-watched agent appears on the island after this long (alt-tab and back must not flash it).
pub(crate) const WATCH_OFF_MS: u64 = 1_500;

/// A top-level window with where it is, topmost first in the list that carries it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    pub win: TopWin,
    pub rect: Rect,
    pub minimized: bool,
}

fn area(r: &Rect) -> i64 {
    (r.width().max(0) as i64) * (r.height().max(0) as i64)
}

fn intersect(a: &Rect, b: &Rect) -> Option<Rect> {
    let r = Rect { left: a.left.max(b.left), top: a.top.max(b.top), right: a.right.min(b.right), bottom: a.bottom.min(b.bottom) };
    (r.width() > 0 && r.height() > 0).then_some(r)
}

/// Area of the union of `rects` (every rect is clipped to nothing: callers pass already-clipped pieces).
fn union_area(rects: &[Rect]) -> i64 {
    if rects.is_empty() {
        return 0;
    }
    let mut xs: Vec<i32> = rects.iter().flat_map(|r| [r.left, r.right]).collect();
    xs.sort_unstable();
    xs.dedup();
    let mut total = 0i64;
    for pair in xs.windows(2) {
        let (x0, x1) = (pair[0], pair[1]);
        let mut spans: Vec<(i32, i32)> =
            rects.iter().filter(|r| r.left <= x0 && r.right >= x1).map(|r| (r.top, r.bottom)).collect();
        spans.sort_unstable();
        let mut covered = 0i64;
        let mut cur: Option<(i32, i32)> = None;
        for (t, b) in spans {
            match cur {
                Some((ct, cb)) if t <= cb => cur = Some((ct, cb.max(b))),
                Some((ct, cb)) => {
                    covered += (cb - ct) as i64;
                    cur = Some((t, b));
                }
                None => cur = Some((t, b)),
            }
        }
        if let Some((ct, cb)) = cur {
            covered += (cb - ct) as i64;
        }
        total += covered * (x1 - x0) as i64;
    }
    total
}

/// The share (0..=1) of `target` that is on a screen and not covered by `above`. Off-screen parts count as hidden.
pub(crate) fn visible_fraction(target: &Rect, above: &[Rect], screens: &[Rect]) -> f64 {
    let full = area(target);
    if full == 0 {
        return 0.0;
    }
    let mut visible = 0i64;
    for s in screens {
        let Some(piece) = intersect(target, s) else { continue };
        let covers: Vec<Rect> = above.iter().filter_map(|a| intersect(&piece, a)).collect();
        visible += area(&piece) - union_area(&covers);
    }
    visible as f64 / full as f64
}

/// Is the window at `idx` (z-order, topmost first) watched?
pub(crate) fn window_watched(idx: usize, placed: &[Placed], screens: &[Rect], foreground: Option<isize>) -> bool {
    let Some(w) = placed.get(idx) else { return false };
    if w.minimized {
        return false;
    }
    if foreground == Some(w.win.hwnd) {
        return true;
    }
    let above: Vec<Rect> = placed[..idx].iter().filter(|p| !p.minimized).map(|p| p.rect).collect();
    visible_fraction(&w.rect, &above, screens) >= MIN_VISIBLE
}

/// One running agent session as the island knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionRef {
    pub key: String,
    pub agent: String,
    pub folder: Option<String>,
}

/// Does this process BE the agent? Its program name or its program path must carry the agent's word. A command line
/// that merely mentions the word (a `grep codex`, a shell snapshot under `.claude`) is not the agent: matching on the
/// whole command line sent every such process to whatever window hosted it and hid agents that were not in view.
/// A script host (`node cli.js`) counts by the script path.
pub(crate) fn is_agent_process(p: &ProcSnap, words: &[&str]) -> bool {
    let first = p.cmd.trim_start().trim_start_matches('"');
    let first = first.split(['"', ' ']).next().unwrap_or("");
    let program = first.rsplit([std::path::MAIN_SEPARATOR, '/']).next().unwrap_or("");
    let direct = |s: &str| words.iter().any(|w| s.contains(w));
    if direct(&p.name) || direct(program) {
        return true;
    }
    const HOSTS: [&str; 7] = ["node.exe", "node", "python.exe", "python", "bun.exe", "deno.exe", "pythonw.exe"];
    if HOSTS.contains(&p.name.as_str()) {
        // the first argument that is a script or package path
        let cmd = p.cmd.trim_start();
        let rest = if let Some(q) = cmd.strip_prefix('"') {
            q.split_once('"').map_or("", |(_, r)| r)
        } else if let Some(i) = cmd.find(".exe") {
            &cmd[i + 4..]
        } else {
            cmd.split_once(' ').map_or("", |(_, r)| r)
        };
        let script = rest.split_whitespace().find(|a| !a.starts_with('-')).unwrap_or("").trim_matches('"');
        return direct(script);
    }
    false
}

/// The roots of the processes that are the agent (a match whose parent is not also a match), never NEXUS itself.
pub(crate) fn agent_roots(agent: &str, procs: &[ProcSnap], own_pid: u32) -> Vec<u32> {
    let words = crate::session_observer::agent_words(agent);
    if words.is_empty() {
        return vec![];
    }
    let ours = crate::session_observer::descendants(&[own_pid], procs);
    let matching: BTreeSet<u32> = procs.iter().filter(|p| !ours.contains(&p.pid) && is_agent_process(p, words)).map(|p| p.pid).collect();
    procs.iter().filter(|p| matching.contains(&p.pid) && !matching.contains(&p.ppid)).map(|p| p.pid).collect()
}

/// The window that hosts a session: through the agent's processes (a terminal agent), else by the name of an IDE.
/// The session's folder picks between several windows of one process (Windows Terminal hosts them all).
pub(crate) fn session_window(s: &SessionRef, procs: &[ProcSnap], wins: &[TopWin], own_pid: u32) -> Option<isize> {
    let prefer: Vec<String> = s.folder.iter().cloned().collect();
    for root in agent_roots(&s.agent, procs, own_pid) {
        if let Some(h) = window_for_pid(root, procs, wins, &prefer) {
            return Some(h);
        }
    }
    let word = match s.agent.as_str() {
        "vscode" => "code",
        other => other,
    };
    wins.iter()
        .find(|w| matches!(s.agent.as_str(), "antigravity" | "cursor" | "windsurf" | "trae" | "vscode") && (w.exe.contains(word) || w.title.to_lowercase().contains(word)))
        .map(|w| w.hwnd)
}

/// The raw (not yet debounced) set of watched session keys.
pub(crate) fn watched_now(
    sessions: &[SessionRef],
    procs: &[ProcSnap],
    placed: &[Placed],
    screens: &[Rect],
    foreground: Option<isize>,
    own_pid: u32,
) -> BTreeSet<String> {
    let wins: Vec<TopWin> = placed.iter().map(|p| p.win.clone()).collect();
    sessions
        .iter()
        .filter(|s| {
            session_window(s, procs, &wins, own_pid)
                .and_then(|h| placed.iter().position(|p| p.win.hwnd == h))
                .is_some_and(|i| window_watched(i, placed, screens, foreground))
        })
        .map(|s| s.key.clone())
        .collect()
}

/// Debounce: a session changes between watched and not watched only after the new state held for a while. A session
/// seen for the first time takes its raw state at once (a freshly started agent appears without a delay).
#[derive(Debug, Default)]
pub(crate) struct Debounce {
    state: BTreeMap<String, (bool, Option<u64>)>,
}

impl Debounce {
    pub(crate) fn step(&mut self, now_ms: u64, raw: &BTreeSet<String>, keys: &[String]) -> BTreeSet<String> {
        self.state.retain(|k, _| keys.contains(k));
        for k in keys {
            let want = raw.contains(k);
            let e = self.state.entry(k.clone()).or_insert((want, None));
            if e.0 == want {
                e.1 = None;
                continue;
            }
            let since = *e.1.get_or_insert(now_ms);
            let need = if want { WATCH_ON_MS } else { WATCH_OFF_MS };
            if now_ms.saturating_sub(since) >= need {
                *e = (want, None);
            }
        }
        self.state.iter().filter(|(_, v)| v.0).map(|(k, _)| k.clone()).collect()
    }
}

/// Every window with its place, topmost first, plus the screens and the foreground window. Windows only.
#[cfg(target_os = "windows")]
pub(crate) fn snapshot_windows() -> (Vec<Placed>, Vec<Rect>, Option<isize>) {
    use windows::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO};
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowRect, IsIconic};

    let placed: Vec<Placed> = crate::window_tile::list_top_windows()
        .into_iter()
        .filter_map(|win| {
            let h = HWND(win.hwnd);
            let mut r = RECT::default();
            let mut cloaked = 0u32;
            // SAFETY: both out-pointers are valid for the call; a stale handle just fails and is skipped.
            let (ok, minimized) = unsafe {
                let ok = GetWindowRect(h, &mut r).as_bool();
                let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
                (ok, IsIconic(h).as_bool())
            };
            // a window on another virtual desktop is cloaked: it is not on screen
            (ok && cloaked == 0).then(|| Placed { win, rect: Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom }, minimized })
        })
        .collect();

    unsafe extern "system" fn each(mon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
        let out = &mut *(lparam.0 as *mut Vec<Rect>);
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let m = info.rcMonitor;
            out.push(Rect { left: m.left, top: m.top, right: m.right, bottom: m.bottom });
        }
        BOOL(1)
    }
    let mut screens: Vec<Rect> = vec![];
    // SAFETY: the callback only pushes into the Vec passed through lparam, which outlives the call.
    unsafe {
        let _ = EnumDisplayMonitors(HDC(0), std::ptr::null(), Some(each), LPARAM(&mut screens as *mut Vec<Rect> as isize));
    }
    // SAFETY: GetForegroundWindow takes no arguments.
    let fg = unsafe { GetForegroundWindow() }.0;
    (placed, screens, (fg != 0).then_some(fg))
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn snapshot_windows() -> (Vec<Placed>, Vec<Rect>, Option<isize>) {
    (vec![], vec![], None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };

    fn r(l: i32, t: i32, rt: i32, b: i32) -> Rect {
        Rect { left: l, top: t, right: rt, bottom: b }
    }

    fn win(hwnd: isize, pid: u32, exe: &str, title: &str) -> TopWin {
        TopWin { hwnd, pid, exe: exe.into(), class: "X".into(), title: title.into() }
    }

    fn placed(hwnd: isize, pid: u32, exe: &str, title: &str, rect: Rect) -> Placed {
        Placed { win: win(hwnd, pid, exe, title), rect, minimized: false }
    }

    fn proc(pid: u32, ppid: u32, name: &str, cmd: &str) -> ProcSnap {
        ProcSnap { pid, ppid, name: name.into(), cmd: cmd.into(), cpu_pct: 0.0, io_total: 0 }
    }

    #[test]
    fn union_area_counts_overlap_once() {
        assert_eq!(union_area(&[]), 0);
        assert_eq!(union_area(&[r(0, 0, 10, 10)]), 100);
        assert_eq!(union_area(&[r(0, 0, 10, 10), r(5, 5, 15, 15)]), 175, "two squares overlapping by 25");
        assert_eq!(union_area(&[r(0, 0, 10, 10), r(0, 0, 10, 10)]), 100, "identical");
        assert_eq!(union_area(&[r(0, 0, 10, 10), r(2, 2, 4, 4)]), 100, "one inside the other");
    }

    #[test]
    fn a_window_is_fully_visible_with_nothing_above_it() {
        assert_eq!(visible_fraction(&r(100, 100, 900, 700), &[], &[SCREEN]), 1.0);
    }

    #[test]
    fn a_maximised_window_above_hides_the_one_below() {
        let f = visible_fraction(&r(100, 100, 900, 700), &[r(0, 0, 1920, 1080)], &[SCREEN]);
        assert_eq!(f, 0.0);
    }

    #[test]
    fn a_split_screen_leaves_both_halves_fully_visible() {
        let left = r(0, 0, 960, 1080);
        let right = r(960, 0, 1920, 1080);
        assert_eq!(visible_fraction(&left, &[right], &[SCREEN]), 1.0);
        assert_eq!(visible_fraction(&right, &[left], &[SCREEN]), 1.0);
    }

    #[test]
    fn a_window_partly_off_screen_counts_only_its_visible_part() {
        let f = visible_fraction(&r(1520, 0, 2320, 800), &[], &[SCREEN]);
        assert!((f - 0.5).abs() < 1e-9, "400 of 800 px are on the screen, got {f}");
    }

    #[test]
    fn the_threshold_is_half() {
        let target = r(0, 0, 1000, 1000);
        assert!(visible_fraction(&target, &[r(0, 0, 499, 1000)], &[SCREEN]) > MIN_VISIBLE, "just over half visible");
        assert!(visible_fraction(&target, &[r(0, 0, 501, 1000)], &[SCREEN]) < MIN_VISIBLE, "just under half visible");
    }

    #[test]
    fn foreground_is_watched_even_when_the_maths_says_otherwise() {
        let p = vec![placed(1, 10, "wt.exe", "claude", r(0, 0, 100, 100))];
        assert!(window_watched(0, &p, &[SCREEN], Some(1)));
        assert!(window_watched(0, &p, &[SCREEN], None), "alone on screen");
    }

    #[test]
    fn a_minimised_window_is_never_watched() {
        let mut p = vec![placed(1, 10, "wt.exe", "claude", r(0, 0, 900, 900))];
        p[0].minimized = true;
        assert!(!window_watched(0, &p, &[SCREEN], Some(1)), "even if it was the last foreground window");
    }

    #[test]
    fn covered_windows_are_not_watched_and_a_window_in_front_is() {
        let p = vec![
            placed(1, 10, "chrome.exe", "docs", r(0, 0, 1920, 1080)),
            placed(2, 20, "wt.exe", "claude", r(300, 200, 1200, 800)),
        ];
        assert!(window_watched(0, &p, &[SCREEN], Some(1)));
        assert!(!window_watched(1, &p, &[SCREEN], Some(1)), "hidden behind the maximised browser");
    }

    fn sess(key: &str, agent: &str, folder: Option<&str>) -> SessionRef {
        SessionRef { key: key.into(), agent: agent.into(), folder: folder.map(|s| s.to_string()) }
    }

    #[test]
    fn a_terminal_agent_is_found_through_its_process_and_its_window() {
        let procs = vec![proc(10, 1, "windowsterminal.exe", ""), proc(11, 10, "node.exe", "node claude --resume"), proc(1, 0, "explorer.exe", "")];
        let wins = vec![win(7, 10, "windowsterminal.exe", "PowerShell")];
        assert_eq!(session_window(&sess("claude:a", "claude", None), &procs, &wins, 999), Some(7));
        assert_eq!(session_window(&sess("codex:a", "codex", None), &procs, &wins, 999), None, "no codex process");
    }

    #[test]
    fn two_sessions_of_one_agent_use_their_own_window_by_folder() {
        // one terminal process hosts both windows; the title carries the folder
        let procs = vec![proc(10, 1, "windowsterminal.exe", ""), proc(11, 10, "node.exe", "node claude")];
        let wins = vec![win(7, 10, "windowsterminal.exe", "zync - claude"), win(8, 10, "windowsterminal.exe", "nexus - claude")];
        assert_eq!(session_window(&sess("a", "claude", Some("nexus")), &procs, &wins, 999), Some(8));
        assert_eq!(session_window(&sess("b", "claude", Some("zync")), &procs, &wins, 999), Some(7));
    }

    #[test]
    fn an_ide_agent_is_found_by_the_name_of_its_window() {
        let wins = vec![win(5, 50, "cursor.exe", "zync - Cursor")];
        assert_eq!(session_window(&sess("cursor:a", "cursor", None), &[], &wins, 999), Some(5));
        assert_eq!(session_window(&sess("claude:a", "claude", None), &[], &wins, 999), None);
    }

    #[test]
    fn claude_in_front_is_watched_and_codex_behind_it_is_not() {
        let procs = vec![
            proc(10, 1, "windowsterminal.exe", ""),
            proc(11, 10, "node.exe", "node claude"),
            proc(20, 1, "code.exe", ""),
            proc(21, 20, "node.exe", "node codex"),
        ];
        let p = vec![
            placed(1, 10, "windowsterminal.exe", "claude", r(0, 0, 1920, 1080)),
            placed(2, 20, "code.exe", "codex", r(100, 100, 1000, 800)),
        ];
        let ss = vec![sess("claude:a", "claude", None), sess("codex:a", "codex", None)];
        let w = watched_now(&ss, &procs, &p, &[SCREEN], Some(1), 999);
        assert!(w.contains("claude:a"));
        assert!(!w.contains("codex:a"), "codex is behind the maximised terminal, so it shows on the island");
    }

    #[test]
    fn a_split_screen_watches_both_agents() {
        let procs = vec![
            proc(10, 1, "windowsterminal.exe", ""),
            proc(11, 10, "node.exe", "node claude"),
            proc(20, 1, "wezterm.exe", ""),
            proc(21, 20, "node.exe", "node codex"),
        ];
        let p = vec![
            placed(1, 10, "windowsterminal.exe", "claude", r(0, 0, 960, 1080)),
            placed(2, 20, "wezterm.exe", "codex", r(960, 0, 1920, 1080)),
        ];
        let ss = vec![sess("claude:a", "claude", None), sess("codex:a", "codex", None)];
        let w = watched_now(&ss, &procs, &p, &[SCREEN], Some(1), 999);
        assert_eq!(w.len(), 2, "both are on screen at once, so neither needs the island");
    }

    #[test]
    fn an_agent_without_a_window_is_not_watched() {
        let ss = vec![sess("claude:a", "claude", None)];
        assert!(watched_now(&ss, &[], &[], &[SCREEN], None, 999).is_empty());
    }

    #[test]
    fn debounce_hides_after_half_a_second_and_shows_after_a_second_and_a_half() {
        let mut d = Debounce::default();
        let keys = vec!["a".to_string()];
        let none: BTreeSet<String> = BTreeSet::new();
        let a: BTreeSet<String> = ["a".to_string()].into();
        // a brand-new session takes its raw state at once: unwatched = it shows right away
        assert!(d.step(0, &none, &keys).is_empty());
        // the user opens its window: still shown until the watch has held WATCH_ON_MS
        assert!(d.step(100, &a, &keys).is_empty());
        assert!(d.step(100 + WATCH_ON_MS - 1, &a, &keys).is_empty());
        assert!(d.step(100 + WATCH_ON_MS, &a, &keys).contains("a"));
        // an alt-tab away and back inside WATCH_OFF_MS never shows it
        assert!(d.step(2_000, &none, &keys).contains("a"));
        assert!(d.step(2_000 + WATCH_OFF_MS - 1, &a, &keys).contains("a"));
        assert!(d.step(2_000 + WATCH_OFF_MS, &none, &keys).contains("a"), "the timer restarted when it was watched again");
        // staying away long enough shows it
        assert!(d.step(2_000 + 2 * WATCH_OFF_MS + 1, &none, &keys).is_empty());
    }

    #[test]
    fn a_session_that_ended_is_forgotten() {
        let mut d = Debounce::default();
        let a: BTreeSet<String> = ["a".to_string()].into();
        d.step(0, &a, &["a".to_string()]);
        assert!(d.step(10, &a, &[]).is_empty());
        assert!(d.state.is_empty());
    }

    #[test]
    fn a_command_line_that_only_mentions_an_agent_is_not_that_agent() {
        let words = crate::session_observer::agent_words("codex");
        assert!(!is_agent_process(&proc(5, 1, "grep.exe", r"c:\git\grep.exe -e codex file"), words));
        assert!(!is_agent_process(&proc(6, 1, "bash.exe", "bash.exe -c source ~/.claude/codex-snapshot.sh"), words));
        assert!(is_agent_process(&proc(7, 1, "codex.exe", r"c:\tools\codex.exe --flag"), words));
        assert!(is_agent_process(&proc(8, 1, "node.exe", r#""c:\program files\nodejs\node.exe" c:\npm\node_modules\@openai\codex\bin\codex.js exec"#), words));
        assert!(!is_agent_process(&proc(9, 1, "node.exe", "node.exe server.js --name codex"), words));
    }

    #[test]
    fn a_shell_that_mentions_an_agent_does_not_hide_that_agent_behind_the_window_that_runs_the_shell() {
        // Claude Desktop (a window) runs a shell whose command line says "codex": codex is NOT in that window
        let procs = vec![proc(100, 1, "claude.exe", "claude.exe"), proc(101, 100, "bash.exe", "bash.exe -c grep codex")];
        let wins = vec![win(1, 100, "claude.exe", "Claude")];
        let placed = vec![placed(1, 100, "claude.exe", "Claude", r(0, 0, 1920, 1080))];
        let codex = sess("codex", "codex", None);
        assert!(agent_roots("codex", &procs, 9999).is_empty());
        assert_eq!(session_window(&codex, &procs, &wins, 9999), None);
        assert!(watched_now(&[codex], &procs, &placed, &[SCREEN], Some(1), 9999).is_empty(), "shown, not hidden");
    }

    /// Developer probe: `cargo test --lib live_probe -- --ignored --nocapture` prints, for each agent, the window it
    /// maps to and how much of it is visible right now. Move windows around and run it again.
    #[test]
    #[ignore]
    fn live_probe() {
        let (placed, screens, fg) = snapshot_windows();
        let mut sampler = crate::session_observer::ProcSampler::new();
        let procs = sampler.sample();
        let wins: Vec<TopWin> = placed.iter().map(|p| p.win.clone()).collect();
        println!("screens: {screens:?}  foreground: {fg:?}");
        for (i, p) in placed.iter().enumerate() {
            let above: Vec<Rect> = placed[..i].iter().filter(|q| !q.minimized).map(|q| q.rect).collect();
            println!("  z{i:02} hwnd={} pid={} {} min={} rect={:?} vis={:.2} '{}'", p.win.hwnd, p.win.pid, p.win.exe, p.minimized, p.rect, visible_fraction(&p.rect, &above, &screens), p.win.title.chars().take(50).collect::<String>());
        }
        for agent in ["claude", "codex", "antigravity", "cursor", "opencode"] {
            let s = SessionRef { key: agent.into(), agent: agent.into(), folder: None };
            for r in agent_roots(agent, &procs, std::process::id()) {
                let pr = procs.iter().find(|p| p.pid == r);
                println!("    root {r}: {:?} cmd={}", pr.map(|p| p.name.clone()), pr.map(|p| p.cmd.chars().take(140).collect::<String>()).unwrap_or_default());
            }
            let w = session_window(&s, &procs, &wins, std::process::id());
            let idx = w.and_then(|h| placed.iter().position(|p| p.win.hwnd == h));
            println!("{agent}: window={w:?} z={idx:?} watched={}", idx.is_some_and(|i| window_watched(i, &placed, &screens, fg)));
        }
    }
}
