//! Click-to-context transaction for a notification (notification-to-sidebar N2). Pure, no Tauri, no I/O.
//!
//! A click carries only an opaque event id. `Opener::begin` resolves it through the validated ledger and starts ONE
//! opening transaction identified by a generation token. A later click supersedes an earlier one; data or a cold-mount
//! fetch carrying an older token is refused, so late results for A can never replace B. An unresolvable target gives
//! a read-only error payload, never another account, a default repo or the newest PR. Opening is read-only: nothing
//! here can merge, reply, rerun or mark anything read at the provider. A pinned/active sidebar view is remembered and
//! returned on close.

use super::notify_event::{EventError, Ledger, NotifyEvent, Provider, Status, Target};

/// What the sidebar renders for an opened alert. Built only from validated ledger state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventDetail {
    pub token: u64,
    pub event_id: String,
    pub provider: Provider,
    /// What is tracked: "pr" | "build" | "deploy" | "mail".
    pub kind: String,
    pub status: Status,
    pub title: String,
    pub preview: String,
    pub target: Target,
    pub original_source_ref: String,
    pub source_ref: String,
    pub occurrences: u32,
    /// Allowlisted provider page derived from the typed target (never from notification text).
    pub provider_url: Option<String>,
}

/// Read-only error view shown instead of any guessed content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventUnavailable {
    pub token: u64,
    pub event_id: String,
    pub reason: EventError,
    pub message: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Opened {
    Detail(Box<EventDetail>),
    Unavailable(EventUnavailable),
}

impl Opened {
    pub fn token(&self) -> u64 {
        match self {
            Opened::Detail(d) => d.token,
            Opened::Unavailable(u) => u.token,
        }
    }
}

fn message_for(e: &EventError) -> &'static str {
    match e {
        EventError::Expired => "This alert has expired.",
        EventError::AccountDisconnected => "The account for this alert is not connected.",
        EventError::NotFound => "This alert is no longer available.",
        _ => "This alert cannot be opened.",
    }
}

/// Provider page for a typed target. GitHub only: Gmail needs the signed-in mailbox index and deployments need the
/// provider's dashboard path, neither of which is known safely here, so they get none rather than a guess.
pub fn provider_url(t: &Target) -> Option<String> {
    match t {
        Target::GitHub { host, owner, repo, pr, run_id, .. } if host == "github.com" => {
            let base = format!("https://github.com/{owner}/{repo}");
            Some(match (pr, run_id) {
                (Some(n), _) => format!("{base}/pull/{n}"),
                (None, Some(r)) => format!("{base}/actions/runs/{r}"),
                _ => base,
            })
        }
        Target::Repo { host, owner, repo } if host == "github.com" => Some(format!("https://github.com/{owner}/{repo}")),
        _ => None,
    }
}

fn detail(token: u64, ev: &NotifyEvent) -> EventDetail {
    EventDetail {
        token,
        event_id: ev.event_id.clone(),
        provider: ev.provider,
        kind: ev.kind.clone(),
        status: ev.status,
        title: ev.title.clone(),
        preview: ev.preview.clone(),
        target: ev.target.clone(),
        original_source_ref: ev.original_source_ref.clone(),
        source_ref: ev.source_ref.clone(),
        occurrences: ev.occurrences,
        provider_url: provider_url(&ev.target),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Accept {
    Accepted,
    /// Older generation, other event, or no open transaction: ignored.
    Stale,
}

/// One opening transaction at a time.
#[derive(Default)]
pub struct Opener {
    generation: u64,
    current: Option<Current>,
}

struct Current {
    token: u64,
    #[cfg(test)]
    event_id: String,
    /// The view that was showing (and pinned) before the alert opened, restored on close.
    restore_view: Option<String>,
    #[cfg(test)]
    pending: Option<Opened>,
    shown: bool,
}

impl Opener {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts opening `event_id`, superseding any earlier transaction. `current_view` is the sidebar view now showing;
    /// when it is `pinned`, it is remembered for `close`. The payload is stored as pending (cold-mount path) and also
    /// returned (warm path); both carry the same token.
    pub fn begin(
        &mut self,
        event_id: &str,
        ledger: &mut Ledger,
        now_ms: u64,
        connected: &dyn Fn(Provider, &str) -> bool,
        current_view: Option<&str>,
        pinned: bool,
    ) -> Opened {
        self.generation += 1;
        let token = self.generation;
        let opened = match ledger.resolve(event_id, now_ms, connected) {
            Ok(_) => match ledger.get(event_id).map(|ev| detail(token, ev)) {
                Some(d) => {
                    ledger.mark_seen(event_id);
                    Opened::Detail(Box::new(d))
                }
                None => Opened::Unavailable(EventUnavailable { token, event_id: event_id.to_string(), reason: EventError::NotFound, message: message_for(&EventError::NotFound) }),
            },
            Err(reason) => Opened::Unavailable(EventUnavailable { token, event_id: event_id.to_string(), message: message_for(&reason), reason }),
        };
        // A second click while the first detail is still showing keeps the ORIGINAL view to restore, not the first
        // alert. A transaction whose sidebar was closed (no view, or another view) is dead: never inherit from it.
        let showing_detail = matches!(current_view, Some("assistant" | "coderabbit"));
        let restore_view = match self.current.take() {
            Some(prev) if showing_detail && prev.restore_view.is_some() => prev.restore_view,
            _ => current_view.filter(|_| pinned).map(str::to_string),
        };
        self.current = Some(Current {
            token,
            #[cfg(test)]
            event_id: event_id.to_string(),
            restore_view,
            #[cfg(test)]
            pending: Some(opened.clone()),
            shown: false,
        });
        opened
    }

#[cfg(test)] // ready/late-data handshake: tested here, wired in N4 (dead-code ratchet)
    /// Cold-mount fetch: returns the pending payload only for the matching token, once.
    pub fn take_pending(&mut self, token: u64) -> Option<Opened> {
        let cur = self.current.as_mut()?;
        if cur.token != token {
            return None;
        }
        cur.pending.take()
    }

    /// Marks the content shell as shown, only for the current transaction.
    pub fn ready(&mut self, token: u64) -> Accept {
        match self.current.as_mut() {
            Some(c) if c.token == token => {
                c.shown = true;
                Accept::Accepted
            }
            _ => Accept::Stale,
        }
    }

#[cfg(test)] // ready/late-data handshake: tested here, wired in N4 (dead-code ratchet)
    #[cfg(test)] // late refreshed data: no producer yet
    /// Late refreshed data (e.g. current status) is accepted only for the live transaction and event.
    pub fn accept_update(&self, token: u64, event_id: &str) -> Accept {
        match &self.current {
            Some(c) if c.token == token && c.event_id == event_id => Accept::Accepted,
            _ => Accept::Stale,
        }
    }

    /// Closes the transaction (Escape/dismiss/cancel). Returns the pinned view to restore, if any. A stale token
    /// does nothing, so a late close of A cannot tear down B.
    pub fn close(&mut self, token: u64) -> Option<String> {
        match &self.current {
            Some(c) if c.token == token => self.current.take().and_then(|c| c.restore_view),
            _ => None,
        }
    }

    /// The pinned view that `close` will restore for the live transaction.
    pub fn restore_view(&self) -> Option<&str> {
        self.current.as_ref().and_then(|c| c.restore_view.as_deref())
    }

    #[cfg(test)]
    pub fn is_open(&self) -> bool {
        self.current.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::super::notify_event::*;
    use super::*;

    fn gh(owner: &str, repo: &str, pr: u64) -> Target {
        Target::GitHub { host: "github.com".into(), owner: owner.into(), repo: repo.into(), pr: Some(pr), run_id: Some(9), sha: None }
    }

    fn failed(account: &str, t: Target, src: &str) -> NotifyEvent {
        let key = entity_key(Provider::GitHub, account, &t, "pr");
        NotifyEvent {
            schema_version: SCHEMA_VERSION,
            event_id: event_id(&key, src, Status::Failed),
            entity_key: key,
            provider: Provider::GitHub,
            account_id: account.into(),
            kind: "pr".into(),
            status: Status::Failed,
            severity: Severity::High,
            occurred_at_ms: 1,
            observed_at_ms: 1,
            updated_at_ms: 1,
            title: "Checks failed".into(),
            preview: "3 tests failed".into(),
            privacy: PrivacyClass::Public,
            target: t,
            source_ref: src.into(),
            original_source_ref: src.into(),
            occurrences: 1,
            delivery: Delivery::default(),
            read: ReadState::Unread,
            expires_at_ms: 1_000,
        }
    }

    fn ledger_with(evs: Vec<NotifyEvent>) -> (Ledger, Vec<String>) {
        let mut l = Ledger::new();
        let ids = evs.iter().map(|e| e.event_id.clone()).collect();
        for e in evs {
            l.record(e).unwrap();
        }
        (l, ids)
    }

    const ALL: &dyn Fn(Provider, &str) -> bool = &|_, _| true;

    #[test]
    fn a_click_opens_the_exact_target_not_a_same_named_or_newer_one() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("alice", "api", 1), "r1"), failed("a", gh("bob", "api", 2), "r1"), failed("a", gh("alice", "newest", 3), "r1")]);
        let mut o = Opener::new();
        match o.begin(&ids[1], &mut l, 5, ALL, None, false) {
            Opened::Detail(d) => {
                assert_eq!(d.target, gh("bob", "api", 2));
                assert_eq!(d.provider_url.as_deref(), Some("https://github.com/bob/api/pull/2"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn unresolvable_targets_give_a_read_only_error_and_no_fallback_content() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "r", 1), "r1"), failed("a", gh("o", "newer", 2), "r1")]);
        let mut o = Opener::new();
        for (id, now, conn, want) in [
            (ids[0].as_str(), 5u64, true, None),
            ("ev_missing", 5, true, Some(EventError::NotFound)),
            (ids[0].as_str(), 5_000, true, Some(EventError::Expired)),
            (ids[0].as_str(), 5, false, Some(EventError::AccountDisconnected)),
        ] {
            let r = o.begin(id, &mut l, now, &move |_, _| conn, None, false);
            match (r, want) {
                (Opened::Detail(_), None) => {}
                (Opened::Unavailable(u), Some(w)) => {
                    assert_eq!(u.reason, w);
                    assert!(!u.message.is_empty());
                }
                (other, w) => panic!("{other:?} vs {w:?}"),
            }
        }
    }

    #[test]
    fn a_later_click_supersedes_and_late_data_for_the_first_is_refused() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1"), failed("a", gh("o", "B", 2), "r1")]);
        let mut o = Opener::new();
        let a = o.begin(&ids[0], &mut l, 5, ALL, None, false).token();
        let b = o.begin(&ids[1], &mut l, 5, ALL, None, false).token();
        assert!(b > a);
        assert_eq!(o.accept_update(a, &ids[0]), Accept::Stale, "late A refresh");
        assert_eq!(o.accept_update(b, &ids[1]), Accept::Accepted);
        assert_eq!(o.accept_update(b, &ids[0]), Accept::Stale, "token of B with event A");
        assert_eq!(o.ready(a), Accept::Stale);
        assert_eq!(o.ready(b), Accept::Accepted);
    }

    #[test]
    fn clicking_the_same_alert_twice_invalidates_the_first_token() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1")]);
        let mut o = Opener::new();
        let first = o.begin(&ids[0], &mut l, 5, ALL, None, false).token();
        let second = o.begin(&ids[0], &mut l, 5, ALL, None, false).token();
        assert_eq!(o.accept_update(first, &ids[0]), Accept::Stale, "same event id, old token");
        assert_eq!(o.accept_update(second, &ids[0]), Accept::Accepted);
    }

    #[test]
    fn the_cold_mount_fetch_gets_only_the_matching_payload_once() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1"), failed("a", gh("o", "B", 2), "r1")]);
        let mut o = Opener::new();
        let a = o.begin(&ids[0], &mut l, 5, ALL, None, false).token();
        let b = o.begin(&ids[1], &mut l, 5, ALL, None, false).token();
        assert_eq!(o.take_pending(a), None, "A's payload was superseded");
        match o.take_pending(b) {
            Some(Opened::Detail(d)) => assert_eq!(d.event_id, ids[1]),
            other => panic!("{other:?}"),
        }
        assert_eq!(o.take_pending(b), None, "consumed once");
    }

    #[test]
    fn a_pinned_view_is_restored_on_close_and_a_stale_close_does_nothing() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1"), failed("a", gh("o", "B", 2), "r1")]);
        let mut o = Opener::new();
        let a = o.begin(&ids[0], &mut l, 5, ALL, Some("pr-list"), true).token();
        // A second click keeps the ORIGINAL pinned view, not alert A.
        let b = o.begin(&ids[1], &mut l, 5, ALL, Some("assistant"), false).token();
        assert_eq!(o.close(a), None, "late close of A must not tear down B");
        assert!(o.is_open());
        assert_eq!(o.close(b), Some("pr-list".to_string()));
        assert!(!o.is_open());
        // Not pinned: nothing to restore.
        let c = o.begin(&ids[0], &mut l, 5, ALL, Some("assistant"), false).token();
        assert_eq!(o.close(c), None);
    }

    #[test]
    fn a_second_click_while_the_coderabbit_alert_view_shows_keeps_the_original_pinned_view() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1"), failed("a", gh("o", "B", 2), "r1")]);
        let mut o = Opener::new();
        o.begin(&ids[0], &mut l, 5, ALL, Some("settings"), true);
        // Alerts now show in the CodeRabbit view, so that is the view showing on the second click.
        let b = o.begin(&ids[1], &mut l, 5, ALL, Some("coderabbit"), false).token();
        assert_eq!(o.restore_view(), Some("settings"));
        assert_eq!(o.close(b), Some("settings".to_string()));
    }

    #[test]
    fn a_dead_transaction_does_not_leak_its_restore_view_into_a_later_alert() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "A", 1), "r1"), failed("a", gh("o", "B", 2), "r1")]);
        let mut o = Opener::new();
        o.begin(&ids[0], &mut l, 5, ALL, Some("settings"), true);
        // The sidebar was closed without the close call arriving: no view is showing now.
        let b = o.begin(&ids[1], &mut l, 5, ALL, None, false).token();
        assert_eq!(o.restore_view(), None, "must not jump back to a long-closed view");
        assert_eq!(o.close(b), None);
        // A different view showing also ends inheritance.
        o.begin(&ids[0], &mut l, 5, ALL, Some("settings"), true);
        o.begin(&ids[1], &mut l, 5, ALL, Some("pr-list"), true);
        assert_eq!(o.restore_view(), Some("pr-list"));
    }

    #[test]
    fn opening_is_read_only_it_marks_only_the_nexus_alert_seen() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "r", 1), "r1")]);
        let before = l.get(&ids[0]).unwrap().clone();
        Opener::new().begin(&ids[0], &mut l, 5, ALL, None, false);
        let after = l.get(&ids[0]).unwrap();
        assert_eq!(after.read, ReadState::Seen);
        assert_eq!((&after.target, after.status, after.delivery, &after.source_ref), (&before.target, before.status, before.delivery, &before.source_ref));
    }

    #[test]
    fn a_recovered_alert_keeps_its_original_context_when_opened() {
        let (mut l, ids) = ledger_with(vec![failed("a", gh("o", "r", 1), "r1")]);
        let t = gh("o", "r", 1);
        let key = entity_key(Provider::GitHub, "a", &t, "pr");
        let mut fixed = failed("a", t, "r2");
        fixed.status = Status::Recovered;
        fixed.event_id = event_id(&key, "r2", Status::Recovered);
        l.record(fixed).unwrap();
        match Opener::new().begin(&ids[0], &mut l, 5, ALL, None, false) {
            Opened::Detail(d) => assert_eq!((d.status, d.original_source_ref.as_str(), d.source_ref.as_str()), (Status::Recovered, "r1", "r2")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn provider_urls_come_only_from_the_typed_target() {
        assert_eq!(provider_url(&gh("o", "r", 4)).as_deref(), Some("https://github.com/o/r/pull/4"));
        let run_only = Target::GitHub { host: "github.com".into(), owner: "o".into(), repo: "r".into(), pr: None, run_id: Some(7), sha: None };
        assert_eq!(provider_url(&run_only).as_deref(), Some("https://github.com/o/r/actions/runs/7"));
        let other_host = Target::GitHub { host: "evil.example".into(), owner: "o".into(), repo: "r".into(), pr: Some(1), run_id: None, sha: None };
        assert_eq!(provider_url(&other_host), None);
        assert_eq!(provider_url(&Target::Gmail { thread_id: "t".into(), message_id: None }), None);
    }
}
