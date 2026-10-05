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

pub struct SttState {
    /// Reused HTTP client — avoids building a new reqwest::Client per transcription.
    pub client: Arc<reqwest::Client>,
}

impl SttState {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build STT HTTP client");
        Self { client: Arc::new(client) }
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
        _ => "unknown",
    }
}

pub(crate) fn last_filter_str() -> &'static str {
    match LAST_FILTER.load(std::sync::atomic::Ordering::Relaxed) {
        1 => "hallucination",
        _ => "none",
    }
}

fn mark_path(code: u8) {
    LAST_STT_PATH.store(code, std::sync::atomic::Ordering::Relaxed);
}

fn mark_filtered(original: &str, filtered: &str) {
    if filtered != original {
        LAST_FILTER.store(1, std::sync::atomic::Ordering::Relaxed);
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

    // Reject buffers shorter than 200ms (3200 samples at 16kHz)
    if samples.len() < 3200 {
        tracing::info!("stt: buffer too short ({} samples), skipping STT", samples.len());
        mark_path(5);
        return Ok("".to_string());
    }

    // Audio energy check: if below threshold, audio is room noise / silence
    let sum_sq: f64 = samples.iter().map(|&s| {
        let f = s as f64 / 32768.0;
        f * f
    }).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    if rms < 0.005 {
        tracing::info!("stt: audio energy below threshold (RMS {:.5} < 0.005), skipping STT", rms);
        mark_path(6);
        return Ok("".to_string());
    }

    // Read Groq API key from settings
    let groq_key = crate::commands::read_groq_api_key(&app);

    // Check if user has enabled "Local STT only" (privacy mode).
    // If true, audio never leaves the device — skip Groq cloud entirely.
    let local_only = crate::commands::read_local_stt_only(&app);
    if local_only {
        tracing::info!("stt: localSttOnly=true, using local whisper (privacy mode)");
        mark_path(4);
    } else if !groq_key.is_empty() {
        // Primary: Groq cloud STT (~247ms, free) — with the NEXUS domain
        // vocabulary so rare entities (Servx, Zync, Eesha) win decoder bets.
        match crate::stt_groq::transcribe_with_groq(&samples, &groq_key, &state.client, Some(crate::stt_groq::NEXUS_VOCABULARY)).await {
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
                tracing::warn!("stt: groq failed ({}), falling back to local whisper", e);
                mark_path(2);
                // Fall through to local STT
            }
        }
    } else {
        tracing::info!("stt: no groq key, using local whisper directly");
        mark_path(3);
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

    // Reject buffers shorter than 200ms (3200 samples at 16kHz)
    if samples.len() < 3200 {
        tracing::info!("stt: buffer too short ({} samples), skipping STT", samples.len());
        mark_path(5);
        return Ok("".to_string());
    }

    // Audio energy check: if below threshold, audio is room noise / silence
    let sum_sq: f64 = samples.iter().map(|&s| {
        let f = s as f64 / 32768.0;
        f * f
    }).sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    if rms < 0.005 {
        tracing::info!("stt: audio energy below threshold (RMS {:.5} < 0.005), skipping STT", rms);
        mark_path(6);
        return Ok("".to_string());
    }

    // Read Groq API key from settings (if app handle is available)
    if let Some(app) = app {
        let groq_key = crate::commands::read_groq_api_key(app);
        let local_only = crate::commands::read_local_stt_only(app);

        if local_only {
            tracing::info!("stt: localSttOnly=true, using local whisper (privacy mode)");
            mark_path(4);
        } else if !groq_key.is_empty() {
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
                    tracing::warn!("stt: groq failed ({}), falling back to local whisper", e);
                    mark_path(2);
                }
            }
        } else {
            tracing::info!("stt: no groq key, using local whisper directly");
            mark_path(3);
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

    let wav_bytes = pcm_to_wav(samples, 16000);
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
fn pcm_to_wav(samples: &[i16], sample_rate: u32) -> Vec<u8> {
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

/// Confabulation counters (P0 deployment SLO: filter hits / total).
/// Incremented by `filter_transcript_counted` on every production STT
/// turn; read via the `stt_filter_stats` IPC command. Process-lifetime
/// counts — reset on restart (rates, not absolutes, are the SLO).
static FILTER_TOTAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static FILTER_HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Filter + count one transcript. Drop-in for `apply_hallucination_filter`
/// at production call sites (tests keep calling the pure fn directly).
pub fn filter_transcript_counted(text: &str) -> String {
    use std::sync::atomic::Ordering;
    FILTER_TOTAL.fetch_add(1, Ordering::Relaxed);
    let out = apply_hallucination_filter(text);
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

/// Filter common Whisper hallucinations on noisy/silent audio.
/// `contains` entries are multi-word junk that never forms a command;
/// `exact` entries are single fragments Whisper emits on pure noise
/// (measured 2026-09-19 on room/TV/conversation clips with nobody speaking:
/// ' Thank you.', ' you', ' BOOAAA!', " It's a", ' out.', ' Oh').
/// Kept OUT deliberately: "i'm sorry" / "i'm not" (dictation-plausible).
fn apply_hallucination_filter(text: &str) -> String {
    let normalized = normalize_english_punctuation(text);
    if !is_english_compatible_text(&normalized) {
        return "".to_string();
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
            return "".to_string();
        }
    }

    let exact_fragments = [
        "you", "oh", "out", "it's a", "thank you", "thanks", "from the", "in the",
        "of the", "to the", "and then", "and the", "on the", "open drift", "open breath",
    ];
    let bare = lower.trim().trim_end_matches(|c: char| matches!(c, '.' | ',' | '?' | '!'));
    if exact_fragments.contains(&bare) {
        return "".to_string();
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
        return "".to_string();
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
        return "".to_string();
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
    if word_count >= 3 && english_token_ratio(&words) < 0.30 {
        tracing::info!("stt: filtered non-English/background gibberish: '{}'", text);
        return "".to_string();
    }

    let alpha_count = normalized.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count < 2 {
        return "".to_string();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::apply_hallucination_filter;
    use super::{filter_transcript_counted, hallucination_stats};

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
}
