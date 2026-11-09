//! Finds the coding agents on this laptop and hooks them up to NEXUS by itself, so the Agent Island and the
//! "watch this" features work without anyone saying "hook up ...".
//!
//! Runs shortly after start and then every half hour (an agent installed later is picked up too). It only adds
//! NEXUS's own entry through the same `agent_hooks::install` the voice command uses (a backup of the agent's
//! settings is made first, a foreign hook is left alone). Setting `agentAutoHook` (Command Hub > Advanced > Agent
//! Island, default on) turns it off; turning it off does not remove what was already added ("remove the Claude hook"
//! still does that).
use crate::agent_hooks::{self, Agent};
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

const FIRST_SCAN_DELAY_SECS: u64 = 25;
const RESCAN_SECS: u64 = 30 * 60;

/// Pure: the agents that still need a hook (present, hookable, not yet installed).
pub fn missing(present: &[Agent], installed: impl Fn(Agent) -> bool) -> Vec<Agent> {
    present.iter().copied().filter(|a| !a.observe_only() && *a != Agent::Devin && !installed(*a)).collect()
}

/// Pure: the one line shown after new hooks were added.
pub fn notice(done: &[Agent]) -> Option<String> {
    let names: Vec<&str> = done.iter().map(|a| a.label()).collect();
    match names.as_slice() {
        [] => None,
        [one] => Some(format!("I found {one} on this laptop and connected it, sir. Restart it once so it starts reporting.")),
        many => Some(format!("I found {} on this laptop and connected them, sir. Restart them once so they start reporting.", many.join(", "))),
    }
}

fn enabled(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("settings.json"))
        .ok()
        .and_then(|c| serde_json::from_str::<serde_json::Value>(&c).ok())
        .and_then(|j| j.get("agentAutoHook").and_then(|v| v.as_bool()))
        .unwrap_or(true)
}

/// One scan. Returns the agents hooked up by this call.
pub fn scan_and_hook<R: Runtime>(app: &AppHandle<R>) -> Vec<Agent> {
    let Ok(dir) = app.path().app_data_dir() else { return Vec::new() };
    if !enabled(&dir) {
        return Vec::new();
    }
    let todo = missing(&agent_hooks::agents_present(), |a| agent_hooks::config_path(a).map_or(true, |c| agent_hooks::is_installed(a, &c)));
    if todo.is_empty() {
        return Vec::new();
    }
    let token = agent_hooks::agent_event_token();
    let script = match agent_hooks::write_script(&dir, &token, crate::webhook::WEBHOOK_PORT) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("[AUTOHOOK] could not write the hook script: {e:?}");
            return Vec::new();
        }
    };
    let target = agent_hooks::HookTarget { script: &script, token: &token, port: crate::webhook::WEBHOOK_PORT };
    let mut done = Vec::new();
    for agent in todo {
        let Some(cfg) = agent_hooks::config_path(agent) else { continue };
        match agent_hooks::install(agent, &cfg, &target) {
            Ok(_) => {
                tracing::info!("[AUTOHOOK] hooked up {}", agent.label());
                done.push(agent);
            }
            Err(e) => tracing::warn!("[AUTOHOOK] {} not hooked up: {e:?}", agent.label()),
        }
    }
    if !done.is_empty() {
        crate::watch_runner::ensure_running(app.clone());
    }
    done
}

pub fn start<R: Runtime>(app: AppHandle<R>) {
    let _ = std::thread::Builder::new().name("agent-autohook".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(FIRST_SCAN_DELAY_SECS));
        loop {
            let done = scan_and_hook(&app);
            if let Some(text) = notice(&done) {
                crate::proactive_policy::submit(&app, format!("agent_autohook_{}", agent_hooks::now_ms()), text, crate::google::types::AlertUrgency::Medium);
            }
            std::thread::sleep(std::time::Duration::from_secs(RESCAN_SECS));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_present_hookable_agents_without_a_hook_are_listed() {
        let present = [Agent::Claude, Agent::OpenCode, Agent::Antigravity, Agent::Devin, Agent::Trae];
        let todo = missing(&present, |a| a == Agent::Claude);
        assert_eq!(todo, vec![Agent::OpenCode, Agent::Antigravity]);
    }

    #[test]
    fn nothing_to_do_when_everything_is_hooked() {
        assert!(missing(&[Agent::Claude], |_| true).is_empty());
        assert!(notice(&[]).is_none());
    }

    #[test]
    fn the_notice_names_each_agent() {
        let one = notice(&[Agent::OpenCode]).unwrap();
        assert!(one.contains(Agent::OpenCode.label()) && one.contains("Restart it"));
        let two = notice(&[Agent::OpenCode, Agent::Antigravity]).unwrap();
        assert!(two.contains(Agent::Antigravity.label()) && two.contains("Restart them"));
    }

    #[test]
    fn a_cli_on_path_counts_even_without_a_settings_folder() {
        let dir = std::env::temp_dir().join(format!("nexus_autohook_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opencode.cmd"), "x").unwrap();
        std::fs::write(dir.join("agy.exe"), "x").unwrap();
        let path = std::env::join_paths([&dir]).unwrap();
        let found = agent_hooks::agents_on_path_in(&path);
        assert!(found.contains(&Agent::OpenCode) && found.contains(&Agent::Antigravity));
        assert!(!found.contains(&Agent::Claude));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
