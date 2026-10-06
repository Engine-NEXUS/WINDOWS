//! TTS — 2-tier fallback: edge-tts (cloud) → Kokoro (local).
//!
//! Phase 2 architecture:
//!   Primary:   edge-tts (Microsoft Neural, cloud, $0, ~200ms, 0 MB RAM)
//!   Fallback:  Kokoro-82M (local ONNX, $0, ~real-time on CPU, ~260-370 MB RAM while loaded,
//!              lazy-loaded + unloaded after 10 min of stable network). Piper was removed (GPL espeak-ng).
//!
//! Cached acknowledgment phrases ("On it sir", etc.) are pre-synthesized at
//! boot using edge-tts and stored as f32 PCM in RAM. Playback is <5ms
//! regardless of which engine generated them.

use crate::meeting_detect::MeetingState;
use rodio::{buffer::SamplesBuffer, OutputStream, Sink};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{Emitter, State};

/// Global Kokoro engine reference — set at TtsState creation. Lets the network monitor unload the
/// engine and the streaming producer task synthesize locally without borrowing TtsState.
static GLOBAL_KOKORO_ENGINE: std::sync::OnceLock<crate::tts_kokoro::KokoroEngine> =
    std::sync::OnceLock::new();

pub struct TtsState {
    /// Local (offline) engine: Kokoro, lazy-loaded only when the cloud is unavailable.
    pub kokoro_engine: crate::tts_kokoro::KokoroEngine,
    /// Pre-synthesized short phrases for instant acknowledgment playback.
    /// Keyed by the exact phrase text. Stores f32 PCM samples + sample rate.
    pub cache: Arc<Mutex<HashMap<String, CachedAudio>>>,
}

/// Cached audio: PCM samples + sample rate for rodio playback, plus the
/// response caption to emit alongside it (so cache-hit playback — the
/// instant <5ms path — gets a caption exactly like every other path).
#[derive(Clone)]
pub struct CachedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub caption: CaptionTrack,
}

/// One word's reveal timing for the response caption (plan Phase 3).
/// `start_ms`/`duration_ms` are milliseconds from the start of the overall
/// utterance (for a streamed multi-chunk reply, chunk N's words already
/// have chunk 0..N-1's cumulative audio duration baked in — see
/// `play_audio_streamed`'s `cumulative_ms` — so the frontend never needs
/// to know chunking happened).
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct CaptionWord {
    pub text: String,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// One utterance's (or one streamed chunk's) full caption track, emitted
/// as the `tts:caption` event right as its audio is appended to the sink.
/// `estimated` is true for the local (Kokoro) path (no real word-boundary
/// data — timings are evenly distributed across the known PCM duration).
///
/// `envelope`/`frame_ms`/`envelope_start_ms` (sub-phase C2) carry NEXUS's
/// own voice amplitude to the frontend so the speaking-state orb's "beat"
/// deformation reacts to the actual TTS audio instead of the microphone's
/// volume reused as a stand-in. `envelope_start_ms` mirrors the same
/// cumulative-offset bookkeeping `CaptionWord.start_ms` already uses for a
/// streamed multi-chunk reply (see `play_audio_streamed`) — both ride the
/// SAME absolute utterance timeline, so the frontend never special-cases
/// chunking for either one.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct CaptionTrack {
    pub words: Vec<CaptionWord>,
    pub total_ms: u64,
    pub estimated: bool,
    pub envelope: Vec<f32>,
    pub frame_ms: u64,
    pub envelope_start_ms: u64,
}

/// Evenly distribute `text`'s whitespace-split words across `total_ms` of
/// known audio duration. Used for the local Kokoro fallback (it has
/// no word-boundary events) so the frontend caption scheduler never needs
/// to special-case which engine spoke. Pure + unit-tested.
pub fn estimate_words(text: &str, total_ms: u64) -> Vec<CaptionWord> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return Vec::new();
    }
    let per = total_ms / words.len() as u64;
    words
        .iter()
        .enumerate()
        .map(|(i, w)| CaptionWord {
            text: (*w).to_string(),
            start_ms: i as u64 * per,
            duration_ms: per,
        })
        .collect()
}

/// Unescape common SSML/XML entities emitted by Microsoft Edge-TTS (e.g. "didn&apos;t").
pub fn unescape_ssml_entities(text: &str) -> String {
    text.replace("&apos;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

/// Convert edge-tts's real word-boundary events (100ns ticks) into
/// `CaptionWord`s, offset by `cumulative_ms` (non-zero for chunk 2+ of a
/// streamed reply — see `play_audio_streamed`). Pure + unit-tested.
pub fn boundaries_to_words(
    boundaries: &[edge_tts_rust::BoundaryEvent],
    cumulative_ms: u64,
) -> Vec<CaptionWord> {
    boundaries
        .iter()
        .map(|b| CaptionWord {
            text: unescape_ssml_entities(&b.text),
            start_ms: cumulative_ms + b.offset_ticks / 10_000,
            duration_ms: b.duration_ticks / 10_000,
        })
        .collect()
}

/// Frame size for the TTS amplitude envelope (sub-phase C2): short enough
/// to track syllable-level energy changes, long enough to stay cheap to
/// compute and transmit over IPC.
pub const ENVELOPE_FRAME_MS: u64 = 20;

/// Per-frame RMS envelope of `samples`, normalized so the loudest frame in
/// the track reads as 1.0 — absolute amplitude varies a lot between
/// engines/voices, so a fixed reference level would make quiet voices
/// barely move the orb and loud ones clip at 1.0 with no headroom. Silent
/// input returns an all-zero envelope rather than dividing by zero.
/// Pure + unit-tested.
pub fn compute_envelope(samples: &[f32], sample_rate: u32, frame_ms: u64) -> Vec<f32> {
    if samples.is_empty() || sample_rate == 0 || frame_ms == 0 {
        return Vec::new();
    }
    let frame_len = ((sample_rate as u64 * frame_ms) / 1000).max(1) as usize;
    let mut raw: Vec<f32> = Vec::with_capacity(samples.len() / frame_len + 1);
    let mut peak = 0.0f32;
    for chunk in samples.chunks(frame_len) {
        let sum_sq: f32 = chunk.iter().map(|s| s * s).sum();
        let rms = (sum_sq / chunk.len() as f32).sqrt();
        peak = peak.max(rms);
        raw.push(rms);
    }
    if peak <= 1e-6 {
        return raw.iter().map(|_| 0.0).collect();
    }
    raw.into_iter().map(|v| (v / peak).clamp(0.0, 1.0)).collect()
}

/// Truncate text to at most `max_chars` CHARACTERS for log lines.
/// Byte-slicing (`&text[..n]`) panics on multibyte UTF-8 (measured
/// 2026-09-19: byte 50 inside 'の' → panic → `panic=abort` → whole app
/// dead). This can never panic by construction.
pub fn truncate_for_log(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

impl TtsState {
    pub fn new() -> Self {
        let kokoro_engine = crate::tts_kokoro::new_engine();
        // Store global reference for the network monitor / streaming producer to access
        let _ = GLOBAL_KOKORO_ENGINE.set(kokoro_engine.clone());
        Self {
            kokoro_engine,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Unload the local engine globally (called by the network monitor after 10 min of stable network).
pub async fn unload_local_global() {
    if let Some(engine) = GLOBAL_KOKORO_ENGINE.get() {
        crate::tts_kokoro::unload(engine).await;
    }
}

/// Handle to the shared local engine (swap worker / startup sync).
pub fn kokoro_engine_handle() -> Option<crate::tts_kokoro::KokoroEngine> {
    GLOBAL_KOKORO_ENGINE.get().cloned()
}

/// Synthesize with the local Kokoro engine (loads it lazily). Used by every offline path.
async fn synthesize_local(text: &str) -> Result<(Vec<f32>, u32, CaptionTrack), String> {
    let engine = GLOBAL_KOKORO_ENGINE
        .get()
        .ok_or_else(|| "local TTS engine unavailable".to_string())?;
    crate::tts_kokoro::ensure_loaded(engine).await?;
    crate::tts_network::mark_local_loaded();
    let (samples, sr) = crate::tts_kokoro::synthesize(engine, text, 1.0).await?;
    let total_ms = samples.len() as u64 * 1000 / (sr as u64).max(1);
    let words = estimate_words(text, total_ms);
    let envelope = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
    Ok((
        samples,
        sr,
        CaptionTrack { words, total_ms, estimated: true, envelope, frame_ms: ENVELOPE_FRAME_MS, envelope_start_ms: 0 },
    ))
}

/// Phrases that are pre-synthesized on first TTS load for instant playback.
/// These are the high-frequency acknowledgment/error phrases that must play
/// with zero synthesis delay to feel natural.
const CACHED_PHRASES: &[&str] = &[
    "On it sir",
    "Didn't understand that sir",
    "Didn't catch that sir",
    "Here is the analysis, sir",
    "Ok sir",
    "Ready to search, sir",
    "Stopped typing, sir",
    "Opening Command Hub, sir",
    "Annotation ready, sir",
];

/// Pre-synthesize cached phrases using edge-tts at boot.
/// Falls back to the local Kokoro voice if edge-tts is unavailable.
/// This runs once at startup so all subsequent speak_cached() calls are instant (<5ms).
pub async fn pregenerate_cache(
    cache_arc: &Arc<Mutex<HashMap<String, CachedAudio>>>,
    voice: &str,
) {
    let cache_start = std::time::Instant::now();

    let mut cached_count = 0;

    // Always try edge-tts first (cloud, best quality).
    // If it fails, the local Kokoro fallback below kicks in automatically.
    tracing::info!("tts: pre-generating cache with edge-tts (voice={})", voice);
    for phrase in CACHED_PHRASES {
        match crate::tts_edge::synthesize_to_pcm(phrase, voice).await {
                Ok((samples, sr, boundaries)) => {
                    let total_ms = samples.len() as u64 * 1000 / (sr as u64).max(1);
                    let words = boundaries_to_words(&boundaries, 0);
                    let envelope = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
                    cache_arc.lock().await.insert(
                        phrase.to_string(),
                        CachedAudio { samples, sample_rate: sr, caption: CaptionTrack { words, total_ms, estimated: false, envelope, frame_ms: ENVELOPE_FRAME_MS, envelope_start_ms: 0 } },
                    );
                    cached_count += 1;
                }
                Err(e) => {
                    tracing::warn!("tts: edge-tts cache failed for '{}': {}", phrase, e);
                }
            }
        }

    // If edge-tts failed, try the local voice for the cache (a temporary engine, freed afterwards)
    if cached_count == 0 {
        tracing::info!("tts: falling back to Kokoro for cache generation");
        let local_engine = crate::tts_kokoro::new_engine();
        if let Err(e) = crate::tts_kokoro::ensure_loaded(&local_engine).await {
            tracing::warn!("tts: local cache generation unavailable: {}", e);
        }
        for phrase in CACHED_PHRASES {
            match crate::tts_kokoro::synthesize(&local_engine, phrase, 1.0).await {
                Ok((samples, sr)) => {
                    let total_ms = samples.len() as u64 * 1000 / (sr as u64).max(1);
                    let words = estimate_words(phrase, total_ms);
                    let envelope = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
                    cache_arc.lock().await.insert(
                        phrase.to_string(),
                        CachedAudio { samples, sample_rate: sr, caption: CaptionTrack { words, total_ms, estimated: true, envelope, frame_ms: ENVELOPE_FRAME_MS, envelope_start_ms: 0 } },
                    );
                    cached_count += 1;
                }
                Err(e) => {
                    tracing::warn!("tts: local cache failed for '{}': {}", phrase, e);
                }
            }
        }
    }

    tracing::info!(
        "tts: cached {} phrases in {:.2}s",
        cached_count,
        cache_start.elapsed().as_secs_f32()
    );
}

/// Global generation counter: incremented by `stop_tts` to signal the playback thread
/// to stop the current audio immediately.
static TTS_GENERATION: AtomicUsize = AtomicUsize::new(0);

/// IPC: Stop any currently-playing TTS audio and immediately restore system volume.
#[tauri::command]
pub fn stop_tts() -> Result<(), String> {
    TTS_GENERATION.fetch_add(1, Ordering::SeqCst);
    tracing::info!("tts: stop requested (generation {})", TTS_GENERATION.load(Ordering::SeqCst));
    crate::volume::force_restore_volume();
    Ok(())
}

/// IPC: Explicitly restore system volume to baseline.
#[tauri::command]
pub fn restore_tts_volume() -> Result<(), String> {
    crate::volume::force_restore_volume();
    Ok(())
}

/// IPC: Speak text using the 3-tier fallback chain.
///
/// Tries edge-tts (cloud) first, then the local Kokoro voice (offline only).
/// For cached phrases, plays instantly from memory (<5ms).
#[tauri::command]
pub async fn speak_text(
    text: String,
    voice: Option<String>,
    _speed: Option<f32>,
    state: State<'_, TtsState>,
    meeting: State<'_, Arc<MeetingState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("tts: speaking '{}'", text);
    crate::directed::note_spoken(&text); // echo rejection for open-mic turns (Phase 8)

    meeting.set_tts_playing(true);
    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // Read settings
    let edge_voice = crate::commands::read_edge_tts_voice(&app);
    let voice_id = voice.unwrap_or_else(|| edge_voice.clone());
    let tts_volume_pct = read_tts_volume(&app);
    // B3: emotional prosody (settings override or auto heuristics).
    let emotion = resolve_emotion(&app, &text);

    // Fast path: exact-match cache (<5ms, no network, no synthesis).
    // Acks like "Ok sir." were pre-generated at boot — replay them instead
    // of re-synthesizing (which previously paid a probe + cold-engine cost).
    // Falls through to synthesis on miss.
    let (audio, sample_rate, caption) = if let Some(hit) = state.cache.lock().await.get(&text).cloned() {
        tracing::info!("tts: cache hit for '{}'", text);
        (hit.samples, hit.sample_rate, hit.caption)
    } else {
        // B2 streaming: long multi-sentence replies play chunk 0
        // immediately while the rest synthesizes. Short texts keep the
        // legacy single-shot path (zero behavior change). While OFFLINE the
        // local engine runs ~real-time, so ANY multi-sentence reply streams
        // (first audio = one sentence, not the whole reply).
        let sentences = split_sentences(&text);
        let use_streaming = sentences.len() > 1
            && (text.chars().count() >= STREAM_MIN_CHARS || !crate::tts_network::is_network_up());
        if use_streaming {
            // Set volume BEFORE playback (same as legacy path).
            let stream_volume_lease = if tts_volume_pct > 0 {
                let target = tts_volume_pct as f32 / 100.0;
                crate::volume::acquire_volume_lease(target)
            } else {
                None
            };
            match speak_streaming(sentences, &voice_id, &state, my_generation, emotion, app.clone()).await {
                Ok(()) => {
                    drop(stream_volume_lease);
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    meeting.set_tts_playing(false);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("tts: streaming failed ({}), falling back to single-shot", e);
                    drop(stream_volume_lease);
                    // Fall through to legacy single-shot synthesis below.
                    match synthesize_with_fallback(&text, &voice_id, &state, emotion).await {
                        Ok(result) => result,
                        Err(e) => {
                            tracing::error!("tts: all TTS engines failed: {}", e);
                            meeting.set_tts_playing(false);
                            return Err(e);
                        }
                    }
                }
            }
        } else {
            // Try to synthesize using the 2-tier fallback chain
            match synthesize_with_fallback(&text, &voice_id, &state, emotion).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("tts: all TTS engines failed: {}", e);
                    meeting.set_tts_playing(false);
                    return Err(e);
                }
            }
        }
    };

    // Check if stop was requested during synthesis
    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        tracing::info!("tts: stop requested during synthesis, skipping playback");
        // Do NOT set tts_playing=false here. A newer TTS call (higher
        // generation) is already active and has set tts_playing=true.
        // If we set it false, the wake word detector becomes un-suppressed
        // during the newer TTS playback, causing false wakes from TTS echo.
        // Only the current (newest) generation should clear the flag.
        return Ok(());
    }

    // Save and set system volume with RAII lease
    let volume_lease = if tts_volume_pct > 0 {
        let target = tts_volume_pct as f32 / 100.0;
        crate::volume::acquire_volume_lease(target)
    } else {
        None
    };

    // Play audio
    let play_result = play_audio(audio, sample_rate, my_generation, caption, app.clone()).await;

    // Release lease
    drop(volume_lease);

    // Grace period
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    meeting.set_tts_playing(false);

    play_result
}

/// IPC: Preview a specific TTS voice with a short demo phrase.
/// Used by the settings sidebar voice picker — plays a demo without
/// saving the voice as the default. Does NOT change system volume.
#[tauri::command]
pub async fn preview_voice(
    voice_id: String,
    text: Option<String>,
    state: State<'_, TtsState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let demo_text = text.unwrap_or_else(|| {
        if voice_id.starts_with("en-US-") {
            let name = voice_id
                .strip_prefix("en-US-")
                .unwrap_or("")
                .trim_end_matches("Neural");
            format!("Hello, I'm {}. This is how I sound.", name)
        } else {
            "Hello, this is a voice preview.".to_string()
        }
    });

    tracing::info!("tts: previewing voice '{}' with text '{}'", voice_id, demo_text);

    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    let (audio, sample_rate, caption) = match synthesize_with_fallback(&demo_text, &voice_id, &state, crate::tts_edge::TtsEmotion::Neutral).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("tts: voice preview failed for '{}': {}", voice_id, e);
            return Err(e);
        }
    };

    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        return Ok(());
    }

    play_audio(audio, sample_rate, my_generation, caption, app).await
}

/// Feature 83 — equip result returned synchronously by set_voice_preference
/// so the hub can update the card without waiting for the status event.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceEquipResult {
    pub voice_key: String,
    pub cloud_id: String,
    pub sync_state: crate::voice_catalog::VoiceSyncState,
}

/// IPC: Equip an iconic voice persona (Feature 83).
/// Validates the key against the voice catalog, writes selected_voice +
/// edge_tts_voice + offline_voice_model to settings.json (instant cloud
/// switch — next utterance speaks in the new voice), kicks a background
/// Fast-ACK cache re-synthesis, and emits `voice:status` (ready when the
/// local twin is already in the single slot, cloud-only otherwise — the
/// P3 swap worker fills the download in).
#[tauri::command]
pub async fn set_voice_preference(
    voice_key: String,
    state: State<'_, TtsState>,
    app: tauri::AppHandle,
) -> Result<VoiceEquipResult, String> {
    use tauri::{Emitter, Manager};
    let persona = crate::voice_catalog::find_by_key(&voice_key)
        .ok_or_else(|| format!("Unknown voice '{voice_key}'"))?;

    // Persist the selection (same write path as save_settings, identity-safe:
    // only the three voice fields change).
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    let mut settings: crate::commands::NexusSettings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    settings.selected_voice = persona.key.to_string();
    settings.edge_tts_voice = persona.cloud_id.to_string();
    settings.offline_voice_model = persona.kokoro_voice.to_string();
    crate::tts_kokoro::set_preferred_voice(persona.kokoro_voice);
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    tracing::info!(
        "tts: voice equipped '{}' (cloud={}, twin={})",
        persona.key,
        persona.cloud_id,
        persona.kokoro_voice
    );

    // Fast-ACK re-synthesis in the background (new voice, ~350ms).
    {
        let cache = state.cache.clone();
        let cloud_id = persona.cloud_id.to_string();
        tokio::spawn(async move {
            pregenerate_cache(&cache, &cloud_id).await;
        });
    }

    // Sync state of the local twin. Twin present → ready now; missing →
    // emit downloading(0) and spawn the P3 swap worker (progress + final
    // ready/error events land as the download proceeds).
    let sync_state = crate::voice_catalog::sync_state_for(persona.key);
    if sync_state == crate::voice_catalog::VoiceSyncState::Ready {
        let _ = app.emit(
            "voice:status",
            serde_json::json!({ "status": sync_state, "voice_key": persona.key }),
        );
    } else {
        // Latest wins: a rapid A -> B -> C ends with C's offline voice installed.
        crate::tts_swap::request_swap(app.clone(), persona.key.to_string(), state.kokoro_engine.clone());
    }

    Ok(VoiceEquipResult {
        voice_key: persona.key.to_string(),
        cloud_id: persona.cloud_id.to_string(),
        sync_state,
    })
}

#[cfg(test)]
mod voice_preference_tests {
    use super::*;

    #[test]
    fn test_equip_result_serializes_camel_case() {
        let r = VoiceEquipResult {
            voice_key: "jarvis".into(),
            cloud_id: "en-GB-RyanNeural".into(),
            sync_state: crate::voice_catalog::VoiceSyncState::Ready,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["voiceKey"], "jarvis");
        assert_eq!(v["cloudId"], "en-GB-RyanNeural");
        assert_eq!(v["syncState"], "ready");
    }

    #[test]
    fn test_sync_state_serializes_cloud_only() {
        let v = serde_json::to_value(crate::voice_catalog::VoiceSyncState::CloudOnly).unwrap();
        assert_eq!(v, "cloud_only");
    }
}

/// IPC: List the iconic voice catalog for the hub grid (single source of
/// truth lives in voice_catalog.rs — the frontend never hardcodes it).
#[tauri::command]
pub fn list_voice_personas() -> Result<Vec<crate::voice_catalog::VoicePersona>, String> {
    Ok(crate::voice_catalog::VOICE_CATALOG.to_vec())
}

/// IPC: Current synthesis transport for the hub header dot
/// ("cloud" = next utterance goes Edge, "local" = next goes Kokoro).
/// Reads the live network flag — no probe, no cost.
#[tauri::command]
pub fn get_voice_transport() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "transport": if crate::tts_network::is_network_up() { "cloud" } else { "local" },
    }))
}

/// IPC: Sync state of the currently selected persona's offline twin
/// (hub reads this on mount for the card pill: ready vs cloud-only).
/// Reports "downloading" truthfully while the swap worker runs.
#[tauri::command]
pub fn get_voice_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<serde_json::Value, String> {
    let key = crate::commands::read_selected_voice(&app);
    let persona = crate::voice_catalog::find_by_key(&key);
    let downloading = crate::tts_swap::swap_target().as_deref() == Some(key.as_str());
    let sync_state = if downloading {
        "downloading"
    } else {
        match crate::voice_catalog::sync_state_for(&key) {
            crate::voice_catalog::VoiceSyncState::Ready => "ready",
            crate::voice_catalog::VoiceSyncState::CloudOnly => "cloud_only",
        }
    };
    Ok(serde_json::json!({
        "voice_key": key,
        "cloud_id": persona.map(|p| p.cloud_id).unwrap_or(""),
        "twin": persona.map(|p| p.kokoro_voice).unwrap_or(""),
        "sync_state": sync_state,
    }))
}
///
/// Falls back to `speak_text` if the phrase is not in the cache.
#[tauri::command]
pub async fn speak_cached(
    text: String,
    state: State<'_, TtsState>,
    meeting: State<'_, Arc<MeetingState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tracing::info!("tts: speaking cached '{}'", text);
    crate::directed::note_spoken(&text);

    meeting.set_tts_playing(true);
    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // Try to get cached audio
    let cache_arc = state.cache.clone();
    let cached_audio = {
        let cache = cache_arc.lock().await;
        cache.get(&text).cloned()
    };

    let (audio, sample_rate, caption) = match cached_audio {
        Some(ca) => {
            tracing::info!("tts: cache hit for '{}'", text);
            (ca.samples, ca.sample_rate, ca.caption)
        }
        None => {
            // Cache miss — synthesize on demand
            tracing::info!("tts: cache miss for '{}', synthesizing on demand", text);
            let edge_voice = crate::commands::read_edge_tts_voice(&app);
            match synthesize_with_fallback(&text, &edge_voice, &state, crate::tts_edge::TtsEmotion::Neutral).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("tts: synthesis failed for cached phrase: {}", e);
                    meeting.set_tts_playing(false);
                    return Err(e);
                }
            }
        }
    };

    // Check if stop was requested
    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        tracing::info!("tts: stop requested before cached playback, skipping");
        // Do NOT set tts_playing=false — a newer generation is active.
        return Ok(());
    }

    // Read TTS volume and acquire lease
    let tts_volume_pct = read_tts_volume(&app);
    let volume_lease = if tts_volume_pct > 0 {
        let target = tts_volume_pct as f32 / 100.0;
        crate::volume::acquire_volume_lease(target)
    } else {
        None
    };

    // Play
    let play_result = play_audio(audio, sample_rate, my_generation, caption, app.clone()).await;

    // Release lease
    drop(volume_lease);

    // Grace period
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    meeting.set_tts_playing(false);

    play_result
}

/// 2-tier synthesis fallback: edge-tts (cloud, primary) → Kokoro (local, only when the cloud is
/// unavailable). Online is ALWAYS tried first; the network watchdog flips back to the cloud as soon
/// as it returns (`tts_network`).
///
/// Returns (f32 PCM samples, sample_rate, response caption) on success.
/// The caption is `estimated: true` for the local path (no real word-boundary
/// data), `false` for edge-tts (real boundaries).
async fn synthesize_with_fallback(
    text: &str,
    voice: &str,
    state: &TtsState,
    emotion: crate::tts_edge::TtsEmotion,
) -> Result<(Vec<f32>, u32, CaptionTrack), String> {
    let _ = state; // the local engine is reached through the global handle (shared with the streaming producer)
    // Upfront engine policy (Main Center TTS director, doc 74 P3):
    // network-down skips Edge entirely (saves
    // ~1-2s of waiting for Edge to fail). Identical branches to before —
    // the policy is now pinned + tested in `center::tts_engine_for`.
    //
    // NOTE: this is the CACHED flag only (maintained by the background
    // monitor + synthesis outcomes below). We used to run a live HTTPS
    // probe here on every call — it lied (reported down while Groq worked)
    // and added up to 5s before synthesis even started. The Edge attempt
    // itself, raced with a timeout, is the only honest connectivity check.
    // A legacy `piper-*` voice id (selected in an older build's picker) means "the offline voice".
    if crate::center::tts_engine_for(voice.starts_with("piper-"), crate::tts_network::is_network_up())
        == crate::center::TtsEngine::Local
    {
        tracing::info!("tts: network down (cached) — using local Kokoro directly (skipping Edge TTS)");
    } else {
        // Tier 1: edge-tts (cloud, ~200ms, best quality, 0 MB RAM),
        // raced with a timeout so a hanging endpoint can't stall speech.
        match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            crate::tts_edge::synthesize_to_pcm_with_emotion(text, voice, emotion),
        )
        .await
        {
            Ok(Ok((samples, sr, boundaries))) => {
                tracing::info!("tts: edge-tts synthesis OK (cloud)");
                crate::tts_network::set_network_up();
                let total_ms = samples.len() as u64 * 1000 / (sr as u64).max(1);
                let words = boundaries_to_words(&boundaries, 0);
                let envelope = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
                return Ok((samples, sr, CaptionTrack { words, total_ms, estimated: false, envelope, frame_ms: ENVELOPE_FRAME_MS, envelope_start_ms: 0 }));
            }
            Err(_elapsed) => {
                // Timeout = genuine transport failure → mark down so the
                // next call skips Edge (saves 8s per call while offline).
                tracing::warn!("tts: edge-tts timed out, trying local fallback");
                crate::tts_network::set_network_down();
            }
            Ok(Err(e)) => {
                // Content/API rejection (bad voice, unsupported script, 4xx)
                // is NOT a network outage — a single bad sentence must not
                // poison the global flag (measured 2026-09-19: Japanese text
                // → English voice rejection → "network down" → forced local
                // voice → panic). Fall through to the local voice for THIS call only.
                tracing::warn!("tts: edge-tts rejected ({e}), trying local fallback (network flag untouched)");
            }
        }
    }

    // Tier 2: Kokoro (local, ~real-time on CPU, ~260-370 MB RAM while loaded)
    match synthesize_local(text).await {
        Ok(result) => {
            tracing::info!("tts: local Kokoro synthesis OK (offline voice)");
            Ok(result)
        }
        Err(e) => {
            tracing::error!("tts: local fallback also failed: {}", e);
            Err(format!("All TTS engines failed. Last error: {}", e))
        }
    }
}

/// Play f32 PCM audio through rodio with barge-in support. Emits
/// `tts:caption` right as the audio is appended to the sink (plan Phase 3)
/// — skipped if a barge-in already superseded this generation, so a
/// stale reply's caption never flashes on screen.
async fn play_audio(
    audio: Vec<f32>,
    sample_rate: u32,
    my_generation: usize,
    caption: CaptionTrack,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => {
                match Sink::try_new(&handle) {
                    Ok(sink) => {
                        if TTS_GENERATION.load(Ordering::SeqCst) <= my_generation {
                            let _ = app.emit("tts:caption", &caption);
                        }
                        let source = SamplesBuffer::new(1, sample_rate, audio);
                        sink.append(source);

                        // Poll for stop request (barge-in)
                        while !sink.empty() {
                            if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                                sink.stop();
                                tracing::info!("tts: playback stopped by user (barge-in)");
                                return Ok(());
                            }
                            std::thread::sleep(std::time::Duration::from_millis(20));
                        }
                        tracing::info!("tts: audio playback completed");
                        Ok(())
                    }
                    Err(e) => {
                        tracing::error!("tts: failed to create audio sink: {}", e);
                        Err(format!("Failed to create audio sink: {}", e))
                    }
                }
            }
            Err(e) => {
                tracing::error!("tts: failed to open audio output: {}", e);
                Err(format!("Failed to get audio output stream: {}", e))
            }
        }
    })
    .await
    .unwrap_or_else(|_| {
        tracing::error!("tts: audio thread panicked");
        Err("Audio thread panicked".to_string())
    })
}

/// Read the TTS emotion setting from settings.json.
/// Returns "auto" (default), "neutral", "cheerful", "calm", "sad",
/// "urgent", or "whisper". "auto" picks per-text heuristics.
fn read_tts_emotion_setting<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> String {
    use tauri::Manager;
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return "auto".to_string(),
    };
    let path = dir.join("settings.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return "neutral".to_string(),
    };
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
        if let Some(e) = json.get("ttsEmotion").and_then(|v| v.as_str()) {
            return e.to_string();
        }
        if let Some(e) = json.get("tts_emotion").and_then(|v| v.as_str()) {
            return e.to_string();
        }
    }
    "neutral".to_string()
}

/// Resolve the effective emotion: explicit setting wins, defaults to Neutral (Ava Neutral).
fn resolve_emotion<R: tauri::Runtime>(app: &tauri::AppHandle<R>, text: &str) -> crate::tts_edge::TtsEmotion {
    let setting = read_tts_emotion_setting(app);
    if setting.eq_ignore_ascii_case("auto") {
        return crate::tts_edge::pick_emotion(text);
    }
    if setting.is_empty() || setting.eq_ignore_ascii_case("neutral") {
        return crate::tts_edge::TtsEmotion::Neutral;
    }
    crate::tts_edge::TtsEmotion::parse_label(&setting)
}

/// Read the TTS volume setting from settings.json.
/// Returns 0-100. 0 means "disabled" (don't adjust system volume).
fn read_tts_volume<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> u8 {

    use tauri::Manager;
    let dir = match app.path().app_data_dir() {
        Ok(d) => d,
        Err(_) => return 75,
    };
    let path = dir.join("settings.json");
    if !path.exists() {
        return 75;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return 75,
    };
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
        if let Some(vol) = json.get("ttsVolume").and_then(|v| v.as_u64()) {
            return vol as u8;
        }
        if let Some(vol) = json.get("tts_volume").and_then(|v| v.as_u64()) {
            return vol as u8;
        }
    }
    75
}

/// Sentence-chunked streaming TTS (B2): split long replies into sentences,
/// synthesize + play the first chunk immediately, synthesize the rest while
/// it plays. First-audio latency ≈ one sentence instead of the full reply.
///
/// Short/single-sentence texts use the legacy single-shot path (zero
/// behavior change for the common case).
const STREAM_MIN_CHARS: usize = 150;

/// Split text into sentence chunks (keeps delimiters). Pure + unit-tested.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    let mut cur = String::new();
    for ch in text.chars() {
        cur.push(ch);
        if matches!(ch, '.' | '!' | '?' | '\n') {
            let s = cur.trim().to_string();
            if !s.is_empty() {
                out.push(s);
            }
            cur.clear();
        }
    }
    let tail = cur.trim().to_string();
    if !tail.is_empty() {
        out.push(tail);
    }
    if out.is_empty() {
        out.push(text.to_string());
    }
    out
}

/// Synthesize one chunk: per-sentence cache check, then the 2-tier fallback.
async fn synthesize_chunk(
    sentence: &str,
    voice_id: &str,
    state: &TtsState,
    emotion: crate::tts_edge::TtsEmotion,
) -> Result<(Vec<f32>, u32, CaptionTrack), String> {
    if let Some(hit) = state.cache.lock().await.get(sentence).cloned() {
        return Ok((hit.samples, hit.sample_rate, hit.caption));
    }
    synthesize_with_fallback(sentence, voice_id, state, emotion).await
}

/// Streaming playback: chunk 0 is already synthesized (plays immediately);
/// remaining chunks are synthesized while earlier audio plays. Barge-in
/// checked before each synthesis + append, and during the drain poll.
async fn speak_streaming(
    sentences: Vec<String>,
    voice_id: &str,
    state: &TtsState,
    my_generation: usize,
    emotion: crate::tts_edge::TtsEmotion,
    app: tauri::AppHandle,
) -> Result<(), String> {
    // Chunk 0 first — this defines first-audio latency.
    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        return Ok(());
    }
    let (first_audio, first_sr, first_caption) = synthesize_chunk(&sentences[0], voice_id, state, emotion).await?;

    let (stream_tx, stream_rx) =
        std::sync::mpsc::channel::<Option<(Vec<f32>, u32, CaptionTrack)>>();
    // Producer: synthesize remaining chunks while chunk 0 plays.
    let rest: Vec<String> = sentences.into_iter().skip(1).collect();
    let voice_owned = voice_id.to_string();
    let state_ref = TtsStateRef {
        cache: state.cache.clone(),
    };
    let gen = my_generation;
    tokio::spawn(async move {
        for s in rest {
            if TTS_GENERATION.load(Ordering::SeqCst) > gen {
                break;
            }
            let res = synthesize_chunk_owned(&s, &voice_owned, &state_ref, emotion).await;
            match res {
                Ok(triple) => {
                    if stream_tx.send(Some(triple)).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::warn!("tts: streaming chunk failed ({}), playing what we have", e);
                    break;
                }
            }
        }
        let _ = stream_tx.send(None);
    });

    // Consumer: single rodio Sink, append chunks as they arrive.
    let first = Some((first_audio, first_sr, first_caption));
    play_audio_streamed(first, stream_rx, my_generation, app).await
}

/// Minimal shared refs for the streaming producer task.
struct TtsStateRef {
    cache: Arc<Mutex<HashMap<String, CachedAudio>>>,
}

async fn synthesize_chunk_owned(
    sentence: &str,
    voice_id: &str,
    state: &TtsStateRef,
    emotion: crate::tts_edge::TtsEmotion,
) -> Result<(Vec<f32>, u32, CaptionTrack), String> {
    if let Some(hit) = state.cache.lock().await.get(sentence).cloned() {
        return Ok((hit.samples, hit.sample_rate, hit.caption));
    }
    // Online: edge-tts per sentence; if that fails (or the network is known down) this sentence is
    // synthesized locally so the reply never goes silent (the voice may change mid-reply — better
    // than muting). Offline streaming therefore plays chunk 0 as soon as ITS sentence is ready.
    let edge = if crate::tts_network::is_network_up() {
        crate::tts_edge::synthesize_to_pcm_with_emotion(sentence, voice_id, emotion).await
    } else {
        Err("network down (cached)".to_string())
    };
    let (samples, sr, boundaries) = match edge {
        Ok(v) => v,
        Err(_) => return synthesize_local(sentence).await,
    };
    // cumulative_ms is applied by the consumer (play_audio_streamed), which
    // is the only place that knows how much audio has already been queued —
    // the producer races ahead of playback and has no visibility into it.
    let total_ms = samples.len() as u64 * 1000 / (sr as u64).max(1);
    let words = boundaries_to_words(&boundaries, 0);
    let envelope = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
    Ok((samples, sr, CaptionTrack { words, total_ms, estimated: false, envelope, frame_ms: ENVELOPE_FRAME_MS, envelope_start_ms: 0 }))
}

/// Play chunk 0 immediately, then append streamed chunks as they arrive.
/// Each chunk's `tts:caption` is emitted right as it's appended to the
/// sink, with its word `start_ms` offset by `cumulative_ms` — the *actual
/// decoded PCM duration* of every previously-queued chunk (not any
/// engine-reported duration), so the frontend never needs to know
/// streaming happened at all: it just sees one utterance's words arrive
/// in a few batches, all already on one absolute timeline.
async fn play_audio_streamed(
    first: Option<(Vec<f32>, u32, CaptionTrack)>,
    rx: std::sync::mpsc::Receiver<Option<(Vec<f32>, u32, CaptionTrack)>>,
    my_generation: usize,
    app: tauri::AppHandle,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => match Sink::try_new(&handle) {
                Ok(sink) => {
                    let mut cumulative_ms: u64 = 0;
                    if let Some((audio, sr, mut caption)) = first {
                        caption.envelope_start_ms = cumulative_ms;
                        if TTS_GENERATION.load(Ordering::SeqCst) <= my_generation {
                            let _ = app.emit("tts:caption", &caption);
                        }
                        cumulative_ms += caption.total_ms;
                        sink.append(SamplesBuffer::new(1, sr, audio));
                    }
                    // Drain the producer channel (blocking recv is fine here —
                    // we're on a blocking thread, audio plays async).
                    while let Ok(msg) = rx.recv() {
                        if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                            sink.stop();
                            tracing::info!("tts: streaming stopped by user (barge-in)");
                            return Ok(());
                        }
                        match msg {
                            Some((audio, sr, mut caption)) => {
                                for w in caption.words.iter_mut() {
                                    w.start_ms += cumulative_ms;
                                }
                                caption.envelope_start_ms = cumulative_ms;
                                let _ = app.emit("tts:caption", &caption);
                                cumulative_ms += caption.total_ms;
                                sink.append(SamplesBuffer::new(1, sr, audio));
                            }
                            None => break,
                        }
                    }
                    while !sink.empty() {
                        if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                            sink.stop();
                            tracing::info!("tts: playback stopped by user (barge-in)");
                            return Ok(());
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    tracing::info!("tts: streaming playback completed");
                    Ok(())
                }
                Err(e) => Err(format!("Failed to create audio sink: {}", e)),
            },
            Err(e) => Err(format!("Failed to get audio output stream: {}", e)),
        }
    })
    .await
    .unwrap_or_else(|_| {
        tracing::error!("tts: streaming audio thread panicked");
        Err("Audio thread panicked".to_string())
    })
}

// ─── Narration player (screen tour) ────────────────────────────────────
//
// Plays N lines back-to-back as ONE rodio queue (one source per line, so
// queue position == line index) and reports exactly which line is audible,
// so the screen-tour overlay can show callout i only while line i plays.
// The frontend never holds future lines — "nothing drawn ahead of speech"
// is structural, not timing luck. Reuses the cache/synthesis helpers of the
// streaming path; barge-in rides the same `TTS_GENERATION` counter.

/// One progress event from `narrate`. The index is into the `lines` slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NarrationEvent {
    Started(usize),
    Ended(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NarrateOutcome {
    Completed,
    /// Barge-in / stop arrived (generation bumped).
    Cancelled,
    /// No audio could be produced for line 0 or no output device — nothing
    /// was heard; the caller may fall back to timed (silent) dwell.
    Unavailable(String),
}

/// Silence appended after every line so callouts breathe.
pub const NARRATION_GAP_MS: u64 = 250;

/// Turns (items appended, items still queued) into the events that became
/// due. Pure + unit-tested — this is the sync core: a line is "started"
/// when it is the head of the queue and "ended" once it left the queue.
#[derive(Debug, Default)]
pub struct NarrationTracker {
    started: usize,
    ended: usize,
}

impl NarrationTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, appended: usize, sink_len: usize) -> Vec<NarrationEvent> {
        let completed = appended.saturating_sub(sink_len);
        let mut out = Vec::new();
        while self.ended < completed {
            if self.started <= self.ended {
                out.push(NarrationEvent::Started(self.ended));
                self.started = self.ended + 1;
            }
            out.push(NarrationEvent::Ended(self.ended));
            self.ended += 1;
        }
        if completed < appended && self.started <= completed {
            out.push(NarrationEvent::Started(completed));
            self.started = completed + 1;
        }
        out
    }
}

/// Append `ms` of silence to `samples` (mono).
fn pad_silence(samples: &mut Vec<f32>, sample_rate: u32, ms: u64) {
    let n = (sample_rate as u64 * ms / 1000) as usize;
    samples.extend(std::iter::repeat(0.0f32).take(n));
}

/// Speak `lines` as a single gap-separated queue and report which line is
/// audible via `on_event`. Items are synthesized ahead while earlier ones
/// play; a line that fails to synthesize (after line 0) becomes timed
/// silence so indices stay 1:1 with the queue.
pub async fn narrate<R, F>(
    app: tauri::AppHandle<R>,
    lines: Vec<String>,
    on_event: F,
) -> NarrateOutcome
where
    R: tauri::Runtime,
    F: FnMut(NarrationEvent) + Send + 'static,
{
    use tauri::Manager;
    if lines.is_empty() {
        return NarrateOutcome::Completed;
    }
    let tts_state = app.state::<TtsState>();
    let meeting = app.state::<Arc<MeetingState>>();
    let voice_id = crate::commands::read_edge_tts_voice(&app);
    let volume_pct = read_tts_volume(&app);
    let emotions: Vec<crate::tts_edge::TtsEmotion> =
        lines.iter().map(|l| resolve_emotion(&app, l)).collect();

    meeting.set_tts_playing(true);
    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    for l in &lines {
        crate::directed::note_spoken(l);
    }

    let state_ref = TtsStateRef { cache: tts_state.cache.clone() };
    // Line 0 first: its success decides Unavailable vs playing.
    let first = match tokio::time::timeout(
        std::time::Duration::from_secs(12),
        synthesize_chunk_owned(&lines[0], &voice_id, &state_ref, emotions[0]),
    )
    .await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            meeting.set_tts_playing(false);
            return NarrateOutcome::Unavailable(e);
        }
        Err(_) => {
            meeting.set_tts_playing(false);
            return NarrateOutcome::Unavailable("synthesis timed out".to_string());
        }
    };
    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        return NarrateOutcome::Cancelled;
    }

    type Item = (Vec<f32>, u32, CaptionTrack);
    let (tx, rx) = std::sync::mpsc::channel::<Option<Item>>();
    let rest: Vec<(String, crate::tts_edge::TtsEmotion)> = lines
        .iter()
        .cloned()
        .zip(emotions.iter().copied())
        .skip(1)
        .collect();
    let voice_owned = voice_id.clone();
    let rate_hint = first.1;
    tokio::spawn(async move {
        for (text, emotion) in rest {
            if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                break;
            }
            let item = match tokio::time::timeout(
                std::time::Duration::from_secs(12),
                synthesize_chunk_owned(&text, &voice_owned, &state_ref, emotion),
            )
            .await
            {
                Ok(Ok(v)) => v,
                _ => {
                    // Timed silence keeps queue index == line index.
                    let words = text.split_whitespace().count() as u64;
                    let ms = (words * 330 + 1200).clamp(2500, 9000);
                    let mut s = Vec::new();
                    pad_silence(&mut s, rate_hint, ms);
                    (s, rate_hint, CaptionTrack::default())
                }
            };
            if tx.send(Some(item)).is_err() {
                return;
            }
        }
        let _ = tx.send(None);
    });

    let volume_changed = if volume_pct > 0 {
        crate::volume::save_and_set_volume(volume_pct as f32 / 100.0)
    } else {
        false
    };

    let app_for_audio = app.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let mut on_event = on_event;
        let (_stream, handle) = match OutputStream::try_default() {
            Ok(v) => v,
            Err(e) => return NarrateOutcome::Unavailable(format!("audio output: {e}")),
        };
        let sink = match Sink::try_new(&handle) {
            Ok(s) => s,
            Err(e) => return NarrateOutcome::Unavailable(format!("audio sink: {e}")),
        };
        let mut tracker = NarrationTracker::new();
        let mut appended = 0usize;
        let mut cumulative_ms: u64 = 0;
        let mut channel_done = false;

        let append = |sink: &Sink, item: Item, appended: &mut usize, cumulative: &mut u64| {
            let (mut audio, sr, mut caption) = item;
            let own_ms = audio.len() as u64 * 1000 / (sr as u64).max(1);
            pad_silence(&mut audio, sr, NARRATION_GAP_MS);
            for w in caption.words.iter_mut() {
                w.start_ms += *cumulative;
            }
            caption.envelope_start_ms = *cumulative;
            if caption.total_ms == 0 {
                caption.total_ms = own_ms;
            }
            let _ = app_for_audio.emit("tts:caption", &caption);
            *cumulative += own_ms + NARRATION_GAP_MS;
            sink.append(SamplesBuffer::new(1, sr, audio));
            *appended += 1;
        };

        append(&sink, first, &mut appended, &mut cumulative_ms);
        for ev in tracker.update(appended, sink.len()) {
            on_event(ev);
        }
        loop {
            if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                sink.stop();
                return NarrateOutcome::Cancelled;
            }
            if !channel_done {
                loop {
                    match rx.try_recv() {
                        Ok(Some(item)) => {
                            append(&sink, item, &mut appended, &mut cumulative_ms);
                        }
                        Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            channel_done = true;
                            break;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    }
                }
            }
            for ev in tracker.update(appended, sink.len()) {
                on_event(ev);
            }
            if channel_done && sink.empty() {
                return NarrateOutcome::Completed;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    })
    .await
    .unwrap_or_else(|_| NarrateOutcome::Unavailable("audio thread panicked".to_string()));

    if volume_changed {
        crate::volume::restore_volume();
    }
    if outcome != NarrateOutcome::Cancelled {
        // Same grace as speak_text; a newer generation owns the flag when cancelled.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        meeting.set_tts_playing(false);
    }
    outcome
}

#[cfg(test)]
mod narration_tests {
    use super::*;
    use NarrationEvent::*;

    #[test]
    fn tracker_first_item_starts_immediately() {
        let mut t = NarrationTracker::new();
        assert_eq!(t.update(1, 1), vec![Started(0)]);
        assert!(t.update(1, 1).is_empty()); // idempotent
    }

    #[test]
    fn tracker_gapless_transition() {
        let mut t = NarrationTracker::new();
        t.update(1, 1);
        assert!(t.update(2, 2).is_empty());
        assert_eq!(t.update(2, 1), vec![Ended(0), Started(1)]);
        assert_eq!(t.update(2, 0), vec![Ended(1)]);
    }

    #[test]
    fn tracker_underrun_gap_clears_before_next() {
        // Item 0 finishes before item 1 is synthesized.
        let mut t = NarrationTracker::new();
        t.update(1, 1);
        assert_eq!(t.update(1, 0), vec![Ended(0)]);
        // Item 1 arrives later: starts only now.
        assert_eq!(t.update(2, 1), vec![Started(1)]);
    }

    #[test]
    fn tracker_missed_polls_still_ordered() {
        // A long stall: two items finished between polls.
        let mut t = NarrationTracker::new();
        assert_eq!(
            t.update(3, 1),
            vec![Started(0), Ended(0), Started(1), Ended(1), Started(2)]
        );
    }

    #[test]
    fn tracker_never_emits_start_before_prior_end() {
        let mut t = NarrationTracker::new();
        let mut open: Option<usize> = None;
        let mut appended = 0usize;
        let mut len = 0usize;
        // deterministic pseudo-random walk
        let mut seed = 12345u64;
        for _ in 0..500 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let r = (seed >> 33) % 3;
            if r == 0 && appended < 8 {
                appended += 1;
                len += 1;
            } else if len > 0 {
                len -= 1;
            }
            for ev in t.update(appended, len) {
                match ev {
                    Started(i) => {
                        assert!(open.is_none(), "start({i}) while {open:?} open");
                        open = Some(i);
                    }
                    Ended(i) => {
                        assert_eq!(open, Some(i));
                        open = None;
                    }
                }
            }
        }
    }

    #[test]
    fn silence_padding_length() {
        let mut s = Vec::new();
        pad_silence(&mut s, 24_000, 250);
        assert_eq!(s.len(), 6_000);
    }
}

#[cfg(test)]
mod streaming_tests {
    use super::*;

    #[test]
    fn test_split_single_sentence() {
        assert_eq!(split_sentences("Hello sir."), vec!["Hello sir."]);
    }

    #[test]
    fn test_split_multi_sentence() {
        let out = split_sentences("First. Second! Third?");
        assert_eq!(out, vec!["First.", "Second!", "Third?"]);
    }

    #[test]
    fn test_split_newlines() {
        let out = split_sentences("Line one\nLine two");
        assert_eq!(out, vec!["Line one", "Line two"]);
    }

    #[test]
    fn test_split_no_delimiter() {
        assert_eq!(split_sentences("hello"), vec!["hello"]);
    }

    #[test]
    fn test_split_empty_tail_ignored() {
        let out = split_sentences("Done. ");
        assert_eq!(out, vec!["Done."]);
    }

    #[test]
    fn test_split_unicode_safe() {
        let out = split_sentences("の test. Second.");
        assert_eq!(out.len(), 2);
    }
}

#[cfg(test)]
mod caption_tests {
    use super::*;
    use edge_tts_rust::{Boundary, BoundaryEvent};

    #[test]
    fn test_estimate_words_empty_text() {
        assert_eq!(estimate_words("   ", 1000), Vec::new());
        assert_eq!(estimate_words("", 1000), Vec::new());
    }

    #[test]
    fn test_estimate_words_single_word_gets_full_duration() {
        let words = estimate_words("Hello", 500);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].start_ms, 0);
        assert_eq!(words[0].duration_ms, 500);
    }

    #[test]
    fn test_estimate_words_evenly_distributed() {
        let words = estimate_words("one two three four", 800);
        assert_eq!(words.len(), 4);
        assert_eq!(words[0].start_ms, 0);
        assert_eq!(words[1].start_ms, 200);
        assert_eq!(words[2].start_ms, 400);
        assert_eq!(words[3].start_ms, 600);
        assert!(words.iter().all(|w| w.duration_ms == 200));
    }

    #[test]
    fn test_boundaries_to_words_converts_ticks_to_ms() {
        // 10_000_000 ticks/sec -> 10_000 ticks/ms.
        let boundaries = vec![
            BoundaryEvent { kind: Boundary::Word, offset_ticks: 0, duration_ticks: 30_000, text: "Hi".into() },
            BoundaryEvent { kind: Boundary::Word, offset_ticks: 50_000, duration_ticks: 40_000, text: "there".into() },
        ];
        let words = boundaries_to_words(&boundaries, 0);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0], CaptionWord { text: "Hi".into(), start_ms: 0, duration_ms: 3 });
        assert_eq!(words[1], CaptionWord { text: "there".into(), start_ms: 5, duration_ms: 4 });
    }

    #[test]
    fn test_boundaries_to_words_applies_cumulative_offset() {
        // Streaming chunk 2+: its own boundaries start at 0, but the
        // consumer must offset them by everything already queued.
        let boundaries = vec![BoundaryEvent {
            kind: Boundary::Word,
            offset_ticks: 0,
            duration_ticks: 10_000,
            text: "second".into(),
        }];
        let words = boundaries_to_words(&boundaries, 1500);
        assert_eq!(words[0].start_ms, 1500);
    }

    #[test]
    fn test_boundaries_to_words_unescapes_xml_entities() {
        let boundaries = vec![
            BoundaryEvent {
                kind: Boundary::Word,
                offset_ticks: 0,
                duration_ticks: 10_000,
                text: "didn&apos;t".into(),
            },
            BoundaryEvent {
                kind: Boundary::Word,
                offset_ticks: 10_000,
                duration_ticks: 10_000,
                text: "&quot;hello&quot;".into(),
            },
        ];
        let words = boundaries_to_words(&boundaries, 0);
        assert_eq!(words[0].text, "didn't");
        assert_eq!(words[1].text, "\"hello\"");
    }

    #[test]
    fn test_compute_envelope_empty_input() {
        assert!(compute_envelope(&[], 24000, ENVELOPE_FRAME_MS).is_empty());
        assert!(compute_envelope(&[0.1, 0.2], 0, ENVELOPE_FRAME_MS).is_empty());
    }

    #[test]
    fn test_compute_envelope_silence_is_all_zero() {
        let samples = vec![0.0f32; 24000]; // 1s of silence at 24kHz
        let env = compute_envelope(&samples, 24000, ENVELOPE_FRAME_MS);
        assert!(!env.is_empty());
        assert!(env.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_compute_envelope_normalizes_to_loudest_frame() {
        // Two 20ms frames at 1000Hz sample rate: frame 0 quiet, frame 1 loud.
        let sr = 1000u32;
        let frame_len = (sr as u64 * ENVELOPE_FRAME_MS / 1000) as usize; // 20 samples
        let mut samples = vec![0.1f32; frame_len];
        samples.extend(vec![0.5f32; frame_len]);
        let env = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
        assert_eq!(env.len(), 2);
        // Loudest frame normalizes to exactly 1.0; the quiet frame is
        // proportionally smaller, never negative, never above 1.0.
        assert!((env[1] - 1.0).abs() < 1e-5);
        assert!(env[0] > 0.0 && env[0] < env[1]);
    }

    #[test]
    fn test_compute_envelope_frame_count_matches_duration() {
        let sr = 24000u32;
        let samples = vec![0.3f32; sr as usize]; // exactly 1000ms
        let env = compute_envelope(&samples, sr, ENVELOPE_FRAME_MS);
        // 1000ms / 20ms per frame = 50 frames.
        assert_eq!(env.len(), 50);
    }
}
