//! Ghost Mode command center. The main command center owns the turn and its
//! ownership/validity gates; this center owns only foreground desktop actions.
//! Each sub-center returns an honest receipt. No vision/OCR path exists here.

use crate::desktop_control::{Resolution, TargetQuery, Verification};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use crate::shortcuts::{map::Action, router};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GhostStep {
    Shortcut { action: String, arg: Option<u32> },
    AccessibleClick { name: String },
    TargetClick { query: TargetQuery },
    Choose { number: usize },
    Learn { alias: String },
    Forget { alias: String },
    LearnedClick { alias: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhostTask {
    pub steps: Vec<GhostStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    KeyboardCenter,
    AccessibilityCenter,
    CursorCenter,
    LearningCenter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Verified,
    Unconfirmed,
    Blocked,
    Stopped,
}

#[derive(Debug, Clone, Serialize)]
pub struct Receipt {
    pub step: usize,
    pub route: Route,
    pub outcome: Outcome,
    pub detail: String,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
}

impl Receipt {
    pub fn spoken(&self) -> String {
        if self.reason_code.as_deref() == Some("clarification_required") {
            return self.detail.clone();
        }
        match self.outcome {
            Outcome::Verified => format!("{}, sir.", self.detail),
            Outcome::Unconfirmed => format!(
                "I tried that, sir, but couldn't verify the result. {}",
                self.detail
            ),
            Outcome::Blocked => format!("I couldn't do that safely, sir. {}", self.detail),
            Outcome::Stopped => "Stopped, sir. Ghost mode is still on.".into(),
        }
    }
}

/// Strict, local grammar for low-risk desktop actions. A compound command is
/// accepted only if EVERY step parses; unknown text never becomes a click.
pub fn parse(transcript: &str) -> Option<GhostTask> {
    let mut text = transcript
        .trim()
        .trim_end_matches(['.', '!', '?'])
        .to_lowercase();
    loop {
        let next = ["nexus, ", "nexus ", "please ", "can you ", "could you "]
            .iter()
            .find_map(|prefix| text.strip_prefix(prefix).map(|rest| rest.to_string()));
        match next {
            Some(rest) => text = rest,
            None => break,
        }
    }
    let parts: Vec<&str> = text.split(" then ").collect();
    if parts.is_empty() || parts.len() > 5 {
        return None;
    }
    let steps = parts
        .into_iter()
        .map(parse_step)
        .collect::<Option<Vec<_>>>()?;
    Some(GhostTask { steps })
}

fn parse_step(raw: &str) -> Option<GhostStep> {
    let t = raw.trim().trim_start_matches("please ").trim();
    for prefix in ["choose ", "option ", "number "] {
        if let Some(n) = t.strip_prefix(prefix).and_then(|v| match v {
            "one" => Some(1),
            "two" => Some(2),
            "three" => Some(3),
            "four" => Some(4),
            "five" => Some(5),
            _ => v.parse::<usize>().ok(),
        }) {
            return (1..=5)
                .contains(&n)
                .then_some(GhostStep::Choose { number: n });
        }
    }
    let t = t.trim_start_matches("the ");
    for prefix in [
        "learn this as ",
        "remember this button as ",
        "remember this control as ",
    ] {
        if let Some(alias) = t.strip_prefix(prefix) {
            return valid_alias(alias).then(|| GhostStep::Learn {
                alias: alias.into(),
            });
        }
    }
    if let Some(alias) = t.strip_prefix("forget learned ") {
        return valid_alias(alias).then(|| GhostStep::Forget {
            alias: alias.into(),
        });
    }
    if let Some(alias) = t.strip_prefix("click learned ") {
        return valid_alias(alias).then(|| GhostStep::LearnedClick {
            alias: alias.into(),
        });
    }
    let action = match t {
        "save" | "save this" | "save file" | "save this file" | "save the file" => {
            Some(Action::Save)
        }
        "copy" | "copy this" | "copy selection" => Some(Action::Copy),
        "paste" | "paste this" => Some(Action::Paste),
        "cut" | "cut this" => Some(Action::Cut),
        "undo" | "undo that" => Some(Action::Undo),
        "redo" | "redo that" => Some(Action::Redo),
        "select all" | "select everything" => Some(Action::SelectAll),
        "refresh" | "reload" | "refresh page" | "reload page" => Some(Action::Refresh),
        "go back" | "back" => Some(Action::GoBack),
        "go forward" | "forward" => Some(Action::GoForward),
        "new tab" | "open new tab" | "open a new tab" => Some(Action::NewTab),
        "close tab" | "close this tab" => Some(Action::CloseTab),
        "reopen tab" | "reopen closed tab" => Some(Action::ReopenTab),
        "next tab" | "switch to next tab" => Some(Action::NextTab),
        "previous tab" | "prev tab" | "switch to previous tab" => Some(Action::PrevTab),
        "address bar" | "focus address bar" => Some(Action::AddressBar),
        "find" | "find in page" | "search in page" => Some(Action::FindInPage),
        "scroll down" => Some(Action::ScrollDown),
        "scroll up" => Some(Action::ScrollUp),
        "zoom in" => Some(Action::ZoomIn),
        "zoom out" => Some(Action::ZoomOut),
        "switch window" => Some(Action::SwitchWindow),
        _ => None,
    };
    if let Some(action) = action {
        return Some(GhostStep::Shortcut {
            action: action.id().into(),
            arg: None,
        });
    }
    for prefix in ["switch to tab ", "go to tab ", "tab "] {
        if let Some(n) = t.strip_prefix(prefix).and_then(|v| v.parse::<u32>().ok()) {
            if (1..=9).contains(&n) {
                return Some(GhostStep::Shortcut {
                    action: Action::GotoTab.id().into(),
                    arg: Some(n),
                });
            }
        }
    }
    let name = t
        .strip_prefix("click ")
        .or_else(|| t.strip_prefix("press "))?
        .trim_start_matches("the ")
        .trim();
    if let Some((label, panel)) = name.rsplit_once(" in ") {
        let mut q = TargetQuery::named(label);
        q.container = Some(panel.to_string());
        for (suffix, role) in [
            (" button", "button"),
            (" tab", "tabitem"),
            (" option", "listitem"),
        ] {
            if let Some(v) = label.strip_suffix(suffix) {
                q.label = v.into();
                q.role = Some(role.into());
                break;
            }
        }
        if q.label.len() < 2
            || q.label.len() > 80
            || panel.is_empty()
            || panel.len() > 80
            || is_risky_target(&q.label)
            || is_ambiguous_target(&q.label)
        {
            return None;
        }
        return Some(GhostStep::TargetClick { query: q });
    }
    for (suffix, role) in [
        (" button", "button"),
        (" tab", "tabitem"),
        (" option", "listitem"),
    ] {
        if let Some(label) = name.strip_suffix(suffix) {
            if label.len() < 2
                || label.len() > 80
                || is_risky_target(label)
                || is_ambiguous_target(label)
            {
                return None;
            }
            let mut query = TargetQuery::named(label);
            query.role = Some(role.into());
            return Some(GhostStep::TargetClick { query });
        }
    }
    let name = name
        .strip_suffix(" button")
        .or_else(|| name.strip_suffix(" tab"))
        .or_else(|| name.strip_suffix(" option"))
        .unwrap_or(name)
        .trim();
    if name.len() < 2 || name.len() > 80 || is_risky_target(name) || is_ambiguous_target(name) {
        return None;
    }
    Some(GhostStep::AccessibleClick { name: name.into() })
}

pub(crate) fn is_risky_target(name: &str) -> bool {
    let lower = name.to_lowercase();
    if ["sign in", "log in", "close account", "place order"]
        .iter()
        .any(|phrase| lower.contains(phrase))
    {
        return true;
    }
    [
        "send",
        "submit",
        "delete",
        "remove",
        "buy",
        "purchase",
        "pay",
        "transfer",
        "confirm",
        "allow",
        "authorize",
        "install",
        "password",
        "erase",
        "discard",
        "publish",
        "post",
        "withdraw",
        "approve",
        "reject",
        "unsubscribe",
        "disconnect",
        "archive",
        "reset",
        "clear",
        "revoke",
        "grant",
        "share",
    ]
    .iter()
    .any(|word| {
        lower
            .split(|c: char| !c.is_alphanumeric())
            .any(|part| part == *word)
    })
}

fn valid_alias(alias: &str) -> bool {
    let alias = alias.trim();
    (2..=40).contains(&alias.len())
        && alias
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
        && !is_risky_target(alias)
        && !is_ambiguous_target(alias)
}

pub(crate) fn is_ambiguous_target(name: &str) -> bool {
    let name = name.to_lowercase();
    let base = name
        .strip_suffix(" button")
        .or_else(|| name.strip_suffix(" option"))
        .or_else(|| name.strip_suffix(" tab"))
        .unwrap_or(&name);
    matches!(
        base,
        "here"
            | "there"
            | "this"
            | "that"
            | "it"
            | "one"
            | "button"
            | "option"
            | "ok"
            | "yes"
            | "continue"
            | "next"
            | "accept"
            | "agree"
            | "red"
            | "blue"
            | "green"
            | "yellow"
            | "black"
            | "white"
    ) || name.ends_with(" one")
        || name.ends_with(" thing")
}

pub fn validate(task: &GhostTask) -> bool {
    !task.steps.is_empty()
        && task.steps.len() <= 5
        && task.steps.iter().all(|step| match step {
            GhostStep::Shortcut { action, arg } => Action::from_id(action).is_some_and(|a| {
                a.takes_number() == arg.is_some() && arg.is_none_or(|n| (1..=9).contains(&n))
            }),
            GhostStep::AccessibleClick { name } => {
                name.len() >= 2
                    && name.len() <= 80
                    && !is_risky_target(name)
                    && !is_ambiguous_target(name)
            }
            GhostStep::Choose { number } => task.steps.len() == 1 && (1..=5).contains(number),
            GhostStep::TargetClick { query } => {
                query.label.len() >= 2
                    && query.label.len() <= 80
                    && !is_risky_target(&query.label)
                    && !is_ambiguous_target(&query.label)
                    && query
                        .container
                        .as_ref()
                        .is_none_or(|v| !v.is_empty() && v.len() <= 80)
                    && query.role.as_ref().is_none_or(|v| {
                        [
                            "button",
                            "tabitem",
                            "menuitem",
                            "listitem",
                            "edit",
                            "checkbox",
                            "radiobutton",
                            "combobox",
                            "hyperlink",
                            "splitbutton",
                        ]
                        .contains(&v.as_str())
                    })
                    && query.learned_alias.as_ref().is_none_or(|v| valid_alias(v))
            }
            GhostStep::Learn { alias }
            | GhostStep::Forget { alias }
            | GhostStep::LearnedClick { alias } => valid_alias(alias),
        })
}

/// Sequential execution is intentional: after an unverified step, the next
/// command may target a different UI state and must not be sent blindly.
pub fn execute_with_store(
    task: &GhostTask,
    cancel: &AtomicBool,
    store_dir: Option<&Path>,
) -> Vec<Receipt> {
    execute_inner(task, cancel, store_dir, None)
}

pub fn execute_authorized(
    task: &GhostTask,
    cancel: Arc<AtomicBool>,
    store_dir: Option<&Path>,
) -> Vec<Receipt> {
    execute_inner(task, &cancel, store_dir, Some(cancel.clone()))
}
fn execute_inner(
    task: &GhostTask,
    cancel: &AtomicBool,
    store_dir: Option<&Path>,
    owner: Option<Arc<AtomicBool>>,
) -> Vec<Receipt> {
    let operation_id = crate::network::uuid_v4();
    if let [GhostStep::Choose { number }] = task.steps.as_slice() {
        return crate::desktop_choices::choose_voice(*number, cancel);
    }
    crate::desktop_choices::clear();
    if !validate(task) {
        return vec![Receipt {
            step: 0,
            route: Route::KeyboardCenter,
            outcome: Outcome::Blocked,
            detail: "The Ghost command is incomplete or unsafe.".into(),
            elapsed_ms: 0,
            reason_code: Some("blocked".into()),
            operation_id: Some(operation_id.clone()),
        }];
    }
    let mut receipts = Vec::with_capacity(task.steps.len());
    for (index, step) in task.steps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed)
            || crate::ghost::stop_requested()
            || !crate::ghost::session_active()
        {
            receipts.push(Receipt {
                step: index,
                route: route_for(step),
                outcome: Outcome::Stopped,
                detail: "The task was stopped.".into(),
                elapsed_ms: 0,
                reason_code: Some("blocked".into()),
                operation_id: Some(operation_id.clone()),
            });
            break;
        }
        let start = Instant::now();
        let (route, outcome, detail) = match step {
            GhostStep::Shortcut { action, arg } => keyboard_subcenter(action, *arg, cancel),
            GhostStep::AccessibleClick { name } => run_query(
                &TargetQuery::named(name),
                None,
                cancel,
                owner.clone(),
                task.steps[index + 1..].to_vec(),
                store_dir,
            ),
            GhostStep::TargetClick { query } => run_query(
                query,
                None,
                cancel,
                owner.clone(),
                task.steps[index + 1..].to_vec(),
                store_dir,
            ),
            GhostStep::Choose { .. } => (
                Route::AccessibilityCenter,
                Outcome::Blocked,
                "A choice must be a separate request.".into(),
            ),
            GhostStep::Learn { alias } => learning_subcenter(alias, store_dir, true),
            GhostStep::Forget { alias } => learning_subcenter(alias, store_dir, false),
            GhostStep::LearnedClick { alias } => learned_click_subcenter(alias, store_dir, cancel),
        };
        let receipt = Receipt {
            step: index,
            route,
            outcome,
            detail,
            elapsed_ms: start.elapsed().as_millis() as u64,
            reason_code: Some(
                match outcome {
                    Outcome::Verified => "effect_verified",
                    Outcome::Unconfirmed if crate::desktop_choices::has_pending() => {
                        "clarification_required"
                    }
                    Outcome::Unconfirmed => "effect_unconfirmed",
                    Outcome::Blocked => "blocked",
                    Outcome::Stopped => "stopped",
                }
                .into(),
            ),
            operation_id: Some(operation_id.clone()),
        };
        tracing::info!(
            "[GHOST-CENTER] operation={} step={} route={route:?} outcome={outcome:?} elapsed_ms={}",
            operation_id,
            index,
            receipt.elapsed_ms
        );
        receipts.push(receipt);
        if outcome != Outcome::Verified {
            break;
        }
    }
    receipts
}

fn route_for(step: &GhostStep) -> Route {
    match step {
        GhostStep::Shortcut { .. } => Route::KeyboardCenter,
        GhostStep::AccessibleClick { .. }
        | GhostStep::TargetClick { .. }
        | GhostStep::Choose { .. } => Route::AccessibilityCenter,
        GhostStep::Learn { .. } | GhostStep::Forget { .. } | GhostStep::LearnedClick { .. } => {
            Route::LearningCenter
        }
    }
}

fn learning_subcenter(
    alias: &str,
    store_dir: Option<&Path>,
    teach: bool,
) -> (Route, Outcome, String) {
    let Some(dir) = store_dir else {
        return (
            Route::LearningCenter,
            Outcome::Blocked,
            "I can't access the Ghost learning store.".into(),
        );
    };
    let result = if teach {
        crate::ghost_learning::teach(dir, alias)
    } else {
        crate::ghost_learning::forget(dir, alias)
    };
    match result {
        Ok(detail) => (Route::LearningCenter, Outcome::Verified, detail),
        Err(detail) => (Route::LearningCenter, Outcome::Blocked, detail),
    }
}

fn learned_click_subcenter(
    alias: &str,
    store_dir: Option<&Path>,
    cancel: &AtomicBool,
) -> (Route, Outcome, String) {
    let Some(dir) = store_dir else {
        return (
            Route::LearningCenter,
            Outcome::Blocked,
            "I can't access the Ghost learning store.".into(),
        );
    };
    match crate::ghost_learning::resolve(dir, alias) {
        Ok((name, id)) => accessibility_subcenter_with_id(&name, Some(&id), cancel),
        Err(reason) => (Route::LearningCenter, Outcome::Blocked, reason),
    }
}

fn keyboard_subcenter(
    action_id: &str,
    arg: Option<u32>,
    cancel: &AtomicBool,
) -> (Route, Outcome, String) {
    if cancel.load(Ordering::Relaxed) {
        return (
            Route::KeyboardCenter,
            Outcome::Stopped,
            "The task was stopped.".into(),
        );
    }
    let Some(action) = Action::from_id(action_id) else {
        return (
            Route::KeyboardCenter,
            Outcome::Blocked,
            "Unknown keyboard action.".into(),
        );
    };
    if matches!(
        action,
        Action::Copy
            | Action::Paste
            | Action::Cut
            | Action::Undo
            | Action::Redo
            | Action::SelectAll
    ) && focused_field_is_secure()
    {
        return (
            Route::KeyboardCenter,
            Outcome::Blocked,
            "I can't safely edit a protected or unreadable field.".into(),
        );
    }
    if !crate::shortcuts::enabled() {
        return (
            Route::KeyboardCenter,
            Outcome::Blocked,
            "Keyboard routing is disabled.".into(),
        );
    }
    let out = crate::shortcuts::exec::do_action_cancelled(action, arg, cancel);
    match &out {
        router::Outcome::Sent {
            verified: router::Verified::Yes,
            ..
        } => (
            Route::KeyboardCenter,
            Outcome::Verified,
            format!("{} completed", action.label()),
        ),
        router::Outcome::Sent { .. } => (
            Route::KeyboardCenter,
            Outcome::Unconfirmed,
            format!("The {} shortcut was sent once.", action.label()),
        ),
        router::Outcome::Aborted(router::Abort::Stopped) => (
            Route::KeyboardCenter,
            Outcome::Stopped,
            "The task was stopped.".into(),
        ),
        other => (
            Route::KeyboardCenter,
            Outcome::Blocked,
            router::reason(action, other).unwrap_or_else(|| "The shortcut was refused.".into()),
        ),
    }
}

#[cfg(target_os = "windows")]
fn focused_field_is_secure() -> bool {
    let Ok(automation) = uiautomation::UIAutomation::new() else {
        return true;
    };
    let Ok(focused) = automation.get_focused_element() else {
        return true;
    };
    focused.is_password().unwrap_or(true)
}

#[cfg(not(target_os = "windows"))]
fn focused_field_is_secure() -> bool {
    true
}

fn accessibility_subcenter_with_id(
    name: &str,
    expected_id: Option<&str>,
    cancel: &AtomicBool,
) -> (Route, Outcome, String) {
    #[cfg(target_os = "windows")]
    {
        match windows_accessibility::act(name, expected_id, cancel) {
            Ok((route, outcome, detail)) => (route, outcome, detail),
            Err(reason) => (Route::AccessibilityCenter, Outcome::Blocked, reason),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (name, expected_id, cancel);
        (
            Route::AccessibilityCenter,
            Outcome::Blocked,
            "Accessible Ghost control currently requires Windows.".into(),
        )
    }
}

fn run_query(
    query: &TargetQuery,
    expected_id: Option<&str>,
    cancel: &AtomicBool,
    owner: Option<Arc<AtomicBool>>,
    remaining: Vec<GhostStep>,
    store: Option<&Path>,
) -> (Route, Outcome, String) {
    #[cfg(target_os = "windows")]
    {
        let result = (|| -> Result<(Route, Outcome, String), String> {
            let learned = query
                .learned_alias
                .as_ref()
                .map(|alias| {
                    store
                        .ok_or_else(|| "Learning store unavailable.".to_string())
                        .and_then(|dir| crate::ghost_learning::resolve(dir, alias))
                })
                .transpose()?;
            let mut query = query.clone();
            if let Some((name, _)) = &learned {
                query.label = name.clone()
            }
            let expected_id = learned.as_ref().map(|(_, id)| id.as_str()).or(expected_id);
            let candidates = crate::desktop_native::candidates()?;
            const ALIASES: &[(&str, &str, &str)] = &[("msedge.exe", "preferences", "settings")];
            match crate::desktop_control::resolve(&query, &candidates, expected_id, ALIASES) {
                Resolution::Unique(candidate) => {
                    let (route, verified) = crate::desktop_native::perform(&candidate, cancel)?;
                    Ok((
                        route,
                        match verified {
                            Verification::Verified => Outcome::Verified,
                            Verification::Stopped => Outcome::Stopped,
                            Verification::ForegroundChanged => Outcome::Blocked,
                            Verification::Unconfirmed => Outcome::Unconfirmed,
                        },
                        "Control action processed".into(),
                    ))
                }
                Resolution::Clarify(choices) => {
                    let owner = owner.ok_or("Several controls match; use a more specific name.")?;
                    if cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested() {
                        return Err("Stopped.".into());
                    }
                    crate::desktop_choices::offer(
                        choices,
                        owner,
                        remaining,
                        store.map(Path::to_path_buf),
                    );
                    Ok((
                        Route::AccessibilityCenter,
                        Outcome::Unconfirmed,
                        "Which control? Choose a number from the sidebar, sir.".into(),
                    ))
                }
                Resolution::Unavailable => {
                    Err("No matching accessible control is available.".into())
                }
                Resolution::Blocked(_) => {
                    Err("Too many or unsafe matches; name the control and containing panel.".into())
                }
            }
        })();
        result.unwrap_or_else(|e| {
            (
                Route::AccessibilityCenter,
                if cancel.load(Ordering::Relaxed) || crate::ghost::stop_requested() {
                    Outcome::Stopped
                } else {
                    Outcome::Blocked
                },
                e,
            )
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (query, expected_id, cancel, owner, remaining, store);
        (
            Route::AccessibilityCenter,
            Outcome::Blocked,
            "Native desktop control requires Windows.".into(),
        )
    }
}

#[cfg(target_os = "windows")]
mod windows_accessibility {
    use super::*;
    pub fn act(
        name: &str,
        id: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<(Route, Outcome, String), String> {
        Ok(run_query(
            &TargetQuery::named(name),
            id,
            cancel,
            None,
            vec![],
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_shortcuts_and_ordered_steps() {
        assert_eq!(
            parse("Nexus save then open a new tab"),
            Some(GhostTask {
                steps: vec![
                    GhostStep::Shortcut {
                        action: "save".into(),
                        arg: None
                    },
                    GhostStep::Shortcut {
                        action: "new_tab".into(),
                        arg: None
                    },
                ]
            })
        );
        assert_eq!(
            parse("switch to tab 3").unwrap().steps[0],
            GhostStep::Shortcut {
                action: "goto_tab_n".into(),
                arg: Some(3)
            }
        );
        assert_eq!(
            parse("Nexus, can you please click the Settings button")
                .unwrap()
                .steps[0],
            GhostStep::TargetClick {
                query: TargetQuery {
                    label: "settings".into(),
                    role: Some("button".into()),
                    container: None,
                    learned_alias: None
                }
            }
        );
    }

    #[test]
    fn parser_abstains_on_ambiguous_or_risky_text() {
        for text in [
            "I wish this was saved",
            "click the blue one",
            "click the blue button",
            "click Send",
            "click Delete",
            "save then maybe buy it",
            "tab 10",
        ] {
            assert!(parse(text).is_none(), "{text}");
        }
        assert!(!validate(&GhostTask {
            steps: vec![GhostStep::AccessibleClick {
                name: "Blue button".into()
            }],
        }));
    }

    #[test]
    fn learned_controls_use_explicit_alias_commands() {
        assert_eq!(
            parse("remember this button as settings").unwrap().steps[0],
            GhostStep::Learn {
                alias: "settings".into()
            }
        );
        assert_eq!(
            parse("click learned settings").unwrap().steps[0],
            GhostStep::LearnedClick {
                alias: "settings".into()
            }
        );
        assert_eq!(
            parse("forget learned settings").unwrap().steps[0],
            GhostStep::Forget {
                alias: "settings".into()
            }
        );
        assert!(parse("remember this button as send").is_none());
    }

    #[test]
    fn receipt_never_claims_unverified_success() {
        let r = Receipt {
            step: 0,
            route: Route::KeyboardCenter,
            outcome: Outcome::Unconfirmed,
            detail: "Save shortcut was sent.".into(),
            elapsed_ms: 1,
            reason_code: None,
            operation_id: None,
        };
        assert!(r.spoken().contains("couldn't verify"));
        assert!(!r.spoken().starts_with("Done"));
    }
    #[test]
    fn contextual_targets_and_spoken_choices_keep_typed_scope() {
        let task = parse("click Open button in Toolbar").unwrap();
        assert!(
            matches!(&task.steps[0],GhostStep::TargetClick{query} if query.label=="open"&&query.role.as_deref()==Some("button")&&query.container.as_deref()==Some("toolbar"))
        );
        assert!(matches!(
            parse("choose two").unwrap().steps[0],
            GhostStep::Choose { number: 2 }
        ));
        assert!(!validate(&GhostTask {
            steps: vec![
                GhostStep::Choose { number: 1 },
                GhostStep::Shortcut {
                    action: "save".into(),
                    arg: None
                }
            ]
        }));
        assert!(serde_json::from_str::<GhostTask>(
            r#"{"steps":[{"kind":"accessible_click","name":"Open"}]}"#
        )
        .is_ok());
    }
}
