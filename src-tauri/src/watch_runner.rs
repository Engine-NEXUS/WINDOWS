//! The unattended part of "watch this session, and if it ends perfectly, shut down my laptop"
//! (doc 142, phase W5). It ties the held plan (`power_plan`), what can be seen of the agent
//! (`session_observer`, the agent's own Stop hook) and the decision (`session_verdict`) to a countdown
//! that can power the laptop off.
//!
//! The decisions are a PURE state machine (`Engine`, injected clock, no I/O) so every path, above all every
//! way of NOT shutting down, is tested. The loop around it (`run_loop`) only gathers observations and
//! carries out the engine's actions.
//!
//! What it takes to power off (all of it, at once, every time):
//!   1. the `watchPower` setting is `on` (default `off`; `dry` = practice, says it but never does it);
//!   2. a plan is held, and it is not "just tell me";
//!   3. the session ENDED (two independent signals, nothing still busy) and the verdict is Go for 15 s in a row;
//!   4. a 30 s countdown, announced aloud, then runs, and during it ANY of these stops it:
//!      "cancel" / "stop" / "wait" ..., a key or mouse movement, a meeting starting, the plan cancelled, the
//!      setting turned off, or the verdict no longer being Go. Not knowing whether the user is back also stops it;
//!   5. the normal shutdown sequence (goodbye, graceful close of apps, a last 3 s cancel window) does the rest.

use crate::power_cmd::PowerKind;
use crate::power_plan::WatchPowerMode;
use crate::session_verdict::{spoken_hold, Verdict};

/// How often the loop looks.
pub const TICK_MS: u64 = 3_000;
/// The verdict must stay the same this long before anything is concluded (a flicker never counts).
pub const SETTLE_MS: u64 = 15_000;
/// The announced, cancellable wait before the power call.
pub const COUNTDOWN_MS: u64 = 30_000;
/// How long a plan is watched when it names no limit.
pub const DEFAULT_MAX_WAIT_MS: u64 = 3 * 60 * 60 * 1000;
/// No plan waits longer than this, whatever was said.
pub const MAX_WAIT_CEILING_MS: u64 = 12 * 60 * 60 * 1000;
/// Input this soon after the countdown began is the user's last action before it, not a reaction to it.
pub const INPUT_SLACK_MS: u64 = 1_500;

#[derive(Debug, Clone, PartialEq)]
pub struct PlanView {
    pub kind: PowerKind,
    pub notify_only: bool,
    pub deadline_ms: Option<u64>,
    /// "Claude Code" / "Codex" / "this session"
    pub who: String,
}

#[derive(Debug, Clone)]
pub struct Obs {
    pub now_ms: u64,
    pub mode: WatchPowerMode,
    /// `None` = the plan was cancelled or replaced.
    pub plan: Option<PlanView>,
    /// `session_verdict::has_ended`: two independent signals and nothing still busy.
    pub ended: bool,
    pub verdict: Verdict,
    /// Milliseconds since the last keyboard / mouse input; `None` = cannot tell.
    pub user_idle_ms: Option<u64>,
    /// The user said cancel (or began another power command) since the countdown started.
    pub voice_cancelled: bool,
    pub meeting: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Tell the user (through the proactive-speech policy: it waits for a breakpoint).
    Say(String),
    /// Tell the user and keep a strict "yes" ready for "shut down anyway?".
    SayAndAsk(String, PowerKind),
    /// Start the cancellable countdown (and announce it).
    BeginCountdown { kind: PowerKind, who: String },
    /// The countdown ran out with nothing stopping it: power off now.
    Fire(PowerKind),
    /// The watch is over. Nothing more happens for this plan.
    End(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
enum Phase {
    Watching { concluding: Option<(bool, u64)> },
    Countdown { start_ms: u64 },
    Finished,
}

#[derive(Debug)]
pub struct Engine {
    started_ms: u64,
    phase: Phase,
}

fn gerund(kind: PowerKind) -> &'static str {
    match kind {
        PowerKind::Shutdown => "shutting down your laptop",
        PowerKind::Restart => "restarting your laptop",
        PowerKind::Sleep => "putting your laptop to sleep",
        PowerKind::Lock => "locking your screen",
        PowerKind::SignOut => "signing you out",
    }
}

fn human_duration(ms: u64) -> String {
    let mins = ms / 60_000;
    if mins >= 120 {
        format!("{} hours", mins / 60)
    } else if mins >= 60 {
        "an hour".to_string()
    } else {
        format!("{} minute{}", mins.max(1), if mins <= 1 { "" } else { "s" })
    }
}

impl Engine {
    pub fn new(now_ms: u64) -> Engine {
        Engine { started_ms: now_ms, phase: Phase::Watching { concluding: None } }
    }

    pub fn finished(&self) -> bool {
        self.phase == Phase::Finished
    }

    fn end(&mut self, reason: &'static str) -> Vec<Action> {
        self.phase = Phase::Finished;
        vec![Action::End(reason)]
    }

    fn abort(&mut self, why: &str, reason: &'static str) -> Vec<Action> {
        self.phase = Phase::Finished;
        vec![Action::Say(format!("Stopped, sir: {why}. I didn't touch your laptop.")), Action::End(reason)]
    }

    /// A countdown is running right now.
    pub fn counting_down(&self) -> bool {
        matches!(self.phase, Phase::Countdown { .. })
    }

    pub fn step(&mut self, o: &Obs) -> Vec<Action> {
        match self.phase.clone() {
            Phase::Finished => Vec::new(),
            Phase::Countdown { start_ms } => self.step_countdown(o, start_ms),
            Phase::Watching { concluding } => self.step_watching(o, concluding),
        }
    }

    fn step_watching(&mut self, o: &Obs, concluding: Option<(bool, u64)>) -> Vec<Action> {
        let Some(plan) = &o.plan else { return self.end("plan gone") };
        if o.mode == WatchPowerMode::Off && !plan.notify_only {
            return self.end("unattended shutdown is off");
        }
        let limit = plan.deadline_ms.unwrap_or(DEFAULT_MAX_WAIT_MS).min(MAX_WAIT_CEILING_MS);
        if o.now_ms.saturating_sub(self.started_ms) > limit {
            let said = format!("I stopped waiting for {} after {}, sir, so I left your laptop on.", plan.who, human_duration(limit));
            let mut out = vec![Action::Say(said)];
            out.extend(self.end("waited long enough"));
            return out;
        }
        if !o.ended {
            // still working (or not enough evidence that it stopped): nothing is concluded, however long it takes
            self.phase = Phase::Watching { concluding: None };
            return Vec::new();
        }
        let is_go = o.verdict == Verdict::Go;
        let since = match concluding {
            Some((was_go, since)) if was_go == is_go => since,
            _ => o.now_ms,
        };
        self.phase = Phase::Watching { concluding: Some((is_go, since)) };
        if o.now_ms.saturating_sub(since) < SETTLE_MS {
            return Vec::new();
        }

        if !is_go {
            // Finished, but not perfect (or not sure). Tell the user; only a real "on" setting offers to go ahead.
            let ask = o.mode == WatchPowerMode::On && !plan.notify_only;
            let said = spoken_hold(&plan.who, &o.verdict, ask);
            let mut out = vec![if ask { Action::SayAndAsk(said, plan.kind) } else { Action::Say(said) }];
            out.extend(self.end("not perfect"));
            return out;
        }

        // Finished and perfect.
        if plan.notify_only {
            let said = format!("{} finished and it looks perfect, sir. You asked me only to tell you, so I haven't touched your laptop.", plan.who);
            let mut out = vec![Action::Say(said)];
            out.extend(self.end("notify only"));
            return out;
        }
        match o.mode {
            WatchPowerMode::Off => self.end("unattended shutdown is off"),
            WatchPowerMode::Dry => {
                let said = format!(
                    "Practice run, sir: {} finished and it looks perfect. I would have started {} after a 30 second countdown. Nothing was changed.",
                    plan.who,
                    gerund(plan.kind)
                );
                let mut out = vec![Action::Say(said)];
                out.extend(self.end("dry run"));
                out
            }
            WatchPowerMode::On => {
                self.phase = Phase::Countdown { start_ms: o.now_ms };
                vec![Action::BeginCountdown { kind: plan.kind, who: plan.who.clone() }]
            }
        }
    }

    fn step_countdown(&mut self, o: &Obs, start_ms: u64) -> Vec<Action> {
        let Some(plan) = &o.plan else { return self.abort("the plan was cancelled", "cancelled: plan") };
        if o.mode != WatchPowerMode::On {
            return self.abort("unattended shutdown was switched off", "cancelled: setting");
        }
        if o.voice_cancelled {
            return self.abort("you said cancel", "cancelled: voice");
        }
        if o.meeting {
            return self.abort("a meeting started", "cancelled: meeting");
        }
        if !(o.ended && o.verdict == Verdict::Go) {
            return self.abort("something changed on the session", "cancelled: session changed");
        }
        let elapsed = o.now_ms.saturating_sub(start_ms);
        match o.user_idle_ms {
            None => return self.abort("I can't tell whether you're back at the keyboard", "cancelled: input unknown"),
            Some(idle) if idle + INPUT_SLACK_MS < elapsed => {
                return self.abort("you're back at the keyboard", "cancelled: input");
            }
            Some(_) => {}
        }
        if elapsed >= COUNTDOWN_MS {
            self.phase = Phase::Finished;
            return vec![Action::Fire(plan.kind)];
        }
        Vec::new()
    }
}

/// The announcement spoken the moment the countdown begins.
pub fn announcement(kind: PowerKind, who: &str) -> String {
    format!(
        "{who} finished and it looks perfect, sir. I'm {} in 30 seconds. Say cancel, or touch the keyboard, to stop me.",
        gerund(kind)
    )
}

// ─── The loop around the engine ──────────────────────────────────────

use crate::agent_hooks::{self, Agent, EventKind};
use crate::power_plan;
use crate::session_observer::{find_roots, tree_snaps, Inputs, Observer, ProcSampler};
use crate::session_verdict::{has_ended, judge, model_system_prompt, parse_model_json, ModelVerdict};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Manager, Runtime};

static RUNNING: AtomicBool = AtomicBool::new(false);

/// The `watchPower` setting, read fresh every time so "turn off unattended shutdown" works at once.
/// Anything missing or unreadable is `off`.
pub fn read_mode<R: Runtime>(app: &AppHandle<R>) -> WatchPowerMode {
    let Ok(dir) = app.path().app_data_dir() else { return WatchPowerMode::Off };
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|j| j.get("watchPower").and_then(|v| v.as_str()).map(WatchPowerMode::from_setting))
        .unwrap_or(WatchPowerMode::Off)
}

/// Milliseconds since the last keyboard / mouse input (None if it cannot be read: then nothing is powered off).
#[cfg(target_os = "windows")]
fn user_idle_ms() -> Option<u64> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    // SAFETY: `info` is a valid, correctly sized LASTINPUTINFO.
    let ok = unsafe { GetLastInputInfo(&mut info) };
    if !ok.as_bool() {
        return None;
    }
    let now = unsafe { GetTickCount() };
    Some(now.wrapping_sub(info.dwTime) as u64)
}

#[cfg(not(target_os = "windows"))]
fn user_idle_ms() -> Option<u64> {
    None
}

fn meeting_active<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.try_state::<std::sync::Arc<crate::meeting_detect::MeetingState>>()
        .map(|m| m.should_suppress_tts())
        .unwrap_or(false)
}

/// Is there anything for the loop to do right now? A plan for an agent whose Stop hook is installed (without
/// it the end of a session cannot be told), and either "just tell me" or a setting that is not `off`.
fn wants_running<R: Runtime>(app: &AppHandle<R>) -> bool {
    let Some(p) = power_plan::current() else { return false };
    let Some(agent) = p.plan.agent.as_deref().and_then(Agent::from_name) else { return false };
    let hooked = agent_hooks::config_path(agent).map_or(false, |c| agent_hooks::is_installed(agent, &c));
    hooked && (p.notify_only || read_mode(app) != WatchPowerMode::Off)
}

/// Start watching the held plan (once). Safe to call whenever the plan or the setting changes.
pub fn ensure_running<R: Runtime>(app: AppHandle<R>) {
    if !wants_running(&app) || RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        run_loop(app.clone()).await;
        RUNNING.store(false, Ordering::SeqCst);
        // a plan made while the loop was winding down must not be left unwatched
        if wants_running(&app) {
            ensure_running(app);
        }
    });
}

/// Ask a free cloud model for a second opinion on the agent's final message (masked, 4 KB at most, logged in
/// the Memory page's "sent to cloud" list). It can only make the verdict stricter. Anything wrong = no opinion.
async fn ask_model<R: Runtime>(app: &AppHandle<R>, tail: &str) -> ModelVerdict {
    if tail.trim().is_empty() {
        return ModelVerdict::Unavailable;
    }
    let Ok(dir) = app.path().app_data_dir() else { return ModelVerdict::Unavailable };
    if crate::memcore::names::enabled(&dir) {
        return ModelVerdict::Unavailable; // name hiding is on: nothing about the session leaves the device
    }
    crate::memcore::log_egress(&dir, "watchverdict", tail, "");
    let keys = crate::router::read_provider_keys(app);
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(10)).build() else { return ModelVerdict::Unavailable };
    match tokio::time::timeout(Duration::from_secs(12), crate::router::route_with_system(model_system_prompt(), tail, 120, &keys, &client)).await {
        Ok(Some(reply)) => parse_model_json(&reply.text).unwrap_or(ModelVerdict::Unavailable),
        _ => ModelVerdict::Unavailable,
    }
}

async fn run_loop<R: Runtime>(app: AppHandle<R>) {
    let t0 = std::time::Instant::now();
    let mut sampler = Some(ProcSampler::new());
    let mut observer = Observer::default();
    let mut engine: Option<(u64, Engine)> = None;
    let mut model_cache: Option<(u64, ModelVerdict)> = None;
    let mut countdown_gen: Option<u64> = None;
    let mut ticks: u64 = 0;
    println!("[WATCH-POWER] watching");

    'watch: loop {
        tokio::time::sleep(Duration::from_millis(TICK_MS)).await;
        ticks += 1;
        let now_ms = t0.elapsed().as_millis() as u64;
        let counting = engine.as_ref().map_or(false, |(_, e)| e.counting_down());
        let stored = power_plan::current();
        let mode = read_mode(&app);

        let Some(stored) = stored else {
            if counting {
                // the plan was cancelled mid-countdown: let the engine say so and stop
                let acts = engine.as_mut().map(|(_, e)| e.step(&Obs { plan: None, ..idle_obs(now_ms, mode) })).unwrap_or_default();
                carry_out(&app, acts, &mut countdown_gen).await;
            }
            break 'watch;
        };
        if mode == WatchPowerMode::Off && !stored.notify_only && !counting {
            println!("[WATCH-POWER] unattended shutdown is off: nothing to do");
            break 'watch;
        }
        let Some(agent) = stored.plan.agent.as_deref().and_then(Agent::from_name) else {
            println!("[WATCH-POWER] no hookable agent for this plan");
            break 'watch;
        };

        // a new plan replaces the old one: start over
        if engine.as_ref().map(|(g, _)| *g) != Some(stored.generation) {
            if let Some(g) = countdown_gen.take() {
                crate::power_cmd::finish(g);
            }
            engine = Some((stored.generation, Engine::new(now_ms)));
            observer = Observer::default();
            model_cache = None;
        }

        // ── look ──
        let snaps = match sampler.take() {
            Some(mut s) => match tauri::async_runtime::spawn_blocking(move || {
                let v = s.sample();
                (s, v)
            })
            .await
            {
                Ok((s, v)) => {
                    sampler = Some(s);
                    v
                }
                Err(_) => {
                    sampler = Some(ProcSampler::new());
                    Vec::new()
                }
            },
            None => {
                sampler = Some(ProcSampler::new());
                Vec::new()
            }
        };
        let roots = find_roots(Some(agent.name()), &snaps, None, std::process::id());
        let tree = tree_snaps(&roots, &snaps);
        let hook_tail = agent_hooks::last_stop_tail(agent);
        let stop_info = agent_hooks::last_stop_info(agent);
        let mut ev = observer.step(Inputs {
            now_ms,
            tree: &tree,
            window_text: None,
            hook_stopped: agent_hooks::stop_is_latest(agent, stored.created_ms),
            hook_tail: hook_tail.as_deref(),
            hook_needs_user: agent_hooks::needs_user(agent, stored.created_ms),
            hook_failed: stop_info.ok == Some(false),
            hook_not_idle: stop_info.fully_idle == Some(false),
        });

        // ── decide ──
        let require_clean = stored.plan.require_clean;
        let mut verdict = judge(&ev, require_clean);
        if has_ended(&ev) && verdict == crate::session_verdict::Verdict::Go {
            let key = agent_hooks::last_event_ms(agent, EventKind::Stop).unwrap_or(0);
            if model_cache.as_ref().map(|(k, _)| *k) != Some(key) {
                let m = ask_model(&app, &ev.tail).await;
                model_cache = Some((key, m));
            }
            if let Some((_, m)) = &model_cache {
                ev.model = m.clone();
                verdict = judge(&ev, require_clean);
            }
        }
        let view = PlanView {
            kind: stored.plan.kind,
            notify_only: stored.notify_only,
            deadline_ms: stored.plan.deadline_mins.map(|m| u64::from(m) * 60_000),
            who: agent.label().to_string(),
        };
        let obs = Obs {
            now_ms,
            mode,
            plan: Some(view),
            ended: has_ended(&ev),
            verdict,
            user_idle_ms: user_idle_ms(),
            voice_cancelled: countdown_gen.map_or(false, |g| !crate::power_cmd::is_current(g)),
            meeting: meeting_active(&app),
        };
        if ticks % 10 == 1 {
            println!(
                "[WATCH-POWER] tick {ticks}: mode={} ended={} verdict={:?} signals={:?} busy={} roots={}",
                mode.as_str(),
                obs.ended,
                obs.verdict,
                ev.signals,
                ev.still_busy,
                roots.len()
            );
        }
        let Some((_, eng)) = engine.as_mut() else { break 'watch };
        let acts = eng.step(&obs);
        let finished = eng.finished();
        let done = carry_out(&app, acts, &mut countdown_gen).await;
        if done || finished {
            break 'watch;
        }
    }
    if let Some(g) = countdown_gen.take() {
        crate::power_cmd::finish(g);
    }
    println!("[WATCH-POWER] stopped watching");
}

fn idle_obs(now_ms: u64, mode: WatchPowerMode) -> Obs {
    Obs {
        now_ms,
        mode,
        plan: None,
        ended: false,
        verdict: Verdict::Unsure(Vec::new()),
        user_idle_ms: user_idle_ms(),
        voice_cancelled: false,
        meeting: false,
    }
}

/// Do what the engine decided. Returns true when the watch is over.
async fn carry_out<R: Runtime>(app: &AppHandle<R>, actions: Vec<Action>, countdown_gen: &mut Option<u64>) -> bool {
    let mut over = false;
    for action in actions {
        match action {
            Action::Say(text) => {
                println!("[WATCH-POWER] say: {text}");
                crate::proactive_policy::submit(app, format!("watchpower_{}", agent_hooks::now_ms()), text, crate::google::types::AlertUrgency::High);
            }
            Action::SayAndAsk(text, kind) => {
                println!("[WATCH-POWER] say+ask: {text}");
                // the answer window opens only once the question is actually being spoken (`sentinel_offer_` ids)
                crate::memcore::offer::set(crate::memcore::offer::Offer::Power { kind: kind.as_str().to_string() }, false);
                crate::proactive_policy::submit(
                    app,
                    format!("sentinel_offer_watchpower_{}", agent_hooks::now_ms()),
                    text,
                    crate::google::types::AlertUrgency::High,
                );
            }
            Action::BeginCountdown { kind, who } => {
                // the same cancel generation the spoken "cancel" listens to (see `power_cmd::cancel`)
                *countdown_gen = Some(crate::power_cmd::begin());
                println!("[WATCH-POWER] countdown started: {} in 30 s", kind.as_str());
                crate::orchestrator::speak_unprompted(app, announcement(kind, &who));
            }
            Action::Fire(kind) => {
                // last look, right before the call: the setting and the plan must both still say yes
                let power_allowed = app.path().app_data_dir().map_or(false, |d| crate::memcore::flag(&d, "powerCommands", true));
                let still_on = power_allowed && read_mode(app) == WatchPowerMode::On && power_plan::current().map_or(false, |p| !p.notify_only);
                if let Some(g) = countdown_gen.take() {
                    crate::power_cmd::finish(g);
                }
                if still_on {
                    println!("[WATCH-POWER] countdown finished: starting {}", kind.as_str());
                    let line = crate::orchestrator::start_power(app, kind).await;
                    crate::orchestrator::speak_unprompted(app, line);
                } else {
                    println!("[WATCH-POWER] last check failed: not powering off");
                }
                power_plan::clear();
                over = true;
            }
            Action::End(reason) => {
                println!("[WATCH-POWER] end: {reason}");
                if let Some(g) = countdown_gen.take() {
                    crate::power_cmd::finish(g);
                }
                power_plan::clear();
                over = true;
            }
        }
    }
    over
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_verdict::Reason;

    fn plan(kind: PowerKind) -> PlanView {
        PlanView { kind, notify_only: false, deadline_ms: None, who: "Claude Code".into() }
    }

    fn obs(now_ms: u64, mode: WatchPowerMode) -> Obs {
        Obs {
            now_ms,
            mode,
            plan: Some(plan(PowerKind::Shutdown)),
            ended: true,
            verdict: Verdict::Go,
            user_idle_ms: Some(10 * 60_000),
            voice_cancelled: false,
            meeting: false,
        }
    }

    /// Run the engine over `[from, to]` in ticks; return (time, action) pairs.
    fn run(engine: &mut Engine, from: u64, to: u64, make: impl Fn(u64) -> Obs) -> Vec<(u64, Action)> {
        let mut out = Vec::new();
        let mut t = from;
        while t <= to {
            for a in engine.step(&make(t)) {
                out.push((t, a));
            }
            t += TICK_MS;
        }
        out
    }

    fn fired(actions: &[(u64, Action)]) -> bool {
        actions.iter().any(|(_, a)| matches!(a, Action::Fire(_)))
    }

    fn began(actions: &[(u64, Action)]) -> Option<u64> {
        actions.iter().find(|(_, a)| matches!(a, Action::BeginCountdown { .. })).map(|(t, _)| *t)
    }

    // ── the one path that powers off ──

    #[test]
    fn a_perfect_finish_with_the_setting_on_counts_down_and_then_fires_exactly_once() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 120_000, |t| obs(t, WatchPowerMode::On));
        let begin = began(&acts).expect("a countdown began");
        assert!(begin >= SETTLE_MS, "the verdict had to hold for {SETTLE_MS} ms first, began at {begin}");
        let fire = acts.iter().find(|(_, a)| matches!(a, Action::Fire(_))).expect("it fired").0;
        assert!(fire - begin >= COUNTDOWN_MS, "fired {} ms after the announcement", fire - begin);
        assert_eq!(acts.iter().filter(|(_, a)| matches!(a, Action::Fire(_))).count(), 1, "exactly once");
        assert!(e.finished());
        // and nothing more, ever
        assert!(e.step(&obs(500_000, WatchPowerMode::On)).is_empty());
    }

    #[test]
    fn the_announced_action_follows_the_plan() {
        for (kind, word) in [
            (PowerKind::Shutdown, "shutting down"),
            (PowerKind::Restart, "restarting"),
            (PowerKind::Sleep, "putting your laptop to sleep"),
            (PowerKind::Lock, "locking"),
            (PowerKind::SignOut, "signing you out"),
        ] {
            assert!(announcement(kind, "Claude Code").contains(word), "{kind:?}");
            assert!(announcement(kind, "Claude Code").contains("30 seconds") && announcement(kind, "Claude Code").contains("cancel"));
        }
    }

    // ── the ways it must NOT power off ──

    #[test]
    fn with_the_setting_off_it_never_fires_or_counts_down_and_stops_quietly() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 600_000, |t| obs(t, WatchPowerMode::Off));
        assert!(!fired(&acts) && began(&acts).is_none());
        assert!(acts.iter().any(|(_, a)| matches!(a, Action::End(_))));
    }

    #[test]
    fn a_practice_run_says_what_it_would_do_and_never_does_it() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 600_000, |t| obs(t, WatchPowerMode::Dry));
        assert!(!fired(&acts) && began(&acts).is_none());
        let said: Vec<&Action> = acts.iter().map(|(_, a)| a).filter(|a| matches!(a, Action::Say(_))).collect();
        assert_eq!(said.len(), 1);
        assert!(matches!(said[0], Action::Say(t) if t.contains("Practice run") && t.contains("Nothing was changed")));
        assert!(e.finished());
    }

    #[test]
    fn just_tell_me_never_powers_off_even_with_the_setting_on() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 600_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            o.plan.as_mut().unwrap().notify_only = true;
            o
        });
        assert!(!fired(&acts) && began(&acts).is_none());
        assert!(acts.iter().any(|(_, a)| matches!(a, Action::Say(t) if t.contains("only to tell you"))));
        // and it still tells the user when the setting is off, because that is what was asked
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 600_000, |t| {
            let mut o = obs(t, WatchPowerMode::Off);
            o.plan.as_mut().unwrap().notify_only = true;
            o
        });
        assert!(acts.iter().any(|(_, a)| matches!(a, Action::Say(_))) && !fired(&acts));
    }

    #[test]
    fn a_session_that_has_not_ended_is_waited_for_and_never_concluded() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 2 * 60 * 60_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            o.ended = false;
            o
        });
        assert!(acts.is_empty(), "{acts:?}");
        assert!(!e.finished());
    }

    #[test]
    fn a_flicker_of_go_is_not_enough_the_verdict_must_hold() {
        let mut e = Engine::new(0);
        // Go for 12 s, then not ended for a tick, then Go again: the 15 s clock restarts each time
        let acts = run(&mut e, 0, 600_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            o.ended = t % 15_000 != 12_000 || t == 0;
            o
        });
        assert!(began(&acts).is_none() || began(&acts).unwrap() > 0);
        // with an interruption every 12 s it never reaches 15 s of Go in a row
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 600_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            o.ended = t % 12_000 != 9_000;
            o
        });
        assert!(began(&acts).is_none() && !fired(&acts), "{acts:?}");
    }

    #[test]
    fn not_perfect_tells_the_user_and_only_a_real_on_setting_asks_to_go_ahead() {
        let hold = Verdict::Hold(vec![Reason::OpenQuestion("Run the tests?".into())]);
        for (mode, notify, asks) in [
            (WatchPowerMode::On, false, true),
            (WatchPowerMode::On, true, false),
            (WatchPowerMode::Dry, false, false),
            (WatchPowerMode::Off, true, false),
        ] {
            let mut e = Engine::new(0);
            let acts = run(&mut e, 0, 120_000, |t| {
                let mut o = obs(t, mode);
                o.verdict = hold.clone();
                o.plan.as_mut().unwrap().notify_only = notify;
                o
            });
            assert!(!fired(&acts) && began(&acts).is_none(), "{mode:?}");
            let ask = acts.iter().any(|(_, a)| matches!(a, Action::SayAndAsk(..)));
            assert_eq!(ask, asks, "{mode:?} notify={notify}");
            let said = acts.iter().find_map(|(_, a)| match a {
                Action::Say(t) | Action::SayAndAsk(t, _) => Some(t.clone()),
                _ => None,
            });
            let said = said.expect("it tells the user");
            assert!(said.contains("not perfect") && said.contains("left your laptop on"), "{said}");
            assert_eq!(said.contains("Shall I shut down anyway"), asks, "{said}");
            assert!(e.finished());
        }
    }

    #[test]
    fn it_gives_up_after_the_time_limit_and_never_powers_off_on_a_timeout() {
        for (limit, check_at) in [(None, DEFAULT_MAX_WAIT_MS + 10_000), (Some(30 * 60_000), 31 * 60_000), (Some(99 * 60 * 60_000), MAX_WAIT_CEILING_MS + 10_000)] {
            let mut e = Engine::new(0);
            let acts = run(&mut e, 0, check_at, |t| {
                let mut o = obs(t, WatchPowerMode::On);
                o.ended = false;
                o.plan.as_mut().unwrap().deadline_ms = limit;
                o
            });
            assert!(!fired(&acts) && began(&acts).is_none());
            assert!(acts.iter().any(|(_, a)| matches!(a, Action::Say(t) if t.contains("stopped waiting") && t.contains("left your laptop on"))), "{limit:?}");
            assert!(e.finished());
        }
    }

    #[test]
    fn a_cancelled_plan_ends_the_watch_in_silence() {
        let mut e = Engine::new(0);
        let acts = e.step(&Obs { plan: None, ..obs(3_000, WatchPowerMode::On) });
        assert_eq!(acts, vec![Action::End("plan gone")]);
    }

    // ── every way to stop the countdown ──

    /// Begin a countdown at `begin_at`, then apply `change` to every observation after it.
    fn countdown_with(change: impl Fn(&mut Obs)) -> (Vec<(u64, Action)>, u64) {
        let mut e = Engine::new(0);
        let mut begun: Option<u64> = None;
        let mut t = 0;
        while t < 60_000 && begun.is_none() {
            if e.step(&obs(t, WatchPowerMode::On)).iter().any(|a| matches!(a, Action::BeginCountdown { .. })) {
                begun = Some(t);
            }
            t += TICK_MS;
        }
        let begin_at = begun.expect("the countdown should have begun");
        let acts = run(&mut e, begin_at + TICK_MS, begin_at + 90_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            if t >= begin_at + 9_000 {
                change(&mut o);
            }
            o
        });
        (acts, begin_at)
    }

    fn assert_stopped(acts: &[(u64, Action)], why: &str) {
        assert!(!fired(acts), "{why}: it still fired");
        let end = acts.iter().find_map(|(_, a)| match a {
            Action::End(r) => Some(*r),
            _ => None,
        });
        assert!(end.map_or(false, |r| r.starts_with("cancelled")), "{why}: ended with {end:?}");
        assert!(acts.iter().any(|(_, a)| matches!(a, Action::Say(t) if t.contains("didn't touch your laptop"))), "{why}: it did not say so");
    }

    #[test]
    fn saying_cancel_stops_the_countdown() {
        let (acts, _) = countdown_with(|o| o.voice_cancelled = true);
        assert_stopped(&acts, "voice");
    }

    #[test]
    fn touching_the_keyboard_or_mouse_stops_the_countdown() {
        // input 4 s before the check = after the countdown began
        let (acts, _) = countdown_with(|o| o.user_idle_ms = Some(4_000));
        assert_stopped(&acts, "input");
        // not knowing whether the user is back also stops it: it never guesses
        let (acts, _) = countdown_with(|o| o.user_idle_ms = None);
        assert_stopped(&acts, "idle unknown");
    }

    #[test]
    fn the_last_input_before_the_countdown_does_not_stop_it() {
        let mut e = Engine::new(0);
        let mut began_at = None;
        // the user typed at t=14 s (just before the verdict settled): their idle time grows with the clock
        let acts = run(&mut e, 0, 200_000, |t| {
            let mut o = obs(t, WatchPowerMode::On);
            o.user_idle_ms = Some(t.saturating_sub(14_000));
            o
        });
        if let Some(b) = began(&acts) {
            began_at = Some(b);
        }
        assert!(began_at.is_some() && fired(&acts), "{acts:?}");
    }

    #[test]
    fn a_meeting_starting_stops_the_countdown() {
        let (acts, _) = countdown_with(|o| o.meeting = true);
        assert_stopped(&acts, "meeting");
    }

    #[test]
    fn cancelling_the_plan_stops_the_countdown() {
        let (acts, _) = countdown_with(|o| o.plan = None);
        assert_stopped(&acts, "plan cancelled");
    }

    #[test]
    fn switching_the_setting_off_or_to_practice_stops_the_countdown() {
        let (acts, _) = countdown_with(|o| o.mode = WatchPowerMode::Off);
        assert_stopped(&acts, "setting off");
        let (acts, _) = countdown_with(|o| o.mode = WatchPowerMode::Dry);
        assert_stopped(&acts, "setting dry");
    }

    #[test]
    fn the_session_becoming_busy_or_imperfect_again_stops_the_countdown() {
        let (acts, _) = countdown_with(|o| o.ended = false);
        assert_stopped(&acts, "busy again");
        let (acts, _) = countdown_with(|o| o.verdict = Verdict::Unsure(vec![Reason::LowConfidence]));
        assert_stopped(&acts, "unsure");
        let (acts, _) = countdown_with(|o| o.verdict = Verdict::Hold(vec![Reason::WaitingForYou]));
        assert_stopped(&acts, "hold");
    }

    #[test]
    fn a_stop_in_the_last_tick_still_wins_over_the_fire() {
        let mut e = Engine::new(0);
        let mut begun: Option<u64> = None;
        let mut t = 0;
        while t < 120_000 && begun.is_none() {
            if e.step(&obs(t, WatchPowerMode::On)).iter().any(|a| matches!(a, Action::BeginCountdown { .. })) {
                begun = Some(t);
            }
            t += TICK_MS;
        }
        let begin_at = begun.expect("the countdown should have begun");
        // right at the end of the countdown the user says cancel
        let mut o = obs(begin_at + COUNTDOWN_MS, WatchPowerMode::On);
        o.voice_cancelled = true;
        let acts = e.step(&o);
        assert!(!acts.iter().any(|a| matches!(a, Action::Fire(_))), "{acts:?}");
        assert!(e.finished());
    }

    #[test]
    fn nothing_fires_before_the_countdown_is_over() {
        let mut e = Engine::new(0);
        let acts = run(&mut e, 0, 40_000, |t| obs(t, WatchPowerMode::On));
        let begin = began(&acts).unwrap();
        assert!(!acts.iter().any(|(t, a)| matches!(a, Action::Fire(_)) && *t < begin + COUNTDOWN_MS));
        assert!(human_duration(30 * 60_000).contains("30 minutes"));
        assert_eq!(human_duration(3 * 60 * 60_000), "3 hours");
        assert_eq!(human_duration(60_000), "1 minute");
    }

    #[test]
    fn a_verdict_that_keeps_changing_between_go_and_unsure_never_settles() {
        // the session stays "ended" but the verdict flips every 12 s: neither side lasts 15 s in a row
        for first_go in [true, false] {
            let mut e = Engine::new(0);
            let acts = run(&mut e, 0, 900_000, |t| {
                let mut o = obs(t, WatchPowerMode::On);
                let go = (t / 12_000) % 2 == 0;
                if go != first_go {
                    o.verdict = Verdict::Unsure(vec![Reason::LowConfidence]);
                }
                o
            });
            assert!(!fired(&acts) && began(&acts).is_none(), "{first_go}: {acts:?}");
            assert!(!e.finished(), "no conclusion either way");
        }
    }

    #[test]
    fn input_within_the_first_moments_of_the_countdown_is_the_last_action_before_it_but_later_input_stops_it() {
        let go = |input_after_start_ms: u64| {
            let mut e = Engine::new(0);
            let mut begun: Option<u64> = None;
            let mut out = Vec::new();
            let mut t = 0;
            while t <= 200_000 && !e.finished() {
                let mut o = obs(t, WatchPowerMode::On);
                // idle time the way the OS reports it: the last input was `input_at` (never before the start of the watch)
                if let Some(b) = begun {
                    let input_at = b + input_after_start_ms;
                    o.user_idle_ms = Some(if t >= input_at { t - input_at } else { 30 * 60_000 });
                }
                for a in e.step(&o) {
                    if matches!(a, Action::BeginCountdown { .. }) {
                        begun = Some(t);
                    }
                    out.push((t, a));
                }
                t += TICK_MS;
            }
            out
        };
        assert!(fired(&go(500)), "an input 0.5 s after the announcement began was the user's last action before it");
        assert!(!fired(&go(2_000)), "an input 2 s after the announcement began is a reaction to it");
    }

    // ── the whole chain, with the real observer and the real verdict, on a timeline ──

    use crate::session_observer::{Inputs, Observer, ProcSnap};
    use crate::session_verdict::{has_ended, judge};

    fn quiet_tree() -> Vec<ProcSnap> {
        vec![
            ProcSnap { pid: 10, ppid: 1, name: "node.exe".into(), cmd: "node claude".into(), cpu_pct: 0.3, io_total: 5_000 },
            ProcSnap { pid: 11, ppid: 10, name: "node.exe".into(), cmd: "node child".into(), cpu_pct: 0.0, io_total: 1_000 },
        ]
    }

    /// Drive Observer + judge + Engine every 3 s. `stopped_at`: when the agent's Stop hook arrives (None = never);
    /// `tail`: its final message; `prompt_at`: a new prompt that starts another turn (stop no longer counts).
    fn timeline(mode: WatchPowerMode, tail: &str, stopped_at: Option<u64>, prompt_at: Option<u64>, user_input_at: Option<u64>, cancel_at: Option<u64>) -> (Vec<(u64, Action)>, Engine) {
        let mut observer = Observer::default();
        let mut engine = Engine::new(0);
        let mut out = Vec::new();
        let tree = quiet_tree();
        let mut t = 0;
        while t <= 400_000 && !engine.finished() {
            let hook_stopped = stopped_at.map_or(false, |s| t >= s) && !prompt_at.map_or(false, |p| t >= p);
            let busy_tree;
            let tree_now: &[ProcSnap] = if prompt_at.map_or(false, |p| t >= p && t < p + 40_000) {
                // the new turn is working: CPU
                busy_tree = vec![ProcSnap { cpu_pct: 60.0, ..tree[0].clone() }, tree[1].clone()];
                &busy_tree
            } else {
                &tree
            };
            let ev = observer.step(Inputs {
                now_ms: t,
                tree: tree_now,
                window_text: None,
                hook_stopped,
                hook_tail: if hook_stopped { Some(tail) } else { None },
                hook_needs_user: false,
                hook_failed: false,
                hook_not_idle: false,
            });
            let obs = Obs {
                now_ms: t,
                mode,
                plan: Some(plan(PowerKind::Shutdown)),
                ended: has_ended(&ev),
                verdict: judge(&ev, true),
                user_idle_ms: Some(user_input_at.map_or(30 * 60_000, |u| if t >= u { t - u } else { 30 * 60_000 })),
                voice_cancelled: cancel_at.map_or(false, |c| t >= c),
                meeting: false,
            };
            for a in engine.step(&obs) {
                out.push((t, a));
            }
            t += TICK_MS;
        }
        (out, engine)
    }

    const PERFECT: &str = "Implemented the parser and wired it in.\nAll 42 tests passed.\nDone.";

    #[test]
    fn end_to_end_a_perfect_finish_with_the_hook_installed_counts_down_and_fires() {
        let (acts, _) = timeline(WatchPowerMode::On, PERFECT, Some(20_000), None, None, None);
        let begin = began(&acts).expect("a countdown began");
        // the process had to stay quiet 45 s before the ending counted, and the verdict then had to hold 15 s
        assert!(begin >= 45_000 + SETTLE_MS, "{begin}");
        assert!(fired(&acts));
    }

    #[test]
    fn end_to_end_nothing_happens_without_the_agents_stop_hook() {
        // however quiet the process is, one signal is not an ending
        let (acts, e) = timeline(WatchPowerMode::On, PERFECT, None, None, None, None);
        assert!(acts.is_empty(), "it keeps waiting, silently: {acts:?}");
        assert!(!e.finished());
    }

    #[test]
    fn end_to_end_a_failure_in_the_final_message_holds_and_asks() {
        for tail in [
            "Ran the suite.\n2 failed, 40 passed.\nDone.",
            "I couldn't finish the migration.",
            "Implemented it.\nWould you like me to run the tests?",
            "Traceback (most recent call last):\n  File \"x.py\", line 1",
        ] {
            let (acts, _) = timeline(WatchPowerMode::On, tail, Some(20_000), None, None, None);
            assert!(!fired(&acts) && began(&acts).is_none(), "{tail}");
            assert!(acts.iter().any(|(_, a)| matches!(a, Action::SayAndAsk(..))), "{tail}: {acts:?}");
        }
    }

    #[test]
    fn end_to_end_a_final_message_that_never_arrived_is_not_trusted() {
        // the hook sent no text: with nothing to judge and no model there is no go
        let (acts, _) = timeline(WatchPowerMode::On, "", Some(20_000), None, None, None);
        assert!(!fired(&acts) && began(&acts).is_none());
        assert!(acts.iter().any(|(_, a)| matches!(a, Action::SayAndAsk(t, _) if t.contains("could not read"))), "{acts:?}");
    }

    #[test]
    fn end_to_end_a_new_prompt_after_the_stop_keeps_the_watch_going() {
        // stop at 20 s, but the user sends another prompt at 30 s and the agent works for 40 s: no conclusion
        let (acts, _) = timeline(WatchPowerMode::On, PERFECT, Some(20_000), Some(30_000), None, None);
        assert!(!fired(&acts));
        assert!(began(&acts).is_none() || began(&acts).unwrap() > 70_000, "{acts:?}");
    }

    #[test]
    fn end_to_end_every_cancel_path_stops_a_real_countdown() {
        // a key press shortly after the countdown starts
        let (acts, _) = timeline(WatchPowerMode::On, PERFECT, Some(20_000), None, Some(75_000), None);
        assert!(!fired(&acts) && acts.iter().any(|(_, a)| matches!(a, Action::End(r) if *r == "cancelled: input")), "{acts:?}");
        // "cancel" said aloud
        let (acts, _) = timeline(WatchPowerMode::On, PERFECT, Some(20_000), None, None, Some(75_000));
        assert!(!fired(&acts) && acts.iter().any(|(_, a)| matches!(a, Action::End(r) if *r == "cancelled: voice")), "{acts:?}");
    }

    #[test]
    fn end_to_end_off_and_practice_never_fire_on_the_same_perfect_finish() {
        for mode in [WatchPowerMode::Off, WatchPowerMode::Dry] {
            let (acts, e) = timeline(mode, PERFECT, Some(20_000), None, None, None);
            assert!(!fired(&acts) && began(&acts).is_none(), "{mode:?}");
            assert!(e.finished());
        }
    }
}
