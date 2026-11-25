//! Is the local speech model on this disk, and can it be fetched in the background?
//!
//! The local speech server (Moonshine) needs about 250 MB of model files. They are downloaded the first time the server
//! starts, and until then the server does not even listen, which on a slow link means minutes of a speech engine that
//! silently is not there. This module lets the Command Hub show the state and start the download by itself, with no server
//! and no model in memory (`stt_server.py --prefetch`).
//!
//! The cache folder is the one the Python package uses (`platformdirs`): on Windows
//! `%LOCALAPPDATA%\moonshine_voice\moonshine_voice\Cache\download.moonshine.ai\model\<model>-en\<version>\`. On other systems
//! the place is not known here and the state is `Unknown` (nothing is gated on it).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// About how big the whole model is, for a progress figure.
pub const EXPECTED_MB: u64 = 245;

/// Files that must be there, each with the smallest size it can have when complete.
const NEEDED: [(&str, u64); 6] = [
    ("decoder_kv.ort", 130 << 20),
    ("encoder.ort", 85 << 20),
    ("cross_kv.ort", 10 << 20),
    ("adapter.ort", 3 << 20),
    ("frontend.weights.ort", 10 << 20),
    ("tokenizer.bin", 100 << 10),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// All files are there and complete.
    Ready,
    /// Files are being fetched (partial files exist, or a background fetch is running).
    Downloading,
    /// Nothing, or too little, is on disk and nothing is being fetched.
    Missing,
    /// The cache place is not known on this system.
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Status {
    pub state: State,
    pub downloaded_mb: u64,
    pub expected_mb: u64,
}

fn mb(bytes: u64) -> u64 {
    bytes >> 20
}

/// The cache root of the model files, on systems where it is known.
pub fn cache_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let local = std::env::var_os("LOCALAPPDATA")?;
        Some(PathBuf::from(local).join("moonshine_voice").join("moonshine_voice").join("Cache").join("download.moonshine.ai").join("model"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// The newest `<model>-en/<version>` folder under `root` (never the spelling model).
fn model_dir(root: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for model in std::fs::read_dir(root).ok()?.flatten() {
        let name = model.file_name().to_string_lossy().to_string();
        if !name.ends_with("-en") || name.starts_with("spelling") || !model.path().is_dir() {
            continue;
        }
        for version in std::fs::read_dir(model.path()).ok()?.flatten() {
            if !version.path().is_dir() {
                continue;
            }
            let t = version.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().map_or(true, |(bt, _)| t > *bt) {
                best = Some((t, version.path()));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The state of the files under `root`. `fetching` is true while this app's own background fetch is running.
pub fn classify(root: &Path, fetching: bool) -> Status {
    let Some(dir) = model_dir(root) else {
        return Status { state: if fetching { State::Downloading } else { State::Missing }, downloaded_mb: 0, expected_mb: EXPECTED_MB };
    };
    let mut total: u64 = 0;
    let mut partial = false;
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.ends_with(".partial") || name.ends_with(".lock") {
                partial = true;
            }
            total += e.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    let complete = NEEDED.iter().all(|(file, min)| std::fs::metadata(dir.join(file)).map_or(false, |m| m.len() >= *min));
    let state = if complete && !partial {
        State::Ready
    } else if partial || fetching {
        State::Downloading
    } else {
        State::Missing
    };
    Status { state, downloaded_mb: mb(total).min(EXPECTED_MB), expected_mb: EXPECTED_MB }
}

static FETCHING: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    match cache_root() {
        Some(root) => classify(&root, FETCHING.load(Ordering::SeqCst)),
        None => Status { state: State::Unknown, downloaded_mb: 0, expected_mb: EXPECTED_MB },
    }
}

/// Starts the download in the background unless it is already there or already running. Returns the state right after.
pub fn prefetch() -> Result<Status, String> {
    let now = status();
    if matches!(now.state, State::Ready | State::Unknown) {
        return Ok(now);
    }
    if FETCHING.swap(true, Ordering::SeqCst) {
        return Ok(status()); // already running
    }
    let Some(python) = crate::lazy_stt::find_python() else {
        FETCHING.store(false, Ordering::SeqCst);
        return Err("Python was not found, so the speech model cannot be downloaded here.".into());
    };
    let Some(script) = crate::lazy_stt::stt_script_path() else {
        FETCHING.store(false, Ordering::SeqCst);
        return Err("The speech server script was not found.".into());
    };
    std::thread::Builder::new()
        .name("stt-model-prefetch".into())
        .spawn(move || {
            tracing::info!("stt_model: downloading the local speech model in the background");
            let result = crate::no_window::command(&python)
                .arg(&script)
                .arg("--prefetch")
                .envs(crate::lazy_stt::stt_child_env())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .output();
            match result {
                Ok(o) if o.status.success() => tracing::info!("stt_model: the local speech model is downloaded"),
                Ok(o) => tracing::warn!("stt_model: the download stopped with {}", o.status),
                Err(e) => tracing::warn!("stt_model: could not start the download: {e}"),
            }
            FETCHING.store(false, Ordering::SeqCst);
        })
        .map_err(|e| {
            FETCHING.store(false, Ordering::SeqCst);
            e.to_string()
        })?;
    Ok(status())
}

#[tauri::command]
pub fn stt_model_status() -> Status {
    status()
}

#[tauri::command]
pub async fn stt_model_prefetch() -> Result<Status, String> {
    tauri::async_runtime::spawn_blocking(prefetch).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(dir: &Path, name: &str, bytes: u64) {
        std::fs::create_dir_all(dir).unwrap();
        let f = std::fs::File::create(dir.join(name)).unwrap();
        f.set_len(bytes).unwrap();
    }

    fn model(root: &Path) -> PathBuf {
        root.join("medium-streaming-en").join("quantized_26_11_02")
    }

    fn fresh(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nexus-stt-model-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn complete(root: &Path) {
        for (f, min) in NEEDED {
            put(&model(root), f, min + 1024);
        }
    }

    #[test]
    fn an_empty_cache_is_missing_and_a_running_fetch_is_downloading() {
        let root = fresh("empty");
        assert_eq!(classify(&root, false).state, State::Missing);
        assert_eq!(classify(&root, true).state, State::Downloading);
        assert_eq!(classify(&root, false).downloaded_mb, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn all_files_present_and_complete_is_ready() {
        let root = fresh("ready");
        complete(&root);
        let s = classify(&root, false);
        assert_eq!(s.state, State::Ready);
        assert!(s.downloaded_mb >= 200 && s.downloaded_mb <= EXPECTED_MB, "{s:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_partial_file_means_downloading_even_when_the_other_files_are_there() {
        let root = fresh("partial");
        complete(&root);
        std::fs::remove_file(model(&root).join("encoder.ort")).unwrap();
        put(&model(&root), "encoder.ort.partial", 20 << 20);
        put(&model(&root), "encoder.ort.lock", 0);
        let s = classify(&root, false);
        assert_eq!(s.state, State::Downloading, "{s:?}");
        // and a finished encoder with a leftover lock file is still not called ready
        put(&model(&root), "encoder.ort", 90 << 20);
        assert_eq!(classify(&root, false).state, State::Downloading);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_truncated_file_is_not_ready_and_the_spelling_model_is_ignored() {
        let root = fresh("trunc");
        complete(&root);
        put(&model(&root), "decoder_kv.ort", 5 << 20); // cut short
        assert_eq!(classify(&root, false).state, State::Missing, "an incomplete file is not a model");
        // a spelling model next to it never counts as the speech model
        // (even one laid out exactly like a speech model, with a version folder and every file, must not count)
        let other = fresh("spell");
        for (f, min) in NEEDED {
            put(&other.join("spelling-en").join("v1"), f, min + 1024);
        }
        assert_eq!(classify(&other, false).state, State::Missing);
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&other);
    }

    #[test]
    fn progress_counts_partial_bytes_but_never_passes_the_expected_size() {
        let root = fresh("progress");
        put(&model(&root), "decoder_kv.ort.partial", 40 << 20);
        put(&model(&root), "adapter.ort", 4 << 20);
        let s = classify(&root, true);
        assert_eq!(s.state, State::Downloading);
        assert_eq!(s.downloaded_mb, 44);
        put(&model(&root), "huge.bin", 900 << 20);
        assert_eq!(classify(&root, true).downloaded_mb, EXPECTED_MB);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_background_fetch_runs_the_server_script_in_its_own_prefetch_mode_and_never_starts_the_server() {
        let src = include_str!("stt_model.rs").split("mod tests {").next().unwrap().to_string();
        assert!(src.contains(".arg(\"--prefetch\")"));
        assert!(!src.contains("ensure_stt_running"), "downloading the model does not start the speech server");
        let py = include_str!("../../server/stt_server.py");
        assert!(py.contains("\"--prefetch\" in sys.argv") && py.contains("sys.exit(0)"), "the script has the prefetch mode");
    }
}
