//! Offline voice-accuracy evaluation (Phase A of the wake-word / STT accuracy plan).
//!
//! Purpose: measure the REAL runtime on audio the model never trained on, so the
//! numbers in `docs/research/voice-accuracy-scorecard-*.md` are measurements and
//! not self-reported training metrics. Nothing here is used by the app itself;
//! it backs `examples/wake_eval.rs` and `examples/stt_eval.rs`.
//!
//! Design rules:
//!   * Drive the production code, never a re-implementation: the wake clips go
//!     through `engine::WakeEngine::process` (the exact call the audio callback
//!     makes), and transcripts go through `stt::filter_transcript_counted` and
//!     `intent_parser::parse_deterministic`.
//!   * Replace only the two WALL-CLOCK fields of the engine (startup grace and
//!     the 3 s refractory) with the AUDIO clock. Offline processing runs much
//!     faster than real time, so left alone they would suppress detections for
//!     minutes of audio after the first trigger and under-count false alarms.
//!   * Mirror (not change) the front-end DSP: stereo downmix, quad-array pair
//!     selection and the linear resampler from `engine::on_audio`.
//!
//! Known, intentional limits (printed in the scorecard footnotes):
//!   * The receiver loop's 1.5 s candidate cooldown is not modelled here; the
//!     engine's own 3 s refractory is the larger of the two.
//!   * Stage-2 (STT "is it nexus?") is evaluated by the example, which needs a
//!     Groq key; `stage2_window` builds the same 2.5 s ring the verifier sees.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use crate::wakeword_oww::engine::WakeEngine;
use crate::wakeword_oww::engine::OWW_CHUNK_SIZE;

/// Sample rate the wake engine runs at.
pub const SR: u32 = 16_000;
/// Stage-2 verifier ring: last 2.5 s of raw audio (`VERIFY_RING_CAP` = 40 000).
pub const VERIFY_WINDOW_SAMPLES: usize = 40_000;

// ─── WAV decoding ────────────────────────────────────────────────────────

/// Decoded WAV: interleaved samples scaled to [-1, 1].
#[derive(Debug, Clone, PartialEq)]
pub struct WavData {
    pub sample_rate: u32,
    pub channels: u16,
    pub samples: Vec<f32>,
}

fn le_u16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn le_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Parse a RIFF/WAVE file: PCM 8/16/24/32-bit, IEEE float 32/64, and the
/// WAVE_FORMAT_EXTENSIBLE wrapper recorders such as `soundfile` emit.
/// Tolerates a truncated `data` chunk (streamed files with size 0xFFFFFFFF).
pub fn parse_wav(bytes: &[u8]) -> Result<WavData, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("not a RIFF/WAVE file".to_string());
    }
    let mut pos = 12usize;
    let mut fmt: Option<(u16, u16, u32, u16)> = None; // tag, channels, rate, bits
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = le_u32(bytes, pos + 4).unwrap_or(0) as usize;
        let start = pos + 8;
        let end = start.saturating_add(size).min(bytes.len());
        match id {
            b"fmt " => {
                let body = &bytes[start..end];
                let mut tag = le_u16(body, 0).ok_or("short fmt chunk")?;
                let channels = le_u16(body, 2).ok_or("short fmt chunk")?;
                let rate = le_u32(body, 4).ok_or("short fmt chunk")?;
                let bits = le_u16(body, 14).ok_or("short fmt chunk")?;
                if tag == 0xFFFE {
                    // EXTENSIBLE: the real format tag is the first 2 bytes of the sub-format GUID.
                    tag = le_u16(body, 24).ok_or("short extensible fmt chunk")?;
                }
                fmt = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(&bytes[start..end]),
            _ => {}
        }
        pos = start.saturating_add(size).saturating_add(size & 1);
    }
    let (tag, channels, rate, bits) = fmt.ok_or("missing fmt chunk")?;
    let data = data.ok_or("missing data chunk")?;
    if channels == 0 || rate == 0 {
        return Err("invalid fmt chunk (zero channels or rate)".to_string());
    }
    let samples: Vec<f32> = match (tag, bits) {
        (1, 8) => data.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
        (1, 16) => data
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
            .collect(),
        (1, 24) => data
            .chunks_exact(3)
            .map(|c| {
                let v = ((c[2] as i32) << 24 | (c[1] as i32) << 16 | (c[0] as i32) << 8) >> 8;
                v as f32 / 8_388_608.0
            })
            .collect(),
        (1, 32) => data
            .chunks_exact(4)
            .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f32 / 2_147_483_648.0)
            .collect(),
        (3, 32) => data
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
        (3, 64) => data
            .chunks_exact(8)
            .map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]) as f32)
            .collect(),
        _ => return Err(format!("unsupported WAV format tag={tag} bits={bits}")),
    };
    Ok(WavData { sample_rate: rate, channels, samples })
}

/// Downmix to mono and resample to 16 kHz exactly like `engine::on_audio`:
/// quad arrays average only the first stereo pair; everything else averages all
/// channels; resampling is plain linear interpolation (no anti-alias filter —
/// that gap is a known accuracy risk, so it is reproduced, not hidden).
pub fn to_mono_16k(wav: &WavData) -> Vec<f32> {
    let ch = (wav.channels as usize).max(1);
    let active = if ch == 4 { 2 } else { ch };
    let frames = wav.samples.len() / ch;
    let mut mono = Vec::with_capacity(frames);
    for i in 0..frames {
        let mut sum = 0.0f32;
        for c in 0..active {
            sum += wav.samples[i * ch + c];
        }
        mono.push(sum / active as f32);
    }
    if wav.sample_rate == SR || mono.is_empty() {
        return mono;
    }
    let ratio = wav.sample_rate as f64 / SR as f64;
    let mut out = Vec::with_capacity((mono.len() as f64 / ratio) as usize + 1);
    let mut pos = 0.0f64;
    while pos + ratio < mono.len() as f64 {
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(mono.len() - 1);
        let t = pos - i0 as f64;
        out.push((mono[i0] as f64 * (1.0 - t) + mono[i1] as f64 * t) as f32);
        pos += ratio;
    }
    out
}

/// Load a WAV file and convert it to the engine's 16 kHz mono f32 stream.
pub fn load_audio_16k(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let wav = parse_wav(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(to_mono_16k(&wav))
}

/// f32 [-1, 1] -> i16 (what the STT stage receives).
pub fn to_i16(audio: &[f32]) -> Vec<i16> {
    audio
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * 32767.0) as i16)
        .collect()
}

/// RMS of a slice.
pub fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// Milliseconds until the first 80 ms chunk whose RMS clears `gate` (speech onset).
pub fn speech_onset_ms(audio: &[f32], gate: f32) -> Option<u64> {
    audio
        .chunks(OWW_CHUNK_SIZE)
        .position(|c| rms(c) > gate)
        .map(|i| (i * OWW_CHUNK_SIZE) as u64 * 1000 / SR as u64)
}

// ─── Wake engine driver ──────────────────────────────────────────────────

/// One stage-1 trigger.
#[derive(Debug, Clone, PartialEq)]
pub struct Fire {
    /// Milliseconds from the start of the audio fed to the engine.
    pub t_ms: u64,
    /// Detection score the engine reported (`pending_probability`).
    pub prob: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WakeRun {
    pub fires: Vec<Fire>,
    /// Max of the raw per-frame probabilities and the score of any fire.
    pub max_prob: f32,
    pub duration_ms: u64,
}

/// Build the production engine from `resources` (`<resources>/oww/*.onnx`),
/// falling back to the crate's own `resources/` directory. The speaker-verifier
/// app-data dir is a throw-away path, so a developer's enrolled profile can
/// never change a measurement.
pub fn new_wake_engine(resources: Option<&Path>) -> Result<WakeEngine, String> {
    let resource_dir: PathBuf = match resources {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    };
    let scratch = std::env::temp_dir().join("nexus-voice-eval-appdata");
    WakeEngine::new(resource_dir, scratch).map_err(|e| e.to_string())
}

/// Clear all streaming state so consecutive clips cannot influence each other.
pub fn reset_engine_state(e: &mut WakeEngine) {
    e.audio_features.reset();
    e.detections_buffer.clear();
    e.preprocessor.reset();
    e.chunk_buffer.clear();
    e.confirmation_active = false;
    e.confirmation_buffer.clear();
    e.pending_probability = 0.0;
}

/// An `Instant` that is `ago` in the past (clamped when the platform's
/// monotonic clock is younger than `ago`).
fn instant_ago(ago: Duration) -> Instant {
    let now = Instant::now();
    now.checked_sub(ago).unwrap_or(now)
}

/// Stream 16 kHz audio through the engine in 80 ms chunks, with the engine's
/// wall-clock fields slaved to the audio clock. A trailing partial chunk is
/// zero-padded so the last syllables are not dropped.
pub fn run_wake_engine(e: &mut WakeEngine, audio: &[f32], threshold: Option<f32>) -> WakeRun {
    if let Some(t) = threshold {
        e.threshold = t;
    }
    let mut run = WakeRun {
        duration_ms: audio.len() as u64 * 1000 / SR as u64,
        ..Default::default()
    };
    let mut last_fire_ms: Option<u64> = None;
    let mut buf = vec![0.0f32; OWW_CHUNK_SIZE];
    for (i, chunk) in audio.chunks(OWW_CHUNK_SIZE).enumerate() {
        let t_ms = (i * OWW_CHUNK_SIZE) as u64 * 1000 / SR as u64;
        buf[..chunk.len()].copy_from_slice(chunk);
        buf[chunk.len()..].fill(0.0);

        // Audio clock instead of wall clock (see module docs).
        e.engine_start_time = instant_ago(Duration::from_secs(3600));
        e.last_detection_time = match last_fire_ms {
            None => instant_ago(Duration::from_secs(3600)),
            Some(l) => instant_ago(Duration::from_millis(t_ms.saturating_sub(l))),
        };

        let fired = e.process(&buf);
        if fired {
            let prob = e.pending_probability;
            run.max_prob = run.max_prob.max(prob);
            run.fires.push(Fire { t_ms, prob });
            last_fire_ms = Some(t_ms);
        } else if let Some(&p) = e.detections_buffer.to_vec().last() {
            run.max_prob = run.max_prob.max(p);
        }
    }
    run
}

/// Result of one labelled wake clip.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipVerdict {
    pub fired: bool,
    pub n_fires: usize,
    /// First trigger, ms from the start of the clip (head padding removed).
    pub first_fire_ms: Option<u64>,
    /// Estimated speech onset, ms from the start of the clip.
    pub onset_ms: Option<u64>,
    pub first_fire_prob: Option<f32>,
    pub max_prob: f32,
    /// The 2.5 s window the stage-2 verifier would transcribe (empty if no fire).
    pub stage2_window: Vec<f32>,
}

/// Head/tail zero padding applied around a clip: a real mic is always streaming
/// room noise, so the engine's buffers must be warm and flushed.
pub const HEAD_PAD_MS: usize = 1_000;
pub const TAIL_PAD_MS: usize = 1_500;

/// Evaluate one clip with a freshly reset engine.
pub fn eval_wake_clip(e: &mut WakeEngine, audio: &[f32], threshold: Option<f32>) -> ClipVerdict {
    reset_engine_state(e);
    let head = HEAD_PAD_MS * SR as usize / 1000;
    let tail = TAIL_PAD_MS * SR as usize / 1000;
    let mut padded = Vec::with_capacity(head + audio.len() + tail);
    padded.resize(head, 0.0);
    padded.extend_from_slice(audio);
    padded.resize(head + audio.len() + tail, 0.0);

    let run = run_wake_engine(e, &padded, threshold);
    let rel = |ms: u64| ms.saturating_sub(HEAD_PAD_MS as u64);
    let first = run.fires.first();
    let stage2_window = first
        .map(|f| {
            // The verifier snapshots the ring when the candidate arrives: the
            // last 2.5 s up to (and including) the detecting chunk.
            let end = (((f.t_ms as usize + 80) * SR as usize) / 1000).min(padded.len());
            let start = end.saturating_sub(VERIFY_WINDOW_SAMPLES);
            padded[start..end].to_vec()
        })
        .unwrap_or_default();
    ClipVerdict {
        fired: first.is_some(),
        n_fires: run.fires.len(),
        first_fire_ms: first.map(|f| rel(f.t_ms)),
        onset_ms: speech_onset_ms(audio, 0.01),
        first_fire_prob: first.map(|f| f.prob),
        max_prob: run.max_prob,
        stage2_window,
    }
}

// ─── Text stage (filter + wake-prefix strip + parser) ────────────────────

/// Resolved intent label as the e2e fixture computes it: the inner NLU/live
/// intent for wrapped results, the snake label otherwise, `"none"` on a miss.
pub fn resolved_label(transcript: &str) -> String {
    use crate::intent_parser::{intent_to_label, parse_deterministic, ParsedIntent};
    match parse_deterministic(transcript) {
        None => "none".to_string(),
        Some(r) => match &r.intent {
            ParsedIntent::NluResult { intent, .. } => intent.clone(),
            other => intent_to_label(other).to_string(),
        },
    }
}

/// What the production text pipeline does with one raw STT transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOutcome {
    pub raw: String,
    pub filtered: String,
    /// The hallucination filter turned a non-empty transcript into nothing.
    pub dropped_by_filter: bool,
    pub intent: String,
}

pub fn text_stage(raw: &str) -> TextOutcome {
    let filtered = crate::stt::filter_transcript_counted(raw);
    let dropped = !raw.trim().is_empty() && filtered.trim().is_empty();
    let intent = resolved_label(&filtered);
    TextOutcome { raw: raw.to_string(), filtered, dropped_by_filter: dropped, intent }
}

/// Acoustic pre-gates `stt::transcribe_samples` applies before any STT call
/// (mirrors stt.rs: < 200 ms, or mean RMS < 0.005). Returns the rejection reason.
pub fn acoustic_gate(samples: &[i16]) -> Option<&'static str> {
    if samples.len() < 3200 {
        return Some("too_short");
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|&s| {
            let f = s as f64 / 32768.0;
            f * f
        })
        .sum();
    if (sum_sq / samples.len() as f64).sqrt() < 0.005 {
        return Some("low_energy");
    }
    None
}

// ─── Stage-2 (STT "is it nexus?") helpers ────────────────────────────────

/// Exactly what the verifier sends to STT: the 2.5 s ring, VAD-trimmed, as i16.
pub fn stage2_samples(window: &[f32]) -> Vec<i16> {
    to_i16(&crate::wakeword_oww::vad_trim(window))
}

/// The production two-key transcript gate (exact "nexus", or a near-word with
/// stage-1 probability >= `VERIFY_NEAR_PROB_FLOOR`).
pub fn stage2_gate(transcript: &str, prob: f32) -> bool {
    crate::wakeword_oww::verify_transcript_gated(transcript, prob)
}

/// Which STT engine produced a transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SttEngine {
    Groq,
    Local,
}

/// Decoder prompt variants used in production (`DecoderBias`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bias {
    /// `NEXUS_VOCABULARY` — only sent today for owner-verified turns.
    Owner,
    /// `COMMAND_VOCABULARY` — the generic hints an unenrolled turn gets when
    /// `sttCommandHints` is on. A/B this against `Neutral`.
    Commands,
    /// The neutral English prompt every other turn gets.
    Neutral,
}

/// Transcribe i16 16 kHz audio through the production STT call.
pub async fn transcribe(
    samples: &[i16],
    engine: SttEngine,
    bias: Bias,
    groq_key: &str,
    client: &reqwest::Client,
) -> Result<String, String> {
    match engine {
        SttEngine::Groq => {
            let prompt = match bias {
                Bias::Owner => Some(crate::stt_groq::NEXUS_VOCABULARY),
                Bias::Commands => Some(crate::stt_groq::COMMAND_VOCABULARY),
                Bias::Neutral => None,
            };
            crate::stt_groq::transcribe_with_groq(samples, groq_key, client, prompt).await
        }
        SttEngine::Local => crate::stt::transcribe_local(samples, client).await,
    }
}

/// Synthesize positive-control clips with the bundled offline Piper voice and
/// return them at 16 kHz. These are SYNTHETIC voices: they prove the harness
/// can detect speech end to end, they are NOT accuracy evidence.
pub async fn synth_controls(
    resources: Option<&Path>,
    phrases: &[&str],
) -> Result<Vec<(String, Vec<f32>)>, String> {
    let res: PathBuf = match resources {
        Some(p) => p.to_path_buf(),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources"),
    };
    if std::env::var("PIPER_ESPEAKNG_DATA_DIRECTORY").is_err() {
        std::env::set_var("PIPER_ESPEAKNG_DATA_DIRECTORY", &res);
    }
    let engine = crate::tts_piper::new_engine();
    let mut out = Vec::new();
    for phrase in phrases {
        let (samples, sr) = crate::tts_piper::synthesize(&engine, phrase).await?;
        let wav = WavData { sample_rate: sr, channels: 1, samples };
        out.push((phrase.to_string(), to_mono_16k(&wav)));
    }
    Ok(out)
}

/// Groq per-transcript confidence statistics, summarised over all segments.
/// These are the inputs of `stt_groq::confidence_rejects`; the scorecard sweeps
/// thresholds over them to see whether the gate separates hallucinations from
/// your real commands before it is ever enabled.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SegStats {
    pub n_segments: usize,
    pub min_avg_logprob: f32,
    /// The gate rejects when EVERY segment is below the floor, i.e. when this
    /// (the best segment) is below it — this is the value to sweep.
    pub max_avg_logprob: f32,
    pub max_compression_ratio: f32,
    pub max_no_speech_prob: f32,
    /// What the shipped gate would decide for this transcript.
    pub gate_rejects: bool,
}

pub fn seg_stats(segments: &[crate::stt_groq::GroqSegment]) -> SegStats {
    if segments.is_empty() {
        return SegStats::default();
    }
    SegStats {
        n_segments: segments.len(),
        min_avg_logprob: segments.iter().map(|s| s.avg_logprob).fold(f32::INFINITY, f32::min),
        max_avg_logprob: segments.iter().map(|s| s.avg_logprob).fold(f32::NEG_INFINITY, f32::max),
        max_compression_ratio: segments.iter().map(|s| s.compression_ratio).fold(0.0, f32::max),
        max_no_speech_prob: segments.iter().map(|s| s.no_speech_prob).fold(0.0, f32::max),
        gate_rejects: crate::stt_groq::confidence_rejects(segments),
    }
}

/// Like [`transcribe`] but also returns confidence statistics (Groq only; the
/// local engine reports none, so its stats are all-default / `n_segments == 0`).
pub async fn transcribe_verbose(
    samples: &[i16],
    engine: SttEngine,
    bias: Bias,
    groq_key: &str,
    client: &reqwest::Client,
) -> Result<(String, SegStats), String> {
    match engine {
        SttEngine::Groq => {
            let prompt = match bias {
                Bias::Owner => Some(crate::stt_groq::NEXUS_VOCABULARY),
                Bias::Commands => Some(crate::stt_groq::COMMAND_VOCABULARY),
                Bias::Neutral => None,
            };
            let (text, segs) =
                crate::stt_groq::transcribe_with_groq_verbose(samples, groq_key, client, prompt).await?;
            Ok((text, seg_stats(&segs)))
        }
        SttEngine::Local => Ok((crate::stt::transcribe_local(samples, client).await?, SegStats::default())),
    }
}

// ─── Word error rate ─────────────────────────────────────────────────────

/// Lowercase, strip punctuation, split on whitespace. Apostrophes are removed
/// ("what's" -> "whats") so a typographic difference is not scored as an error.
pub fn normalize_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else if c == '\'' || c == '\u{2019}' { '\0' } else { ' ' })
        .filter(|&c| c != '\0')
        .collect::<String>()
        .split_whitespace()
        .map(|w| w.to_string())
        .collect()
}

/// Word-level Levenshtein distance and the reference length.
pub fn word_edits(reference: &str, hypothesis: &str) -> (usize, usize) {
    let r = normalize_words(reference);
    let h = normalize_words(hypothesis);
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    (prev[h.len()], r.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 16-bit mono PCM WAV encoder for tests.
    fn wav16(samples: &[i16], rate: u32, channels: u16) -> Vec<u8> {
        let data_len = samples.len() * 2;
        let mut w = Vec::new();
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&((36 + data_len) as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&channels.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2 * channels as u32).to_le_bytes());
        w.extend_from_slice(&(2 * channels).to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data_len as u32).to_le_bytes());
        for s in samples {
            w.extend_from_slice(&s.to_le_bytes());
        }
        w
    }

    #[test]
    fn test_parse_wav_pcm16_roundtrip() {
        let w = parse_wav(&wav16(&[0, 16384, -16384, 32767], 16000, 1)).unwrap();
        assert_eq!(w.sample_rate, 16000);
        assert_eq!(w.channels, 1);
        assert_eq!(w.samples.len(), 4);
        assert!((w.samples[1] - 0.5).abs() < 1e-4 && (w.samples[2] + 0.5).abs() < 1e-4);
    }

    #[test]
    fn test_parse_wav_skips_unknown_chunks_and_odd_padding() {
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF\0\0\0\0WAVE");
        b.extend_from_slice(b"LIST");
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + 1 pad byte
        let inner = wav16(&[1000, -1000], 16000, 1);
        b.extend_from_slice(&inner[12..]); // fmt + data chunks
        let w = parse_wav(&b).unwrap();
        assert_eq!(w.samples.len(), 2);
    }

    #[test]
    fn test_parse_wav_tolerates_streamed_data_size() {
        let mut b = wav16(&[100, 200, 300], 16000, 1);
        let at = b.windows(4).position(|w| w == b"data").unwrap() + 4;
        b[at..at + 4].copy_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(parse_wav(&b).unwrap().samples.len(), 3);
    }

    #[test]
    fn test_parse_wav_rejects_garbage() {
        assert!(parse_wav(b"nope").is_err());
        assert!(parse_wav(&[0u8; 64]).is_err());
        let mut no_data = wav16(&[1, 2], 16000, 1);
        no_data.truncate(36);
        assert!(parse_wav(&no_data).is_err());
    }

    #[test]
    fn test_to_mono_16k_quad_uses_first_pair_only() {
        // 4 channels: ch0=0.4 ch1=0.2 ch2=1.0 ch3=1.0 -> (0.4+0.2)/2 = 0.3
        let w = WavData {
            sample_rate: 16000,
            channels: 4,
            samples: vec![0.4, 0.2, 1.0, 1.0, 0.4, 0.2, 1.0, 1.0],
        };
        let m = to_mono_16k(&w);
        assert_eq!(m.len(), 2);
        assert!((m[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_to_mono_16k_stereo_averages_and_resamples_length() {
        let w = WavData { sample_rate: 48000, channels: 2, samples: vec![0.5; 2 * 4800] };
        let m = to_mono_16k(&w);
        // 4800 frames @ 48k = 100 ms -> ~1600 samples @ 16k
        assert!((m.len() as i64 - 1600).abs() <= 2, "{}", m.len());
        assert!(m.iter().all(|&s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn test_word_edits_and_normalisation() {
        assert_eq!(word_edits("open chrome", "open chrome"), (0, 2));
        assert_eq!(word_edits("Open Chrome.", "open chrome"), (0, 2));
        assert_eq!(word_edits("what's up", "whats up"), (0, 2));
        assert_eq!(word_edits("open chrome", "open"), (1, 2));
        assert_eq!(word_edits("open chrome", "close chrome now"), (2, 2));
        assert_eq!(word_edits("", "anything"), (1, 0));
        assert_eq!(word_edits("a b c d", ""), (4, 4));
    }

    #[test]
    fn test_acoustic_gate_mirrors_stt() {
        assert_eq!(acoustic_gate(&vec![5000i16; 1000]), Some("too_short"));
        assert_eq!(acoustic_gate(&vec![10i16; 16000]), Some("low_energy"));
        assert_eq!(acoustic_gate(&vec![3000i16; 16000]), None);
    }

    #[test]
    fn test_text_stage_reports_filter_drops_and_intent() {
        let ok = text_stage("open chrome");
        assert_eq!(ok.intent, "open_app");
        assert!(!ok.dropped_by_filter);
        let silence = text_stage("");
        assert!(!silence.dropped_by_filter, "an empty transcript is not a filter drop");
        let halluc = text_stage("Thank you for watching.");
        assert!(halluc.dropped_by_filter);
        assert_eq!(halluc.intent, "none");
    }

    #[test]
    fn test_speech_onset_finds_first_loud_chunk() {
        let mut a = vec![0.0f32; OWW_CHUNK_SIZE * 3];
        a.extend(vec![0.3f32; OWW_CHUNK_SIZE]);
        assert_eq!(speech_onset_ms(&a, 0.01), Some(240));
        assert_eq!(speech_onset_ms(&vec![0.0; 5000], 0.01), None);
    }

    #[test]
    fn test_engine_silence_and_noise_never_fire_and_streaming_is_audio_clocked() {
        let Ok(mut e) = new_wake_engine(None) else {
            eprintln!("skipping: oww models not available");
            return;
        };
        // 20 s of digital silence: no fire.
        let silence = vec![0.0f32; 16000 * 20];
        let run = run_wake_engine(&mut e, &silence, None);
        assert!(run.fires.is_empty());
        assert_eq!(run.duration_ms, 20_000);

        // Low-level noise: no fire either.
        let noise: Vec<f32> = (0..16000 * 10)
            .map(|i| ((i as f32 * 12.9898).sin() * 43758.545).fract() * 0.01)
            .collect();
        reset_engine_state(&mut e);
        assert!(run_wake_engine(&mut e, &noise, None).fires.is_empty());
    }

    #[test]
    fn test_eval_clip_is_deterministic_and_resets_state() {
        let Ok(mut e) = new_wake_engine(None) else {
            eprintln!("skipping: oww models not available");
            return;
        };
        let clip: Vec<f32> = (0..16000 * 2)
            .map(|i| ((i as f32 * 0.05).sin() * 0.2) * (((i / 3000) % 2) as f32))
            .collect();
        let a = eval_wake_clip(&mut e, &clip, None);
        let b = eval_wake_clip(&mut e, &clip, None);
        assert_eq!(a, b, "a reused engine must give identical results per clip");
        assert!(a.onset_ms.is_some());
    }

    #[test]
    fn test_engine_threshold_override_is_applied() {
        let Ok(mut e) = new_wake_engine(None) else {
            return;
        };
        let _ = run_wake_engine(&mut e, &vec![0.0; 16000], Some(0.9));
        assert!((e.threshold - 0.9).abs() < 1e-6);
    }
    #[test]
    fn test_seg_stats_summarises_segments() {
        use crate::stt_groq::GroqSegment;
        assert_eq!(seg_stats(&[]), SegStats::default());
        let segs = vec![
            GroqSegment { text: "a".into(), avg_logprob: -0.4, no_speech_prob: 0.0, compression_ratio: 1.2 },
            GroqSegment { text: "b".into(), avg_logprob: -1.6, no_speech_prob: 0.1, compression_ratio: 2.9 },
        ];
        let st = seg_stats(&segs);
        assert_eq!(st.n_segments, 2);
        assert!((st.min_avg_logprob + 1.6).abs() < 1e-6);
        assert!((st.max_avg_logprob + 0.4).abs() < 1e-6);
        assert!((st.max_compression_ratio - 2.9).abs() < 1e-6);
        assert!(st.gate_rejects, "ratio 2.9 trips the shipped gate");
    }
}
