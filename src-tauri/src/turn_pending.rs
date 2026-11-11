//! The gap between "the capture ended with speech" and "the orchestrator owns a request".
//!
//! In that gap (speech-to-text, then the hop through the stage window) nothing is capturing, nothing is
//! speaking and no request exists yet, so Ctrl+Space used to read NEXUS as idle: the press CALLED it again,
//! the old question was still answered aloud, and the new capture heard that answer and replied to it
//! (live log 2026-11-11 18:30:02). This module marks the gap so the press is a cancel, and remembers the
//! cancelled capture so its transcript is dropped when it arrives.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Longest time a heard turn may stay "pending" with no request installed. Cloud speech-to-text answers in
/// well under a second and the local engine in a few; after this the mark expires by itself so a turn that
/// was dropped on the way (ignored speech, an error) can never leave the hotkey stuck on "cancel".
const PENDING_FOR: Duration = Duration::from_secs(5);
/// How long a cancelled capture is remembered. Its transcript arrives within seconds or never.
const CANCEL_MEMORY: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Pending {
    pending: Option<(u64, Instant)>,
    cancelled: Option<(u64, Instant)>,
}

impl Pending {
    fn mark(&mut self, session: u64, now: Instant) {
        self.pending = Some((session, now + PENDING_FOR));
    }

    fn is_pending(&self, now: Instant) -> bool {
        matches!(self.pending, Some((_, until)) if now < until)
    }

    fn clear(&mut self) {
        self.pending = None;
    }

    /// The user cancelled: the pending capture (if any) must not be answered. Returns its session.
    fn cancel(&mut self, now: Instant) -> Option<u64> {
        let (session, until) = self.pending.take()?;
        if now >= until {
            return None;
        }
        self.cancelled = Some((session, now + CANCEL_MEMORY));
        Some(session)
    }

    /// True once for the capture the user cancelled.
    fn take_cancelled(&mut self, session: u64, now: Instant) -> bool {
        match self.cancelled {
            Some((s, until)) if s == session && now < until => {
                self.cancelled = None;
                true
            }
            _ => false,
        }
    }
}

static STATE: Mutex<Pending> = Mutex::new(Pending { pending: None, cancelled: None });

fn lock() -> std::sync::MutexGuard<'static, Pending> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// A capture ended with speech and is being transcribed.
pub(crate) fn mark(session: u64) {
    lock().mark(session, Instant::now());
}

/// A heard turn is on its way to the orchestrator.
pub(crate) fn is_pending() -> bool {
    lock().is_pending(Instant::now())
}

/// The turn arrived (a request now owns the pipeline) or produced no words.
pub(crate) fn clear() {
    lock().clear();
}

/// Ctrl+Space cancel: drop the turn that is on its way.
pub(crate) fn cancel() {
    if let Some(session) = lock().cancel(Instant::now()) {
        tracing::info!("[TURN] capture session {session} cancelled before it reached the orchestrator");
    }
}

/// Was this capture cancelled by the user while it was being transcribed?
pub(crate) fn was_cancelled(session: u64) -> bool {
    lock().take_cancelled(session, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_heard_turn_is_pending_until_it_arrives_or_expires() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        assert!(!p.is_pending(t0));
        p.mark(7, t0);
        assert!(p.is_pending(t0 + Duration::from_millis(300)));
        assert!(!p.is_pending(t0 + PENDING_FOR), "the mark must expire by itself");
        p.mark(8, t0);
        p.clear();
        assert!(!p.is_pending(t0));
    }

    #[test]
    fn a_cancel_drops_exactly_the_capture_that_was_on_its_way() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        p.mark(7, t0);
        assert_eq!(p.cancel(t0 + Duration::from_millis(100)), Some(7));
        assert!(!p.is_pending(t0 + Duration::from_millis(100)));
        assert!(!p.take_cancelled(8, t0 + Duration::from_secs(1)), "another capture is not affected");
        assert!(p.take_cancelled(7, t0 + Duration::from_secs(1)));
        assert!(!p.take_cancelled(7, t0 + Duration::from_secs(1)), "reported once");
    }

    #[test]
    fn a_cancel_with_nothing_pending_or_an_expired_mark_remembers_nothing() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        assert_eq!(p.cancel(t0), None);
        p.mark(7, t0);
        assert_eq!(p.cancel(t0 + PENDING_FOR), None);
        assert!(!p.take_cancelled(7, t0 + PENDING_FOR));
    }

    #[test]
    fn a_cancelled_capture_is_forgotten_after_its_memory_window() {
        let t0 = Instant::now();
        let mut p = Pending::default();
        p.mark(7, t0);
        p.cancel(t0);
        assert!(!p.take_cancelled(7, t0 + CANCEL_MEMORY));
    }
}
