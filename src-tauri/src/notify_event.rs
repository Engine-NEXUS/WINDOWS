//! Canonical, versioned notification event + typed navigation target + resolver (notification-to-sidebar N1).
//!
//! Identity is FULL: provider host, account id, owner, repo, PR/run/thread/deployment ids. A human project name is
//! display text only. A click resolves an opaque event id through the ledger; it never parses a sentence, follows a
//! URL from provider content, or falls back to a default repo / another account / the newest item.
//!
//! Event, entity, delivery and read state stay separate: viewing an alert changes only NEXUS's own `read` flag and
//! never provider state. Pure and bounded: no I/O, no network, no Tauri. Persistence and the `notify_brain` key
//! migration are described in docs/changes/171-notification-event-contract.md (N2 wires them).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
const LEDGER_CAP: usize = 200;
const MAX_ID_LEN: usize = 100;
const MAX_TITLE: usize = 120;
const MAX_PREVIEW: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Provider {
    GitHub,
    Gmail,
    Vercel,
    Render,
    Netlify,
    Supabase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Status {
    Failed,
    Recovered,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

/// `Redacted` events never keep their preview text (meeting / privacy contexts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrivacyClass {
    Public,
    Redacted,
}

/// NEXUS alert state only; never mirrors or changes provider state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReadState {
    Unread,
    Seen,
    Dismissed,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Delivery {
    pub spoken: bool,
    pub carded: bool,
    pub history: bool,
}

/// Exact object to show. Resolved through the ledger by opaque event id, never built from text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Target {
    GitHub {
        host: String,
        owner: String,
        repo: String,
        pr: Option<u64>,
        run_id: Option<u64>,
        sha: Option<String>,
    },
    Gmail {
        thread_id: String,
        message_id: Option<String>,
    },
    Deployment {
        provider: Provider,
        team: String,
        project: String,
        deployment_id: String,
        revision: Option<String>,
    },
    /// Repository-wide observation: do not guess a PR.
    Repo {
        host: String,
        owner: String,
        repo: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NotifyEvent {
    pub schema_version: u32,
    pub event_id: String,
    pub entity_key: String,
    pub provider: Provider,
    pub account_id: String,
    /// What is tracked: "pr" | "build" | "deploy" | "mail". Part of the entity key, not of the status.
    pub kind: String,
    pub status: Status,
    pub severity: Severity,
    pub occurred_at_ms: u64,
    pub observed_at_ms: u64,
    pub updated_at_ms: u64,
    pub title: String,
    pub preview: String,
    pub privacy: PrivacyClass,
    pub target: Target,
    pub source_ref: String,
    /// The first failing occurrence; a recovery never overwrites it.
    pub original_source_ref: String,
    /// How many failing occurrences were grouped into this event.
    pub occurrences: u32,
    pub delivery: Delivery,
    pub read: ReadState,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventError {
    UnsupportedSchema(u32),
    InvalidField(&'static str),
    NotFound,
    Expired,
    AccountDisconnected,
    #[cfg(test)]
    Malformed,
}

fn ident_ok(s: &str, extra: &[char]) -> bool {
    !s.is_empty()
        && s.len() <= MAX_ID_LEN
        && !s.contains("..")
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || extra.contains(&c))
}

/// host / owner / repo / team / project: `[A-Za-z0-9._-]` (host also `:`); no slashes, spaces or URLs.
fn name_ok(s: &str) -> bool {
    ident_ok(s, &['.'])
}
fn host_ok(s: &str) -> bool {
    ident_ok(s, &['.', ':'])
}
/// thread / message / deployment / revision ids.
fn id_ok(s: &str) -> bool {
    ident_ok(s, &[])
}
/// Opaque local account binding: also allows `@`, `.` and `+` (an email-shaped id is fine).
fn account_ok(s: &str) -> bool {
    ident_ok(s, &['.', '@', '+'])
}

pub fn validate_target(t: &Target) -> Result<(), EventError> {
    let bad = EventError::InvalidField;
    match t {
        Target::GitHub { host, owner, repo, run_id, sha, pr } => {
            if !host_ok(host) {
                return Err(bad("host"));
            }
            if !name_ok(owner) {
                return Err(bad("owner"));
            }
            if !name_ok(repo) {
                return Err(bad("repo"));
            }
            if sha.as_deref().is_some_and(|s| !id_ok(s)) {
                return Err(bad("sha"));
            }
            if *run_id == Some(0) {
                return Err(bad("run_id"));
            }
            if *pr == Some(0) {
                return Err(bad("pr"));
            }
            Ok(())
        }
        Target::Repo { host, owner, repo } => {
            if !host_ok(host) {
                return Err(bad("host"));
            }
            if !name_ok(owner) {
                return Err(bad("owner"));
            }
            if !name_ok(repo) {
                return Err(bad("repo"));
            }
            Ok(())
        }
        Target::Gmail { thread_id, message_id } => {
            if !id_ok(thread_id) {
                return Err(bad("thread_id"));
            }
            if message_id.as_deref().is_some_and(|m| !id_ok(m)) {
                return Err(bad("message_id"));
            }
            Ok(())
        }
        Target::Deployment { provider, team, project, deployment_id, revision } => {
            if matches!(provider, Provider::GitHub | Provider::Gmail) {
                return Err(bad("provider"));
            }
            if !name_ok(team) {
                return Err(bad("team"));
            }
            if !name_ok(project) {
                return Err(bad("project"));
            }
            if !id_ok(deployment_id) {
                return Err(bad("deployment_id"));
            }
            if revision.as_deref().is_some_and(|r| !id_ok(r)) {
                return Err(bad("revision"));
            }
            Ok(())
        }
    }
}

/// The provider a target can legitimately belong to (None = must be a Deployment provider, checked separately).
fn target_matches_provider(provider: Provider, t: &Target) -> bool {
    match (provider, t) {
        (Provider::GitHub, Target::GitHub { .. } | Target::Repo { .. }) => true,
        (Provider::Gmail, Target::Gmail { .. }) => true,
        (p, Target::Deployment { provider: tp, .. }) => p == *tp,
        _ => false,
    }
}

/// Strips control characters, collapses whitespace runs, truncates to `max` chars (char-safe).
pub fn sanitize_text(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for c in s.chars() {
        if c.is_control() {
            continue;
        }
        if c.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(c);
    }
    out.chars().take(max).collect()
}

fn slug(p: Provider) -> &'static str {
    match p {
        Provider::GitHub => "github",
        Provider::Gmail => "gmail",
        Provider::Vercel => "vercel",
        Provider::Render => "render",
        Provider::Netlify => "netlify",
        Provider::Supabase => "supabase",
    }
}

/// Stable entity key from FULL identity. Two accounts or owners sharing a repo name never collide.
/// Independent of status, so a failure and its recovery address the same entity.
pub fn entity_key(provider: Provider, account_id: &str, t: &Target, kind: &str) -> String {
    let (p, a, k) = (slug(provider), account_id.to_lowercase(), kind.to_lowercase());
    match t {
        Target::GitHub { host, owner, repo, .. } | Target::Repo { host, owner, repo } => {
            format!("v1:{p}:{a}:{}/{}/{}:{k}", host.to_lowercase(), owner.to_lowercase(), repo.to_lowercase())
        }
        Target::Gmail { thread_id, .. } => format!("v1:{p}:{a}:{}:{k}", thread_id.to_lowercase()),
        Target::Deployment { team, project, .. } => {
            format!("v1:{p}:{a}:{}/{}:{k}", team.to_lowercase(), project.to_lowercase())
        }
    }
}

/// Deterministic opaque id: `ev_` + 24 hex chars of sha256(entity_key|source_ref|status).
pub fn event_id(entity_key: &str, source_ref: &str, status: Status) -> String {
    let mut h = Sha256::new();
    h.update(entity_key.as_bytes());
    h.update(b"|");
    h.update(source_ref.as_bytes());
    h.update(b"|");
    h.update(format!("{status:?}").as_bytes());
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    format!("ev_{}", &hex[..24])
}

/// Everything a record must satisfy before it enters the ledger. A forged id, a key that does not match the
/// identity, or a target of another provider is refused instead of being trusted.
fn validate_event(ev: &NotifyEvent) -> Result<(), EventError> {
    if ev.schema_version != SCHEMA_VERSION {
        return Err(EventError::UnsupportedSchema(ev.schema_version));
    }
    validate_target(&ev.target)?;
    if !account_ok(&ev.account_id) {
        return Err(EventError::InvalidField("account_id"));
    }
    if !ident_ok(&ev.kind, &[]) {
        return Err(EventError::InvalidField("kind"));
    }
    if ev.source_ref.is_empty()
        || ev.source_ref.len() > MAX_ID_LEN
        || !ev.source_ref.chars().all(|c| c.is_ascii_alphanumeric() || "._-:".contains(c))
    {
        return Err(EventError::InvalidField("source_ref"));
    }
    if !target_matches_provider(ev.provider, &ev.target) {
        return Err(EventError::InvalidField("provider"));
    }
    if ev.entity_key != entity_key(ev.provider, &ev.account_id, &ev.target, &ev.kind) {
        return Err(EventError::InvalidField("entity_key"));
    }
    if ev.event_id != event_id(&ev.entity_key, &ev.source_ref, ev.status) {
        return Err(EventError::InvalidField("event_id"));
    }
    if ev.title.chars().count() > MAX_TITLE {
        return Err(EventError::InvalidField("title"));
    }
    if ev.preview.chars().count() > MAX_PREVIEW {
        return Err(EventError::InvalidField("preview"));
    }
    if ev.expires_at_ms == 0 {
        return Err(EventError::InvalidField("expires_at_ms"));
    }
    Ok(())
}

#[cfg(test)] // persistence load path (N-later); tested, not yet used in production
/// Strict JSON entry point (unknown fields, unknown target types and other schema versions are refused).
pub fn parse_event(json: &str) -> Result<NotifyEvent, EventError> {
    let ev: NotifyEvent = serde_json::from_str(json).map_err(|_| EventError::Malformed)?;
    validate_event(&ev)?;
    Ok(ev)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// A new event; deliver it.
    Created(String),
    /// Exactly the same occurrence again; nothing changes, nothing is delivered again.
    Duplicate(String),
    /// A further failing run of an entity that is already failing: grouped into the open event, no new delivery.
    Grouped(String),
    /// The entity recovered; the ORIGINAL failure event was updated and keeps its context.
    Recovered(String),
}

/// Bounded in-memory event ledger (cap 200, oldest `updated_at_ms` evicted).
#[derive(Default)]
pub struct Ledger {
    entries: Vec<NotifyEvent>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.event_id == id)
    }

    fn open_failure(&self, key: &str) -> Option<usize> {
        self.entries.iter().position(|e| e.entity_key == key && e.status == Status::Failed)
    }

    pub fn record(&mut self, mut ev: NotifyEvent) -> Result<Recorded, EventError> {
        validate_event(&ev)?;
        if self.index(&ev.event_id).is_some() {
            return Ok(Recorded::Duplicate(ev.event_id));
        }
        ev.title = sanitize_text(&ev.title, MAX_TITLE);
        ev.preview = if ev.privacy == PrivacyClass::Redacted { String::new() } else { sanitize_text(&ev.preview, MAX_PREVIEW) };
        ev.delivery = Delivery::default();
        ev.read = ReadState::Unread;
        ev.occurrences = 1;
        ev.original_source_ref = ev.source_ref.clone();

        match ev.status {
            Status::Recovered => {
                if let Some(i) = self.open_failure(&ev.entity_key) {
                    let open = &mut self.entries[i];
                    open.status = Status::Recovered;
                    open.updated_at_ms = ev.updated_at_ms;
                    open.source_ref = ev.source_ref.clone();
                    // Same entity (same key), so the same owner/repo: show the latest wording and run.
                    open.title = ev.title.clone();
                    open.preview = ev.preview.clone();
                    open.target = ev.target.clone();
                    return Ok(Recorded::Recovered(open.event_id.clone()));
                }
                // A repeat of the recovery that already updated its failure must not become a new event.
                if let Some(done) = self.entries.iter().find(|e| e.entity_key == ev.entity_key && e.status == Status::Recovered && e.source_ref == ev.source_ref) {
                    return Ok(Recorded::Duplicate(done.event_id.clone()));
                }
                // No known failure: keep the fact as information, never invent a failure.
                ev.status = Status::Info;
                ev.severity = Severity::Low;
                ev.event_id = event_id(&ev.entity_key, &ev.source_ref, Status::Info);
                if self.index(&ev.event_id).is_some() {
                    return Ok(Recorded::Duplicate(ev.event_id));
                }
            }
            Status::Failed => {
                if let Some(i) = self.open_failure(&ev.entity_key) {
                    let open = &mut self.entries[i];
                    open.occurrences = open.occurrences.saturating_add(1);
                    open.updated_at_ms = ev.updated_at_ms;
                    open.source_ref = ev.source_ref.clone();
                    open.target = ev.target.clone(); // links the newest failing run
                    return Ok(Recorded::Grouped(open.event_id.clone()));
                }
            }
            Status::Info => {}
        }
        let id = ev.event_id.clone();
        self.entries.push(ev);
        while self.entries.len() > LEDGER_CAP {
            if let Some(oldest) = self.entries.iter().enumerate().min_by_key(|(_, e)| e.updated_at_ms).map(|(i, _)| i) {
                self.entries.remove(oldest);
            }
        }
        Ok(Recorded::Created(id))
    }

    /// Unread -> Seen only. Changes NEXUS's flag, never provider state.
    pub fn mark_seen(&mut self, id: &str) -> bool {
        match self.index(id).map(|i| &mut self.entries[i]) {
            Some(e) if e.read == ReadState::Unread => {
                e.read = ReadState::Seen;
                true
            }
            _ => false,
        }
    }

    #[cfg(test)] // the card (and its dismiss button) is gone; the toast is dismissed in the Action Center
    pub fn dismiss(&mut self, id: &str) -> bool {
        match self.index(id).map(|i| &mut self.entries[i]) {
            Some(e) if e.read != ReadState::Dismissed => {
                e.read = ReadState::Dismissed;
                true
            }
            _ => false,
        }
    }

    /// OR-s delivery flags in; a flag never goes back to false.
    pub fn mark_delivered(&mut self, id: &str, spoken: bool, carded: bool, history: bool) {
        if let Some(i) = self.index(id) {
            let d = &mut self.entries[i].delivery;
            d.spoken |= spoken;
            d.carded |= carded;
            d.history |= history;
        }
    }

    /// The most recently updated event (what `nexus notify open` opens), if any.
    pub fn latest_id(&self) -> Option<&str> {
        self.entries.iter().max_by_key(|e| e.updated_at_ms).map(|e| e.event_id.as_str())
    }

    pub fn get(&self, id: &str) -> Option<&NotifyEvent> {
        self.index(id).map(|i| &self.entries[i])
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Opaque id -> the stored, validated target. Unknown / expired / disconnected are explicit errors;
    /// there is no fallback of any kind (no default repo, no other account, no newest item).
    pub fn resolve(&self, id: &str, now_ms: u64, connected: &dyn Fn(Provider, &str) -> bool) -> Result<Target, EventError> {
        let ev = self.get(id).ok_or(EventError::NotFound)?;
        if now_ms >= ev.expires_at_ms {
            return Err(EventError::Expired);
        }
        if !connected(ev.provider, &ev.account_id) {
            return Err(EventError::AccountDisconnected);
        }
        validate_target(&ev.target)?;
        Ok(ev.target.clone())
    }

    pub fn prune(&mut self, now_ms: u64) {
        self.entries.retain(|e| e.expires_at_ms > now_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(owner: &str, repo: &str) -> Target {
        Target::GitHub { host: "github.com".into(), owner: owner.into(), repo: repo.into(), pr: Some(7), run_id: Some(42), sha: Some("abc123".into()) }
    }

    fn ev(provider: Provider, account: &str, target: Target, kind: &str, status: Status, source_ref: &str, at: u64) -> NotifyEvent {
        let key = entity_key(provider, account, &target, kind);
        NotifyEvent {
            schema_version: SCHEMA_VERSION,
            event_id: event_id(&key, source_ref, status),
            entity_key: key,
            provider,
            account_id: account.into(),
            kind: kind.into(),
            status,
            severity: Severity::High,
            occurred_at_ms: at,
            observed_at_ms: at,
            updated_at_ms: at,
            title: "Checks failed".into(),
            preview: "3 tests failed".into(),
            privacy: PrivacyClass::Public,
            target,
            source_ref: source_ref.into(),
            original_source_ref: source_ref.into(),
            occurrences: 1,
            delivery: Delivery::default(),
            read: ReadState::Unread,
            expires_at_ms: 1_000_000,
        }
    }

    fn failed(account: &str, target: Target, src: &str, at: u64) -> NotifyEvent {
        ev(Provider::GitHub, account, target, "pr", Status::Failed, src, at)
    }

    #[test]
    fn same_repo_name_under_another_owner_or_account_never_collides() {
        let a = failed("acct-a", gh("alice", "api"), "r1", 1);
        let b = failed("acct-a", gh("bob", "api"), "r1", 1);
        let c = failed("acct-b", gh("alice", "api"), "r1", 1);
        assert_ne!(a.entity_key, b.entity_key);
        assert_ne!(a.entity_key, c.entity_key);
        assert_ne!(a.event_id, b.event_id);
        let mut led = Ledger::new();
        for e in [a.clone(), b.clone(), c.clone()] {
            assert!(matches!(led.record(e), Ok(Recorded::Created(_))));
        }
        // Each click resolves to its own owner, never the other one.
        let any = |_: Provider, _: &str| true;
        assert_eq!(led.resolve(&a.event_id, 5, &any), Ok(gh("alice", "api")));
        assert_eq!(led.resolve(&b.event_id, 5, &any), Ok(gh("bob", "api")));
    }

    #[test]
    fn unsafe_targets_are_rejected() {
        for (t, field) in [
            (gh("alice", "../x"), "repo"),
            (gh("a b", "r"), "owner"),
            (gh("alice", "https://x"), "repo"),
            (gh("alice", "a/b"), "repo"),
            (gh("", "r"), "owner"),
        ] {
            assert_eq!(validate_target(&t), Err(EventError::InvalidField(field)), "{t:?}");
        }
        assert_eq!(validate_target(&Target::Gmail { thread_id: String::new(), message_id: None }), Err(EventError::InvalidField("thread_id")));
        assert_eq!(
            validate_target(&Target::Deployment { provider: Provider::GitHub, team: "t".into(), project: "p".into(), deployment_id: "d".into(), revision: None }),
            Err(EventError::InvalidField("provider"))
        );
        assert!(validate_target(&gh("a-l_i.c.e", "repo-1.2")).is_ok());
    }

    #[test]
    fn a_forged_id_a_wrong_key_or_a_foreign_target_is_refused() {
        let mut led = Ledger::new();
        let mut forged = failed("a", gh("o", "r"), "r1", 1);
        forged.event_id = "ev_000000000000000000000000".into();
        assert_eq!(led.record(forged), Err(EventError::InvalidField("event_id")));
        let mut wrong_key = failed("a", gh("o", "r"), "r1", 1);
        wrong_key.entity_key = entity_key(Provider::GitHub, "a", &gh("o", "other"), "pr");
        assert_eq!(led.record(wrong_key), Err(EventError::InvalidField("entity_key")));
        // A Gmail provider cannot carry a GitHub target.
        let mut foreign = failed("a", gh("o", "r"), "r1", 1);
        foreign.provider = Provider::Gmail;
        assert_eq!(led.record(foreign), Err(EventError::InvalidField("provider")));
        assert_eq!(led.len(), 0);
    }

    #[test]
    fn a_duplicate_observation_neither_recreates_nor_resets_delivery() {
        let mut led = Ledger::new();
        let e = failed("a", gh("o", "r"), "r1", 1);
        let id = e.event_id.clone();
        assert_eq!(led.record(e.clone()), Ok(Recorded::Created(id.clone())));
        led.mark_delivered(&id, true, true, true);
        assert_eq!(led.record(e), Ok(Recorded::Duplicate(id.clone())));
        let d = led.get(&id).unwrap().delivery;
        assert!(d.spoken && d.carded && d.history);
        assert_eq!(led.len(), 1);
    }

    #[test]
    fn a_new_run_of_a_failing_entity_is_grouped_not_redelivered() {
        let mut led = Ledger::new();
        let first = failed("a", gh("o", "r"), "r1", 1);
        let id = first.event_id.clone();
        led.record(first).unwrap();
        led.mark_delivered(&id, true, true, false);
        assert_eq!(led.record(failed("a", gh("o", "r"), "r2", 2)), Ok(Recorded::Grouped(id.clone())));
        let e = led.get(&id).unwrap();
        assert_eq!((e.occurrences, e.source_ref.as_str(), e.original_source_ref.as_str()), (2, "r2", "r1"));
        assert!(e.delivery.spoken && e.delivery.carded);
        assert_eq!(led.len(), 1);
        let mut newer = failed("a", gh("o", "r"), "r3", 3);
        newer.target = Target::GitHub { host: "github.com".into(), owner: "o".into(), repo: "r".into(), pr: Some(7), run_id: Some(777), sha: None };
        led.record(newer).unwrap();
        assert!(matches!(led.get(&id).unwrap().target, Target::GitHub { run_id: Some(777), .. }), "grouped repeat links the newest run");
    }

    #[test]
    fn recovery_updates_the_original_failure_and_keeps_its_context() {
        let mut led = Ledger::new();
        let first = failed("a", gh("o", "r"), "r1", 1);
        let id = first.event_id.clone();
        led.record(first).unwrap();
        let mut fixed = ev(Provider::GitHub, "a", gh("o", "r"), "pr", Status::Recovered, "r2", 5);
        fixed.title = "All green".into();
        assert_eq!(led.record(fixed.clone()), Ok(Recorded::Recovered(id.clone())));
        let e = led.get(&id).unwrap();
        assert_eq!((e.status, e.source_ref.as_str(), e.original_source_ref.as_str(), e.updated_at_ms), (Status::Recovered, "r2", "r1", 5));
        assert_eq!(e.target, gh("o", "r"));
        assert_eq!(e.title, "All green", "recovery refreshes the wording shown in the detail");
        // The same recovery observed again must not create an extra event.
        assert!(matches!(led.record(fixed), Ok(Recorded::Duplicate(_))));
        assert_eq!(led.len(), 1);
    }

    #[test]
    fn a_recovery_with_no_known_failure_invents_no_failure() {
        let mut led = Ledger::new();
        let r = ev(Provider::GitHub, "a", gh("o", "r"), "pr", Status::Recovered, "r9", 3);
        let out = led.record(r).unwrap();
        assert!(matches!(out, Recorded::Created(_)));
        assert!(led.entries.iter().all(|e| e.status != Status::Failed));
        assert_eq!(led.entries[0].severity, Severity::Low);
    }

    #[test]
    fn repeating_an_unmatched_recovery_does_not_duplicate_it() {
        let mut led = Ledger::new();
        let r = ev(Provider::GitHub, "a", gh("o", "r"), "pr", Status::Recovered, "r9", 3);
        led.record(r.clone()).unwrap();
        assert!(matches!(led.record(r), Ok(Recorded::Duplicate(_))));
        assert_eq!(led.len(), 1);
    }

    #[test]
    fn a_new_failure_after_recovery_is_a_new_event() {
        let mut led = Ledger::new();
        let first = failed("a", gh("o", "r"), "r1", 1);
        let first_id = first.event_id.clone();
        led.record(first).unwrap();
        led.record(ev(Provider::GitHub, "a", gh("o", "r"), "pr", Status::Recovered, "r2", 2)).unwrap();
        match led.record(failed("a", gh("o", "r"), "r3", 3)).unwrap() {
            Recorded::Created(id) => assert_ne!(id, first_id),
            other => panic!("expected Created, got {other:?}"),
        }
        assert_eq!(led.len(), 2);
    }

    #[test]
    fn resolve_is_explicit_about_every_failure_and_never_falls_back() {
        let mut led = Ledger::new();
        let e = failed("acct-a", gh("o", "r"), "r1", 1);
        let id = e.event_id.clone();
        led.record(e).unwrap();
        led.record(failed("acct-a", gh("o", "newer"), "r1", 99)).unwrap(); // a newer unrelated PR exists
        let ok = |p: Provider, a: &str| p == Provider::GitHub && a == "acct-a";
        assert_eq!(led.resolve(&id, 5, &ok), Ok(gh("o", "r")));
        assert_eq!(led.resolve(&id, 1_000_000, &ok), Err(EventError::Expired));
        assert_eq!(led.resolve(&id, 5, &|_, _| false), Err(EventError::AccountDisconnected));
        assert_eq!(led.resolve("ev_unknown", 5, &ok), Err(EventError::NotFound));
        assert_eq!(led.resolve("", 5, &ok), Err(EventError::NotFound));
    }

    #[test]
    fn viewing_changes_only_the_nexus_read_flag() {
        let mut led = Ledger::new();
        let e = failed("a", gh("o", "r"), "r1", 1);
        let id = e.event_id.clone();
        led.record(e).unwrap();
        let before = led.get(&id).unwrap().clone();
        assert!(led.mark_seen(&id));
        assert!(!led.mark_seen(&id), "Seen is not set twice");
        let after = led.get(&id).unwrap();
        assert_eq!(after.read, ReadState::Seen);
        assert_eq!((&after.target, after.status, after.delivery, &after.source_ref), (&before.target, before.status, before.delivery, &before.source_ref));
        assert!(led.dismiss(&id));
        assert!(!led.mark_seen(&id), "a dismissed alert does not come back to Seen");
    }

    #[test]
    fn redacted_events_keep_no_preview_and_text_is_sanitised() {
        let mut led = Ledger::new();
        let mut e = ev(Provider::Gmail, "me@x.com", Target::Gmail { thread_id: "t1".into(), message_id: None }, "mail", Status::Info, "m1", 1);
        e.privacy = PrivacyClass::Redacted;
        e.preview = "secret body".into();
        e.title = "Hi\u{0}\u{7}  there".into();
        let id = e.event_id.clone();
        led.record(e).unwrap();
        assert_eq!(led.get(&id).unwrap().preview, "");
        assert_eq!(led.get(&id).unwrap().title, "Hi there");
    }

    #[test]
    fn parse_is_strict_about_schema_unknown_fields_and_bounds() {
        let good = failed("a", gh("o", "r"), "r1", 1);
        assert_eq!(parse_event(&serde_json::to_string(&good).unwrap()).unwrap(), good);
        let mut v2 = good.clone();
        v2.schema_version = 2;
        assert_eq!(parse_event(&serde_json::to_string(&v2).unwrap()), Err(EventError::UnsupportedSchema(2)));
        let mut long = good.clone();
        long.title = "x".repeat(500);
        assert_eq!(parse_event(&serde_json::to_string(&long).unwrap()), Err(EventError::InvalidField("title")));
        let mut json: serde_json::Value = serde_json::to_value(&good).unwrap();
        json["openUrl"] = serde_json::json!("https://evil.example");
        assert_eq!(parse_event(&json.to_string()), Err(EventError::Malformed));
        let mut json: serde_json::Value = serde_json::to_value(&good).unwrap();
        json["target"]["type"] = serde_json::json!("Shell");
        assert_eq!(parse_event(&json.to_string()), Err(EventError::Malformed));
        assert_eq!(parse_event("not json"), Err(EventError::Malformed));
    }

    #[test]
    fn the_ledger_is_bounded_and_keeps_the_newest() {
        let mut led = Ledger::new();
        for i in 0..210u64 {
            let mut e = failed("a", gh("o", &format!("r{i}")), "r1", i * 10 + 10);
            e.expires_at_ms = u64::MAX;
            led.record(e).unwrap();
        }
        assert_eq!(led.len(), 200);
        assert!(led.entries.iter().all(|e| e.updated_at_ms >= 110));
        led.prune(u64::MAX - 1);
        assert_eq!(led.len(), 200, "unexpired events survive a prune");
    }

    #[test]
    fn sanitize_strips_controls_and_truncates_by_chars() {
        assert_eq!(sanitize_text("a\u{0}b\u{7}c   d", 100), "abc d");
        assert_eq!(sanitize_text("é中xyz", 2).chars().count(), 2);
        assert_eq!(sanitize_text("  lead", 10), "lead");
    }
}
