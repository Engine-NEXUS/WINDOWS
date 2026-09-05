//! Brain client — calls the Qwen brain server for advanced intent classification.
//!
//! The brain server is a Python sidecar (like STT 39217 and NLU 39218) on port 39219.
//! It runs a Qwen2.5-0.5B-Instruct model that can:
//!   - Classify transcripts the deterministic parser + BERT-Mini miss
//!   - Generate alternative phrasings for training BERT-Mini
//!   - Cross-validate mispronunciations (e.g. "zys" → "zync" via PR number lookup)
//!   - Build a personal pronunciation map that grows over time
//!
//! Admin-only: the brain server is only started when is_admin is true.
//! Always loaded (no idle timeout) — stays in memory for instant responses.

use crate::intent_parser::{ParseResult, ParsedIntent};
use serde::Deserialize;
use std::time::Duration;

/// Brain server port (separate from STT 39217 and NLU 39218).
const BRAIN_PORT: u16 = 39219;

/// Brain server response format.
#[derive(Debug, Deserialize)]
struct BrainResponse {
    intent: String,
    slots: serde_json::Value,
    confidence: f32,
    corrected_repo: Option<String>,
    #[serde(rename = "original_text")]
    _original_text: String,
    #[serde(rename = "corrected_text")]
    _corrected_text: String,
    #[serde(rename = "latency_ms")]
    _latency_ms: f32,
}

/// Phrasing generation response.
#[derive(Debug, Deserialize)]
struct PhrasingResponse {
    phrasings: Vec<String>,
    #[serde(rename = "latency_ms")]
    _latency_ms: f32,
}

/// Pronunciation map response.
#[derive(Debug, Deserialize)]
pub struct PronunciationMapResponse {
    pub map: std::collections::HashMap<String, String>,
    pub count: usize,
}

/// Classify a transcript via the brain server.
///
/// Returns None if the server is not running or the request fails.
/// Returns Some(ParseResult) if the brain returns a valid classification.
pub async fn brain_classify(transcript: &str) -> Option<ParseResult> {
    let url = format!("http://127.0.0.1:{}/classify", BRAIN_PORT);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10)) // brain is slower than NLU (LLM inference)
        .build()
        .ok()?;

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "text": transcript }))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        tracing::debug!("[brain_client] server returned non-success status");
        return None;
    }

    let brain: BrainResponse = response.json().await.ok()?;

    // Convert brain response to ParsedIntent (reuse the same mapping as nlu_client)
    let intent = brain_to_parsed_intent(&brain.intent, &brain.slots)?;

    Some(ParseResult {
        intent,
        confidence: brain.confidence,
        source: "brain".to_string(),
    })
}

/// Generate alternative phrasings for an intent (for training BERT-Mini).
pub async fn brain_generate_phrasings(
    intent: &str,
    slots: &serde_json::Value,
    count: u32,
) -> Option<Vec<String>> {
    let url = format!("http://127.0.0.1:{}/generate_phrasings", BRAIN_PORT);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .ok()?;

    let response = client
        .post(&url)
        .json(&serde_json::json!({
            "intent": intent,
            "slots": slots,
            "count": count,
        }))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        return None;
    }

    let result: PhrasingResponse = response.json().await.ok()?;
    Some(result.phrasings)
}

/// Get the current pronunciation map.
pub async fn brain_get_pronunciation_map() -> Option<PronunciationMapResponse> {
    let url = format!("http://127.0.0.1:{}/pronunciation_map", BRAIN_PORT);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()?;

    let response = client.get(&url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }

    response.json().await.ok()
}

/// Check if the brain server is running.
pub async fn is_brain_available() -> bool {
    let url = format!("http://127.0.0.1:{}/health", BRAIN_PORT);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    match client.get(&url).send().await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// Convert brain server response to ParsedIntent.
/// This reuses the same intent mapping as nlu_client.rs since the brain
/// outputs the same 46-intent schema.
fn brain_to_parsed_intent(intent: &str, slots: &serde_json::Value) -> Option<ParsedIntent> {
    // The brain uses the same intent labels as the NLU server.
    // We can reuse the same mapping logic by calling nlu_client's function.
    // However, nlu_client::nlu_to_parsed_intent is private. So we duplicate
    // the mapping here (or make it public). For now, we handle the most
    // common intents and fall back to Unknown for the rest.
    //
    // In practice, the brain is a fallback — the deterministic parser and
    // BERT-Mini handle most commands. The brain only fires when both miss.
    // So we only need to map the intents that are likely to reach the brain.

    match intent {
        // Local commands
        "open_app" => {
            let target = slots.get("app_name").and_then(|v| v.as_str()).unwrap_or("");
            if target.is_empty() { return None; }
            Some(ParsedIntent::OpenApp { target: target.to_string() })
        }
        "open_url" => {
            let url = slots.get("url").and_then(|v| v.as_str()).unwrap_or("");
            if url.is_empty() { return None; }
            Some(ParsedIntent::OpenUrl { target: url.to_string(), url: url.to_string() })
        }
        "close_app" => {
            let target = slots.get("app_name").and_then(|v| v.as_str()).unwrap_or("");
            if target.is_empty() { return None; }
            Some(ParsedIntent::CloseApp { target: target.to_string() })
        }
        "whatsapp_chat" => {
            let contact = slots.get("contact").and_then(|v| v.as_str()).unwrap_or("");
            if contact.is_empty() { return None; }
            Some(ParsedIntent::WhatsappChat { contact: contact.to_string() })
        }
        "open_architect" => Some(ParsedIntent::OpenArchitect),
        "search" => {
            let query = slots.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if query.is_empty() { return None; }
            Some(ParsedIntent::Search { query: query.to_string() })
        }
        "media_play_pause" => Some(ParsedIntent::MediaPlayPause),
        "media_next" => Some(ParsedIntent::MediaNext),
        "media_previous" => Some(ParsedIntent::MediaPrevious),
        "media_stop" => Some(ParsedIntent::MediaStop),

        // Analysis commands
        "analyse_repo" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            if repo.is_empty() { return None; }
            Some(ParsedIntent::AnalyseRepo { owner, repo: repo.to_string() })
        }
        "analyse_pr" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            let pr_number = slots.get("pr_number").and_then(|v| {
                v.as_u64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            }).unwrap_or(0) as u32;
            if repo.is_empty() || pr_number == 0 { return None; }
            Some(ParsedIntent::AnalysePr { owner, repo: repo.to_string(), pr_number })
        }
        "analyse_latest_pr" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            let author = slots.get("author").and_then(|v| v.as_str()).map(String::from);
            if repo.is_empty() { return None; }
            Some(ParsedIntent::AnalyseLatestPr { owner, repo: repo.to_string(), author })
        }
        "check_branch" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            let author = slots.get("author").and_then(|v| v.as_str()).map(String::from);
            if repo.is_empty() { return None; }
            Some(ParsedIntent::CheckBranch { owner, repo: repo.to_string(), author })
        }

        // GitHub commands — delegate to the same mapping as nlu_client
        // For now, route GitHub commands as Unknown → Worker (the Worker
        // can handle natural language GitHub commands). The brain's main
        // value is pronunciation correction + phrasing generation, not
        // GitHub command parsing (the deterministic parser handles those).
        "merge_pr" | "approve_pr" | "close_pr" | "list_prs" | "get_pr" |
        "create_pr" | "update_branch" | "revert_pr" | "list_pr_files" |
        "comment_pr" | "add_collaborator" | "remove_collaborator" |
        "list_collaborators" | "add_org_member" | "remove_org_member" |
        "list_org_members" | "delete_branch" | "list_branches" |
        "create_release" | "list_releases" | "list_workflows" |
        "list_workflow_runs" | "rerun_workflow" | "cancel_workflow" => {
            // These are better handled by the deterministic parser.
            // If the brain classifies one, return Unknown so the Worker
            // can handle it as a natural language query.
            None
        }

        "greeting" => None, // greetings are handled locally
        "unknown" => None,
        _ => None,
    }
}
