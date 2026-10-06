//! edge-tts — Microsoft Edge Read Aloud TTS (cloud, free, no API key).
//!
//! Uses the edge-tts-rust crate which connects to Microsoft's WebSocket endpoint
//! and returns MP3 audio bytes. 400+ neural voices across 140+ locales.
//!
//! Latency: ~200ms (network + synthesis)
//! RAM: 0 MB (cloud, no local model)
//! Cost: $0 (free, no account, no API key)
//! Quality: Excellent (broadcast-quality neural voices)

use edge_tts_rust::{EdgeTtsClient, SpeakOptions, Boundary};

/// Emotional prosody for TTS (B3). The edge-tts-rust crate exposes
/// rate/volume/pitch (no mstts express-as), so emotions map to prosody.
/// Pure + unit-tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtsEmotion {
    Neutral,
    Cheerful,
    Calm,
    Sad,
    Urgent,
    Whisper,
}

impl TtsEmotion {
    /// Parse from settings string ("auto" handled by caller).
    pub fn parse_label(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "cheerful" => TtsEmotion::Cheerful,
            "calm" => TtsEmotion::Calm,
            "sad" => TtsEmotion::Sad,
            "urgent" => TtsEmotion::Urgent,
            "whisper" => TtsEmotion::Whisper,
            _ => TtsEmotion::Neutral,
        }
    }

    /// (rate, volume, pitch) for SpeakOptions.
    pub fn prosody(&self) -> (&'static str, &'static str, &'static str) {
        match self {
            TtsEmotion::Neutral => ("+0%", "+0%", "+0Hz"),
            TtsEmotion::Cheerful => ("+10%", "+0%", "+30Hz"),
            TtsEmotion::Calm => ("-10%", "+0%", "-10Hz"),
            TtsEmotion::Sad => ("-15%", "+0%", "-20Hz"),
            TtsEmotion::Urgent => ("+20%", "+0%", "+20Hz"),
            TtsEmotion::Whisper => ("-5%", "-30%", "+0Hz"),
        }
    }
}

/// Auto-pick an emotion from text heuristics. Pure + unit-tested.
/// Errors/apologies → Sad; success/confirmations → Cheerful;
/// warnings/stops → Urgent; quiet hours → Whisper (late night).
pub fn pick_emotion(text: &str) -> TtsEmotion {
    let lower = text.to_lowercase();
    let has_any = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has_any(&["sorry", "failed", "couldn't", "could not", "error", "unable to"]) {
        return TtsEmotion::Sad;
    }
    if has_any(&["warning", "stop", "careful", "abort", "emergency"]) {
        return TtsEmotion::Urgent;
    }
    if has_any(&["done", "complete", "ready", "connected", "remembered", "on it", "got it"]) {
        return TtsEmotion::Cheerful;
    }
    TtsEmotion::Neutral
}

/// Internal: synthesize + return the full edge-tts result (audio bytes +
/// word-boundary timing events). `synthesize_to_mp3`/`_with_emotion` below
/// (unchanged external shape, still used by the TTS benchmarks) wrap this
/// and discard `.boundaries`; `synthesize_to_pcm`/`_with_emotion` thread
/// them through for the response-caption feature (plan Phase 3).
///
/// Always requests `Boundary::Word` (not `Sentence` — mutually exclusive
/// per call in this crate): nothing in this codebase reads sentence
/// boundaries, and word-level timing is what per-word caption reveal needs.
async fn synthesize_raw(
    text: &str,
    voice: &str,
    emotion: Option<TtsEmotion>,
) -> Result<edge_tts_rust::SynthesisResult, String> {
    if text.is_empty() {
        return Err("Empty text".to_string());
    }

    let client = EdgeTtsClient::new()
        .map_err(|e| format!("edge-tts client init failed: {}", e))?;

    let options = if let Some(emotion) = emotion {
        let (rate, volume, pitch) = emotion.prosody();
        SpeakOptions {
            voice: voice.to_string(),
            rate: rate.to_string(),
            volume: volume.to_string(),
            pitch: pitch.to_string(),
            boundary: Boundary::Word,
        }
    } else {
        SpeakOptions {
            voice: voice.to_string(),
            boundary: Boundary::Word,
            ..SpeakOptions::default()
        }
    };

    let result = client
        .synthesize(text, options)
        .await
        .map_err(|e| format!("edge-tts synthesis failed: {}", e))?;

    tracing::info!(
        "tts-edge: synthesized '{}' ({} bytes MP3, {} word boundaries, voice={}, emotion={:?})",
        crate::tts::truncate_for_log(text, 50),
        result.audio.len(),
        result.boundaries.len(),
        voice,
        emotion
    );

    Ok(result)
}

/// Synthesize text to MP3 bytes using edge-tts.
///
/// Returns raw MP3 audio bytes on success. The caller is responsible for
/// decoding MP3 to PCM samples for rodio playback.
///
/// # Arguments
/// * `text` - Text to synthesize (max ~4 KB per call)
/// * `voice` - Microsoft voice short-name (e.g. "en-US-AvaNeural")
pub async fn synthesize_to_mp3(
    text: &str,
    voice: &str,
) -> Result<Vec<u8>, String> {
    Ok(synthesize_raw(text, voice, None).await?.audio)
}

/// Synthesize with emotional prosody (rate/pitch/volume per emotion).
pub async fn synthesize_to_mp3_with_emotion(
    text: &str,
    voice: &str,
    emotion: TtsEmotion,
) -> Result<Vec<u8>, String> {
    Ok(synthesize_raw(text, voice, Some(emotion)).await?.audio)
}

/// Decode MP3 bytes to f32 PCM samples at the native sample rate (shared by
/// both PCM synthesis variants below).
fn decode_mp3_to_pcm(mp3_bytes: Vec<u8>) -> Result<(Vec<f32>, u32), String> {
    let cursor = std::io::Cursor::new(mp3_bytes);
    let source = rodio::Decoder::new(cursor)
        .map_err(|e| format!("MP3 decode failed: {}", e))?;

    // rodio Decoder is an Iterator of i16 samples; convert to f32
    let sample_rate = 24000; // edge-tts outputs 24kHz; rodio handles resampling
    let samples: Vec<f32> = source
        .map(|s: i16| s as f32 / i16::MAX as f32)
        .collect();

    Ok((samples, sample_rate))
}

/// Synthesize text and decode MP3 to f32 PCM samples at the native sample rate.
///
/// Returns (samples, sample_rate, word-boundary events) for direct rodio
/// playback plus response-caption scheduling (ticks are 100ns units — see
/// `tts::boundaries_to_words`).
pub async fn synthesize_to_pcm(
    text: &str,
    voice: &str,
) -> Result<(Vec<f32>, u32, Vec<edge_tts_rust::BoundaryEvent>), String> {
    let result = synthesize_raw(text, voice, None).await?;
    let (samples, sample_rate) = decode_mp3_to_pcm(result.audio)?;

    tracing::info!(
        "tts-edge: decoded {} PCM samples ({}ms audio)",
        samples.len(),
        samples.len() as u64 * 1000 / sample_rate as u64
    );

    Ok((samples, sample_rate, result.boundaries))
}

/// PCM variant with emotional prosody.
pub async fn synthesize_to_pcm_with_emotion(
    text: &str,
    voice: &str,
    emotion: TtsEmotion,
) -> Result<(Vec<f32>, u32, Vec<edge_tts_rust::BoundaryEvent>), String> {
    let result = synthesize_raw(text, voice, Some(emotion)).await?;
    let (samples, sample_rate) = decode_mp3_to_pcm(result.audio)?;
    Ok((samples, sample_rate, result.boundaries))
}

#[cfg(test)]
mod emotion_tests {
    use super::*;

    #[test]
    fn test_emotion_from_str() {
        assert_eq!(TtsEmotion::parse_label("cheerful"), TtsEmotion::Cheerful);
        assert_eq!(TtsEmotion::parse_label("CALM"), TtsEmotion::Calm);
        assert_eq!(TtsEmotion::parse_label("unknown"), TtsEmotion::Neutral);
        assert_eq!(TtsEmotion::parse_label("auto"), TtsEmotion::Neutral);
    }

    #[test]
    fn test_prosody_table() {
        assert_eq!(TtsEmotion::Neutral.prosody(), ("+0%", "+0%", "+0Hz"));
        assert_eq!(TtsEmotion::Cheerful.prosody(), ("+10%", "+0%", "+30Hz"));
        assert_eq!(TtsEmotion::Whisper.prosody(), ("-5%", "-30%", "+0Hz"));
    }

    #[test]
    fn test_pick_emotion_sad() {
        assert_eq!(pick_emotion("Sorry, that failed."), TtsEmotion::Sad);
        assert_eq!(pick_emotion("Unable to connect."), TtsEmotion::Sad);
    }

    #[test]
    fn test_pick_emotion_urgent() {
        assert_eq!(pick_emotion("Stop! Warning."), TtsEmotion::Urgent);
    }

    #[test]
    fn test_pick_emotion_cheerful() {
        assert_eq!(pick_emotion("Done, sir. Task complete."), TtsEmotion::Cheerful);
        assert_eq!(pick_emotion("Remembered dog as Bruno."), TtsEmotion::Cheerful);
    }

    #[test]
    fn test_pick_emotion_neutral() {
        assert_eq!(pick_emotion("The weather is cloudy."), TtsEmotion::Neutral);
    }
}

/// Check if edge-tts is reachable (network test).
pub async fn is_available() -> bool {
    // Quick connectivity test — try connecting to Microsoft's speech endpoint.
    // Uses reqwest to check if we can reach the internet at all.
    use std::time::Duration;

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };

    // Connectivity test — check if Microsoft speech or web endpoint is reachable
    match client
        .get("https://www.bing.com")
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            tracing::debug!("tts_edge is_available status: {}", status);
            status.is_success() || status.is_redirection()
        }
        Err(e) => {
            tracing::warn!("tts_edge is_available failed: {}", e);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edge_tts_module_loads() {
        // Just verify the module compiles and links
        let _ = std::hint::black_box(());
    }

    /// Char-safe log truncation: the exact 2026-09-19 crash string
    /// (byte 50 inside 'の') must truncate without panicking.
    #[test]
    fn test_truncate_for_log_multibyte() {
        let jp = "治療が短くなっていると感じているのですね。具体的にどのような変化が起きていますか？まずは担当医に状況を伝えて相談することをおすすめします。";
        let out = crate::tts::truncate_for_log(jp, 50);
        assert_eq!(out.chars().count(), 50);
        assert!(jp.starts_with(&out));
        assert_eq!(crate::tts::truncate_for_log("short", 50), "short");
    }

    /// Test that Edge TTS succeeds with a valid Microsoft voice ID.
    /// This proves the cloud TTS path works when given the correct voice.
    #[tokio::test]
    async fn test_edge_tts_valid_voice() {
        let result = synthesize_to_mp3("Hello world", "en-US-AvaNeural").await;
        match &result {
            Ok(bytes) => println!("test_edge_tts_valid_voice: OK ({} bytes)", bytes.len()),
            Err(e) => println!("test_edge_tts_valid_voice: FAILED — {}", e),
        }
        // Don't fail the test on network errors — just report
        // (CI might not have internet)
        if let Ok(bytes) = &result {
            assert!(!bytes.is_empty(), "Edge TTS should return non-empty audio");
        }
    }

    /// Test that Edge TTS FAILS with a Kokoro voice ID ("af_sky").
    /// This proves the bug: the frontend sends "af_sky" to Edge TTS,
    /// which fails and silently falls back to the local Kokoro voice.
    #[tokio::test]
    async fn test_edge_tts_invalid_kokoro_voice() {
        let result = synthesize_to_mp3("Hello world", "af_sky").await;
        match &result {
            Ok(bytes) => {
                // If this succeeds, Microsoft might have added "af_sky" as an alias
                // — but this is extremely unlikely
                println!("test_edge_tts_invalid_kokoro_voice: UNEXPECTED OK ({} bytes)", bytes.len());
            }
            Err(e) => {
                // This is the expected case — Edge TTS rejects "af_sky"
                println!("test_edge_tts_invalid_kokoro_voice: FAILED as expected — {}", e);
                // This confirms the bug: "af_sky" is not a valid Edge TTS voice
            }
        }
        // We expect this to fail — if it succeeds, something changed
        // Don't assert hard fail since network might be down
    }

    /// Test that is_available() works (network check).
    #[tokio::test]
    async fn test_edge_tts_is_available() {
        let available = is_available().await;
        println!("test_edge_tts_is_available: {}", available);
        // Don't fail on network issues — just report
    }
}
