//! Administrator review commands. This is a separate capability from the
//! machine owner's wake-word profile and from `admin-brain` configuration.
//! No UI or transcript can submit a biometric verdict to this module.

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

const SESSION_MAX: Duration = Duration::from_secs(15 * 60);
const SESSION_IDLE: Duration = Duration::from_secs(5 * 60);
const CAPTURE_PROOF_TTL: Duration = Duration::from_secs(60);
const MAX_CAPTURE_PROOFS: usize = 8;

/// Only native STT can register a captured transcript. The WebView can relay
/// its session number and text, but it cannot mint an entry in this ledger.
/// A proof is exact, one-use, and short-lived; learned text corrections do
/// not create Administrator authority.
struct CapturedTurn {
    session: u64,
    text: String,
    recorded: Instant,
}

#[derive(Default)]
struct CaptureProofs(VecDeque<CapturedTurn>);

impl CaptureProofs {
    fn register(&mut self, session: u64, text: &str, now: Instant) {
        if session == 0 || text.trim().is_empty() { return; }
        self.prune(now);
        self.0.push_back(CapturedTurn {
            session, text: text.to_string(), recorded: now,
        });
        while self.0.len() > MAX_CAPTURE_PROOFS { self.0.pop_front(); }
    }

    fn consume(&mut self, session: u64, text: &str, now: Instant) -> bool {
        self.prune(now);
        if session == 0 { return false; }
        let Some(index) = self.0.iter().position(|proof| {
            proof.session == session && proof.text == text
        }) else { return false; };
        self.0.remove(index);
        true
    }

    fn prune(&mut self, now: Instant) {
        self.0.retain(|proof| now.duration_since(proof.recorded) < CAPTURE_PROOF_TTL);
    }
}

static CAPTURE_PROOFS: Lazy<Mutex<CaptureProofs>> = Lazy::new(|| Mutex::new(CaptureProofs::default()));

/// Opaque evidence that this exact text came from the native capture path.
pub(crate) struct NativeCapture { _private: () }

/// Called only by the Rust microphone/STT receiver before it emits a turn.
pub(crate) fn register_captured_turn(session: u64, text: &str) {
    CAPTURE_PROOFS.lock().register(session, text, Instant::now());
}

/// Called at the Administrator command boundary. There is no Tauri command
/// that exposes this operation or accepts a WebView-provided verdict.
pub(crate) fn consume_captured_turn(session: u64, text: &str) -> Option<NativeCapture> {
    CAPTURE_PROOFS.lock().consume(session, text, Instant::now())
        .then_some(NativeCapture { _private: () })
}

pub(crate) fn invalidate_captured_turns() {
    CAPTURE_PROOFS.lock().0.clear();
}

/// This state has no production constructor until the live voice/face verifier
/// can create a ceremony proof. A transcript or WebView cannot open it.
struct Session {
    started: Instant,
    last_action: Instant,
    export_approved: bool,
}

impl Session {
    fn live(&self, now: Instant) -> bool {
        now.duration_since(self.started) < SESSION_MAX
            && now.duration_since(self.last_action) < SESSION_IDLE
    }

    fn remaining(&self, now: Instant) -> Option<Duration> {
        if !self.live(now) { return None; }
        Some((SESSION_MAX - now.duration_since(self.started))
            .min(SESSION_IDLE - now.duration_since(self.last_action)))
    }

    fn authorize(&self, capability: Capability, captured: bool, paired_laptop: bool, now: Instant)
        -> Result<(), &'static str>
    {
        // A policy check is not a completed action. The executor in A4 will
        // update last_action only after its own gate and outcome succeed.
        capability_policy(capability, captured, self.live(now), paired_laptop,
            self.export_approved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Capability {
    ReviewSidebar,
    ReviewExport,
}

fn capability_policy(
    capability: Capability, captured: bool, active_session: bool,
    paired_laptop: bool, export_approved: bool,
) -> Result<(), &'static str> {
    if !captured { return Err("captured_voice_required"); }
    if !active_session { return Err("administrator_session_required"); }
    match capability {
        Capability::ReviewExport if !paired_laptop => Err("administrator_laptop_not_paired"),
        Capability::ReviewExport if !export_approved => Err("device_user_export_approval_required"),
        _ => Ok(()),
    }
}

static SESSION: Lazy<Mutex<Option<Session>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewTarget {
    /// Resolve only from the active coding workspace at execution time.
    /// The parser never accepts a spoken arbitrary filesystem path.
    CurrentRepository,
    Repository {
        owner: Option<String>,
        repo: String,
    },
    PullRequest {
        owner: Option<String>,
        repo: String,
        number: u32,
    },
    LatestPullRequest {
        owner: Option<String>,
        repo: String,
        author: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewDestination {
    PairedAdministratorLaptop,
    DeviceUserSidebar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCommand {
    BeginSession,
    EndSession,
    SessionStatus,
    Review {
        target: ReviewTarget,
        destination: ReviewDestination,
    },
    Unsupported,
}

fn valid_repository_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn valid_target(target: &ReviewTarget) -> bool {
    if matches!(target, ReviewTarget::CurrentRepository) {
        return true;
    }
    let (owner, repo) = match target {
        ReviewTarget::CurrentRepository => unreachable!(),
        ReviewTarget::Repository { owner, repo }
        | ReviewTarget::PullRequest { owner, repo, .. }
        | ReviewTarget::LatestPullRequest { owner, repo, .. } => (owner, repo),
    };
    valid_repository_part(repo)
        && (owner.is_some() || !matches!(repo.as_str(), "repo" | "repository"))
        && owner
            .as_ref()
            .is_none_or(|owner| valid_repository_part(owner))
        && match target {
            ReviewTarget::CurrentRepository => true,
            ReviewTarget::PullRequest { number, .. } => *number > 0,
            ReviewTarget::LatestPullRequest { author, .. } => author
                .as_ref()
                .is_none_or(|author| valid_repository_part(author)),
            ReviewTarget::Repository { .. } => true,
        }
}

fn split_repo(value: &str) -> (Option<String>, String) {
    match value.split_once('/') {
        Some((owner, repo)) => (Some(owner.to_string()), repo.to_string()),
        None => (None, value.to_string()),
    }
}

/// Security-sensitive grammar stays independent of learned STT corrections,
/// fuzzy app matching, NLU, and cloud model output.
fn parse_review_target(body: &str) -> Option<ReviewTarget> {
    if matches!(
        body,
        "analyse this repo"
            | "analyse this repository"
            | "analyse the current repo"
            | "analyse the current repository"
            | "analyse the active repo"
            | "analyse the active repository"
    ) {
        return Some(ReviewTarget::CurrentRepository);
    }
    static NUMBERED_PR: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r"^analyse (?:the )?(?:pr|pull request) (?:number )?([1-9][0-9]*) (?:in |of |for )?([a-z0-9._-]+(?:/[a-z0-9._-]+)?)$"
    ).unwrap()
    });
    static LATEST_PR: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r"^analyse (?:the )?(?:latest )?(?:pr|pull request) (?:in |of |for )([a-z0-9._-]+(?:/[a-z0-9._-]+)?)(?: by ([a-z0-9._-]+))?$"
    ).unwrap()
    });
    static REPO: Lazy<Regex> = Lazy::new(|| {
        Regex::new(
        r"^analyse (?:the )?(?:(?:repo|repository) )?([a-z0-9._-]+(?:/[a-z0-9._-]+)?)(?: (?:repo|repository))?$"
    ).unwrap()
    });
    if let Some(c) = NUMBERED_PR.captures(body) {
        let number = c.get(1)?.as_str().parse().ok()?;
        let (owner, repo) = split_repo(c.get(2)?.as_str());
        return Some(ReviewTarget::PullRequest {
            owner,
            repo,
            number,
        });
    }
    if let Some(c) = LATEST_PR.captures(body) {
        let (owner, repo) = split_repo(c.get(1)?.as_str());
        let author = c.get(2).map(|m| m.as_str().to_string());
        return Some(ReviewTarget::LatestPullRequest {
            owner,
            repo,
            author,
        });
    }
    if let Some(c) = REPO.captures(body) {
        let (owner, repo) = split_repo(c.get(1)?.as_str());
        return Some(ReviewTarget::Repository { owner, repo });
    }
    None
}

/// The parser accepts explicit administrator requests only. An unsupported
/// request is claimed locally so the cloud cannot interpret it as authority.
pub fn parse_command(transcript: &str) -> Option<AdminCommand> {
    let normalized = transcript
        .trim()
        .trim_end_matches(['.', '?', '!'])
        .to_ascii_lowercase();
    let s = normalized.trim();
    let s = s
        .strip_prefix("hey nexus, ")
        .or_else(|| s.strip_prefix("hey nexus "))
        .or_else(|| s.strip_prefix("nexus, "))
        .or_else(|| s.strip_prefix("nexus "))
        .unwrap_or(s);
    if matches!(
        s,
        "start administrator session"
            | "open administrator session"
            | "start admin session"
            | "open admin session"
            | "verify me as administrator"
            | "authenticate me as administrator"
            | "administrator access"
            | "admin access"
    ) {
        return Some(AdminCommand::BeginSession);
    }
    if matches!(
        s,
        "end administrator session"
            | "close administrator session"
            | "end admin session"
            | "close admin session"
            | "exit administrator mode"
            | "exit admin mode"
    ) {
        return Some(AdminCommand::EndSession);
    }
    if matches!(
        s,
        "administrator session status"
            | "admin session status"
            | "am i in administrator mode"
            | "am i in admin mode"
    ) {
        return Some(AdminCommand::SessionStatus);
    }
    if matches!(s, "revoke administrator access" | "revoke admin access") {
        return Some(AdminCommand::Unsupported);
    }

    let request = s
        .strip_prefix("administrator ")
        .or_else(|| s.strip_prefix("administrator, "))
        .or_else(|| s.strip_prefix("admin "))
        .or_else(|| s.strip_prefix("admin, "))
        .or_else(|| s.strip_prefix("as administrator "))
        .or_else(|| s.strip_prefix("as administrator, "))
        .or_else(|| s.strip_prefix("as admin "));
    let request = request.or_else(|| s.strip_prefix("as admin, "));
    let Some(request) = request else {
        // Prevent a destination-only command from being mistaken for a chat
        // request or a generic file send through another executor.
        if (s.contains("send") || s.contains("deliver"))
            && s.contains("review")
            && (s.contains("my laptop")
                || s.contains("administrator laptop")
                || s.contains("admin laptop")
                || s.contains("user sidebar"))
        {
            return Some(AdminCommand::Unsupported);
        }
        return None;
    };

    let destinations = [
        (
            " to my laptop",
            ReviewDestination::PairedAdministratorLaptop,
        ),
        (
            " to the administrator laptop",
            ReviewDestination::PairedAdministratorLaptop,
        ),
        (
            " to the admin laptop",
            ReviewDestination::PairedAdministratorLaptop,
        ),
        (
            " to my paired laptop",
            ReviewDestination::PairedAdministratorLaptop,
        ),
        (" to the user sidebar", ReviewDestination::DeviceUserSidebar),
        (
            " to the device user sidebar",
            ReviewDestination::DeviceUserSidebar,
        ),
        (" in the user sidebar", ReviewDestination::DeviceUserSidebar),
        (" on the user sidebar", ReviewDestination::DeviceUserSidebar),
    ];
    let Some((body, destination)) = destinations.iter().find_map(|(suffix, destination)| {
        request
            .strip_suffix(suffix)
            .map(|body| (body.trim(), *destination))
    }) else {
        return Some(AdminCommand::Unsupported);
    };
    let body = body
        .strip_suffix(" and send the review")
        .or_else(|| body.strip_suffix(" and send it"))
        .or_else(|| body.strip_suffix(" and deliver the review"))
        .or_else(|| body.strip_suffix(" and deliver it"))
        .or_else(|| body.strip_suffix(" then send the review"))
        .or_else(|| body.strip_suffix(" then send it"))
        .unwrap_or(body);
    let body = if let Some(rest) = body.strip_prefix("review ") {
        format!("analyse {rest}")
    } else if let Some(rest) = body.strip_prefix("analyze ") {
        format!("analyse {rest}")
    } else {
        body.to_string()
    };
    let Some(target) = parse_review_target(&body) else {
        return Some(AdminCommand::Unsupported);
    };
    if !valid_target(&target) {
        return Some(AdminCommand::Unsupported);
    }
    Some(AdminCommand::Review {
        target,
        destination,
    })
}

/// Release builds still cannot open a session until live voice and face
/// verifiers are integrated. Expired sessions are cleared on every read.
pub fn end_session() {
    *SESSION.lock() = None;
}
pub fn session_active() -> bool {
    session_remaining_seconds().is_some()
}
pub fn session_remaining_seconds() -> Option<u64> {
    let now = Instant::now();
    let mut guard = SESSION.lock();
    let remaining = guard.as_ref().and_then(|s| s.remaining(now));
    if remaining.is_none() { *guard = None; }
    remaining.map(|duration| duration.as_secs())
}

/// Only the review capability is in scope. Neither a valid session nor an
/// artifact destination confers general file, message, or device access.
pub fn review_allowed(
    command: &AdminCommand,
    captured: Option<&NativeCapture>,
    paired_laptop: bool,
) -> Result<(), &'static str> {
    let capability = match command {
        AdminCommand::Review { destination: ReviewDestination::DeviceUserSidebar, .. } => Capability::ReviewSidebar,
        AdminCommand::Review { destination: ReviewDestination::PairedAdministratorLaptop, .. } => Capability::ReviewExport,
        _ => return Err("unsupported_administrator_action"),
    };
    let now = Instant::now();
    let mut guard = SESSION.lock();
    let Some(session) = guard.as_mut() else { return capability_policy(capability, captured.is_some(), false, paired_laptop, false); };
    let result = session.authorize(capability, captured.is_some(), paired_laptop, now);
    if !session.live(now) { *guard = None; }
    result
}

#[cfg(test)]
fn review_policy(
    command: &AdminCommand,
    captured: bool,
    active_session: bool,
    paired_laptop: bool,
    export_approved: bool,
) -> Result<(), &'static str> {
    let AdminCommand::Review { destination, .. } = command else {
        return Err("unsupported_administrator_action");
    };
    let capability = match destination {
        ReviewDestination::DeviceUserSidebar => Capability::ReviewSidebar,
        ReviewDestination::PairedAdministratorLaptop => Capability::ReviewExport,
    };
    capability_policy(capability, captured, active_session, paired_laptop, export_approved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_capture_proof_is_exact_one_use_and_expires() {
        let now = Instant::now();
        let mut proofs = CaptureProofs::default();
        proofs.register(0, "admin access", now);
        assert!(!proofs.consume(0, "admin access", now));
        proofs.register(7, "admin access", now);
        assert!(!proofs.consume(7, "admin send files", now));
        assert!(!proofs.consume(8, "admin access", now));
        assert!(proofs.consume(7, "admin access", now));
        assert!(!proofs.consume(7, "admin access", now));
        proofs.register(9, "admin access", now);
        assert!(!proofs.consume(9, "admin access", now + CAPTURE_PROOF_TTL));
        for index in 1..=MAX_CAPTURE_PROOFS + 1 {
            proofs.register(index as u64, &format!("turn {index}"), now);
        }
        assert!(!proofs.consume(1, "turn 1", now));
        assert!(proofs.consume(2, "turn 2", now));
    }

    #[test]
    fn session_control_phrases_are_claimed_locally() {
        for phrase in [
            "Start administrator session",
            "Open admin session",
            "Verify me as administrator",
            "Authenticate me as administrator",
            "Nexus, administrator access",
        ] {
            assert_eq!(
                parse_command(phrase),
                Some(AdminCommand::BeginSession),
                "{phrase}"
            );
        }
        for phrase in [
            "End administrator session",
            "Close admin session",
            "Exit admin mode",
        ] {
            assert_eq!(
                parse_command(phrase),
                Some(AdminCommand::EndSession),
                "{phrase}"
            );
        }
        for phrase in ["Administrator session status", "Am I in admin mode?"] {
            assert_eq!(
                parse_command(phrase),
                Some(AdminCommand::SessionStatus),
                "{phrase}"
            );
        }
    }

    #[test]
    fn explicit_review_phrases_parse_with_destination() {
        for (phrase, destination) in [
            ("Nexus, administrator, analyse PR 24 in owner/repo and send it to my laptop", ReviewDestination::PairedAdministratorLaptop),
            ("admin, review owner/repo and deliver the review to the user sidebar", ReviewDestination::DeviceUserSidebar),
            ("as administrator analyze the latest PR in owner/repo then send it to my paired laptop", ReviewDestination::PairedAdministratorLaptop),
            ("as admin analyse project-repo and send it to the admin laptop", ReviewDestination::PairedAdministratorLaptop),
            ("administrator analyse pull request 19 in owner/repo and send it to the administrator laptop", ReviewDestination::PairedAdministratorLaptop),
            ("administrator analyse repository owner/repo and send it in the user sidebar", ReviewDestination::DeviceUserSidebar),
            ("administrator review project-repo and send it to the device user sidebar", ReviewDestination::DeviceUserSidebar),
            ("admin analyze project-repo and send it on the user sidebar", ReviewDestination::DeviceUserSidebar),
        ] {
            assert!(matches!(parse_command(phrase), Some(AdminCommand::Review { destination: d, .. }) if d == destination), "{phrase}");
        }
        assert!(matches!(
            parse_command("administrator review this repo and send it to the user sidebar"),
            Some(AdminCommand::Review {
                target: ReviewTarget::CurrentRepository,
                destination: ReviewDestination::DeviceUserSidebar,
            })
        ));
    }

    #[test]
    fn unsupported_admin_actions_fail_closed() {
        for phrase in [
            "admin send the user's files to my laptop",
            "administrator review owner/repo",
            "administrator review the repo and send it to my laptop",
            "send the review to my laptop",
            "administrator analyse the user's messages and send it to my laptop",
            "administrator analyse PR 0 in owner/repo and send it to my laptop",
            "admin review ../.env and send it to my laptop",
            "administrator analyse owner/repo and delete files and send it to my laptop",
            "revoke administrator access",
        ] {
            assert_eq!(
                parse_command(phrase),
                Some(AdminCommand::Unsupported),
                "{phrase}"
            );
        }
    }

    #[test]
    fn review_requires_live_session_voice_origin_and_pairing() {
        let capture = NativeCapture { _private: () };
        assert!(!session_active());
        assert_eq!(
            review_allowed(&AdminCommand::Unsupported, Some(&capture), false),
            Err("unsupported_administrator_action")
        );
        let review = AdminCommand::Review {
            target: ReviewTarget::Repository {
                owner: Some("owner".into()),
                repo: "repo".into(),
            },
            destination: ReviewDestination::PairedAdministratorLaptop,
        };
        assert_eq!(
            review_policy(&review, false, true, true, true),
            Err("captured_voice_required")
        );
        assert_eq!(
            review_policy(&review, true, false, true, true),
            Err("administrator_session_required")
        );
        assert_eq!(
            review_policy(&review, true, true, false, true),
            Err("administrator_laptop_not_paired")
        );
        assert_eq!(
            review_policy(&review, true, true, true, false),
            Err("device_user_export_approval_required")
        );
        assert_eq!(review_policy(&review, true, true, true, true), Ok(()));
        assert_eq!(
            review_allowed(&review, Some(&capture), true),
            Err("administrator_session_required")
        );
        end_session(); // harmless and idempotent while the feature is unavailable
        assert!(!session_active());
    }

    #[test]
    fn session_expires_at_idle_or_absolute_limit() {
        let now = Instant::now();
        let base = Session {
            started: now, last_action: now, export_approved: false,
        };
        assert!(base.live(now + Duration::from_secs(299)));
        assert!(!base.live(now + Duration::from_secs(300)));
        let recently_active = Session {
            started: now, last_action: now + Duration::from_secs(700),
            export_approved: true,
        };
        assert!(recently_active.live(now + Duration::from_secs(899)));
        assert!(!recently_active.live(now + Duration::from_secs(900)));
    }

    #[test]
    fn policy_checks_do_not_extend_idle() {
        let now = Instant::now();
        let mut session = Session {
            started: now, last_action: now, export_approved: false,
        };
        let later = now + Duration::from_secs(240);
        assert_eq!(session.authorize(Capability::ReviewExport, true, true, later),
            Err("device_user_export_approval_required"));
        assert_eq!(session.last_action, now);
        assert_eq!(session.authorize(Capability::ReviewSidebar, true, false, later), Ok(()));
        assert_eq!(session.last_action, now);
        assert_eq!(session.authorize(Capability::ReviewExport, true, true, later),
            Err("device_user_export_approval_required"));
        session.export_approved = true;
        assert_eq!(session.authorize(Capability::ReviewExport, true, false, later),
            Err("administrator_laptop_not_paired"));
        assert_eq!(session.authorize(Capability::ReviewExport, true, true, later), Ok(()));
        assert_eq!(session.authorize(Capability::ReviewSidebar, false, true, later),
            Err("captured_voice_required"));
        assert_eq!(session.authorize(Capability::ReviewSidebar, true, true,
            now + SESSION_MAX), Err("administrator_session_required"));
    }
}
