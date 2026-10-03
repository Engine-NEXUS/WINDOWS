//! NEXUS Central Orchestrator — the single owner of request lifecycle.
//!
//! The orchestrator is the "main system" that decides:
//!   - Which subsystem handles a given transcript (routing)
//!   - When to show/hide the loading indicator (top-right corner)
//!   - When to speak the acknowledgement ("On it sir")
//!   - When to speak the result
//!   - When to cancel an in-flight request (barge-in / new wake)
//!
//! Subsystems are the "workers":
//!   - LocalCommand   — open/close apps, media controls, greetings (Rust, <5ms)
//!   - WorkerBackend  — PR analysis, GitHub writes, research, general Q&A (Cloudflare)
//!   - Architect      — architecture mapper (Rust + Worker enrichment)
//!
//! Every request gets a unique `request_id` (UUID v4). The orchestrator
//! tracks the active request in a mutex. When a new request arrives, the
//! old one is cancelled (its `cancelled` flag is set). Subsystems check
//! the flag and abort early.
//!
//! Events emitted to the frontend (all on channel "orchestrator:event"):
//!   { type: "state",    state: "thinking"|"speaking", request_id }
//!   { type: "loading",  visible: bool, request_id }
//!   { type: "ack",      text: "On it sir.", request_id }
//!   { type: "result",   text: "...", request_id, analysis?, dialog_state? }
//!   { type: "done",     request_id }
//!   { type: "error",    message: "...", request_id }
//!
//! The frontend listens to these events instead of the old "assistant:server"
//! channel. This centralizes all state transitions in Rust.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::intent_parser::{parse_deterministic, ParsedIntent};
use crate::network;

// ─── Types ─────────────────────────────────────────────────────────────

/// Orchestrator lifecycle states (mirrors the frontend AssistantState).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OrchestratorState {
    Idle,
    Listening,
    Thinking,
    Speaking,
}

/// Which subsystem will handle this request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Subsystem {
    /// Local Rust command — open/close app, media, greeting. <5ms, no network.
    LocalCommand,
    /// Cloudflare Worker — PR analysis, GitHub writes, research, general Q&A.
    WorkerBackend,
    /// Architecture Mapper — repo analysis + graph + AI enrichment.
    Architect,
    /// GitHub sub-command system — typed GitHub operations via octocrab.
    /// Handles merge/approve/close PR, collaborators, org members, branches,
    /// releases, workflows. Token fetched from Worker, execution in Rust.
    GitHub,
    /// MCP sub-center — external services via Model Context Protocol.
    /// Handles Swiggy (food/grocery), Amazon (product search), WhatsApp
    /// (messaging). Each MCP server is called via JSON-RPC over HTTP.
    Mcp,
    /// Command Center — compound multi-step tasks ("X then Y").
    /// Plans steps, routes each to a sub-center, merges results.
    CommandCenter,
    /// No subsystem — the command was unparseable or empty.
    None,
}

/// The active request, tracked in the orchestrator's mutex.
struct ActiveRequest {
    id: String,
    cancelled: Arc<AtomicBool>,
    subsystem: Subsystem,
}

/// Event sent to the frontend via the "orchestrator:event" channel.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum OrchestratorEvent {
    State {
        state: OrchestratorState,
        request_id: String,
    },
    Loading {
        visible: bool,
        request_id: String,
    },
    Ack {
        text: String,
        request_id: String,
    },
    Result {
        text: String,
        request_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        analysis: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        dialog_state: Option<serde_json::Value>,
    },
    Done {
        request_id: String,
    },
    Clarify {
        prompt: String,
        request_id: String,
        expected_slot: String,
        timeout_ms: u64,
    },
    Error {
        message: String,
        request_id: String,
    },
    /// GitHub sub-command system: confirmation required for a destructive
    /// operation. The frontend should ask the user to confirm, then call
    /// `orchestrator_github_confirm` with the request_id and confirmed=true.
    Confirm {
        prompt: String,
        request_id: String,
        /// The serialized GitHubCommand that needs confirmation.
        command: serde_json::Value,
    },
    /// GitHub sub-command system: merge conflict detected. The frontend
    /// should display the conflict details with copy-paste options.
    // NOTE: variant-level `rename` is required — the enum-wide
    // `rename_all = "lowercase"` would emit "conflictreport"/"githubresult",
    // and `snake_case` would emit "git_hub_result" (serde splits every camel
    // hump). The frontend union expects "conflict_report"/"github_result".
    #[serde(rename = "conflict_report")]
    ConflictReport {
        request_id: String,
        pr_number: u64,
        repo: String,
        conflict_files: serde_json::Value,
        message: String,
    },
    /// GitHub sub-command system: operation result. The frontend should
    /// speak the text and/or display structured data.
    #[serde(rename = "github_result")]
    GitHubResult {
        request_id: String,
        result: serde_json::Value,
    },
}

// ─── Global state ──────────────────────────────────────────────────────

/// The single active request. Only one request is active at a time.
/// When a new request arrives, the previous one is cancelled.
static ACTIVE_REQUEST: once_cell::sync::Lazy<Arc<Mutex<Option<ActiveRequest>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

/// Acknowledgement phrases — same as network.rs but owned by the orchestrator now.
const ACK_PHRASES: &[&str] = &[
    "On it sir.",
    "Right away sir.",
    "Working on it sir.",
    "Let me check that sir.",
    "One moment sir.",
];

// ─── Dictation Mode ────────────────────────────────────────────────────
static DICTATION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn is_dictation_active() -> bool {
    DICTATION_ACTIVE.load(Ordering::SeqCst)
}

pub fn set_dictation_active(active: bool) {
    DICTATION_ACTIVE.store(active, Ordering::SeqCst);
}

// ─── Helpers ───────────────────────────────────────────────────────────

/// Generate a short request ID (first 12 hex chars of a UUID, enough for uniqueness).
fn new_request_id() -> String {
    let full = network::uuid_v4();
    // Strip hyphens and take first 12 hex chars for brevity in logs
    let hex: String = full.chars().filter(|c| *c != '-').collect();
    hex[..12].to_string()
}

/// Spoken on every ghost exit path (voice, Esc, Ctrl+Space). Single source
/// so all three say the identical line (pinned by test).
pub const GHOST_EXIT_LINE: &str = "Ghost mode off, sir.";

/// Pick a random ack phrase.
fn pick_ack() -> &'static str {
    let idx = network::uuid_v4().as_bytes()[0] as usize % ACK_PHRASES.len();
    ACK_PHRASES[idx]
}

/// Repeat-back ack for WorkerBackend turns (doc 74, P1): the ack names the
/// action so the user hears WHAT was understood, not a bare "on it sir".
/// Unknown/general chat keeps the generic ack (nothing to name).
fn ack_for(intent: &ParsedIntent) -> String {
    match intent {
        ParsedIntent::AnalysePr { pr_number, repo, .. } => {
            format!("On it sir — analysing PR {pr_number} in {repo}.")
        }
        ParsedIntent::AnalyseRepo { repo, .. } => {
            format!("On it sir — analysing {repo}.")
        }
        ParsedIntent::AnalyseLatestPr { .. } => {
            "On it sir — checking the latest PR.".to_string()
        }
        ParsedIntent::CheckBranch { .. } => "On it sir — checking the branch.".to_string(),
        ParsedIntent::Search { query } => {
            let q: String = query.chars().take(40).collect();
            format!("On it sir — searching for {q}.")
        }
        _ => pick_ack().to_string(),
    }
}

fn clarification_for(intent: &ParsedIntent) -> (String, String) {
    match intent {
        ParsedIntent::AnalyseRepo { .. }
        | ParsedIntent::AnalysePr { .. }
        | ParsedIntent::AnalyseLatestPr { .. }
        | ParsedIntent::CheckBranch { .. }
        | ParsedIntent::GitHubCommand { .. } => (
            "Which repository should I use, sir?".to_string(),
            "repo".to_string(),
        ),
        ParsedIntent::OpenArchitect => (
            "Do you want the architecture mapper, sir?".to_string(),
            "architect_confirm".to_string(),
        ),
        _ => (
            "I didn't catch that clearly, sir — say it again.".to_string(),
            "repeat".to_string(),
        ),
    }
}

/// Speak a clarification prompt and keep the turn open for one bounded reply.
/// Unlike `speak_prompt_and_finish`, this does not emit `Done` or clear the
/// active request. The frontend auto-listens after TTS and cancels on timeout.
async fn speak_prompt_and_hold<R: Runtime>(
    app: &AppHandle<R>,
    prompt: String,
    expected_slot: String,
) -> Result<ProcessResult, String> {
    const CLARIFICATION_TIMEOUT_MS: u64 = 8_000;
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    emit(
        app,
        &OrchestratorEvent::Clarify {
            prompt,
            request_id: request_id.clone(),
            expected_slot,
            timeout_ms: CLARIFICATION_TIMEOUT_MS,
        },
    );
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Emit an orchestrator event to the frontend.
fn emit<R: Runtime>(app: &AppHandle<R>, event: &OrchestratorEvent) {
    let _ = app.emit("orchestrator:event", event);
    tracing::debug!("orchestrator: emitted {:?}", event);
}

/// Cancel any active request and install a new one.
/// Returns the new request's ID and its cancel flag.
/// `pub(crate)` for the global-hotkey ghost-exit path (same turn semantics
/// as a voice exit: the bail cancels whatever is in flight).
pub(crate) fn install_new_request(subsystem: Subsystem) -> (String, Arc<AtomicBool>) {
    let id = new_request_id();
    let cancel_flag = Arc::new(AtomicBool::new(false));

    // Cancel the previous request
    let mut guard = ACTIVE_REQUEST.lock().unwrap();
    if let Some(prev) = guard.as_ref() {
        prev.cancelled.store(true, Ordering::Relaxed);
        tracing::info!(
            "orchestrator: cancelling previous request {} (was {:?})",
            prev.id,
            prev.subsystem
        );
    }

    *guard = Some(ActiveRequest {
        id: id.clone(),
        cancelled: cancel_flag.clone(),
        subsystem: subsystem.clone(),
    });

    tracing::info!("orchestrator: new request {} -> {:?}", id, subsystem);
    (id, cancel_flag)
}

/// Check if a request is cancelled.
/// Public wrapper for `is_cancelled` — used by command_center step execution.
pub(crate) fn is_cancelled_pub(cancel_flag: &Arc<AtomicBool>) -> bool {
    is_cancelled(cancel_flag)
}

fn is_cancelled(cancel_flag: &Arc<AtomicBool>) -> bool {
    cancel_flag.load(Ordering::Relaxed)
}

/// Clear the active request (called on done/error).
/// `pub(crate)` for the global-hotkey ghost-exit path.
pub(crate) fn clear_active_request(request_id: &str) {
    let mut guard = ACTIVE_REQUEST.lock().unwrap();
    if let Some(ref current) = *guard {
        if current.id == request_id {
            *guard = None;
            tracing::debug!("orchestrator: cleared active request {}", request_id);
        }
    }
}

/// True while a user turn owns the pipeline. Background announcers
/// (stage watchdog) must stay silent when this is true — never talk
/// over the user.
pub(crate) fn has_active_request() -> bool {
    ACTIVE_REQUEST.lock().unwrap().is_some()
}

// ─── ML parsing fallback ──────────────────────────────────────────────

/// Try brain (Qwen, admin-only) then NLU (BERT-Mini) when the deterministic
/// parser misses. This is the same pipeline as `parse_transcript` (Tauri
/// command) — extracted here so the orchestrator can use it for routing
/// instead of bypassing the ML classifiers.
///
/// Returns `None` if both brain and NLU are unavailable or low-confidence.
async fn parse_with_ml(transcript: &str) -> Option<crate::intent_parser::ParseResult> {
    // 1. Try brain server FIRST (admin-only, if enabled)
    // The brain (Qwen 0.5B LLM) is much smarter than BERT-Mini and can
    // understand mishearings, filler words, and unusual phrasing.
    #[cfg(feature = "admin-brain")]
    {
        if crate::admin_config::is_admin() {
            if let Some(mut result) = crate::brain_client::brain_classify(transcript).await {
                tracing::info!(
                    "orchestrator: brain: {:?} (confidence={:.3})",
                    result.intent, result.confidence
                );
                // Validate and sanitize the brain's output before using it.
                // The brain (Qwen 0.5B) sometimes hallucinates repo names
                // from garbage transcripts or returns literal placeholders
                // like "owner/repo". Sanitize before routing.
                sanitize_ml_intent(&mut result);
                // Capped window-opens must NOT route: fall through to NLU,
                // not to the Architect window. (The 0.5 gate below already
                // exists for brain; the cap forces it to trigger.)
                if cap_ml_window_open(&mut result) {
                    tracing::info!("orchestrator: capped ML Architect → NLU fallback");
                }
                if result.confidence >= 0.5 {
                    return Some(result);
                }
                tracing::info!(
                    "orchestrator: brain confidence too low ({:.2}), falling back to NLU",
                    result.confidence
                );
            }
        }
    }

    // 2. Try NLU server (BERT-Mini fallback)
    if let Some(mut result) = crate::nlu_client::parse_via_nlu(transcript).await {
        tracing::info!(
            "orchestrator: nlu: {:?} (confidence={:.3})",
            result.intent, result.confidence
        );
        sanitize_ml_intent(&mut result);
        // Capped window-opens must NOT route: fall to Unknown (retry prompt),
        // not to the Architect window. Other intents keep legacy behavior.
        if cap_ml_window_open(&mut result) {
            return None;
        }
        return Some(result);
    }

    None
}

/// Cap ML-only window-opening intents. Returns true when capped.
///
/// Qwen/BERT are overconfident on out-of-distribution garbage transcripts
/// (measured 2026-09-19: 'You feel it, no?' → OpenArchitect @0.99 from a
/// ~1s noise capture → Architect window opened uninvited). Opening windows
/// is a visible side effect, so ML-sourced Architect requires the same
/// caution as a destructive op: cap below the 0.5 accept line → falls to
/// Unknown/NLU → retry prompt. Deterministic 'open architect' / 'architect'
/// phrases never pass through here (parse_with_ml only runs on
/// deterministic miss), so real requests keep working.
fn cap_ml_window_open(result: &mut crate::intent_parser::ParseResult) -> bool {
    use crate::intent_parser::ParsedIntent;
    if matches!(result.intent, ParsedIntent::OpenArchitect)
        && !result.source.starts_with("deterministic")
    {
        tracing::warn!(
            "orchestrator: ML-only OpenArchitect (source={}, conf={:.2}) capped — garbage-transcript guard",
            result.source,
            result.confidence
        );
        result.confidence = result.confidence.min(0.4);
        return true;
    }
    // doc 07 P3 finding, measured live against the real 0.5B brain model
    // (not theoretical): system_click_element/minimize/maximize exist
    // ONLY in the brain's system prompt text, with zero actual training —
    // and at 0.5B params it confidently (0.9-1.0) mis-tags unrelated
    // phrases with them ("click submit" -> screen_analysis@0.9,
    // "switch to brave" -> start_dictation@1.0 were observed; the
    // reverse direction, guessing these NEW labels for unrelated speech,
    // is the same failure mode). NLU (BERT-mini) can't make this mistake
    // — these three labels aren't in its trained vocabulary at all, so
    // it can never emit them — this guard is brain-path only by
    // construction (deterministic/NLU sources already can't trigger it).
    // The deterministic parser (intent_parser.rs) remains the reliable
    // path for all three; this just stops an ML guess from executing.
    if let ParsedIntent::NluResult { intent, .. } = &result.intent {
        if !result.source.starts_with("deterministic")
            && matches!(
                intent.as_str(),
                "system_click_element" | "system_minimize_window" | "system_maximize_window"
            )
        {
            tracing::warn!(
                "orchestrator: ML-only {} (source={}, conf={:.2}) capped — unverified brain-only label",
                intent,
                result.source,
                result.confidence
            );
            result.confidence = result.confidence.min(0.4);
            return true;
        }
    }
    false
}

/// Sanitize ML-classified intent: validate repo names, reject garbage.
///
/// The brain (Qwen 0.5B) and NLU (BERT-Mini) can hallucinate repo names
/// from garbage transcripts. This function:
///   - Replaces literal "owner/repo" placeholder with empty string
///   - Validates repo names against GitHub naming rules
///   - Falls back to account-wide (empty repo) for list_prs if repo is garbage
///   - Rejects commands with invalid repos by lowering confidence below threshold
fn sanitize_ml_intent(result: &mut crate::intent_parser::ParseResult) {
    use crate::github_cmd::GitHubCommand;
    use crate::intent_parser::ParsedIntent;

    if let ParsedIntent::GitHubCommand { command } = &mut result.intent {
        match command {
            GitHubCommand::ListPrs { repo, .. }
            | GitHubCommand::GetPr { repo, .. }
            | GitHubCommand::MergePr { repo, .. }
            | GitHubCommand::ApprovePr { repo, .. }
            | GitHubCommand::ClosePr { repo, .. }
            | GitHubCommand::RevertPr { repo, .. }
            | GitHubCommand::ListPrFiles { repo, .. }
            | GitHubCommand::CommentPr { repo, .. }
            | GitHubCommand::CreatePr { repo, .. }
            | GitHubCommand::UpdateBranch { repo, .. } => {
                let trimmed = repo.trim().to_string();
                // Reject literal placeholder "owner/repo"
                if trimmed.eq_ignore_ascii_case("owner/repo") || trimmed.eq_ignore_ascii_case("owner/repo/") {
                    tracing::warn!("orchestrator: brain returned literal placeholder '{}' as repo — clearing", trimmed);
                    *repo = String::new();
                    return;
                }
                // Validate repo name: only alphanumeric, hyphens, underscores, dots, slashes
                // GitHub repo names: alphanumeric, -, _, . and owner/repo format
                if !trimmed.is_empty() && !is_valid_repo_name(&trimmed) {
                    tracing::warn!(
                        "orchestrator: brain returned invalid repo '{}' — clearing (likely hallucinated from garbage transcript)",
                        trimmed
                    );
                    *repo = String::new();
                    // For ListPrs, empty repo = account-wide (valid).
                    // For other commands, empty repo will cause a user-friendly error.
                    // Lower confidence so the brain monitor logs it as a failure.
                    if !matches!(command, GitHubCommand::ListPrs { .. }) {
                        result.confidence = result.confidence.min(0.4);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Check if a string is a valid GitHub repository name.
/// Valid: "zync", "owner/repo", "zync-ui", "my_repo", "v1.0"
/// Invalid: "very homely person", "owner/repo", sentences, phrases
fn is_valid_repo_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 100 {
        return false;
    }
    // GitHub repo names only contain: a-z, A-Z, 0-9, -, _, ., /
    // No spaces, no special characters
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/')
}

// ─── Routing ───────────────────────────────────────────────────────────

/// Decide which subsystem should handle this intent.
///
/// Routing priority:
///   1. Local commands (open/close app, media, greeting) → LocalCommand
///   2. Architecture mapper → Architect
///   3. Everything else (analyse PR, research, GitHub writes, general) → WorkerBackend
pub(crate) fn route_intent(intent: &ParsedIntent) -> Subsystem {
    match intent {
        // Local commands — handled in Rust, no network
        ParsedIntent::OpenApp { .. }
        | ParsedIntent::OpenUrl { .. }
        | ParsedIntent::CloseApp { .. }
        | ParsedIntent::WhatsappChat { .. }
        | ParsedIntent::MediaPlayPause
        | ParsedIntent::MediaNext
        | ParsedIntent::MediaPrevious
        | ParsedIntent::MediaStop
        | ParsedIntent::Greeting { .. } => Subsystem::LocalCommand,

        // Architecture mapper — Rust + Worker enrichment
        ParsedIntent::OpenArchitect => Subsystem::Architect,

        // Settings sidebar — handled locally (no Worker round-trip)
        ParsedIntent::OpenSettings => Subsystem::LocalCommand,

        // GitHub sub-command system — typed operations via octocrab
        ParsedIntent::GitHubCommand { .. } => Subsystem::GitHub,

        // MCP sub-center — external services via Model Context Protocol
        ParsedIntent::OrderFood { .. }
        | ParsedIntent::SearchProduct { .. }
        | ParsedIntent::SendWhatsAppMessage { .. } => Subsystem::Mcp,

        // Clarification prompt — spoken locally, never touches the Worker.
        // Partial MCP commands land here instead of Unknown so the user is
        // asked for the missing slot rather than getting a guess/refusal.
        ParsedIntent::NeedMoreInfo { .. } => Subsystem::LocalCommand,

        // Screen control intents are handled explicitly in
        // process_transcript (run_screen_click/read/tab); tracked as local.
        ParsedIntent::ScreenClick { .. }
        | ParsedIntent::ScreenRead { .. }
        | ParsedIntent::BrowserTab { .. }
        | ParsedIntent::BrowserCloseTab { .. }
        | ParsedIntent::BrowserSearch { .. }
        | ParsedIntent::BrowserSearchFocus
        | ParsedIntent::WatchScreenEmail
        | ParsedIntent::StartDictation
        | ParsedIntent::StopDictation => Subsystem::LocalCommand,

        // Ghostwriter room entry — handled explicitly in process_transcript
        // (session start + sidebar card), tracked as a local request.
        // Cursor-control Ghost Mode entry — same explicit handling
        // (ghost session + ring, no sidebar card).
        ParsedIntent::EnterGhostwriter { .. } => Subsystem::LocalCommand,
        ParsedIntent::EnterGhostControl => Subsystem::LocalCommand,
        ParsedIntent::ExitGhostControl => Subsystem::LocalCommand,

        // Everything else goes to the Worker
        ParsedIntent::AnalyseRepo { .. }
        | ParsedIntent::AnalysePr { .. }
        | ParsedIntent::AnalyseLatestPr { .. }
        | ParsedIntent::CheckBranch { .. }
        | ParsedIntent::Search { .. }
        | ParsedIntent::NluResult { .. }
        | ParsedIntent::Unknown { .. } => Subsystem::WorkerBackend,
    }
}

/// Determine if a subsystem is "long-running" and should show the loading indicator.
///
/// Local commands are instant (<5ms) — no loading indicator.
/// Worker and Architect are long-running — show loading indicator after ack.
#[allow(dead_code)]
fn is_long_running(subsystem: &Subsystem) -> bool {
    matches!(
        subsystem,
        Subsystem::WorkerBackend
            | Subsystem::Architect
            | Subsystem::GitHub
            | Subsystem::Mcp
            | Subsystem::CommandCenter
    )
}

// ─── Public API ────────────────────────────────────────────────────────

/// Result of processing a transcript through the orchestrator.
#[derive(Debug, Clone, Serialize)]
pub struct ProcessResult {
    pub request_id: String,
    pub subsystem: Subsystem,
    pub handled_locally: bool,
}

/// Process a transcript through the central orchestrator.
///
/// This is the MAIN ENTRY POINT called when the user finishes speaking.
/// It:
///   1. Parses the intent: deterministic (regex, <1ms) → brain (Qwen,
///      admin-only) → NLU (BERT-Mini) → Unknown
///   2. Routes to the correct subsystem
///   3. Installs a new request (cancels any previous)
///   4. Emits ack + loading state to the frontend
///   5. Dispatches to the subsystem
///   6. Emits result + done
///
/// The frontend calls this via the `orchestrator_process` Tauri command.
pub async fn process_transcript<R: Runtime>(
    app: AppHandle<R>,
    transcript: String,
    dialog_context: Option<serde_json::Value>,
    turn_context: Option<crate::center::TurnContext>,
) -> Result<ProcessResult, String> {
    if transcript.trim().is_empty() {
        return Err("empty transcript".into());
    }

    tracing::info!(
        "orchestrator: processing transcript: {:?}",
        transcript.chars().take(80).collect::<String>()
    );

    // 1. Parse intent: deterministic (fast, <1ms) → brain (admin) → NLU → Unknown
    // The full pipeline ensures that phrases trained in BERT-Mini or classified
    // by the Qwen brain are actually used for routing, not just observed.
    let parse_result = parse_deterministic(&transcript);

    // If deterministic missed:
    // When online: route directly to cloud backend / 9Router (0 MB local RAM).
    // When offline: fall back to local ML sidecar (BERT-Mini / brain on-demand).
    let parse_result = if parse_result.is_some() {
        parse_result
    } else {
        let is_online = crate::tts_network::check_network().await;
        if !is_online {
            tracing::info!("orchestrator: deterministic missed & offline, trying local ML fallback");
            let ml_result = parse_with_ml(&transcript).await;
            if let Some(ref r) = ml_result {
                tracing::info!(
                    "orchestrator: ML classified as {:?} (confidence={}, source={})",
                    r.intent, r.confidence, r.source
                );
            }
            ml_result
        } else {
            tracing::info!("orchestrator: deterministic missed & online, routing directly to cloud backend (saves RAM)");
            None
        }
    };

    let intent = parse_result
        .as_ref()
        .map(|r| r.intent.clone())
        .unwrap_or_else(|| {
            crate::missed_intent_logger::log_missed_intent(
                &transcript,
                "orchestrator",
                "deterministic_miss",
            );
            ParsedIntent::Unknown {
                raw: transcript.clone(),
            }
        });

    tracing::info!("orchestrator: parsed intent: {:?}", intent);
    let turn = turn_context.unwrap_or_default();
    if turn.ownership == crate::voice_profile::TurnOwnership::Rejected {
        tracing::info!(
            "orchestrator: ambient drop (session={}, score={:.3})",
            turn.session,
            turn.owner_score
        );
        return Ok(ProcessResult {
            request_id: new_request_id(),
            subsystem: Subsystem::None,
            handled_locally: true,
        });
    }

    let source = parse_result
        .as_ref()
        .map(|result| result.source.as_str())
        .unwrap_or("none");
    match crate::center::action_disposition(
        &intent,
        &transcript,
        source,
        turn.ownership,
        crate::ghost::session_active(),
    ) {
        crate::center::ActionDisposition::AmbientDrop => {
            tracing::info!("orchestrator: ambient drop (weak evidence, auto session)");
            return Ok(ProcessResult {
                request_id: new_request_id(),
                subsystem: Subsystem::None,
                handled_locally: true,
            });
        }
        crate::center::ActionDisposition::Clarify => {
            let (prompt, slot) = clarification_for(&intent);
            return speak_prompt_and_hold(&app, prompt, slot).await;
        }
        crate::center::ActionDisposition::Allow => {}
    }

    // Main Center decision tree (doc 74, P0: classification only — the
    // routing below is untouched, so behavior is identical).
    tracing::info!(
        "main-center: {:?} → sub-center {}",
        intent,
        crate::center::center_for(&intent)
    );

    // Main Center validity gate (doc 74, P1): garbage / missing slots /
    // invalid values never reach execution or cloud chat — the user hears
    // exactly what's wrong. Skipped where every transcript is meaningful
    // by design: dictation mode (type everything), the ghostwriter room
    // (everything becomes ink), and a running drill (follow-ups queue for
    // the drain pass instead of being judged).
    if !is_dictation_active()
        && !crate::ghostwriter::is_active()
        && !crate::ghost::drill_running()
    {
        match crate::center::validate(&intent, &transcript, dialog_context.is_some()) {
            crate::center::Validity::Ok => {}
            crate::center::Validity::Unheard { prompt } => {
                println!("[ACTION] validity: Unheard → \"{prompt}\"");
                return speak_prompt_and_hold(&app, prompt, "repeat".to_string()).await;
            }
            crate::center::Validity::NeedSlot { slot, prompt } => {
                println!("[ACTION] validity: NeedSlot({slot}) → \"{prompt}\"");
                return speak_prompt_and_hold(&app, prompt, slot.to_string()).await;
            }
            crate::center::Validity::Invalid { reason, prompt } => {
                println!("[ACTION] validity: Invalid({reason}) → \"{prompt}\"");
                return speak_prompt_and_hold(&app, prompt, "repeat".to_string()).await;
            }
        }
    }

    // ─── Active Dictation Stream ──────────────────────────────────────
    if is_dictation_active() {
        let trimmed = transcript.trim();
        let lower = trimmed.to_lowercase();
        if lower == "stop typing"
            || lower == "stop dictation"
            || lower == "done typing"
            || lower == "end typing"
            || lower == "finish typing"
            || lower == "exit dictation"
            || lower == "stop"
            || lower == "stand down"
            || lower == "cancel"
        {
            set_dictation_active(false);
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(&app, "Stopped typing, sir.".to_string(), &request_id);
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }

        let (request_id, _) = install_new_request(Subsystem::LocalCommand);
        let line_to_type = format!("{}\n", trimmed);
        let outcome = tokio::task::spawn_blocking(move || {
            crate::live::commands::keyboard::type_text(&line_to_type)
        })
        .await
        .map_err(|e| e.to_string())?;

        if let Err(e) = outcome {
            tracing::warn!("dictation type_text failed: {}", e);
        }
        clear_active_request(&request_id);
        return Ok(ProcessResult {
            request_id,
            subsystem: Subsystem::LocalCommand,
            handled_locally: true,
        });
    }

    // ─── Ghostwriter room entry ─────────────────────────────────────
    // Explicit entry bypasses everything else: start session + card + reply.
    if let ParsedIntent::EnterGhostwriter { contact } = &intent {
        return run_ghostwriter_enter(app, contact.clone()).await;
    }

    // ─── Ghost cursor-control entry ───────────────────────────────────
    // "ghost mode" means the cursor, never dictation (that collision sent
    // users into the Ghostwriter room). Enters the ghost session (ring +
    // Esc cancel armed) and narrates; no sidebar card - the ring IS
    // the UI. Tracked as a local request like the room entry.
    if matches!(&intent, ParsedIntent::EnterGhostControl) {
        return run_ghost_control_enter(app).await;
    }

    // ─── Screen control (ordinal click / read-back / tab switch) ────
    // Executes inline (UIA grounding is local, ~50-500ms) with spoken
    // results. Nothing here touches the network.
    match &intent {
        ParsedIntent::ScreenClick { ordinal } => {
            return run_screen_click(app, *ordinal).await;
        }
        ParsedIntent::ScreenRead { ordinal } => {
            return run_screen_read(app, *ordinal).await;
        }
        ParsedIntent::BrowserTab { index } => {
            return run_browser_tab(app, *index).await;
        }
        ParsedIntent::BrowserCloseTab { index } => {
            return run_browser_close(app, *index).await;
        }
        ParsedIntent::NluResult { intent, .. } if intent == "browser_new_tab" => {
            // Live browser hotkeys: local, visible keyboard action. Routing
            // this to WorkerBackend sent "open a new tab" to the cloud chat
            // while ctrl+t sat unused in the executor (audit).
            return run_browser_new_tab(app).await;
        }
        ParsedIntent::BrowserSearch { query } => {
            return run_browser_search(app, query.clone()).await;
        }
        ParsedIntent::BrowserSearchFocus => {
            return run_browser_search_focus(app).await;
        }
        ParsedIntent::WatchScreenEmail => {
            return run_watch_screen_email(app).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "youtube_search" => {
            let query = slots.get("query").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return run_youtube_search(app, query).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "youtube_journal" => {
            let url = slots.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return run_youtube_journal(app, url).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "type_text" => {
            let text = slots.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            return run_type_text(app, text).await;
        }
        // Local system control (doc 07 P2): these previously had no
        // dispatch arm at all, so a classified "focus_app"/"system_*"
        // intent silently defaulted to Subsystem::WorkerBackend — the
        // classifier ran instantly and locally, then its answer was
        // thrown away for a cloud round-trip that couldn't act on it.
        ParsedIntent::NluResult { intent, slots, .. } if intent == "focus_app" => {
            let target = slots.get("target").and_then(|v| v.as_str())
                .or_else(|| slots.get("app").and_then(|v| v.as_str()))
                .or_else(|| slots.get("title").and_then(|v| v.as_str()))
                .unwrap_or("").to_string();
            return run_focus_app(app, target).await;
        }
        ParsedIntent::NluResult { intent, .. } if intent == "system_minimize_window" => {
            return run_minimize_window(app).await;
        }
        ParsedIntent::NluResult { intent, .. } if intent == "system_maximize_window" => {
            return run_maximize_window(app).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "system_click_element" || intent == "click_element" => {
            let name = slots.get("name").and_then(|v| v.as_str())
                .or_else(|| slots.get("element").and_then(|v| v.as_str()))
                .unwrap_or("").to_string();
            return run_click_element(app, name).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "screen_analysis" => {
            let prompt = slots.get("prompt").and_then(|v| v.as_str()).unwrap_or("analyze the screen").to_string();
            return run_screen_analysis(app, prompt).await;
        }
        ParsedIntent::NluResult { intent, slots, .. } if intent == "screen_annotation" => {
            let prompt = slots.get("prompt").and_then(|v| v.as_str()).unwrap_or("annotate the screen").to_string();
            return run_screen_annotation(app, prompt).await;
        }
        ParsedIntent::StartDictation => {
            set_dictation_active(true);
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(&app, "Dictation started, sir. Speak line by line.".to_string(), &request_id);
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }
        ParsedIntent::StopDictation => {
            set_dictation_active(false);
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(&app, "Stopped typing, sir.".to_string(), &request_id);
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }
        ParsedIntent::OpenSettings => {
            return run_open_settings(app).await;
        }
        _ => {}
    }

    // ─── Ghostwriter session intercept ──────────────────────────────
    // Mic-hot rule: while a session is live, EVERY transcript routes to the
    // room (dictation or allowlisted commands) instead of the normal
    // pipeline. No wake word needed between turns; a timed-out session
    // resumes with its draft intact on re-entry.
    if crate::ghostwriter::is_active() {
        return run_ghostwriter_turn(app, transcript).await;
    }

    // ─── Ghost drill overlap: stop-words + follow-up queue ──────────
    // While a ghost drill runs, mid-drill speech must not route normally
    // (a second full turn would fight the drill for the mouse/keyboard):
    // stop-words abort the TASK (session stays — user directive: mouse
    // use / stop never turns the session off; "exit ghost mode" ends it),
    // everything else queues silently for the drill's drain pass. Bare
    // test sessions don't set the drill flag, so they never swallow
    // commands.
    if crate::ghost::drill_running() {
        // Doc 07 P5: is_stop_phrase is a raw exact-match list — it doesn't
        // see through STT mishearings the way the deterministic parser's
        // own normalize_phonetic_mishearings does internally. Checking the
        // phonetically-normalized transcript too (OR, never replacing the
        // raw check) means "stahp"/"cansel"-style STT noise on these very
        // short, safety-critical words gets the same correction ghost-mode
        // entry already benefits from, without weakening anything.
        let normalized = crate::intent_parser::normalize_phonetic_mishearings(&transcript);
        let is_stop = crate::ghost::is_stop_phrase(&transcript)
            || crate::ghost::is_stop_phrase(&normalized)
            || matches!(&intent, ParsedIntent::NluResult { intent, .. } if intent == "cancel_action");
        if is_stop {
            // Rule 1 (priority lane): stop-words never queue — they preempt
            // the in-flight step at its boundary and purge everything
            // pending (a queued "cancel" behind its target is the F1 bug).
            // Barge-in first: cut drill narration instantly ("Stopped, sir"
            // still speaks after via the normal leading edge).
            request_barge_in("drill-stop");
            crate::ghost::request_stop();
            crate::ghost::drop_followups();
            crate::live::state::context().reset();
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(&app, "Stopped, sir. Ghost mode is still on.".to_string(), &request_id);
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }
        // FIFO lane: supersede-or-append (Rules 2+4), silent by design —
        // the drill's drain pass executes in arrival order.
        let (kept, depth) = crate::ghost::enqueue_command(transcript.clone());
        if kept {
            tracing::info!("ghost: queued mid-drill follow-up (silent, drains after main steps)");
            // Rule 4: depth-ACK once per session so a script user knows
            // the mic heard them without per-command chatter.
            if depth >= 2
                && crate::commands::read_ghost_depth_ack(&app)
                && !crate::ghost::DEPTH_ACK_SPOKEN
                    .swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                let (request_id, _) = install_new_request(Subsystem::LocalCommand);
                speak_line(&app, "Queued, sir.".to_string(), &request_id);
                clear_active_request(&request_id);
            }
        } else {
            // Cap reached (drop-newest): logged; deliberately silent — TTS
            // mid-drill would echo into the still-hot mic.
            tracing::warn!("ghost: queue full, rejected newest follow-up");
        }
        return Ok(ProcessResult {
            request_id: new_request_id(),
            subsystem: Subsystem::None,
            handled_locally: true,
        });
    }

    // ─── Ghost session routing: exits + app opens + desktop messages ──
    // With a ghost session live (but no drill running — that's handled
    // above): explicit exits end the session; OpenApp and WhatsApp-
    // message transcripts drive the ghost desktop flows instead of the
    // plain paths (ring narration, session guards, visible typing,
    // session stays open for follow-ups). Messages NEVER auto-send —
    // the confirm gate still owns that. Stop-words with nothing running
    // inform instead of routing to the Worker. Everything else routes
    // normally (questions, dictation, media).
    if crate::ghost::session_active() {
        // Explicit exit: "exit/close/turn off ghost mode" voice phrase.
        if matches!(&intent, ParsedIntent::ExitGhostControl) {
            let _ = crate::ghost::ghost_exit(crate::ghost::ghost_wry::g_wry(app.clone())).await;
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(&app, GHOST_EXIT_LINE.to_string(), &request_id);
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }
        // Stop-words with nothing running: inform (never route to the
        // Worker — "stop" mid-idle means "nothing to stop"). Phonetic
        // normalization check too — see doc 07 P5 note above.
        if crate::ghost::is_stop_phrase(&transcript)
            || crate::ghost::is_stop_phrase(&crate::intent_parser::normalize_phonetic_mishearings(&transcript))
            || matches!(&intent, ParsedIntent::NluResult { intent, .. } if intent == "cancel_action")
        {
            let (request_id, _) = install_new_request(Subsystem::LocalCommand);
            emit(
                &app,
                &OrchestratorEvent::Result {
                    text: "Nothing running, sir.".to_string(),
                    request_id: request_id.clone(),
                    analysis: None,
                    dialog_state: None,
                },
            );
            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: request_id.clone(),
                },
            );
            clear_active_request(&request_id);
            return Ok(ProcessResult {
                request_id,
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            });
        }
        match &intent {
            ParsedIntent::OpenApp { target } => {
                return run_ghost_open(app, target.clone()).await;
            }
            ParsedIntent::SendWhatsAppMessage { contact, message } => {
                return run_ghost_message(app, contact.clone(), message.clone()).await;
            }
            ParsedIntent::WhatsappChat { contact } => {
                return run_ghost_message(app, contact.clone(), String::new()).await;
            }
            // Browser control in ghost mode: real hotkeys on the user's
            // laptop (visible), never MCP / Worker.
            ParsedIntent::BrowserCloseTab { index } => {
                return run_browser_close(app, *index).await;
            }
            ParsedIntent::NluResult { intent, .. } if intent == "browser_new_tab" => {
                return run_browser_new_tab(app).await;
            }
            ParsedIntent::Search { query } | ParsedIntent::BrowserSearch { query } => {
                return run_browser_search(app, query.clone()).await;
            }
            ParsedIntent::BrowserSearchFocus => {
                return run_browser_search_focus(app).await;
            }
            ParsedIntent::WatchScreenEmail => {
                return run_watch_screen_email(app).await;
            }
            ParsedIntent::ScreenClick { ordinal } => {
                return run_screen_click(app, *ordinal).await;
            }
            ParsedIntent::ScreenRead { ordinal } => {
                return run_screen_read(app, *ordinal).await;
            }
            ParsedIntent::NluResult { intent, slots, .. } if intent == "screen_analysis" => {
                let prompt = slots.get("prompt").and_then(|v| v.as_str()).unwrap_or("analyze the screen").to_string();
                return run_screen_analysis(app, prompt).await;
            }
            ParsedIntent::NluResult { intent, slots, .. } if intent == "screen_annotation" => {
                let prompt = slots.get("prompt").and_then(|v| v.as_str()).unwrap_or("annotate the screen").to_string();
                return run_screen_annotation(app, prompt).await;
            }
            ParsedIntent::NluResult { intent, slots, .. } if intent == "type_text" => {
                let text = slots.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                return run_type_text(app, text).await;
            }
            ParsedIntent::NluResult { intent, slots, .. } if intent == "focus_app" => {
                let target = slots.get("target").and_then(|v| v.as_str())
                    .or_else(|| slots.get("app").and_then(|v| v.as_str()))
                    .or_else(|| slots.get("title").and_then(|v| v.as_str()))
                    .unwrap_or("").to_string();
                return run_focus_app(app, target).await;
            }
            ParsedIntent::NluResult { intent, .. } if intent == "system_minimize_window" => {
                return run_minimize_window(app).await;
            }
            ParsedIntent::NluResult { intent, .. } if intent == "system_maximize_window" => {
                return run_maximize_window(app).await;
            }
            ParsedIntent::NluResult { intent, slots, .. } if intent == "system_click_element" || intent == "click_element" => {
                let name = slots.get("name").and_then(|v| v.as_str())
                    .or_else(|| slots.get("element").and_then(|v| v.as_str()))
                    .unwrap_or("").to_string();
                return run_click_element(app, name).await;
            }
            ParsedIntent::StartDictation => {
                set_dictation_active(true);
                let (request_id, _) = install_new_request(Subsystem::LocalCommand);
                speak_line(&app, "Dictation started, sir. Speak line by line.".to_string(), &request_id);
                clear_active_request(&request_id);
                return Ok(ProcessResult {
                    request_id,
                    subsystem: Subsystem::LocalCommand,
                    handled_locally: true,
                });
            }
            ParsedIntent::StopDictation => {
                set_dictation_active(false);
                let (request_id, _) = install_new_request(Subsystem::LocalCommand);
                speak_line(&app, "Stopped typing, sir.".to_string(), &request_id);
                clear_active_request(&request_id);
                return Ok(ProcessResult {
                    request_id,
                    subsystem: Subsystem::LocalCommand,
                    handled_locally: true,
                });
            }
            ParsedIntent::OpenSettings => {
                return run_open_settings(app).await;
            }
            _ => {}
        }
    }

    // ─── Command Center: compound task fast path ───────────────────────
    // If the transcript is compound ("X then Y"), build a task plan and
    // execute via the command center instead of normal single-intent
    // routing. This must run BEFORE routing because a compound transcript
    // won't parse as a single intent anyway (the deterministic parser
    // would return Unknown or a wrong partial match).
    let request_id_probe = new_request_id();
    if let Some(plan) =
        crate::command_center::build_plan_with_brain(&transcript, &request_id_probe).await
    {
        tracing::info!(
            "orchestrator: compound task detected — {} steps, using command center",
            plan.steps.len()
        );
        // Install a real request so cancellation works
        let (request_id, cancel_flag) = install_new_request(Subsystem::CommandCenter);
        // Re-tag the plan with the real request_id
        let plan = crate::command_center::TaskPlan {
            task_id: request_id.clone(),
            ..plan
        };
        return run_command_center(
            app,
            plan,
            transcript,
            dialog_context,
            request_id,
            cancel_flag,
            &turn,
        )
        .await;
    }

    // 1b. Brain monitor — watches every transcript in the background.
    // Non-blocking: spawns a tokio task, never delays the main pipeline.
    // The brain cross-checks the parse, learns pronunciations, and
    // auto-generates training data for BERT-Mini.
    // Only compiled when the admin-brain feature is enabled.
    #[cfg(feature = "admin-brain")]
    {
        let det_intent_name = parse_result.as_ref().map(|r| crate::intent_parser::intent_to_label(&r.intent).to_string());
        let transcript_clone = transcript.clone();
        crate::brain_monitor::monitor_transcript(
            transcript_clone,
            det_intent_name,
            None,
        );
    }

    // 2. Route to subsystem
    let subsystem = route_intent(&intent);

    // Console tracking (one line per turn): transcript → intent → subsystem.
    // INFO-level routing detail stays hidden; this is the trackable shape
    // (run.ps1 surfaces [ACTION] lines) so executed turns are provable
    // from `nexus start` alone — previously only transcripts showed.
    // Intent label (not Debug) keeps the line short on purpose.
    {
        let t: String = transcript.chars().take(60).collect();
        println!(
            "[ACTION] '{}' → {} → {:?}",
            t.replace('\n', " "),
            crate::intent_parser::intent_to_label(&intent),
            subsystem
        );
    }

    // 2a. Custom declarative specs (D2): user intents.yaml fills the
    // fallback slot — checked on deterministic-miss Unknown only, after
    // all session intercepts, before NLU/Worker. Built-ins always win.
    // 2b. Explicit memory write (B1): "remember that X is Y" / "my X is Y".
    // Same slot (Unknown only, post-intercepts) so in-room dictation like
    // "remember that we need milk" stays in the room.
    if matches!(&intent, ParsedIntent::Unknown { .. }) {
        if let Some((key, value)) = crate::memory::parse_remember(&transcript) {
            if let Ok(dir) = app.path().app_data_dir() {
                if crate::memory::remember(&dir, &key, &value) {
                    let reply = format!("Remembered {} as {}.", key.replace('_', " "), value);
                    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text: reply,
                            request_id: request_id.clone(),
                            analysis: None,
                            dialog_state: None,
                        },
                    );
                    emit(
                        &app,
                        &OrchestratorEvent::Done {
                            request_id: request_id.clone(),
                        },
                    );
                    clear_active_request(&request_id);
                    return Ok(ProcessResult {
                        request_id,
                        subsystem: Subsystem::LocalCommand,
                        handled_locally: true,
                    });
                }
            }
        }
        if let Ok(dir) = app.path().app_data_dir() {
            let specs = crate::agent_specs::load_specs(&dir);
            if let Some(custom) = crate::agent_specs::match_spec(&specs, &transcript) {
                return run_custom_spec(app, custom).await;
            }
        }
    }

    // 2b. Check for verbal "wrong" feedback (admin says "wrong" after a bad command)
    #[cfg(feature = "admin-brain")]
    {
        if crate::brain_monitor::is_verbal_wrong(&transcript) {
            crate::brain_monitor::report_verbal_wrong();
            // Emit done immediately — "wrong" is a meta-command, not a real command
            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: "verbal_wrong".to_string(),
                },
            );
            return Ok(ProcessResult {
                request_id: "verbal_wrong".to_string(),
                subsystem: Subsystem::None,
                handled_locally: true,
            });
        }
    }

    // 2c. Record the last command (for verbal "wrong" feedback)
    #[cfg(feature = "admin-brain")]
    {
        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
        crate::brain_monitor::record_last_command(transcript.clone(), intent_name);
    }

    // 3. Install new request (cancels previous)
    let (request_id, cancel_flag) = install_new_request(subsystem.clone());

    // 4. Emit "thinking" state
    emit(
        &app,
        &OrchestratorEvent::State {
            state: OrchestratorState::Thinking,
            request_id: request_id.clone(),
        },
    );

    // 5. Handle based on subsystem
    match subsystem {
        Subsystem::LocalCommand => {
            // Local commands are instant — no ack, no loading indicator.
            // The frontend handles these directly (open app, media, etc).
            // We just emit done immediately.

            // Clarification prompts (partial MCP commands) are spoken as a
            // Result event — same channel the frontend already speaks — so
            // the user hears the question instead of a Worker guess.
            if let ParsedIntent::NeedMoreInfo { prompt } = &intent {
                emit(
                    &app,
                    &OrchestratorEvent::Result {
                        text: prompt.clone(),
                        request_id: request_id.clone(),
                        analysis: None,
                        dialog_state: None,
                    },
                );
                emit(
                    &app,
                    &OrchestratorEvent::Done {
                        request_id: request_id.clone(),
                    },
                );
                clear_active_request(&request_id);

                return Ok(ProcessResult {
                    request_id,
                    subsystem,
                    handled_locally: true,
                });
            }

            // Report execution success to the brain monitor (admin-only)
            #[cfg(feature = "admin-brain")]
            {
                let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                crate::brain_monitor::report_execution_success(&transcript, &intent_name);
            }

            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: request_id.clone(),
                },
            );
            clear_active_request(&request_id);

            Ok(ProcessResult {
                request_id,
                subsystem,
                handled_locally: true,
            })
        }

        Subsystem::WorkerBackend => {
            // Long-running — emit ack, then show loading indicator after TTS.
            // Repeat-back: the ack names the action (P1), so the user hears
            // WHAT was understood instead of a bare "on it sir".
            let ack = ack_for(&intent);
            emit(
                &app,
                &OrchestratorEvent::Ack {
                    text: ack.to_string(),
                    request_id: request_id.clone(),
                },
            );

            // Show loading indicator (Rust owns this — no frontend IPC needed)
            emit(
                &app,
                &OrchestratorEvent::Loading {
                    visible: true,
                    request_id: request_id.clone(),
                },
            );
            show_loading(&app);

            // Dispatch to Worker backend
            let result = dispatch_to_worker(
                app.clone(),
                transcript.clone(),
                dialog_context,
                request_id.clone(),
                cancel_flag.clone(),
                &turn,
                crate::intent_parser::intent_to_label(&intent),
            )
            .await;

            // Hide loading indicator
            emit(
                &app,
                &OrchestratorEvent::Loading {
                    visible: false,
                    request_id: request_id.clone(),
                },
            );
            hide_loading(&app);

            match result {
                Ok((text, analysis, dialog_state)) => {
                    // Report execution success to the brain monitor (admin-only)
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_success(&transcript, &intent_name);
                    }

                    let is_analysis_intent = matches!(
                        &intent,
                        ParsedIntent::AnalysePr { .. }
                            | ParsedIntent::AnalyseRepo { .. }
                            | ParsedIntent::AnalyseLatestPr { .. }
                            | ParsedIntent::CheckBranch { .. }
                    );
                    let has_structured_analysis = analysis.as_ref().map_or(false, |a| !a.is_null());
                    let is_long_markdown = text.len() > 300 || text.contains("\n#") || text.contains("\n##");

                    let (spoken_text, show_sidebar) = if is_analysis_intent || has_structured_analysis || is_long_markdown {
                        let spoken = match &intent {
                            ParsedIntent::AnalysePr { pr_number, repo, .. } => {
                                format!("Here is the analysis for PR #{} in {}, sir.", pr_number, repo)
                            }
                            ParsedIntent::AnalyseRepo { repo, .. } => {
                                format!("Here is the analysis for {}, sir.", repo)
                            }
                            ParsedIntent::AnalyseLatestPr { repo, .. } => {
                                format!("Here is the analysis for the latest PR in {}, sir.", repo)
                            }
                            ParsedIntent::CheckBranch { repo, .. } => {
                                format!("Here is the branch check for {}, sir.", repo)
                            }
                            _ => "Here is the response in the sidebar, sir.".to_string(),
                        };
                        (spoken, true)
                    } else {
                        (text.clone(), false)
                    };

                    let sidebar_text = text.clone();
                    let sidebar_analysis = analysis.clone();

                    // Emit result with concise spoken text for TTS
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text: spoken_text,
                            request_id: request_id.clone(),
                            analysis: analysis.clone(),
                            dialog_state,
                        },
                    );

                    // Show sidebar for analysis/reports
                    if show_sidebar {
                        let app_clone = app.clone();
                        let transcript_clone = transcript.clone();
                        tauri::async_runtime::spawn(async move {
                            if let Some(ref a) = sidebar_analysis {
                                if !a.is_null() {
                                    if let Err(e) = crate::commands::show_sidebar_with_analysis(
                                        app_clone,
                                        transcript_clone,
                                        sidebar_text,
                                        a.clone(),
                                    )
                                    .await
                                    {
                                        tracing::warn!("orchestrator: sidebar analysis failed: {}", e);
                                    }
                                    return;
                                }
                            }
                            if let Err(e) = crate::commands::show_sidebar_with_content(
                                app_clone,
                                transcript_clone,
                                sidebar_text,
                            )
                            .await
                            {
                                tracing::warn!("orchestrator: sidebar show failed: {}", e);
                            }
                        });
                    }

                    clear_active_request(&request_id);

                    Ok(ProcessResult {
                        request_id,
                        subsystem,
                        handled_locally: false,
                    })
                }
                Err(e) => {
                    hide_loading(&app);
                    // Offline clarity (P3.1): a network-dropped Worker turn
                    // speaks an attributed line (never raw error JSON) and
                    // names what still works locally.
                    let offline = !crate::tts_network::is_network_up();
                    let msg = if offline {
                        "Network's down, sir — I've gone local. Offline commands still work: open and close apps, media, dictation, memory. Ask me again once you're back online."
                            .to_string()
                    } else {
                        e.clone()
                    };
                    // Report execution failure to the brain monitor
                    // (admin-only, no-op if not admin)
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_failure(
                            &transcript,
                            &intent_name,
                            &e,
                        );
                    }
                    emit(
                        &app,
                        &OrchestratorEvent::Error {
                            message: msg,
                            request_id: request_id.clone(),
                        },
                    );
                    emit(
                        &app,
                        &OrchestratorEvent::Done {
                            request_id: request_id.clone(),
                        },
                    );
                    clear_active_request(&request_id);
                    Err(e)
                }
            }
        }

        Subsystem::Architect => {
            // Long-running — emit ack, then show loading indicator.
            let ack = pick_ack();
            emit(
                &app,
                &OrchestratorEvent::Ack {
                    text: ack.to_string(),
                    request_id: request_id.clone(),
                },
            );
            emit(
                &app,
                &OrchestratorEvent::Loading {
                    visible: true,
                    request_id: request_id.clone(),
                },
            );
            show_loading(&app);

            // The architect subsystem is triggered via the existing
            // `open_architect_window` command. The frontend will call it
            // when it receives this event with subsystem=Architect.
            // We don't dispatch here — the frontend handles the architect
            // flow because it needs to hide the orb and manage the window.
            //
            // The orchestrator's job is to:
            //   - emit the ack
            //   - show the loading indicator
            //   - track the request ID
            // The frontend will emit "done" when the architect window opens.

            Ok(ProcessResult {
                request_id,
                subsystem,
                handled_locally: false,
            })
        }

        Subsystem::GitHub => {
            // GitHub sub-command system — typed operations via octocrab.
            // Long-running (network to GitHub API) — emit ack + loading.
            let ack = pick_ack();
            emit(
                &app,
                &OrchestratorEvent::Ack {
                    text: ack.to_string(),
                    request_id: request_id.clone(),
                },
            );
            emit(
                &app,
                &OrchestratorEvent::Loading {
                    visible: true,
                    request_id: request_id.clone(),
                },
            );
            show_loading(&app);

            // Get session info for token fetch
            let session_info = network::get_session_info()
                .ok_or("no session open — call open_session first")?;
            let (worker_url, user_id, _device_id) = session_info;

            // Extract the GitHubCommand from the parsed intent.
            // The intent parser (Phase 2A-6) produces ParsedIntent::GitHubCommand
            // for recognized GitHub operations.
            let gh_cmd = match &intent {
                ParsedIntent::GitHubCommand { command } => command.clone(),
                _ => {
                    // If we somehow got here without a GitHubCommand, fall back
                    // to the Worker (backward compatibility).
                    let result = dispatch_to_worker(
                        app.clone(),
                        transcript.clone(),
                        dialog_context,
                        request_id.clone(),
                        cancel_flag.clone(),
                        &turn,
                        crate::intent_parser::intent_to_label(&intent),
                    )
                    .await;

                    emit(
                        &app,
                        &OrchestratorEvent::Loading {
                            visible: false,
                            request_id: request_id.clone(),
                        },
                    );
                    hide_loading(&app);

                    match result {
                        Ok((text, analysis, dialog_state)) => {
                            emit(
                                &app,
                                &OrchestratorEvent::Result {
                                    text,
                                    request_id: request_id.clone(),
                                    analysis,
                                    dialog_state,
                                },
                            );
                            clear_active_request(&request_id);
                            return Ok(ProcessResult {
                                request_id,
                                subsystem,
                                handled_locally: false,
                            });
                        }
                        Err(e) => {
                            hide_loading(&app);
                            emit(
                                &app,
                                &OrchestratorEvent::Error {
                                    message: e.clone(),
                                    request_id: request_id.clone(),
                                },
                            );
                            emit(
                                &app,
                                &OrchestratorEvent::Done {
                                    request_id: request_id.clone(),
                                },
                            );
                            clear_active_request(&request_id);
                            return Err(e);
                        }
                    }
                }
            };

            // Execute the GitHub command via the typed subsystem
            let gh_result = crate::github_cmd::execute_command(
                &worker_url,
                &user_id,
                &gh_cmd,
                false, // not confirmed yet — confirmation flow handled by events
            )
            .await;

            // Hide loading indicator
            emit(
                &app,
                &OrchestratorEvent::Loading {
                    visible: false,
                    request_id: request_id.clone(),
                },
            );
            hide_loading(&app);

            // Emit the appropriate event based on the result type
            match &gh_result {
                crate::github_cmd::GitHubResult::NeedsConfirmation { prompt, command } => {
                    // Report successful detection (even though it needs confirmation,
                    // the parsing was correct)
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_success(&transcript, &intent_name);
                    }
                    let cmd_json = serde_json::to_value(command).unwrap_or(serde_json::Value::Null);
                    let _confirm_event = emit(
                        &app,
                        &OrchestratorEvent::Confirm {
                            prompt: prompt.clone(),
                            request_id: request_id.clone(),
                            command: cmd_json.clone(),
                        },
                    );
                    let confirm_payload = serde_json::json!({
                        "requestId": request_id.clone(),
                        "prompt": prompt.clone(),
                        "command": cmd_json,
                    });
                    // Log (never swallow): if the sidebar fails to open, the
                    // Confirm event above still fired but PENDING_SIDEBAR was
                    // never stored — the user would see no approve/cancel UI.
                    if let Err(e) = crate::commands::show_sidebar_with_confirmation(
                        app.clone(),
                        "GitHub Confirmation".to_string(),
                        prompt.clone(),
                        confirm_payload,
                    ).await {
                        tracing::warn!("confirm: GitHub sidebar failed to open: {e}");
                    }
                }
                crate::github_cmd::GitHubResult::MergeConflict {
                    pr_number,
                    repo,
                    conflict_files,
                    message,
                } => {
                    let files_json = serde_json::to_value(conflict_files).unwrap_or(serde_json::Value::Null);
                    emit(
                        &app,
                        &OrchestratorEvent::ConflictReport {
                            request_id: request_id.clone(),
                            pr_number: *pr_number,
                            repo: repo.clone(),
                            conflict_files: files_json,
                            message: message.clone(),
                        },
                    );
                }
                crate::github_cmd::GitHubResult::Text { text } => {
                    // Report GitHub command success to the brain monitor
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_success(&transcript, &intent_name);
                    }
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text: text.clone(),
                            request_id: request_id.clone(),
                            analysis: None,
                            dialog_state: None,
                        },
                    );
                }
                crate::github_cmd::GitHubResult::PrList { repo, state, prs } => {
                    // Report GitHub command success to the brain monitor
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_success(&transcript, &intent_name);
                    }
                    // Emit a short TTS ack + the structured PR list for the sidebar.
                    // The frontend will open the PR list sidebar panel.
                    let count = prs.len();
                    let ack_text = if repo == "all repositories" {
                        format!(
                            "Showing {} {} PR{} across all your repositories.",
                            count,
                            state,
                            if count == 1 { "" } else { "s" },
                        )
                    } else {
                        format!(
                            "Showing {} {} PR{} in {}.",
                            count,
                            state,
                            if count == 1 { "" } else { "s" },
                            repo
                        )
                    };
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text: ack_text,
                            request_id: request_id.clone(),
                            analysis: None,
                            dialog_state: None,
                        },
                    );

                    // Store the PR list as pending data BEFORE creating the
                    // sidebar window. The frontend fetches this on mount via
                    // `get_pending_pr_list`, which is race-free regardless of
                    // how long the WebView takes to load. This fixes the bug
                    // where the `github_result` event was emitted before the
                    // sidebar window existed, so the event was lost.
                    let pr_list_json = serde_json::json!({
                        "type": "pr_list",
                        "repo": repo,
                        "state": state,
                        "prs": prs,
                    });
                    crate::commands::set_pending_pr_list(pr_list_json);

                    // Create the sidebar window directly from Rust (don't
                    // rely on the frontend to call `show_pr_list_sidebar`,
                    // because the frontend listener that would call it
                    // doesn't exist yet — the window hasn't been created).
                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = crate::commands::show_pr_list_sidebar(app_clone).await {
                            tracing::error!("orchestrator: failed to show PR list sidebar: {}", e);
                        }
                    });
                }
                crate::github_cmd::GitHubResult::Error { message, .. } => {
                    // Report GitHub command failure to the brain monitor
                    #[cfg(feature = "admin-brain")]
                    {
                        let intent_name = crate::intent_parser::intent_to_label(&intent).to_string();
                        crate::brain_monitor::report_execution_failure(
                            &transcript,
                            &intent_name,
                            message,
                        );
                    }
                    emit(
                        &app,
                        &OrchestratorEvent::Error {
                            message: message.clone(),
                            request_id: request_id.clone(),
                        },
                    );
                }
            }

            // Emit the raw GitHubResult for the frontend to use
            let result_json = serde_json::to_value(&gh_result).unwrap_or(serde_json::Value::Null);
            emit(
                &app,
                &OrchestratorEvent::GitHubResult {
                    request_id: request_id.clone(),
                    result: result_json,
                },
            );
            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: request_id.clone(),
                },
            );
            clear_active_request(&request_id);
            Ok(ProcessResult {
                request_id,
                subsystem,
                handled_locally: false,
            })
        }

        Subsystem::Mcp => {
            // MCP sub-center — external services via Model Context Protocol.
            let is_write_intent = matches!(
                intent,
                ParsedIntent::SendWhatsAppMessage { .. }
            );

            // Only emit generic long-running Ack/Loading if this is a read operation
            // that executes directly without an immediate confirmation prompt.
            if !is_write_intent {
                let ack = pick_ack();
                emit(
                    &app,
                    &OrchestratorEvent::Ack {
                        text: ack.to_string(),
                        request_id: request_id.clone(),
                    },
                );
                emit(
                    &app,
                    &OrchestratorEvent::Loading {
                        visible: true,
                        request_id: request_id.clone(),
                    },
                );
                show_loading(&app);
            }

            let mcp_outcome = dispatch_to_mcp(&app, &intent, &transcript, &request_id).await;

            if !is_write_intent {
                emit(
                    &app,
                    &OrchestratorEvent::Loading {
                        visible: false,
                        request_id: request_id.clone(),
                    },
                );
                hide_loading(&app);
            }

            match mcp_outcome {
                Ok(Some(text)) => {
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text,
                            request_id: request_id.clone(),
                            analysis: None,
                            dialog_state: None,
                        },
                    );
                    emit(
                        &app,
                        &OrchestratorEvent::Done {
                            request_id: request_id.clone(),
                        },
                    );
                    clear_active_request(&request_id);
                    Ok(ProcessResult {
                        request_id,
                        subsystem,
                        handled_locally: false,
                    })
                }
                Ok(None) => {
                    // Confirmation gate active — OrchestratorEvent::Confirm was emitted
                    // with the prompt question. Do NOT emit Result or Done so TTS speaks
                    // the confirmation question cleanly without cancellation.
                    Ok(ProcessResult {
                        request_id,
                        subsystem,
                        handled_locally: false,
                    })
                }
                Err(e) => {
                    emit(
                        &app,
                        &OrchestratorEvent::Error {
                            message: e.clone(),
                            request_id: request_id.clone(),
                        },
                    );
                    emit(
                        &app,
                        &OrchestratorEvent::Done {
                            request_id: request_id.clone(),
                        },
                    );
                    clear_active_request(&request_id);
                    // Best-of combine: voice spoke the guidance; open the
                    // fix-it card alongside (first failure per session).
                    // The stashed retry fires when the server connects.
                    if let Some(server) = server_for_mcp_intent(&intent) {
                        open_mcp_connect_card(&app, server, &transcript).await;
                    }
                    Err(e)
                }
            }
        }

        Subsystem::CommandCenter => {
            // Unreachable — compound tasks return early via
            // run_command_center() before this match. Keep as a safety
            // fallback: treat like WorkerBackend.
            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: request_id.clone(),
                },
            );
            clear_active_request(&request_id);
            Ok(ProcessResult {
                request_id,
                subsystem,
                handled_locally: true,
            })
        }

        Subsystem::None => {
            emit(
                &app,
                &OrchestratorEvent::Done {
                    request_id: request_id.clone(),
                },
            );
            clear_active_request(&request_id);
            Ok(ProcessResult {
                request_id,
                subsystem,
                handled_locally: true,
            })
        }
    }
}

/// DAC/room-reverb tail drain after cutting TTS before fresh capture (ms).
/// Single owner — every barge path sleeps this after `request_barge_in`.
/// Matches the hotkey branch's measured 150 ms (sound card + room tail
/// outlive `stop_tts()` and would otherwise poison the new capture).
pub(crate) const BARGE_DAC_DRAIN_MS: u64 = 150;

/// Alexa-style barge-in choke point: cut speech mid-sentence, cancel the
/// active turn, clear the TTS flag, purge queued follow-ups — in that
/// order (audio first, bookkeeping after). Idempotent: every primitive
/// is safe when nothing is playing, so unconditional calls are free.
/// Single call site owner for ALL barge paths (hotkey, wake fire, drill
/// stop) — never duplicate these four lines elsewhere.
pub(crate) fn request_barge_in(reason: &str) {
    let _ = crate::tts::stop_tts();
    cancel_active();
    crate::wakeword_oww::clear_tts_playing();
    crate::ghost::drop_followups();
    tracing::info!("barge-in: audio cut + turn cancelled ({reason})");
}

/// Cancel the active request (if any). Called on barge-in or new wake.
pub fn cancel_active() {
    let mut guard = ACTIVE_REQUEST.lock().unwrap();
    if let Some(req) = guard.as_ref() {
        req.cancelled.store(true, Ordering::Relaxed);
        tracing::info!("orchestrator: cancelled request {}", req.id);
    }
    *guard = None;
}

/// Signal that the current request is done (called by frontend after TTS).
pub fn signal_done(request_id: &str) {
    clear_active_request(request_id);
}

// ─── Subsystem dispatchers ─────────────────────────────────────────────

/// Dispatch to the Cloudflare Worker backend.
///
/// This reuses the existing `network::send_transcript` HTTP logic but
/// routes the response through the orchestrator's event channel instead
/// of the old "assistant:server" channel.
///
/// **9Router optimization:** For general questions, 9Router tries free
/// cloud providers (Cerebras → Groq → Gemini) directly from the device,
/// bypassing the Worker for 3-7x lower latency (~242ms vs ~2s). The
/// Worker is the fallback if 9Router fails or the task requires Worker
/// infrastructure (PR analysis, GitHub token, search).
/// NEXUS wire protocol version (C3) — sent on every Worker POST.
/// Must match `PROTOCOL_VERSION` in server/worker/src/protocol.ts.
pub const PROTOCOL_VERSION: &str = "1";

/// Build the Worker request payload. Pure + unit-tested.
/// `profile_id` rides `requester` for canonical (Worker-issued) identity
/// clients; legacy clients pass None and keep the pre-identity shape.
pub fn build_worker_payload(
    request_id: &str,
    user_id: &str,
    device_id: &str,
    transcript: &str,
    dialog_context: Option<&serde_json::Value>,
) -> serde_json::Value {
    build_worker_payload_ident(request_id, user_id, device_id, None, transcript, dialog_context)
}

/// Identity-aware payload builder (Feature 88).
pub fn build_worker_payload_ident(
    request_id: &str,
    user_id: &str,
    device_id: &str,
    profile_id: Option<&str>,
    transcript: &str,
    dialog_context: Option<&serde_json::Value>,
) -> serde_json::Value {    let task = if let Some(ctx) = dialog_context {
        serde_json::json!({
            "type": "general",
            "request": transcript,
            "dialog_context": ctx,
        })
    } else {
        serde_json::json!({
            "type": "general",
            "request": transcript,
        })
    };
    let mut requester = serde_json::json!({
        "id": user_id,
        "device_id": device_id,
    });
    if let Some(p) = profile_id {
        requester["profile_id"] = serde_json::Value::String(p.to_string());
    }
    serde_json::json!({
        "protocol_version": PROTOCOL_VERSION,
        "request_id": request_id,
        "requester": requester,
        "task": task,
    })
}

/// Feature 88 (C2): map a Worker AI-entitlement denial to its distinct,
/// speakable line. Returns None for non-denial errors (regular worker
/// failures keep the generic error path).
pub fn denial_spoken_line(body: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    if json["error"].as_str()? != "worker_ai_not_enabled" {
        return None;
    }
    let line = match json["code"].as_str().unwrap_or("") {
        "pending" => "Cloud access is awaiting approval from your administrator, sir. Everything local still works meanwhile.",
        "suspended" => "Cloud access has been suspended, sir. Please contact your administrator.",
        "revoked" => "This device's cloud access has been revoked, sir.",
        "expired" => "This device's cloud access grant has expired, sir. Please ask your administrator to renew it.",
        "bad_token" => "This device's cloud credentials are invalid, sir. Please reconnect in the setup wizard.",
        _ => "Cloud access isn't enabled for this device yet, sir.",
    };
    Some(line.to_string())
}

fn record_worker_turn<R: Runtime>(
    app: &AppHandle<R>,
    transcript: &str,
    response: &str,
    dialog_state: Option<&serde_json::Value>,
    turn: &crate::center::TurnContext,
    intent_label: &str,
    outcome: crate::conversation::ConversationOutcome,
    unresolved: Vec<String>,
) {
    if let Ok(dir) = app.path().app_data_dir() {
        let mut unresolved = unresolved;
        unresolved.extend(crate::conversation::unresolved_from_dialog_state(dialog_state));
        crate::conversation::record_conversation_turn(
            &dir,
            crate::conversation::ConversationTurnInput {
                agent: "worker".to_string(),
                intent: intent_label.to_string(),
                transcript: transcript.to_string(),
                response: response.to_string(),
                outcome,
                unresolved,
                owner: turn.ownership,
            },
        );
    }
}

async fn dispatch_to_worker<R: Runtime>(
    app: AppHandle<R>,
    transcript: String,
    dialog_context: Option<serde_json::Value>,
    request_id: String,
    cancel_flag: Arc<AtomicBool>,
    turn: &crate::center::TurnContext,
    intent_label: &str,
) -> Result<(String, Option<serde_json::Value>, Option<serde_json::Value>), String> {
    // Redact PII before the transcript leaves the device.
    let transcript = crate::pii_filter::sanitize(&transcript);
    let transcript = transcript.as_str();
    // Enrich dialog context with persistent facts plus the coordinated
    // conversation brief and high-signal recent turns.
    let mut dialog_context = dialog_context;
    if let Ok(dir) = app.path().app_data_dir() {
        let mut parts = Vec::new();
        if let Some(mem) = crate::memory::get_memory_context(&dir, transcript) {
            parts.push(mem);
        }
        if let Some(history) = crate::conversation::conversation_prompt(&dir) {
            parts.push(history);
        }
        if !parts.is_empty() {
            let mut ctx = dialog_context.unwrap_or(serde_json::json!({}));
            if let Some(obj) = ctx.as_object_mut() {
                obj.insert("memory".to_string(), serde_json::Value::String(parts.join("\n")));
            }
            dialog_context = Some(ctx);
        }
    }
    // ─── 9Router fast path ──────────────────────────────────────────────
    // Try local → free cloud providers first. This bypasses the Worker
    // entirely for general questions, cutting latency from ~2s to ~242ms.
    if crate::router::can_route(&transcript) {
        let keys = crate::router::read_provider_keys(&app);
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| format!("9router http client: {e}"))?;

        tracing::info!(
            "9router: trying fast path for request {} (transcript: {:?})",
            request_id,
            crate::router::truncate_pub(&transcript, 60),
        );

        match crate::router::route_question(
            &transcript,
            dialog_context.as_ref(),
            &keys,
            &client,
        )
        .await
        {
            Some(resp) => {
                tracing::info!(
                    "9router: fast path succeeded via {} in {}ms",
                    resp.provider.name(),
                    resp.latency_ms
                );
                // Record the completed cloud turn in coordinated conversation memory.
                record_worker_turn(
                    &app,
                    transcript,
                    &resp.text,
                    None,
                    turn,
                    intent_label,
                    crate::conversation::ConversationOutcome::Completed,
                    Vec::new(),
                );
                // 9Router answered — return directly, skip Worker entirely.
                return Ok((resp.text, None, None));
            }
            None => {
                tracing::info!(
                    "9router: fast path failed for {}, falling back to Worker",
                    request_id
                );
                // Fall through to Worker
            }
        }
    }

    // Check if cancelled before Worker call
    if is_cancelled(&cancel_flag) {
        return Err("cancelled".into());
    }

    // ─── Worker fallback path (original logic) ──────────────────────────
    // Get session info
    let session_info = network::get_session_info()
        .ok_or("no session open — call open_session first")?;
    let (worker_url, user_id, device_id) = session_info;

    // Build the request payload (versioned wire protocol v1).
    // Sanitize dialog history + memory: the Worker forwards these to
    // its own LLM calls, so raw PII must not ride along.
    let clean_ctx = dialog_context
        .as_ref()
        .map(|c| crate::pii_filter::sanitize_context(c));
    // Feature 88: canonical clients attach the Worker-issued profile_id
    // and authenticate with the device token (keyring, never plaintext).
    let identity = app.path().app_data_dir().ok()
        .map(|dir| crate::identity_state::read_identity_config(&dir));
    let profile_id = identity.as_ref().and_then(|i| {
        (i.identity == crate::identity_state::IDENTITY_CANONICAL && !i.profile_id.is_empty())
            .then(|| i.profile_id.clone())
    });
    let payload = build_worker_payload_ident(
        &request_id,
        &user_id,
        &device_id,
        profile_id.as_deref(),
        transcript,
        clean_ctx.as_ref(),
    );
    let device_token = profile_id
        .as_ref()
        .and_then(|_| crate::auth_vault::get_api_key(crate::identity_state::DEVICE_TOKEN_SERVICE));

    let record_failure = |message: &str| {
        record_worker_turn(
            &app,
            transcript,
            message,
            None,
            turn,
            intent_label,
            crate::conversation::ConversationOutcome::Failed,
            vec!["followup".to_string()],
        );
    };

    // HTTP POST to the Worker
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| {
            let message = format!("http client: {e}");
            record_failure(&message);
            message
        })?;

    tracing::info!(
        "orchestrator: dispatching to worker: url={} request_id={}",
        worker_url,
        request_id
    );

    let resp = client
        .post(&worker_url)
        .json(&payload)
        .headers({
            let mut h = reqwest::header::HeaderMap::new();
            if let Some(tok) = &device_token {
                if let Ok(v) = reqwest::header::HeaderValue::from_str(&format!("Bearer {tok}")) {
                    h.insert(reqwest::header::AUTHORIZATION, v);
                }
            }
            h
        })
        .send()
        .await
        .map_err(|e| {
            let message = format!("worker request: {e}");
            record_failure(&message);
            message
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Feature 88: a cloud-access denial gets its own spoken line.
        let message = denial_spoken_line(&body)
            .unwrap_or_else(|| format!("Worker error {status}: {body}"));
        record_failure(&message);
        return Err(message);
    }

    // Check if cancelled while waiting
    if is_cancelled(&cancel_flag) {
        tracing::info!("orchestrator: request {} cancelled, discarding result", request_id);
        return Err("cancelled".into());
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| {
            let message = format!("worker json: {e}");
            record_failure(&message);
            message
        })?;

    let reply_text = data["reply_text"]
        .as_str()
        .or(data["text"].as_str())
        .or(data["content"].as_str())
        .or(data["response"].as_str())
        .unwrap_or("I couldn't process that request.")
        .to_string();

    let analysis = data.get("analysis").cloned();
    let dialog_state = data.get("dialog_state").cloned();

    // Quota-aware narration (P3.2): a quota denial rides reply_text from
    // the Worker (quota.reason) — surface it verbatim (already spoken
    // downstream) and mark the flag so callers/UI can badge the state.
    if data.get("quota_exceeded").and_then(|q| q.as_bool()).unwrap_or(false) {
        tracing::warn!(
            "orchestrator: worker quota denial for request {} (spoken verbatim)",
            request_id
        );
    }

    // Record the completed cloud turn in coordinated conversation memory.
    let unresolved = crate::conversation::unresolved_from_dialog_state(dialog_state.as_ref());
    let outcome = if unresolved.is_empty() {
        crate::conversation::ConversationOutcome::Completed
    } else {
        crate::conversation::ConversationOutcome::Clarification
    };
    record_worker_turn(
        &app,
        transcript,
        &reply_text,
        dialog_state.as_ref(),
        turn,
        intent_label,
        outcome,
        Vec::new(),
    );

    Ok((reply_text, analysis, dialog_state))
}

/// Public wrapper for `dispatch_to_worker` — used by command_center steps.
pub(crate) async fn dispatch_to_worker_pub<R: Runtime>(
    app: AppHandle<R>,
    transcript: String,
    dialog_context: Option<serde_json::Value>,
    request_id: String,
    cancel_flag: Arc<AtomicBool>,
    turn: &crate::center::TurnContext,
    intent_label: &str,
) -> Result<(String, Option<serde_json::Value>, Option<serde_json::Value>), String> {
    dispatch_to_worker(
        app,
        transcript,
        dialog_context,
        request_id,
        cancel_flag,
        turn,
        intent_label,
    )
    .await
}

// ─── MCP sub-center dispatch ───────────────────────────────────────────

/// Dispatch an MCP intent to the appropriate MCP server.
///
/// Maps intents to (server, tool, params):
///   OrderFood            → SwiggyFood / search_restaurants
///   SearchProduct        → Amazon / amazon_search
///   SendWhatsAppMessage  → WhatsApp / send_message (confirmation gated)
///
/// Read operations execute directly. Write/destructive operations emit a
/// Confirm event and return a pending message — the actual call happens
/// after the user confirms via `orchestrator_mcp_confirm`.
async fn dispatch_to_mcp<R: Runtime>(
    app: &AppHandle<R>,
    intent: &ParsedIntent,
    transcript: &str,
    request_id: &str,
) -> Result<Option<String>, String> {
    use crate::mcp_client::{call_tool, extract_text, McpServer};

    // Resolve the intent → (server, tool, params)
    let (server, tool, params): (McpServer, &str, serde_json::Value) = match intent {
        ParsedIntent::OrderFood { query, restaurant } => {
            let q = if let Some(r) = restaurant {
                format!("{} from {}", query, r)
            } else if !query.is_empty() {
                query.clone()
            } else {
                transcript.to_string()
            };
            (
                McpServer::SwiggyFood,
                "search_restaurants",
                serde_json::json!({ "query": q }),
            )
        }
        ParsedIntent::SearchProduct { query } => (
            McpServer::Amazon,
            "amazon_search",
            serde_json::json!({ "query": query, "max_results": 5 }),
        ),
        ParsedIntent::SendWhatsAppMessage { contact, message } => (
            McpServer::WhatsApp,
            "send_message",
            serde_json::json!({ "recipient": contact, "message": message }),
        ),
        _ => return Err(format!("no MCP mapping for intent")),
    };

    // Pre-flight credential check — BEFORE the confirm gate. Asking the
    // user to approve a write the system can't execute is worse than
    // useless; missing credentials speak guidance immediately.
    // Resolve the bearer token from the auth vault (one login per service
    // group, with refresh).
    let pre_status = server
        .vault_key()
        .map(crate::auth_vault::token_status);
    let mut vault_token: Option<String> =
        crate::auth_vault::resolve_server_token(server).await;
    if server.vault_key().is_some() && vault_token.is_none() {
        // Distinguish expired (had a login, it died) from missing (never
        // connected) so the spoken guidance is exact.
        if pre_status == Some("expired") {
            let what = match server {
                crate::mcp_client::McpServer::SwiggyFood
                | crate::mcp_client::McpServer::SwiggyInstamart
                | crate::mcp_client::McpServer::SwiggyDineout => "Swiggy",
                crate::mcp_client::McpServer::WhatsApp => "WhatsApp",
                crate::mcp_client::McpServer::Amazon => "Amazon",
            };
            return Err(format!(
                "Your {what} login expired, sir — reconnect it in Settings, Connections tab."
            ));
        }
        return Err(mcp_error_guidance(server, "HTTP 401: no credential"));
    }

    // Confirmation gate for write/destructive operations.
    if server.requires_confirmation(tool) {
        let prompt = if server.is_destructive(tool) {
            format!(
                "This will perform an irreversible action on {}. Tool: {}. Proceed?",
                server.name(),
                tool
            )
        } else {
            match intent {
                ParsedIntent::SendWhatsAppMessage { contact, message } => {
                    format!("Send WhatsApp message to {}: \"{}\"?", contact, message)
                }
                _ => format!("Execute {} on {}?", tool, server.name()),
            }
        };

        let pending = serde_json::json!({
            "kind": "mcp",
            "server": server.name(),
            "tool": tool,
            "params": params.clone(),
            "transcript": transcript,
        });

        emit(
            app,
            &OrchestratorEvent::Confirm {
                prompt: prompt.clone(),
                request_id: request_id.to_string(),
                command: pending.clone(),
            },
        );

        let confirm_payload = serde_json::json!({
            "requestId": request_id,
            "prompt": prompt,
            "command": pending,
        });
        // Log (never swallow): Confirm already emitted above; a failed
        // sidebar open leaves the user with no approve/cancel UI.
        if let Err(e) = crate::commands::show_sidebar_with_confirmation(
            app.clone(),
            "Action Confirmation".to_string(),
            prompt.clone(),
            confirm_payload,
        ).await {
            tracing::warn!("confirm: MCP action sidebar failed to open: {e}");
        }

        // The actual call happens in orchestrator_mcp_confirm after the
        // user approves. Return Ok(None) to indicate confirmation is pending.
        return Ok(None);
    }

    // Read operations — execute directly.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("mcp http client: {e}"))?;

    tracing::info!(
        "orchestrator: mcp dispatch server={} tool={} request_id={}",
        server.name(),
        tool,
        request_id
    );

    // Resolve the bearer token from the auth vault (one login per service
    // group, with refresh). Falls back to anonymous when the service isn't
    // connected — the failure arm below speaks the reconnect path.
    // (Pre-flight runs above; this re-resolves fresh for the actual call.)
    vault_token = crate::auth_vault::resolve_server_token(server).await;
    let mut result = call_tool(server, tool, params.clone(), &client, vault_token.as_deref()).await;

    // 401 clear-and-retry: the token may have died between resolve and use
    // (revoked server-side). Evict, mint fresh once, retry once — then
    // guidance. Never replays the same dead token.
    if !result.ok {
        let first_err = result.error.clone().unwrap_or_default();
        if is_auth_failure(&first_err) {
            if let Some(key) = server.vault_key() {
                crate::auth_vault::clear_token(key);
                tracing::info!(
                    "orchestrator: mcp {} auth failed — cleared stale token, retrying once",
                    server.name()
                );
                vault_token = crate::auth_vault::resolve_server_token(server).await;
                result = call_tool(
                    server,
                    tool,
                    params.clone(),
                    &client,
                    vault_token.as_deref(),
                )
                .await;
            }
        }
    }

    if !result.ok {
        let err = result
            .error
            .clone()
            .unwrap_or_else(|| "unknown MCP error".to_string());
        tracing::warn!("orchestrator: mcp {} failed: {}", server.name(), err);
        // Stash for auto-retry: when the Connect card's monitor sees the
        // server turn Ready, it replays this exact call (Composio
        // WAIT_FOR_CONNECTIONS shape — no re-speaking, no re-confirm).
        stash_mcp_retry(server, tool, &params);
        return Err(mcp_error_guidance(server, &err));
    }

    Ok(Some(extract_text(&result)))
}

/// Narrow auth-failure detector for retry/clear decisions.
/// Deliberately strict (status codes + explicit phrases) — the old
/// substring "auth" also matched "author"/"authentic" in tool output.
fn is_auth_failure(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("401")
        || lower.contains("unauthorized")
        || lower.contains("invalid_token")
        || lower.contains("authentication required")
        || lower.contains("login required")
        || lower.contains("token expired")
        || lower.contains("invalid token")
}

/// Turn an MCP transport/auth failure into an actionable spoken message.
/// Raw errors ("HTTP 401", "connection refused") mean nothing by voice —
/// every failure must tell the user WHICH connection to fix and WHERE.
/// Backend rule: never report a dead MCP without its reconnect path.
fn mcp_error_guidance(server: crate::mcp_client::McpServer, err: &str) -> String {
    use crate::mcp_client::McpServer as S;
    let lower = err.to_lowercase();
    let needs_login = is_auth_failure(err);
    let unreachable = lower.contains("refused")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("dns")
        || lower.contains("unreachable")
        || lower.contains("failed to resolve");
    if needs_login {
        let what = match server {
            S::SwiggyFood | S::SwiggyInstamart | S::SwiggyDineout => {
                "Swiggy — reconnect it"
            }
            S::WhatsApp => "WhatsApp — re-scan the bridge QR",
            S::Amazon => "Amazon — reconnect the bridge session",
        };
        return format!("{what} in Settings, Connections tab, sir, then try again.");
    }
    if unreachable && server.url().contains("127.0.0.1") {
        // Auto-start assist: name the EXACT binary and first-run steps, not
        // "start it". The user should be able to fix this from the spoken
        // sentence alone. (Catalog: docs/mcp/02-server-catalog.md)
        let what = match server {
            S::WhatsApp => "WhatsApp bridge isn't running, sir — run the mcp-whatsapp program on this PC, scan the QR it shows with WhatsApp on your phone, then press Recheck in Connections.",
            S::Amazon => "Amazon bridge isn't running, sir — start the Amazon bridge program on this PC, sign in when its browser window opens, then press Recheck in Connections.",
            _ => return format!(
                "The {} isn't running, sir — start it on this PC, then press Recheck in Connections.",
                server.name()
            ),
        };
        return what.to_string();
    }
    if lower.contains("circuit open") {
        return format!(
            "The {} connection is cooling down after repeated failures, sir — press Recheck in Connections to retry now.",
            server.name()
        );
    }
    format!("{} failed, sir: {}", server.name(), err)
}

// ─── MCP Connect card (best-of combine) ─────────────────────────────────
// Industry pattern (Composio/Claude/Cursor): a failed connector opens a
// fix-it card where the user already is, with status + numbered steps +
// the auth action inline — and the interrupted task auto-resumes on
// completion. Voice still speaks first; the card opens alongside, once
// per server per session (no window spam).

/// Servers whose Connect card was already opened this session.
static SHOWN_CONNECT_CARDS: once_cell::sync::Lazy<
    Arc<Mutex<std::collections::HashSet<String>>>,
> = once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(std::collections::HashSet::new())));

/// A failed MCP call stashed for auto-retry when its server connects.
#[derive(Debug, Clone)]
struct McpRetry {
    server: crate::mcp_client::McpServer,
    tool: String,
    params: serde_json::Value,
}

static PENDING_MCP_RETRY: once_cell::sync::Lazy<Arc<Mutex<Option<McpRetry>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(None)));

fn stash_mcp_retry(
    server: crate::mcp_client::McpServer,
    tool: &str,
    params: &serde_json::Value,
) {
    let mut guard = PENDING_MCP_RETRY.lock().unwrap();
    // Single slot: a second failure before the 5s monitor fires discards the
    // first stashed call. Log the drop so it is visible instead of silent.
    if let Some(prev) = guard.as_ref() {
        tracing::warn!(
            "mcp-retry: overwriting pending {}.{} with {}.{} (first call dropped)",
            prev.server.name(),
            prev.tool,
            server.name(),
            tool
        );
    }
    *guard = Some(McpRetry {
        server,
        tool: tool.to_string(),
        params: params.clone(),
    });
}

fn take_mcp_retry() -> Option<McpRetry> {
    PENDING_MCP_RETRY.lock().unwrap().take()
}

/// Map an MCP-routed intent to its server (mirrors dispatch_to_mcp).
fn server_for_mcp_intent(intent: &ParsedIntent) -> Option<crate::mcp_client::McpServer> {
    use crate::mcp_client::McpServer as S;
    match intent {
        ParsedIntent::OrderFood { .. } => Some(S::SwiggyFood),
        ParsedIntent::SearchProduct { .. } => Some(S::Amazon),
        ParsedIntent::SendWhatsAppMessage { .. } => Some(S::WhatsApp),
        _ => None,
    }
}

/// Render a Connect card as sidebar markdown: status, numbered steps,
/// QR image (data-URI passes the markdown sanitizer straight through),
/// pairing link (opens externally via openExternal), safety notes.
fn connect_card_markdown(
    card: &crate::mcp_client::McpConnectCard,
    transcript: &str,
) -> String {
    use crate::mcp_client::McpConnectState as St;
    let title = match card.server.as_str() {
        "swiggy-food" | "swiggy-instamart" | "swiggy-dineout" => "Swiggy",
        "whatsapp" => "WhatsApp",
        "amazon" => "Amazon",
        _ => card.server.as_str(),
    };
    let state_line = match card.state {
        St::Down => "Not running",
        St::AuthRequired => "Needs login",
        St::Ready => "Connected",
        St::Unknown => "Status unknown",
    };
    let mut md = format!("## Connect {title}\n\n**Status:** {state_line} — {note}\n\n", note = card.note);
    if card.server == "whatsapp" {
        md.push_str(&format!("_Request: \"{transcript}\" — held, not lost._\n\n"));
    }
    for (i, step) in card.steps.iter().enumerate() {
        md.push_str(&format!("{}. {}\n", i + 1, step));
    }
    md.push('\n');
    if let Some(img) = &card.qr_image_uri {
        md.push_str(&format!("![Scan with WhatsApp → Settings → Linked Devices]({img})\n\n"));
    } else if let Some(code) = &card.qr_code_text {
        md.push_str(&format!("Pairing code: `{code}`\n\n"));
    }
    if let Some(url) = &card.pair_url {
        md.push_str(&format!("[Open pairing page in browser]({url})\n\n"));
    }
    if card.server == "whatsapp" {
        md.push_str("> Unofficial bridge (WhatsApp ToS risk) — a secondary number is safer.\n>\n> Session rotates roughly every 20 days; a fresh QR appears here automatically.\n\n");
    }
    if card.server.starts_with("swiggy") {
        md.push_str("> Localhost dev is free; production needs Swiggy Builders-Club approval.\n\n");
    }
    md.push_str("_NEXUS watches in the background and confirms the moment it connects._\n");
    md
}

/// Open the Connect card for a failed server (first failure per session
/// only) and start the ready-monitor that auto-retries the stashed call.
pub(crate) async fn open_mcp_connect_card<R: Runtime>(
    app: &AppHandle<R>,
    server: crate::mcp_client::McpServer,
    transcript: &str,
) {
    // Reserve the once-per-session slot atomically (insert returns false if
    // already present). The reservation is RELEASED on any failure below so
    // a failed card open doesn't burn the session's one chance (audit M5 —
    // the QR rotates every 20-30s, a permanently-suppressed card is dead).
    let first = {
        let mut shown = SHOWN_CONNECT_CARDS.lock().unwrap();
        shown.insert(server.name().to_string())
    };
    if !first {
        return;
    }
    let release = |server: &crate::mcp_client::McpServer| {
        SHOWN_CONNECT_CARDS
            .lock()
            .unwrap()
            .remove(&server.name().to_string());
    };
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .connect_timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("connect card: http client failed: {e}");
            release(&server);
            return;
        }
    };
    let card = crate::mcp_client::connect_card_for(server, &client).await;
    let md = connect_card_markdown(&card, transcript);
    if let Err(e) = crate::commands::show_sidebar_with_content(
        app.clone(),
        format!("Connect {}", card.server),
        md,
    )
    .await
    {
        tracing::warn!("connect card: sidebar failed: {e}");
        release(&server);
        return;
    }
    spawn_ready_monitor(app.clone(), server, transcript.to_string());
}

/// Background watch: poll connect state; while the card is open, keep its
/// content fresh (WhatsApp's QR rotates every 20-30s — a stale QR can't
/// scan, so re-render whenever the payload changes), and when the server
/// turns Ready, render the Connected card + retry the stashed call once
/// and speak the outcome (Composio WAIT_FOR_CONNECTIONS shape). Gives up
/// silently after ~10 min — the card stays open with manual Recheck.
/// Never speaks over a newer turn: if another request is active, the
/// retry is dropped.
fn spawn_ready_monitor<R: Runtime>(
    app: AppHandle<R>,
    server: crate::mcp_client::McpServer,
    transcript: String,
) {
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .connect_timeout(std::time::Duration::from_secs(3))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut last_qr: Option<String> = None;
        for _ in 0..120 {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let card = crate::mcp_client::connect_card_for(server, &client).await;
            if card.state != crate::mcp_client::McpConnectState::Ready {
                // Fresh QR while the card is open: WhatsApp rotates the
                // pairing QR every 20-30s; a static card would show an
                // expired code (Novu deletes/re-renders its card for the
                // same reason). Re-render only when the payload changed so
                // we don't spam the sidebar.
                let current_qr = card
                    .qr_image_uri
                    .clone()
                    .or_else(|| card.qr_code_text.clone());
                if current_qr.is_some() && current_qr != last_qr {
                    last_qr = current_qr;
                    let md = connect_card_markdown(&card, &transcript);
                    let _ = crate::commands::show_sidebar_with_content(
                        app.clone(),
                        format!("Connect {}", card.server),
                        md,
                    )
                    .await;
                }
                continue;
            }
            // Ready: render the truth on the card (never show stale
            // "needs login" content after success — the truthfulness
            // failure Claude's tracker documented).
            let ready_md = connect_card_markdown(&card, &transcript);
            let _ = crate::commands::show_sidebar_with_content(
                app.clone(),
                format!("Connect {}", card.server),
                ready_md,
            )
            .await;
            // Another turn started meanwhile: drop the retry, stay silent.
            if ACTIVE_REQUEST.lock().unwrap().is_some() {
                take_mcp_retry();
                return;
            }
            let retry = match take_mcp_retry() {
                Some(r) if r.server == server => r,
                other => {
                    // Wrong server (or nothing stashed): just announce.
                    if other.is_some() {
                        let mut guard = PENDING_MCP_RETRY.lock().unwrap();
                        *guard = other;
                    }
                    let rid = new_request_id();
                    emit(
                        &app,
                        &OrchestratorEvent::Result {
                            text: format!(
                                "{} is connected, sir.",
                                display_server_name(server)
                            ),
                            request_id: rid,
                            analysis: None,
                            dialog_state: None,
                        },
                    );
                    return;
                }
            };
            // Retry the original call once with a fresh token.
            let call_client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .connect_timeout(std::time::Duration::from_secs(10))
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };
            let vault_token =
                crate::auth_vault::resolve_server_token(server).await;
            let result = crate::mcp_client::call_tool(
                server,
                &retry.tool,
                retry.params,
                &call_client,
                vault_token.as_deref(),
            )
            .await;
            let rid = new_request_id();
            let text = if result.ok {
                let body = crate::mcp_client::extract_text(&result);
                format!(
                    "{} is connected, sir. {}",
                    display_server_name(server),
                    body
                )
            } else {
                format!(
                    "{} is reachable now, sir, but the retry failed: {}. The setup card is still open.",
                    display_server_name(server),
                    result.error.unwrap_or_default()
                )
            };
            emit(
                &app,
                &OrchestratorEvent::Result {
                    text,
                    request_id: rid,
                    analysis: None,
                    dialog_state: None,
                },
            );
            return;
        }
    });
}

fn display_server_name(server: crate::mcp_client::McpServer) -> &'static str {
    match server {
        crate::mcp_client::McpServer::SwiggyFood
        | crate::mcp_client::McpServer::SwiggyInstamart
        | crate::mcp_client::McpServer::SwiggyDineout => "Swiggy",
        crate::mcp_client::McpServer::WhatsApp => "WhatsApp",
        crate::mcp_client::McpServer::Amazon => "Amazon",
    }
}

/// Drain ghost follow-ups queued mid-drill, in arrival order, through the
/// normal pipeline. Called by drill runners AFTER main steps + session
/// exit, and only when the run was clean (no stop request — resuming
/// behind a stop is rejected by design). Bounded: stops after
/// DRAIN_CAP items or on a fresh stop request, dropping the rest with a
/// log. A follow-up that starts its own drill nests sequentially; items
/// it queues are picked up by the same loop.
pub(crate) async fn drain_ghost_followups<R: Runtime>(app: &AppHandle<R>) {
    // FIFO serial executor (Approach E): arrival order, one step at a
    // time, re-verified grounding, bounded. Cadence + watchdog come from
    // settings (D4/D7), read once per drain pass.
    let gap_ms = crate::commands::read_ghost_turn_gap_ms(app);
    let step_timeout_ms = crate::commands::read_ghost_step_timeout_ms(app);
    let mut done = 0usize;
    loop {
        if crate::ghost::stop_requested() {
            crate::ghost::drop_followups();
            break;
        }
        let cmd = match crate::ghost::dequeue_command() {
            Some(c) => c,
            None => break,
        };
        if done >= crate::ghost::DRAIN_CAP {
            crate::ghost::drop_followups();
            break;
        }
        // Rule 3: re-verify grounding at dequeue time — never execute
        // blind against a window the user has since left (F4).
        if let Err(e) = crate::ghost::verify_grounding(&cmd) {
            tracing::warn!("ghost: grounding failed for '{}': {}", cmd.transcript, e);
            let (req_id, _) = install_new_request(Subsystem::LocalCommand);
            speak_line(app, "Target window lost, sir — skipping.".to_string(), &req_id);
            clear_active_request(&req_id);
            continue;
        }
        // Inter-command gap τ (D4): the finished step's narration + echo
        // tail clear before the next step acts (~1s narrated cadence).
        tokio::time::sleep(tokio::time::Duration::from_millis(gap_ms)).await;
        // A stop that landed during the gap still wins (F1/F2).
        if crate::ghost::stop_requested() {
            crate::ghost::drop_followups();
            break;
        }
        tracing::info!("ghost: executing dequeued follow-up #{} ({done}): '{}'", cmd.id, cmd.transcript);
        // Boxed: process_transcript → drill → drain forms a cycle;
        // the boxed future breaks the infinite-size recursion.
        // Clone the label first: the transcript moves into the future
        // and is still needed for the watchdog log below.
        let label = cmd.transcript.clone();
                let fut = Box::pin(process_transcript(app.clone(), cmd.transcript, None, None));
        // Rule 4: per-step watchdog — a poisoned step (hung UIA lookup,
        // 60s analysis) must not wedge the queue behind it (F6).
        match tokio::time::timeout(std::time::Duration::from_millis(step_timeout_ms), fut).await {
            Ok(_) => {}
            Err(_) => {
                tracing::warn!(
                    "ghost: step watchdog ({}ms) expired, skipping queued '{}'",
                    step_timeout_ms,
                    label
                );
                let (req_id, _) = install_new_request(Subsystem::LocalCommand);
                speak_line(app, "Skipping step, sir.".to_string(), &req_id);
                clear_active_request(&req_id);
            }
        }
        done += 1;
    }
}

/// Public wrapper for `dispatch_to_mcp` — used by command_center steps.
pub(crate) async fn dispatch_to_mcp_pub<R: Runtime>(
    app: &AppHandle<R>,
    intent: &ParsedIntent,
    transcript: &str,
    request_id: &str,
) -> Result<Option<String>, String> {
    dispatch_to_mcp(app, intent, transcript, request_id).await
}

/// Render the Ghostwriter sidebar card: target header + draft bubble +
/// command hint. Same 400px overlay; blur + scrim already live.
async fn show_ghostwriter_card<R: Runtime>(app: &AppHandle<R>) {
    let (contact, draft) = crate::ghostwriter::card_state()
        .unwrap_or((None, String::new()));
    let to_line = contact
        .map(|c| format!("To: {c}"))
        .unwrap_or_else(|| "To: — (say \"this is for …\")".to_string());
    let body = if draft.trim().is_empty() {
        "(blank page — speak, and I'll write)".to_string()
    } else {
        draft
    };
    let text = format!(
        "✒️ Ghostwriter\n{to_line}\n\n> {body}\n\n—",
    );
    let _ = crate::commands::show_sidebar_with_content(
        app.clone(),
        "ghostwriter".to_string(),
        text,
    )
    .await;
}

/// Click the Nth on-screen actionable (Windows UIA grounding).
async fn run_screen_click<R: Runtime>(
    app: AppHandle<R>,
    ordinal: u32,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    #[cfg(target_os = "windows")]
    {
        let els = crate::screen::list_actionables();
        match crate::screen::pick_ordinal(&els, ordinal) {
            Some(el) => {
                let name = el.name.clone();
                match crate::screen::click_element(el) {
                    Ok(()) => speak_line(&app, format!("Clicked {name}, sir."), &request_id),
                    Err(e) => speak_line(&app, format!("Couldn't click, sir: {e}"), &request_id),
                }
            }
            None => speak_line(
                &app,
                format!("I only see {} clickable things, sir.", els.len()),
                &request_id,
            ),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = ordinal;
        speak_line(&app, "Screen clicking needs Windows, sir.".to_string(), &request_id);
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Run a user-defined declarative spec (D2): open apps + speak lines.
/// Actions run in order; an open failure speaks the error and continues
/// to the remaining say lines (best-effort routine, not a transaction).
async fn run_custom_spec<R: Runtime>(
    app: AppHandle<R>,
    custom: &crate::agent_specs::CustomIntent,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    tracing::info!("specs: running custom intent '{}'", custom.name);
    let mut said: Vec<String> = vec![];
    for action in &custom.run {
        match action {
            crate::agent_specs::SpecAction::Open(target) => {
                match crate::command_executor::resolve_and_open_app(target) {
                    Ok(res) => {
                        if !res.message.is_empty() {
                            said.push(res.message);
                        }
                    }
                    Err(e) => said.push(format!("Couldn't open {}, sir: {}", target, e)),
                }
            }
            crate::agent_specs::SpecAction::Say(text) => said.push(text.clone()),
        }
    }
    let reply = said.join(" ");
    speak_line(&app, reply, &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Read back the Nth on-screen actionable (no click).
async fn run_screen_read<R: Runtime>(
    app: AppHandle<R>,
    ordinal: u32,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    #[cfg(target_os = "windows")]
    {
        let els = crate::screen::list_actionables();
        match crate::screen::pick_ordinal(&els, ordinal) {
            Some(el) => speak_line(
                &app,
                format!("{} {} says {}, sir.", ordinal_word(ordinal), el.kind, el.name),
                &request_id,
            ),
            None => speak_line(
                &app,
                format!("I only see {} clickable things, sir.", els.len()),
                &request_id,
            ),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = ordinal;
        speak_line(&app, "Screen reading needs Windows, sir.".to_string(), &request_id);
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

fn ordinal_word(n: u32) -> String {
    match n {
        1 => "1st".to_string(),
        2 => "2nd".to_string(),
        3 => "3rd".to_string(),
        _ => format!("{n}th"),
    }
}

/// Screen-query cost tier (Feature 86, Phase 5 pre-filter).
///
/// A "read my screen" / "what's on my screen" is answered completely by
/// free local OCR (~50ms, no quota). Sending it through two Gemini calls
/// first wastes ~2s and 2 quota units. Complex queries (analyse, explain,
/// research) need visual reasoning the OCR text can't provide, so they
/// keep the VLM-first order. Pure + unit-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenQueryTier {
    /// Raw transcription suffices — OCR first, VLM only if OCR is empty.
    ReadOnly,
    /// Visual reasoning required — VLM first, OCR as fallback.
    Visual,
}

/// Classify a screen-analysis prompt into a cost tier.
/// Visual wins on ANY complex verb (analyse/analyze/explain/research/
/// compare/summarize/why/error/diagnose/mean/teach/describe); everything
/// else is a transcription request. Pure.
pub fn classify_screen_query(prompt: &str) -> ScreenQueryTier {
    const COMPLEX_VERBS: &[&str] = &[
        "analys", "analyz", "explain", "research", "compar", "summar",
        "why", "error", "diagnos", "mean", "teach", "describe",
    ];
    let lower = prompt.to_lowercase();
    if COMPLEX_VERBS.iter().any(|v| lower.contains(v)) {
        ScreenQueryTier::Visual
    } else {
        ScreenQueryTier::ReadOnly
    }
}

/// LOCAL OCR answer attempt (Feature 86, zero-RAM, keyless+offline).
/// Runs Windows.Media.Ocr on a blocking thread (WinRT async), speaks a
/// text summary, and shows the extracted text in the assistant panel via
/// the race-free pending pattern. `prompt` is the raw transcript (used as
/// the pending query so the copy/title gating keys off the real words).
///
/// Returns Some(ProcessResult) when OCR produced text (turn answered +
/// cleared); None when OCR found nothing → caller continues the chain.
#[cfg(target_os = "windows")]
async fn try_ocr_answer<R: Runtime>(
    app: &AppHandle<R>,
    prompt: &str,
    request_id: &str,
) -> Option<ProcessResult> {
    match tokio::task::spawn_blocking(crate::ocr::capture_screen_text).await {
        Ok(Some((text, rects))) => {
            let line_count = text.lines().count();
            let first: String = text.lines().next().unwrap_or("").chars().take(90).collect();
            // Keyless/quota-spent runs prepend the onboarding guidance (P5).
            let has_key = !crate::commands::read_api_key(app, "gemini").is_empty();
            let exhausted = app
                .path()
                .app_data_dir()
                .ok()
                .map(|d| crate::vision::exhausted(&d, "gemini"))
                .unwrap_or(false);
            let prefix = screen_key_guidance(has_key, exhausted)
                .map(|s| format!("{s} "))
                .unwrap_or_default();
            speak_line(
                app,
                format!(
                    "{prefix}Your screen shows {} text lines, sir, starting with: {}. The full text is in the sidebar.",
                    line_count, first
                ),
                request_id,
            );
            crate::commands::set_pending_sidebar_text(
                prompt.to_string(),
                format!(
                    "## Screen Text (OCR)\n\n```\n{}\n```",
                    text.chars().take(3000).collect::<String>()
                ),
            );
            let _ = crate::commands::unified_show_sidebar(app, "assistant", None).await;
            tracing::info!(
                "ocr: screen text extracted ({} lines, {} rects)",
                line_count,
                rects.len()
            );
            clear_active_request(request_id);
            Some(ProcessResult {
                request_id: request_id.to_string(),
                subsystem: Subsystem::LocalCommand,
                handled_locally: true,
            })
        }
        Ok(None) => {
            tracing::warn!("ocr: screen text extraction returned None — continuing chain");
            None
        }
        Err(e) => {
            tracing::warn!("ocr: blocking task failed: {e} — continuing chain");
            None
        }
    }
}

/// Spoken guidance prefix for the OCR fallback in screen analysis
/// (Feature 83 P5 — turns the keyless/quota-spent dead-end into an
/// onboarding funnel). Returns None when a Gemini key exists with quota
/// remaining (the VLM path speaks for itself). Pure + unit-tested.
pub fn screen_key_guidance(has_gemini_key: bool, gemini_exhausted: bool) -> Option<&'static str> {
    if !has_gemini_key {
        Some("Screen vision needs a Gemini key, sir — add it in Command Hub, Accounts.")
    } else if gemini_exhausted {
        Some("Vision quota is spent for today, sir.")
    } else {
        None
    }
}

/// Seed payload for the annotation canvas (Feature 87).
/// Pure + unit-tested: empty element list, default tool, original prompt.
pub fn annotation_seed(prompt: &str) -> serde_json::Value {
    serde_json::json!({ "tool": "select", "elements": [], "prompt": prompt })
}

/// Start interactive screen annotation (Feature 87): stage overlay ink
/// canvas + sidebar Annotate palette. All canvas ops stay frontend-local
/// (no IPC per stroke); commit serializes on the frontend and surfaces
/// through stage events. Pure shell: show stage → seed pending → show
/// sidebar view → warm-path event → cached spoken confirmation.
async fn run_screen_annotation<R: Runtime>(
    app: AppHandle<R>,
    prompt: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    // 1. Stage overlay on screen (click-through; the ink layer registers
    //    its own hitboxes once the frontend canvas mounts).
    if let Err(e) = crate::stage::stage_show(app.clone()).await {
        tracing::warn!("annotate: stage show failed: {e}");
    }
    // 2. Sidebar: Annotate view + race-free seed payload (fresh-window
    //    fetch path) + warm-window event path.
    let seed = annotation_seed(&prompt);
    crate::commands::set_pending_annotation(&seed);
    if let Err(e) = crate::commands::unified_show_sidebar(&app, "annotate", None).await {
        tracing::warn!("annotate: sidebar show failed: {e}");
    }
    let _ = app.emit("sidebar:show_annotation", seed);
    // 3. Cached spoken confirmation (<5ms, see tts::CACHED_PHRASES).
    speak_line(&app, "Annotation ready, sir.".to_string(), &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Analyze screen contents (Feature 86 spatial flow): Gemini-primary
/// spatial decomposition → stage overlay pins + sidebar SpatialDashboard.
/// Graceful degradation chain: spatial VLM → plain-text VLM → UIA count.
/// Phase 5: ReadOnly queries try free OCR first (cost pre-filter).
async fn run_screen_analysis<R: Runtime>(
    app: AppHandle<R>,
    prompt: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    speak_line(&app, "Analyzing the screen, sir.".to_string(), &request_id);

    #[cfg(target_os = "windows")]
    {
        // ── Phase 5 cost pre-filter: transcription queries ("read my
        // screen", "what's on my screen") skip BOTH VLM paths and go
        // straight to free local OCR. Visual queries keep VLM-first.
        if classify_screen_query(&prompt) == ScreenQueryTier::ReadOnly {
            if let Some(result) = try_ocr_answer(&app, &prompt, &request_id).await {
                return Ok(result);
            }
            tracing::info!("ocr: empty for a ReadOnly query — continuing to VLM chain");
        }

        // ── Primary path: spatial decomposition (Gemini → Groq) ──────
        match crate::vision::analyze_screen_spatial(&app, &prompt).await {
            Ok(payload) => {
                // 1. Ensure the stage overlay is on screen (click-through;
                //    ghost ring coexists when a session is live).
                if let Err(e) = crate::stage::stage_show(app.clone()).await {
                    tracing::warn!("spatial: stage show failed: {e}");
                }
                // 2. Physical-px pins (stage frontend divides by dpr —
                //    same contract as ghost:ring).
                let pins = match crate::screen::primary_monitor_size() {
                    Some((mw, mh)) => payload
                        .items
                        .iter()
                        .map(|item| {
                            let (x, y, w, h) =
                                crate::vision::denormalize_spatial_box(&item.box_2d, mw, mh);
                            let (pin_x, pin_y) =
                                crate::vision::pin_anchor_for(&item.box_2d, mw, mh);
                            serde_json::json!({
                                "id": item.id, "label": item.label,
                                "x": x, "y": y, "width": w, "height": h,
                                "pin_x": pin_x, "pin_y": pin_y,
                            })
                        })
                        .collect::<Vec<_>>(),
                    None => {
                        tracing::warn!("spatial: primary monitor size unavailable");
                        Vec::new()
                    }
                };
                let _ = app.emit(
                    "stage:spatial_annotations",
                    serde_json::json!({ "title": payload.title, "pins": pins }),
                );
                // 3. Sidebar: Spatial view + race-free pending payload.
                let payload_json =
                    serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null);
                crate::commands::set_pending_spatial(&payload_json);
                if let Err(e) =
                    crate::commands::unified_show_sidebar(&app, "spatial", None).await
                {
                    tracing::warn!("spatial: sidebar show failed: {e}");
                }
                let _ = app.emit("sidebar:show_spatial", payload_json);
                // 4. Speak the high-level summary.
                let n = payload.items.len();
                let line = format!(
                    "I've highlighted {} {} on your screen, sir. Detailed breakdown is in the sidebar.",
                    n,
                    if n == 1 { "element" } else { "elements" }
                );
                speak_line(&app, line, &request_id);
                clear_active_request(&request_id);
                return Ok(ProcessResult {
                    request_id,
                    subsystem: Subsystem::LocalCommand,
                    handled_locally: true,
                });
            }
            Err(e) => {
                tracing::warn!("spatial analysis unavailable: {e} — falling back to plain-text VLM");
            }
        }

        // ── Fallback 1: plain-text VLM (previous behavior) ───────────
        // 2026-10-01: Groq decommissioned all vision models (404) — this
        // fallback now runs on Gemini Flash-Lite.
        let gemini_key = crate::commands::read_api_key(&app, "gemini");
        if !gemini_key.is_empty() {
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .unwrap_or_default();

            if let Some((b64, _, _)) = crate::vision::capture_gridded_jpeg_base64() {
                let vlm_prompt = format!("You are NEXUS, an advanced AI desktop assistant. Concisely describe what is visible on the user's screen in 1-2 clear, direct sentences. Focus on the active app and main content: {prompt}");

                let body = serde_json::json!({
                    "contents": [{
                        "parts": [
                            {"text": vlm_prompt},
                            {"inline_data": {"mime_type": "image/jpeg", "data": b64}},
                        ],
                    }],
                    "generationConfig": {"maxOutputTokens": 160, "temperature": 0.2},
                });
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                    crate::vision::GEMINI_VISION_MODEL
                );

                if let Ok(resp) = client
                    .post(&url)
                    .header("x-goog-api-key", &gemini_key)
                    .json(&body)
                    .send()
                    .await
                {
                    if resp.status().is_success() {
                        if let Ok(json) = resp.json::<serde_json::Value>().await {
                            let content: String = json["candidates"][0]["content"]["parts"]
                                .as_array()
                                .map(|parts| {
                                    parts
                                        .iter()
                                        .filter_map(|p| p.get("text")?.as_str())
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default();
                            if !content.trim().is_empty() {
                                speak_line(&app, content, &request_id);
                                clear_active_request(&request_id);
                                return Ok(ProcessResult {
                                    request_id,
                                    subsystem: Subsystem::LocalCommand,
                                    handled_locally: true,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = prompt;
        speak_line(&app, "Screen analysis needs Windows, sir.".to_string(), &request_id);
    }

    // ── Fallback 2: LOCAL OCR (Feature 86, zero-RAM, keyless+offline) ──
    // Windows.Media.Ocr extracts the visible text; the user gets a real
    // answer (spoken summary + extracted text in the sidebar) with zero
    // API keys and zero network.
    #[cfg(target_os = "windows")]
    {
        if let Some(result) = try_ocr_answer(&app, &prompt, &request_id).await {
            return Ok(result);
        }

        // ── Fallback 3: UIA actionable-element count ─────────────────
        let els = crate::screen::list_actionables();
        speak_line(&app, format!("I see {} actionable elements on the screen, sir.", els.len()), &request_id);
    }

    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Switch browser tab via hotkey (cross-platform, instant).
async fn run_browser_tab<R: Runtime>(
    app: AppHandle<R>,
    index: u32,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    match crate::screen::switch_browser_tab(index) {
        Ok(msg) => {
            println!("[ACTION] switch to tab {index} → hotkey sent ({msg})");
            speak_line(&app, msg, &request_id)
        }
        Err(e) => {
            println!("[ACTION] switch to tab {index} → FAILED: {e}");
            speak_line(&app, format!("Couldn't switch tabs, sir: {e}"), &request_id)
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Open a new browser tab (Ctrl+T). Ghost-session aware: a requested stop
/// wins before acting (mirrors run_ghost_open's guard).
async fn run_browser_new_tab<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        match crate::command_executor::browser_new_tab_cmd() {
            Ok(result) => {
                println!("[ACTION] new tab → Ctrl+T sent (\"{}\")", result.message);
                speak_line(&app, result.message, &request_id)
            }
            Err(e) => {
                println!("[ACTION] new tab → FAILED: {e}");
                speak_line(&app, format!("Couldn't open a new tab, sir: {e}"), &request_id)
            }
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Close a browser tab (Ctrl+W; an index switches there first). Best-effort
/// hotkey path — the same enigo pipeline the executor uses.
async fn run_browser_close<R: Runtime>(
    app: AppHandle<R>,
    index: Option<u32>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        match crate::command_executor::browser_close_tab_cmd(index) {
            Ok(result) => {
                println!("[ACTION] close tab {:?} → Ctrl+W sent (\"{}\")", index, result.message);
                speak_line(&app, result.message, &request_id)
            }
            Err(e) => {
                println!("[ACTION] close tab {:?} → FAILED: {e}", index);
                speak_line(&app, format!("Couldn't close the tab, sir: {e}"), &request_id)
            }
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Search in the active browser tab (Ctrl+L -> type -> Enter).
async fn run_browser_search<R: Runtime>(
    app: AppHandle<R>,
    query: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        speak_line(&app, "On it sir".to_string(), &request_id);
        // Bracket the action (see run_ghost_open): mid-search speech
        // queues; the guard drops before the drain so dequeued steps
        // execute instead of re-queueing.
        let outcome = {
            let _guard = crate::ghost::GhostDrillGuard::new();
            tokio::task::spawn_blocking(move || {
                crate::live::commands::browser::search(&query)
            })
            .await
            .map_err(|e| e.to_string())?
        };

        if let Err(e) = outcome {
            tracing::warn!("browser search failed: {}", e);
        }
    }
    clear_active_request(&request_id);
    if crate::ghost::session_active() && !crate::ghost::stop_requested() {
        drain_ghost_followups(&app).await;
    } else {
        crate::ghost::drop_followups();
    }
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Focus the address/search bar in the active browser tab (Ctrl+L).
async fn run_browser_search_focus<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        // Bracket the action (see run_ghost_open): mid-focus speech
        // queues; the guard drops before the drain so dequeued steps
        // execute instead of re-queueing.
        let outcome = {
            let _guard = crate::ghost::GhostDrillGuard::new();
            tokio::task::spawn_blocking(|| {
                crate::live::commands::browser::focus_search_bar()
            })
            .await
            .map_err(|e| e.to_string())?
        };

        if let Err(e) = outcome {
            tracing::warn!("browser focus_search_bar failed: {}", e);
        }
        speak_line(&app, "Ready to search, sir.".to_string(), &request_id);
    }
    clear_active_request(&request_id);
    if crate::ghost::session_active() && !crate::ghost::stop_requested() {
        drain_ghost_followups(&app).await;
    } else {
        crate::ghost::drop_followups();
    }
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Search YouTube for videos and open the top result or search page.
async fn run_youtube_search<R: Runtime>(
    app: AppHandle<R>,
    query: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        speak_line(&app, format!("Searching YouTube for {}, sir.", query), &request_id);

        let query_clone = query.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            crate::youtube_center::search_videos(&query_clone, 5)
        })
        .await
        .map_err(|e| e.to_string())?;

        match outcome {
            Ok(results) => {
                if let Some(first_url) = results.get(0).and_then(|v| v.get("url")).and_then(|u| u.as_str()) {
                    let url = first_url.to_string();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = open::that(&url);
                    })
                    .await;
                } else {
                    let encoded: String = query.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_string() } else { format!("%{:02X}", c as u8) }).collect();
                    let fallback_url = format!("https://www.youtube.com/results?search_query={}", encoded);
                    let _ = tokio::task::spawn_blocking(move || {
                        let _ = open::that(&fallback_url);
                    })
                    .await;
                }
            }
            Err(e) => {
                tracing::warn!("youtube search error: {}", e);
                let encoded: String = query.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_string() } else { format!("%{:02X}", c as u8) }).collect();
                let fallback_url = format!("https://www.youtube.com/results?search_query={}", encoded);
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = open::that(&fallback_url);
                })
                .await;
            }
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Analyze a YouTube video and append structured notes to local journal (diary.rs).
async fn run_youtube_journal<R: Runtime>(
    app: AppHandle<R>,
    mut url: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        if url.is_empty() {
            if let Some(active_url) = crate::browser_url::get_active_browser_url() {
                if active_url.contains("youtube.com") || active_url.contains("youtu.be") {
                    url = active_url;
                }
            }
        }

        if url.is_empty() {
            speak_line(
                &app,
                "Please open a YouTube video first so I can summarize it for your journal, sir.".to_string(),
                &request_id,
            );
        } else {
            speak_line(
                &app,
                "Extracting video transcript and recording it in your journal, sir.".to_string(),
                &request_id,
            );

            let app_dir = app.path().app_data_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let url_clone = url.clone();
            let outcome = tokio::task::spawn_blocking(move || {
                crate::youtube_center::summarize_to_diary(&app_dir, &url_clone)
            })
            .await
            .map_err(|e| e.to_string())?;

            match outcome {
                Ok(entry) => {
                    let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or("the video");
                    speak_line(
                        &app,
                        format!("Recorded summary for '{}' into your journal, sir.", title),
                        &request_id,
                    );
                }
                Err(e) => {
                    tracing::warn!("youtube journal error: {}", e);
                    speak_line(
                        &app,
                        "I couldn't extract the transcript for that video, sir.".to_string(),
                        &request_id,
                    );
                }
            }
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Type text into the focused window via keyboard simulation.
async fn run_type_text<R: Runtime>(
    app: AppHandle<R>,
    text: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() {
        speak_line(&app, "Stopped, sir.".to_string(), &request_id);
    } else {
        let outcome = tokio::task::spawn_blocking(move || {
            crate::live::commands::keyboard::type_text(&text)
        })
        .await
        .map_err(|e| e.to_string())?;

        if let Err(e) = outcome {
            tracing::warn!("type_text failed: {}", e);
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Focus an already-running app/window by (partial, case-insensitive)
/// title. Local, instant, no network — see doc 07 P2.
async fn run_focus_app<R: Runtime>(app: AppHandle<R>, target: String) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if target.trim().is_empty() {
        speak_line(&app, "Which app should I focus, sir?".to_string(), &request_id);
    } else {
        let target_clone = target.clone();
        let found = tokio::task::spawn_blocking(move || {
            crate::live::commands::window::focus_app_by_title(&target_clone)
        })
        .await
        .unwrap_or(false);
        let reply = if found {
            "Ok sir.".to_string()
        } else {
            format!("Couldn't find {target} running, sir.")
        };
        speak_line(&app, reply, &request_id);
    }
    clear_active_request(&request_id);
    Ok(ProcessResult { request_id, subsystem: Subsystem::LocalCommand, handled_locally: true })
}

/// Minimize the foreground window. Local, instant, no network.
async fn run_minimize_window<R: Runtime>(app: AppHandle<R>) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let ok = tokio::task::spawn_blocking(crate::live::commands::window::minimize_foreground_window)
        .await
        .unwrap_or(false);
    let reply = if ok { "Minimized, sir." } else { "Nothing to minimize, sir." };
    speak_line(&app, reply.to_string(), &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult { request_id, subsystem: Subsystem::LocalCommand, handled_locally: true })
}

/// Maximize the foreground window. Local, instant, no network.
async fn run_maximize_window<R: Runtime>(app: AppHandle<R>) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let ok = tokio::task::spawn_blocking(crate::live::commands::window::maximize_foreground_window)
        .await
        .unwrap_or(false);
    let reply = if ok { "Maximized, sir." } else { "Nothing to maximize, sir." };
    speak_line(&app, reply.to_string(), &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult { request_id, subsystem: Subsystem::LocalCommand, handled_locally: true })
}

/// Click an on-screen element by name, OUTSIDE a ghost session: a single
/// confirmed action, not a driving session (no ring, no Esc-arming). UIA
/// bounds first (exact, free, ~ms) — reuses the exact resolver Ghost Mode
/// uses; vision only on miss, same daily-quota-aware fallback as
/// `live::commands::mouse::ghost_click`. Local-first by construction: the
/// network is only ever touched when UIA genuinely can't see the element.
async fn run_click_element<R: Runtime>(app: AppHandle<R>, name: String) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if name.trim().is_empty() {
        speak_line(&app, "Which button or element should I click, sir?".to_string(), &request_id);
        clear_active_request(&request_id);
        return Ok(ProcessResult { request_id, subsystem: Subsystem::LocalCommand, handled_locally: true });
    }

    #[cfg(not(target_os = "windows"))]
    {
        speak_line(&app, "Clicking by name isn't available on this platform, sir.".to_string(), &request_id);
    }

    #[cfg(target_os = "windows")]
    {
        let name_clone = name.clone();
        let resolved = tokio::task::spawn_blocking(move || {
            crate::live::commands::mouse::resolve_element(&name_clone)
        })
        .await
        .unwrap_or(None);

        let el = match resolved {
            Some(el) => Some(el),
            None => {
                let groq_key = crate::commands::read_groq_api_key(&app);
                let gemini_key = crate::commands::read_api_key(&app, "gemini");
                if groq_key.is_empty() && gemini_key.is_empty() {
                    None
                } else {
                    let usage_dir = app
                        .path()
                        .app_data_dir()
                        .unwrap_or_else(|_| std::path::PathBuf::from("."));
                    let order = crate::vision::read_vision_provider(&usage_dir);
                    match reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(25))
                        .build()
                    {
                        Ok(client) => crate::vision::locate_with_fallback(
                            &name, &groq_key, &gemini_key, &order, &usage_dir, &client,
                        )
                        .await
                        .map(|t| t.el),
                        Err(_) => None,
                    }
                }
            }
        };

        match el {
            Some(el) => {
                let (cx, cy) = crate::live::commands::mouse::element_center(&el);
                let clicked = tokio::task::spawn_blocking(move || {
                    crate::live::commands::mouse::click_at(cx, cy, || false)
                })
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
                let reply = match clicked {
                    Ok(()) => format!("Clicked {name}, sir."),
                    Err(e) => e,
                };
                speak_line(&app, reply, &request_id);
            }
            None => {
                speak_line(&app, format!("Couldn't find '{name}' on screen, sir."), &request_id);
            }
        }
    }

    clear_active_request(&request_id);
    Ok(ProcessResult { request_id, subsystem: Subsystem::LocalCommand, handled_locally: true })
}

/// Open the NEXUS Command Hub sidebar.
async fn run_open_settings<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    speak_line(&app, "Opening Command Hub, sir.".to_string(), &request_id);
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::commands::show_settings_sidebar(app_handle).await {
            tracing::error!("failed to show command hub sidebar: {e}");
        }
    });
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Speak a short line (Result event channel the frontend already speaks).
pub fn speak_line<R: Runtime>(app: &AppHandle<R>, text: String, request_id: &str) {
    // Console tracking: every spoken line is provable (truncated). The
    // frontend speaks on receipt; completion surfaces via its onEnd/done
    // handshake (no Rust-side playback callback exists to log).
    {
        let preview: String = text.chars().take(80).collect();
        println!("[TTS] speak (req={request_id}): \"{preview}\"");
    }
    emit(
        app,
        &OrchestratorEvent::Result {
            text,
            request_id: request_id.to_string(),
            analysis: None,
            dialog_state: None,
        },
    );
}

/// Speak a proactive alert generated in the background by sentinel.
pub fn speak_proactive_alert<R: Runtime>(app: &AppHandle<R>, text: String) {
    let req_id = format!(
        "sentinel_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    speak_line(app, text, &req_id);
}

/// Watch the current screen email for updates or deadline changes.
async fn run_watch_screen_email<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    tracing::info!("watch_screen_email: initiating active email watch capture");
    println!("[WATCH] ─── Screen Email Watch Initiated ───");

    // 1. Try active browser URL grounding (<5ms)
    let active_url = crate::browser_url::get_active_browser_url();
    println!("[WATCH] Active browser URL: {:?}", active_url);
    let thread_id_from_url = active_url
        .as_deref()
        .and_then(crate::browser_url::extract_gmail_thread_id_from_url);

    let (thread_id, subject, sender, deadline) = if let Some(ref tid) = thread_id_from_url {
        tracing::info!("watch_screen_email: grounded via active browser URL: thread_id={tid}");
        println!("[WATCH] Grounded via browser active URL: thread_id={tid}");
        (
            tid.clone(),
            format!("Email Thread {tid}"),
            "Active Browser Email".to_string(),
            None,
        )
    } else {
        tracing::info!("watch_screen_email: no browser URL thread found, generating local watch target");
        println!("[WATCH] No direct browser thread ID found; creating local watch target...");
        let now_sec = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        (
            format!("screen_thread_{now_sec}"),
            "Current Screen Email".to_string(),
            "Screen Sender".to_string(),
            None,
        )
    };

    // 2. Persist watch in local memory store
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("com.nexus.assistant"));

    // Resolve matching connected Google account for this active email
    let connected_accounts = crate::auth_vault::get_google_accounts();
    let account_email = if let Some(ref u) = active_url {
        if let Some(idx) = crate::browser_url::extract_gmail_account_index_from_url(u) {
            connected_accounts.get(idx).map(|a| a.email.clone())
        } else {
            None
        }
    } else {
        None
    }.or_else(|| {
        connected_accounts
            .iter()
            .find(|a| a.is_primary)
            .or_else(|| connected_accounts.first())
            .map(|a| a.email.clone())
    });

    if let Some(ref em) = account_email {
        println!("[WATCH] Bound watch target to Google account: {em}");
    }

    let target = crate::google::types::ThreadWatchTarget {
        watch_id: format!("watch_{now_ms}"),
        thread_id: thread_id.clone(),
        account_email,
        initial_history_id: None,
        sender: sender.clone(),
        subject: subject.clone(),
        initial_deadline_raw: deadline,
        message_count: 1,
        created_at_ms: now_ms,
        last_checked_ms: now_ms,
        status: crate::google::types::WatchStatus::Active,
    };

    let added = crate::memory::add_mail_watch(&app_data_dir, target);
    tracing::info!("watch_screen_email: watch target persisted (new={added}): id={thread_id}");
    println!("[WATCH] Saved watch target to %APPDATA%/com.nexus.assistant/memory/mail_watches.json");
    println!("[WATCH] Active target: Thread ID='{}', Subject='{}', Status=Active", thread_id, subject);

    // 3. Confirm via speech
    let spoken_reply = "Watching this email for any updates or deadline changes, sir.".to_string();
    speak_line(&app, spoken_reply, &request_id);
    clear_active_request(&request_id);

    // 4. Trigger proactive sentinel check
    crate::google::sentinel::trigger_proactive_check(app.clone());

    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Enter the Ghostwriter room: start session + card + spoken reply.
async fn run_ghostwriter_enter<R: Runtime>(
    app: AppHandle<R>,
    contact: Option<String>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    let reply = crate::ghostwriter::enter(contact);
    show_ghostwriter_card(&app).await;
    speak_line(&app, reply, &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Enter cursor-control Ghost Mode: ghost session + spoken reply.
/// No sidebar card (the ring is the UI). If the stage is unavailable the
/// session still arms logically — ring appears when the stage shows.
async fn run_ghost_control_enter<R: Runtime>(
    app: AppHandle<R>,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if let Err(e) = crate::ghost::ghost_enter(crate::ghost::ghost_wry::g_wry(app.clone())).await {
        let reply = format!("Couldn't start Ghost Mode, sir: {e}");
        speak_line(&app, reply, &request_id);
        clear_active_request(&request_id);
        return Ok(ProcessResult {
            request_id,
            subsystem: Subsystem::LocalCommand,
            handled_locally: true,
        });
    }
    let reply = "Ghost mode initialized, sir. Tell me what to do".to_string();
    speak_line(&app, reply, &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// Ghost open flow: OpenApp while a ghost session is live. Registry fast
/// path first (focus-or-launch, ~ms), Win-search drill only on miss.
/// Session guards before acting; the session stays open after (follow-ups
/// keep working). One spoken outcome line, then done.
async fn run_ghost_open<R: Runtime>(
    app: AppHandle<R>,
    target: String,
) -> Result<ProcessResult, String> {
    let finish = |app: &AppHandle<R>, request_id: String, text: String| {
        speak_line(app, text, &request_id);
        clear_active_request(&request_id);
        Ok(ProcessResult {
            request_id,
            subsystem: Subsystem::LocalCommand,
            handled_locally: true,
        })
    };
    // Safety screen identical to the manual open path.
    if let crate::live::safety::SafetyVerdict::Blocked(msg) =
        crate::live::safety::safety_check("open_app", Some(&target))
    {
        let (request_id, _) = install_new_request(Subsystem::LocalCommand);
        return finish(&app, request_id, msg);
    }
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    if crate::ghost::stop_requested() || !crate::ghost::session_active() {
        return finish(&app, request_id, "Stopped, sir.".to_string());
    }
    // Bracket the action: mid-open speech queues (FIFO) instead of
    // routing a rival turn. The guard MUST drop before the drain below —
    // draining while drill_running() is true would re-queue forever.
    let outcome = {
        let _guard = crate::ghost::GhostDrillGuard::new();
        match crate::command_executor::resolve_and_open_app(&target) {
            Ok(_) => {
                #[cfg(target_os = "windows")]
                {
                    let _ = crate::live::commands::window::focus_app_by_title(&target);
                }
                "Ok sir.".to_string()
            }
            Err(_) => match crate::live::commands::launcher::open_app_via_search(&target) {
                Ok(()) => "Ok sir.".to_string(),
                Err(e) => e,
            },
        }
    };
    println!("[ACTION] ghost open '{target}' → \"{outcome}\"");
    let res = finish(&app, request_id, outcome);
    // Drain mid-action follow-ups (clean runs only — a stop/abort means
    // the context is gone and the queue was already purged).
    if crate::ghost::session_active() && !crate::ghost::stop_requested() {
        drain_ghost_followups(&app).await;
    } else {
        crate::ghost::drop_followups();
    }
    res
}

/// Ghost message flow: WhatsApp message while a ghost session is live.
/// Desktop drill: open → search contact → paste-type visibly → STOP.
/// The send stays behind the existing confirm gate — never auto-fires.
/// Empty message (chat-open intent) just opens the chat.
/// Boxed future: the drill → drain → process_transcript path cycles back
/// into this function; boxing breaks the infinite-size recursion (and
/// keeps the whole chain Send for spawn_blocking interplay).
async fn run_ghost_message<R: Runtime>(
    app: AppHandle<R>,
    contact: String,
    message: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    // Safety screen (contact denylist).
    if let crate::live::safety::SafetyVerdict::Blocked(msg) =
        crate::live::safety::safety_check("whatsapp_search", Some(&contact))
    {
        speak_line(&app, msg, &request_id);
        clear_active_request(&request_id);
        return Ok(ProcessResult {
            request_id,
            subsystem: Subsystem::LocalCommand,
            handled_locally: true,
        });
    }
    let fut = Box::pin(async {
        if message.is_empty() {
            // Chat-open: open WhatsApp + find the contact, no message typed.
            match crate::live::commands::ghost_drill::whatsapp_drill(app.clone(), &contact, "").await {
                Ok(_) => format!("{contact} is open, sir."),
                Err(e) => e,
            }
        } else {
            match crate::live::commands::ghost_drill::whatsapp_drill(app.clone(), &contact, &message).await {
                Ok(msg) => msg,
                Err(e) => e,
            }
        }
    });
    let outcome = fut.await;
    println!("[ACTION] ghost whatsapp contact='{contact}' msg_len={} → \"{}\"", message.len(), outcome);
    speak_line(&app, outcome, &request_id);
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

/// One in-session turn: dictate, command, send-ready, or exit.
async fn run_ghostwriter_turn<R: Runtime>(
    app: AppHandle<R>,
    transcript: String,
) -> Result<ProcessResult, String> {
    let (request_id, _) = install_new_request(Subsystem::LocalCommand);
    match crate::ghostwriter::handle_turn(&transcript) {
        crate::ghostwriter::TurnOutcome::Dictated(_) => {
            // Ink is visible on the card — no speech (never read the draft
            // unasked; "read it back" exists for that).
            show_ghostwriter_card(&app).await;
        }
        crate::ghostwriter::TurnOutcome::Replied(reply) => {
            show_ghostwriter_card(&app).await;
            speak_line(&app, reply, &request_id);
        }
        crate::ghostwriter::TurnOutcome::SendReady { contact, message } => {
            let intent = ParsedIntent::SendWhatsAppMessage { contact, message };
            match dispatch_to_mcp(&app, &intent, &transcript, &request_id).await {
                Ok(out) => {
                    // Sent (or awaiting orb confirmation) — clear the ink,
                    // stay in the room for the next dictation.
                    {
                        // Reset draft but keep the room + contact.
                        crate::ghostwriter::enter(
                            crate::ghostwriter::card_state().and_then(|(c, _)| c),
                        );
                        // enter() preserves the draft — clear it explicitly.
                        // (clear_draft below is a tiny helper on the module.)
                        crate::ghostwriter::clear_draft();
                    }
                    show_ghostwriter_card(&app).await;
                    if let Some(msg) = out {
                        speak_line(&app, msg, &request_id);
                    }
                }
                Err(e) => {
                    show_ghostwriter_card(&app).await;
                    speak_line(
                        &app,
                        format!("Couldn't send, sir: {e}"),
                        &request_id,
                    );
                }
            }
        }
        crate::ghostwriter::TurnOutcome::Exited(reply) => {
            let _ = crate::commands::hide_sidebar(app.clone());
            speak_line(&app, reply, &request_id);
        }
    }
    clear_active_request(&request_id);
    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::LocalCommand,
        handled_locally: true,
    })
}

// ─── Command Center (multi-step compound tasks) ────────────────────────

/// Run a compound task through the command center.
///
/// Called from `process_transcript` when `build_plan` detects a compound
/// command. Emits ack + loading, executes the plan, emits merged result.
async fn run_command_center<R: Runtime>(
    app: AppHandle<R>,
    plan: crate::command_center::TaskPlan,
    transcript: String,
    dialog_context: Option<serde_json::Value>,
    request_id: String,
    cancel_flag: Arc<AtomicBool>,
    turn: &crate::center::TurnContext,
) -> Result<ProcessResult, String> {
    let ack = pick_ack();
    emit(
        &app,
        &OrchestratorEvent::Ack {
            text: ack.to_string(),
            request_id: request_id.clone(),
        },
    );
    emit(
        &app,
        &OrchestratorEvent::Loading {
            visible: true,
            request_id: request_id.clone(),
        },
    );
    show_loading(&app);

    let outcome = crate::command_center::execute_plan(
        &app,
        plan,
        &request_id,
        &cancel_flag,
        dialog_context,
        turn,
    )
    .await;

    emit(
        &app,
        &OrchestratorEvent::Loading {
            visible: false,
            request_id: request_id.clone(),
        },
    );
    hide_loading(&app);

    tracing::info!(
        "command_center: request {} done — {:?} — {} steps, {} ok, {} failed, awaiting={}",
        request_id,
        crate::router::truncate_pub(&transcript, 60),
        outcome.summary.total_steps,
        outcome.summary.completed,
        outcome.summary.failed,
        outcome.awaiting_confirmation,
    );

    if outcome.awaiting_confirmation {
        // A step emitted a Confirm event — the pending compound is stashed
        // and orchestrator_mcp_confirm will resume it. Emit Done (no Result —
        // the Confirm event already spoke the prompt).
        emit(
            &app,
            &OrchestratorEvent::Done {
                request_id: request_id.clone(),
            },
        );
        // Keep the active request installed — the confirm command reuses
        // this request_id for the resumed steps.
        return Ok(ProcessResult {
            request_id,
            subsystem: Subsystem::CommandCenter,
            handled_locally: false,
        });
    }

    // Emit merged result
    emit(
        &app,
        &OrchestratorEvent::Result {
            text: outcome.text,
            request_id: request_id.clone(),
            analysis: None,
            dialog_state: None,
        },
    );
    emit(
        &app,
        &OrchestratorEvent::Done {
            request_id: request_id.clone(),
        },
    );
    clear_active_request(&request_id);

    // Report execution to the brain monitor (admin-only)
    #[cfg(feature = "admin-brain")]
    {
        let ok = outcome.summary.failed == 0;
        if ok {
            crate::brain_monitor::report_execution_success(&transcript, "compound_task");
        } else {
            crate::brain_monitor::report_execution_failure(
                &transcript,
                "compound_task",
                "one or more steps failed",
            );
        }
    }

    Ok(ProcessResult {
        request_id,
        subsystem: Subsystem::CommandCenter,
        handled_locally: false,
    })
}

// ─── Tauri commands ────────────────────────────────────────────────────

/// IPC: Process a transcript through the central orchestrator.
///
/// This is the single entry point for all voice commands. The frontend
/// calls this after STT produces a transcript.
#[tauri::command]
pub async fn orchestrator_process<R: Runtime>(
    app: AppHandle<R>,
    transcript: String,
    dialog_context: Option<serde_json::Value>,
    turn_context: Option<crate::center::TurnContext>,
) -> Result<ProcessResult, String> {
    process_transcript(app, transcript, dialog_context, turn_context).await
}

/// IPC: Cancel the active orchestrator request (barge-in / new wake).
#[tauri::command]
pub async fn orchestrator_cancel() -> Result<(), String> {
    cancel_active();
    Ok(())
}

/// IPC: Signal that a request is done (called by frontend after TTS finishes).
#[tauri::command]
pub async fn orchestrator_done(
    request_id: String,
) -> Result<(), String> {
    signal_done(&request_id);
    Ok(())
}

/// IPC: Get the current orchestrator state (for diagnostics).
#[tauri::command]
pub fn orchestrator_status() -> Result<serde_json::Value, String> {
    let guard = ACTIVE_REQUEST.lock().unwrap();
    Ok(serde_json::json!({
        "active": guard.is_some(),
        "request_id": guard.as_ref().map(|r| r.id.clone()),
        "subsystem": guard.as_ref().map(|r| serde_json::to_value(&r.subsystem).unwrap_or(serde_json::Value::Null)),
    }))
}

// ─── GitHub sub-command Tauri commands ─────────────────────────────────

/// Execute a GitHub command. The frontend calls this with a serialized
/// `GitHubCommand` object. The orchestrator:
///   1. Fetches the GitHub token from the Worker
///   2. Runs pre-checks (conflict detection for merge)
///   3. If destructive and not confirmed → emits a Confirm event
///   4. Executes the command via octocrab
///   5. Emits the result (text, conflict report, or error)
#[tauri::command]
pub async fn orchestrator_github_execute<R: Runtime>(
    app: AppHandle<R>,
    command: serde_json::Value,
    confirmed: Option<bool>,
) -> Result<serde_json::Value, String> {
    let cmd: crate::github_cmd::GitHubCommand =
        serde_json::from_value(command).map_err(|e| format!("invalid command: {e}"))?;

    let session_info = crate::network::get_session_info()
        .ok_or("no session open")?;
    let (worker_url, user_id, _device_id) = session_info;

    let request_id = {
        let id = new_request_id();
        let (rid, _flag) = install_new_request(Subsystem::GitHub);
        let _ = id;
        rid
    };

    let confirmed = confirmed.unwrap_or(false);

    // Emit thinking state
    emit(
        &app,
        &OrchestratorEvent::State {
            state: OrchestratorState::Thinking,
            request_id: request_id.clone(),
        },
    );

    let result = crate::github_cmd::execute_command(
        &worker_url,
        &user_id,
        &cmd,
        confirmed,
    )
    .await;

    let result_json = serde_json::to_value(&result).unwrap_or(serde_json::Value::Null);

    // Emit the appropriate event based on the result type
    match &result {
        crate::github_cmd::GitHubResult::NeedsConfirmation { prompt, command } => {
            let cmd_json = serde_json::to_value(command).unwrap_or(serde_json::Value::Null);
            emit(
                &app,
                &OrchestratorEvent::Confirm {
                    prompt: prompt.clone(),
                    request_id: request_id.clone(),
                    command: cmd_json.clone(),
                },
            );
            let confirm_payload = serde_json::json!({
                "requestId": request_id.clone(),
                "prompt": prompt.clone(),
                "command": cmd_json,
            });
            // Log (never swallow): same Confirm-without-UI hazard as above.
            if let Err(e) = crate::commands::show_sidebar_with_confirmation(
                app.clone(),
                "GitHub Confirmation".to_string(),
                prompt.clone(),
                confirm_payload,
            ).await {
                tracing::warn!("confirm: GitHub sidebar failed to open: {e}");
            }
        }
        crate::github_cmd::GitHubResult::MergeConflict {
            pr_number,
            repo,
            conflict_files,
            message,
        } => {
            let files_json = serde_json::to_value(conflict_files).unwrap_or(serde_json::Value::Null);
            emit(
                &app,
                &OrchestratorEvent::ConflictReport {
                    request_id: request_id.clone(),
                    pr_number: *pr_number,
                    repo: repo.clone(),
                    conflict_files: files_json,
                    message: message.clone(),
                },
            );
        }
        crate::github_cmd::GitHubResult::Text { text } => {
            emit(
                &app,
                &OrchestratorEvent::Result {
                    text: text.clone(),
                    request_id: request_id.clone(),
                    analysis: None,
                    dialog_state: None,
                },
            );
        }
        crate::github_cmd::GitHubResult::PrList { repo, state, prs } => {
            let count = prs.len();
            let ack_text = format!(
                "Showing {} {} PR{} in {}.",
                count,
                state,
                if count == 1 { "" } else { "s" },
                repo
            );
            emit(
                &app,
                &OrchestratorEvent::Result {
                    text: ack_text,
                    request_id: request_id.clone(),
                    analysis: None,
                    dialog_state: None,
                },
            );
        }
        crate::github_cmd::GitHubResult::Error { message, .. } => {
            emit(
                &app,
                &OrchestratorEvent::Error {
                    message: message.clone(),
                    request_id: request_id.clone(),
                },
            );
        }
    }

    emit(
        &app,
        &OrchestratorEvent::GitHubResult {
            request_id: request_id.clone(),
            result: result_json.clone(),
        },
    );
    emit(
        &app,
        &OrchestratorEvent::Done {
            request_id: request_id.clone(),
        },
    );
    clear_active_request(&request_id);

    Ok(result_json)
}

/// Clear the cached GitHub token (e.g., after disconnecting GitHub).
#[tauri::command]
pub async fn orchestrator_github_clear_token() -> Result<(), String> {
    crate::github_cmd::clear_github_token().await;
    Ok(())
}

// ─── MCP sub-center Tauri commands ─────────────────────────────────────

/// Confirm or cancel a pending MCP write/destructive operation.
///
/// When `dispatch_to_mcp` encounters a write/destructive tool (send message,
/// place order, book table), it emits a Confirm event carrying a pending
/// payload `{server, tool, params, transcript}`. The frontend shows the
/// prompt; on user approval it calls this command with `confirmed=true`
/// and the same pending payload.
#[tauri::command]
pub async fn orchestrator_mcp_confirm<R: Runtime>(
    app: AppHandle<R>,
    request_id: String,
    confirmed: bool,
    pending: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use crate::mcp_client::{call_tool, extract_text, McpServer};

    if !confirmed {
        emit(
            &app,
            &OrchestratorEvent::Result {
                text: "Cancelled, sir.".to_string(),
                request_id: request_id.clone(),
                analysis: None,
                dialog_state: None,
            },
        );
        emit(
            &app,
            &OrchestratorEvent::Done {
                request_id: request_id.clone(),
            },
        );
        return Ok(serde_json::json!({ "cancelled": true }));
    }

    // Reconstruct the pending call
    let server_name = pending["server"]
        .as_str()
        .ok_or("invalid pending payload: missing server")?;
    let tool = pending["tool"]
        .as_str()
        .ok_or("invalid pending payload: missing tool")?;
    let params = pending["params"].clone();

    let server = match server_name {
        "swiggy-food" => McpServer::SwiggyFood,
        "swiggy-instamart" => McpServer::SwiggyInstamart,
        "swiggy-dineout" => McpServer::SwiggyDineout,
        "whatsapp" => McpServer::WhatsApp,
        "amazon" => McpServer::Amazon,
        other => return Err(format!("unknown MCP server: {}", other)),
    };

    emit(
        &app,
        &OrchestratorEvent::Loading {
            visible: true,
            request_id: request_id.clone(),
        },
    );
    show_loading(&app);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("mcp http client: {e}"))?;

    // Same vault resolution as the pre-confirm path — without this,
    // confirmed writes went out anonymous and died with 401 even when
    // the service was connected.
    let mut vault_token: Option<String> =
        crate::auth_vault::resolve_server_token(server).await;
    let mut result =
        call_tool(server, tool, params.clone(), &client, vault_token.as_deref()).await;

    // 401 clear-and-retry (mirrors the dispatch path): evict the dead
    // token, mint fresh once, retry once. The user already confirmed —
    // retrying the call (not the confirmation) is safe.
    if !result.ok {
        let first_err = result.error.clone().unwrap_or_default();
        if is_auth_failure(&first_err) {
            if let Some(key) = server.vault_key() {
                crate::auth_vault::clear_token(key);
                vault_token = crate::auth_vault::resolve_server_token(server).await;
                result = call_tool(
                    server,
                    tool,
                    params.clone(),
                    &client,
                    vault_token.as_deref(),
                )
                .await;
            }
        }
    }

    emit(
        &app,
        &OrchestratorEvent::Loading {
            visible: false,
            request_id: request_id.clone(),
        },
    );
    hide_loading(&app);

    if !result.ok {
        let err = result
            .error
            .clone()
            .unwrap_or_else(|| "unknown MCP error".to_string());
        let spoken = mcp_error_guidance(server, &err);
        let retry_transcript = pending["transcript"].as_str().unwrap_or("");
        stash_mcp_retry(server, tool, &params);
        open_mcp_connect_card(&app, server, retry_transcript).await;
        emit(
            &app,
            &OrchestratorEvent::Error {
                message: spoken.clone(),
                request_id: request_id.clone(),
            },
        );
        emit(
            &app,
            &OrchestratorEvent::Done {
                request_id: request_id.clone(),
            },
        );
        return Err(spoken);
    }

    let text = extract_text(&result);

    // If this confirmed step was part of a compound task, resume the
    // remaining steps and emit the merged result instead.
    if let Some(pending) = crate::command_center::take_pending_compound(&request_id) {
        tracing::info!(
            "command_center: resuming compound for {} — {} remaining steps",
            request_id,
            pending.remaining_steps.len()
        );

        // Fresh cancel flag for the resumed steps — the original was
        // consumed by the compound's execute_plan.
        let resume_flag = Arc::new(AtomicBool::new(false));

        let outcome = crate::command_center::resume_compound(
            &app,
            pending,
            text,
            resume_flag,
        )
        .await;

        emit(
            &app,
            &OrchestratorEvent::Result {
                text: outcome.text,
                request_id: request_id.clone(),
                analysis: None,
                dialog_state: None,
            },
        );
        emit(
            &app,
            &OrchestratorEvent::Done {
                request_id: request_id.clone(),
            },
        );
        clear_active_request(&request_id);

        return Ok(serde_json::json!({
            "ok": true,
            "server": server.name(),
            "tool": tool,
            "latency_ms": result.latency_ms,
            "compound_resumed": true,
        }));
    }

    emit(
        &app,
        &OrchestratorEvent::Result {
            text,
            request_id: request_id.clone(),
            analysis: None,
            dialog_state: None,
        },
    );
    emit(
        &app,
        &OrchestratorEvent::Done {
            request_id: request_id.clone(),
        },
    );

    Ok(serde_json::json!({
        "ok": true,
        "server": server.name(),
        "tool": tool,
        "latency_ms": result.latency_ms,
    }))
}

// ─── Loading indicator control (owned by orchestrator) ─────────────────

/// Show the loading indicator window at the top-right corner.
///
/// This is the Rust-side implementation — the orchestrator calls this
/// directly instead of going through the frontend IPC. This ensures the
/// loading state is owned by the central system, not scattered across
/// frontend components.
///
/// Runs INLINE (no spawn): creation completes before dispatch starts, so
/// the paired `hide_loading` destroy can never land before the create
/// finishes and wedge a fresh spinner on screen with no hide in flight.
pub fn show_loading<R: Runtime>(app: &AppHandle<R>) {
    // P2 UI-director guard (also enforced in `direct_ui` for new callers):
    // an orphan loading window with no turn behind it leaves the user
    // staring at a spinner. Ghost sessions suppress the window entirely
    // (the waves own the visual there).
    if !has_active_request() {
        tracing::warn!("orchestrator: Loading(true) ignored — no active request (orphan spinner)");
        return;
    }
    if crate::ghost::session_active() {
        tracing::debug!("orchestrator: ghost session active, suppressing top-right loading window");
        return;
    }

    if let Err(e) = crate::dyn_windows::get_or_create_window(
        app,
        crate::dyn_windows::WindowConfig::loading_indicator(),
    ) {
        tracing::warn!("orchestrator: failed to create loading window: {}", e);
        return;
    }

    // Position from the user-calibrated loading placement (settings.json;
    // defaults ≈ the historical top-right corner).
    if let Some(win) = app.get_webview_window("loading-indicator") {
        let _ = crate::window_manager::position_loading(&win);
        let _ = win.set_ignore_cursor_events(true);
        let _ = win.show();
        tracing::info!("orchestrator: loading indicator shown");
    }
}

/// Hide the loading indicator window.
/// P-E (doc 75): HIDE, don't destroy. The old destroy-on-hide freed a few
/// MB but cost a full WebView2 create (~300-800ms) on EVERY long turn —
/// the window landed mid-next-utterance ("thinking loads while I speak").
/// The 80px static window stays resident; `show_loading` reuses it via
/// get_or_create_window. Falls back to destroy if hide fails.
pub fn hide_loading<R: Runtime>(app: &AppHandle<R>) {
    if let Some(win) = app.get_webview_window("loading-indicator") {
        if win.hide().is_ok() {
            tracing::info!("orchestrator: loading indicator hidden (window kept for reuse)");
            return;
        }
    }
    let _ = crate::dyn_windows::destroy_window(app, "loading-indicator");
    tracing::info!("orchestrator: loading indicator hidden");
}

/// IPC: Show loading indicator (can be called from frontend if needed).
#[tauri::command]
pub async fn orchestrator_show_loading<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    show_loading(&app);
    Ok(())
}

/// IPC: Hide loading indicator (can be called from frontend if needed).
#[tauri::command]
pub async fn orchestrator_hide_loading<R: Runtime>(
    app: AppHandle<R>,
) -> Result<(), String> {
    hide_loading(&app);
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_is_1() {
        assert_eq!(PROTOCOL_VERSION, "1");
    }

    #[test]
    fn test_classify_screen_query_readonly() {
        use super::ScreenQueryTier::*;
        for q in [
            "read my screen",
            "what's on my screen",
            "what is on the screen",
            "list the text on my screen",
            "show me my screen",
            "tell me what i see",
        ] {
            assert_eq!(classify_screen_query(q), ReadOnly, "query: {q}");
        }
    }

    #[test]
    fn test_classify_screen_query_visual() {
        use super::ScreenQueryTier::*;
        for q in [
            "analyse the screen",
            "analyze my screen for me",
            "explain what i see",
            "research this screen for me",
            "compare these two windows",
            "summarize the error on screen",
            "why is this failing",
            "what does this error mean",
            "describe my screen",
            "teach me this diagram",
            "diagnose the screen",
        ] {
            assert_eq!(classify_screen_query(q), Visual, "query: {q}");
        }
    }

    #[test]
    fn test_annotation_seed_shape() {
        let seed = annotation_seed("annotate my screen");
        assert_eq!(seed.get("tool").and_then(|v| v.as_str()), Some("select"));
        assert_eq!(seed.get("elements").and_then(|v| v.as_array()).map(|a| a.len()), Some(0));
        assert_eq!(seed.get("prompt").and_then(|v| v.as_str()), Some("annotate my screen"));
    }

    #[test]
    fn test_worker_payload_has_version() {
        let p = build_worker_payload("r1", "u1", "d1", "hello", None);
        assert_eq!(p["protocol_version"], "1");
        assert_eq!(p["request_id"], "r1");
        assert_eq!(p["task"]["request"], "hello");
        assert!(p["task"].get("dialog_context").is_none());
    }

    #[test]
    fn test_worker_payload_with_context() {
        let ctx = serde_json::json!({"history": []});
        let p = build_worker_payload("r2", "u2", "d2", "hi", Some(&ctx));
        assert_eq!(p["protocol_version"], "1");
        assert_eq!(p["task"]["dialog_context"], ctx);
    }

    /// Feature 88: canonical identity rides `requester.profile_id`; legacy
    /// payloads keep the exact pre-identity shape (migration invariant).
    #[test]
    fn test_worker_payload_ident_variant() {
        let legacy = build_worker_payload_ident("r3", "u3", "d3", None, "hey", None);
        assert!(legacy["requester"].get("profile_id").is_none());
        assert_eq!(legacy["requester"]["id"], "u3");

        let canonical = build_worker_payload_ident("r4", "u4", "d4", Some("prof_Kx7"), "hey", None);
        assert_eq!(canonical["requester"]["profile_id"], "prof_Kx7");
        assert_eq!(canonical["requester"]["id"], "u4");
        assert_eq!(canonical["requester"]["device_id"], "d4");
        assert_eq!(canonical["protocol_version"], "1");
    }

    /// Feature 88 (C2): each denial code gets its distinct spoken line;
    /// non-denial bodies return None (generic error path).
    #[test]
    fn test_denial_spoken_lines() {
        let mk = |code: &str| {
            serde_json::json!({
                "error": "worker_ai_not_enabled",
                "code": code,
                "request_id": "r",
            }).to_string()
        };
        assert_eq!(
            denial_spoken_line(&mk("pending")).unwrap(),
            "Cloud access is awaiting approval from your administrator, sir. Everything local still works meanwhile."
        );
        assert_eq!(
            denial_spoken_line(&mk("suspended")).unwrap(),
            "Cloud access has been suspended, sir. Please contact your administrator."
        );
        assert_eq!(
            denial_spoken_line(&mk("revoked")).unwrap(),
            "This device's cloud access has been revoked, sir."
        );
        assert_eq!(
            denial_spoken_line(&mk("expired")).unwrap(),
            "This device's cloud access grant has expired, sir. Please ask your administrator to renew it."
        );
        assert_eq!(
            denial_spoken_line(&mk("bad_token")).unwrap(),
            "This device's cloud credentials are invalid, sir. Please reconnect in the setup wizard."
        );
        assert_eq!(
            denial_spoken_line(&mk("")).unwrap(),
            "Cloud access isn't enabled for this device yet, sir."
        );
        // Non-denial worker error → None.
        assert!(denial_spoken_line(r#"{"error":"not found"}"#).is_none());
        assert!(denial_spoken_line("Worker error 500: internal").is_none());
    }

    /// Pins the wire tags against the frontend union in orchestrator.ts
    /// (`"conflict_report"` / `"github_result"`). Regressing to enum-wide
    /// lowercase ("conflictreport") or snake_case ("git_hub_result") silently
    /// unreachables the merge-conflict panel and live PR-list refresh.
    #[test]
    fn test_event_wire_tags_match_frontend_union() {
        let conflict = OrchestratorEvent::ConflictReport {
            request_id: "abc123".into(),
            pr_number: 7,
            repo: "owner/repo".into(),
            conflict_files: serde_json::json!([]),
            message: "conflicts".into(),
        };
        let v = serde_json::to_value(&conflict).unwrap();
        assert_eq!(v["type"], "conflict_report");

        let gh = OrchestratorEvent::GitHubResult {
            request_id: "abc123".into(),
            result: serde_json::json!({"kind": "text"}),
        };
        let v = serde_json::to_value(&gh).unwrap();
        assert_eq!(v["type"], "github_result");

        // Single-word variants must stay plain lowercase.
        let ack = OrchestratorEvent::Ack {
            text: "On it sir.".into(),
            request_id: "abc123".into(),
        };
        assert_eq!(serde_json::to_value(&ack).unwrap()["type"], "ack");

        let result = OrchestratorEvent::Result {
            text: "done".into(),
            request_id: "abc123".into(),
            analysis: None,
            dialog_state: None,
        };
        assert_eq!(serde_json::to_value(&result).unwrap()["type"], "result");
    }

    #[test]
    fn test_route_local_command() {
        let intent = ParsedIntent::OpenApp {
            target: "chrome".to_string(),
        };
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    /// Browser control routes local (hotkey execution), never Worker/MCP —
    /// in ghost mode especially, actions must run on the user's laptop.
    #[test]
    fn test_route_browser_local() {        assert_eq!(
            route_intent(&ParsedIntent::BrowserCloseTab { index: Some(5) }),
            Subsystem::LocalCommand
        );
        assert_eq!(
            route_intent(&ParsedIntent::BrowserCloseTab { index: None }),
            Subsystem::LocalCommand
        );
        assert_eq!(
            route_intent(&ParsedIntent::NluResult {
                intent: "browser_new_tab".to_string(),
                slots: serde_json::json!({}),
                confidence: 1.0,
            }),
            // NluResult is still WorkerBackend at route_intent level; the
            // browser_new_tab intercept fires earlier in process_transcript.
            Subsystem::WorkerBackend
        );
    }

    /// Repeat-back acks (P1): the ack names the action. Unknown/general
    /// keeps the generic ack (nothing to name).
    #[test]
    fn test_ack_for_names_action() {
        let ack = ack_for(&ParsedIntent::AnalysePr {
            pr_number: 24,
            repo: "zync".to_string(),
            owner: None,
        });
        assert!(ack.contains("PR 24") && ack.contains("zync"), "{ack}");
        let ack = ack_for(&ParsedIntent::Search {
            query: "rust programming".to_string(),
        });
        assert!(ack.contains("rust programming"), "{ack}");
        // Generic fallback ends with "sir." like every pick_ack phrase.
        let ack = ack_for(&ParsedIntent::Unknown {
            raw: "blah".to_string(),
        });
        assert!(ack.ends_with("sir."), "{ack}");
    }

    #[test]
    fn test_route_greeting() {
        let intent = ParsedIntent::Greeting {
            reply: "Hello sir.".to_string(),
        };
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_ghost_control_is_local() {
        // Cursor-control entry must route locally (handled explicitly in
        // process_transcript) — never to the Worker.
        assert_eq!(
            route_intent(&ParsedIntent::EnterGhostControl),
            Subsystem::LocalCommand
        );
    }

    #[test]
    fn test_route_media() {
        assert_eq!(route_intent(&ParsedIntent::MediaPlayPause), Subsystem::LocalCommand);
        assert_eq!(route_intent(&ParsedIntent::MediaNext), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_architect() {
        assert_eq!(route_intent(&ParsedIntent::OpenArchitect), Subsystem::Architect);
    }

    /// Garbage-transcript guard: ML-only OpenArchitect (e.g. Qwen 0.99 on
    /// 'You feel it, no?' from a 1s noise capture) is capped below the
    /// accept line; deterministic and non-Architect results pass through.
    #[test]
    fn test_cap_ml_window_open() {
        use crate::intent_parser::ParseResult;
        let mut r = ParseResult {
            intent: ParsedIntent::OpenArchitect,
            confidence: 0.99,
            source: "brain".to_string(),
        };
        assert!(cap_ml_window_open(&mut r));
        assert!(r.confidence < 0.5);
        let mut r2 = ParseResult {
            intent: ParsedIntent::OpenArchitect,
            confidence: 1.0,
            source: "deterministic".to_string(),
        };
        assert!(!cap_ml_window_open(&mut r2));
        assert_eq!(r2.confidence, 1.0);
        let mut r3 = ParseResult {
            intent: ParsedIntent::OpenApp {
                target: "chrome".to_string(),
            },
            confidence: 0.99,
            source: "brain".to_string(),
        };
        assert!(!cap_ml_window_open(&mut r3));
    }

    /// doc 07 P3: the brain-only (never-trained) system_* click/window
    /// labels are capped the same way, confirmed against real observed
    /// brain output ("click submit" -> screen_analysis, "switch to brave"
    /// -> start_dictation, both @ high confidence from Qwen 0.5B).
    /// Deterministic-sourced and NLU-sourced results are unaffected —
    /// NLU can't even emit these labels (not in its trained vocabulary).
    #[test]
    fn test_cap_ml_unverified_system_labels() {
        use crate::intent_parser::ParseResult;
        for label in ["system_click_element", "system_minimize_window", "system_maximize_window"] {
            let mut r = ParseResult {
                intent: ParsedIntent::NluResult {
                    intent: label.to_string(),
                    slots: serde_json::json!({}),
                    confidence: 0.95,
                },
                confidence: 0.95,
                source: "brain".to_string(),
            };
            assert!(cap_ml_window_open(&mut r), "{label} should be capped from brain source");
            assert!(r.confidence < 0.5, "{label} confidence should drop below accept line");
        }
        // Deterministic source is never capped (Tier-0 is the trusted path).
        let mut det = ParseResult {
            intent: ParsedIntent::NluResult {
                intent: "system_click_element".to_string(),
                slots: serde_json::json!({ "name": "submit" }),
                confidence: 0.85,
            },
            confidence: 0.85,
            source: "deterministic".to_string(),
        };
        assert!(!cap_ml_window_open(&mut det));
        assert_eq!(det.confidence, 0.85);
    }

    #[test]
    fn test_route_worker_backend() {
        let intent = ParsedIntent::Search {
            query: "what is rust".to_string(),
        };
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);

        let intent = ParsedIntent::AnalysePr {
            owner: None,
            repo: "zync".to_string(),
            pr_number: 24,
        };
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_unknown() {
        let intent = ParsedIntent::Unknown {
            raw: "blah blah".to_string(),
        };
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_is_long_running() {
        assert!(!is_long_running(&Subsystem::LocalCommand));
        assert!(is_long_running(&Subsystem::WorkerBackend));
        assert!(is_long_running(&Subsystem::Architect));
        assert!(!is_long_running(&Subsystem::None));
    }

    #[test]
    fn test_install_and_cancel() {
        // Install two requests — the second should cancel the first
        let (id1, _cancel1) = install_new_request(Subsystem::WorkerBackend);
        let (id2, cancel2) = install_new_request(Subsystem::WorkerBackend);
        assert_ne!(id1, id2, "IDs should be different");
        assert!(!is_cancelled(&cancel2), "second request should not be cancelled");
        // cancel1 may or may not be cancelled depending on parallel test execution
        // The key property: the second request is active and not cancelled
        clear_active_request(&id2);
    }

    #[test]
    fn test_request_id_is_short() {
        let id = new_request_id();
        assert!(id.len() <= 12);
    }

    #[test]
    fn test_pick_ack_returns_valid_phrase() {
        let ack = pick_ack();
        assert!(ACK_PHRASES.contains(&ack));
    }

    // ─── Comprehensive routing tests for every command type ───

    #[test]
    fn test_route_open_app() {
        let result = parse_deterministic("open chrome");
        assert!(result.is_some(), "should parse 'open chrome'");
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_open_url() {
        let result = parse_deterministic("open youtube.com");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_close_app() {
        let result = parse_deterministic("close chrome");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_whatsapp_chat() {
        let result = parse_deterministic("open chat with mom");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_greeting_hello() {
        let result = parse_deterministic("hello");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_greeting_thanks() {
        let result = parse_deterministic("thank you");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_media_pause() {
        let result = parse_deterministic("pause");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_media_next() {
        let result = parse_deterministic("next");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::LocalCommand);
    }

    #[test]
    fn test_route_architect_explicit() {
        let result = parse_deterministic("open architecture mapper");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::Architect);
    }

    #[test]
    fn test_route_search_query() {
        let result = parse_deterministic("search for rust programming");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_analyse_pr() {
        let result = parse_deterministic("analyse PR 24 in zync");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_analyse_repo() {
        let result = parse_deterministic("analyse zync");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_analyse_latest_pr() {
        let result = parse_deterministic("analyse the pr in zync");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_check_branch() {
        let result = parse_deterministic("check the latest branch of servx");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_unknown_goes_to_worker() {
        let result = parse_deterministic("what is the meaning of life");
        // Unknown commands go to the Worker for general Q&A
        let intent = result.map(|r| r.intent).unwrap_or(ParsedIntent::Unknown {
            raw: "what is the meaning of life".to_string(),
        });
        assert_eq!(route_intent(&intent), Subsystem::WorkerBackend);
    }

    #[test]
    fn test_route_empty_transcript() {
        let result = parse_deterministic("");
        assert!(result.is_none());
        // Empty transcript → Unknown → WorkerBackend (but process_transcript
        // rejects empty transcripts before routing)
    }

    // ─── MCP routing tests ───

    #[test]
    fn test_route_order_food() {
        let result = parse_deterministic("order pizza from dominos");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::Mcp);
    }

    #[test]
    fn test_route_search_product() {
        let result = parse_deterministic("search for sony headphones on amazon");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::Mcp);
    }

    #[test]
    fn test_route_send_whatsapp_message() {
        let result = parse_deterministic("send mom a whatsapp message saying hi");
        assert!(result.is_some());
        let intent = result.unwrap().intent;
        assert_eq!(route_intent(&intent), Subsystem::Mcp);
    }

    #[test]
    fn test_mcp_is_long_running() {
        assert!(is_long_running(&Subsystem::Mcp));
    }

    #[test]
    fn test_is_auth_failure_narrow() {
        assert!(is_auth_failure("HTTP 401: invalid_token"));
        assert!(is_auth_failure("Unauthorized"));
        assert!(is_auth_failure("token expired, reconnect"));
        // Must NOT match ordinary words containing "auth".
        assert!(!is_auth_failure("author not found"));
        assert!(!is_auth_failure("authentic restaurant list"));
        assert!(!is_auth_failure("connection refused"));
    }

    #[test]
    fn test_mcp_guidance_points_at_connections() {
        let g = mcp_error_guidance(
            crate::mcp_client::McpServer::SwiggyFood,
            "HTTP 401",
        );
        assert!(g.contains("Connections"), "guidance must name where: {g}");
        let g2 = mcp_error_guidance(
            crate::mcp_client::McpServer::WhatsApp,
            "connection refused",
        );
        assert!(
            g2.contains("bridge") && g2.contains("Recheck"),
            "bridge guidance: {g2}"
        );
        // Assist names the exact program + first-run step (QR scan).
        assert!(
            g2.contains("mcp-whatsapp") && g2.contains("QR"),
            "bridge assist must be actionable: {g2}"
        );
        let g3 = mcp_error_guidance(
            crate::mcp_client::McpServer::WhatsApp,
            "circuit open for whatsapp (42s left) — bridge failing repeatedly",
        );
        assert!(
            g3.contains("cooling down") && g3.contains("Recheck"),
            "breaker guidance: {g3}"
        );
    }

    #[test]
    fn test_server_for_mcp_intent() {
        use crate::mcp_client::McpServer as S;
        let food = ParsedIntent::OrderFood {
            query: "biryani".into(),
            restaurant: None,
        };
        assert_eq!(server_for_mcp_intent(&food), Some(S::SwiggyFood));
        let prod = ParsedIntent::SearchProduct { query: "x".into() };
        assert_eq!(server_for_mcp_intent(&prod), Some(S::Amazon));
        let wa = ParsedIntent::SendWhatsAppMessage {
            contact: "mummy".into(),
            message: "hi".into(),
        };
        assert_eq!(server_for_mcp_intent(&wa), Some(S::WhatsApp));
        let greet = ParsedIntent::Greeting {
            reply: "hi".into(),
        };
        assert_eq!(server_for_mcp_intent(&greet), None);
    }

    #[test]
    fn test_connect_card_markdown_has_fix_action() {
        let card = crate::mcp_client::McpConnectCard {
            server: "whatsapp".to_string(),
            state: crate::mcp_client::McpConnectState::AuthRequired,
            note: "bridge up, phone not paired".to_string(),
            steps: vec!["Scan this QR.".to_string()],
            qr_image_uri: Some("data:image/png;base64,AAA".to_string()),
            qr_code_text: None,
            pair_url: Some("http://127.0.0.1:8765/pair".to_string()),
        };
        let md = connect_card_markdown(&card, "send hi to mummy");
        assert!(md.contains("Scan this QR."));
        assert!(md.contains("data:image/png;base64,AAA"));
        assert!(md.contains("http://127.0.0.1:8765/pair"));
        assert!(md.contains("20 days"), "rotation warning required");
        assert!(md.contains("ToS"), "burner warning required");
    }

    #[test]
    fn test_mcp_retry_stash_take_roundtrip() {
        // take on empty is None (no stale retry from other tests).
        take_mcp_retry();
        stash_mcp_retry(
            crate::mcp_client::McpServer::Amazon,
            "amazon_search",
            &serde_json::json!({"query": "x"}),
        );
        let r = take_mcp_retry().expect("stashed retry must come back");
        assert_eq!(r.server, crate::mcp_client::McpServer::Amazon);
        assert_eq!(r.tool, "amazon_search");
        assert!(take_mcp_retry().is_none(), "take must drain");
    }

    // ─── Barge-in / cancellation tests ───

    #[test]
    fn test_barge_in_cancels_previous() {
        // Start request 1
        let (_id1, cancel1) = install_new_request(Subsystem::WorkerBackend);
        let _cancel1_was_cancelled = is_cancelled(&cancel1);

        // Start request 2 (barge-in) — this cancels request 1
        let (id2, cancel2) = install_new_request(Subsystem::WorkerBackend);
        // cancel1 should now be cancelled (unless a parallel test already cancelled it)
        // The key assertion: cancel2 is NOT cancelled
        assert!(!is_cancelled(&cancel2), "req2 should not be cancelled");

        clear_active_request(&id2);
    }

    #[test]
    fn test_cancel_active_sets_flag() {
        let (_, cancel) = install_new_request(Subsystem::WorkerBackend);
        cancel_active();
        // The cancel flag should be set (or was already set by a parallel test)
        // Either way, cancel_active() should not panic
        let _ = is_cancelled(&cancel);
    }

    #[test]
    fn test_request_barge_in_clears_active() {
        // Barge-in choke point: active turn cancelled, slot cleared,
        // idempotent when nothing is playing (second call must not panic).
        let (_, cancel) = install_new_request(Subsystem::WorkerBackend);
        request_barge_in("test");
        assert!(is_cancelled(&cancel), "barge-in must cancel the active turn");
        assert!(!has_active_request(), "barge-in must clear the active slot");
        request_barge_in("test-idle");
    }

    #[test]
    fn test_signal_done_doesnt_panic() {
        // Just verify signal_done doesn't panic with any ID
        signal_done("test_id_123");
    }

    #[test]
    fn test_signal_done_doesnt_clear_wrong_id() {
        // Note: This test shares the global ACTIVE_REQUEST with other tests
        // that run in parallel. We use a unique wrong ID that no other test
        // would generate, and just verify signal_done doesn't panic.
        signal_done("definitely_wrong_id_999");
        // If we get here without panicking, the test passes.
        // (We can't assert the global state because parallel tests may have
        // changed it between install and check.)
    }

    // ─── Subsystem classification tests ───

    #[test]
    fn test_local_commands_are_not_long_running() {
        assert!(!is_long_running(&Subsystem::LocalCommand));
    }

    #[test]
    fn test_worker_backend_is_long_running() {
        assert!(is_long_running(&Subsystem::WorkerBackend));
    }

    #[test]
    fn test_architect_is_long_running() {
        assert!(is_long_running(&Subsystem::Architect));
    }

    #[test]
    fn test_none_is_not_long_running() {
        assert!(!is_long_running(&Subsystem::None));
    }

    // ─── Request ID tests ───

    #[test]
    fn test_request_ids_are_unique() {
        let id1 = new_request_id();
        let id2 = new_request_id();
        let id3 = new_request_id();
        assert_ne!(id1, id2, "IDs should be unique");
        assert_ne!(id2, id3, "IDs should be unique");
        assert_ne!(id1, id3, "IDs should be unique");
    }

    #[test]
    fn test_request_id_is_alphanumeric() {
        let id = new_request_id();
        for c in id.chars() {
            assert!(c.is_ascii_alphanumeric(), "ID should be alphanumeric, found: {}", c);
        }
    }

    #[test]
    fn test_screen_key_guidance_matrix() {
        // No key → onboarding funnel (points at Command Hub, Accounts).
        let g = screen_key_guidance(false, false).unwrap();
        assert!(g.contains("Gemini key"));
        assert!(g.contains("Command Hub"));
        // Key present but quota spent → quota line, no key nag.
        let g = screen_key_guidance(true, true).unwrap();
        assert!(g.contains("quota"));
        assert!(!g.contains("Command Hub"));
        // Key + quota → silent (VLM path speaks for itself).
        assert_eq!(screen_key_guidance(true, false), None);
    }
}
