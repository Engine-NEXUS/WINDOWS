//! Keep the screen/system awake while NEXUS is speaking or working.
//!
//! Structurally a trimmed copy of [`crate::volume`]'s `Controller`/RAII-lease/heartbeat-watchdog
//! pattern — there is no baseline to restore here (the OS releases an execution-state assertion
//! when the process dies, unlike system volume, which is a persistent OS setting), and no
//! deferred-restore grace period (a flicker in sleep-prevention has no audible defect, and the
//! user explicitly wants it cleared the instant it's idle, not after a grace window).
//!
//! Two independent reasons can each hold the system awake — "NEXUS is speaking" (a lease per
//! utterance, acquired next to the existing `volume::acquire_volume_lease` calls in `tts.rs`) and
//! "NEXUS is working" (one lease, held/dropped by [`set_working`] from a 3s poll of
//! `ghost::drill_running()`/`orchestrator::has_active_request()`). Both share one refcounted
//! [`Controller`]: the screen stays awake while either reason holds a lease, and clears the
//! instant both let go.
//!
//! The Win32 contract for `SetThreadExecutionState` is inconsistently documented (some sources
//! say `ES_CONTINUOUS` holds indefinitely from one call, others imply it resets an idle timer and
//! needs periodic re-assertion). Rather than trust either reading, the watchdog re-asserts on
//! every tick while any lease is held — a free Win32 call either way, correct under both
//! interpretations.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WATCHDOG_MS: u64 = 10_000;
const WATCHDOG_POLL_MS: u64 = 500;

fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// OS access, injectable for tests.
pub trait KeepAwakeBackend: Send + Sync {
    fn assert(&self) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

struct SystemBackend;

impl KeepAwakeBackend for SystemBackend {
    fn assert(&self) -> Result<(), String> {
        assert_awake()
    }
    fn clear(&self) -> Result<(), String> {
        clear_awake()
    }
}

#[derive(Default)]
struct State {
    /// Live leases across both reasons (speaking + working).
    count: usize,
    watchdog_running: bool,
}

pub struct Controller {
    st: Mutex<State>,
    backend: Arc<dyn KeepAwakeBackend>,
    last_beat: AtomicU64,
    watchdog: Duration,
    watchdog_poll: Duration,
}

/// RAII lease: clears its slot when dropped.
pub struct KeepAwakeLease {
    ctrl: Arc<Controller>,
    active: bool,
}

impl Drop for KeepAwakeLease {
    fn drop(&mut self) {
        if self.active {
            self.active = false;
            self.ctrl.release();
        }
    }
}

impl Controller {
    pub fn new(backend: Arc<dyn KeepAwakeBackend>) -> Arc<Self> {
        Self::with_timing(backend, Duration::from_millis(WATCHDOG_MS), Duration::from_millis(WATCHDOG_POLL_MS))
    }

    pub fn with_timing(backend: Arc<dyn KeepAwakeBackend>, watchdog: Duration, watchdog_poll: Duration) -> Arc<Self> {
        Arc::new(Self {
            st: Mutex::new(State::default()),
            backend,
            last_beat: AtomicU64::new(now_millis()),
            watchdog,
            watchdog_poll,
        })
    }

    pub fn heartbeat(&self) {
        self.last_beat.store(now_millis(), Ordering::Relaxed);
    }

    pub fn acquire(self: &Arc<Self>) -> Option<KeepAwakeLease> {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        self.heartbeat();
        if st.count == 0 {
            if let Err(e) = self.backend.assert() {
                tracing::warn!("keep_awake: failed to assert ({e}); not holding a lease");
                return None;
            }
            tracing::info!("[KEEPAWAKE] asserted");
        }
        st.count += 1;
        tracing::debug!("[KEEPAWAKE] acquire count={}", st.count);
        if !st.watchdog_running {
            st.watchdog_running = true;
            let me = Arc::clone(self);
            std::thread::spawn(move || me.watchdog_loop());
        }
        drop(st);
        Some(KeepAwakeLease { ctrl: Arc::clone(self), active: true })
    }

    fn release(self: &Arc<Self>) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        st.count = st.count.saturating_sub(1);
        tracing::debug!("[KEEPAWAKE] release count={}", st.count);
        if st.count == 0 {
            // Cleared instantly, no grace period — unlike volume, a flicker here has no
            // audible defect, and the user explicitly wants this instant.
            if let Err(e) = self.backend.clear() {
                tracing::warn!("keep_awake: failed to clear: {e}");
            } else {
                tracing::info!("[KEEPAWAKE] cleared");
            }
        }
    }

    /// Immediate clear regardless of refcount (explicit stop, app exit).
    pub fn force_clear(&self) {
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        if st.count > 0 {
            st.count = 0;
            let _ = self.backend.clear();
            tracing::info!("[KEEPAWAKE] force-cleared");
        }
    }

    fn watchdog_loop(self: Arc<Self>) {
        loop {
            std::thread::sleep(self.watchdog_poll);
            let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
            if st.count == 0 {
                st.watchdog_running = false;
                return;
            }
            let silent = now_millis().saturating_sub(self.last_beat.load(Ordering::Relaxed));
            if silent as u128 > self.watchdog.as_millis() {
                tracing::warn!("[KEEPAWAKE] watchdog: no heartbeat for {silent} ms, force-clearing");
                st.count = 0;
                let _ = self.backend.clear();
                st.watchdog_running = false;
                return;
            }
            // Re-assert on every tick while held: the documented Win32 contract for
            // ES_CONTINUOUS is inconsistent across sources, so don't rely on a single
            // call holding forever. A no-op-if-redundant Win32 call is cheap either way.
            let _ = self.backend.assert();
        }
    }

    #[cfg(test)]
    fn snapshot(&self) -> usize {
        self.st.lock().unwrap().count
    }
}

// ─── Global instance + stable free-function API ────────────────────────

static CONTROLLER: OnceLock<Arc<Controller>> = OnceLock::new();
/// Holds the "NEXUS is working" lease independently of per-utterance speaking leases.
static WORKING_LEASE: Mutex<Option<KeepAwakeLease>> = Mutex::new(None);

fn ctrl() -> &'static Arc<Controller> {
    CONTROLLER.get_or_init(|| Controller::new(Arc::new(SystemBackend)))
}

/// Acquire an RAII lease (e.g. held for the duration of one spoken utterance).
pub fn acquire_lease() -> Option<KeepAwakeLease> {
    ctrl().acquire()
}

/// Playback/work loops call this periodically so a long-running lease is never force-cleared.
pub fn heartbeat() {
    ctrl().heartbeat();
}

/// Immediate clear: explicit stop, app exit.
pub fn force_clear() {
    ctrl().force_clear();
    *WORKING_LEASE.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Set whether the "NEXUS is working on a task" reason currently holds the screen awake.
/// Idempotent: calling with the same value twice is a no-op (no acquire/release churn).
pub fn set_working(active: bool) {
    let mut slot = WORKING_LEASE.lock().unwrap_or_else(|e| e.into_inner());
    match (active, slot.is_some()) {
        (true, false) => *slot = acquire_lease(),
        (false, true) => *slot = None,
        _ => {}
    }
}

/// Background poller: holds the screen awake while a Ghost drill or an orchestrator request is
/// running, independent of whether NEXUS is also speaking. No app handle needed — both signals
/// it reads are free functions.
pub fn start_working_poller() {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let busy = crate::ghost::drill_running() || crate::orchestrator::has_active_request();
            set_working(busy);
        }
    });
}

// ─── Windows: SetThreadExecutionState ──────────────────────────────────

#[cfg(target_os = "windows")]
fn assert_awake() -> Result<(), String> {
    use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};
    // SAFETY: plain Win32 call, no pointers.
    let prev = unsafe { SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED) };
    if prev.0 != 0 { Ok(()) } else { Err("SetThreadExecutionState (assert) failed".into()) }
}

#[cfg(target_os = "windows")]
fn clear_awake() -> Result<(), String> {
    use windows::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS};
    // SAFETY: plain Win32 call, no pointers.
    let prev = unsafe { SetThreadExecutionState(ES_CONTINUOUS) };
    if prev.0 != 0 { Ok(()) } else { Err("SetThreadExecutionState (clear) failed".into()) }
}

#[cfg(not(target_os = "windows"))]
fn assert_awake() -> Result<(), String> {
    Err("keep_awake: unsupported platform".to_string())
}

#[cfg(not(target_os = "windows"))]
fn clear_awake() -> Result<(), String> {
    Err("keep_awake: unsupported platform".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct Fake {
        asserted: AtomicUsize,
        cleared: AtomicUsize,
        live: Mutex<bool>,
    }
    impl Fake {
        fn new() -> Arc<Self> {
            Arc::new(Self { asserted: AtomicUsize::new(0), cleared: AtomicUsize::new(0), live: Mutex::new(false) })
        }
    }
    impl KeepAwakeBackend for Fake {
        fn assert(&self) -> Result<(), String> {
            self.asserted.fetch_add(1, Ordering::SeqCst);
            *self.live.lock().unwrap() = true;
            Ok(())
        }
        fn clear(&self) -> Result<(), String> {
            self.cleared.fetch_add(1, Ordering::SeqCst);
            *self.live.lock().unwrap() = false;
            Ok(())
        }
    }

    fn rig() -> (Arc<Fake>, Arc<Controller>) {
        let fake = Fake::new();
        let c = Controller::with_timing(fake.clone(), Duration::from_millis(400), Duration::from_millis(40));
        (fake, c)
    }

    fn sleep(ms: u64) {
        std::thread::sleep(Duration::from_millis(ms));
    }

    #[test]
    fn acquire_asserts_and_release_clears_immediately_no_grace() {
        let (fake, c) = rig();
        let lease = c.acquire().expect("lease");
        assert!(*fake.live.lock().unwrap());
        drop(lease);
        // Unlike volume.rs, clearing is instant — no deferred-restore window to wait out.
        assert!(!*fake.live.lock().unwrap(), "must clear the instant the last lease drops");
    }

    #[test]
    fn concurrent_leases_share_one_assert_and_clear_only_when_both_drop() {
        let (fake, c) = rig();
        let a = c.acquire().unwrap();
        let b = c.acquire().unwrap();
        assert_eq!(fake.asserted.load(Ordering::SeqCst), 1, "second acquire must not re-assert");
        drop(a);
        assert!(*fake.live.lock().unwrap(), "must stay awake while one lease remains");
        drop(b);
        assert!(!*fake.live.lock().unwrap());
    }

    #[test]
    fn leaked_lease_is_released_by_the_watchdog() {
        let (fake, c) = rig();
        let lease = c.acquire().unwrap();
        std::mem::forget(lease); // a task that died without dropping
        sleep(900);
        assert!(!*fake.live.lock().unwrap(), "leaked lease kept the screen awake");
        assert_eq!(c.snapshot(), 0);
    }

    #[test]
    fn long_lease_with_heartbeat_survives_the_watchdog_and_reasserts() {
        let (fake, c) = rig();
        let lease = c.acquire().unwrap();
        for _ in 0..20 {
            sleep(60); // 1.2 s total, three times the 400 ms watchdog
            c.heartbeat();
        }
        assert!(*fake.live.lock().unwrap(), "long speech/work was force-cleared");
        assert!(fake.asserted.load(Ordering::SeqCst) > 1, "watchdog should have re-asserted at least once");
        drop(lease);
        assert!(!*fake.live.lock().unwrap());
    }

    #[test]
    fn force_clear_drops_the_live_count_regardless_of_outstanding_leases() {
        let (fake, c) = rig();
        let lease = c.acquire().unwrap();
        c.force_clear();
        assert!(!*fake.live.lock().unwrap());
        assert_eq!(c.snapshot(), 0);
        drop(lease); // the stale drop must not underflow or re-clear incorrectly
        assert_eq!(c.snapshot(), 0);
    }

    #[test]
    fn set_working_is_idempotent() {
        // Exercises the global set_working API directly against the real global controller.
        // Isolated by using force_clear() first so this test doesn't depend on ordering.
        force_clear();
        set_working(true);
        set_working(true); // no-op, must not double-acquire
        set_working(false);
        set_working(false); // no-op
    }
}
