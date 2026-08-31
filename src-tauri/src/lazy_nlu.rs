//! Lazy NLU server manager — starts the Python NLU server (BERT-Mini ONNX)
//! on-demand when the deterministic parser can't handle a command, and kills
//! it after idle to save RAM.
//!
//! The NLU server (server/nlu_server.py) uses a BERT-Mini ONNX model and
//! takes ~50-100 MB of RAM. Instead of running it constantly, we:
//!   1. Spawn it when the deterministic parser returns None
//!   2. Kill it after 60 seconds of no parse requests
//!
//! This keeps RAM low at idle while providing ML-based intent classification
//! as a fallback for commands the deterministic parser can't handle.

use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static NLU_CHILD: Mutex<Option<Child>> = Mutex::new(None);
static NLU_RUNNING: AtomicBool = AtomicBool::new(false);
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

const NLU_IDLE_TIMEOUT: Duration = Duration::from_secs(60); // 1 minute
const NLU_PORT: u16 = 39218;

/// Get the NLU server script path.
fn nlu_script_path() -> Option<std::path::PathBuf> {
    let candidates = [
        // Development: src-tauri/target/release/../../../server/nlu_server.py
        std::env::current_exe()
            .ok()?
            .parent()?          // target/release
            .parent()?          // target
            .parent()?          // src-tauri
            .parent()?          // project root (ULTRON)
            .join("server")
            .join("nlu_server.py"),
        // Fallback: src-tauri/target/release/../../server/nlu_server.py
        std::env::current_exe()
            .ok()?
            .parent()?          // target/release
            .parent()?          // target
            .parent()?          // src-tauri
            .join("server")
            .join("nlu_server.py"),
        // Production: installed directory/server/nlu_server.py
        std::env::current_exe()
            .ok()?
            .parent()?
            .join("server")
            .join("nlu_server.py"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    tracing::warn!("[lazy_nlu] nlu_server.py not found in any candidate location");
    None
}

/// Check if the NLU server is already running (e.g. started externally or by a previous call).
fn is_nlu_responsive() -> bool {
    // Use a raw TCP connection — works from any thread (unlike tokio runtime checks)
    use std::net::TcpStream;
    use std::time::Duration as TcpDuration;
    let addr = format!("127.0.0.1:{}", NLU_PORT);
    TcpStream::connect_timeout(
        &addr.parse().unwrap_or_else(|_| "127.0.0.1:39218".parse().unwrap()),
        TcpDuration::from_millis(200),
    )
    .is_ok()
}

/// Ensure the NLU server is running. Spawns it if not.
pub fn ensure_nlu_running() {
    // Already running?
    if NLU_RUNNING.load(Ordering::Relaxed) {
        return;
    }

    // Check if an external NLU server is already running on the port
    if is_nlu_responsive() {
        NLU_RUNNING.store(true, Ordering::Relaxed);
        tracing::info!("[lazy_nlu] external NLU server detected on port {}", NLU_PORT);
        return;
    }

    let script = match nlu_script_path() {
        Some(p) => p,
        None => {
            tracing::warn!("[lazy_nlu] cannot start NLU server — script not found");
            return;
        }
    };

    tracing::info!("[lazy_nlu] starting NLU server: {:?}", script);

    // Spawn: python nlu_server.py
    let child = Command::new("python")
        .arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    match child {
        Ok(c) => {
            *NLU_CHILD.lock().unwrap() = Some(c);
            NLU_RUNNING.store(true, Ordering::Relaxed);
            tracing::info!("[lazy_nlu] NLU server spawned, waiting for it to be ready...");
            // Wait for the server to be responsive (up to 15 seconds)
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(500));
                if is_nlu_responsive() {
                    tracing::info!("[lazy_nlu] NLU server is ready");
                    // Start the idle killer thread
                    start_idle_killer();
                    return;
                }
            }
            tracing::warn!("[lazy_nlu] NLU server did not become responsive in 15s");
            NLU_RUNNING.store(false, Ordering::Relaxed);
        }
        Err(e) => {
            tracing::error!("[lazy_nlu] failed to spawn NLU server: {}", e);
        }
    }
}

/// Mark that an NLU request was just made (resets the idle timer).
pub fn mark_nlu_request() {
    *LAST_REQUEST.lock().unwrap() = Some(Instant::now());
}

/// Start a background thread that kills the NLU server after idle timeout.
fn start_idle_killer() {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        let should_kill = {
            let last = LAST_REQUEST.lock().unwrap();
            if let Some(t) = *last {
                t.elapsed() > NLU_IDLE_TIMEOUT
            } else {
                // No request ever made — kill after timeout from start
                true
            }
        };
        if should_kill && NLU_RUNNING.load(Ordering::Relaxed) {
            tracing::info!("[lazy_nlu] idle timeout reached, killing NLU server");
            let mut child_guard = NLU_CHILD.lock().unwrap();
            if let Some(mut child) = child_guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            NLU_RUNNING.store(false, Ordering::Relaxed);
            return;
        }
    });
}
