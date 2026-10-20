//! STT proxy — routes to Groq cloud STT (primary) or local Moonshine (fallback).
//!
//! Primary: Groq Whisper Large v3 Turbo (cloud, ~247ms, $0 free tier)
//! Fallback: Moonshine Small Streaming (local Python sidecar, ~165ms, 7.84% WER)
//!
//! The fallback is used when:
//! - No Groq API key is set in settings
//! - Groq API is unreachable (network error)
//! - Groq rate limit is hit (429)
//! - Groq returns an error
//! - localSttOnly is true (privacy mode — audio never leaves the device)

use std::sync::Arc;
use tauri::State;

static SHARED_STT_CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();

/// Global shared reqwest client with persistent HTTP/2 connection pooling
/// and TCP keep-alive. Reused across all STT capture turns to eliminate cold
/// TLS handshake latency (~180ms saved per turn).
pub fn shared_client() -> &'static reqwest::Client {
    SHARED_STT_CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .expect("failed to build shared STT HTTP client")
    })
}

pub struct SttState {
    /// Reused HTTP client — avoids building a new reqwest::Client per transcription.
    pub client: Arc<reqwest::Client>,
}

impl SttState {
    pub fn new() -> Self {
        Self { client: Arc::new(shared_client().clone()) }
    }
}

const STT_URL: &str = "http://127.0.0.1:39217/transcribe";

/// Turn-packet markers (approach A): which STT path ran + whether the
/// hallucination filter rewrote the text. Written at every branch of
/// `transcribe_audio`/`transcribe_samples`, read once by the Rust capture
/// receiver after each turn. Best-effort diagnostics only — the receiver
/// thread is the sole reader in the capture flow, so no lock needed.
static LAST_STT_PATH: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
static LAST_FILTER: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Path codes: 1 groq · 2 groq-error→local · 3 keyless→local · 4 local-only
/// (privacy) · 5 short-buffer reject · 6 low-energy reject · 7 local error.
pub(crate) fn reset_turn_markers() {
    LAST_STT_PATH.store(0, std::sync::atomic::Ordering::Relaxed);
    LAST_FILTER.store(0, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn last_stt_path_str() -> &'static str {
    match LAST_STT_PATH.load(std::sync::atomic::Ordering::Relaxed) {
        1 => "groq",
        2 => "groq_err_local",
        3 => "keyless_local",
        4 => "local_only",
        5 => "short_reject",
        6 => "energy_reject",
        7 => "local_err",
        8 => "deepgram",
        _ => "unknown",
    }
}

pub(crate) fn last_filter_str() -> &'static str {
    match LAST_FILTER.load(std::sync::atomic::Ordering::Relaxed) {
        1 => "hallucination",
        2 => "dict_gate",
        3 => "non_ascii",
        4 => "fragment",
        5 => "repeat_loop",
        6 => "cjk",
        7 => "too_short",
        _ => "none",
    }
}

/// Why `evaluate_transcript` blanked a transcript (codes shared with `last_filter_str`).
const R_PHRASE: u8 = 1;
const R_DICT: u8 = 2;
const R_NON_ASCII: u8 = 3;
const R_FRAGMENT: u8 = 4;
const R_REPEAT: u8 = 5;
const R_CJK: u8 = 6;
const R_SHORT: u8 = 7;

fn mark_path(code: u8) {
    LAST_STT_PATH.store(code, std::sync::atomic::Ordering::Relaxed);
}

fn mark_filtered(original: &str, filtered: &str) {
    // `filter_transcript_counted` already stored the specific reason; this only covers a caller
    // that filtered without it (the marker was still "none").
    if filtered != original
        && LAST_FILTER.load(std::sync::atomic::Ordering::Relaxed) == 0
    {
        LAST_FILTER.store(R_PHRASE, std::sync::atomic::Ordering::Relaxed);
    }
}

/// Worst-case Whisper confidence over a turn's segments: (lowest avg_logprob, highest
/// no_speech_prob). `None` when the engine returned no segment metadata.
pub(crate) fn confidence_summary(segments: &[crate::stt_groq::GroqSegment]) -> Option<(f32, f32)> {
    if segments.is_empty() {
        return None;
    }
    let min_lp = segments.iter().map(|s| s.avg_logprob).fold(f32::INFINITY, f32::min);
    let max_ns = segments.iter().map(|s| s.no_speech_prob).fold(f32::NEG_INFINITY, f32::max);
    Some((min_lp, max_ns))
}

/// One `[STT-CONF]` log line per command turn (P0 measurement; opt-in `sttLogConfidence`).
/// Picked up by scripts/run.ps1's catch-all and greppable for threshold selection.
pub(crate) fn format_confidence_line(
    text: &str,
    segments: &[crate::stt_groq::GroqSegment],
    samples: usize,
) -> String {
    let dur = samples as f32 / 16000.0;
    let words = text.split_whitespace().count();
    match confidence_summary(segments) {
        Some((lp, ns)) => format!(
            "[STT-CONF] dur={dur:.2}s words={words} segs={} min_avg_logprob={lp:.3} max_no_speech={ns:.3} text='{}'",
            segments.len(),
            text.trim()
        ),
        None => format!("[STT-CONF] dur={dur:.2}s words={words} segs=0 (no confidence info) text='{}'", text.trim()),
    }
}

/// Minimum audio samples required for transcription (200ms at 16kHz).
pub const MIN_AUDIO_SAMPLES: usize = 3200;

/// Root-mean-square energy floor below which audio is treated as silence / noise.
pub const RMS_ENERGY_FLOOR: f64 = 0.005;

/// Pre-gate decision for raw audio buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioPreGate {
    Pass,
    TooShort,
    LowEnergy,
}

/// Compute root-mean-square energy of raw PCM samples.
/// Returns 0.0 for empty buffers and guards against NaN.
pub fn audio_rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f64 = samples.iter().map(|&s| {
        let f = s as f64 / 32768.0;
        f * f
    }).sum();
    let mean = sum_sq / samples.len() as f64;
    if mean <= 0.0 || mean.is_nan() {
        0.0
    } else {
        mean.sqrt()
    }
}

/// Acoustic pre-gate: drops buffers shorter than 200ms (3200 samples)
/// or with RMS energy under 0.005 before any cloud API or local server call.
pub fn check_audio_pre_gate(samples: &[i16]) -> AudioPreGate {
    if samples.len() < MIN_AUDIO_SAMPLES {
        return AudioPreGate::TooShort;
    }
    let rms = audio_rms(samples);
    if rms.is_nan() || rms < RMS_ENERGY_FLOOR {
        return AudioPreGate::LowEnergy;
    }
    AudioPreGate::Pass
}

/// STT execution engines ordered in a route plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttEngine {
    Deepgram,
    Groq,
    Local,
}

/// Ordered list of STT engines to attempt.
pub type RoutePlan = Vec<SttEngine>;

/// Pure routing decision: decides which STT engines to attempt in order.
/// If `local_only` is true (privacy mode), only `[Local]` is returned.
/// Otherwise, engines with available keys are prioritized: Deepgram -> Groq -> Local.
pub fn choose_route(local_only: bool, has_deepgram: bool, has_groq: bool) -> RoutePlan {
    if local_only {
        return vec![SttEngine::Local];
    }
    let mut plan = Vec::with_capacity(3);
    if has_deepgram {
        plan.push(SttEngine::Deepgram);
    }
    if has_groq {
        plan.push(SttEngine::Groq);
    }
    plan.push(SttEngine::Local);
    plan
}

/// Classification of errors from STT engines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SttError {
    Network(String),
    RateLimited,
    Unauthorized,
    ServerError(u16),
    Timeout,
    EmptyCloudTranscript,
    InvalidAudio(String),
    TooShort { samples: usize, min_samples: usize },
    Other(String),
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SttError::Network(msg) => write!(f, "Network error: {msg}"),
            SttError::RateLimited => write!(f, "Rate limit hit (429)"),
            SttError::Unauthorized => write!(f, "Unauthorized (401)"),
            SttError::ServerError(code) => write!(f, "Server error ({code})"),
            SttError::Timeout => write!(f, "Request timed out"),
            SttError::EmptyCloudTranscript => write!(f, "Empty transcript returned from cloud"),
            SttError::InvalidAudio(msg) => write!(f, "Invalid audio: {msg}"),
            SttError::TooShort { samples, min_samples } => {
                write!(f, "Audio buffer too short ({samples} < {min_samples} samples)")
            }
            SttError::Other(msg) => write!(f, "STT error: {msg}"),
        }
    }
}

impl std::error::Error for SttError {}

impl SttError {
    pub fn from_error_message(msg: &str) -> Self {
        let lower = msg.to_lowercase();
        if lower.contains("too short") || lower.contains("empty audio") {
            SttError::TooShort { samples: 0, min_samples: MIN_AUDIO_SAMPLES }
        } else if lower.starts_with("invalid audio:") {
            // Only OUR own pre-validation message. A cloud provider rejecting the file ("invalid audio mime type",
            // "mime error") is a provider-side failure: the local engine can still transcribe it, so it falls back.
            SttError::InvalidAudio(msg.to_string())
        } else if lower.contains("429") || lower.contains("rate limit") {
            SttError::RateLimited
        } else if lower.contains("401") || lower.contains("unauthorized") || lower.contains("invalid api key") || lower.contains("api key invalid") {
            SttError::Unauthorized
        } else if lower.contains("timeout") || lower.contains("timed out") {
            SttError::Timeout
        } else if lower.contains("empty transcript") {
            SttError::EmptyCloudTranscript
        } else if lower.contains("500") || lower.contains("502") || lower.contains("503") || lower.contains("504") {
            SttError::ServerError(500)
        } else if lower.contains("network") || lower.contains("connection") || lower.contains("request failed") {
            SttError::Network(msg.to_string())
        } else {
            SttError::Other(msg.to_string())
        }
    }
}

/// Fallback decision: returns true if the error is a transient failure or provider issue
/// that warrants falling back to the next engine in the plan.
/// Client-side invalid inputs (e.g. invalid audio format or buffer too short) do not fall back.
pub fn should_fall_back(err: &SttError) -> bool {
    match err {
        SttError::Network(_)
        | SttError::RateLimited
        | SttError::Unauthorized
        | SttError::ServerError(_)
        | SttError::Timeout
        | SttError::EmptyCloudTranscript
        | SttError::Other(_) => true,
        SttError::InvalidAudio(_)
        | SttError::TooShort { .. } => false,
    }
}

/// Transcribe audio — tries Groq cloud first, falls back to local faster-whisper.
///
/// The frontend sends Int16 PCM samples at 16kHz. We route to Groq if an API
/// key is configured, otherwise use the local Python sidecar.
#[tauri::command]
pub async fn transcribe_audio(
    samples: Vec<i16>,
    state: State<'_, SttState>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    tracing::info!("stt: received {} samples for transcription", samples.len());

    match check_audio_pre_gate(&samples) {
        AudioPreGate::TooShort => {
            tracing::info!("stt: buffer too short ({} samples), skipping STT", samples.len());
            mark_path(5);
            return Ok("".to_string());
        }
        AudioPreGate::LowEnergy => {
            let rms = audio_rms(&samples);
            tracing::info!("stt: audio energy below threshold (RMS {:.5} < 0.005), skipping STT", rms);
            mark_path(6);
            return Ok("".to_string());
        }
        AudioPreGate::Pass => {}
    }

    // Read API keys from settings
    let deepgram_key = crate::commands::read_api_key(&app, "deepgram");
    let groq_key = crate::commands::read_groq_api_key(&app);
    let local_only = crate::commands::read_local_stt_only(&app);

    let plan = choose_route(local_only, !deepgram_key.is_empty(), !groq_key.is_empty());

    if plan.first() == Some(&SttEngine::Local) {
        if local_only {
            tracing::info!("stt: localSttOnly=true, using local whisper (privacy mode)");
            mark_path(4);
        } else {
            tracing::info!("stt: no groq/deepgram key, using local whisper directly");
            mark_path(3);
        }
    }

    if plan.contains(&SttEngine::Deepgram) {
        match crate::stt_deepgram::transcribe_with_deepgram(&samples, &deepgram_key, &state.client).await {
            Ok(text) => {
                let filtered = filter_transcript_counted(&text);
                mark_path(8);
                mark_filtered(&text, &filtered);
                tracing::info!("stt: deepgram transcript: '{}'", filtered);
                return Ok(filtered);
            }
            Err(e) => {
                let err = SttError::from_error_message(&e);
                if should_fall_back(&err) {
                    tracing::warn!("stt: deepgram failed ({}), falling back to Groq LPU pipeline", e);
                } else {
                    tracing::warn!("stt: deepgram failed with non-fallback error ({})", e);
                    return Err(e);
                }
            }
        }
    }

    if plan.contains(&SttEngine::Groq) {
        // Primary: Groq cloud STT (~247ms, free) — with the NEXUS domain
        // vocabulary so rare entities (Servx, Zync, Eesha) win decoder bets.
        // P0 measurement (opt-in `sttLogConfidence`): same request as verbose_json so Whisper's
        // per-segment confidence can be logged for every command turn. Never changes the result.
        let groq_result = if crate::commands::read_stt_log_confidence(&app) {
            match crate::stt_groq::transcribe_with_groq_verbose(&samples, &groq_key, &state.client, Some(crate::stt_groq::NEXUS_VOCABULARY)).await {
                Ok((text, segments)) => {
                    tracing::info!("{}", format_confidence_line(&text, &segments, samples.len()));
                    Ok(text)
                }
                Err(e) => Err(e),
            }
        } else {
            crate::stt_groq::transcribe_with_groq(&samples, &groq_key, &state.client, Some(crate::stt_groq::NEXUS_VOCABULARY)).await
        };
        match groq_result {
            Ok(text) => {
                let filtered = filter_transcript_counted(&text);
                mark_path(1);
                mark_filtered(&text, &filtered);
                if filtered != text {
                    tracing::info!("stt: filtered hallucination '{}' -> '{}'", text, filtered);
                } else {
                    tracing::info!("stt: groq transcript: '{}'", filtered);
                }
                return Ok(filtered);
            }
            Err(e) => {
                let err = SttError::from_error_message(&e);
                if should_fall_back(&err) {
                    tracing::warn!("stt: groq failed ({}), falling back to local whisper", e);
                    println!("[STT-ERR] Groq STT failed, falling back to local (slower, may reorder turns): {e}");
                    mark_path(2);
                } else {
                    tracing::warn!("stt: groq failed with non-fallback error ({})", e);
                    return Err(e);
                }
            }
        }
    }

    // Fallback: local faster-whisper Python sidecar
    match transcribe_local(&samples, &state.client).await {
        Ok(text) => Ok(text),
        Err(e) => {
            mark_path(7);
            Err(e)
        }
    }
}

/// Transcribe audio samples using the full STT pipeline (Groq → local fallback).
/// This is the same logic as `transcribe_audio` but can be called from
/// the Rust-side STT capture thread (no Tauri command context needed).
///
/// # Arguments
/// * `samples` - Raw i16 PCM samples at 16kHz mono
/// * `client` - Reused reqwest client
/// * `app` - Optional AppHandle for reading Groq API key and local_stt_only setting
/// * `prompt` - Optional Groq decoder-bias hint (None for normal transcription)
pub async fn transcribe_samples<R: tauri::Runtime>(
    samples: &[i16],
    client: &reqwest::Client,
    app: Option<&tauri::AppHandle<R>>,
    prompt: Option<&'static str>,
) -> Result<String, String> {
    tracing::info!("stt: transcribing {} samples (Rust-side capture)", samples.len());

    match check_audio_pre_gate(samples) {
        AudioPreGate::TooShort => {
            tracing::info!("stt: buffer too short ({} samples), skipping STT", samples.len());
            mark_path(5);
            return Ok("".to_string());
        }
        AudioPreGate::LowEnergy => {
            let rms = audio_rms(samples);
            tracing::info!("stt: audio energy below threshold (RMS {:.5} < 0.005), skipping STT", rms);
            mark_path(6);
            return Ok("".to_string());
        }
        AudioPreGate::Pass => {}
    }

    // Read API keys from settings (if app handle is available)
    if let Some(app) = app {
        let deepgram_key = crate::commands::read_api_key(app, "deepgram");
        let groq_key = crate::commands::read_groq_api_key(app);
        let local_only = crate::commands::read_local_stt_only(app);

        let plan = choose_route(local_only, !deepgram_key.is_empty(), !groq_key.is_empty());

        if plan.first() == Some(&SttEngine::Local) {
            if local_only {
                tracing::info!("stt: localSttOnly=true, using local whisper (privacy mode)");
                mark_path(4);
            } else {
                tracing::info!("stt: no groq/deepgram key, using local whisper directly");
                mark_path(3);
            }
        }

        if plan.contains(&SttEngine::Deepgram) {
            // Optional Deepgram Nova-2 cloud STT
            match crate::stt_deepgram::transcribe_with_deepgram(samples, &deepgram_key, client).await {
                Ok(text) => {
                    let filtered = filter_transcript_counted(&text);
                    mark_path(8);
                    mark_filtered(&text, &filtered);
                    tracing::info!("stt: deepgram transcript: '{}'", filtered);
                    return Ok(filtered);
                }
                Err(e) => {
                    let err = SttError::from_error_message(&e);
                    if should_fall_back(&err) {
                        tracing::warn!("stt: deepgram failed ({}), falling back to Groq LPU pipeline", e);
                    } else {
                        tracing::warn!("stt: deepgram failed with non-fallback error ({})", e);
                        return Err(e);
                    }
                }
            }
        }

        if plan.contains(&SttEngine::Groq) {
            match crate::stt_groq::transcribe_with_groq(samples, &groq_key, client, prompt).await {
                Ok(text) => {
                    let filtered = filter_transcript_counted(&text);
                    mark_path(1);
                    mark_filtered(&text, &filtered);
                    if filtered != text {
                        tracing::info!("stt: filtered hallucination '{}' -> '{}'", text, filtered);
                    } else {
                        tracing::info!("stt: groq transcript: '{}'", filtered);
                    }
                    return Ok(filtered);
                }
                Err(e) => {
                    let err = SttError::from_error_message(&e);
                    if should_fall_back(&err) {
                        tracing::warn!("stt: groq failed ({}), falling back to local whisper", e);
                        println!("[STT-ERR] Groq STT failed, falling back to local (slower, may reorder turns): {e}");
                        mark_path(2);
                    } else {
                        tracing::warn!("stt: groq failed with non-fallback error ({})", e);
                        return Err(e);
                    }
                }
            }
        }
    }

    // Fallback: local faster-whisper Python sidecar
    match transcribe_local(samples, client).await {
        Ok(text) => Ok(text),
        Err(e) => {
            mark_path(7);
            Err(e)
        }
    }
}

/// Verbose variant for the wake-word verifier (v3 confidence gate).
/// Returns (transcript, segments); segments carry no_speech_prob/avg_logprob.
/// Groq path uses `verbose_json`; local fallback returns text with NO segments
/// (caller must treat empty segments as "no confidence info" → word gate only).
/// Applies the same hallucination filter to the transcript text.
/// Unused under `mock-wake` — allowed, not dead.
#[allow(dead_code)]
pub async fn transcribe_samples_verbose<R: tauri::Runtime>(
    samples: &[i16],
    client: &reqwest::Client,
    app: Option<&tauri::AppHandle<R>>,
    prompt: Option<&'static str>,
) -> Result<(String, Vec<crate::stt_groq::GroqSegment>), String> {
    if let Some(app) = app {
        let groq_key = crate::commands::read_groq_api_key(app);
        let local_only = crate::commands::read_local_stt_only(app);

        if local_only {
            tracing::info!("stt: localSttOnly=true, using local whisper (privacy mode)");
            mark_path(4);
        } else if !groq_key.is_empty() {
            match crate::stt_groq::transcribe_with_groq_verbose(
                samples, &groq_key, client, prompt,
            )
            .await
            {
                Ok((text, segments)) => {
                    let filtered = filter_transcript_counted(&text);
                    mark_path(1);
                    mark_filtered(&text, &filtered);
                    if filtered != text {
                        tracing::info!("stt: filtered hallucination '{}' -> '{}'", text, filtered);
                    } else {
                        tracing::info!("stt: groq transcript: '{}'", filtered);
                    }
                    return Ok((filtered, segments));
                }
                Err(e) => {
                    tracing::warn!("stt: groq failed ({}), falling back to local whisper", e);
                println!("[STT-ERR] Groq STT failed, falling back to local (slower, may reorder turns): {e}");
                    mark_path(2);
                }
            }
        } else {
            tracing::info!("stt: no groq key, using local whisper directly");
            mark_path(3);
        }
    }

    // Fallback: local sidecar returns plain text (no segment metadata).
    match transcribe_local(samples, client).await {
        Ok(text) => Ok((text, Vec::new())),
        Err(e) => {
            mark_path(7);
            Err(e)
        }
    }
}

/// Transcribe using the local faster-whisper Python sidecar (port 39217).
pub async fn transcribe_local(samples: &[i16], client: &reqwest::Client) -> Result<String, String> {
    // Ensure the STT server is running (lazy start)
    crate::lazy_stt::ensure_stt_running();
    crate::lazy_stt::mark_stt_request();

    let mut ready = false;
    for attempt in 0..40 {
        let health = client
            .get("http://127.0.0.1:39217/health")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;

        if let Ok(resp) = health {
            if resp.status().is_success() {
                ready = true;
                break;
            }
        }

        if attempt == 0 {
            tracing::info!("stt: waiting for local STT server to be ready...");
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    if !ready {
        return Err("STT server did not become ready in 20s".to_string());
    }

    let wav_bytes = encode_wav_for_upload(samples, 16000)?;
    tracing::info!("stt: sending {} bytes WAV to {}", wav_bytes.len(), STT_URL);

    let part = reqwest::multipart::Part::bytes(wav_bytes)
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|e| format!("MIME error: {}", e))?;
    let form = reqwest::multipart::Form::new().part("audio", part);

    let resp = client
        .post(STT_URL)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("STT server request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        tracing::error!("stt: server returned {} : {}", status, body);
        return Err(format!("STT server error {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("STT response parse error: {}", e))?;

    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let filtered = filter_transcript_counted(&text);

    mark_filtered(&text, &filtered);
    if filtered != text {
        tracing::info!("stt: filtered hallucination '{}' -> '{}'", text, filtered);
    } else {
        tracing::info!("stt: local transcript: '{}'", filtered);
    }

    Ok(filtered)
}
// NOTE: stt_status was deleted (audit) — its only frontend caller
// (stt.ts sttStatus) was dead code, and the IPC registration with it.

/// Convert raw i16 PCM samples to a WAV file (16-bit, mono, given sample rate).
pub(crate) fn pcm_to_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
    let num_samples = samples.len();
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let mut wav = Vec::with_capacity(44 + data_size);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size as u32).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes());  // audio format = PCM
    wav.extend_from_slice(&1u16.to_le_bytes());  // num channels = mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    wav.extend_from_slice(&2u16.to_le_bytes());  // block align
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(data_size as u32).to_le_bytes());

    // PCM samples (little-endian i16)
    for &s in samples {
        wav.extend_from_slice(&s.to_le_bytes());
    }

    wav
}

/// Encode 16-bit mono PCM samples to a WAV byte buffer for upload to STT engines.
/// Returns an error if the sample buffer is empty (0 samples).
pub(crate) fn encode_wav_for_upload(samples: &[i16], sample_rate: u32) -> Result<Vec<u8>, String> {
    if samples.is_empty() {
        return Err("cannot encode empty audio buffer (0 samples)".to_string());
    }
    Ok(pcm_to_wav(samples, sample_rate))
}

/// Confabulation counters (P0 deployment SLO: filter hits / total).
/// Incremented by `filter_transcript_counted` on every production STT
/// turn; read via the `stt_filter_stats` IPC command. Process-lifetime
/// counts — reset on restart (rates, not absolutes, are the SLO).
static FILTER_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FILTER_HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Filter + count one transcript. Drop-in for `apply_hallucination_filter`
/// at production call sites (tests keep calling the pure fn directly).
/// May this audio go to Groq? Privacy mode (`localSttOnly`) never reaches the cloud.
/// Pure + unit-tested so the routing cannot silently regress again.
pub fn groq_allowed(local_only: bool, groq_key_empty: bool) -> bool {
    !local_only && !groq_key_empty
}

pub fn filter_transcript_counted(text: &str) -> String {
    use std::sync::atomic::Ordering;
    FILTER_TOTAL.fetch_add(1, Ordering::Relaxed);
    let out = match evaluate_transcript(text) {
        Ok(kept) => kept,
        Err(code) => {
            LAST_FILTER.store(code, Ordering::Relaxed);
            // Live miss 2026-10-19: "practice unattended shut down." was blanked by the dictionary
            // gate and the user only saw "transcript = ''". Say what was heard and why it was dropped.
            #[cfg(not(test))]
            if !text.trim().is_empty() {
                let reason = last_filter_str();
                println!("[STT-DROP] heard '{}' -> blocked by {}", text.trim(), reason);
                crate::missed_intent_logger::log_missed_intent(text, "stt_filter", reason);
            }
            String::new()
        }
    };
    if out.is_empty() {
        FILTER_HITS.fetch_add(1, Ordering::Relaxed);
    }
    out
}

/// (total_captures, filtered_empty) for the SLO board. Rate =
/// filtered / total; alert when sustained > 1.5% (confabulation storm
/// or dead mic feeding noise into Groq).
pub fn hallucination_stats() -> (u64, u64) {
    use std::sync::atomic::Ordering;
    (
        FILTER_TOTAL.load(Ordering::Relaxed),
        FILTER_HITS.load(Ordering::Relaxed),
    )
}

/// IPC: read the confabulation counters (SLO dashboard / `nexus check`).
#[tauri::command]
pub fn stt_filter_stats() -> (u64, u64) {
    hallucination_stats()
}

/// True if the word list is k≥2 repeats of one identical block (block ≥2
/// words). Pure — matrix-tested below.
fn is_repetition_loop(words: &[&str]) -> bool {
    let n = words.len();
    if n < 4 {
        return false;
    }
    // Safety-critical repeats must ALWAYS pass: a 4× "stop" / "no" is a
    // human hammering cancel/decline, and filtering it to "" would swallow
    // the drill-stop and the voice-decline (empty never reaches them).
    let stripped: Vec<String> = words
        .iter()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphabetic())
                .to_lowercase()
        })
        .collect();
    if stripped.iter().all(|w| w == &stripped[0])
        && ["stop", "cancel", "exit", "quit", "no"].contains(&stripped[0].as_str())
    {
        return false;
    }
    for block in 2..=n / 2 {
        if n % block != 0 {
            continue;
        }
        if words.chunks(block).all(|c| c == &words[..block]) {
            return true;
        }
    }
    false
}

/// Common English + assistant command vocabulary. The dictionary gate only
/// fires on 3+ word transcripts, so single-word commands never consult it —
/// but multi-word commands ("open chat with mummy", "switch to tab 3")
/// must still pass, hence command verbs, app nouns and connectors live here.
const COMMON_WORDS: &[&str] = &[
    // articles / prepositions / pronouns
    "a", "an", "the", "to", "for", "in", "on", "and", "or", "of", "me",
    "you", "i", "it", "this", "that", "my", "your", "with", "from", "is",
    "are", "was", "what", "when", "where", "who", "how", "which", "there",
    "here", "out", "up", "so", "no", "not", "do", "does", "did", "can",
    "will", "would", "could", "should", "please", "thanks", "thank", "hello",
    "hi", "hey", "ok", "okay", "yes", "sir", "i'm", "im", "i've", "don't",
    "dont", "it's", "its", "s", "t",
    // more pure function words: ordinary statements ("I work at Google as a software
    // engineer") scored 2/8 on the old list and were blanked as "gibberish".
    "at", "as", "by", "about", "after", "all", "also", "am", "any", "because", "but", "he",
    "she", "her", "him", "his", "if", "into", "just", "know", "like", "more", "now", "only",
    "our", "over", "some", "than", "them", "then", "they", "their", "these", "those", "we",
    "were", "had", "has", "have", "us", "work", "my", "very", "much", "too", "about",
    // command verbs
    "open", "close", "start", "stop", "play", "pause", "press", "type",
    "send", "search", "create", "delete", "switch", "move", "go", "show",
    "set", "turn", "lock", "mute", "launch", "run", "quit", "exit",
    "kill", "message", "chat", "call", "tell", "say", "repeat", "cancel",
    "confirm", "check", "analyse", "analyze", "review", "map", "take",
    "make", "give", "put", "saying", "said", "told", "be", "been", "late",
    "early", "going", "get", "getting", "will",
    // app / domain nouns
    "new", "tab", "ghost", "mode", "nexus", "app", "browser", "whatsapp",
    "chrome", "brave", "spotify", "youtube", "music", "song", "repo",
    "pr", "branch", "code", "architect", "architecture", "settings",
    "timer", "alarm", "screenshot", "video", "mail", "email",
    "window", "screen", "volume", "mic", "time", "date", "news",
    "weather",
    // power / session vocabulary: "shut down my laptop" scored 0.25 and "practice unattended
    // shutdown" 0.00, so ordinary requests were blanked as gibberish (live miss 2026-10-19).
    "shut", "down", "off", "power", "restart", "reboot", "sleep", "hibernate", "laptop", "computer",
    "pc", "battery", "percentage", "charge", "charging", "practice", "unattended", "allow",
    "enable", "disable", "shutdown", "brightness", "watch", "session", "done", "until", "finished",
    "finish", "complete", "perfect", "wifi", "bluetooth", "level", "status",
];

/// Fraction of whitespace tokens (letters only, lowercased) found in
/// `COMMON_WORDS`. Pure — matrix-tested below.
fn english_token_ratio(words: &[&str]) -> f32 {
    if words.is_empty() {
        return 1.0;
    }
    let hits = words
        .iter()
        .filter(|w| {
            let clean: String = w
                .chars()
                .filter(|c| c.is_alphabetic() || *c == '\'')
                .collect::<String>()
                .to_lowercase();
            COMMON_WORDS.contains(&clean.as_str())
        })
        .count();
    hits as f32 / words.len() as f32
}

/// Normalize punctuation Whisper may emit so the English-compatibility gate
/// does not mistake typography for another language.
fn normalize_english_punctuation(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '‘' | '’' | '‚' | '‛' => '\'',
            '“' | '”' | '„' | '‟' => '"',
            '–' | '—' | '―' => '-',
            '…' => '.',
            _ => c,
        })
        .collect()
}

/// English-only output gate. Translated or non-English source audio is not
/// actionable even when Whisper renders it in Latin characters; script and
/// lexical provenance gates happen downstream. This gate rejects text that
/// cannot be represented with ordinary English letters.
fn is_english_compatible_text(text: &str) -> bool {
    let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return true;
    }
    letters.iter().all(|c| c.is_ascii_alphabetic())
}

/// True when the STT hallucination filter would blank this text (memory hygiene reuses the
/// exact same rules instead of keeping a second blocklist).
pub(crate) fn looks_hallucinated(text: &str) -> bool {
    !text.trim().is_empty() && apply_hallucination_filter(text).trim().is_empty()
}

/// Filter common Whisper hallucinations on noisy/silent audio.
/// `contains` entries are multi-word junk that never forms a command;
/// `exact` entries are single fragments Whisper emits on pure noise
/// (measured 2026-09-19 on room/TV/conversation clips with nobody speaking:
/// ' Thank you.', ' you', ' BOOAAA!', " It's a", ' out.', ' Oh').
/// Kept OUT deliberately: "i'm sorry" / "i'm not" (dictation-plausible).
fn apply_hallucination_filter(text: &str) -> String {
    evaluate_transcript(text).unwrap_or_default()
}

/// True when NEXUS has a grammar for this sentence. Gibberish and background chatter never parse,
/// so a parse is the strongest proof that the words are a real command, whatever the word list says.
fn is_known_command(text: &str) -> bool {
    crate::intent_parser::parse_deterministic(text).is_some()
        || crate::power_plan::parse_command(text).is_some()
        || crate::watch_power::parse(text).is_some()
        || crate::power_cmd::parse_weak(text).is_some()
        || crate::power_cmd::parse_casual(text).is_some()
}

/// `Ok(kept text)` or `Err(reason code)`.
fn evaluate_transcript(text: &str) -> Result<String, u8> {
    let normalized = normalize_english_punctuation(text);
    if !is_english_compatible_text(&normalized) {
        return Err(R_NON_ASCII);
    }
    let lower = normalized.to_lowercase();
    let hallucinations = [
        "thank you for watching", "thanks for watching", "thank you.", "you.", "bye.",
        "please subscribe", "subscribe to my channel", "booaaa", "and the world is coming",
        "cunzon usarlo", "sarese manguer", "tadrao vara", "yeraddy",
        "subtitles by", "translated by", "all rights reserved",
    ];

    for h in hallucinations.iter() {
        if lower.contains(h) {
            return Err(R_PHRASE);
        }
    }

    let exact_fragments = [
        "you", "oh", "out", "it's a", "thank you", "thanks", "from the", "in the",
        "of the", "to the", "and then", "and the", "on the", "open drift", "open breath",
    ];
    let bare = lower.trim().trim_end_matches(|c: char| matches!(c, '.' | ',' | '?' | '!'));
    if exact_fragments.contains(&bare) {
        return Err(R_FRAGMENT);
    }

    // Repetition hallucination loop guard, n× general form: any full-phrase
    // repeat with k≥2 identical blocks ("Open breath. Open breath.",
    // "I'm gonna say that." ×3). The old check required an even word count
    // with exact half-split equality, so 3× loops (12 words, uneven thirds)
    // sailed through to cloud chat (live miss 2026-09-29 21:00).
    // Blocks are ≥2 words: single-word repeats ("stop stop stop stop")
    // pass through so emphatic real commands still work.
    let words: Vec<&str> = lower.split_whitespace().collect();
    if is_repetition_loop(&words) {
        tracing::info!("stt: filtered repetition loop hallucination: '{}'", text);
        return Err(R_REPEAT);
    }

    // Foreign-script guard (measured 2026-09-19: TV anime audio transcribed
    // as Japanese flowed through as a "command", got answered in Japanese,
    // and crashed Piper). 2+ CJK/Hiragana/Katakana/Hangul chars = background
    // media or non-English speech the English pipeline cannot act on → retry
    // prompt, never an action. Shorter fragments fall to the min-length rule
    // below, same safe direction.
    let cjk_count = text
        .chars()
        .filter(|c| {
            matches!(c,
                '\u{3040}'..='\u{30FF}'   // Hiragana + Katakana
                | '\u{4E00}'..='\u{9FFF}' // CJK Unified Ideographs
                | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
            )
        })
        .count();
    if cjk_count >= 2 {
        return Err(R_CJK);
    }

    // Latin-gibberish dictionary gate (live miss 2026-09-29 21:00:
    // "Tentang Otor. Post Immortal." — background audio routed to cloud
    // chat and spoken back). 3+ words with <30% common-English tokens is
    // not a command in any supported phrasing → retry. Short transcripts
    // skip this (single-word commands + the P1 validity gate own those).
    // NOTE: no_speech_prob veto deliberately NOT used here — measured
    // dormant (0.000 on every segment with whisper-large-v3-turbo, see
    // verify_confidence); wiring it would add latency for zero effect.
    let word_count = words.len();
    if word_count >= 3 && english_token_ratio(&words) < 0.30 && !is_known_command(&normalized) {
        tracing::info!("stt: filtered non-English/background gibberish: '{}'", text);
        return Err(R_DICT);
    }

    let alpha_count = normalized.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 2 {
        return Err(R_SHORT);
    }

    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::apply_hallucination_filter;
    use super::{filter_transcript_counted, hallucination_stats};

    #[test]
    fn privacy_mode_never_reaches_the_cloud() {
        assert!(!super::groq_allowed(true, false), "localSttOnly must block Groq even with a key");
        assert!(!super::groq_allowed(true, true));
        assert!(super::groq_allowed(false, false));
        assert!(!super::groq_allowed(false, true), "no key => no cloud");
    }

    #[test]
    fn confidence_summary_takes_worst_case_across_segments() {
        use crate::stt_groq::GroqSegment;
        let seg = |lp: f32, ns: f32| GroqSegment { text: String::new(), avg_logprob: lp, no_speech_prob: ns };
        assert_eq!(super::confidence_summary(&[]), None);
        let (lp, ns) = super::confidence_summary(&[seg(-0.2, 0.01), seg(-0.9, 0.05), seg(-0.4, 0.30)]).unwrap();
        assert!((lp - -0.9).abs() < 1e-6 && (ns - 0.30).abs() < 1e-6);
    }

    #[test]
    fn confidence_line_is_greppable_and_handles_missing_segments() {
        use crate::stt_groq::GroqSegment;
        let line = super::format_confidence_line(
            " open chrome ",
            &[GroqSegment { text: "open chrome".into(), avg_logprob: -0.31, no_speech_prob: 0.02 }],
            16000,
        );
        assert_eq!(
            line,
            "[STT-CONF] dur=1.00s words=2 segs=1 min_avg_logprob=-0.310 max_no_speech=0.020 text='open chrome'"
        );
        let none = super::format_confidence_line("hello", &[], 8000);
        assert!(none.contains("segs=0 (no confidence info)") && none.contains("dur=0.50s"));
    }

    /// Counted wrapper preserves filter semantics and moves the SLO
    /// counters exactly once per turn (empty → hit, speech → total only).
    #[test]
    fn test_counted_wrapper_moves_counters() {
        let (t0, h0) = hallucination_stats();
        assert_eq!(filter_transcript_counted(" Thank you."), "");
        assert_eq!(filter_transcript_counted("Open WhatsApp."), "Open WhatsApp.");
        let (t1, h1) = hallucination_stats();
        assert_eq!(t1 - t0, 2, "two counted turns");
        assert_eq!(h1 - h0, 1, "one filter hit");
    }

    /// Turn-packet markers (approach A): reset → codes → wire strings.
    /// The capture receiver is the sole reader; tests pin the mapping.
    #[test]
    fn test_turn_markers_roundtrip() {
        use super::{last_filter_str, last_stt_path_str, reset_turn_markers};
        use super::{LAST_FILTER, LAST_STT_PATH};
        use std::sync::atomic::Ordering;
        reset_turn_markers();
        assert_eq!(last_stt_path_str(), "unknown");
        assert_eq!(last_filter_str(), "none");
        LAST_STT_PATH.store(1, Ordering::Relaxed);
        assert_eq!(last_stt_path_str(), "groq");
        LAST_STT_PATH.store(2, Ordering::Relaxed);
        assert_eq!(last_stt_path_str(), "groq_err_local");
        LAST_STT_PATH.store(5, Ordering::Relaxed);
        assert_eq!(last_stt_path_str(), "short_reject");
        LAST_STT_PATH.store(7, Ordering::Relaxed);
        assert_eq!(last_stt_path_str(), "local_err");
        LAST_FILTER.store(1, Ordering::Relaxed);
        assert_eq!(last_filter_str(), "hallucination");
        reset_turn_markers();
        assert_eq!(last_stt_path_str(), "unknown");
        assert_eq!(last_filter_str(), "none");
    }

    /// Measured noise confabulations (2026-09-19 repro on pure background
    /// clips) are filtered; real commands and dictation pass through.
    #[test]
    fn test_noise_fragments_filtered() {
        for junk in [
            " Thank you.",
            " you",
            " BOOAAA!",
            " It's a",
            " out.",
            " Oh",
            " and the world is coming.",
        ] {
            assert_eq!(apply_hallucination_filter(junk), "", "'{junk}' must filter");
        }
    }

    #[test]
    fn test_real_speech_passes() {
        for real in [
            "Open WhatsApp.",
            "Nexus.",
            "type I'm sorry you feel bad",
            "What's up?",
        ] {
            assert_eq!(apply_hallucination_filter(real), real);
        }
    }

    #[test]
    fn test_english_typography_is_normalized_not_rejected() {
        assert_eq!(
            apply_hallucination_filter("Open “WhatsApp”…"),
            "Open \"WhatsApp\"."
        );
    }

    #[test]
    fn test_non_latin_letters_are_rejected() {
        assert_eq!(apply_hallucination_filter("café society"), "");
        assert_eq!(apply_hallucination_filter("naïve response"), "");
    }

    /// Foreign-script guard: TV-anime Japanese (the exact 2026-09-19
    /// crash transcript) filters to retry; English + single stray chars pass.
    #[test]
    fn test_foreign_script_filtered() {
        assert_eq!(
            apply_hallucination_filter("治療が縮まれ出ている。"),
            ""
        );
        assert_eq!(
            apply_hallucination_filter("治療が短くなっていると感じているのですね。"),
            ""
        );
        assert_eq!(apply_hallucination_filter("Open WhatsApp."), "Open WhatsApp.");
        // single stray CJK char: caught by the min-length rule → retry.
        assert_eq!(apply_hallucination_filter("愛"), "");
    }

    /// n× repetition loops (live misses 2026-09-29 21:00): 2× half-splits
    /// AND 3×+ loops filter; emphatic single-word repeats still pass.
    #[test]
    fn test_repetition_loop_nx() {
        for junk in [
            "Open breath. Open breath.",
            "I'm gonna say that. I'm gonna say that. I'm gonna say that.",
            "hello hello hello hello",
        ] {
            assert_eq!(apply_hallucination_filter(junk), "", "'{junk}' must filter");
        }
        // Emphatic real speech passes (block-size-1 repeats allowed, and
        // safety-critical repeats never filter — see is_repetition_loop).
        assert_eq!(
            apply_hallucination_filter("stop stop stop stop"),
            "stop stop stop stop"
        );
        assert_eq!(apply_hallucination_filter("no no no no"), "no no no no");
    }

    /// Live miss 2026-10-19 15:05: the user said "practice unattended shutdown" three times and the
    /// dictionary gate blanked every transcript (0 of 4 words in the list), so NEXUS said "didn't catch
    /// that" although Groq had heard the words almost exactly.
    #[test]
    fn real_commands_the_word_list_does_not_know_are_not_blanked() {
        for real in [
            "practice unattended shut down.",
            "allow unattended shutdown",
            "turn off unattended shutdown",
            "shut down my laptop",
            "shut down the laptop",
            "power off the computer",
            "watch this session until it's done",
        ] {
            assert_eq!(apply_hallucination_filter(real), real.trim(), "'{real}' is a command and must pass");
        }
        // ordinary questions made of words the old list lacked
        assert_eq!(
            apply_hallucination_filter("what's the battery percentage"),
            "what's the battery percentage"
        );
    }

    /// The word list can never cover every title, name and place a command contains. These phrases are
    /// accepted by a grammar yet score under 30% on the word list, so ONLY the grammar bypass saves them
    /// (this is what makes the bypass non-vacuous: remove it and this test fails).
    #[test]
    fn a_grammar_match_saves_commands_made_of_names() {
        use super::{english_token_ratio, evaluate_transcript, is_known_command};
        for cmd in [
            "order paneer butter masala",
            "order chicken biryani paradise",
            "search kesariya arijit singh lyrics",
            "ask claude to fix kesariya arijit singh",
            "find kesariya arijit singh",
        ] {
            let words: Vec<&str> = cmd.split_whitespace().collect();
            assert!(english_token_ratio(&words) < 0.30, "'{cmd}' must be unknown to the word list for this test to mean anything");
            assert!(is_known_command(cmd), "'{cmd}' must parse");
            assert_eq!(evaluate_transcript(cmd), Ok(cmd.to_string()), "'{cmd}'");
        }
    }

    /// The bypass is "NEXUS has a grammar for it", so background audio and near-misses of the grammar
    /// still go through the gate and get a specific reason.
    #[test]
    fn the_gate_still_blocks_what_no_grammar_accepts() {
        use super::{evaluate_transcript, R_DICT};
        for junk in [
            "Tentang Otor. Post Immortal.",
            "Cunzon usarlo sarese manguer",
            "Souch Mamek tadrao vara yeraddy",
            "Raktis unobtended chaman",
        ] {
            assert!(evaluate_transcript(junk).is_err(), "'{junk}' must still be blocked");
            assert!(!super::is_known_command(junk), "'{junk}' must not parse as a command");
        }
        for junk in ["Tentang Otor. Post Immortal.", "Raktis unobtended chaman"] {
            assert_eq!(evaluate_transcript(junk), Err(R_DICT), "'{junk}' must be blocked by the dictionary gate");
        }
        // "Raktis unattended shutdown." (the live mishearing of "practice ...") is mostly ordinary words, so
        // it now passes the word-ratio gate; no grammar accepts it, so it can never act, it only reaches chat.
        assert!(evaluate_transcript("Raktis unattended shutdown.").is_ok());
        assert!(!super::is_known_command("Raktis unattended shutdown."));
    }

    /// Every blank carries its own reason (shown in `stt:turn_stats` and the `[STT-DROP]` line).
    #[test]
    fn each_blocked_transcript_names_its_reason() {
        use super::{evaluate_transcript, R_CJK, R_DICT, R_FRAGMENT, R_NON_ASCII, R_PHRASE, R_REPEAT, R_SHORT};
        assert_eq!(evaluate_transcript("café society"), Err(R_NON_ASCII));
        assert_eq!(evaluate_transcript("Thank you for watching"), Err(R_PHRASE));
        assert_eq!(evaluate_transcript("from the"), Err(R_FRAGMENT));
        assert_eq!(evaluate_transcript("Open breath. Open breath."), Err(R_REPEAT));
        assert_eq!(evaluate_transcript("Tentang Otor. Post Immortal."), Err(R_DICT));
        assert_eq!(evaluate_transcript("x"), Err(R_SHORT));
        assert!(evaluate_transcript("open brave").is_ok());
        let _ = R_CJK;
    }

    #[test]
    fn the_counted_wrapper_stores_the_specific_reason() {
        use super::{last_filter_str, reset_turn_markers};
        reset_turn_markers();
        assert_eq!(filter_transcript_counted("Tentang Otor. Post Immortal."), "");
        assert_eq!(last_filter_str(), "dict_gate");
        reset_turn_markers();
        assert_eq!(filter_transcript_counted("practice unattended shut down."), "practice unattended shut down.");
        assert_eq!(last_filter_str(), "none");
        reset_turn_markers();
    }

    /// Latin-gibberish dictionary gate: background audio with (almost) no
    /// English tokens filters; every real command family passes, including
    /// contacts/repos the dictionary can't know (ratio, not blacklist).
    #[test]
    fn test_latin_gibberish_gate() {
        for junk in [
            "Tentang Otor. Post Immortal.",
            "Cunzon usarlo sarese manguer",
            "Souch Mamek tadrao vara yeraddy",
        ] {
            assert_eq!(apply_hallucination_filter(junk), "", "'{junk}' must filter");
        }
        for real in [
            "open brave",
            "open a new tab",
            "switch to tab 3",
            "close tab 5",
            "ghost mode",
            "message mummy",
            "open chat with mom",
            "type hello world",
            "press enter",
            "what is the capital of france",
            "analyse PR 24 in zync",
            "send mom a whatsapp message saying I'll be late",
        ] {
            assert_eq!(apply_hallucination_filter(real), real, "'{real}' must pass");
        }
    }

    /// Task ST: Full truth table for choose_route (8 combinations).
    #[test]
    fn test_choose_route_truth_table_all_combinations() {
        use super::{choose_route, SttEngine};

        // Privacy mode always yields only Local
        assert_eq!(choose_route(true, true, true), vec![SttEngine::Local]);
        assert_eq!(choose_route(true, true, false), vec![SttEngine::Local]);
        assert_eq!(choose_route(true, false, true), vec![SttEngine::Local]);
        assert_eq!(choose_route(true, false, false), vec![SttEngine::Local]);

        // Non-privacy routes based on available provider keys
        assert_eq!(
            choose_route(false, true, true),
            vec![SttEngine::Deepgram, SttEngine::Groq, SttEngine::Local]
        );
        assert_eq!(
            choose_route(false, true, false),
            vec![SttEngine::Deepgram, SttEngine::Local]
        );
        assert_eq!(
            choose_route(false, false, true),
            vec![SttEngine::Groq, SttEngine::Local]
        );
        assert_eq!(
            choose_route(false, false, false),
            vec![SttEngine::Local]
        );
    }

    /// Task ST: Privacy guarantee — local_only == true NEVER yields a cloud step.
    #[test]
    fn test_privacy_mode_never_yields_cloud_step() {
        use super::{choose_route, SttEngine};
        for has_deepgram in [false, true] {
            for has_groq in [false, true] {
                let plan = choose_route(true, has_deepgram, has_groq);
                assert!(
                    !plan.contains(&SttEngine::Deepgram),
                    "privacy mode must never route to Deepgram (keys: dg={has_deepgram}, groq={has_groq})"
                );
                assert!(
                    !plan.contains(&SttEngine::Groq),
                    "privacy mode must never route to Groq (keys: dg={has_deepgram}, groq={has_groq})"
                );
                assert_eq!(plan, vec![SttEngine::Local]);
            }
        }
    }

    /// Task ST: Fallback decision on transient/server errors vs client/invalid audio errors.
    #[test]
    fn test_should_fall_back_classification() {
        use super::{should_fall_back, SttError};

        // Network error -> fall back
        assert!(should_fall_back(&SttError::Network("connection reset".to_string())));
        // 429 Rate limit -> fall back
        assert!(should_fall_back(&SttError::RateLimited));
        // 401 Unauthorized -> fall back
        assert!(should_fall_back(&SttError::Unauthorized));
        // 5xx Server error -> fall back
        assert!(should_fall_back(&SttError::ServerError(500)));
        assert!(should_fall_back(&SttError::ServerError(502)));
        assert!(should_fall_back(&SttError::ServerError(503)));
        // Timeout -> fall back
        assert!(should_fall_back(&SttError::Timeout));
        // Empty cloud transcript -> fall back
        assert!(should_fall_back(&SttError::EmptyCloudTranscript));
        // Generic cloud error -> fall back
        assert!(should_fall_back(&SttError::Other("unknown error".to_string())));

        // Invalid audio or too short -> do NOT call downstream engines
        assert!(!should_fall_back(&SttError::InvalidAudio("unsupported format".to_string())));
        assert!(!should_fall_back(&SttError::TooShort { samples: 100, min_samples: 3200 }));

        // Test parser from message
        assert_eq!(SttError::from_error_message("429 Too Many Requests"), SttError::RateLimited);
        assert_eq!(SttError::from_error_message("Groq rate limit hit (2,000 req/day free tier)"), SttError::RateLimited);
        assert_eq!(SttError::from_error_message("Groq API key invalid — check Settings"), SttError::Unauthorized);
        assert_eq!(SttError::from_error_message("401 Unauthorized"), SttError::Unauthorized);
        assert_eq!(SttError::from_error_message("Groq STT error 500: internal server error"), SttError::ServerError(500));
        assert_eq!(SttError::from_error_message("Request timed out after 15s"), SttError::Timeout);
        assert_eq!(SttError::from_error_message("empty transcript received"), SttError::EmptyCloudTranscript);
        assert_eq!(SttError::from_error_message("Invalid audio: not 16 kHz mono"), SttError::InvalidAudio("Invalid audio: not 16 kHz mono".into()));
        // a provider rejecting the file is not our validation: it must still fall back to the local engine
        let provider = SttError::from_error_message("Groq STT error 400: invalid audio mime type");
        assert!(should_fall_back(&provider), "{provider:?}");
        assert!(should_fall_back(&SttError::from_error_message("mime error from provider")));
        assert_eq!(SttError::from_error_message("audio buffer too short"), SttError::TooShort { samples: 0, min_samples: 3200 });
        assert_eq!(SttError::from_error_message("Empty audio"), SttError::TooShort { samples: 0, min_samples: 3200 });
    }

    /// Task ST: Pre-gate boundary tests at exactly 3199, 3200, 3201 samples.
    #[test]
    fn test_pre_gate_sample_count_boundaries() {
        use super::{check_audio_pre_gate, AudioPreGate};

        // Constant amplitude 164 gives RMS = 164 / 32768 = ~0.0050049 > 0.005
        let make_buf = |len: usize| vec![164i16; len];

        assert_eq!(
            check_audio_pre_gate(&make_buf(3199)),
            AudioPreGate::TooShort,
            "3199 samples is below 3200 minimum (200ms at 16kHz)"
        );
        assert_eq!(
            check_audio_pre_gate(&make_buf(3200)),
            AudioPreGate::Pass,
            "3200 samples exactly meets 200ms threshold"
        );
        assert_eq!(
            check_audio_pre_gate(&make_buf(3201)),
            AudioPreGate::Pass,
            "3201 samples passes length threshold"
        );
    }

    /// Task ST: Pre-gate RMS energy boundary tests at 0.00499, 0.005, 0.00501.
    #[test]
    fn test_pre_gate_rms_energy_boundaries() {
        use super::{audio_rms, check_audio_pre_gate, AudioPreGate};

        // Construct 3200-sample buffers with precise RMS values:
        // Amplitude 163 -> RMS = 163 / 32768.0 = 0.004974 < 0.005
        let low_buf = vec![163i16; 3200];
        let low_rms = audio_rms(&low_buf);
        assert!(low_rms < 0.005, "low_rms must be < 0.005: {low_rms}");
        assert_eq!(check_audio_pre_gate(&low_buf), AudioPreGate::LowEnergy);

        // Amplitude 164 -> RMS = 164 / 32768.0 = 0.00500488 > 0.005
        let pass_buf = vec![164i16; 3200];
        let pass_rms = audio_rms(&pass_buf);
        assert!(pass_rms >= 0.005, "pass_rms must be >= 0.005: {pass_rms}");
        assert_eq!(check_audio_pre_gate(&pass_buf), AudioPreGate::Pass);

        // Amplitude 200 -> RMS = 200 / 32768.0 = 0.00610 > 0.00501
        let high_buf = vec![200i16; 3200];
        assert_eq!(check_audio_pre_gate(&high_buf), AudioPreGate::Pass);
    }

    /// Task ST: Silence, click, empty buffer, and NaN handling in pre-gate.
    #[test]
    fn test_pre_gate_silence_click_nan_empty_buffers() {
        use super::{audio_rms, check_audio_pre_gate, AudioPreGate};

        // Silence (all zeros) -> low energy
        let silence = vec![0i16; 3200];
        assert_eq!(audio_rms(&silence), 0.0);
        assert_eq!(check_audio_pre_gate(&silence), AudioPreGate::LowEnergy);

        // Single click impulse: 1 sample at 2000 amplitude, rest 3199 silence
        // RMS = sqrt((2000/32768)^2 / 3200) = ~0.00108 < 0.005
        let mut click = vec![0i16; 3200];
        click[0] = 2000;
        let click_rms = audio_rms(&click);
        assert!(click_rms < 0.005, "single click RMS {click_rms} should be < 0.005");
        assert_eq!(check_audio_pre_gate(&click), AudioPreGate::LowEnergy);

        // Empty buffer -> TooShort, no panic
        assert_eq!(audio_rms(&[]), 0.0);
        assert_eq!(check_audio_pre_gate(&[]), AudioPreGate::TooShort);
    }

    /// Task ST: Hallucination filter table of 40 inputs (20 removed, 20 kept).
    /// Reviews "thank you" behavior: isolated "thank you" is filtered as an exact noise fragment,
    /// while genuine conversational responses like "thank you very much" and "thank you nexus" pass.
    #[test]
    fn test_hallucination_filter_40_cases_table() {
        use super::apply_hallucination_filter;

        // 20 inputs that MUST be removed (blanked to "")
        let removed_cases = [
            "thank you for watching",
            "thanks for watching",
            "please subscribe",
            "subscribe to my channel",
            "all rights reserved",
            "subtitles by",
            "translated by",
            "booaaa",
            "and the world is coming",
            // Isolated "thank you" / "thanks" are Whisper noise fragments on silence/TV
            "thank you",
            "thanks",
            "from the",
            "to the",
            "open breath. open breath.",
            "open notepad open notepad open notepad",
            "I'm gonna say that. I'm gonna say that. I'm gonna say that.",
            "café society",
            "治療が短くなっている",
            "12345",
            "Tentang Otor. Post Immortal.",
        ];

        for junk in removed_cases {
            assert_eq!(
                apply_hallucination_filter(junk),
                "",
                "Hallucination filter must remove '{junk}'"
            );
        }

        // 20 real commands and genuine phrases that MUST be kept
        let kept_cases = [
            "open notepad",
            "open brave",
            "open a new tab",
            "switch to tab 3",
            "close tab 5",
            "ghost mode",
            "shut down my laptop",
            "restart the computer",
            "type hello world",
            "press enter",
            "what time is it",
            "what's the weather today",
            "order food",
            "analyse PR 24 in zync",
            "stop stop stop stop",
            "cancel cancel cancel cancel",
            "no no no no",
            // Genuine conversational replies containing gratitude with context pass through
            "thank you very much",
            "thank you nexus",
            "practice unattended shutdown",
        ];

        for cmd in kept_cases {
            let res = apply_hallucination_filter(cmd);
            assert!(
                !res.is_empty(),
                "Hallucination filter must keep valid command '{cmd}'"
            );
        }
    }

    /// Task ST: Repetition loops of different lengths (2x2, 3x2, 2x3, 3x3, 2x4, 3x4).
    /// Emphatic safety-critical single-word repeats must pass.
    #[test]
    fn test_hallucination_filter_repetition_loops_various_lengths() {
        use super::apply_hallucination_filter;

        // 2-word block, 2 repeats (len 4)
        assert_eq!(apply_hallucination_filter("open drift open drift"), "");
        // 2-word block, 3 repeats (len 6)
        assert_eq!(apply_hallucination_filter("open notepad open notepad open notepad"), "");
        // 3-word block, 2 repeats (len 6)
        assert_eq!(apply_hallucination_filter("open a tab open a tab"), "");
        // 3-word block, 3 repeats (len 9)
        assert_eq!(apply_hallucination_filter("switch the tab switch the tab switch the tab"), "");
        // 4-word block, 2 repeats (len 8)
        assert_eq!(apply_hallucination_filter("what is the time what is the time"), "");
        // 4-word block, 3 repeats (len 12)
        assert_eq!(
            apply_hallucination_filter("I'm gonna say that. I'm gonna say that. I'm gonna say that."),
            ""
        );

        // Safety-critical single word repeats pass
        assert_eq!(apply_hallucination_filter("stop stop stop stop"), "stop stop stop stop");
        assert_eq!(apply_hallucination_filter("cancel cancel cancel cancel"), "cancel cancel cancel cancel");

        // Non-safety single-word repeats filter
        assert_eq!(apply_hallucination_filter("hello hello hello hello"), "");
    }

    /// Task ST: Mixed case and punctuation normalization in hallucination filter.
    #[test]
    fn test_hallucination_filter_mixed_case_and_punctuation() {
        use super::apply_hallucination_filter;

        assert_eq!(apply_hallucination_filter("OPEN NOTEPAD"), "OPEN NOTEPAD");
        assert_eq!(apply_hallucination_filter("Open Brave."), "Open Brave.");
        assert_eq!(apply_hallucination_filter("What's up?"), "What's up?");
        // Typographical punctuation normalization
        assert_eq!(apply_hallucination_filter("Open “WhatsApp”…"), "Open \"WhatsApp\".");
    }

    /// Task ST: Text with only digits is rejected (fewer than 2 alphabetic characters).
    #[test]
    fn test_hallucination_filter_digits_only_rejected() {
        use super::apply_hallucination_filter;

        assert_eq!(apply_hallucination_filter("12345"), "");
        assert_eq!(apply_hallucination_filter("999"), "");
        assert_eq!(apply_hallucination_filter("42"), "");
        assert_eq!(apply_hallucination_filter("0"), "");
    }

    /// Task ST: WAV encoding for upload header fields (sample rate 16000, mono, 16-bit PCM).
    #[test]
    fn test_wav_encoding_header_fields_mono_16bit_16000hz() {
        use super::encode_wav_for_upload;

        let samples = vec![100i16; 160];
        let wav = encode_wav_for_upload(&samples, 16000).expect("valid wav encoding");

        // Magic markers
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");

        // Format chunk size (16 for PCM)
        let fmt_chunk_size = u32::from_le_bytes([wav[16], wav[17], wav[18], wav[19]]);
        assert_eq!(fmt_chunk_size, 16);

        // Audio format = 1 (PCM)
        let audio_format = u16::from_le_bytes([wav[20], wav[21]]);
        assert_eq!(audio_format, 1);

        // Num channels = 1 (mono)
        let channels = u16::from_le_bytes([wav[22], wav[23]]);
        assert_eq!(channels, 1);

        // Sample rate = 16000
        let sample_rate = u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]);
        assert_eq!(sample_rate, 16000);

        // Byte rate = sample_rate * channels * bits/8 = 16000 * 1 * 16 / 8 = 32000
        let byte_rate = u32::from_le_bytes([wav[28], wav[29], wav[30], wav[31]]);
        assert_eq!(byte_rate, 32000);

        // Block align = channels * bits/8 = 1 * 16 / 8 = 2
        let block_align = u16::from_le_bytes([wav[32], wav[33]]);
        assert_eq!(block_align, 2);

        // Bits per sample = 16
        let bits_per_sample = u16::from_le_bytes([wav[34], wav[35]]);
        assert_eq!(bits_per_sample, 16);
    }

    /// Task ST: WAV encoding length field matches exact data size.
    #[test]
    fn test_wav_encoding_length_field_matches_data() {
        use super::encode_wav_for_upload;

        for count in [10, 100, 3200, 8000] {
            let samples = vec![50i16; count];
            let wav = encode_wav_for_upload(&samples, 16000).expect("valid wav");
            let data_size = (count * 2) as u32;

            // RIFF chunk size at bytes 4..8 = 36 + data_size
            let riff_size = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
            assert_eq!(riff_size, 36 + data_size);

            // Data chunk size at bytes 40..44 = data_size
            let read_data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
            assert_eq!(read_data_size, data_size);

            assert_eq!(wav.len(), 44 + (count * 2));
        }
    }

    /// Task ST: A buffer of 0 samples is refused for upload.
    #[test]
    fn test_wav_encoding_refuses_empty_buffer() {
        use super::encode_wav_for_upload;

        let empty: Vec<i16> = Vec::new();
        let res = encode_wav_for_upload(&empty, 16000);
        assert!(res.is_err(), "A buffer of 0 samples must be refused for upload");
    }

    /// Task ST: Odd sample counts work and produce correct WAV encoding.
    #[test]
    fn test_wav_encoding_odd_sizes_work() {
        use super::encode_wav_for_upload;

        for count in [1, 3199, 3201] {
            let samples = vec![42i16; count];
            let wav = encode_wav_for_upload(&samples, 16000).expect("odd size wav");
            let data_size = (count * 2) as u32;

            let riff_size = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
            assert_eq!(riff_size, 36 + data_size);

            let read_data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
            assert_eq!(read_data_size, data_size);
            assert_eq!(wav.len(), 44 + (count * 2));
        }
    }
}
