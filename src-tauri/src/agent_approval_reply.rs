//! What an agent's permission hook prints back: the one place that knows each agent's stdout contract.
//!
//! Ported from `windows/hook/src/reply.rs` of Louis-CFM/coucou (MIT licence, Copyright (c) 2026 Louis Raillé;
//! notice in THIRD_PARTY_NOTICES.md). Changed from the original: product name in the denial text, and the agent
//! names follow NEXUS's `agent_hooks::Agent::name()` through `reply_agent` ("claude" is coucou's empty agent).
//!
//! The one rule that matters: **nothing that allows anything is ever printed without a decision a human clicked.**
//! With no decision the reply is silence, an empty `{}`, or Copilot's explicit "ask", and every agent then asks in
//! its own terminal exactly as if NEXUS were not installed.

use serde_json::{json, Map, Value};

/// NEXUS's agent name -> the name this module's contract tables use (Claude Code is the empty name).
pub fn reply_agent(nexus_name: &str) -> &str {
    if nexus_name == "claude" {
        ""
    } else {
        nexus_name
    }
}

/// The agents whose permission requests the island can answer: Claude Code (empty name), Codex, Copilot CLI and
/// OpenCode. Any decision for another agent is ignored here too, whatever the app sent.
pub fn takes_decisions(agent: &str) -> bool {
    matches!(agent, "" | "codex" | "copilot" | "muse" | "claude-desktop" | "opencode")
}

/// Antigravity's PreToolUse: it has no permission request and waits for this hook's decision instead.
pub fn waits_on_pre_tool(agent: &str, event: &str) -> bool {
    agent.eq_ignore_ascii_case("antigravity") && event == "PreToolUse"
}

/// Agents that read a JSON object on stdout after every hook and get `{}` ("no opinion") when there is nothing to say.
fn wants_json(agent: &str) -> bool {
    matches!(agent, "gemini" | "antigravity" | "copilot" | "muse")
}

/// The line to print for `event` from `agent`, given the island's `decision` (`None`: nobody clicked) and, for
/// Claude Code's AskUserQuestion, the question as it was asked.
pub fn stdout(agent: &str, event: &str, decision: Option<&str>, question: Option<&Value>) -> Option<String> {
    if event != "PermissionRequest" {
        if waits_on_pre_tool(agent, event) {
            let reply = match decision.map(str::trim) {
                Some("allow" | "always") => r#"{"decision":"allow"}"#,
                Some("deny") => r#"{"decision":"deny","reason":"Denied from NEXUS"}"#,
                _ => r#"{"decision":"ask"}"#,
            };
            return Some(reply.to_string());
        }
        return wants_json(agent).then(|| "{}".to_string());
    }
    let decision = decision.filter(|_| takes_decisions(agent));
    match agent {
        // Copilot CLI and Muse Code read a bare permissionDecision. Copilot is fail-closed: it must always get
        // valid JSON, so silence is an "ask".
        "copilot" | "muse" => {
            let word = match decision.map(str::trim) {
                Some("allow" | "always") => Some("allow"),
                Some("deny") => Some("deny"),
                _ => None,
            };
            match word {
                Some(word) => Some(json!({ "permissionDecision": word }).to_string()),
                None if agent == "copilot" => Some(json!({ "permissionDecision": "ask" }).to_string()),
                None => None,
            }
        }
        // Claude Code, Claude Desktop and Codex share the documented hookSpecificOutput. Claude can ask questions.
        "" | "claude-desktop" => decision.and_then(|d| decision_json(d, question)),
        _ => decision.and_then(|d| decision_json(d, None)),
    }
}

/// The documented PermissionRequest output (https://code.claude.com/docs/en/hooks). Anything unrecognised prints
/// nothing rather than guessing: silence is the safe answer.
///
/// `question` is the AskUserQuestion input, when that is what is being asked: the island may then answer it, which
/// Claude Code takes as the same input with an `answers` map added. Nothing else of the input can be changed from
/// the island, and no other tool's input can be changed at all.
pub fn decision_json(decision: &str, question: Option<&Value>) -> Option<String> {
    if decision.trim_start().starts_with('{') {
        let reply = serde_json::from_str::<Value>(decision).ok()?;
        let answers = reply.get("answers")?.as_object()?;
        let question = question?;
        if !answers_fit(question, answers) {
            return None;
        }
        let mut input = question.as_object()?.clone();
        input.insert("answers".into(), Value::Object(answers.clone()));
        return Some(
            json!({
                "hookSpecificOutput": {
                    "hookEventName": "PermissionRequest",
                    "decision": { "behavior": "allow", "updatedInput": input },
                }
            })
            .to_string(),
        );
    }
    let behavior = match decision.trim() {
        // "always" still answers a plain allow; remembering it is the island's business, not the agent's.
        "allow" | "always" => r#"{"behavior":"allow"}"#.to_string(),
        "deny" => r#"{"behavior":"deny","message":"Denied from NEXUS"}"#.to_string(),
        _ => return None,
    };
    Some(format!(r#"{{"hookSpecificOutput":{{"hookEventName":"PermissionRequest","decision":{behavior}}}}}"#))
}

/// True when `answers` answers exactly the questions Claude Code asked: one entry per question, keyed by its text;
/// a single-select answer is one of its option labels (a string), a multi-select answer a non-empty list of
/// distinct labels.
fn answers_fit(question: &Value, answers: &Map<String, Value>) -> bool {
    let Some(items) = question.get("questions").and_then(|q| q.as_array()) else { return false };
    if items.is_empty() || items.len() != answers.len() {
        return false;
    }
    items.iter().all(|item| {
        let Some(text) = item.get("question").and_then(|q| q.as_str()) else { return false };
        let labels: Vec<&str> = item
            .get("options")
            .and_then(|o| o.as_array())
            .map(|opts| opts.iter().filter_map(|o| o.get("label").and_then(|l| l.as_str())).collect())
            .unwrap_or_default();
        let multi = item.get("multiSelect").and_then(|m| m.as_bool()).unwrap_or(false);
        match answers.get(text) {
            Some(Value::String(pick)) if !multi => labels.contains(&pick.as_str()),
            Some(Value::Array(picks)) if multi => {
                let picks: Vec<&str> = picks.iter().filter_map(|p| p.as_str()).collect();
                let mut seen = picks.clone();
                seen.sort_unstable();
                seen.dedup();
                !picks.is_empty() && seen.len() == picks.len() && picks.iter().all(|p| labels.contains(p))
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const AGENTS: &[&str] = &[
        "", "gemini", "antigravity", "cursor", "codex", "copilot", "muse", "opencode", "amp", "hermes", "claude-desktop",
        "my-tool",
    ];
    const EVENTS: &[&str] = &[
        "SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest",
        "Notification", "Stop", "StopFailure", "SessionEnd", "Interrupt", "SubagentStart", "SubagentStop", "",
    ];

    /// Whatever a reply says, does it let anything through?
    fn allows(out: &str) -> bool {
        let v: Value = serde_json::from_str(out).expect("every reply is JSON");
        let text = v.to_string();
        let decided = v.get("decision").map_or(false, |d| d.as_str() != Some("ask"));
        text.contains("allow") || decided || v.get("permission").is_some() || v.get("continue").is_some()
    }

    #[test]
    fn nothing_is_ever_allowed_without_a_decision() {
        for agent in AGENTS {
            for event in EVENTS {
                if let Some(out) = stdout(agent, event, None, None) {
                    assert!(!allows(&out), "{agent:?} {event} printed {out} with nobody clicking");
                }
                for not_a_decision in ["", "ask", "maybe", "{\"permissionDecision\":\"allow\"}"] {
                    if let Some(out) = stdout(agent, event, Some(not_a_decision), None) {
                        assert!(!allows(&out), "{agent:?} {event} {not_a_decision:?} printed {out}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_decision_only_counts_on_a_permission_request_from_an_agent_that_takes_one() {
        for agent in AGENTS {
            for event in EVENTS.iter().filter(|e| **e != "PermissionRequest") {
                if let Some(out) = stdout(agent, event, Some("allow"), None) {
                    assert_eq!(allows(&out), waits_on_pre_tool(agent, event), "{agent:?} {event}: {out}");
                }
            }
            let out = stdout(agent, "PermissionRequest", Some("allow"), None);
            assert_eq!(out.as_deref().is_some_and(allows), takes_decisions(agent), "{agent:?}: {out:?}");
        }
    }

    #[test]
    fn each_agent_gets_its_own_reply_shape() {
        let allow = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}"#;
        let deny = r#"{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"deny","message":"Denied from NEXUS"}}}"#;
        for agent in ["", "codex", "claude-desktop", "opencode"] {
            assert_eq!(stdout(agent, "PermissionRequest", Some("allow"), None).unwrap(), allow);
            assert_eq!(stdout(agent, "PermissionRequest", Some("always"), None).unwrap(), allow);
            assert_eq!(stdout(agent, "PermissionRequest", Some("deny"), None).unwrap(), deny);
            assert_eq!(stdout(agent, "PermissionRequest", None, None), None);
            assert_eq!(stdout(agent, "PreToolUse", None, None), None);
        }
        for agent in ["copilot", "muse"] {
            assert_eq!(stdout(agent, "PermissionRequest", Some("allow"), None).unwrap(), r#"{"permissionDecision":"allow"}"#);
            assert_eq!(stdout(agent, "PermissionRequest", Some("deny"), None).unwrap(), r#"{"permissionDecision":"deny"}"#);
        }
        assert_eq!(stdout("copilot", "PermissionRequest", None, None).unwrap(), r#"{"permissionDecision":"ask"}"#);
        assert_eq!(stdout("antigravity", "PreToolUse", None, None).unwrap(), r#"{"decision":"ask"}"#);
        assert_eq!(stdout("antigravity", "PreToolUse", Some("allow"), None).unwrap(), r#"{"decision":"allow"}"#);
        assert_eq!(stdout("hermes", "PermissionRequest", Some("allow"), None), None);
    }

    #[test]
    fn claude_is_the_empty_agent() {
        assert_eq!(reply_agent("claude"), "");
        assert_eq!(reply_agent("codex"), "codex");
        let out = stdout(reply_agent("claude"), "PermissionRequest", Some("allow"), None);
        assert!(out.is_some_and(|o| allows(&o)));
    }

    #[test]
    fn anything_unrecognised_prints_nothing() {
        assert!(decision_json("", None).is_none());
        assert!(decision_json("maybe", None).is_none());
        assert!(decision_json(r#"{"permissionDecision":"allow"}"#, None).is_none());
    }

    #[test]
    fn an_answered_question_goes_back_as_the_same_input_plus_answers() {
        let question = json!({
            "questions": [{ "question": "Which one?", "options": [{ "label": "A" }, { "label": "B" }] }]
        });
        let out = stdout("", "PermissionRequest", Some(r#"{"answers":{"Which one?":"B"}}"#), Some(&question)).unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        let decision = &v["hookSpecificOutput"]["decision"];
        assert_eq!(decision["behavior"], "allow");
        assert_eq!(decision["updatedInput"]["questions"], question["questions"]);
        // Only Claude asks questions: an answer for anyone else is nothing.
        assert!(stdout("codex", "PermissionRequest", Some(r#"{"answers":{"Which one?":"B"}}"#), Some(&question)).is_none());
    }

    #[test]
    fn answers_must_match_the_questions_asked() {
        let q = json!({ "questions": [
            { "question": "Which one?", "options": [{ "label": "A" }, { "label": "B" }] },
            { "question": "Extras?", "multiSelect": true,
              "options": [{ "label": "Tests" }, { "label": "Docs" }, { "label": "Lint" }] }
        ]});
        let ok = |a: &str| decision_json(a, Some(&q)).is_some();
        assert!(ok(r#"{"answers":{"Which one?":"A","Extras?":["Tests","Docs"]}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":"C","Extras?":["Tests"]}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":"A","Extras?":["Tests"],"Other?":"x"}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":"A"}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":["A"],"Extras?":["Tests"]}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":"A","Extras?":[]}}"#));
        assert!(!ok(r#"{"answers":{"Which one?":"A","Extras?":["Tests","Tests"]}}"#));
        assert!(decision_json(r#"{"answers":{"q":"a"}}"#, None).is_none());
    }
}
