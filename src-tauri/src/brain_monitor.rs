//! Brain monitor — watches every transcript and auto-trains BERT-Mini.
//!
//! This is the continuous learning loop:
//!   1. Every transcript passes through the brain (in background, non-blocking)
//!   2. The brain classifies it + generates phrasings
//!   3. 3-gate auto-approval decides if phrasings go into training data
//!   4. Every 50 new approved examples, triggers a background retrain
//!   5. New model must score ≥ old model or it's discarded (rollback safety)
//!
//! Admin-only: the monitor only runs when is_admin is true.
//! Non-blocking: the monitor runs in a tokio task, never blocks the main pipeline.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::sync::Mutex;

use crate::brain_client;

/// Number of approved examples needed to trigger a retrain.
const RETRAIN_THRESHOLD: u32 = 50;

/// Minimum brain confidence for auto-approval.
const MIN_CONFIDENCE: f32 = 0.90;

static PENDING_COUNT: AtomicU32 = AtomicU32::new(0);

/// Approved phrasing entry (written to approved_phrasings.jsonl).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ApprovedPhrasing {
    text: String,
    intent: String,
    slots: serde_json::Value,
    source: String,
    brain_confidence: f32,
    timestamp: f64,
}

/// Gap entry (written to gaps.jsonl when the brain disagrees with BERT-Mini).
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GapEntry {
    transcript: String,
    deterministic_result: Option<String>,
    nlu_result: Option<String>,
    brain_result: String,
    brain_confidence: f32,
    timestamp: f64,
}

/// Get the path to the brain data directory.
fn brain_data_dir() -> PathBuf {
    let base = dirs_next::data_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("com.nexus.assistant").join("brain")
}

/// Get the path to approved_phrasings.jsonl.
fn approved_phrasings_path() -> PathBuf {
    brain_data_dir().join("approved_phrasings.jsonl")
}

/// Get the path to gaps.jsonl.
fn gaps_path() -> PathBuf {
    brain_data_dir().join("gaps.jsonl")
}

/// Ensure the brain data directory exists.
fn ensure_data_dir() {
    let dir = brain_data_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
    }
}

/// Append a JSON line to a file.
fn append_jsonl(path: &PathBuf, entry: &impl Serialize) {
    ensure_data_dir();
    if let Ok(line) = serde_json::to_string(entry) {
        // Use OpenOptions to append
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            use std::io::Write;
            writeln!(file, "{}", line).ok();
        }
    }
}

/// Monitor a transcript in the background.
///
/// This is called after the deterministic parser + BERT-Mini have processed
/// the transcript. The brain watches the result and learns from it.
///
/// NON-BLOCKING: spawns a tokio task, returns immediately.
pub fn monitor_transcript(
    transcript: String,
    deterministic_intent: Option<String>,
    nlu_intent: Option<String>,
) {
    // Don't await — fire and forget
    tokio::spawn(async move {
        monitor_transcript_inner(transcript, deterministic_intent, nlu_intent).await;
    });
}

/// Inner monitor logic — runs in a background task.
async fn monitor_transcript_inner(
    transcript: String,
    deterministic_intent: Option<String>,
    nlu_intent: Option<String>,
) {
    // 1. Classify via brain
    let brain_result = match brain_client::brain_classify(&transcript).await {
        Some(r) => r,
        None => return, // brain not available, skip
    };

    let brain_intent_name = format!("{:?}", brain_result.intent);
    let brain_confidence = brain_result.confidence;

    // 2. Check for gaps (brain disagrees with both deterministic and NLU)
    let det_matches = deterministic_intent
        .as_ref()
        .map(|d| d == &brain_intent_name)
        .unwrap_or(false);
    let nlu_matches = nlu_intent
        .as_ref()
        .map(|n| n == &brain_intent_name)
        .unwrap_or(false);

    if !det_matches && !nlu_matches && brain_confidence > 0.8 {
        // Brain disagrees with both — log a gap
        let gap = GapEntry {
            transcript: transcript.clone(),
            deterministic_result: deterministic_intent.clone(),
            nlu_result: nlu_intent.clone(),
            brain_result: brain_intent_name.clone(),
            brain_confidence,
            timestamp: chrono::Utc::now().timestamp() as f64,
        };
        append_jsonl(&gaps_path(), &gap);
        tracing::info!(
            "[brain_monitor] gap detected: transcript='{}' brain='{}' conf={:.2}",
            transcript,
            brain_intent_name,
            brain_confidence
        );
    }

    // 3. Auto-approval gate (3 checks)
    let approved = auto_approve(
        &brain_intent_name,
        brain_confidence,
        &deterministic_intent,
        &nlu_intent,
    );

    if !approved {
        return;
    }

    // 4. Generate phrasings for the approved intent
    let slots = serde_json::to_value(&brain_result.intent).unwrap_or(serde_json::Value::Null);
    let phrasings = brain_client::brain_generate_phrasings(&brain_intent_name, &slots, 20).await;

    if let Some(phrasings) = phrasings {
        let phrasing_count = phrasings.len();
        for phrasing in phrasings {
            let entry = ApprovedPhrasing {
                text: phrasing,
                intent: brain_intent_name.clone(),
                slots: slots.clone(),
                source: "brain_auto".to_string(),
                brain_confidence,
                timestamp: chrono::Utc::now().timestamp() as f64,
            };
            append_jsonl(&approved_phrasings_path(), &entry);
        }

        let count = PENDING_COUNT.fetch_add(phrasing_count as u32, Ordering::Relaxed) + phrasing_count as u32;
        tracing::info!(
            "[brain_monitor] auto-approved {} phrasings for '{}' (total pending: {})",
            phrasing_count,
            brain_intent_name,
            count
        );

        // 5. Check if we should trigger a retrain
        if count >= RETRAIN_THRESHOLD {
            PENDING_COUNT.store(0, Ordering::Relaxed);
            trigger_retrain().await;
        }
    }
}

/// 3-gate auto-approval logic.
///
/// Gate 1: Brain confidence ≥ 0.90
/// Gate 2: Brain intent matches deterministic OR NLU (cross-validation)
/// Gate 3: Slot values are valid (checked in brain_client during classification)
fn auto_approve(
    brain_intent: &str,
    brain_confidence: f32,
    deterministic_intent: &Option<String>,
    nlu_intent: &Option<String>,
) -> bool {
    // Gate 1: confidence check
    if brain_confidence < MIN_CONFIDENCE {
        tracing::debug!(
            "[brain_monitor] gate 1 failed: confidence {:.2} < {:.2}",
            brain_confidence,
            MIN_CONFIDENCE
        );
        return false;
    }

    // Gate 2: cross-validation
    // The brain's classification must agree with EITHER the deterministic
    // parser OR BERT-Mini. If all three disagree, something is ambiguous.
    //
    // Exception: if the deterministic parser returned None AND NLU returned
    // None (both missed), we trust the brain if it's confident enough.
    let det_matches = deterministic_intent
        .as_ref()
        .map(|d| intent_names_match(d, brain_intent))
        .unwrap_or(false);
    let nlu_matches = nlu_intent
        .as_ref()
        .map(|n| intent_names_match(n, brain_intent))
        .unwrap_or(false);

    let both_missed = deterministic_intent.is_none() && nlu_intent.is_none();

    if !det_matches && !nlu_matches && !both_missed {
        tracing::debug!(
            "[brain_monitor] gate 2 failed: brain='{}' det={:?} nlu={:?}",
            brain_intent,
            deterministic_intent,
            nlu_intent
        );
        return false;
    }

    // Gate 3: slot validation is done in brain_client during classification
    // (the brain server validates slots before returning)

    true
}

/// Check if two intent names match (handling minor format differences).
fn intent_names_match(a: &str, b: &str) -> bool {
    // Normalize both to lowercase snake_case
    let normalize = |s: &str| -> String {
        s.to_lowercase()
            .replace("parsedintent::", "")
            .replace("githubcommand::", "")
    };
    normalize(a) == normalize(b)
}

/// Trigger a background retrain of BERT-Mini.
///
/// This runs the merge_and_train.py script which:
///   1. Merges approved_phrasings.jsonl into dataset.json
///   2. Deduplicates
///   3. Balances classes
///   4. Runs train.py
///   5. Compares new model accuracy vs old
///   6. Hot-swaps if new ≥ old, discards if new < old
async fn trigger_retrain() {
    tracing::info!("[brain_monitor] triggering background retrain...");

    // Run the retrain script in a blocking task
    let result = tokio::task::spawn_blocking(|| {
        let script = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("resources").join("server").join("nlu").join("merge_and_train.py")))
            .unwrap_or_else(|| PathBuf::from("server/nlu/merge_and_train.py"));

        std::process::Command::new("python")
            .arg(&script)
            .output()
    })
    .await;

    match result {
        Ok(Ok(output)) => {
            if output.status.success() {
                tracing::info!("[brain_monitor] retrain completed successfully");
                // The retrain script handles hot-swapping the ONNX model
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::warn!("[brain_monitor] retrain failed: {}", stderr.chars().take(200).collect::<String>());
            }
        }
        Ok(Err(e)) => {
            tracing::warn!("[brain_monitor] failed to run retrain script: {}", e);
        }
        Err(e) => {
            tracing::warn!("[brain_monitor] retrain task panicked: {}", e);
        }
    }
}

/// Get the current pending count (for UI display).
pub fn pending_count() -> u32 {
    PENDING_COUNT.load(Ordering::Relaxed)
}

/// Get the number of approved phrasings in the file.
pub fn approved_phrasings_count() -> usize {
    let path = approved_phrasings_path();
    if !path.exists() {
        return 0;
    }
    std::fs::read_to_string(&path)
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}

/// Get the number of gaps in the file.
pub fn gaps_count() -> usize {
    let path = gaps_path();
    if !path.exists() {
        return 0;
    }
    std::fs::read_to_string(&path)
        .map(|s| s.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0)
}
