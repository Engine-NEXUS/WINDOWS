//! Action Router, keyboard tier (shortcut brain, phase E1).
//!
//! `run` turns "perform ACTION in the app that is in front" into at most one chord sent, with the safety
//! gates the legacy PowerShell-`SendKeys` path never had:
//!
//!   1. there is a foreground window                       (else `NoForeground`)
//!   2. it is not a protected window (banking/password…)   (`Blocked`)
//!   3. it is not an elevated app we cannot type into      (`Elevated`; UIPI would silently eat the keys)
//!   4. the Shortcut Map has a chord for (exe, action)     (`NoBinding` / `BadArg`)
//!   5. no stop was requested                              (`Stopped`)
//!   6. **the foreground is still the SAME window right before sending** (`ForegroundChanged`) —
//!      keystrokes go to whoever is in front, so a window switch between decision and send must send nothing
//!   7. send one chord (one batched call), then optionally verify an observable effect.
//!
//! All OS access is behind `Env`, so the gates — above all #6 "0 wrong-window sends" — are unit-tested
//! with a fake environment.

use super::keys::Chord;
use super::map::{Action, Lookup, Scope, ShortcutMap};

/// What the router knows about the window in front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreground {
    /// Lower-case executable name, e.g. `brave.exe`.
    pub exe: String,
    pub title: String,
    /// Opaque window identity (HWND value). Two different windows of the same app differ.
    pub hwnd: isize,
    /// The window's process is elevated (running as admin).
    pub elevated: bool,
}

/// The OS surface the router needs. The real implementation is `exec::WinEnv`.
pub trait Env {
    fn window_identity(&self) -> Option<crate::desktop_control::WindowIdentity> {
        None
    }
    fn identity_required(&self) -> bool {
        false
    }
    fn foreground(&self) -> Option<Foreground>;
    fn send(&self, chord: &Chord) -> Result<(), String>;
    fn sleep_ms(&self, ms: u64);
    fn now_ms(&self) -> u64;
    /// A ghost stop (voice "stop", Esc) is pending.
    fn stop_requested(&self) -> bool;
    fn self_elevated(&self) -> bool;
    /// Banking / password-manager / crypto-wallet windows (live::safety denylist).
    fn is_protected(&self, exe: &str, title: &str) -> bool;
    /// Semantic state, never a window-title heuristic. Unsupported providers return None.
    fn effect_snapshot(&self, _action: Action) -> Option<crate::desktop_control::EffectState> {
        None
    }
}

/// Attach the owning request's cancellation to the same tested router gates.
pub fn run_cancelled(
    env: &dyn Env,
    map: &ShortcutMap,
    action: Action,
    arg: Option<u32>,
    cancel: &std::sync::atomic::AtomicBool,
) -> Outcome {
    struct Bound<'a>(&'a dyn Env, &'a std::sync::atomic::AtomicBool);
    impl Env for Bound<'_> {
        fn window_identity(&self) -> Option<crate::desktop_control::WindowIdentity> {
            self.0.window_identity()
        }
        fn identity_required(&self) -> bool {
            self.0.identity_required()
        }
        fn foreground(&self) -> Option<Foreground> {
            self.0.foreground()
        }
        fn send(&self, c: &Chord) -> Result<(), String> {
            self.0.send(c)
        }
        fn sleep_ms(&self, ms: u64) {
            self.0.sleep_ms(ms)
        }
        fn now_ms(&self) -> u64 {
            self.0.now_ms()
        }
        fn stop_requested(&self) -> bool {
            self.1.load(std::sync::atomic::Ordering::Relaxed) || self.0.stop_requested()
        }
        fn self_elevated(&self) -> bool {
            self.0.self_elevated()
        }
        fn is_protected(&self, e: &str, t: &str) -> bool {
            self.0.is_protected(e, t)
        }
        fn effect_snapshot(&self, a: Action) -> Option<crate::desktop_control::EffectState> {
            self.0.effect_snapshot(a)
        }
    }
    run(&Bound(env, cancel), map, action, arg)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abort {
    NoForeground,
    Blocked,
    Elevated,
    ForegroundChanged { expected: String, actual: String },
    Stopped,
}

/// Did the action visibly take effect?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verified {
    /// No cheap observable exists for this action (copy, save, scroll…).
    NotChecked,
    /// An action-specific state change was observed in the intended window.
    Yes,
    /// Keys were sent but no change was observed. NOT a failure (e.g. "tab 2" while already on tab 2).
    Unconfirmed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Sent {
        chord: String,
        scope: Scope,
        verified: Verified,
        ms: u64,
    },
    /// No chord for this (app, action). `known_app`: the app is in a seeded family.
    NoBinding {
        known_app: bool,
        exe: String,
    },
    BadArg(String),
    Aborted(Abort),
    Failed(String),
}

/// Milliseconds to observe the requested semantic effect after sending.
/// `None` = nothing cheap to observe, so no waiting at all.
pub fn verify_budget_ms(action: Action) -> Option<u64> {
    match action {
        Action::NewTab
        | Action::CloseTab
        | Action::ReopenTab
        | Action::NextTab
        | Action::PrevTab
        | Action::GotoTab
        | Action::GoBack
        | Action::GoForward
        | Action::ScrollDown
        | Action::ScrollUp => Some(2_000),
        _ => None,
    }
}

const VERIFY_POLL_MS: u64 = 50;

pub fn expected_effect(
    action: Action,
    arg: Option<u32>,
    before: &crate::desktop_control::EffectState,
) -> crate::desktop_control::EffectSpec {
    use crate::desktop_control::EffectSpec as E;
    match action {
        Action::NewTab | Action::ReopenTab => E::TabCount(1),
        Action::CloseTab => E::TabCount(-1),
        Action::GotoTab => arg
            .and_then(|n| {
                if n == 9 {
                    before
                        .tab_ids
                        .as_ref()
                        .and_then(|ids| ids.len().checked_sub(1))
                        .map(|i| i as u32)
                } else {
                    n.checked_sub(1)
                }
            })
            .map(|n| E::TabSelected(Some(n as usize)))
            .unwrap_or(E::Unsupported),
        Action::NextTab | Action::PrevTab => {
            let index = before.tab_ids.as_ref().and_then(|ids| {
                before
                    .selected_tab
                    .as_ref()
                    .and_then(|id| ids.iter().position(|x| x == id))
                    .map(|i| {
                        if action == Action::NextTab {
                            (i + 1) % ids.len()
                        } else {
                            (i + ids.len() - 1) % ids.len()
                        }
                    })
            });
            index
                .map(|i| E::TabSelected(Some(i)))
                .unwrap_or(E::Unsupported)
        }
        Action::GoBack | Action::GoForward => E::DocumentChanged,
        Action::ScrollDown => E::Scroll(true),
        Action::ScrollUp => E::Scroll(false),
        _ => E::Unsupported,
    }
}

pub fn run(env: &dyn Env, map: &ShortcutMap, action: Action, arg: Option<u32>) -> Outcome {
    let t0 = env.now_ms();
    let identity = env.window_identity();
    if env.identity_required() && identity.is_none() {
        return Outcome::Aborted(Abort::NoForeground);
    }

    // 1-3: who is in front, and may we act on it at all?
    let Some(fg) = env.foreground() else {
        return Outcome::Aborted(Abort::NoForeground);
    };
    if env.is_protected(&fg.exe, &fg.title) {
        return Outcome::Aborted(Abort::Blocked);
    }
    if fg.elevated && !env.self_elevated() {
        return Outcome::Aborted(Abort::Elevated);
    }

    // 4: is there a chord?
    let (chord, scope) = match map.lookup(&fg.exe, action, arg) {
        Lookup::Found { chord, scope, .. } => (chord, scope),
        Lookup::NoBinding { known_app, .. } => {
            return Outcome::NoBinding {
                known_app,
                exe: fg.exe.clone(),
            };
        }
        Lookup::BadArg(msg) => return Outcome::BadArg(msg),
    };

    // 5: a pending stop wins.
    if env.stop_requested() {
        return Outcome::Aborted(Abort::Stopped);
    }

    // 6: THE gate — still the same window, immediately before the keys go out.
    match env.foreground() {
        None => return Outcome::Aborted(Abort::NoForeground),
        Some(now) => {
            if now.hwnd != fg.hwnd || !now.exe.eq_ignore_ascii_case(&fg.exe) {
                return Outcome::Aborted(Abort::ForegroundChanged {
                    expected: fg.exe,
                    actual: now.exe,
                });
            }
        }
    }
    if env.stop_requested() {
        return Outcome::Aborted(Abort::Stopped);
    }

    let before = env.effect_snapshot(action);
    // Snapshot acquisition may block: recheck after observing, before dispatch.
    if env.stop_requested() {
        return Outcome::Aborted(Abort::Stopped);
    }
    match env.foreground() {
        Some(now) if now.hwnd == fg.hwnd && now.exe.eq_ignore_ascii_case(&fg.exe) => {
            if env.is_protected(&now.exe, &now.title) {
                return Outcome::Aborted(Abort::Blocked);
            }
            if now.elevated && !env.self_elevated() {
                return Outcome::Aborted(Abort::Elevated);
            }
            if before.as_ref().is_some_and(|s| {
                s.window.hwnd != fg.hwnd || !s.window.exe.eq_ignore_ascii_case(&fg.exe)
            }) {
                return Outcome::Aborted(Abort::ForegroundChanged {
                    expected: fg.exe,
                    actual: now.exe,
                });
            }
        }
        _ => {
            return Outcome::Aborted(Abort::ForegroundChanged {
                expected: fg.exe,
                actual: "changed".into(),
            })
        }
    }
    if let Some(expected) = identity {
        if expected.hwnd != fg.hwnd
            || !expected.exe.eq_ignore_ascii_case(&fg.exe)
            || env.window_identity().as_ref() != Some(&expected)
        {
            return Outcome::Aborted(Abort::ForegroundChanged {
                expected: fg.exe,
                actual: "process identity changed".into(),
            });
        }
    }
    // No late lookup may dispatch after its owning request has expired.
    if env.stop_requested() {
        return Outcome::Aborted(Abort::Stopped);
    }
    if env.now_ms().saturating_sub(t0) > crate::desktop_control::VERIFY_MS {
        return Outcome::Failed("Desktop observation timed out before dispatch.".into());
    }
    // 7: send once, then verify the requested effect.
    if let Err(e) = env.send(&chord) {
        return Outcome::Failed(e);
    }

    let verified = match verify_budget_ms(action) {
        None => Verified::NotChecked,
        Some(budget) => {
            let start = env.now_ms();
            let effect = before.as_ref().map(|s| expected_effect(action, arg, s));
            let mut seen_change = false;
            loop {
                if env.stop_requested() {
                    return Outcome::Aborted(Abort::Stopped);
                }
                env.sleep_ms(VERIFY_POLL_MS);
                match env.foreground() {
                    // A vanished or unrelated window cannot prove the requested effect.
                    None => {
                        break;
                    }
                    Some(now) => {
                        if env.is_protected(&now.exe, &now.title) {
                            return Outcome::Aborted(Abort::Blocked);
                        }
                        if now.hwnd != fg.hwnd || !now.exe.eq_ignore_ascii_case(&fg.exe) {
                            break;
                        }
                    }
                }
                if let (Some(before), Some(effect), Some(after)) =
                    (&before, &effect, env.effect_snapshot(action))
                {
                    if env.stop_requested() {
                        return Outcome::Aborted(Abort::Stopped);
                    }
                    let still_same = env.foreground().is_some_and(|now| {
                        now.hwnd == fg.hwnd
                            && now.exe.eq_ignore_ascii_case(&fg.exe)
                            && !env.is_protected(&now.exe, &now.title)
                    });
                    if still_same
                        && env.now_ms().saturating_sub(start) <= budget
                        && effect.observed(before, &after)
                    {
                        seen_change = true;
                        break;
                    }
                }
                if env.now_ms().saturating_sub(start) >= budget {
                    break;
                }
            }
            if seen_change {
                Verified::Yes
            } else {
                Verified::Unconfirmed
            }
        }
    };

    Outcome::Sent {
        chord: chord.display(),
        scope,
        verified,
        ms: env.now_ms().saturating_sub(t0),
    }
}

/// "brave.exe" → "Brave".
fn app_display(exe: &str) -> String {
    let base = exe.trim_end_matches(".exe");
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => "that app".to_string(),
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn noun(action: Action) -> &'static str {
    match action {
        Action::GotoTab => "tab-switching",
        Action::NewTab => "new-tab",
        Action::CloseTab => "close-tab",
        Action::ReopenTab => "reopen-tab",
        Action::NextTab => "next-tab",
        Action::PrevTab => "previous-tab",
        Action::Refresh => "refresh",
        Action::GoBack => "back",
        Action::GoForward => "forward",
        Action::AddressBar => "address-bar",
        Action::FindInPage => "find",
        Action::Save => "save",
        Action::Copy => "copy",
        Action::Paste => "paste",
        Action::Cut => "cut",
        Action::Undo => "undo",
        Action::Redo => "redo",
        Action::SelectAll => "select-all",
        Action::ScrollDown | Action::ScrollUp => "scroll",
        Action::ZoomIn | Action::ZoomOut => "zoom",
        Action::SwitchWindow => "window-switching",
    }
}

/// Why nothing (useful) happened, as a plain clause with NO honorific and no final stop, so callers can
/// embed it in their own sentence ("Couldn't switch tabs, sir: {reason}") without doubling "sir".
/// `None` for `Sent`.
pub fn reason(action: Action, out: &Outcome) -> Option<String> {
    match out {
        Outcome::Sent { .. } => None,
        Outcome::NoBinding { known_app, exe } => Some(if *known_app {
            format!("{} has no {} shortcut", app_display(exe), noun(action))
        } else {
            format!(
                "I don't know a {} shortcut for {}",
                noun(action),
                app_display(exe)
            )
        }),
        Outcome::BadArg(msg) => Some(format!("which tab? {msg}")),
        Outcome::Aborted(a) => Some(match a {
            Abort::NoForeground => "I can't tell which window is in front".to_string(),
            Abort::Blocked => "that window is protected, I won't send keys to it".to_string(),
            Abort::Elevated => "that app runs as administrator, I can't send it keys".to_string(),
            Abort::ForegroundChanged { .. } => "the window changed, I pressed nothing".to_string(),
            Abort::Stopped => "stopped".to_string(),
        }),
        Outcome::Failed(e) => Some(format!("couldn't send the shortcut: {e}")),
    }
}

/// Spoken result for direct speech. `Ok` = success line (identical wording to the legacy executor so nothing
/// the user hears changes); `Err` = a clear spoken reason, never silence.
pub fn spoken(action: Action, arg: Option<u32>, out: &Outcome) -> Result<String, String> {
    if matches!(
        out,
        Outcome::Sent {
            verified: Verified::Unconfirmed | Verified::NotChecked,
            ..
        }
    ) {
        return Ok("The shortcut was sent once, but I could not verify its result, sir.".into());
    }
    match reason(action, out) {
        None => Ok(match (action, arg) {
            (Action::GotoTab, Some(n)) => format!("Switched to tab {n}, sir."),
            _ => format!("{} sir.", capitalize(action.label())),
        }),
        Some(r) => Err(format!("{}, sir.", capitalize(&r))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shortcuts::map::SEED;
    use std::cell::{Cell, RefCell};

    /// Scriptable environment: `fg` is consumed one answer per `foreground()` call (the last repeats).
    struct FakeEnv {
        fg: RefCell<Vec<Option<Foreground>>>,
        sent: RefCell<Vec<String>>,
        send_err: Option<String>,
        stop: Cell<bool>,
        stop_after_decision: Cell<bool>,
        self_elev: bool,
        protected_exe: Option<&'static str>,
        protected_title: Option<&'static str>,
        clock: Cell<u64>,
        fg_calls: Cell<usize>,
        call_log: RefCell<Vec<String>>,
        semantic_change: bool,
    }

    fn win(exe: &str, title: &str, hwnd: isize) -> Foreground {
        Foreground {
            exe: exe.into(),
            title: title.into(),
            hwnd,
            elevated: false,
        }
    }

    impl FakeEnv {
        fn new(seq: Vec<Option<Foreground>>) -> Self {
            Self {
                fg: RefCell::new(seq),
                sent: RefCell::new(vec![]),
                send_err: None,
                stop: Cell::new(false),
                stop_after_decision: Cell::new(false),
                self_elev: false,
                protected_exe: None,
                protected_title: None,
                clock: Cell::new(0),
                fg_calls: Cell::new(0),
                call_log: RefCell::new(vec![]),
                semantic_change: false,
            }
        }
        /// The same window forever.
        fn steady(f: Foreground) -> Self {
            Self::new(vec![Some(f)])
        }
        fn sent(&self) -> Vec<String> {
            self.sent.borrow().clone()
        }
        fn calls(&self) -> Vec<String> {
            self.call_log.borrow().clone()
        }
    }

    impl Env for FakeEnv {
        fn effect_snapshot(&self, action: Action) -> Option<crate::desktop_control::EffectState> {
            use crate::desktop_control::*;
            let fg = self.fg.borrow()[0].clone()?;
            let mut state = EffectState::empty(WindowIdentity {
                hwnd: fg.hwnd,
                pid: 2,
                created: 3,
                exe: fg.exe,
            });
            let after = self.semantic_change && !self.sent.borrow().is_empty();
            state.tab_ids = Some(if action == Action::NextTab {
                vec![vec![1], vec![2]]
            } else if after {
                vec![vec![1], vec![2]]
            } else {
                vec![vec![1]]
            });
            state.selected_tab = Some(if after { vec![2] } else { vec![1] });
            Some(state)
        }
        fn foreground(&self) -> Option<Foreground> {
            self.fg_calls.set(self.fg_calls.get() + 1);
            self.call_log.borrow_mut().push("foreground".to_string());
            let mut q = self.fg.borrow_mut();
            if q.len() > 1 {
                q.remove(0)
            } else {
                q[0].clone()
            }
        }
        fn send(&self, chord: &Chord) -> Result<(), String> {
            self.call_log
                .borrow_mut()
                .push(format!("send({})", chord.display()));
            if let Some(e) = &self.send_err {
                return Err(e.clone());
            }
            self.sent.borrow_mut().push(chord.display());
            Ok(())
        }
        fn sleep_ms(&self, ms: u64) {
            self.call_log.borrow_mut().push(format!("sleep({ms})"));
            self.clock.set(self.clock.get() + ms);
        }
        fn now_ms(&self) -> u64 {
            self.clock.get()
        }
        fn stop_requested(&self) -> bool {
            if self.stop_after_decision.get() && self.fg_calls.get() > 1 {
                return true;
            }
            self.stop.get()
        }
        fn self_elevated(&self) -> bool {
            self.self_elev
        }
        fn is_protected(&self, exe: &str, title: &str) -> bool {
            if self.protected_exe.map(|p| p == exe).unwrap_or(false) {
                return true;
            }
            if let Some(needle) = self.protected_title {
                if title
                    .to_ascii_lowercase()
                    .contains(&needle.to_ascii_lowercase())
                {
                    return true;
                }
            }
            false
        }
    }

    #[test]
    fn happy_path_sends_exactly_one_chord_and_verifies_tab_evidence() {
        // decision, pre-send gate, then the title changes on the first poll.
        let mut env = FakeEnv::new(vec![
            Some(win("brave.exe", "Inbox - Brave", 7)),
            Some(win("brave.exe", "Inbox - Brave", 7)),
            Some(win("brave.exe", "New Tab - Brave", 7)),
        ]);
        env.semantic_change = true;
        let out = run(&env, &SEED, Action::NewTab, None);
        assert_eq!(env.sent(), vec!["Ctrl+T"]);
        match out {
            Outcome::Sent {
                chord,
                verified,
                scope,
                ..
            } => {
                assert_eq!(chord, "Ctrl+T");
                assert_eq!(verified, Verified::Yes);
                assert_eq!(scope, Scope::Family("browser".into()));
            }
            other => panic!("expected Sent, got {other:?}"),
        }
    }

    #[test]
    fn unchanged_title_is_unconfirmed_not_a_failure() {
        let env = FakeEnv::steady(win("brave.exe", "Same - Brave", 7));
        let out = run(&env, &SEED, Action::GotoTab, Some(2)); // already on tab 2: title cannot change
        assert_eq!(env.sent(), vec!["Ctrl+2"]);
        assert!(
            matches!(
                out,
                Outcome::Sent {
                    verified: Verified::Unconfirmed,
                    ..
                }
            ),
            "{out:?}"
        );
        // It waited its budget, and no longer.
        assert!(
            env.now_ms() >= 2000 && env.now_ms() < 2000 + 2 * VERIFY_POLL_MS + 1,
            "{}",
            env.now_ms()
        );
    }

    /// THE GATE: the window changes between decision and send => nothing is sent.
    #[test]
    fn foreground_change_before_send_sends_nothing() {
        let env = FakeEnv::new(vec![
            Some(win("brave.exe", "Inbox", 7)),   // decision
            Some(win("slack.exe", "General", 9)), // user alt-tabbed before the keys went out
        ]);
        let out = run(&env, &SEED, Action::NewTab, None);
        assert_eq!(
            out,
            Outcome::Aborted(Abort::ForegroundChanged {
                expected: "brave.exe".into(),
                actual: "slack.exe".into()
            })
        );
        assert!(env.sent().is_empty(), "0 wrong-window sends");
    }

    #[test]
    fn a_different_window_of_the_same_app_also_aborts() {
        let env = FakeEnv::new(vec![
            Some(win("brave.exe", "A", 7)),
            Some(win("brave.exe", "B", 8)),
        ]);
        assert!(matches!(
            run(&env, &SEED, Action::Refresh, None),
            Outcome::Aborted(Abort::ForegroundChanged { .. })
        ));
        assert!(env.sent().is_empty());
    }

    #[test]
    fn losing_the_foreground_entirely_sends_nothing() {
        let env = FakeEnv::new(vec![Some(win("brave.exe", "A", 7)), None]);
        assert_eq!(
            run(&env, &SEED, Action::Refresh, None),
            Outcome::Aborted(Abort::NoForeground)
        );
        assert!(env.sent().is_empty());
        let none = FakeEnv::new(vec![None]);
        assert_eq!(
            run(&none, &SEED, Action::Refresh, None),
            Outcome::Aborted(Abort::NoForeground)
        );
    }

    #[test]
    fn protected_windows_get_no_keys_and_are_not_even_looked_up() {
        let mut env = FakeEnv::steady(win("1password.exe", "Vault", 3));
        env.protected_exe = Some("1password.exe");
        assert_eq!(
            run(&env, &SEED, Action::Paste, None),
            Outcome::Aborted(Abort::Blocked)
        );
        assert!(env.sent().is_empty());
    }

    #[test]
    fn elevated_targets_are_refused_unless_we_are_elevated_too() {
        let mut f = win("taskmgr.exe", "Task Manager", 4);
        f.elevated = true;
        let env = FakeEnv::steady(f.clone());
        assert_eq!(
            run(&env, &SEED, Action::Copy, None),
            Outcome::Aborted(Abort::Elevated)
        );
        assert!(env.sent().is_empty());

        let mut both = FakeEnv::steady(f);
        both.self_elev = true;
        assert!(matches!(
            run(&both, &SEED, Action::Copy, None),
            Outcome::Sent { .. }
        ));
    }

    #[test]
    fn stop_requested_wins_at_either_checkpoint() {
        let env = FakeEnv::steady(win("brave.exe", "A", 7));
        env.stop.set(true);
        assert_eq!(
            run(&env, &SEED, Action::NewTab, None),
            Outcome::Aborted(Abort::Stopped)
        );
        assert!(env.sent().is_empty());
    }

    #[test]
    fn no_binding_reports_known_vs_unknown_apps_and_sends_nothing() {
        let env = FakeEnv::steady(win("notepad.exe", "Untitled", 5));
        assert_eq!(
            run(&env, &SEED, Action::Refresh, None),
            Outcome::NoBinding {
                known_app: true,
                exe: "notepad.exe".into()
            }
        );
        let env = FakeEnv::steady(win("randomapp.exe", "x", 6));
        assert_eq!(
            run(&env, &SEED, Action::NewTab, None),
            Outcome::NoBinding {
                known_app: false,
                exe: "randomapp.exe".into()
            }
        );
        assert!(env.sent().is_empty());
    }

    #[test]
    fn consoles_never_receive_global_edit_shortcuts() {
        // Ctrl+C in a console is an interrupt: must be NoBinding, never a send.
        for exe in [
            "cmd.exe",
            "powershell.exe",
            "pwsh.exe",
            "windowsterminal.exe",
        ] {
            for a in [Action::Cut, Action::Undo, Action::Save, Action::SelectAll] {
                let env = FakeEnv::steady(win(exe, "term", 2));
                assert!(
                    matches!(
                        run(&env, &SEED, a, None),
                        Outcome::NoBinding {
                            known_app: true,
                            ..
                        }
                    ),
                    "{exe} {a:?}"
                );
                assert!(env.sent().is_empty());
            }
        }
        let env = FakeEnv::steady(win("cmd.exe", "cmd", 2));
        assert!(matches!(
            run(&env, &SEED, Action::Copy, None),
            Outcome::NoBinding { .. }
        ));
        assert!(env.sent().is_empty());
    }

    #[test]
    fn bad_tab_number_is_rejected_before_anything_is_sent() {
        let env = FakeEnv::steady(win("brave.exe", "A", 7));
        assert!(matches!(
            run(&env, &SEED, Action::GotoTab, Some(12)),
            Outcome::BadArg(_)
        ));
        assert!(matches!(
            run(&env, &SEED, Action::GotoTab, None),
            Outcome::BadArg(_)
        ));
        assert!(env.sent().is_empty());
    }

    #[test]
    fn send_failure_is_reported_and_not_verified() {
        let mut env = FakeEnv::steady(win("brave.exe", "A", 7));
        env.send_err = Some("blocked by UIPI".into());
        assert_eq!(
            run(&env, &SEED, Action::NewTab, None),
            Outcome::Failed("blocked by UIPI".into())
        );
        assert_eq!(env.now_ms(), 0, "no verify wait after a failed send");
    }

    #[test]
    fn actions_without_an_observable_effect_do_not_wait() {
        let env = FakeEnv::steady(win("notepad.exe", "Untitled", 5));
        let out = run(&env, &SEED, Action::Copy, None);
        assert!(matches!(
            out,
            Outcome::Sent {
                verified: Verified::NotChecked,
                ..
            }
        ));
        assert_eq!(env.now_ms(), 0);
        assert_eq!(
            env.fg_calls.get(),
            3,
            "decision + pre-send + post-observation recheck"
        );
    }

    #[test]
    fn changed_foreground_cannot_confirm_a_closed_tab() {
        let env = FakeEnv::new(vec![
            Some(win("brave.exe", "Only tab", 7)),
            Some(win("brave.exe", "Only tab", 7)),
            Some(win("explorer.exe", "Desktop", 1)),
        ]);
        assert!(matches!(
            run(&env, &SEED, Action::CloseTab, None),
            Outcome::Aborted(_)
                | Outcome::Sent {
                    verified: Verified::Unconfirmed,
                    ..
                }
        ));
    }

    /// RELEASE GATE (docs/features/103 E1): over the whole seeded browser table, every action sends the
    /// documented chord exactly once, and no run ever sends to a window that changed.
    #[test]
    fn corpus_gate_every_browser_action_sends_once_and_never_to_the_wrong_window() {
        let expect = [
            (Action::NewTab, None, "Ctrl+T"),
            (Action::CloseTab, None, "Ctrl+W"),
            (Action::ReopenTab, None, "Ctrl+Shift+T"),
            (Action::NextTab, None, "Ctrl+Tab"),
            (Action::PrevTab, None, "Ctrl+Shift+Tab"),
            (Action::GotoTab, Some(1), "Ctrl+1"),
            (Action::GotoTab, Some(8), "Ctrl+8"),
            (Action::GotoTab, Some(9), "Ctrl+9"),
            (Action::Refresh, None, "F5"),
            (Action::GoBack, None, "Alt+Left"),
            (Action::GoForward, None, "Alt+Right"),
            (Action::AddressBar, None, "Ctrl+L"),
            (Action::FindInPage, None, "Ctrl+F"),
            (Action::Save, None, "Ctrl+S"),
            (Action::Copy, None, "Ctrl+C"),
            (Action::Paste, None, "Ctrl+V"),
            (Action::Cut, None, "Ctrl+X"),
            (Action::Undo, None, "Ctrl+Z"),
            (Action::Redo, None, "Ctrl+Y"),
            (Action::SelectAll, None, "Ctrl+A"),
            (Action::ScrollDown, None, "PageDown"),
            (Action::ScrollUp, None, "PageUp"),
            (Action::ZoomIn, None, "Ctrl+="),
            (Action::ZoomOut, None, "Ctrl+-"),
            (Action::SwitchWindow, None, "Alt+Tab"),
        ];
        let mut pass = 0;
        for exe in ["brave.exe", "chrome.exe", "msedge.exe", "firefox.exe"] {
            for (action, arg, want) in expect {
                // Same window throughout => exactly one send of the documented chord.
                let env = FakeEnv::steady(win(exe, "Page", 7));
                let out = run(&env, &SEED, action, arg);
                assert!(
                    matches!(out, Outcome::Sent { .. }),
                    "{exe} {action:?}: {out:?}"
                );
                assert_eq!(env.sent(), vec![want.to_string()], "{exe} {action:?}");
                // Window swapped before the send => nothing sent.
                let swapped = FakeEnv::new(vec![
                    Some(win(exe, "Page", 7)),
                    Some(win("other.exe", "x", 9)),
                ]);
                let out = run(&swapped, &SEED, action, arg);
                assert!(
                    matches!(out, Outcome::Aborted(Abort::ForegroundChanged { .. })),
                    "{out:?}"
                );
                assert!(
                    swapped.sent().is_empty(),
                    "wrong-window send for {exe} {action:?}"
                );
                pass += 1;
            }
        }
        assert_eq!(pass, 4 * expect.len(), "100% of the corpus");
    }

    #[test]
    fn spoken_lines_keep_the_legacy_wording_on_success() {
        let sent = |chord: &str| Outcome::Sent {
            chord: chord.into(),
            scope: Scope::Global,
            verified: Verified::Yes,
            ms: 1,
        };
        assert_eq!(
            spoken(Action::NewTab, None, &sent("Ctrl+T")).unwrap(),
            "New tab sir."
        );
        assert_eq!(
            spoken(Action::CloseTab, None, &sent("Ctrl+W")).unwrap(),
            "Tab closed sir."
        );
        assert_eq!(
            spoken(Action::NextTab, None, &sent("Ctrl+Tab")).unwrap(),
            "Next tab sir."
        );
        assert_eq!(
            spoken(Action::GoBack, None, &sent("Alt+Left")).unwrap(),
            "Back sir."
        );
        assert_eq!(
            spoken(Action::GotoTab, Some(3), &sent("Ctrl+3")).unwrap(),
            "Switched to tab 3, sir."
        );
    }

    #[test]
    fn reasons_have_no_honorific_so_callers_can_embed_them() {
        let r = reason(
            Action::GotoTab,
            &Outcome::NoBinding {
                known_app: true,
                exe: "notepad.exe".into(),
            },
        )
        .unwrap();
        assert_eq!(r, "Notepad has no tab-switching shortcut");
        assert!(!r.contains("sir"));
        let r = reason(
            Action::Refresh,
            &Outcome::Aborted(Abort::ForegroundChanged {
                expected: "a".into(),
                actual: "b".into(),
            }),
        )
        .unwrap();
        assert_eq!(r, "the window changed, I pressed nothing");
        // Sent has no reason; direct speech capitalises and adds the honorific exactly once.
        let sent = Outcome::Sent {
            chord: "F5".into(),
            scope: Scope::Global,
            verified: Verified::NotChecked,
            ms: 0,
        };
        assert_eq!(reason(Action::Refresh, &sent), None);
        assert_eq!(
            spoken(Action::Refresh, None, &Outcome::Aborted(Abort::Stopped)).unwrap_err(),
            "Stopped, sir."
        );
    }

    #[test]
    fn every_failure_has_a_spoken_reason() {
        let cases = [
            Outcome::NoBinding {
                known_app: true,
                exe: "notepad.exe".into(),
            },
            Outcome::NoBinding {
                known_app: false,
                exe: "x.exe".into(),
            },
            Outcome::BadArg("tab 12 is out of range".into()),
            Outcome::Aborted(Abort::NoForeground),
            Outcome::Aborted(Abort::Blocked),
            Outcome::Aborted(Abort::Elevated),
            Outcome::Aborted(Abort::ForegroundChanged {
                expected: "a".into(),
                actual: "b".into(),
            }),
            Outcome::Aborted(Abort::Stopped),
            Outcome::Failed("boom".into()),
        ];
        for c in cases {
            let msg = spoken(Action::Refresh, None, &c).expect_err("failures are Err");
            assert!(msg.ends_with("sir.") || msg.contains("sir"), "{msg}");
        }
        assert_eq!(
            spoken(
                Action::Refresh,
                None,
                &Outcome::NoBinding {
                    known_app: true,
                    exe: "notepad.exe".into()
                }
            )
            .unwrap_err(),
            "Notepad has no refresh shortcut, sir."
        );
    }

    #[test]
    fn wrong_foreground_at_recheck_swapped_app_or_hwnd_or_gone() {
        // Sub-case 1: Application exe changed before send.
        let env_app_changed = FakeEnv::new(vec![
            Some(win("brave.exe", "Inbox", 10)),
            Some(win("code.exe", "editor.rs", 20)),
        ]);
        let out = run(&env_app_changed, &SEED, Action::NewTab, None);
        assert_eq!(
            out,
            Outcome::Aborted(Abort::ForegroundChanged {
                expected: "brave.exe".into(),
                actual: "code.exe".into(),
            })
        );
        assert!(env_app_changed.sent().is_empty());
        assert_eq!(
            spoken(Action::NewTab, None, &out).unwrap_err(),
            "The window changed, I pressed nothing, sir."
        );

        // Sub-case 2: Window handle changed (different window of same app).
        let env_hwnd_changed = FakeEnv::new(vec![
            Some(win("brave.exe", "Tab 1", 10)),
            Some(win("brave.exe", "Tab 2", 11)),
        ]);
        let out = run(&env_hwnd_changed, &SEED, Action::Refresh, None);
        assert_eq!(
            out,
            Outcome::Aborted(Abort::ForegroundChanged {
                expected: "brave.exe".into(),
                actual: "brave.exe".into(),
            })
        );
        assert!(env_hwnd_changed.sent().is_empty());

        // Sub-case 3: Foreground window lost completely right before send.
        let env_lost = FakeEnv::new(vec![Some(win("brave.exe", "Inbox", 10)), None]);
        let out = run(&env_lost, &SEED, Action::CloseTab, None);
        assert_eq!(out, Outcome::Aborted(Abort::NoForeground));
        assert!(env_lost.sent().is_empty());
        assert_eq!(
            spoken(Action::CloseTab, None, &out).unwrap_err(),
            "I can't tell which window is in front, sir."
        );
    }

    #[test]
    fn protected_exe_list_password_managers_and_banking_refused() {
        for exe in ["1password.exe", "bitwarden.exe", "keepass.exe", "chase.exe"] {
            let mut env = FakeEnv::steady(win(exe, "Vault", 100));
            env.protected_exe = Some(exe);
            let out = run(&env, &SEED, Action::Copy, None);
            assert_eq!(out, Outcome::Aborted(Abort::Blocked));
            assert!(
                env.sent().is_empty(),
                "never send keys to protected exe {exe}"
            );
            assert_eq!(
                reason(Action::Copy, &out),
                Some("that window is protected, I won't send keys to it".to_string())
            );
            assert_eq!(
                spoken(Action::Copy, None, &out).unwrap_err(),
                "That window is protected, I won't send keys to it, sir."
            );
        }
    }

    #[test]
    fn credential_tool_recognised_only_by_window_title_refuses() {
        let sensitive_titles = [
            ("brave.exe", "Bitwarden Web Vault - Personal"),
            ("chrome.exe", "1Password — Password Manager"),
            ("msedge.exe", "MetaMask Notification"),
            ("firefox.exe", "KeePass Password Safe - Database"),
            ("brave.exe", "LastPass Vault - Web Browser"),
        ];

        for (exe, title) in sensitive_titles {
            let mut env = FakeEnv::steady(win(exe, title, 50));
            // Match any credential needle in title.
            for needle in ["bitwarden", "1password", "metamask", "keepass", "lastpass"] {
                if title.to_ascii_lowercase().contains(needle) {
                    env.protected_title = Some(needle);
                    break;
                }
            }
            let out = run(&env, &SEED, Action::Paste, None);
            assert_eq!(
                out,
                Outcome::Aborted(Abort::Blocked),
                "blocked by title '{title}'"
            );
            assert!(
                env.sent().is_empty(),
                "0 keys sent to credential title '{title}'"
            );
            assert_eq!(
                spoken(Action::Paste, None, &out).unwrap_err(),
                "That window is protected, I won't send keys to it, sir."
            );
        }
    }

    #[test]
    fn elevated_target_refuses_and_says_why() {
        let mut elevated_target = win("powershell.exe", "Administrator: Windows PowerShell", 99);
        elevated_target.elevated = true;

        let env = FakeEnv::steady(elevated_target);
        assert!(!env.self_elevated());

        let out = run(&env, &SEED, Action::CloseTab, None);
        assert_eq!(out, Outcome::Aborted(Abort::Elevated));
        assert!(env.sent().is_empty());
        assert_eq!(
            reason(Action::CloseTab, &out),
            Some("that app runs as administrator, I can't send it keys".to_string())
        );
        assert_eq!(
            spoken(Action::CloseTab, None, &out).unwrap_err(),
            "That app runs as administrator, I can't send it keys, sir."
        );
    }

    #[test]
    fn verification_title_change_succeeds_vs_unconfirmed_vs_failure_spoken_results() {
        // Case A: Title change verified (Verified::Yes) -> spoken is Ok.
        let mut env_verified = FakeEnv::new(vec![
            Some(win("brave.exe", "Old Tab - Brave", 1)),
            Some(win("brave.exe", "Old Tab - Brave", 1)),
            Some(win("brave.exe", "New Tab - Brave", 1)),
        ]);
        env_verified.semantic_change = true;
        let out_verified = run(&env_verified, &SEED, Action::NewTab, None);
        match &out_verified {
            Outcome::Sent { verified, .. } => assert_eq!(*verified, Verified::Yes),
            other => panic!("expected Sent, got {other:?}"),
        }
        assert_eq!(
            spoken(Action::NewTab, None, &out_verified).unwrap(),
            "New tab sir."
        );

        // Case B: Title change unconfirmed (Verified::Unconfirmed) -> keys sent, spoken is still Ok.
        let env_unconfirmed = FakeEnv::steady(win("brave.exe", "Static Tab - Brave", 1));
        let out_unconfirmed = run(&env_unconfirmed, &SEED, Action::NewTab, None);
        match &out_unconfirmed {
            Outcome::Sent { verified, .. } => assert_eq!(*verified, Verified::Unconfirmed),
            other => panic!("expected Sent, got {other:?}"),
        }
        assert!(spoken(Action::NewTab, None, &out_unconfirmed)
            .unwrap()
            .contains("could not verify"));

        // Case C: Recheck abort (Abort::ForegroundChanged) -> spoken is Err.
        let env_aborted = FakeEnv::new(vec![
            Some(win("brave.exe", "Old Tab", 1)),
            Some(win("other.exe", "Swapped", 2)),
        ]);
        let out_aborted = run(&env_aborted, &SEED, Action::NewTab, None);
        assert!(matches!(
            out_aborted,
            Outcome::Aborted(Abort::ForegroundChanged { .. })
        ));
        assert_eq!(
            spoken(Action::NewTab, None, &out_aborted).unwrap_err(),
            "The window changed, I pressed nothing, sir."
        );

        // Case D: Send failure (Outcome::Failed) -> spoken is Err.
        let mut env_failed = FakeEnv::steady(win("brave.exe", "Tab", 1));
        env_failed.send_err = Some("SendInput error 5".into());
        let out_failed = run(&env_failed, &SEED, Action::NewTab, None);
        assert!(matches!(out_failed, Outcome::Failed(_)));
        assert_eq!(
            spoken(Action::NewTab, None, &out_failed).unwrap_err(),
            "Couldn't send the shortcut: SendInput error 5, sir."
        );
    }

    #[test]
    fn send_failure_is_reported_and_not_hidden() {
        let mut env = FakeEnv::steady(win("notepad.exe", "Document - Notepad", 10));
        env.send_err = Some("Win32 error: blocked by UIPI".into());
        let out = run(&env, &SEED, Action::Save, None);
        assert_eq!(out, Outcome::Failed("Win32 error: blocked by UIPI".into()));
        assert!(env.sent().is_empty());
        assert_eq!(
            reason(Action::Save, &out),
            Some("couldn't send the shortcut: Win32 error: blocked by UIPI".to_string())
        );
        assert_eq!(
            spoken(Action::Save, None, &out).unwrap_err(),
            "Couldn't send the shortcut: Win32 error: blocked by UIPI, sir."
        );
        // Failed send skips verification completely.
        assert_eq!(env.now_ms(), 0);
    }

    #[test]
    fn presend_recheck_call_order_and_timing() {
        let mut env = FakeEnv::new(vec![
            Some(win("brave.exe", "Tab 1 - Brave", 1)), // 1. Initial decision
            Some(win("brave.exe", "Tab 1 - Brave", 1)), // 2. Pre-send re-check
            Some(win("brave.exe", "Tab 2 - Brave", 1)), // 3. Verification poll
        ]);
        env.semantic_change = true;
        let out = run(&env, &SEED, Action::NextTab, None);
        assert!(matches!(
            out,
            Outcome::Sent {
                verified: Verified::Yes,
                ..
            }
        ));

        let calls = env.calls();
        // Call #0: initial foreground check.
        assert_eq!(calls[0], "foreground");
        // Call #1: pre-send re-check (must happen immediately before send).
        assert_eq!(calls[1], "foreground");
        // Call #2: send chord.
        assert_eq!(calls[2], "foreground");
        // Call #3: sleep in verification loop.
        assert_eq!(calls[3], "send(Ctrl+Tab)");
        // Call #4: verification foreground poll.
        assert_eq!(calls[4], "sleep(50)");
    }

    #[test]
    fn known_app_without_shortcut_says_so_with_speakable_text() {
        let known_cases = [
            (
                win("notepad.exe", "Untitled - Notepad", 1),
                Action::Refresh,
                "Notepad has no refresh shortcut, sir.",
            ),
            (
                win("notepad.exe", "Untitled - Notepad", 1),
                Action::ReopenTab,
                "Notepad has no reopen-tab shortcut, sir.",
            ),
            (
                win("notepad.exe", "Untitled - Notepad", 1),
                Action::GoBack,
                "Notepad has no back shortcut, sir.",
            ),
            (
                win("cmd.exe", "Command Prompt", 2),
                Action::Save,
                "Cmd has no save shortcut, sir.",
            ),
            (
                win("cmd.exe", "Command Prompt", 2),
                Action::Undo,
                "Cmd has no undo shortcut, sir.",
            ),
            (
                win("explorer.exe", "File Explorer", 3),
                Action::ReopenTab,
                "Explorer has no reopen-tab shortcut, sir.",
            ),
            (
                win("windowsterminal.exe", "Windows Terminal", 4),
                Action::GoBack,
                "Windowsterminal has no back shortcut, sir.",
            ),
        ];

        for (target, action, expected_spoken) in known_cases {
            let env = FakeEnv::steady(target);
            let out = run(&env, &SEED, action, None);
            assert!(
                matches!(
                    out,
                    Outcome::NoBinding {
                        known_app: true,
                        ..
                    }
                ),
                "{:?} should report NoBinding(known_app=true)",
                action
            );
            assert!(env.sent().is_empty());
            assert_eq!(spoken(action, None, &out).unwrap_err(), expected_spoken);
        }
    }

    #[test]
    fn unknown_app_without_binding_says_so_differently_from_known_app() {
        let env_unknown = FakeEnv::steady(win("arc.exe", "Arc", 1));
        let out_unknown = run(&env_unknown, &SEED, Action::NewTab, None);
        assert_eq!(
            out_unknown,
            Outcome::NoBinding {
                known_app: false,
                exe: "arc.exe".into()
            }
        );
        assert_eq!(
            reason(Action::NewTab, &out_unknown),
            Some("I don't know a new-tab shortcut for Arc".to_string())
        );
        assert_eq!(
            spoken(Action::NewTab, None, &out_unknown).unwrap_err(),
            "I don't know a new-tab shortcut for Arc, sir."
        );

        // Contrast with known app:
        let env_known = FakeEnv::steady(win("notepad.exe", "Untitled", 2));
        let out_known = run(&env_known, &SEED, Action::Refresh, None);
        assert_eq!(
            spoken(Action::Refresh, None, &out_known).unwrap_err(),
            "Notepad has no refresh shortcut, sir."
        );
    }

    #[test]
    fn actions_with_and_without_observable_verification_budget() {
        // Actions with observable title change wait up to 700ms.
        for action in [
            Action::NewTab,
            Action::CloseTab,
            Action::ReopenTab,
            Action::NextTab,
            Action::PrevTab,
            Action::GotoTab,
            Action::GoBack,
            Action::GoForward,
            Action::ScrollDown,
            Action::ScrollUp,
        ] {
            assert_eq!(verify_budget_ms(action), Some(2000), "{action:?}");
        }

        // Actions without observable title change have None budget and do not poll.
        for action in [
            Action::Refresh,
            Action::AddressBar,
            Action::FindInPage,
            Action::Save,
            Action::Copy,
            Action::Paste,
            Action::Cut,
            Action::Undo,
            Action::Redo,
            Action::SelectAll,
            Action::ZoomIn,
            Action::ZoomOut,
            Action::SwitchWindow,
        ] {
            assert_eq!(verify_budget_ms(action), None, "{action:?}");
        }

        // Running an unobservable action sends keys immediately and skips sleep.
        let env = FakeEnv::steady(win("brave.exe", "Page - Brave", 1));
        let out = run(&env, &SEED, Action::Save, None);
        assert!(matches!(
            out,
            Outcome::Sent {
                verified: Verified::NotChecked,
                ..
            }
        ));
        assert_eq!(env.now_ms(), 0);
        assert_eq!(
            env.calls(),
            vec!["foreground", "foreground", "foreground", "send(Ctrl+S)"]
        );
    }

    #[test]
    fn stop_requested_at_presend_recheck_aborts_send() {
        let env = FakeEnv::steady(win("brave.exe", "Page - Brave", 1));
        // Set stop to trigger after the initial decision (on the 2nd foreground call).
        env.stop_after_decision.set(true);

        let out = run(&env, &SEED, Action::NewTab, None);
        assert_eq!(out, Outcome::Aborted(Abort::Stopped));
        assert!(
            env.sent().is_empty(),
            "stop request at pre-send gate must send nothing"
        );
        assert_eq!(
            spoken(Action::NewTab, None, &out).unwrap_err(),
            "Stopped, sir."
        );
    }
}
