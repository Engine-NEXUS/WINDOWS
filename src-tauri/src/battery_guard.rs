//! Battery guard: "when the battery is under 5 percent, tell the coding agent to stop here, then shut down."
//!
//! Added 2026-11-24 (research: docs/research/watch-then-power-off-battery-guard-and-agent-coverage-2026-11-24.md).
//! Nothing read the battery before this.
//!
//! The decisions are a PURE state machine (`Engine`, injected clock, no I/O) so every way of NOT powering off is
//! tested; the loop around it (`spawn`) only reads the battery, asks the agent monitor who is working, and carries out
//! the engine's actions.
//!
//! What it takes to power off (all of it, every time):
//!   1. the `batteryGuard` setting is `on` (default `off`; `dry` = practice: speaks what it would do, does nothing);
//!   2. the laptop is on its battery (a charger, or an unreadable level, never acts);
//!   3. the level is at or under the chosen percent (default 5, never under 3 or over 20);
//!   4. an announced 30 s countdown that stops on "cancel" (the same cancel as the shutdown countdown), or when the
//!      charger is connected, or when the level recovers;
//!   5. the normal sequence does the rest (a shutdown says goodbye, closes apps gracefully and gives a last 3 s to
//!      cancel; a hibernate saves the whole session to disk).
//! Before that, two spoken warnings (15 % and 8 % by default) while an agent is working, so the first thing you hear is
//! not the countdown.
//!
//! "Stop the agent here": there is no message channel into a running agent. The one mechanism that is documented is the
//! hook every Claude Code tool call already goes through: while a stop order is active the hook answers the next tool
//! call with `continue:false` plus a `deny` whose reason tells the agent to stop and why (`stop_reply_for`). Other
//! agents are only warned aloud: a hook that can stop them is not verified (docs/research/...). Nothing is killed.

use crate::power_cmd::PowerKind;
use std::sync::Mutex;

pub const SETTING_MODE: &str = "batteryGuard";
pub const SETTING_PERCENT: &str = "batteryGuardPercent";
pub const SETTING_ACTION: &str = "batteryGuardAction";
pub const SETTING_STOP_AGENTS: &str = "batteryGuardStopAgents";

pub const MIN_PERCENT: u8 = 3;
pub const MAX_PERCENT: u8 = 20;
pub const DEFAULT_PERCENT: u8 = 5;
/// The announced, cancellable wait before the power call.
pub const COUNTDOWN_MS: u64 = 30_000;
/// After a cancel or a power call the guard stays quiet until the level is this far above the trigger (or a charger).
pub const REARM_MARGIN: u8 = 5;
/// How long an agent is only told to stop before `continue:false` ends its loop outright.
pub const HARD_STOP_AFTER_MS: u64 = 15_000;
/// A charger must stay connected this long before a cancelled or spent guard re-arms (one flickering reading must not).
pub const AC_STEADY_MS: u64 = 60_000;
/// A stop order never outlives this, whatever happens to the loop.
pub const STOP_ORDER_MAX_MS: u64 = 5 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Off,
    Dry,
    On,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Dry => "dry",
            Mode::On => "on",
        }
    }
    /// Anything unknown is `Off`: a typo in settings must never arm a shutdown.
    pub fn from_setting(s: &str) -> Mode {
        match s.trim().to_lowercase().as_str() {
            "on" => Mode::On,
            "dry" => Mode::Dry,
            _ => Mode::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Shutdown,
    Hibernate,
}

impl End {
    pub fn as_str(self) -> &'static str {
        match self {
            End::Shutdown => "shutdown",
            End::Hibernate => "hibernate",
        }
    }
    pub fn from_setting(s: &str) -> End {
        if s.trim().eq_ignore_ascii_case("hibernate") { End::Hibernate } else { End::Shutdown }
    }
    /// Hibernate needs hibernation to be switched on in Windows. Without it the guard shuts down instead (and says so
    /// when it is armed), because a hibernate that silently does nothing would leave the laptop to die.
    pub fn effective(self, hibernate_ok: bool) -> End {
        if self == End::Hibernate && !hibernate_ok { End::Shutdown } else { self }
    }
    fn phrase(self) -> &'static str {
        match self {
            End::Shutdown => "shut the laptop down",
            End::Hibernate => "hibernate the laptop",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub mode: Mode,
    pub critical: u8,
    pub end: End,
    pub stop_agents: bool,
}

impl Config {
    pub fn new(mode: Mode, percent: u32, end: End, stop_agents: bool) -> Config {
        Config { mode, critical: clamp_percent(percent), end, stop_agents }
    }
    /// The second warning, a little above the trigger.
    pub fn warn_low(&self) -> u8 {
        self.critical + 3
    }
    /// The first warning.
    pub fn warn_high(&self) -> u8 {
        (self.critical + 7).max(15)
    }
}

pub fn clamp_percent(p: u32) -> u8 {
    p.clamp(MIN_PERCENT as u32, MAX_PERCENT as u32) as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reading {
    pub percent: u8,
    /// A charger is connected (or Windows cannot say: then the guard never acts).
    pub on_ac: bool,
}

/// Windows' `SYSTEM_POWER_STATUS` fields to a reading. No battery (a desktop) or an unknown level gives `None`; an
/// unknown charger state counts as plugged in, because a wrong "on battery" would power off a laptop that is not dying.
pub fn reading_from_raw(ac_line: u8, battery_flag: u8, percent: u8) -> Option<Reading> {
    if battery_flag == 128 || percent > 100 {
        return None;
    }
    Some(Reading { percent, on_ac: ac_line != 0 })
}

#[cfg(target_os = "windows")]
pub fn read_battery() -> Option<Reading> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS { ACLineStatus: 255, BatteryFlag: 255, BatteryLifePercent: 255, SystemStatusFlag: 0, BatteryLifeTime: 0, BatteryFullLifeTime: 0 };
    // SAFETY: a plain out-pointer to a local struct.
    let ok = unsafe { GetSystemPowerStatus(&mut s) };
    if ok.0 == 0 {
        return None;
    }
    reading_from_raw(s.ACLineStatus, s.BatteryFlag, s.BatteryLifePercent)
}

#[cfg(not(target_os = "windows"))]
pub fn read_battery() -> Option<Reading> {
    None
}

/// Can this machine hibernate right now (S4 supported and a hibernation file present)?
#[cfg(target_os = "windows")]
pub fn hibernate_available() -> bool {
    use windows::Win32::System::Power::{GetPwrCapabilities, SYSTEM_POWER_CAPABILITIES};
    // SAFETY: SYSTEM_POWER_CAPABILITIES is plain data (zeroed is a valid value) and the pointer is to a local.
    let mut caps: SYSTEM_POWER_CAPABILITIES = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetPwrCapabilities(&mut caps) };
    ok.0 != 0 && caps.SystemS4.0 != 0 && caps.HiberFilePresent.0 != 0
}

#[cfg(not(target_os = "windows"))]
pub fn hibernate_available() -> bool {
    false
}

// ── the stop order the agents' hooks consult ─────────────────────────────────

#[derive(Debug, Clone)]
struct StopOrder {
    reason: String,
    since_ms: u64,
}

static STOP_ORDER: Mutex<Option<StopOrder>> = Mutex::new(None);

pub fn set_stop_order(reason: &str, now_ms: u64) {
    if let Ok(mut g) = STOP_ORDER.lock() {
        *g = Some(StopOrder { reason: reason.chars().take(300).collect(), since_ms: now_ms });
    }
}

pub fn clear_stop_order() {
    if let Ok(mut g) = STOP_ORDER.lock() {
        *g = None;
    }
}

pub fn stop_order_active(now_ms: u64) -> bool {
    STOP_ORDER.lock().ok().and_then(|g| g.clone()).map_or(false, |o| now_ms.saturating_sub(o.since_ms) < STOP_ORDER_MAX_MS)
}

/// What a Claude Code `PreToolUse` hook must print while a stop order is active, or `None` (print nothing special).
/// A `deny` refuses the tool call and hands the agent the reason (so it can say where it stopped). Per Claude Code's
/// hook documentation `continue:false` ends the agent loop and wins over any decision, and its reason is shown to the
/// user, not to the agent; so it is added only after HARD_STOP_AFTER_MS, for an agent that keeps trying.
pub fn stop_reply_for(agent: &str, now_ms: u64) -> Option<String> {
    stop_reply_with(agent, now_ms, STOP_ORDER.lock().ok().and_then(|g| g.clone()))
}

/// First the agent is only TOLD (a `deny` with the reason: it can answer with a summary of where it stopped); if it is
/// still making tool calls HARD_STOP_AFTER_MS later, `continue:false` ends its loop as well.
fn stop_reply_with(agent: &str, now_ms: u64, order: Option<StopOrder>) -> Option<String> {
    if !matches!(agent, "claude" | "devin") {
        return None;
    }
    let order = order?;
    let age = now_ms.saturating_sub(order.since_ms);
    if age >= STOP_ORDER_MAX_MS {
        return None;
    }
    let for_user = format!("NEXUS: {}", order.reason);
    let for_agent = format!(
        "NEXUS: {} Stop now. Do not start any more tool calls. In one short message say what you finished and what is left.",
        order.reason
    );
    let mut reply = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": for_agent
        }
    });
    if age >= HARD_STOP_AFTER_MS {
        reply["continue"] = serde_json::json!(false);
        reply["stopReason"] = serde_json::json!(for_user);
    }
    Some(reply.to_string())
}

// ── the decisions ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Say(String),
    /// Start the stop order (the agents' next tool call is refused) with this reason.
    BeginStop(String),
    /// Open the cancel window the spoken "cancel" listens to.
    BeginCountdown,
    /// The countdown ran out with nothing cancelling it: do it.
    Fire(End),
    ClearStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Idle,
    /// The end announced when the countdown began is the end that happens, whatever the settings say later.
    Counting { since_ms: u64, end: End },
    /// Fired, cancelled or practised: quiet until the battery is charged again.
    Spent,
}

#[derive(Debug, Clone)]
pub struct Obs {
    pub now_ms: u64,
    pub reading: Option<Reading>,
    /// Labels of the coding agents working right now ("Claude Code").
    pub working: Vec<String>,
    /// The spoken "cancel" was heard since the countdown began.
    pub cancelled: bool,
}

#[derive(Debug)]
pub struct Engine {
    stage: Stage,
    said_high: bool,
    said_low: bool,
    /// Since when the charger has been connected without a break.
    ac_since_ms: Option<u64>,
}

impl Default for Engine {
    fn default() -> Self {
        Engine { stage: Stage::Idle, said_high: false, said_low: false, ac_since_ms: None }
    }
}

pub fn agents_phrase(working: &[String]) -> String {
    match working {
        [] => "no coding agent".to_string(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl Engine {
    pub fn counting(&self) -> bool {
        matches!(self.stage, Stage::Counting { .. })
    }

    pub fn step(&mut self, cfg: &Config, o: &Obs) -> Vec<Action> {
        let mut out = Vec::new();
        // Anything but "on" ends a countdown that is under way (off, or switched to practice).
        if cfg.mode != Mode::On && self.counting() {
            out.push(Action::ClearStop);
            out.push(Action::Say(format!(
                "The battery guard is now {}, sir, so I stopped the countdown. Nothing will be switched off.",
                if cfg.mode == Mode::Off { "off" } else { "in practice mode" }
            )));
            self.stage = Stage::Idle;
        }
        if cfg.mode == Mode::Off {
            *self = Engine::default();
            return out;
        }
        let Some(r) = o.reading else {
            // nothing readable: change nothing, do nothing
            return out;
        };
        self.ac_since_ms = if r.on_ac { Some(self.ac_since_ms.unwrap_or(o.now_ms)) } else { None };
        let steady_ac = self.ac_since_ms.map_or(false, |t| o.now_ms.saturating_sub(t) >= AC_STEADY_MS);
        let recovered = r.percent >= cfg.critical.saturating_add(REARM_MARGIN);
        // The flags that keep each warning to once re-arm on a steady charger or a level well above both.
        if steady_ac || r.percent >= cfg.warn_high().saturating_add(3) {
            self.said_high = false;
            self.said_low = false;
        }
        match self.stage {
            Stage::Counting { since_ms, end } => {
                // A charger, even for one reading, or a recovered level: stand down at once (the safe direction).
                if r.on_ac || recovered {
                    out.push(Action::ClearStop);
                    out.push(Action::Say(if r.on_ac {
                        "The charger is connected, sir. I have stood down; nothing will be switched off.".to_string()
                    } else {
                        format!("The battery is back to {} percent, sir. I have stood down.", r.percent)
                    }));
                    self.stage = Stage::Idle;
                } else if o.cancelled {
                    out.push(Action::ClearStop);
                    out.push(Action::Say(format!(
                        "Cancelled, sir. The battery is at {} percent. I will not try again until it has been charged.",
                        r.percent
                    )));
                    self.stage = Stage::Spent;
                } else if o.now_ms.saturating_sub(since_ms) >= COUNTDOWN_MS {
                    out.push(Action::Fire(end));
                    self.stage = Stage::Spent;
                }
            }
            Stage::Spent => {
                // Back to normal only after a STEADY charger or a recovered level, never one flickering reading.
                if steady_ac || recovered {
                    self.stage = Stage::Idle;
                }
            }
            Stage::Idle => {
                if r.on_ac {
                    return out; // a charger: never warn, never act
                }
                if r.percent <= cfg.critical {
                    let who = agents_phrase(&o.working);
                    match cfg.mode {
                        Mode::Dry => {
                            out.push(Action::Say(format!(
                                "Practice, sir: the battery is at {} percent, so I would ask {} to stop and then {} in 30 seconds. I did nothing.",
                                r.percent, who, cfg.end.phrase()
                            )));
                            self.stage = Stage::Spent;
                        }
                        Mode::On => {
                            let stopping = cfg.stop_agents && !o.working.is_empty();
                            let mut say = format!("The battery is at {} percent, sir. ", r.percent);
                            if stopping {
                                say.push_str(&format!("I am asking {who} to stop at its next step. "));
                            }
                            say.push_str(&format!("I will {} in 30 seconds. Say cancel to stop me.", cfg.end.phrase()));
                            out.push(Action::Say(say));
                            if stopping {
                                out.push(Action::BeginStop(format!("The battery is at {} percent and the laptop is about to power off.", r.percent)));
                            }
                            out.push(Action::BeginCountdown);
                            self.stage = Stage::Counting { since_ms: o.now_ms, end: cfg.end };
                        }
                        Mode::Off => {}
                    }
                } else if r.percent <= cfg.warn_low() && !self.said_low {
                    self.said_low = true;
                    self.said_high = true;
                    out.push(Action::Say(format!(
                        "The battery is down to {} percent, sir. At {} I will {}{}.",
                        r.percent,
                        cfg.critical,
                        cfg.end.phrase(),
                        if cfg.stop_agents && !o.working.is_empty() { format!(" after asking {} to stop", agents_phrase(&o.working)) } else { String::new() }
                    )));
                } else if r.percent <= cfg.warn_high() && !self.said_high && !o.working.is_empty() {
                    self.said_high = true;
                    out.push(Action::Say(format!(
                        "The battery is at {} percent and {} is still working, sir. Plug in the charger, or it will be stopped at {}.",
                        r.percent,
                        agents_phrase(&o.working),
                        cfg.critical
                    )));
                }
            }
        }
        out
    }
}

// ── spoken commands ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// "when the battery is under 8 percent stop claude and hibernate"
    Arm { percent: u8, end: End, stop_agents: bool },
    SetMode(Mode),
    Status,
}

impl Command {
    /// The wire form carried by `ParsedIntent::BatteryGuardCommand`.
    pub fn as_str(&self) -> String {
        match self {
            Command::Arm { percent, end, stop_agents } => format!("arm:{percent}:{}:{}", end.as_str(), if *stop_agents { "stop" } else { "nostop" }),
            Command::SetMode(m) => format!("mode:{}", m.as_str()),
            Command::Status => "status".to_string(),
        }
    }
    pub fn from_str(s: &str) -> Option<Command> {
        let parts: Vec<&str> = s.split(':').collect();
        match parts.as_slice() {
            ["arm", p, e, st] => Some(Command::Arm { percent: clamp_percent(p.parse().ok()?), end: End::from_setting(e), stop_agents: *st == "stop" }),
            ["mode", m] => Some(Command::SetMode(Mode::from_setting(m))),
            ["status"] => Some(Command::Status),
            _ => None,
        }
    }
}

const NUMBER_WORDS: &[(&str, u32)] = &[
    ("three", 3), ("four", 4), ("five", 5), ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9), ("ten", 10),
    ("eleven", 11), ("twelve", 12), ("thirteen", 13), ("fourteen", 14), ("fifteen", 15), ("sixteen", 16),
    ("seventeen", 17), ("eighteen", 18), ("nineteen", 19), ("twenty", 20),
];

fn number_of(token: &str) -> Option<u32> {
    token.parse::<u32>().ok().or_else(|| NUMBER_WORDS.iter().find(|(w, _)| *w == token).map(|(_, n)| *n))
}

fn tokens_of(text: &str) -> Vec<String> {
    let t = text
        .to_lowercase()
        .replace('%', " percent ")
        .replace("per cent", "percent")
        .replace("shut down", "shutdown")
        .replace("power off", "shutdown")
        .replace("powered off", "shutdown")
        .replace("turn off the laptop", "shutdown")
        .replace("switch off", "shutdown")
        .replace("hibernation", "hibernate");
    t.split(|c: char| !(c.is_alphanumeric() || c == '\'')).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// The level said: the number right before "percent", or (no "percent" said) the number right after "below", "under",
/// "than", "reaches" or "hits". Digits or words.
fn percent_in(tokens: &[String]) -> Option<u32> {
    if let Some(i) = tokens.iter().position(|t| t == "percent" || t == "percentage") {
        return number_of(tokens.get(i.checked_sub(1)?)?);
    }
    let i = tokens.iter().position(|t| matches!(t.as_str(), "below" | "under" | "than" | "reaches" | "hits"))?;
    number_of(tokens.get(i + 1)?)
}

const QUESTION_STARTS: &[&str] = &["how", "what", "why", "which", "who", "where", "is", "are", "does", "do", "did", "should", "would"];
/// Spoken into a document or a chat, never a command about the battery.
const DICTATION_STARTS: &[&str] = &["type", "write", "dictate", "say", "tell", "remind", "message", "text", "email", "send"];

/// "when the battery is below 5 percent stop claude and shut down the laptop", "allow battery shutdown", "practice
/// battery shutdown", "turn off the battery guard", "battery guard status". Strict: the sentence must be about the
/// battery AND say what to do, so "what is my battery percentage" and "the battery is at 5 percent" are never claimed.
pub fn parse_command(text: &str) -> Option<Command> {
    let tokens = tokens_of(text);
    if tokens.len() < 3 || tokens.len() > 40 {
        return None;
    }
    let has = |w: &str| tokens.iter().any(|t| t == w);
    let has_any = |ws: &[&str]| ws.iter().any(|w| has(w));
    let first = tokens.first().map(String::as_str).unwrap_or("");
    let first = if first == "nexus" { tokens.get(1).map(String::as_str).unwrap_or("") } else { first };
    // "don't allow battery shutdown", "never shut down when the battery is low": a negation arms nothing
    if has_any(&["don't", "dont", "not", "never", "no", "without", "n't"]) {
        return None;
    }
    if DICTATION_STARTS.contains(&first) && !tokens.iter().take(3).any(|t| t == "claude" || t == "codex") && first != "tell" {
        return None;
    }
    if QUESTION_STARTS.contains(&first) && !has_any(&["allow", "practice", "enable", "disable"]) {
        // "is the battery guard on?" is the one question this answers
        return if has("battery") && has_any(&["guard", "shutdown", "protection"]) && has_any(&["on", "set", "armed", "status", "enabled"]) { Some(Command::Status) } else { None };
    }
    let about_battery = has("battery") || (has("charge") && has_any(&["drops", "falls", "below", "under", "reaches", "hits"]));
    if !about_battery {
        return None;
    }
    // modes
    if has_any(&["guard", "shutdown", "protection"]) && !has_any(&["when", "if", "once", "whenever", "below", "under", "drops", "falls", "reaches", "hits"]) {
        if has("status") {
            return Some(Command::Status);
        }
        if has_any(&["allow", "enable", "activate"]) {
            return Some(Command::SetMode(Mode::On));
        }
        if has_any(&["practice", "rehearse", "simulate"]) {
            return Some(Command::SetMode(Mode::Dry));
        }
        if has_any(&["disable", "cancel", "stop", "off", "disarm"]) {
            return Some(Command::SetMode(Mode::Off));
        }
    }
    // arming: a trigger level, a comparator, and what to do
    let percent = percent_in(&tokens)?;
    if !(MIN_PERCENT as u32..=MAX_PERCENT as u32).contains(&percent) {
        return None;
    }
    // a condition word is required: "my battery is at 5 percent, shut down the laptop" states a fact, it sets nothing
    let comparator = has_any(&["when", "if", "once", "whenever", "below", "under", "drops", "falls", "reaches", "hits", "less", "lower"]);
    let end_word = has("shutdown") || has("hibernate");
    if !comparator || !end_word {
        return None;
    }
    let stop_agents = has("stop") || has("tell") || has("ask") || has("save") || has_any(&["agent", "agents", "claude", "codex", "cursor", "kiro", "antigravity", "opencode", "gemini"]);
    Some(Command::Arm { percent: percent as u8, end: if has("hibernate") { End::Hibernate } else { End::Shutdown }, stop_agents })
}

/// What NEXUS says back to a command. Pure.
pub fn describe(cfg: &Config) -> String {
    let stop = if cfg.stop_agents { "I ask coding agents to stop first, then " } else { "" };
    match cfg.mode {
        Mode::Off => "The battery guard is off, sir. Say allow battery shutdown to switch it on, or practice battery shutdown to rehearse.".to_string(),
        Mode::Dry => format!(
            "The battery guard is in practice mode, sir. At {} percent on battery I would say what I would do: {stop}{}. Nothing is switched off. Say allow battery shutdown to make it real.",
            cfg.critical, cfg.end.phrase()
        ),
        Mode::On => format!(
            "The battery guard is on, sir. At {} percent on battery, with no charger, {stop}I will {} after a 30 second countdown you can cancel by saying cancel.",
            cfg.critical, cfg.end.phrase()
        ),
    }
}

// ── the loop ─────────────────────────────────────────────────────────────────

use tauri::{AppHandle, Manager, Runtime};

fn settings_json<R: Runtime>(app: &AppHandle<R>) -> Option<serde_json::Value> {
    let dir = app.path().app_data_dir().ok()?;
    serde_json::from_str(&std::fs::read_to_string(dir.join("settings.json")).ok()?).ok()
}

pub fn read_config<R: Runtime>(app: &AppHandle<R>) -> Config {
    let j = settings_json(app).unwrap_or_default();
    let mode = j.get(SETTING_MODE).and_then(|v| v.as_str()).map(Mode::from_setting).unwrap_or(Mode::Off);
    let percent = j.get(SETTING_PERCENT).and_then(|v| v.as_u64()).unwrap_or(DEFAULT_PERCENT as u64) as u32;
    let end = j.get(SETTING_ACTION).and_then(|v| v.as_str()).map(End::from_setting).unwrap_or(End::Shutdown);
    let stop = j.get(SETTING_STOP_AGENTS).and_then(|v| v.as_bool()).unwrap_or(true);
    Config::new(mode, percent, end, stop)
}

fn working_agents() -> Vec<String> {
    crate::agent_monitor::snapshot_now()
        .sessions
        .into_iter()
        .filter(|s| s.state.running() && s.state != crate::agent_monitor::State::Waiting)
        .map(|s| s.label)
        .collect()
}

static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// True from the power call until it is over: the stop order must stay while a power-off is really under way.
static FIRING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Start the loop once (a second call does nothing). If the loop ever panics it is started again.
pub fn spawn<R: Runtime>(app: &AppHandle<R>) {
    if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let a = app.clone();
            let run = tauri::async_runtime::spawn(async move { run_loop(a).await });
            match run.await {
                Ok(()) => break,
                Err(e) => {
                    tracing::error!("battery guard loop stopped ({e}); starting it again");
                    clear_stop_order();
                    FIRING.store(false, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            }
        }
    });
}

async fn say<R: Runtime>(app: &AppHandle<R>, now: u64, text: String) {
    println!("[BATTERY] say: {text}");
    crate::proactive_policy::submit(app, format!("battery_guard_{now}"), text, crate::google::types::AlertUrgency::High);
}

async fn run_loop<R: Runtime>(app: AppHandle<R>) {
    use std::sync::atomic::Ordering;
    let mut engine = Engine::default();
    let mut last_mode = Mode::Off;
    // (generation of the cancel window, how many cancels had been heard when it opened)
    let mut countdown: Option<(u64, u64)> = None;
    loop {
        let mut cfg = read_config(&app);
        if cfg.mode != last_mode {
            // practising and then allowing, or any other change, starts the guard fresh
            engine = Engine::default();
            last_mode = cfg.mode;
        }
        cfg.end = cfg.end.effective(hibernate_available());
        let reading = read_battery();
        let low = reading.map_or(false, |r| !r.on_ac && r.percent <= cfg.warn_high().saturating_add(5));
        let now = crate::agent_hooks::now_ms();
        let cancelled = countdown.map_or(false, |(_, at)| crate::power_cmd::cancel_count() > at);
        let obs = Obs { now_ms: now, reading, working: if cfg.mode == Mode::Off { Vec::new() } else { working_agents() }, cancelled };
        for a in engine.step(&cfg, &obs) {
            match a {
                Action::Say(text) => say(&app, now, text).await,
                Action::BeginStop(reason) => {
                    println!("[BATTERY] stop order: {reason}");
                    set_stop_order(&reason, now);
                }
                Action::BeginCountdown => {
                    countdown = Some((crate::power_cmd::begin(), crate::power_cmd::cancel_count()));
                    println!("[BATTERY] countdown started: {} in 30 s", cfg.end.as_str());
                }
                Action::ClearStop => {
                    clear_stop_order();
                    if let Some((g, _)) = countdown.take() {
                        crate::power_cmd::finish(g);
                    }
                }
                Action::Fire(end) => {
                    let heard_before = countdown.map_or(0, |(_, at)| at);
                    if let Some((g, _)) = countdown.take() {
                        crate::power_cmd::finish(g);
                    }
                    // last look: the setting must still say yes
                    if read_config(&app).mode != Mode::On {
                        println!("[BATTERY] last check failed: the guard was switched off, not powering off");
                        clear_stop_order();
                        continue;
                    }
                    FIRING.store(true, Ordering::SeqCst);
                    println!("[BATTERY] countdown finished: {}", end.as_str());
                    let mut done;
                    if end == End::Shutdown {
                        // the graceful sequence (goodbye, close apps, a last 3 s to cancel) can stop short: an editor or
                        // a terminal asks before it closes. A dying battery cannot wait for a click.
                        let line = crate::orchestrator::start_power(&app, PowerKind::Shutdown).await;
                        crate::orchestrator::speak_unprompted(&app, line);
                        tokio::time::sleep(std::time::Duration::from_secs(45)).await;
                        if crate::power_cmd::cancel_count() > heard_before {
                            println!("[BATTERY] cancelled during the shutdown sequence");
                            clear_stop_order();
                            FIRING.store(false, Ordering::SeqCst);
                            continue;
                        }
                        // still running: something refused to close
                        if hibernate_available() {
                            say(&app, now, "Something would not close, sir, so I am hibernating instead. Everything open is saved.".into()).await;
                            done = crate::power_cmd::execute_now(PowerKind::Hibernate).is_ok();
                        } else {
                            say(&app, now, "Something would not close, sir, so I am shutting down now. Anything unsaved may be lost.".into()).await;
                            done = crate::power_cmd::execute_now(PowerKind::Shutdown).is_ok();
                        }
                    } else {
                        done = crate::power_cmd::execute_now(PowerKind::Hibernate).is_ok();
                        if !done {
                            say(&app, now, "I could not hibernate, sir, so I am shutting down.".into()).await;
                            done = crate::power_cmd::execute_now(PowerKind::Shutdown).is_ok();
                        }
                    }
                    if !done {
                        say(&app, now, "I could not power the laptop off, sir. Please plug in the charger or save your work.".into()).await;
                    }
                    // still here: whatever was meant to happen did not; agents may work again
                    clear_stop_order();
                    FIRING.store(false, Ordering::SeqCst);
                }
            }
        }
        // a stop order with no countdown and no power call under way is stale: never leave one behind
        if !engine.counting() && !FIRING.load(Ordering::SeqCst) && stop_order_active(now) {
            clear_stop_order();
        }
        let wait = if engine.counting() { 2 } else if low { 5 } else { 20 };
        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: Mode) -> Config {
        Config::new(mode, 5, End::Shutdown, true)
    }
    fn obs(now_ms: u64, percent: u8, on_ac: bool, working: &[&str]) -> Obs {
        Obs { now_ms, reading: Some(Reading { percent, on_ac }), working: working.iter().map(|s| s.to_string()).collect(), cancelled: false }
    }
    fn says(a: &[Action]) -> Vec<String> {
        a.iter().filter_map(|x| if let Action::Say(s) = x { Some(s.clone()) } else { None }).collect()
    }

    // ── reading the battery ──

    #[test]
    fn a_desktop_an_unknown_level_and_an_unknown_charger_never_cause_action() {
        assert_eq!(reading_from_raw(0, 128, 100), None, "no battery at all");
        assert_eq!(reading_from_raw(0, 0, 255), None, "level unknown");
        assert_eq!(reading_from_raw(1, 8, 4), Some(Reading { percent: 4, on_ac: true }));
        assert_eq!(reading_from_raw(0, 4, 4), Some(Reading { percent: 4, on_ac: false }), "on battery, critical flag");
        assert!(reading_from_raw(255, 4, 4).unwrap().on_ac, "charger state unknown counts as plugged in");
    }

    // ── the engine ──

    #[test]
    fn off_does_nothing_at_any_level() {
        let mut e = Engine::default();
        for p in [100, 50, 15, 8, 5, 2] {
            assert!(e.step(&cfg(Mode::Off), &obs(1_000, p, false, &["Claude Code"])).is_empty(), "{p}");
        }
    }

    #[test]
    fn a_charger_or_an_unreadable_level_never_acts() {
        let mut e = Engine::default();
        assert!(e.step(&cfg(Mode::On), &obs(1_000, 2, true, &["Claude Code"])).is_empty(), "plugged in at 2 percent");
        let mut o = obs(1_000, 2, false, &[]);
        o.reading = None;
        assert!(e.step(&cfg(Mode::On), &o).is_empty(), "no reading");
        assert!(!e.counting());
    }

    #[test]
    fn two_warnings_while_an_agent_works_each_said_once() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        assert!(e.step(&c, &obs(0, 40, false, &["Claude Code"])).is_empty());
        let a = e.step(&c, &obs(1_000, 15, false, &["Claude Code"]));
        assert_eq!(says(&a).len(), 1, "{a:?}");
        assert!(says(&a)[0].contains("15 percent") && says(&a)[0].contains("Claude Code"));
        assert!(e.step(&c, &obs(2_000, 14, false, &["Claude Code"])).is_empty(), "said once");
        let b = e.step(&c, &obs(3_000, 8, false, &["Claude Code"]));
        assert_eq!(says(&b).len(), 1);
        assert!(says(&b)[0].contains("8 percent") && says(&b)[0].contains("At 5"), "{b:?}");
        assert!(e.step(&c, &obs(4_000, 7, false, &["Claude Code"])).is_empty());
        assert!(!b.iter().any(|x| matches!(x, Action::BeginStop(_) | Action::BeginCountdown | Action::Fire(_))), "a warning never stops or powers off");
    }

    #[test]
    fn the_first_warning_is_for_a_working_agent_only() {
        let mut e = Engine::default();
        assert!(e.step(&cfg(Mode::On), &obs(0, 15, false, &[])).is_empty(), "no agent working: Windows' own warning is enough");
    }

    #[test]
    fn at_the_trigger_it_announces_asks_the_agent_to_stop_and_counts_down() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        let a = e.step(&c, &obs(10_000, 5, false, &["Claude Code"]));
        let text = says(&a).join(" ");
        assert!(text.contains("5 percent") && text.contains("Claude Code") && text.contains("30 seconds") && text.contains("cancel"), "{text}");
        assert!(a.iter().any(|x| matches!(x, Action::BeginStop(r) if r.contains("5 percent"))));
        assert!(a.contains(&Action::BeginCountdown));
        assert!(!a.iter().any(|x| matches!(x, Action::Fire(_))), "never at once");
        assert!(e.counting());
        // 29 s later: still waiting
        assert!(e.step(&c, &obs(39_000, 4, false, &["Claude Code"])).is_empty());
        // 30 s: the power call
        let b = e.step(&c, &obs(40_000, 4, false, &[]));
        assert_eq!(b, vec![Action::Fire(End::Shutdown)]);
        assert!(!e.counting());
        // and only once
        assert!(e.step(&c, &obs(50_000, 3, false, &[])).is_empty());
    }

    #[test]
    fn with_no_agent_working_there_is_no_stop_order_but_the_countdown_still_runs() {
        let mut e = Engine::default();
        let a = e.step(&cfg(Mode::On), &obs(0, 5, false, &[]));
        assert!(!a.iter().any(|x| matches!(x, Action::BeginStop(_))));
        assert!(a.contains(&Action::BeginCountdown));
        assert!(!says(&a).join(" ").contains("asking"));
    }

    #[test]
    fn the_stop_setting_off_means_no_stop_order_even_with_agents_working() {
        let mut e = Engine::default();
        let c = Config::new(Mode::On, 5, End::Shutdown, false);
        let a = e.step(&c, &obs(0, 5, false, &["Claude Code"]));
        assert!(!a.iter().any(|x| matches!(x, Action::BeginStop(_))));
    }

    #[test]
    fn saying_cancel_stops_everything_and_it_does_not_try_again_until_charged() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        e.step(&c, &obs(0, 5, false, &["Claude Code"]));
        let mut o = obs(5_000, 5, false, &["Claude Code"]);
        o.cancelled = true;
        let a = e.step(&c, &o);
        assert!(a.contains(&Action::ClearStop));
        assert!(says(&a).join(" ").contains("Cancelled"));
        assert!(!a.iter().any(|x| matches!(x, Action::Fire(_))));
        // still on battery and critical: quiet, no new countdown
        assert!(e.step(&c, &obs(60_000, 4, false, &[])).is_empty());
        assert!(e.step(&c, &obs(61_000, 3, false, &[])).is_empty());
        // charged and drained again: it can act again
        assert!(e.step(&c, &obs(100_000, 30, false, &[])).is_empty());
        let again = e.step(&c, &obs(200_000, 5, false, &[]));
        assert!(again.contains(&Action::BeginCountdown));
    }

    #[test]
    fn a_charger_during_the_countdown_stands_it_down() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        e.step(&c, &obs(0, 5, false, &["Claude Code"]));
        let a = e.step(&c, &obs(10_000, 5, true, &["Claude Code"]));
        assert!(a.contains(&Action::ClearStop));
        assert!(says(&a).join(" ").contains("charger"));
        assert!(!e.counting());
        assert!(!a.iter().any(|x| matches!(x, Action::Fire(_))));
    }

    #[test]
    fn a_recovering_level_during_the_countdown_stands_it_down() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        e.step(&c, &obs(0, 5, false, &[]));
        let a = e.step(&c, &obs(10_000, 11, false, &[]));
        assert!(a.contains(&Action::ClearStop) && !e.counting());
    }

    #[test]
    fn switching_the_guard_off_during_the_countdown_stops_it() {
        let mut e = Engine::default();
        e.step(&cfg(Mode::On), &obs(0, 5, false, &["Claude Code"]));
        let a = e.step(&cfg(Mode::Off), &obs(10_000, 5, false, &["Claude Code"]));
        assert!(a.contains(&Action::ClearStop));
        assert!(!e.counting());
    }

    #[test]
    fn practice_mode_speaks_but_never_stops_or_powers_off() {
        let mut e = Engine::default();
        let a = e.step(&cfg(Mode::Dry), &obs(0, 5, false, &["Claude Code"]));
        assert_eq!(a.len(), 1, "{a:?}");
        assert!(says(&a)[0].contains("Practice") && says(&a)[0].contains("did nothing"));
        assert!(e.step(&cfg(Mode::Dry), &obs(100_000, 3, false, &["Claude Code"])).is_empty(), "once");
    }

    #[test]
    fn the_trigger_and_end_are_the_configured_ones() {
        let mut e = Engine::default();
        let c = Config::new(Mode::On, 8, End::Hibernate, true);
        assert!(e.step(&c, &obs(0, 9, false, &["Claude Code"])).iter().all(|x| !matches!(x, Action::BeginCountdown)), "9 percent is above an 8 percent trigger");
        let a = e.step(&c, &obs(1_000, 8, false, &["Claude Code"]));
        assert!(says(&a).join(" ").contains("hibernate"));
        assert_eq!(e.step(&c, &obs(32_000, 7, false, &[])), vec![Action::Fire(End::Hibernate)]);
    }

    #[test]
    fn the_percent_is_kept_inside_its_limits() {
        assert_eq!(clamp_percent(0), 3);
        assert_eq!(clamp_percent(1), 3);
        assert_eq!(clamp_percent(5), 5);
        assert_eq!(clamp_percent(90), 20);
        assert_eq!(Config::new(Mode::On, 99, End::Shutdown, true).critical, 20, "a hand-edited 99 cannot make it fire at 99 percent");
        assert_eq!(Mode::from_setting("ON"), Mode::On);
        assert_eq!(Mode::from_setting("yes please"), Mode::Off, "a typo never arms it");
    }

    // ── the stop order ──

    #[test]
    fn the_stop_reply_stops_claude_gives_it_the_reason_and_expires() {
        clear_stop_order();
        assert_eq!(stop_reply_for("claude", 1_000), None, "no order: nothing special is printed");
        set_stop_order("The battery is at 5 percent and the laptop is about to power off.", 1_000);
        // first the agent is only TOLD (a deny with the reason, so it can say where it stopped)
        let r = stop_reply_for("claude", 2_000).expect("an order is active");
        let v: serde_json::Value = serde_json::from_str(&r).expect("valid JSON");
        assert!(v.get("continue").is_none(), "not yet a hard stop: the deny reason must reach the agent: {r}");
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(v["hookSpecificOutput"]["permissionDecision"], "deny");
        assert!(v["hookSpecificOutput"]["permissionDecisionReason"].as_str().unwrap().contains("Stop now"));
        // an agent that keeps calling tools 15 s later is stopped outright as well
        let hard = stop_reply_for("claude", 1_000 + HARD_STOP_AFTER_MS).expect("still active");
        let h: serde_json::Value = serde_json::from_str(&hard).unwrap();
        assert_eq!(h["continue"], false);
        assert!(h["stopReason"].as_str().unwrap().contains("5 percent"));
        assert_eq!(h["hookSpecificOutput"]["permissionDecision"], "deny", "the deny stays");
        assert!(stop_reply_for("devin", 2_000).is_some(), "Devin shares Claude's hook");
        assert_eq!(stop_reply_for("cursor", 2_000), None, "no verified way to stop the others");
        assert_eq!(stop_reply_for("claude", 1_000 + STOP_ORDER_MAX_MS), None, "an order never outlives five minutes");
        clear_stop_order();
        assert_eq!(stop_reply_for("claude", 2_000), None);
    }

    // ── what is said ──

    #[test]
    fn agents_are_named_in_a_spoken_list() {
        assert_eq!(agents_phrase(&[]), "no coding agent");
        assert_eq!(agents_phrase(&["Claude Code".into()]), "Claude Code");
        assert_eq!(agents_phrase(&["Claude Code".into(), "Codex".into()]), "Claude Code and Codex");
        assert_eq!(agents_phrase(&["A".into(), "B".into(), "C".into()]), "A, B and C");
    }

    // ── the spoken commands ──

    #[test]
    fn the_sentences_from_the_owner_arm_the_guard() {
        use Command::Arm;
        for (said, percent, end, stop) in [
            ("when the battery is below 5 percent stop claude and shut down the laptop", 5, End::Shutdown, true),
            ("if the charge drops under five percent tell claude to stop here and shut down the laptop", 5, End::Shutdown, true),
            ("nexus when the battery drops below 8% hibernate the laptop", 8, End::Hibernate, false),
            ("when my battery reaches ten percent stop the agents and shut down", 10, End::Shutdown, true),
            ("if the battery falls under 7 percent ask codex to stop and power off the laptop", 7, End::Shutdown, true),
        ] {
            assert_eq!(parse_command(said), Some(Arm { percent, end, stop_agents: stop }), "{said}");
        }
    }

    #[test]
    fn modes_and_status_are_understood() {
        assert_eq!(parse_command("allow battery shutdown"), Some(Command::SetMode(Mode::On)));
        assert_eq!(parse_command("practice battery shutdown"), Some(Command::SetMode(Mode::Dry)));
        assert_eq!(parse_command("turn off the battery guard"), Some(Command::SetMode(Mode::Off)));
        assert_eq!(parse_command("disable battery shutdown"), Some(Command::SetMode(Mode::Off)));
        assert_eq!(parse_command("battery guard status"), Some(Command::Status));
        assert_eq!(parse_command("is the battery guard on"), Some(Command::Status));
    }

    #[test]
    fn ordinary_battery_talk_is_never_claimed() {
        for said in [
            "what is my battery percentage",
            "how much battery is left",
            "the battery is at 5 percent",
            "my battery drops fast",
            "shut down the laptop",
            "when claude finishes shut down the laptop",
            "when the battery is below 5 percent",
            "stop claude and shut down the laptop",
            "what happens if the battery is below 5 percent and i shut down",
            "when the battery is below 50 percent shut down the laptop",
            "when the battery is below 1 percent shut down the laptop",
            "charge my phone and shut down",
        ] {
            assert_eq!(parse_command(said), None, "{said}");
        }
    }

    #[test]
    fn the_wire_form_round_trips() {
        for c in [
            Command::Arm { percent: 5, end: End::Shutdown, stop_agents: true },
            Command::Arm { percent: 12, end: End::Hibernate, stop_agents: false },
            Command::SetMode(Mode::Dry),
            Command::Status,
        ] {
            assert_eq!(Command::from_str(&c.as_str()), Some(c.clone()), "{c:?}");
        }
        assert_eq!(Command::from_str("arm:99:shutdown:stop"), Some(Command::Arm { percent: 20, end: End::Shutdown, stop_agents: true }), "clamped");
        assert_eq!(Command::from_str("rm -rf"), None);
    }

    #[test]
    fn the_description_says_what_will_happen() {
        assert!(describe(&cfg(Mode::Off)).contains("off"));
        let dry = describe(&cfg(Mode::Dry));
        assert!(dry.contains("practice") && dry.contains("Nothing is switched off"), "{dry}");
        let on = describe(&Config::new(Mode::On, 8, End::Hibernate, true));
        assert!(on.contains("8 percent") && on.contains("hibernate") && on.contains("cancel") && on.contains("stop first"), "{on}");
    }

    #[test]
    fn the_loop_uses_the_shared_cancel_and_the_last_look_before_the_power_call() {
        let src = include_str!("battery_guard.rs");
        let prod = src.split("#[cfg(test)]").next().unwrap();
        assert!(prod.contains("crate::power_cmd::begin()") && prod.contains("crate::power_cmd::cancel_count() > at"), "the spoken cancel, told apart from another power command starting");
        assert!(!prod.contains("is_current("), "another power command moving the generation on must never count as a cancel");
        let fire = prod.split("Action::Fire(end) =>").nth(1).unwrap();
        assert!(fire.find("read_config(&app).mode != Mode::On").unwrap() < fire.find("start_power").unwrap(), "a last look at the setting before power");
    }

    // ── the review of 2026-11-24 ──

    #[test]
    fn a_flickering_charger_does_not_undo_a_cancel_but_a_steady_one_does() {
        let mut e = Engine::default();
        let c = cfg(Mode::On);
        e.step(&c, &obs(0, 5, false, &["Claude Code"]));
        let mut o = obs(5_000, 5, false, &["Claude Code"]);
        o.cancelled = true;
        e.step(&c, &o); // cancelled: spent
        // one reading with the charger, then back on battery: still quiet
        e.step(&c, &obs(10_000, 5, true, &[]));
        assert!(e.step(&c, &obs(12_000, 4, false, &[])).is_empty(), "a blip is not a recharge");
        assert!(e.step(&c, &obs(14_000, 4, false, &[])).is_empty());
        // a charger connected for a full minute is
        e.step(&c, &obs(20_000, 4, true, &[]));
        e.step(&c, &obs(20_000 + AC_STEADY_MS, 4, true, &[]));
        let again = e.step(&c, &obs(30_000 + AC_STEADY_MS, 5, false, &["Claude Code"]));
        assert!(again.contains(&Action::BeginCountdown), "{again:?}");
    }

    #[test]
    fn practising_does_not_block_the_real_thing_once_the_mode_changes() {
        // the loop starts a fresh engine on a mode change; the engine itself must act at once when it is new
        let mut dry = Engine::default();
        assert_eq!(says(&dry.step(&cfg(Mode::Dry), &obs(0, 4, false, &[]))).len(), 1);
        assert!(dry.step(&cfg(Mode::On), &obs(1_000, 4, false, &[])).is_empty(), "a spent engine stays quiet: that is why the loop resets it");
        let mut fresh = Engine::default();
        assert!(fresh.step(&cfg(Mode::On), &obs(2_000, 4, false, &[])).contains(&Action::BeginCountdown));
        let src = include_str!("battery_guard.rs").split("#[cfg(test)]").next().unwrap().to_string();
        assert!(src.contains("if cfg.mode != last_mode") && src.contains("engine = Engine::default();"), "the loop resets on a mode change");
    }

    #[test]
    fn switching_from_on_to_practice_during_the_countdown_stops_it_too() {
        let mut e = Engine::default();
        e.step(&cfg(Mode::On), &obs(0, 5, false, &["Claude Code"]));
        let a = e.step(&cfg(Mode::Dry), &obs(10_000, 5, false, &["Claude Code"]));
        assert!(a.contains(&Action::ClearStop), "{a:?}");
        assert!(!a.iter().any(|x| matches!(x, Action::Fire(_) | Action::BeginCountdown)));
        assert!(!e.counting());
    }

    #[test]
    fn the_end_announced_is_the_end_that_happens_even_if_the_setting_changes_mid_countdown() {
        let mut e = Engine::default();
        e.step(&Config::new(Mode::On, 5, End::Hibernate, true), &obs(0, 5, false, &[]));
        let later = Config::new(Mode::On, 5, End::Shutdown, true);
        assert_eq!(e.step(&later, &obs(31_000, 4, false, &[])), vec![Action::Fire(End::Hibernate)]);
    }

    #[test]
    fn without_hibernation_in_windows_a_hibernate_setting_becomes_a_shutdown() {
        assert_eq!(End::Hibernate.effective(false), End::Shutdown);
        assert_eq!(End::Hibernate.effective(true), End::Hibernate);
        assert_eq!(End::Shutdown.effective(true), End::Shutdown);
        let src = include_str!("battery_guard.rs").split("#[cfg(test)]").next().unwrap().to_string();
        assert!(src.contains("cfg.end = cfg.end.effective(hibernate_available())"), "the loop applies it before the engine announces anything");
    }

    #[test]
    fn a_shutdown_that_stops_short_is_not_left_to_die_and_the_order_is_never_left_behind() {
        let src = include_str!("battery_guard.rs").split("#[cfg(test)]").next().unwrap().to_string();
        let fire = src.split("Action::Fire(end) =>").nth(1).unwrap().split("// a stop order with no countdown").next().unwrap();
        assert!(fire.contains("tokio::time::sleep(std::time::Duration::from_secs(45))"), "waits for the graceful sequence");
        assert!(fire.contains("PowerKind::Hibernate") && fire.contains("PowerKind::Shutdown"), "then falls back to hibernate or a plain shutdown");
        assert!(fire.contains("cancel_count() > heard_before"), "but never against a spoken cancel");
        assert!(fire.matches("clear_stop_order()").count() >= 3, "the stop order is cleared on every way out");
        assert!(src.contains("static FIRING") && src.contains("&& !FIRING.load"), "and kept while a power-off is really under way");
    }

    #[test]
    fn the_loop_restarts_itself_after_a_panic() {
        let src = include_str!("battery_guard.rs").split("#[cfg(test)]").next().unwrap().to_string();
        assert!(src.contains("run.await") && src.contains("starting it again"));
    }

    #[test]
    fn negations_and_plain_statements_never_arm_or_change_the_guard() {
        for said in [
            "don't allow battery shutdown",
            "do not enable the battery guard",
            "never shut down when the battery is below 5 percent",
            "shut down the laptop, my battery is at 5 percent",
            "type when the battery is below 5 percent shut down the laptop",
            "write that when the battery drops under five percent i shut down",
        ] {
            assert_eq!(parse_command(said), None, "{said}");
        }
    }

    #[test]
    fn polite_phrasings_and_a_level_without_the_word_percent_are_understood() {
        use Command::Arm;
        for (said, percent) in [
            ("can you shut down the laptop if the battery goes below 5 percent and stop claude", 5),
            ("could you stop claude and shut down when the battery falls under 8 percent", 8),
            ("when the battery drops below 6 stop claude and shut down the laptop", 6),
            ("if the battery falls under thirteen percent shut down the laptop", 13),
        ] {
            match parse_command(said) {
                Some(Arm { percent: p, .. }) => assert_eq!(p as u32, percent, "{said}"),
                other => panic!("{said}: {other:?}"),
            }
        }
    }

    #[test]
    fn the_arming_sentence_never_switches_the_guard_on_by_itself() {
        let src = include_str!("orchestrator.rs");
        let handler = src.split("async fn run_battery_guard_command").nth(1).unwrap().split("async fn run_watch_plan_command").next().unwrap();
        let arm = handler.split("Some(Command::Arm").nth(1).unwrap().split("Some(Command::Status)").next().unwrap();
        assert!(arm.contains("Mode::Dry"), "off becomes practice");
        assert!(!arm.contains("Mode::On.as_str()"), "an arming sentence never writes on");
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::intent_parser::{intent_to_label, parse_deterministic, ParsedIntent};

    #[test]
    fn the_owners_sentences_reach_the_battery_guard_before_any_other_parser_can_take_them() {
        for said in [
            "when the battery is below 5 percent stop claude and shut down the laptop",
            "if the charge drops under five percent tell claude to stop here and shut down the laptop",
            "Nexus, when my battery drops below 8% stop the agents and hibernate the laptop.",
            "allow battery shutdown",
            "practice battery shutdown",
            "battery guard status",
        ] {
            let r = parse_deterministic(said).unwrap_or_else(|| panic!("not understood: {said}"));
            assert_eq!(intent_to_label(&r.intent), "battery_guard_command", "{said}");
            assert!(matches!(r.intent, ParsedIntent::BatteryGuardCommand { .. }));
        }
    }

    #[test]
    fn the_watch_then_shut_down_sentences_are_still_theirs() {
        for said in [
            "shut down the laptop when claude finishes",
            "after kiro finishes shut down the laptop",
            "when all my agents are done shut down the laptop",
        ] {
            let r = parse_deterministic(said).unwrap();
            assert_eq!(intent_to_label(&r.intent), "watch_then_power", "{said}");
        }
        // an ordinary question about the battery is nobody's guard command
        assert_ne!(parse_deterministic("what is my battery percentage").map(|r| intent_to_label(&r.intent)), Some("battery_guard_command"));
    }

    #[test]
    fn the_hook_script_prints_the_answer_for_a_tool_call_and_nothing_else_changes() {
        let text = crate::agent_hooks::script_text("tok", 39220);
        // a tool call: the reply of NEXUS is printed (the stop order reaches the agent through it)
        let pre = text.split(":pre").nth(1).expect("a :pre branch").split(":aider").next().unwrap();
        assert!(pre.contains("curl") && pre.contains("event=%EV%") && !pre.contains(">nul"), "{pre}");
        // ... and a tool-call event is ROUTED there (a branch nobody jumps to would silently drop the stop order)
        #[cfg(windows)]
        assert!(text.contains("if /i \"%EV%\"==\"pretool\" goto pre"), "{text}");
        #[cfg(not(windows))]
        assert!(text.contains("[ \"$EV\" = \"pretool\" ]"), "{text}");
        // every other event still ends in a neutral {} that cannot block an agent
        assert!(text.contains("echo {}"));
    }
}
