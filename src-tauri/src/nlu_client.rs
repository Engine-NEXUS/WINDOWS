//! NLU client — calls the Python NLU server (BERT-Mini) for intent classification.
//!
//! The NLU server is a lazy-started Python sidecar (like the STT server).
//! It loads a BERT-Mini ONNX model and provides a /parse endpoint that
//! returns intent + slots + confidence.
//!
//! If the NLU server is not running or unavailable, this module returns None
//! and the caller falls back to the deterministic parser or unknown intent.

use crate::intent_parser::{ParseResult, ParsedIntent};
use serde::Deserialize;
use std::time::Duration;

/// NLU server port (separate from STT port 39217).
const NLU_PORT: u16 = 39218;

/// NLU server response format.
#[derive(Debug, Deserialize)]
struct NluResponse {
    intent: String,
    slots: serde_json::Value,
    confidence: f32,
}

/// Parse a transcript via the NLU server.
///
/// Returns None if the server is not running or the request fails.
/// Returns Some(ParseResult) if the server returns a valid classification.
pub async fn parse_via_nlu(transcript: &str) -> Option<ParseResult> {
    // Ensure the NLU server is running (lazy-start)
    crate::lazy_nlu::ensure_nlu_running();

    let url = format!("http://127.0.0.1:{}/parse", NLU_PORT);

    // Short timeout — if NLU is slow, fall back to deterministic
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .ok()?;

    let response = client
        .post(&url)
        .json(&serde_json::json!({ "text": transcript }))
        .send()
        .await
        .ok()?;

    if !response.status().is_success() {
        tracing::debug!("[nlu_client] server returned non-success status");
        return None;
    }

    let nlu: NluResponse = response.json().await.ok()?;

    // Mark that a request was made (resets idle timer)
    crate::lazy_nlu::mark_nlu_request();

    // Convert NLU response to ParsedIntent
    let intent = nlu_to_parsed_intent(&nlu.intent, &nlu.slots)?;

    Some(ParseResult {
        intent,
        confidence: nlu.confidence,
        source: "nlu".to_string(),
    })
}

/// Convert NLU server response to ParsedIntent.
fn nlu_to_parsed_intent(intent: &str, slots: &serde_json::Value) -> Option<ParsedIntent> {
    match intent {
        "open_app" => {
            let target = slots
                .get("app_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if target.is_empty() {
                return None;
            }
            Some(ParsedIntent::OpenApp {
                target: target.to_string(),
            })
        }
        "analyse_repo" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            if repo.is_empty() {
                return None;
            }
            Some(ParsedIntent::AnalyseRepo {
                owner,
                repo: repo.to_string(),
            })
        }
        "analyse_pr" => {
            let repo = slots.get("repo").and_then(|v| v.as_str()).unwrap_or("");
            let owner = slots.get("owner").and_then(|v| v.as_str()).map(String::from);
            let pr_number = slots
                .get("pr_number")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as u32;
            if repo.is_empty() || pr_number == 0 {
                return None;
            }
            Some(ParsedIntent::AnalysePr {
                owner,
                repo: repo.to_string(),
                pr_number,
            })
        }
        "search" => {
            let query = slots.get("query").and_then(|v| v.as_str()).unwrap_or("");
            if query.is_empty() {
                return None;
            }
            Some(ParsedIntent::Search {
                query: query.to_string(),
            })
        }
        "open_architect" => Some(ParsedIntent::OpenArchitect),
        "media_play_pause" => Some(ParsedIntent::MediaPlayPause),
        "media_next" => Some(ParsedIntent::MediaNext),
        "media_previous" => Some(ParsedIntent::MediaPrevious),
        "media_stop" => Some(ParsedIntent::MediaStop),
        _ => None,
    }
}

/// Check if the NLU server is running.
pub async fn is_nlu_available() -> bool {
    let url = format!("http://127.0.0.1:{}/health", NLU_PORT);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_millis(200))
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
