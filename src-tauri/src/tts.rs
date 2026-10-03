//! TTS — 2-tier fallback: edge-tts (cloud) → Piper (local).
//!
//! Phase 2 architecture:
//!   Primary:   edge-tts (Microsoft Neural, cloud, $0, ~200ms, 0 MB RAM)
//!   Fallback:  Piper (local VITS ONNX, $0, ~40ms, 80 MB RAM, lazy-loaded)
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
use tauri::State;

/// Global Piper engine reference — set at TtsState creation.
/// Allows the network monitor to unload Piper without accessing TtsState.
static GLOBAL_PIPER_ENGINE: std::sync::OnceLock<crate::tts_piper::PiperEngine> =
    std::sync::OnceLock::new();

pub struct TtsState {
    /// Piper fallback engine (lazy-loaded only when edge-tts fails).
    pub piper_engine: crate::tts_piper::PiperEngine,
    /// Pre-synthesized short phrases for instant acknowledgment playback.
    /// Keyed by the exact phrase text. Stores f32 PCM samples + sample rate.
    pub cache: Arc<Mutex<HashMap<String, CachedAudio>>>,
}

/// Cached audio: PCM samples + sample rate for rodio playback.
#[derive(Clone)]
pub struct CachedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
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
        let piper_engine = crate::tts_piper::new_engine();
        // Store global reference for the network monitor to access
        let _ = GLOBAL_PIPER_ENGINE.set(piper_engine.clone());
        Self {
            piper_engine,
            cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Unload the Piper engine globally (called by network monitor after 10 min).
pub async fn unload_piper_global() {
    if let Some(engine) = GLOBAL_PIPER_ENGINE.get() {
        crate::tts_piper::unload_engine(engine).await;
    }
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
/// Falls back to Piper if edge-tts is unavailable.
/// This runs once at startup so all subsequent speak_cached() calls are instant (<5ms).
pub async fn pregenerate_cache(
    cache_arc: &Arc<Mutex<HashMap<String, CachedAudio>>>,
    voice: &str,
) {
    let cache_start = std::time::Instant::now();

    let mut cached_count = 0;

    // Always try edge-tts first (cloud, best quality).
    // If it fails, the Piper fallback below kicks in automatically.
    tracing::info!("tts: pre-generating cache with edge-tts (voice={})", voice);
    for phrase in CACHED_PHRASES {
        match crate::tts_edge::synthesize_to_pcm(phrase, voice).await {
                Ok((samples, sr)) => {
                    cache_arc.lock().await.insert(
                        phrase.to_string(),
                        CachedAudio { samples, sample_rate: sr },
                    );
                    cached_count += 1;
                }
                Err(e) => {
                    tracing::warn!("tts: edge-tts cache failed for '{}': {}", phrase, e);
                }
            }
        }

    // If edge-tts failed, try Piper for cache
    if cached_count == 0 {
        tracing::info!("tts: falling back to Piper for cache generation");
        // We need a temporary Piper engine for cache generation
        let piper_engine = crate::tts_piper::new_engine();
        for phrase in CACHED_PHRASES {
            match crate::tts_piper::synthesize(&piper_engine, phrase).await {
                Ok((samples, sr)) => {
                    cache_arc.lock().await.insert(
                        phrase.to_string(),
                        CachedAudio { samples, sample_rate: sr },
                    );
                    cached_count += 1;
                }
                Err(e) => {
                    tracing::warn!("tts: piper cache failed for '{}': {}", phrase, e);
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

/// IPC: Stop any currently-playing TTS audio.
#[tauri::command]
pub fn stop_tts() -> Result<(), String> {
    TTS_GENERATION.fetch_add(1, Ordering::SeqCst);
    tracing::info!("tts: stop requested (generation {})", TTS_GENERATION.load(Ordering::SeqCst));
    Ok(())
}

/// IPC: Speak text using the 3-tier fallback chain.
///
/// Tries edge-tts (cloud) first, then Piper (local), then eSpeak (last resort).
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
    let (audio, sample_rate) = if let Some(hit) = state.cache.lock().await.get(&text).cloned() {
        tracing::info!("tts: cache hit for '{}'", text);
        (hit.samples, hit.sample_rate)
    } else {
        // B2 streaming: long multi-sentence replies play chunk 0
        // immediately while the rest synthesizes. Short texts keep the
        // legacy single-shot path (zero behavior change, Piper fallback).
        // Piper voices also stay single-shot: local ~40ms synthesis needs
        // no streaming, and chunked edge-tts would mix voices mid-reply.
        let sentences = split_sentences(&text);
        let use_streaming = text.chars().count() >= STREAM_MIN_CHARS
            && sentences.len() > 1
            && crate::tts_piper::voice_stem(&voice_id).is_none();
        if use_streaming {
            // Set volume BEFORE playback (same as legacy path).
            let stream_volume_changed = if tts_volume_pct > 0 {
                let target = tts_volume_pct as f32 / 100.0;
                crate::volume::save_and_set_volume(target)
            } else {
                false
            };
            match speak_streaming(sentences, &voice_id, &state, my_generation, emotion).await {
                Ok(()) => {
                    if stream_volume_changed {
                        crate::volume::restore_volume();
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    meeting.set_tts_playing(false);
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!("tts: streaming failed ({}), falling back to single-shot", e);
                    if stream_volume_changed {
                        crate::volume::restore_volume();
                    }
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

    // Save and set system volume
    let volume_changed = if tts_volume_pct > 0 {
        let target = tts_volume_pct as f32 / 100.0;
        crate::volume::save_and_set_volume(target)
    } else {
        false
    };

    // Play audio
    let play_result = play_audio(audio, sample_rate, my_generation).await;

    // Restore volume
    if volume_changed {
        crate::volume::restore_volume();
    }

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
    _app: tauri::AppHandle,
) -> Result<(), String> {
    let demo_text = text.unwrap_or_else(|| {
        if voice_id.starts_with("en-US-") {
            let name = voice_id
                .strip_prefix("en-US-")
                .unwrap_or("")
                .trim_end_matches("Neural");
            format!("Hello, I'm {}. This is how I sound.", name)
        } else if voice_id == "piper-amy" {
            "Hello, I'm Amy. This is the offline voice.".to_string()
        } else {
            "Hello, this is a voice preview.".to_string()
        }
    });

    tracing::info!("tts: previewing voice '{}' with text '{}'", voice_id, demo_text);

    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    let (audio, sample_rate) = match synthesize_with_fallback(&demo_text, &voice_id, &state, crate::tts_edge::TtsEmotion::Neutral).await {
        Ok(result) => result,
        Err(e) => {
            tracing::error!("tts: voice preview failed for '{}': {}", voice_id, e);
            return Err(e);
        }
    };

    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        return Ok(());
    }

    play_audio(audio, sample_rate, my_generation).await
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
    settings.offline_voice_model = persona.local_model.to_string();
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    tracing::info!(
        "tts: voice equipped '{}' (cloud={}, twin={})",
        persona.key,
        persona.cloud_id,
        persona.local_model
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
        let engine = state.piper_engine.clone();
        let app_clone = app.clone();
        let key = persona.key.to_string();
        tokio::spawn(async move {
            crate::tts_swap::run_swap(app_clone, key, engine).await;
        });
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
/// ("cloud" = next utterance goes Edge, "local" = next goes Piper).
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
        "twin": persona.map(|p| p.local_model).unwrap_or(""),
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

    meeting.set_tts_playing(true);
    let my_generation = TTS_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

    // Try to get cached audio
    let cache_arc = state.cache.clone();
    let cached_audio = {
        let cache = cache_arc.lock().await;
        cache.get(&text).cloned()
    };

    let (audio, sample_rate) = match cached_audio {
        Some(ca) => {
            tracing::info!("tts: cache hit for '{}'", text);
            (ca.samples, ca.sample_rate)
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

    // Read TTS volume
    let tts_volume_pct = read_tts_volume(&app);
    let volume_changed = if tts_volume_pct > 0 {
        let target = tts_volume_pct as f32 / 100.0;
        crate::volume::save_and_set_volume(target)
    } else {
        false
    };

    // Play
    let play_result = play_audio(audio, sample_rate, my_generation).await;

    // Restore volume
    if volume_changed {
        crate::volume::restore_volume();
    }

    // Grace period
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    meeting.set_tts_playing(false);

    play_result
}

/// 2-tier synthesis fallback: edge-tts → Piper.
///
/// Returns (f32 PCM samples, sample_rate) on success.
async fn synthesize_with_fallback(
    text: &str,
    voice: &str,
    state: &TtsState,
    emotion: crate::tts_edge::TtsEmotion,
) -> Result<(Vec<f32>, u32), String> {
    // Piper voices (local): `piper-amy` legacy + any `piper-<stem>`
    // with model files present (bundled, cache, or user drop-in dir).
    if let Some(stem) = crate::tts_piper::voice_stem(voice) {
        tracing::info!("tts: using Piper voice '{stem}' (local)");
        crate::tts_network::mark_piper_loaded();
        return crate::tts_piper::synthesize_with_voice(&state.piper_engine, text, &stem).await;
    }

    // Upfront engine policy (Main Center TTS director, doc 74 P3): explicit
    // Piper voice handled above; network-down skips Edge entirely (saves
    // ~1-2s of waiting for Edge to fail). Identical branches to before —
    // the policy is now pinned + tested in `center::tts_engine_for`.
    //
    // NOTE: this is the CACHED flag only (maintained by the background
    // monitor + synthesis outcomes below). We used to run a live HTTPS
    // probe here on every call — it lied (reported down while Groq worked)
    // and added up to 5s before synthesis even started. The Edge attempt
    // itself, raced with a timeout, is the only honest connectivity check.
    if crate::center::tts_engine_for(false, crate::tts_network::is_network_up())
        == crate::center::TtsEngine::Piper
    {
        tracing::info!("tts: network down (cached) — using Piper directly (skipping Edge TTS)");
    } else {
        // Tier 1: edge-tts (cloud, ~200ms, best quality, 0 MB RAM),
        // raced with a timeout so a hanging endpoint can't stall speech.
        match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            crate::tts_edge::synthesize_to_pcm_with_emotion(text, voice, emotion),
        )
        .await
        {
            Ok(Ok((samples, sr))) => {
                tracing::info!("tts: edge-tts synthesis OK (cloud)");
                crate::tts_network::set_network_up();
                return Ok((samples, sr));
            }
            Err(_elapsed) => {
                // Timeout = genuine transport failure → mark down so the
                // next call skips Edge (saves 8s per call while offline).
                tracing::warn!("tts: edge-tts timed out, trying Piper fallback");
                crate::tts_network::set_network_down();
            }
            Ok(Err(e)) => {
                // Content/API rejection (bad voice, unsupported script, 4xx)
                // is NOT a network outage — a single bad sentence must not
                // poison the global flag (measured 2026-09-19: Japanese text
                // → English voice rejection → "network down" → forced Piper
                // → panic). Fall through to Piper for THIS call only.
                tracing::warn!("tts: edge-tts rejected ({e}), trying Piper fallback (network flag untouched)");
            }
        }
    }

    // Tier 2: Piper (local, ~40ms, good quality, ~80 MB RAM)
    crate::tts_network::mark_piper_loaded();
    match crate::tts_piper::synthesize(&state.piper_engine, text).await {
        Ok((samples, sr)) => {
            tracing::info!("tts: piper fallback synthesis OK (local)");
            return Ok((samples, sr));
        }
        Err(e) => {
            tracing::error!("tts: piper fallback also failed: {}", e);
            Err(format!("All TTS engines failed. Last error: {}", e))
        }
    }
}

/// Play f32 PCM audio through rodio with barge-in support.
async fn play_audio(
    audio: Vec<f32>,
    sample_rate: u32,
    my_generation: usize,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => {
                match Sink::try_new(&handle) {
                    Ok(sink) => {
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
fn read_tts_emotion_setting(app: &tauri::AppHandle) -> String {
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
fn resolve_emotion(app: &tauri::AppHandle, text: &str) -> crate::tts_edge::TtsEmotion {
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
fn read_tts_volume(app: &tauri::AppHandle) -> u8 {

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
) -> Result<(Vec<f32>, u32), String> {
    if let Some(hit) = state.cache.lock().await.get(sentence).cloned() {
        return Ok((hit.samples, hit.sample_rate));
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
) -> Result<(), String> {
    // Chunk 0 first — this defines first-audio latency.
    if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
        return Ok(());
    }
    let (first_audio, first_sr) = synthesize_chunk(&sentences[0], voice_id, state, emotion).await?;

    let (stream_tx, stream_rx) =
        std::sync::mpsc::channel::<Option<(Vec<f32>, u32)>>();
    // Producer: synthesize remaining chunks while chunk 0 plays.
    let rest: Vec<String> = sentences.into_iter().skip(1).collect();
    let voice_owned = voice_id.to_string();
    let state_ref = TtsStateRef {
        cache: state.cache.clone(),
    };
    let gen = my_generation;
    tokio::spawn(async move {
        // Re-resolve Piper engine via the global (spawned task can't borrow state).
        for s in rest {
            if TTS_GENERATION.load(Ordering::SeqCst) > gen {
                break;
            }
            let res = synthesize_chunk_owned(&s, &voice_owned, &state_ref, emotion).await;
            match res {
                Ok(pair) => {
                    if stream_tx.send(Some(pair)).is_err() {
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
    let first = Some((first_audio, first_sr));
    play_audio_streamed(first, stream_rx, my_generation).await
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
) -> Result<(Vec<f32>, u32), String> {
    if let Some(hit) = state.cache.lock().await.get(sentence).cloned() {
        return Ok((hit.samples, hit.sample_rate));
    }
    // Piper engine lives in TtsState (not Send-shareable here); the fallback
    // chain needs it — route through edge-tts directly, Piper on failure
    // is handled by the caller falling back to legacy path on chunk-0 error.
    // For chunks 1+, edge-tts failure ends the stream gracefully (we play
    // what we have) instead of blocking on Piper load.
    let pcm = crate::tts_edge::synthesize_to_pcm_with_emotion(sentence, voice_id, emotion).await?;
    Ok(pcm)
}

/// Play chunk 0 immediately, then append streamed chunks as they arrive.
async fn play_audio_streamed(
    first: Option<(Vec<f32>, u32)>,
    rx: std::sync::mpsc::Receiver<Option<(Vec<f32>, u32)>>,
    my_generation: usize,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => match Sink::try_new(&handle) {
                Ok(sink) => {
                    let mut first_sr = 24000;
                    if let Some((audio, sr)) = first {
                        first_sr = sr;
                        sink.append(SamplesBuffer::new(1, sr, audio));
                    }
                    let _ = first_sr;
                    // Drain the producer channel (blocking recv is fine here —
                    // we're on a blocking thread, audio plays async).
                    while let Ok(msg) = rx.recv() {
                        if TTS_GENERATION.load(Ordering::SeqCst) > my_generation {
                            sink.stop();
                            tracing::info!("tts: streaming stopped by user (barge-in)");
                            return Ok(());
                        }
                        match msg {
                            Some((audio, sr)) => {
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
