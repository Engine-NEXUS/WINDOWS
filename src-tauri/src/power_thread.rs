//! Serialize OS execution-state calls on one long-lived thread (Windows assertions are thread-local).
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use std::time::Duration;

struct Request { awake: bool, cancelled: Arc<AtomicBool>, reply: SyncSender<Result<(), String>> }
pub struct PowerThread { tx: SyncSender<Request> }
impl PowerThread {
    pub fn new(mut apply: impl FnMut(bool) -> Result<(), String> + Send + 'static) -> Result<Self, String> {
        let (tx, rx) = mpsc::sync_channel::<Request>(1);
        std::thread::Builder::new().name("nexus-power-owner".into()).spawn(move || {
            while let Ok(r) = rx.recv() {
                if r.cancelled.load(Ordering::SeqCst) { continue; }
                let result = apply(r.awake);
                // A timed-out caller cannot leave a late assertion behind.
                if r.reply.send(result).is_err() && r.awake { let _ = apply(false); }
            }
            let _ = apply(false);
        }).map_err(|e| e.to_string())?;
        Ok(Self { tx })
    }
    pub fn set(&self, awake: bool) -> Result<(), String> {
        self.set_timeout(awake, Duration::from_secs(2))
    }
    pub(crate) fn set_timeout(&self, awake: bool, timeout: Duration) -> Result<(), String> {
        let (tx, rx) = mpsc::sync_channel(1);
        let cancelled=Arc::new(AtomicBool::new(false));
        self.tx.try_send(Request { awake, cancelled:cancelled.clone(), reply: tx }).map_err(|_| "power owner unavailable/busy".to_string())?;
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => { cancelled.store(true, Ordering::SeqCst); Err("power owner acknowledgement timed out".into()) }
        }
    }
}
