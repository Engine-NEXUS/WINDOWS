//! Content-free capture accounting. This records observations, never authority.
//! Ownership of an Attempt moves with its audio packet; every normal path closes
//! it once. Process crashes remain unfinished in the external reconciliation.
use serde::Serialize;
use std::sync::{atomic::{AtomicU64, Ordering}, Arc};
use std::time::Instant;

#[derive(Clone, Debug, Serialize)]
pub struct RunInfo {
    pub run_id: String,
    pub build_identity: Option<String>,
    pub source_hash: Option<String>,
    pub executable_matches_manifest: bool,
}

impl RunInfo {
    fn local() -> Self {
        let mut info = Self {
            run_id: format!("{}-{}", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()),
            build_identity: None, source_hash: None, executable_matches_manifest: false,
        };
        let proof = (|| {
            use sha2::{Digest, Sha256};
            use std::io::Read;
            let exe = std::env::current_exe().ok()?;
            let manifest: serde_json::Value = serde_json::from_slice(
                &std::fs::read(exe.parent()?.join("nexus-build.json")).ok()?).ok()?;
            let mut file = std::fs::File::open(exe).ok()?;
            let mut digest = Sha256::new();
            let mut block = [0u8; 65536];
            loop {
                let count = file.read(&mut block).ok()?;
                if count == 0 { break; }
                digest.update(&block[..count]);
            }
            let hash = format!("{:x}", digest.finalize());
            manifest_link(&manifest, &hash)
        })();
        if let Some((identity, source)) = proof {
            info.build_identity = Some(identity);
            info.source_hash = Some(source);
            info.executable_matches_manifest = true;
        }
        info
    }
}

fn manifest_link(manifest: &serde_json::Value, executable_hash: &str) -> Option<(String, String)> {
    let identity = manifest.get("identity")?.as_str()?;
    let source = manifest.get("source")?.as_str()?;
    let valid_hex = |text: &str, size| text.len() == size && text.bytes().all(|c| c.is_ascii_hexdigit());
    if !valid_hex(identity, 16) || !valid_hex(source, 64)
        || manifest.get("executable")?.as_str()? != executable_hash { return None; }
    Some((identity.to_owned(), source.to_owned()))
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    MicHeld, Duplicate, Cancelled, Superseded, ReceiverUnavailable,
    Stale, EmptyBuffer, Phantom, OwnerHold, SttError, Filtered,
    EmptyTranscript, DeliveryFailed, Delivered, Abandoned,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    #[serde(flatten)]
    pub run: RunInfo,
    pub attempt_id: u64,
    pub session: Option<u64>,
    pub origin: &'static str,
    pub event: &'static str,
    pub outcome: Option<Outcome>,
    pub utc_ms: i64,
    pub elapsed_ms: u64,
}

type Sink = Arc<dyn Fn(Event) + Send + Sync>;

pub struct Audit {
    run: RunInfo,
    next: AtomicU64,
    sink: Sink,
}

impl Audit {
    fn begin(&self, origin: &'static str) -> Attempt {
        let attempt = Attempt {
            run: self.run.clone(), id: self.next.fetch_add(1, Ordering::Relaxed),
            session: None, origin, started: Instant::now(), done: false, sink: self.sink.clone(),
        };
        attempt.emit("requested", None);
        attempt
    }
}

static AUDIT: once_cell::sync::Lazy<Audit> = once_cell::sync::Lazy::new(|| Audit {
    run: RunInfo::local(), next: AtomicU64::new(1),
    sink: Arc::new(|event| {
        if let Ok(json) = serde_json::to_string(&event) {
            tracing::info!(target: "capture_audit", "[CAPTURE] {json}");
        }
    }),
});

/// Call before opening the native audio stream, so executable hashing is never
/// performed in the audio callback or the first wake's critical path.
pub fn initialize() { once_cell::sync::Lazy::force(&AUDIT); }

pub fn begin(origin: &'static str) -> Attempt { AUDIT.begin(origin) }

/// Correlates routing entry without claiming an action executed. The session
/// comes from existing native turn context, not a transcript string.
pub fn dispatch_received(session: u64) {
    if session == 0 { return; }
    tracing::info!(target: "capture_audit", "[CAPTURE-DISPATCH] {}", serde_json::json!({
        "run_id": AUDIT.run.run_id, "session": session, "event": "dispatch_received",
        "utc_ms": chrono::Utc::now().timestamp_millis(),
    }));
}

pub struct Attempt {
    run: RunInfo,
    id: u64,
    session: Option<u64>,
    origin: &'static str,
    started: Instant,
    done: bool,
    sink: Sink,
}

impl Attempt {
    fn emit(&self, event: &'static str, outcome: Option<Outcome>) {
        (self.sink)(Event {
            run: self.run.clone(), attempt_id: self.id, session: self.session, origin: self.origin,
            event, outcome, utc_ms: chrono::Utc::now().timestamp_millis(),
            elapsed_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
        });
    }
    pub fn started(&mut self, session: u64) {
        self.session = Some(session);
        self.stage("started");
    }
    pub fn stage(&self, stage: &'static str) {
        if !self.done { self.emit(stage, None); }
    }
    pub fn finish(&mut self, outcome: Outcome) {
        if !self.done {
            self.done = true;
            self.emit("terminal", Some(outcome));
        }
    }
}

impl Drop for Attempt {
    fn drop(&mut self) { self.finish(Outcome::Abandoned); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    fn fixture() -> (Audit, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = events.clone();
        (Audit {
            run: RunInfo { run_id: "test-run".into(), build_identity: None, source_hash: None, executable_matches_manifest: false },
            next: AtomicU64::new(1), sink: Arc::new(move |e| sink_events.lock().unwrap().push(e)),
        }, events)
    }
    #[test]
    fn ten_requests_have_unique_ids_and_exactly_ten_terminals() {
        let (audit, events) = fixture();
        for session in 1..=10 {
            let mut attempt = audit.begin("direct");
            attempt.started(session);
            attempt.finish(if session % 2 == 0 { Outcome::Cancelled } else { Outcome::Delivered });
        }
        let events = events.lock().unwrap();
        let requested: Vec<_> = events.iter().filter(|e| e.event == "requested").map(|e| e.attempt_id).collect();
        assert_eq!(requested, (1..=10).collect::<Vec<_>>());
        assert_eq!(events.iter().filter(|e| e.event == "terminal").count(), 10);
    }
    #[test]
    fn duplicate_finish_and_drop_do_not_double_count() {
        let (audit, events) = fixture();
        let mut attempt = audit.begin("hot_mic");
        attempt.finish(Outcome::Duplicate);
        attempt.finish(Outcome::Delivered);
        attempt.stage("late");
        drop(attempt);
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(serde_json::to_value(&events[1]).unwrap()["outcome"], "duplicate");
    }
    #[test]
    fn abandoned_packet_is_visible_and_failed_send_drops_it() {
        let (audit, events) = fixture();
        let (tx, rx) = std::sync::mpsc::channel();
        drop(rx);
        drop(tx.send(audit.begin("direct")));
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(serde_json::to_value(&events[1]).unwrap()["outcome"], "abandoned");
    }
    #[test]
    fn panic_unwind_closes_the_owned_attempt() {
        let (audit, events) = fixture();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _attempt = audit.begin("direct");
            panic!("simulated worker panic");
        }));
        assert_eq!(events.lock().unwrap().iter().filter(|e| e.event == "terminal").count(), 1);
    }
    #[test]
    fn manifest_requires_actual_executable_match_and_valid_hashes() {
        let hash = "a".repeat(64);
        let mut manifest = serde_json::json!({"identity":"0123456789abcdef","source":"b".repeat(64),"executable":hash});
        assert!(manifest_link(&manifest, &hash).is_some());
        assert!(manifest_link(&manifest, &"c".repeat(64)).is_none());
        manifest["identity"] = "invalid/identity".into();
        assert!(manifest_link(&manifest, &hash).is_none());
    }
    #[test]
    fn audit_schema_has_no_speech_or_biometric_content() {
        let (audit, events) = fixture();
        drop(audit.begin("direct"));
        let json = serde_json::to_value(&events.lock().unwrap()[0]).unwrap();
        let keys: std::collections::BTreeSet<_> = json.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(keys, ["run_id","build_identity","source_hash","executable_matches_manifest","attempt_id","session","origin","event","outcome","utc_ms","elapsed_ms"].into_iter().collect());
    }
}
