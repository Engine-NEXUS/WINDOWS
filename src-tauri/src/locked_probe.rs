//! Locked-screen probe (plan 193, phase L0). LOG ONLY: changes no behaviour.
//!
//! Switched on by the environment variable `NEXUS_LOCKED_PROBE=1`. Every 5 seconds it writes one `[LOCKED]` line
//! with the session lock flag and the number of microphone callbacks since the last line (spoken replies show in
//! the existing `tts: speaking` lines). Lock changes get their own line. The owner locks the laptop, lets the screen dim and go off, and says
//! the wake word; the lines show whether the microphone kept delivering audio and when it stopped.
//! No audio, no transcript text and no personal data is written.

use std::time::Duration;

/// Start the probe thread if `NEXUS_LOCKED_PROBE` is set to `1`. Safe to call once at startup.
pub(crate) fn start_if_enabled() {
    if std::env::var("NEXUS_LOCKED_PROBE").map(|v| v == "1").unwrap_or(false) {
        spawn();
    }
}

fn spawn() {
    let _ = std::thread::Builder::new().name("locked-probe".into()).spawn(|| {
        tracing::info!("[LOCKED] probe started (log only)");
        let t0 = std::time::Instant::now();
        let mut last_cb = crate::wakeword_oww::mic_callback_count();
        let mut last_locked = crate::early_hello::is_session_locked();
        loop {
            std::thread::sleep(Duration::from_secs(5));
            let locked = crate::early_hello::is_session_locked();
            let cb = crate::wakeword_oww::mic_callback_count();
            let delta = cb.saturating_sub(last_cb);
            last_cb = cb;
            if locked != last_locked {
                tracing::info!("[LOCKED] session {} at +{}s", if locked { "LOCKED" } else { "UNLOCKED" }, t0.elapsed().as_secs());
                last_locked = locked;
            }
            tracing::info!(
                "[LOCKED] t=+{}s locked={} mic_callbacks_5s={}",
                t0.elapsed().as_secs(),
                locked,
                delta
            );
        }
    });
}
