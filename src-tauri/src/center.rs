use crate::intent_parser::ParsedIntent;
use crate::voice_profile::TurnOwnership;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Typed identity for the action families. `as_str` supplies display names
/// for existing logs and speech metadata; policy uses this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CenterId {
    App, Browser, System, Media, YouTube, Message, Commerce, GitHub,
    Architect, Ghost, Hand, Google, Memory, File, Dictation, Greeting, Knowledge,
}

impl CenterId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::App => "AppCenter", Self::Browser => "BrowserCenter",
            Self::System => "SystemCenter", Self::Media => "MediaCenter",
            Self::YouTube => "YouTubeCenter", Self::Message => "MessageCenter",
            Self::Commerce => "CommerceCenter", Self::GitHub => "GitHubCenter",
            Self::Architect => "ArchitectCenter", Self::Ghost => "GhostCenter",
            Self::Hand => "HandCenter", Self::Google => "GoogleCenter",
            Self::Memory => "MemoryCenter", Self::File => "FileCenter",
            Self::Dictation => "DictationCenter", Self::Greeting => "GreetingCenter",
            Self::Knowledge => "KnowledgeCenter",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationPolicy { None, ExistingGate }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompoundPolicy { Allowed, StandaloneOnly }

/// One policy and routing snapshot for a parsed action. Legacy centers use
/// the same snapshot while their execution still delegates to existing code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionSpec {
    pub center: CenterId,
    pub route: crate::orchestrator::Subsystem,
    pub risk: crate::action_kernel::Risk,
    pub confirmation: ConfirmationPolicy,
    pub compound: CompoundPolicy,
}

pub fn action_spec(intent: &ParsedIntent) -> ActionSpec {
    let route = crate::orchestrator::route_intent(intent);
    let compound = match intent {
        ParsedIntent::OpenApp { .. } | ParsedIntent::OpenUrl { .. }
        | ParsedIntent::WhatsappChat { .. }
        | ParsedIntent::MediaPlayPause | ParsedIntent::MediaNext
        | ParsedIntent::MediaPrevious | ParsedIntent::MediaStop
        | ParsedIntent::Greeting { .. } | ParsedIntent::Search { .. }
        | ParsedIntent::AnalyseRepo { .. } | ParsedIntent::AnalysePr { .. }
        | ParsedIntent::AnalyseLatestPr { .. } | ParsedIntent::CheckBranch { .. }
        | ParsedIntent::OrderFood { .. } | ParsedIntent::SearchProduct { .. }
        | ParsedIntent::SendWhatsAppMessage { .. } => CompoundPolicy::Allowed,
        // Every other action, including any new intent, needs an explicit
        // review before it can be composed with another action.
        _ => CompoundPolicy::StandaloneOnly,
    };
    let confirmation = match intent {
        ParsedIntent::GitHubCommand { .. } | ParsedIntent::OrderFood { .. }
        | ParsedIntent::SendWhatsAppMessage { .. } | ParsedIntent::FileAction { .. }
        | ParsedIntent::PowerAction { .. } => ConfirmationPolicy::ExistingGate,
        _ => ConfirmationPolicy::None,
    };
    ActionSpec {
        center: center_id_for(intent), route,
        risk: crate::action_kernel::risk_of_intent(intent), confirmation, compound,
    }
}

/// NLU labels are data, not executable authority. New labels need an explicit
/// entry here and a corresponding dispatcher before they can run.
pub fn supported_nlu_label(label: &str) -> bool {
    matches!(label,
        "youtube_search" | "youtube_journal" | "system_minimize_window"
        | "system_maximize_window" | "system_click_element" | "click_element"
        | "focus_app" | "browser_new_tab" | "type_text" | "cancel_action"
        | "screen_analysis" | "screen_annotation" | "reverse_engineer"
    )
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyVerdict {
    Ready(ActionSpec),
    Clarify(Validity),
    AmbientDrop,
}

pub fn prepare_action(
    intent: &ParsedIntent,
    transcript: &str,
    source: &str,
    turn: &TurnContext,
    ghost_active: bool,
    has_dialog_context: bool,
    skip_validation: bool,
) -> PolicyVerdict {
    match action_disposition(intent, transcript, source, turn.ownership, ghost_active) {
        ActionDisposition::AmbientDrop => return PolicyVerdict::AmbientDrop,
        ActionDisposition::Clarify => return PolicyVerdict::Clarify(Validity::Invalid {
            reason: "uncertain_command",
            prompt: "I wasn't sure which action you meant, sir. Please say it again.".into(),
        }),
        ActionDisposition::Allow => {}
    }
    if let ParsedIntent::NluResult { intent: label, .. } = intent {
        if !supported_nlu_label(label) {
            return PolicyVerdict::Clarify(Validity::Invalid {
                reason: "unsupported_nlu_action",
                prompt: "I can't perform that action yet, sir.".into(),
            });
        }
    }
    if !skip_validation {
        let validity = validate(intent, transcript, has_dialog_context);
        if validity != Validity::Ok {
            return PolicyVerdict::Clarify(validity);
        }
    }
    PolicyVerdict::Ready(action_spec(intent))
}

/// Result of validating required dialog slots and invariants for a sub-center.
#[derive(Debug, Clone, PartialEq)]
pub enum Validity {
    Ok,
    Unheard { prompt: String },
    NeedSlot { slot: &'static str, prompt: String },
    Invalid { reason: &'static str, prompt: String },
}

/// Strength of the textual evidence that the user actually requested the
/// parsed action. Deterministic confidence alone is insufficient because
/// background speech can produce a fluent transcript with weak command
/// anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandEvidence {
    Strong,
    Medium,
    Weak,
    None,
}

/// Final routing disposition after combining command evidence, turn
/// ownership, and session context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionDisposition {
    Allow,
    Clarify,
    AmbientDrop,
}

fn owner_repo_token(text: &str) -> bool {
    text.split_whitespace().any(|raw| {
        let token = raw.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '/' || c == '-' || c == '_' || c == '.'));
        match token.split_once('/') {
            Some((owner, repo)) => {
                !owner.is_empty()
                    && !repo.is_empty()
                    && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    && repo.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
            }
            None => false,
        }
    })
}

fn source_strength(source: &str) -> u8 {
    // Every rule-based parser ("deterministic", "-live", "-dictation",
    // "-directions", "-memory", ...) is a phrase-grammar match. Only the fuzzy
    // variants are weak. An unlisted `deterministic-*` source used to score 0
    // and the turn was silently dropped as ambient in a Ghost session (the
    // "type google meet" turn that produced no reply and a 28 s stall).
    if source == "deterministic-fuzzy" {
        1
    } else if source == "deterministic" || source.starts_with("deterministic-") {
        3
    } else if source == "nlu" || source == "brain" {
        2
    } else if source == "fuzzy" {
        1
    } else {
        0
    }
}

/// Whether executing this intent can change external state, spend quota/API
/// calls, move the cursor, type text, or open a window.
pub fn is_side_effect(intent: &ParsedIntent) -> bool {
    match intent {
        ParsedIntent::GitHubCommand { command } => {
            !matches!(command,
                crate::github_cmd::GitHubCommand::ListPrs { .. }
                | crate::github_cmd::GitHubCommand::GetPr { .. }
                | crate::github_cmd::GitHubCommand::ListPrFiles { .. }
                | crate::github_cmd::GitHubCommand::ListCollaborators { .. }
                | crate::github_cmd::GitHubCommand::ListOrgMembers { .. }
                | crate::github_cmd::GitHubCommand::ListOutsideCollaborators { .. }
                | crate::github_cmd::GitHubCommand::ListBranches { .. }
                | crate::github_cmd::GitHubCommand::ListReleases { .. }
                | crate::github_cmd::GitHubCommand::ListWorkflows { .. }
                | crate::github_cmd::GitHubCommand::ListWorkflowRuns { .. }
                | crate::github_cmd::GitHubCommand::FindPr { .. })
        }
        // These remain available to an unenrolled user's clear request.
        // App/site opening is an explicit Ghost policy exception.
        ParsedIntent::OpenApp { .. } | ParsedIntent::OpenUrl { .. }
        | ParsedIntent::OpenSettings | ParsedIntent::OpenCodeRabbitHub
        | ParsedIntent::CodeRabbitReviewThis | ParsedIntent::AiReviewThis
        | ParsedIntent::OpenGitHubDashboard { .. }
        | ParsedIntent::MediaPlayPause | ParsedIntent::MediaNext
        | ParsedIntent::MediaPrevious | ParsedIntent::MediaStop
        | ParsedIntent::PlayMusic { .. } | ParsedIntent::OpenMusicPlayer { .. }
        | ParsedIntent::Greeting { .. } | ParsedIntent::NeedMoreInfo { .. }
        | ParsedIntent::Unknown { .. } | ParsedIntent::MemoryAudit
        | ParsedIntent::Briefing | ParsedIntent::TimetableShow
        | ParsedIntent::MailDigest | ParsedIntent::WhatsappRead { .. }
        | ParsedIntent::PeopleList | ParsedIntent::CalendarAgenda { .. }
        | ParsedIntent::ReviewForeground
        | ParsedIntent::ListBlockedContacts | ParsedIntent::ShareConcern { .. } => false,
        // Any new variant is side-effecting until explicitly reviewed above.
        _ => true,
    }
}

pub fn command_evidence(
    intent: &ParsedIntent,
    transcript: &str,
    source: &str,
) -> CommandEvidence {
    let strength = source_strength(source);
    match intent {
        ParsedIntent::Unknown { .. } => CommandEvidence::None,
        ParsedIntent::Greeting { .. }
        | ParsedIntent::NeedMoreInfo { .. }
        | ParsedIntent::OpenSettings
        | ParsedIntent::OpenCodeRabbitHub
        | ParsedIntent::CodeRabbitReviewThis
        | ParsedIntent::AiReviewThis
        | ParsedIntent::OpenGitHubDashboard { .. } => CommandEvidence::Strong,
        ParsedIntent::OpenArchitect => {
            if strength >= 3 {
                CommandEvidence::Strong
            } else if strength == 2 {
                CommandEvidence::Medium
            } else {
                CommandEvidence::Weak
            }
        }
        ParsedIntent::AnalyseRepo { repo, .. } => {
            if matches!(
                crate::github_repo_index::peek(repo),
                Some(crate::github_repo_index::Resolution::One(_))
            ) {
                CommandEvidence::Strong
            } else if repo.trim().is_empty() || !owner_repo_token(transcript) {
                // Pattern-4 style inference ("analyse <arbitrary words>") is a
                // repository guess, not repository evidence.
                if strength >= 3 {
                    CommandEvidence::Medium
                } else {
                    CommandEvidence::Weak
                }
            } else if strength >= 3 {
                CommandEvidence::Strong
            } else {
                CommandEvidence::Medium
            }
        }
        ParsedIntent::AnalysePr { repo, .. }
        | ParsedIntent::AnalyseLatestPr { repo, .. }
        | ParsedIntent::CheckBranch { repo, .. } => {
            if repo.trim().is_empty() {
                CommandEvidence::Weak
            } else if strength >= 3 {
                CommandEvidence::Strong
            } else {
                CommandEvidence::Medium
            }
        }
        ParsedIntent::GitHubCommand { command } => {
            let has_repo = !format!("{command:?}").contains("repo: \"\"");
            let is_list_prs = matches!(
                command,
                crate::github_cmd::GitHubCommand::ListPrs { .. }
                    | crate::github_cmd::GitHubCommand::FindPr { .. }
            );
            if (has_repo || is_list_prs) && strength >= 3 {
                CommandEvidence::Strong
            } else if has_repo || is_list_prs {
                CommandEvidence::Medium
            } else {
                CommandEvidence::Weak
            }
        }
        _ => {
            if strength >= 3 {
                CommandEvidence::Strong
            } else if strength == 2 {
                CommandEvidence::Medium
            } else if strength == 1 {
                CommandEvidence::Weak
            } else {
                CommandEvidence::None
            }
        }
    }
}

/// Combine authorship, evidence, and session context into one routing
/// decision. Rejected turns are always ambient. Uncertain ghost turns never
/// execute or prompt; they must wait for an owner-verified repetition.
pub fn action_disposition(
    intent: &ParsedIntent,
    transcript: &str,
    source: &str,
    ownership: TurnOwnership,
    ghost_active: bool,
) -> ActionDisposition {
    if ownership == TurnOwnership::Rejected {
        return ActionDisposition::AmbientDrop;
    }

    let evidence = command_evidence(intent, transcript, source);
    if evidence == CommandEvidence::None {
        if is_side_effect(intent) {
            return if ghost_active {
                ActionDisposition::AmbientDrop
            } else {
                ActionDisposition::Clarify
            };
        }
        return match ownership {
            TurnOwnership::Verified => ActionDisposition::Allow,
            TurnOwnership::Unenrolled if !ghost_active => ActionDisposition::Allow,
            TurnOwnership::Uncertain if !ghost_active => ActionDisposition::Clarify,
            _ => ActionDisposition::AmbientDrop,
        };
    }

    if !is_side_effect(intent) {
        return match ownership {
            TurnOwnership::Verified | TurnOwnership::Unenrolled => ActionDisposition::Allow,
            TurnOwnership::Uncertain if !ghost_active => ActionDisposition::Clarify,
            _ => ActionDisposition::AmbientDrop,
        };
    }

    match (evidence, ownership) {
        (CommandEvidence::Strong, TurnOwnership::Verified) => ActionDisposition::Allow,
        (CommandEvidence::Strong, TurnOwnership::Unenrolled) => ActionDisposition::Allow,
        (CommandEvidence::Medium, TurnOwnership::Verified) => ActionDisposition::Allow,
        // Ghost mode is full control: an unenrolled owner's medium-evidence command (the NLU's
        // classification of "nexus. oco notepad.") runs instead of vanishing with no reply, which
        // is what the live test of 2026-10-13 did. Outside Ghost it still asks first.
        (CommandEvidence::Medium, TurnOwnership::Unenrolled) => {
            if ghost_active {
                ActionDisposition::Allow
            } else {
                ActionDisposition::Clarify
            }
        }
        (_, _) if ghost_active => ActionDisposition::AmbientDrop,
        _ => ActionDisposition::Clarify,
    }
}

/// Execution outcome returned by a sub-center.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(dead_code)]
pub struct Receipt {
    pub ok: bool,
    pub summary: String,
    pub data: Value,
    pub latency_ms: u64,
}

/// Confirmation policy before execution (Alexa standard: routine repeat-back vs destructive gate).
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ConfirmKind {
    None,
    RepeatBack { ack: String },
    GatePrompt { prompt: String, timeout_secs: u64 },
}

/// Provenance supplied with a transcript turn. Unknown/missing fields mean an
/// older or non-Rust-capture caller; those turns use the legacy fail-open path.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct TurnContext {
    #[serde(default)]
    pub ownership: TurnOwnership,
    #[serde(default)]
    #[serde(alias = "ownerScore")]
    pub owner_score: f32,
    #[serde(default)]
    #[serde(alias = "decoderBias")]
    #[allow(dead_code)]
    pub decoder_bias: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub language: String,
    #[serde(default)]
    pub session: u64,
    /// Original STT text before frontend display corrections. Never trusted as
    /// authority, but used to catch an action invented by a correction.
    #[serde(default)]
    #[serde(alias = "rawText")]
    pub raw_text: String,
    #[serde(default)]
    #[serde(alias = "correctedText")]
    pub corrected_text: String,
}

/// A correction may normalize spelling, but it must not turn a different
/// command or target into an executable action without a repeat.
pub fn correction_needs_repeat(raw: &str, corrected: &str, intent: &ParsedIntent) -> bool {
    if raw.trim().is_empty() || corrected.trim().is_empty()
        || matches!(intent, ParsedIntent::Unknown { .. } | ParsedIntent::Greeting { .. }
            | ParsedIntent::NeedMoreInfo { .. }) {
        return false;
    }
    let normalize = |s: &str| {
        let cleaned: String = s.to_ascii_lowercase().replace("analyze", "analyse")
            .chars().filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
            .collect();
        cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    normalize(raw) != normalize(corrected)
}

#[cfg(test)]
mod transcript_provenance_tests {
    use super::{correction_needs_repeat, TurnContext};
    use crate::intent_parser::ParsedIntent;
    #[test]
    fn client_provenance_aliases_deserialize() {
        let ctx: TurnContext = serde_json::from_value(serde_json::json!({
            "session": 42, "ownership": "unenrolled", "ownerScore": 0.6,
            "decoderBias": "neutral", "rawText": "open zink", "correctedText": "open zync"
        })).unwrap();
        assert_eq!(ctx.session, 42);
        assert_eq!(ctx.raw_text, "open zink");
        assert_eq!(ctx.corrected_text, "open zync");
        assert_eq!(ctx.decoder_bias, "neutral");
    }
    #[test]
    fn correction_cannot_invent_app_or_repo_target() {
        let app = ParsedIntent::OpenApp { target: "Chrome".into() };
        assert!(correction_needs_repeat("open chroma", "open Chrome", &app));
        assert!(!correction_needs_repeat("Open Chrome.", "open chrome", &app));
        assert!(!correction_needs_repeat("analyze my screen", "analyse my screen", &app));
    }
}

/// Common contract for all action sub-centers (Feature 74 & 75).
pub trait SubCenter: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &'static str;
    fn validate(&self, action: &str, slots: &Value) -> Validity;
    /// Legacy repeat-back copy retained for migration reference. This method
    /// is advisory only; `ActionSpec::confirmation` identifies the existing
    /// executor gate, which remains the sole place that opens a confirmation.
    #[allow(dead_code)]
    fn confirm_kind(&self, action: &str, slots: &Value) -> ConfirmKind;
}

/// Assign each parsed intent to exactly one typed action center.
pub fn center_id_for(intent: &ParsedIntent) -> CenterId {
    match intent {
        ParsedIntent::OpenApp { .. }
        | ParsedIntent::OpenUrl { .. }
        | ParsedIntent::CloseApp { .. }
        | ParsedIntent::OpenSettings
        | ParsedIntent::OpenGitHubDashboard { .. } => CenterId::App,

        ParsedIntent::BrowserTab { .. }
        | ParsedIntent::BrowserCloseTab { .. }
        | ParsedIntent::BrowserSearch { .. }
        | ParsedIntent::BrowserSearchFocus => CenterId::Browser,

        ParsedIntent::MediaPlayPause
        | ParsedIntent::MediaNext
        | ParsedIntent::MediaPrevious
        | ParsedIntent::MediaStop
        | ParsedIntent::PlayMusic { .. }
        | ParsedIntent::OpenMusicPlayer { .. } => CenterId::Media,

        ParsedIntent::WhatsappChat { .. }
        | ParsedIntent::SendWhatsAppMessage { .. }
        | ParsedIntent::AnalyseChatDocs { .. } => CenterId::Message,

        ParsedIntent::OrderFood { .. }
        | ParsedIntent::SearchProduct { .. } => CenterId::Commerce,

        ParsedIntent::GitHubCommand { .. } => CenterId::GitHub,

        ParsedIntent::OpenArchitect
        | ParsedIntent::ReviewForeground
        | ParsedIntent::RecreateLastReview
        | ParsedIntent::OpenCodeRabbitHub
        | ParsedIntent::CodeRabbitReviewThis
        | ParsedIntent::AiReviewThis
        | ParsedIntent::AnalyseRepo { .. }
        | ParsedIntent::AnalysePr { .. }
        | ParsedIntent::AnalyseLatestPr { .. }
        | ParsedIntent::CheckBranch { .. }
        | ParsedIntent::RunOnLocalhost
        | ParsedIntent::CodeProject { .. }
        | ParsedIntent::CodingAgentTask { .. }
        | ParsedIntent::WatchAgentTask { .. }
        | ParsedIntent::WatchThenPower { .. }
        | ParsedIntent::WatchPlanCommand { .. }
        | ParsedIntent::AgentHookCommand { .. } => CenterId::Architect,

        ParsedIntent::Search { .. } => CenterId::Knowledge,

        ParsedIntent::EnterGhostwriter { .. }
        | ParsedIntent::StartDictation
        | ParsedIntent::StopDictation => CenterId::Dictation,

        ParsedIntent::EnterHandMode
        | ParsedIntent::ExitHandMode
        | ParsedIntent::HandMouseLive { .. }
        | ParsedIntent::PointerClick { .. } => CenterId::Hand,

        ParsedIntent::EnterGhostControl
        | ParsedIntent::ExitGhostControl
        | ParsedIntent::GhostAction { .. }
        | ParsedIntent::ScreenClick { .. }
        | ParsedIntent::ScreenRead { .. } => CenterId::Ghost,

        ParsedIntent::FileAction { .. } | ParsedIntent::FileUndo => CenterId::File,

        ParsedIntent::Greeting { .. }
        | ParsedIntent::NeedMoreInfo { .. } => CenterId::Greeting,

        ParsedIntent::WatchScreenEmail
        | ParsedIntent::Directions { .. }
        | ParsedIntent::PlacesSearch { .. }
        | ParsedIntent::ScanInternshipEmails => CenterId::Google,

        ParsedIntent::MemoryAudit
        | ParsedIntent::Briefing
        | ParsedIntent::ImportContacts
        | ParsedIntent::PowerAction { .. }
        | ParsedIntent::ReopenSession
        | ParsedIntent::TimetableAdd { .. }
        | ParsedIntent::TimetableShow
        | ParsedIntent::TimetableClear
        | ParsedIntent::TimetableCommit
        | ParsedIntent::StudyPref { .. }
        | ParsedIntent::MailDigest
        | ParsedIntent::MailMute
        | ParsedIntent::CalendarAgenda { .. }
        | ParsedIntent::CalendarAdd { .. }
        | ParsedIntent::WhatsappRead { .. }
        | ParsedIntent::PeopleFlag { .. }
        | ParsedIntent::PeopleList
        | ParsedIntent::MemoryForget { .. }
        | ParsedIntent::MemoryForgetAll
        | ParsedIntent::MemoryForgetAllConfirm
        | ParsedIntent::BlockContact { .. }
        | ParsedIntent::UnblockContact { .. }
        | ParsedIntent::ListBlockedContacts
        | ParsedIntent::CreateVoiceReminder { .. } => CenterId::Memory,

        ParsedIntent::PersonaFriend
        | ParsedIntent::PersonaButler
        | ParsedIntent::ShareConcern { .. } => CenterId::Greeting,

        ParsedIntent::NluResult { intent: name, .. } => {
            if name.starts_with("youtube_") {
                CenterId::YouTube
            } else if name.starts_with("system_") || name == "inspect_window" || name == "click_element" {
                CenterId::System
            } else if name.starts_with("browser_") {
                CenterId::Browser
            } else if name.starts_with("whatsapp_") {
                CenterId::Message
            } else if name == "type_text" || name == "press_key" || name == "press_hotkey" {
                CenterId::Dictation
            } else if name == "focus_app" {
                CenterId::App
            } else if name == "screen_analysis" || name == "screen_annotation" || name == "reverse_engineer" || name == "cancel_action" {
                CenterId::Ghost
            } else {
                CenterId::Knowledge
            }
        }

        ParsedIntent::Unknown { .. } => CenterId::Knowledge,
    }
}

/// Main Center validity gate (Feature 74 §2).
/// Validates if the user command has sufficient input or required slots before execution.
pub fn validate(intent: &ParsedIntent, transcript: &str, has_dialog_context: bool) -> Validity {
    let clean = transcript.trim();

    // In a running multi-turn dialog, brief answers ("yes", "no", "5") are expected.
    if has_dialog_context {
        return Validity::Ok;
    }

    // Unheard / silence check
    if clean.is_empty() {
        return Validity::Unheard {
            prompt: "I didn't catch that clearly, sir — say it again.".to_string(),
        };
    }

    // Confabulation / sub-threshold check: must contain at least 2 alphanumeric characters
    let alpha_count = clean.chars().filter(|c| c.is_alphanumeric()).count();
    if alpha_count < 2 {
        return Validity::Unheard {
            prompt: "I didn't catch that clearly, sir — say it again.".to_string(),
        };
    }

    // Required slot validation
    match intent {
        ParsedIntent::OpenApp { target } if target.trim().is_empty() => Validity::NeedSlot {
            slot: "target",
            prompt: "Which app should I open, sir?".to_string(),
        },
        ParsedIntent::CloseApp { target } if target.trim().is_empty() => Validity::NeedSlot {
            slot: "target",
            prompt: "Which app should I close, sir?".to_string(),
        },
        ParsedIntent::WhatsappChat { contact } if contact.trim().is_empty() => Validity::NeedSlot {
            slot: "contact",
            prompt: "Who would you like to chat with on WhatsApp, sir?".to_string(),
        },
        ParsedIntent::SendWhatsAppMessage { contact, message } => {
            if contact.trim().is_empty() {
                Validity::NeedSlot {
                    slot: "contact",
                    prompt: "Who should I send the WhatsApp message to, sir?".to_string(),
                }
            } else if message.trim().is_empty() {
                Validity::NeedSlot {
                    slot: "message",
                    prompt: format!("What message should I send to {}, sir?", contact),
                }
            } else {
                Validity::Ok
            }
        }
        ParsedIntent::BrowserTab { index } => crate::browser_center::BrowserCenter::new()
            .validate("browser_tab", &serde_json::json!({"index": index})),
        ParsedIntent::BrowserCloseTab { index } => crate::browser_center::BrowserCenter::new()
            .validate("browser_close_tab", &serde_json::json!({"index": index})),
        ParsedIntent::BrowserSearch { query } => crate::browser_center::BrowserCenter::new()
            .validate("browser_search", &serde_json::json!({"query": query})),
        ParsedIntent::BrowserSearchFocus => crate::browser_center::BrowserCenter::new()
            .validate("browser_search_focus", &Value::Null),
        ParsedIntent::Search { query } if query.trim().is_empty() => Validity::NeedSlot {
            slot: "query",
            prompt: "What would you like me to look up, sir?".to_string(),
        },
        ParsedIntent::NeedMoreInfo { prompt } => Validity::NeedSlot {
            slot: "more_info",
            prompt: prompt.clone(),
        },
        ParsedIntent::GhostAction { task } if !crate::ghost_center::validate(task) => Validity::Invalid {
            reason: "ghost_task",
            prompt: "That Ghost command is incomplete or unsafe, sir.".to_string(),
        },
        ParsedIntent::NluResult { intent: name, slots, .. } => {
            if name.starts_with("youtube_") {
                crate::youtube_center::YouTubeCenter::new().validate(name, slots)
            } else if name.starts_with("system_")
                || name == "focus_app"
                || name == "inspect_window"
                || name == "click_element"
            {
                crate::system_center::SystemCenter::new().validate(name, slots)
            } else if name.starts_with("browser_") {
                crate::browser_center::BrowserCenter::new().validate(name, slots)
            } else {
                Validity::Ok
            }
        }
        _ => Validity::Ok,
    }
}

/// All 17 action sub-centers in the NEXUS Command Center architecture.
#[allow(dead_code)]
pub const ALL_SUB_CENTERS: &[CenterId] = &[
    CenterId::App, CenterId::Browser, CenterId::System, CenterId::Media,
    CenterId::YouTube, CenterId::Message, CenterId::Commerce, CenterId::GitHub,
    CenterId::Architect, CenterId::Ghost, CenterId::Hand, CenterId::Google,
    CenterId::Memory, CenterId::File, CenterId::Dictation, CenterId::Greeting,
    CenterId::Knowledge,
];

// ─── Unit Tests ──────────────────────────────────────────────────────────────

// ─── P2: UI directives (doc 74) ───────────────────────────────────────
// The Main Center is the only entry point for Rust-originated visual
// transitions. `direct_ui` applies transition guards, logs every directive
// (receipt trail), then delegates to the existing implementations.
//
// Guard rules (each exists because a live bug proved it necessary):
// - Loading(true) needs an installed request: an orphan loading window
//   with no turn behind it leaves the user staring at a spinner.
// - Loading is suppressed inside ghost sessions (pre-existing rule, kept):
//   the waves own the visual there, never the top-right window.
// - Session(true/false) always passes: it IS the source of truth.
// - Ring stays in ghost.rs: per-tick hot path with its own spam cap and
//   session scoping — routing it through here would add overhead and
//   Send-complexity for zero gain.

/// Visual transitions the Main Center may direct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiDirective {
    /// Top-right loading window show/hide.
    #[allow(dead_code)]
    Loading(bool),
    /// Ghost session enter/exit (orb waves + flag).
    Session(bool),
}

/// Pure predicate behind the Loading guard — matrix-tested below.
#[allow(dead_code)]
pub fn loading_should_show(has_active_request: bool, ghost_session: bool) -> bool {
    has_active_request && !ghost_session
}

/// Single choke point for visual transitions. New code MUST use this;
/// legacy `show_loading`/`hide_loading` callers are covered because the
/// guard also lives in `show_loading` itself.
pub fn direct_ui<R: tauri::Runtime>(app: &tauri::AppHandle<R>, d: UiDirective) {
    tracing::info!("main-center: ui-directive {:?}", d);
    match d {
        UiDirective::Loading(true) => {
            if !crate::orchestrator::has_active_request() {
                tracing::warn!(
                    "main-center: Loading(true) ignored — no active request (orphan spinner)"
                );
                return;
            }
            crate::orchestrator::show_loading(app);
        }
        UiDirective::Loading(false) => {
            crate::orchestrator::hide_loading(app);
        }
        UiDirective::Session(active) => {
            crate::ghost::emit_session(crate::ghost::ghost_wry::g_wry_ref(app), active);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validity_gate_unheard_and_short() {
        let dummy = ParsedIntent::Unknown { raw: "".into() };
        assert!(matches!(
            validate(&dummy, "", false),
            Validity::Unheard { .. }
        ));
        assert!(matches!(
            validate(&dummy, ".", false),
            Validity::Unheard { .. }
        ));
    }

    #[test]
    fn test_validity_gate_slots() {
        let open_empty = ParsedIntent::OpenApp { target: "".into() };
        assert!(matches!(
            validate(&open_empty, "open", false),
            Validity::NeedSlot { slot: "target", .. }
        ));

        let open_valid = ParsedIntent::OpenApp { target: "Chrome".into() };
        assert_eq!(validate(&open_valid, "open chrome", false), Validity::Ok);
    }

    #[test]
    fn test_center_id_routing() {
        assert_eq!(center_id_for(&ParsedIntent::OpenSettings).as_str(), "AppCenter");
        assert_eq!(center_id_for(&ParsedIntent::MediaPlayPause).as_str(), "MediaCenter");
        assert_eq!(center_id_for(&ParsedIntent::EnterGhostControl).as_str(), "GhostCenter");
        let ghost = ParsedIntent::GhostAction {
            task: crate::ghost_center::parse("save").unwrap(),
        };
        assert_eq!(center_id_for(&ghost).as_str(), "GhostCenter");
        assert!(is_side_effect(&ghost));
        assert_eq!(validate(&ghost, "save", false), Validity::Ok);
        assert_eq!(center_id_for(&ParsedIntent::EnterHandMode).as_str(), "HandCenter");
        assert_eq!(center_id_for(&ParsedIntent::HandMouseLive { live: true }).as_str(), "HandCenter");
    }

    #[test]
    fn action_spec_pins_route_and_compound_safety() {
        let browser = action_spec(&ParsedIntent::BrowserSearch { query: "cats".into() });
        assert_eq!(browser.center, CenterId::Browser);
        assert_eq!(browser.route, crate::orchestrator::Subsystem::LocalCommand);
        assert_eq!(browser.compound, CompoundPolicy::StandaloneOnly);

        let knowledge = action_spec(&ParsedIntent::Search { query: "cats".into() });
        assert_eq!(knowledge.center, CenterId::Knowledge);
        assert_eq!(knowledge.route, crate::orchestrator::Subsystem::WorkerBackend);
        assert_eq!(knowledge.compound, CompoundPolicy::Allowed);

        let message = action_spec(&ParsedIntent::SendWhatsAppMessage {
            contact: "Alex".into(), message: "Hello".into(),
        });
        assert_eq!(message.center, CenterId::Message);
        assert_eq!(message.route, crate::orchestrator::Subsystem::Mcp);
        assert_eq!(message.confirmation, ConfirmationPolicy::ExistingGate);

        let review = action_spec(&ParsedIntent::ReviewForeground);
        assert_eq!(review.center, CenterId::Architect);
        assert_eq!(review.route, crate::orchestrator::Subsystem::LocalCommand);
        assert_eq!(review.risk, crate::action_kernel::Risk::Read);
        assert_eq!(review.compound, CompoundPolicy::StandaloneOnly);
        let recreate = action_spec(&ParsedIntent::RecreateLastReview);
        assert_eq!(recreate.center, CenterId::Architect);
        assert_eq!(recreate.route, crate::orchestrator::Subsystem::LocalCommand);
        assert_eq!(recreate.risk, crate::action_kernel::Risk::Destructive);
        assert_eq!(recreate.compound, CompoundPolicy::StandaloneOnly);

        let file = action_spec(&ParsedIntent::FileUndo);
        assert_eq!(file.compound, CompoundPolicy::StandaloneOnly);
        assert_eq!(
            action_spec(&ParsedIntent::CloseApp { target: "Chrome".into() }).compound,
            CompoundPolicy::StandaloneOnly
        );
        assert_eq!(
            action_spec(&ParsedIntent::GitHubCommand {
                command: crate::github_cmd::GitHubCommand::ClosePr {
                    repo: "o/r".into(), pr_number: 1,
                },
            }).compound,
            CompoundPolicy::StandaloneOnly
        );
    }

    #[test]
    fn one_policy_checks_slots_owner_and_nlu_allowlist() {
        let mut turn = TurnContext::default();
        turn.ownership = TurnOwnership::Verified;
        let bad_tab = ParsedIntent::BrowserTab { index: 12 };
        assert!(matches!(
            prepare_action(&bad_tab, "switch to tab 12", "deterministic", &turn, false, false, false),
            PolicyVerdict::Clarify(Validity::Invalid { .. })
        ));
        let unknown_label = ParsedIntent::NluResult {
            intent: "system_unreviewed_action".into(), slots: Value::Null, confidence: 1.0,
        };
        assert!(matches!(
            prepare_action(&unknown_label, "do something", "nlu", &turn, false, false, false),
            PolicyVerdict::Clarify(Validity::Invalid { reason: "unsupported_nlu_action", .. })
        ));
        assert!(matches!(
            prepare_action(
                &ParsedIntent::CloseApp { target: "Chrome".into() },
                "close Chrome", "", &turn, false, false, false,
            ),
            PolicyVerdict::Clarify(Validity::Invalid { reason: "uncertain_command", .. })
        ));
        assert!(!is_side_effect(&ParsedIntent::OpenApp { target: "Chrome".into() }));
        assert!(is_side_effect(&ParsedIntent::CloseApp { target: "Chrome".into() }));
    }

    #[test]
    #[ignore = "manual same-machine routing latency comparison"]
    fn routing_policy_latency_probe() {
        use std::hint::black_box;
        use std::time::Instant;

        let intent = ParsedIntent::OpenApp { target: "Chrome".into() };
        let mut turn = TurnContext::default();
        turn.ownership = TurnOwnership::Verified;
        let count = 100_000;
        let before = Instant::now();
        for _ in 0..count {
            black_box(action_disposition(&intent, "open Chrome", "deterministic", turn.ownership, false));
            black_box(validate(&intent, "open Chrome", false));
            black_box(crate::orchestrator::route_intent(&intent));
            black_box(center_id_for(&intent).as_str());
            black_box(crate::action_kernel::risk_of_intent(&intent));
        }
        let legacy_ns = before.elapsed().as_nanos() / count;
        let after = Instant::now();
        for _ in 0..count {
            black_box(prepare_action(&intent, "open Chrome", "deterministic", &turn, false, false, false));
        }
        let aligned_ns = after.elapsed().as_nanos() / count;
        println!("routing policy ns/turn: legacy={legacy_ns} aligned={aligned_ns}");
    }

    /// Plan 120 H1: every `CenterId` output must be a registered sub-center.
    /// Coverage of *variants* is compiler-forced (exhaustive match, no wildcard);
    /// this pins the *spelling* — a typo'd center name fails here, not silently
    /// at runtime. One sample intent per center; extend when adding a center.
    #[test]
    fn test_center_id_names_are_registered() {
        let samples: &[(&str, ParsedIntent)] = &[
            ("AppCenter", ParsedIntent::OpenSettings),
            ("BrowserCenter", ParsedIntent::BrowserSearch { query: "x".into() }),
            (
                "SystemCenter",
                ParsedIntent::NluResult {
                    intent: "system_focus_app".into(),
                    slots: Value::Null,
                    confidence: 1.0,
                },
            ),
            ("MediaCenter", ParsedIntent::MediaNext),
            (
                "YouTubeCenter",
                ParsedIntent::NluResult {
                    intent: "youtube_search".into(),
                    slots: Value::Null,
                    confidence: 1.0,
                },
            ),
            ("MessageCenter", ParsedIntent::WhatsappChat { contact: "x".into() }),
            (
                "CommerceCenter",
                ParsedIntent::OrderFood { query: "x".into(), restaurant: None },
            ),
            (
                "GitHubCenter",
                ParsedIntent::GitHubCommand {
                    command: crate::github_cmd::GitHubCommand::ClosePr {
                        repo: "o/r".into(),
                        pr_number: 1,
                    },
                },
            ),
            ("ArchitectCenter", ParsedIntent::OpenArchitect),
            ("GhostCenter", ParsedIntent::EnterGhostControl),
            ("HandCenter", ParsedIntent::EnterHandMode),
            ("GoogleCenter", ParsedIntent::WatchScreenEmail),
            ("MemoryCenter", ParsedIntent::MemoryAudit),
            ("FileCenter", ParsedIntent::FileUndo),
            ("DictationCenter", ParsedIntent::StartDictation),
            ("GreetingCenter", ParsedIntent::Greeting { reply: "x".into() }),
            ("KnowledgeCenter", ParsedIntent::Search { query: "x".into() }),
            ("KnowledgeCenter", ParsedIntent::Unknown { raw: "x".into() }),
        ];
        for (expected, intent) in samples {
            let got = center_id_for(intent).as_str();
            assert_eq!(got, *expected);
            assert!(
                ALL_SUB_CENTERS.contains(&center_id_for(intent)),
                "center_id_for returned unregistered center '{got}'"
            );
        }
        // The registry itself stays clean: no duplicates, no dead entries —
        // every name must be produced by at least one sample above.
        let mut seen = std::collections::HashSet::new();
        for name in ALL_SUB_CENTERS {
            assert!(seen.insert(name), "duplicate center '{}' in ALL_SUB_CENTERS", name.as_str());
            assert!(
                samples.iter().any(|(e, _)| e == &name.as_str()),
                "registered center '{}' has no routing sample (dead entry?)", name.as_str()
            );
        }
    }

    /// Plan 120 H2: the side-effect list is safety-critical. `is_side_effect`
    /// Defaults to side-effecting; this pins explicit exceptions and known
    /// write actions so a change to the safe list is deliberate.
    #[test]
    fn test_side_effect_membership_is_pinned() {
        let must_gate: &[ParsedIntent] = &[
            ParsedIntent::SendWhatsAppMessage { contact: "x".into(), message: "y".into() },
            ParsedIntent::OrderFood { query: "x".into(), restaurant: None },
            ParsedIntent::GitHubCommand {
                command: crate::github_cmd::GitHubCommand::ClosePr {
                    repo: "o/r".into(),
                    pr_number: 1,
                },
            },
            ParsedIntent::ScreenClick { ordinal: 1 },
            ParsedIntent::StartDictation,
            ParsedIntent::WatchScreenEmail,
            ParsedIntent::Search { query: "x".into() },
            ParsedIntent::CloseApp { target: "Chrome".into() },
            ParsedIntent::BlockContact { target: "A".into(), platform: "whatsapp".into(), duration_days: None },
            ParsedIntent::MemoryForgetAll,
            ParsedIntent::CalendarAdd { text: "meeting".into() },
            ParsedIntent::CodeProject { source: "desktop".into(), prompt: None },
        ];
        for intent in must_gate {
            assert!(
                is_side_effect(intent),
                "state-changing intent lost its side-effect gate: {:?}",
                crate::intent_parser::intent_to_label(intent)
            );
        }
        let pure: &[ParsedIntent] = &[
            ParsedIntent::Greeting { reply: "x".into() },
            ParsedIntent::MemoryAudit,
            ParsedIntent::OpenSettings,
            ParsedIntent::OpenGitHubDashboard { subpage: "feed".into() },
            ParsedIntent::Unknown { raw: "x".into() },
        ];
        for intent in pure {
            assert!(
                !is_side_effect(intent),
                "pure intent wrongly gated as side-effect: {:?}",
                crate::intent_parser::intent_to_label(intent)
            );
        }
    }

    /// Loading guard matrix (P2): orphan spinners and ghost-session windows
    /// are refused; normal turns pass.
    #[test]
    fn test_loading_guard_matrix() {
        assert!(loading_should_show(true, false));
        assert!(!loading_should_show(false, false)); // orphan — no turn
        assert!(!loading_should_show(true, true)); // ghost — waves own it
        assert!(!loading_should_show(false, true));
    }
}

// ─── P3: voice directors (doc 74) ─────────────────────────────────────
// Pure policy behind STT mute gating and TTS engine selection. These mirror
// the inline conditions in the audio callback (`should_suppress_wake`) and
// the synthesis fallback chain — same outcomes, now pinned and tested, with
// human-readable reasons for logs (and future spoken turn-block notices).

/// STT capture gate verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttGate {
    Open,
    Muted(SttMuteReason),
}

/// Why capture is muted, in priority order (matches `should_suppress_wake`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttMuteReason {
    ManualPause,
    TtsPlaying,
    Meeting,
}

/// `meeting_suppress` is `detection_enabled && meeting_active` (same inputs
/// as `MeetingState::should_suppress_wake`).
pub fn stt_gate(manual_pause: bool, tts_playing: bool, meeting_suppress: bool) -> SttGate {
    if manual_pause {
        SttGate::Muted(SttMuteReason::ManualPause)
    } else if tts_playing {
        SttGate::Muted(SttMuteReason::TtsPlaying)
    } else if meeting_suppress {
        SttGate::Muted(SttMuteReason::Meeting)
    } else {
        SttGate::Open
    }
}

/// Short reason for logs; doubles as spoken phrasing if a turn is ever
/// blocked audibly ("not listening — {note}").
pub fn stt_mute_note(gate: SttGate) -> &'static str {
    match gate {
        SttGate::Open => "listening",
        SttGate::Muted(SttMuteReason::ManualPause) => "manually paused",
        SttGate::Muted(SttMuteReason::TtsPlaying) => "NEXUS speaking (TTS mute)",
        SttGate::Muted(SttMuteReason::Meeting) => "meeting mode (mic in use)",
    }
}

/// TTS synthesis engine choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsEngine {
    Edge,
    Local,
}

/// Upfront engine policy: explicit local voice always goes local;
/// network-down skips Edge entirely (saves ~1-2s of failing). Otherwise
/// Edge is attempted first with the local Kokoro voice as the per-call fallback.
pub fn tts_engine_for(local_voice_selected: bool, network_up: bool) -> TtsEngine {
    if local_voice_selected || !network_up {
        TtsEngine::Local
    } else {
        TtsEngine::Edge
    }
}

#[cfg(test)]
mod director_tests {
    use super::{
        action_disposition, command_evidence, is_side_effect, stt_gate, stt_mute_note,
        tts_engine_for, ActionDisposition, CommandEvidence, SttGate, SttMuteReason, TtsEngine,
    };
    use crate::intent_parser::ParsedIntent;
    use crate::voice_profile::TurnOwnership;

    /// Priority order mirrors `should_suppress_wake`: pause > TTS > meeting.
    #[test]
    fn test_stt_gate_matrix() {
        assert_eq!(stt_gate(false, false, false), SttGate::Open);
        assert_eq!(stt_gate(true, false, false), SttGate::Muted(SttMuteReason::ManualPause));
        assert_eq!(stt_gate(false, true, false), SttGate::Muted(SttMuteReason::TtsPlaying));
        assert_eq!(stt_gate(false, false, true), SttGate::Muted(SttMuteReason::Meeting));
        // Priority, not first-match-any:
        assert_eq!(stt_gate(true, true, true), SttGate::Muted(SttMuteReason::ManualPause));
        assert_eq!(stt_gate(false, true, true), SttGate::Muted(SttMuteReason::TtsPlaying));
        assert_eq!(
            stt_mute_note(SttGate::Muted(SttMuteReason::Meeting)),
            "meeting mode (mic in use)"
        );
        assert_eq!(stt_mute_note(SttGate::Open), "listening");
    }

    #[test]
    fn test_tts_engine_matrix() {
        assert_eq!(tts_engine_for(true, true), TtsEngine::Local);
        assert_eq!(tts_engine_for(false, false), TtsEngine::Local);
        assert_eq!(tts_engine_for(false, true), TtsEngine::Edge);
    }

    #[test]
    fn test_background_narrative_never_becomes_repository_action() {
        let narrative = "I tell my job is to say I should be the king of the king";
        let unknown = ParsedIntent::Unknown {
            raw: narrative.to_string(),
        };
        assert_eq!(
            command_evidence(&unknown, narrative, "none"),
            CommandEvidence::None
        );
        assert!(!is_side_effect(&unknown));
        assert_eq!(
            action_disposition(&unknown, narrative, "none", TurnOwnership::Uncertain, true),
            ActionDisposition::AmbientDrop
        );
        assert_eq!(
            action_disposition(&unknown, narrative, "none", TurnOwnership::Rejected, false),
            ActionDisposition::AmbientDrop
        );
    }

    #[test]
    fn test_repository_actions_require_repository_evidence() {
        let bare = ParsedIntent::AnalyseRepo {
            owner: None,
            repo: "king of the king".to_string(),
        };
        assert_eq!(
            command_evidence(&bare, "analyse king of the king", "deterministic"),
            CommandEvidence::Medium
        );
        assert_eq!(
            action_disposition(
                &bare,
                "analyse king of the king",
                "deterministic",
                TurnOwnership::Uncertain,
                true
            ),
            ActionDisposition::AmbientDrop
        );

        let grounded = ParsedIntent::AnalyseRepo {
            owner: None,
            repo: "zync".to_string(),
        };
        assert_eq!(
            command_evidence(&grounded, "analyse zync", "deterministic"),
            CommandEvidence::Medium
        );
        assert_eq!(
            action_disposition(
                &grounded,
                "analyse zync",
                "deterministic",
                TurnOwnership::Verified,
                true
            ),
            ActionDisposition::Allow
        );
    }

    #[test]
    fn test_analyse_repo_cached_bare_repo_yields_strong_evidence() {
        let unique_name = "unique-repo-feat161";
        crate::github_repo_index::seed_cache_for_test(vec![
            crate::github_repo_index::RepoEntry {
                full_name: format!("acme/{unique_name}"),
                owner: "acme".into(),
                name: unique_name.into(),
                owner_type: "User".into(),
                open_issues: 1,
            },
        ]);

        let intent = ParsedIntent::AnalyseRepo {
            owner: None,
            repo: unique_name.to_string(),
        };
        assert_eq!(
            command_evidence(&intent, &format!("analyse {unique_name}"), "deterministic"),
            CommandEvidence::Strong
        );
        assert_eq!(
            action_disposition(
                &intent,
                &format!("analyse {unique_name}"),
                "deterministic",
                TurnOwnership::Unenrolled,
                false
            ),
            ActionDisposition::Allow
        );
    }

    #[test]
    fn test_ghost_mode_unenrolled_strong_command_allowed() {
        let tab_cmd = ParsedIntent::NluResult {
            intent: "browser_new_tab".to_string(),
            slots: serde_json::json!({}),
            confidence: 0.95,
        };
        assert_eq!(
            command_evidence(&tab_cmd, "create a new tab", "deterministic"),
            CommandEvidence::Strong
        );
        assert_eq!(
            action_disposition(
                &tab_cmd,
                "create a new tab",
                "deterministic",
                TurnOwnership::Unenrolled,
                true
            ),
            ActionDisposition::Allow
        );
    }

    /// Every phrase-grammar parse is evidence in a Ghost session, whatever its
    /// `deterministic-*` source label. "type google meet" (source
    /// `deterministic-dictation`) used to score 0 and vanish without a word.
    #[test]
    fn ghost_unenrolled_accepts_every_deterministic_source() {
        for src in [
            "deterministic",
            "deterministic-live",
            "deterministic-dictation",
            "deterministic-screen-analysis",
            "deterministic-directions",
            "deterministic-memory",
            "deterministic-persona",
            "deterministic-places",
        ] {
            assert_eq!(crate::center::source_strength(src), 3, "{src}");
        }
        assert_eq!(crate::center::source_strength("deterministic-fuzzy"), 1);

        let parsed = crate::intent_parser::parse_deterministic("type google meet.")
            .expect("'type google meet' must parse");
        assert_eq!(parsed.source, "deterministic-dictation");
        assert_eq!(
            action_disposition(&parsed.intent, "type google meet.", &parsed.source, TurnOwnership::Unenrolled, true),
            ActionDisposition::Allow
        );
    }

    #[test]
    fn ghost_unenrolled_medium_evidence_runs_instead_of_vanishing() {
        let nlu = ParsedIntent::NluResult {
            intent: "open_app".to_string(),
            slots: serde_json::json!({ "target": "notepad" }),
            confidence: 0.945,
        };
        assert_eq!(command_evidence(&nlu, "nexus. oco notepad.", "nlu"), CommandEvidence::Medium);
        assert_eq!(
            action_disposition(&nlu, "nexus. oco notepad.", "nlu", TurnOwnership::Unenrolled, true),
            ActionDisposition::Allow
        );
        // Outside Ghost the same turn is still confirmed first.
        assert_eq!(
            action_disposition(&nlu, "nexus. oco notepad.", "nlu", TurnOwnership::Unenrolled, false),
            ActionDisposition::Clarify
        );
        // A voice that is positively NOT the owner is still dropped.
        assert_eq!(
            action_disposition(&nlu, "nexus. oco notepad.", "nlu", TurnOwnership::Rejected, true),
            ActionDisposition::AmbientDrop
        );
    }

    fn live(intent: &str) -> ParsedIntent {
        ParsedIntent::NluResult {
            intent: intent.to_string(),
            slots: serde_json::json!({}),
            confidence: 1.0,
        }
    }

    /// Regression: with speaker verification off (every turn `Unenrolled`) an
    /// explicit ghost-session command must run, not be dropped as ambient.
    #[test]
    fn test_ghost_unenrolled_explicit_commands_are_allowed() {
        for source in [
            "deterministic",
            "deterministic-live",
            "deterministic-dictation",
            "deterministic-screen-analysis",
        ] {
            assert_eq!(
                action_disposition(
                    &live("browser_new_tab"),
                    "new tab",
                    source,
                    TurnOwnership::Unenrolled,
                    true
                ),
                ActionDisposition::Allow,
                "source {source}"
            );
        }
    }

    /// The ghost mic is hot: narrative speech stays ambient, and a voice that is
    /// positively not the owner is never obeyed.
    #[test]
    fn test_ghost_unenrolled_narrative_still_dropped() {
        let unknown = ParsedIntent::Unknown {
            raw: "we should probably leave soon".to_string(),
        };
        assert_eq!(
            action_disposition(&unknown, "we should probably leave soon", "none", TurnOwnership::Unenrolled, true),
            ActionDisposition::AmbientDrop
        );
        assert_eq!(
            action_disposition(&live("type_text"), "type hi", "deterministic-live", TurnOwnership::Rejected, true),
            ActionDisposition::AmbientDrop
        );
    }

    /// Outside ghost the original behavior is unchanged.
    #[test]
    fn test_non_ghost_unenrolled_unchanged() {
        assert_eq!(
            action_disposition(&live("browser_new_tab"), "new tab", "deterministic-live", TurnOwnership::Unenrolled, false),
            ActionDisposition::Allow
        );
        assert_eq!(
            action_disposition(&live("type_text"), "type hi", "nlu", TurnOwnership::Unenrolled, false),
            ActionDisposition::Clarify
        );
    }

    #[test]
    fn test_find_pr_is_pure_and_strong_evidence() {
        let find_intent = ParsedIntent::GitHubCommand {
            command: crate::github_cmd::GitHubCommand::FindPr { pr_number: 34 },
        };
        assert!(!is_side_effect(&find_intent), "FindPr must be pure (read-only)");
        assert_eq!(
            command_evidence(&find_intent, "analyse pr 34", "deterministic"),
            CommandEvidence::Strong
        );
    }
}
