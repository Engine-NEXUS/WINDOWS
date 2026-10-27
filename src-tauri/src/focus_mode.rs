//! Focus Mode core: pauses non-essential background tasks while high-precision
//! user interactions (e.g. Hand Mode camera mouse) are running.
//!
//! Structurally modelled on [`crate::volume`] and [`crate::keep_awake`]:
//! - **Epoch leases.** `acquire` returns an epoch. Releases with an older or
//!   mismatched epoch are safely ignored (stale epoch release).
//! - **Idempotence.** Double acquire returns the current active epoch and counts
//!   once; release is idempotent.
//! - **Heartbeat watchdog.** Active controllers must receive regular heartbeats.
//!   If silent for `WATCHDOG_MS` (10s), the lease is force-released so leaked
//!   or hung foreground tasks cannot leave NEXUS permanently paused.
//! - **Deduplicated logging.** [`defer_log`] prints `[FOCUS] <name> deferred` at
//!   most once per focus session per task name.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// A lease with no heartbeat for this long is considered leaked and force-released.
pub const WATCHDOG_MS: u64 = 10_000;

#[derive(Default, Debug)]
struct State {
    epoch: u64,
    active: bool,
    reason: Option<String>,
    last_beat_ms: u64,
}

/// Pure state machine with an injected clock (`now_ms`).
#[derive(Default, Debug)]
pub struct Controller {
    st: Mutex<State>,
}

impl Controller {
    pub const fn new() -> Self {
        Self {
            st: Mutex::new(State {
                epoch: 0,
                active: false,
                reason: None,
                last_beat_ms: 0,
            }),
        }
    }

    /// Acquire focus mode. If already active, keeps the current epoch ("double acquire counts once").
    /// Idempotent. Returns the active epoch.
    pub fn acquire(&self, now_ms: u64, reason: &str) -> u64 {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        st.last_beat_ms = now_ms;
        if st.active {
            return st.epoch;
        }
        st.epoch = st.epoch.wrapping_add(1);
        st.active = true;
        st.reason = Some(reason.to_string());
        st.epoch
    }

    /// Release focus mode for the given epoch.
    /// A release with an older or mismatched epoch is safely ignored.
    /// Idempotent.
    pub fn release(&self, epoch: u64) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if !st.active || epoch != st.epoch {
            return;
        }
        st.active = false;
        st.reason = None;
    }

    /// Update the heartbeat timestamp while active.
    pub fn heartbeat(&self, now_ms: u64) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if st.active {
            st.last_beat_ms = now_ms;
        }
    }

    /// Watchdog tick: if a lease is held and no heartbeat for `WATCHDOG_MS` (10s),
    /// force release and return true.
    pub fn tick(&self, now_ms: u64) -> bool {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if st.active && now_ms.saturating_sub(st.last_beat_ms) >= WATCHDOG_MS {
            st.active = false;
            st.epoch = st.epoch.wrapping_add(1);
            st.reason = None;
            true
        } else {
            false
        }
    }

    /// Whether focus mode is currently active.
    pub fn active(&self) -> bool {
        self.st.lock().unwrap_or_else(|e| e.into_inner()).active
    }

    /// Force release focus mode (app exit, error, watchdog).
    pub fn force_release(&self, _reason: &str) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if st.active {
            st.active = false;
            st.epoch = st.epoch.wrapping_add(1);
            st.reason = None;
        }
    }
}

// ── Process-global glue ────────────────────────────────────────────────────────

static CONTROLLER: Controller = Controller::new();
static PAUSED: AtomicBool = AtomicBool::new(false);
static WATCHDOG_RUNNING: AtomicBool = AtomicBool::new(false);
static DEFERRED_NAMES: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);
static START: OnceLock<std::time::Instant> = OnceLock::new();

fn now_ms() -> u64 {
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

/// Acquire global focus mode. Returns epoch lease.
pub fn acquire(reason: &str) -> u64 {
    let now = now_ms();
    let epoch = CONTROLLER.acquire(now, reason);
    let was_active = PAUSED.swap(true, Ordering::SeqCst);
    if !was_active {
        if let Ok(mut g) = DEFERRED_NAMES.lock() {
            *g = Some(HashSet::new());
        }
    }
    spawn_watchdog_if_needed();
    epoch
}

/// Release global focus mode for the given epoch.
pub fn release(epoch: u64) {
    CONTROLLER.release(epoch);
    let active = CONTROLLER.active();
    PAUSED.store(active, Ordering::SeqCst);
    if !active {
        if let Ok(mut g) = DEFERRED_NAMES.lock() {
            *g = None;
        }
    }
}

/// Update global heartbeat.
pub fn heartbeat() {
    CONTROLLER.heartbeat(now_ms());
}

/// Cheap read: safe to call in hot loops.
#[inline]
pub fn paused() -> bool {
    PAUSED.load(Ordering::Relaxed)
}

/// Force release global focus mode on app exit or error.
pub fn force_release(reason: &str) {
    CONTROLLER.force_release(reason);
    PAUSED.store(false, Ordering::SeqCst);
    if let Ok(mut g) = DEFERRED_NAMES.lock() {
        *g = None;
    }
}

fn spawn_watchdog_if_needed() {
    if !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
        std::thread::Builder::new()
            .name("focus-mode-watchdog".into())
            .spawn(|| {
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if !CONTROLLER.active() {
                        WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
                        if CONTROLLER.active() && !WATCHDOG_RUNNING.swap(true, Ordering::SeqCst) {
                            continue;
                        }
                        break;
                    }
                    if CONTROLLER.tick(now_ms()) {
                        PAUSED.store(false, Ordering::SeqCst);
                        if let Ok(mut g) = DEFERRED_NAMES.lock() {
                            *g = None;
                        }
                        println!("[FOCUS] off");
                        WATCHDOG_RUNNING.store(false, Ordering::SeqCst);
                        break;
                    }
                }
            })
            .ok();
    }
}

/// Prints `[FOCUS] <name> deferred` at most once per focus session per name.
pub fn defer_log(name: &'static str) {
    if !paused() {
        return;
    }
    if let Ok(mut g) = DEFERRED_NAMES.lock() {
        let set = g.get_or_insert_with(HashSet::new);
        if set.insert(name) {
            println!("[FOCUS] {name} deferred");
        }
    }
}

/// Polls `paused()` every 2 s until focus mode ends (asynchronous).
pub async fn wait_until_clear(name: &'static str) {
    if !paused() {
        return;
    }
    defer_log(name);
    while paused() {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

/// Polls `paused()` every 2 s until focus mode ends (synchronous threads).
pub fn wait_until_clear_sync(name: &'static str) {
    if !paused() {
        return;
    }
    defer_log(name);
    while paused() {
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acquire_and_release() {
        let ctrl = Controller::new();
        let epoch = ctrl.acquire(100, "hand");
        assert_eq!(epoch, 1);
        assert!(ctrl.active());
        ctrl.release(epoch);
        assert!(!ctrl.active());
        // Idempotent: repeated release does not change state or panic
        ctrl.release(epoch);
        assert!(!ctrl.active());
    }

    #[test]
    fn double_acquire_counts_once() {
        let ctrl = Controller::new();
        let ep1 = ctrl.acquire(100, "hand");
        let ep2 = ctrl.acquire(200, "hand");
        assert_eq!(ep1, ep2);
        assert!(ctrl.active());
        ctrl.release(ep1);
        assert!(!ctrl.active());
    }

    #[test]
    fn stale_epoch_release_ignored() {
        let ctrl = Controller::new();
        let ep1 = ctrl.acquire(100, "hand");
        ctrl.release(ep1);
        assert!(!ctrl.active());

        let ep2 = ctrl.acquire(200, "hand");
        assert_ne!(ep1, ep2);
        assert!(ctrl.active());

        // Stale release from previous epoch must be ignored
        ctrl.release(ep1);
        assert!(ctrl.active());

        ctrl.release(ep2);
        assert!(!ctrl.active());
    }

    #[test]
    fn watchdog_releases_after_10s_without_heartbeat() {
        let ctrl = Controller::new();
        let ep = ctrl.acquire(1_000, "hand");
        assert!(ctrl.active());

        // 9.999 s elapsed: still alive
        assert!(!ctrl.tick(1_000 + WATCHDOG_MS - 1));
        assert!(ctrl.active());

        // 10.000 s elapsed: watchdog triggers force-release
        assert!(ctrl.tick(1_000 + WATCHDOG_MS));
        assert!(!ctrl.active());

        // The old lease epoch is now stale
        ctrl.release(ep);
        assert!(!ctrl.active());
    }

    #[test]
    fn heartbeat_keeps_it_alive() {
        let ctrl = Controller::new();
        let _ep = ctrl.acquire(1_000, "hand");
        assert!(ctrl.active());

        // Heartbeat at 8 s
        assert!(!ctrl.tick(9_000));
        ctrl.heartbeat(9_000);

        // At 17 s (8 s since last heartbeat): still alive
        assert!(!ctrl.tick(17_000));
        assert!(ctrl.active());

        // At 19.001 s (10.001 s since last heartbeat): watchdog triggers
        assert!(ctrl.tick(19_001));
        assert!(!ctrl.active());
    }

    #[test]
    fn force_release_clears_any_lease() {
        let ctrl = Controller::new();
        let ep = ctrl.acquire(1_000, "hand");
        assert!(ctrl.active());

        ctrl.force_release("app_exit");
        assert!(!ctrl.active());

        // Stale release after force_release
        ctrl.release(ep);
        assert!(!ctrl.active());
    }

    #[test]
    fn paused_mirror_matches() {
        force_release("test_reset");
        assert!(!paused());

        let ep = acquire("test");
        assert!(paused());

        release(ep);
        assert!(!paused());

        let _ep2 = acquire("test2");
        assert!(paused());

        force_release("test2_exit");
        assert!(!paused());
    }

    #[test]
    fn defer_log_at_most_once_per_session() {
        force_release("test_reset");
        defer_log("test_loop"); // no-op when paused is false

        let ep = acquire("test_session_1");
        assert!(paused());
        defer_log("task_a");
        defer_log("task_a"); // deduplicated
        defer_log("task_b");
        release(ep);

        // Next session allows task_a to log once again
        let ep2 = acquire("test_session_2");
        assert!(paused());
        defer_log("task_a");
        release(ep2);
    }
}
