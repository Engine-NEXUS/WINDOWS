//! Groq Cloud STT — Whisper Large v3 Turbo via Groq API.
//!
//! Free tier: 2,000 requests/day, 28,800 audio seconds/day per user.
//! No credit card required. Sign up at console.groq.com.
//!
//! Latency: ~247ms (batch, LPU-accelerated)
//! RAM: 0 MB (cloud, no local model)
//! Cost: $0 on free tier, $0.04/hr on paid tier

use reqwest::Client;

const GROQ_STT_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const GROQ_MODEL: &str = "whisper-large-v3-turbo";
/// Slower, more accurate sibling of the production model. NOT used by the app;
/// it exists so the evaluator can answer "is turbo costing us command accuracy?"
/// (`stt_eval --engine groq-large`) before anyone changes the production default.
pub const GROQ_MODEL_LARGE: &str = "whisper-large-v3";
/// The production model id, exported for the evaluator.
pub const GROQ_MODEL_TURBO: &str = GROQ_MODEL;

/// Domain-vocabulary decoder bias for every NEXUS command transcription.
///
/// Whisper's decoder bets on words it knows: "cervix" (millions of prior
/// sightings) beats "servx" (zero) on identical acoustics. Sending this as
/// the `prompt` field seeds the decoder with our world, flipping those
/// bets — same audio, "servx" wins. A nudge, not a command: clearly-spoken
/// other words still transcribe normally.
///
/// Rules honored: command-styled sentences (the decoder continues prompt
/// style, so prompts read like NEXUS commands), entities + verbs +
/// user phrasing openers, well under Whisper's ~244-token prompt cap.
/// The alias map (`canonical_repo_name`) and heard-text NLU rows stay as
/// the downstream safety net — prompt biasing reduces errors, it never
/// eliminates them.
pub const NEXUS_VOCABULARY: &str = "Ghost mode. Ghost control. Exit ghost mode. \
Open WhatsApp. Open Chrome. Open VS Code. Open Spotify. Open Discord. \
Open Brave. Open YouTube. Open settings. Open architecture mapper. \
Open my chat with Mummy in WhatsApp. Chat with Amma. \
Analyse Servx. Analyse Zync. GitHub pull request. Merge pull request. \
List pull requests. Search for. Play pause. Next track. Previous track. \
Memory audit. Forget everything. Take the mouse. Control my cursor. \
Type this. Press enter. Click the button. \
Ghostwriter. Take a letter. Order food. Send message. \
Deepgram Groq Supabase Eesha Prem Lakshya Congi Shopkart NEXUS \
Shut down my laptop. Restart my laptop. Lock my laptop. \
Practice unattended shutdown. Allow unattended shutdown. Turn off unattended shutdown. \
Hook up Antigravity. Hook up OpenCode. \
open close stop cancel navigate analyse search type click press scroll.";

/// Generic command hints for turns that are NOT owner-verified (speaker
/// verification is off by default, so this is most users' turns). Unlike
/// [`NEXUS_VOCABULARY`] it names no private projects or people — only phrases
/// every NEXUS user can say — so it cannot push a stranger's speech toward the
/// developer's repo names. Opt-in (`sttCommandHints`): a decoder prompt can also
/// make Whisper invent these words on silence, so it ships off until the
/// scorecard's A/B (`stt_eval --bias commands` vs `--bias neutral`) shows the
/// leak rate does not rise. Stays well inside Whisper's 224-token prompt window.
pub const COMMAND_VOCABULARY: &str = "NEXUS. Open Chrome. Open WhatsApp. Open Spotify. Open settings. Close the window. New tab. Search for. Play. Pause. Next song. Stop. Cancel. Ghost mode. Exit ghost mode. List pull requests. Send a WhatsApp message.";

/// Neutral English decoder instruction used when a turn has not been
/// owner-verified. Unlike the owner vocabulary, it does not bias Whisper
/// toward NEXUS commands or known entities.
pub const NEUTRAL_DECODER_PROMPT: &str = "Transcribe this English audio exactly as spoken.";

/// Locked English-only Groq transcription contract. Every STT upload must use
/// these decoder settings; language support is a routing concern, not a
/// per-call option.
pub const STT_LANGUAGE: &str = "en";
pub const STT_TEMPERATURE: &str = "0";
pub const STT_RESPONSE_FORMAT: &str = "json";
pub const STT_VERBOSE_RESPONSE_FORMAT: &str = "verbose_json";

/// Decoder policy is independent of speaker authority. Captured commands use
/// neutral decoding while the legacy vocabulary remains available for comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderBias {
    OwnerCommand,
    /// Generic [`COMMAND_VOCABULARY`] for unenrolled turns, when enabled.
    Commands,
    Neutral,
}

/// Which decoder bias a turn gets. Only a verified owner earns the private
/// vocabulary; an unenrolled user (no voice profile — the default) can opt into
/// the generic command hints; a stranger, an uncertain or a rejected voice never
/// gets any bias.
// Same-clip replay on 2026-10-26: owner vocabulary duplicated a single command
// twice in two runs; neutral decoding did not. Keep this off pending independent
// corpus evidence. This is a decoder policy, never an owner-verification bypass.
const USE_OWNER_COMMAND_VOCABULARY: bool = false;

pub fn decoder_bias_for_turn(
    ownership: crate::voice_profile::TurnOwnership,
    command_hints: bool,
) -> DecoderBias {
    use crate::voice_profile::TurnOwnership as T;
    match ownership {
        T::Verified if USE_OWNER_COMMAND_VOCABULARY => DecoderBias::OwnerCommand,
        T::Unenrolled if command_hints => DecoderBias::Commands,
        _ => DecoderBias::Neutral,
    }
}

/// Label recorded in `stt:turn_stats.decoder_bias`.
pub fn decoder_bias_label(bias: DecoderBias) -> &'static str {
    match bias {
        DecoderBias::OwnerCommand => "owner",
        DecoderBias::Commands => "commands",
        DecoderBias::Neutral => "neutral",
    }
}

/// Pure Groq multipart fields. Centralizing these values prevents one STT path
/// from accidentally permitting another language, temperature, or format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GroqTranscriptionFields {
    pub model: &'static str,
    pub language: &'static str,
    pub temperature: &'static str,
    pub response_format: &'static str,
    pub prompt: &'static str,
    pub file_name: String,
    pub mime: String,
}

pub(crate) fn decoder_prompt(prompt: Option<&'static str>) -> &'static str {
    prompt.unwrap_or(NEUTRAL_DECODER_PROMPT)
}

pub(crate) fn decoder_bias_prompt(bias: DecoderBias) -> &'static str {
    match bias {
        DecoderBias::OwnerCommand => NEXUS_VOCABULARY,
        DecoderBias::Commands => COMMAND_VOCABULARY,
        DecoderBias::Neutral => NEUTRAL_DECODER_PROMPT,
    }
}

pub(crate) fn transcription_fields(
    file_name: &str,
    mime: &str,
    prompt: Option<&'static str>,
    verbose: bool,
) -> GroqTranscriptionFields {
    GroqTranscriptionFields {
        model: GROQ_MODEL,
        language: STT_LANGUAGE,
        temperature: STT_TEMPERATURE,
        response_format: if verbose {
            STT_VERBOSE_RESPONSE_FORMAT
        } else {
            STT_RESPONSE_FORMAT
        },
        prompt: decoder_prompt(prompt),
        file_name: file_name.to_string(),
        mime: mime.to_string(),
    }
}

fn groq_transcription_form(
    file_bytes: Vec<u8>,
    fields: &GroqTranscriptionFields,
) -> Result<reqwest::multipart::Form, String> {
    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(fields.file_name.clone())
        .mime_str(&fields.mime)
        .map_err(|e| format!("MIME error: {}", e))?;

    Ok(reqwest::multipart::Form::new()
        .text("model", fields.model.to_string())
        .text("language", fields.language.to_string())
        .text("temperature", fields.temperature.to_string())
        .text("response_format", fields.response_format.to_string())
        .text("prompt", fields.prompt.to_string())
        .part("file", part))
}

/// Transcribe audio using Groq's Whisper Large v3 Turbo model.
///
/// Sends WAV audio (16kHz, mono, 16-bit) to Groq's OpenAI-compatible API.
/// Returns the transcript text on success, or an error string on failure.
///
/// # Arguments
/// * `samples` - Raw i16 PCM samples at 16kHz mono
/// * `api_key` - User's Groq API key (starts with "gsk_")
/// * `client` - Reused reqwest client (avoids per-call Client::build)
/// * `prompt` - Optional decoder-bias hint (Whisper `prompt` field). Used by
///   the wake-word verifier to prefer wake-word readings of ambiguous audio.
pub async fn transcribe_with_groq(
    samples: &[i16],
    api_key: &str,
    client: &Client,
    prompt: Option<&'static str>,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("No Groq API key provided".to_string());
    }

    let wav_bytes = crate::stt::encode_wav_for_upload(samples, 16000)?;
    tracing::info!(
        "stt-groq: sending {} bytes ({}ms audio) to Groq",
        wav_bytes.len(),
        samples.len() / 16
    );

    let fields = transcription_fields("audio.wav", "audio/wav", prompt, false);
    let form = groq_transcription_form(wav_bytes, &fields)?;

    let start = std::time::Instant::now();

    crate::usage_counter::bump("stt");
    let resp = client
        .post(GROQ_STT_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Groq STT request failed: {}", e))?;

    let elapsed = start.elapsed();

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Check for rate limit (429) or auth error (401) for better error messages
        if status.as_u16() == 429 {
            return Err("Groq rate limit hit (2,000 req/day free tier)".to_string());
        }
        if status.as_u16() == 401 {
            return Err("Groq API key invalid — check Settings".to_string());
        }
        return Err(format!("Groq STT error {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Groq response parse error: {}", e))?;

    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    tracing::info!("stt-groq: transcript '{}' in {}ms", text, elapsed.as_millis());

    Ok(text)
}

/// Transcribe pre-encoded audio bytes (e.g. a Telegram voice note in Opus/OGG)
/// via Groq's OpenAI-compatible API. Same model/parse contract as
/// [`transcribe_with_groq`]; the caller supplies filename + MIME.
pub async fn transcribe_bytes_with_groq(
    audio: &[u8],
    filename: &str,
    mime: &str,
    api_key: &str,
    client: &Client,
    prompt: Option<&'static str>,
) -> Result<String, String> {
    if api_key.is_empty() {
        return Err("No Groq API key provided".to_string());
    }
    if audio.is_empty() {
        return Err("Empty audio".to_string());
    }

    let fields = transcription_fields(filename, mime, prompt, false);
    let form = groq_transcription_form(audio.to_vec(), &fields)?;

    let start = std::time::Instant::now();

    crate::usage_counter::bump("stt");
    let resp = client
        .post(GROQ_STT_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Groq STT request failed: {}", e))?;

    let elapsed = start.elapsed();

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.as_u16() == 429 {
            return Err("Groq rate limit hit (2,000 req/day free tier)".to_string());
        }
        if status.as_u16() == 401 {
            return Err("Groq API key invalid — check Settings".to_string());
        }
        return Err(format!("Groq STT error {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Groq response parse error: {}", e))?;

    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    tracing::info!("stt-groq: transcript '{}' in {}ms", text, elapsed.as_millis());

    Ok(text)
}

/// A single transcription segment from `verbose_json` responses.
/// Carries the model-native confidence signals used by the wake-word
/// verifier's confidence gate (v3): `no_speech_prob` flags phantom
/// readings on mangled/dropout-torn audio; `avg_logprob` flags garbage.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct GroqSegment {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub avg_logprob: f32,
    #[serde(default)]
    pub no_speech_prob: f32,
    /// Whisper's own repetition signal (gzip ratio of the segment text);
    /// > ~2.4 means a looping/degenerate decode.
    #[serde(default)]
    pub compression_ratio: f32,
}

/// Thresholds for [`confidence_rejects`]. Whisper's reference decoder falls
/// back/gives up at avg_logprob < -1.0 and compression_ratio > 2.4; we are
/// stricter about *acting* than the decoder is about retrying, so the shipped
/// values leave a margin. They are a starting point: calibrate them from the
/// held-out set (`voice_scorecard.py` prints the sweep) before enabling the gate.
pub const CONFIDENCE_MIN_AVG_LOGPROB: f32 = -1.2;
pub const CONFIDENCE_MAX_COMPRESSION_RATIO: f32 = 2.6;

/// True when Groq's own segment statistics say the transcript is not trustworthy:
/// EVERY segment is below the log-probability floor, or ANY segment is a
/// degenerate repetition. Empty segment list (no information) never rejects.
/// `no_speech_prob` is deliberately not used: it reads 0.000 on every segment of
/// `whisper-large-v3-turbo` (see `verify_confidence`).
pub fn confidence_rejects(segments: &[GroqSegment]) -> bool {
    confidence_rejects_with(segments, CONFIDENCE_MIN_AVG_LOGPROB, CONFIDENCE_MAX_COMPRESSION_RATIO)
}

pub fn confidence_rejects_with(segments: &[GroqSegment], min_logprob: f32, max_ratio: f32) -> bool {
    if segments.is_empty() {
        return false;
    }
    segments.iter().all(|s| s.avg_logprob < min_logprob)
        || segments.iter().any(|s| s.compression_ratio > max_ratio)
}

/// Transcribe with structured segment metadata (`verbose_json`).
/// Used ONLY by the wake-word verifier (v3 confidence gate). The normal
/// transcription path keeps plain `json` (smaller, faster to parse).
///
/// Requests pinned `temperature=0` (deterministic greedy; the documented
/// recommendation for STT accuracy) and English language.
pub async fn transcribe_with_groq_verbose(
    samples: &[i16],
    api_key: &str,
    client: &Client,
    prompt: Option<&'static str>,
) -> Result<(String, Vec<GroqSegment>), String> {
    transcribe_with_groq_verbose_model(samples, api_key, client, prompt, GROQ_MODEL).await
}

/// [`transcribe_with_groq_verbose`] with an explicit model id (evaluator A/B).
pub async fn transcribe_with_groq_verbose_model(
    samples: &[i16],
    api_key: &str,
    client: &Client,
    prompt: Option<&'static str>,
    model: &'static str,
) -> Result<(String, Vec<GroqSegment>), String> {
    if api_key.is_empty() {
        return Err("No Groq API key provided".to_string());
    }

    let wav_bytes = crate::stt::encode_wav_for_upload(samples, 16000)?;

    let mut fields = transcription_fields("audio.wav", "audio/wav", prompt, true);
    fields.model = model;
    let form = groq_transcription_form(wav_bytes, &fields)?;

    crate::usage_counter::bump("stt");
    let resp = client
        .post(GROQ_STT_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Groq STT request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.as_u16() == 429 {
            return Err("Groq rate limit hit (2,000 req/day free tier)".to_string());
        }
        if status.as_u16() == 401 {
            return Err("Groq API key invalid — check Settings".to_string());
        }
        return Err(format!("Groq STT error {}: {}", status, body));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Groq response parse error: {}", e))?;

    let text = json
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .trim()
        .to_string();

    let mut segments = Vec::new();
    if let Some(arr) = json.get("segments").and_then(|s| s.as_array()) {
        for seg in arr {
            segments.push(GroqSegment {
                text: seg
                    .get("text")
                    .and_then(|t| t.as_str())
                    .unwrap_or("")
                    .to_string(),
                avg_logprob: seg
                    .get("avg_logprob")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                no_speech_prob: seg
                    .get("no_speech_prob")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
                compression_ratio: seg
                    .get("compression_ratio")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32,
            });
        }
    }

    Ok((text, segments))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_groq_url_constant() {
        assert!(GROQ_STT_URL.starts_with("https://api.groq.com"));
        assert!(GROQ_STT_URL.contains("/audio/transcriptions"));
    }

    #[test]
    fn test_groq_model_constant() {
        assert_eq!(GROQ_MODEL, "whisper-large-v3-turbo");
    }

    #[test]
    fn test_nexus_vocabulary_budget_and_coverage() {
        // Whisper prompt cap is ~244 tokens; English averages ~1.3
        // tokens/word, so cap words at 140 for a safe margin. If this
        // fails, trim terms — never raise the cap.
        let words = NEXUS_VOCABULARY.split_whitespace().count();
        assert!(words <= 140, "vocabulary too long: {words} words");
        // Every entity the alias map rescues must be seeded upstream.
        for term in [
            "servx", "zync", "eesha", "nexus", "github", "whatsapp",
            "prem", "lakshya", "congi", "shopkart",
        ] {
            assert!(
                NEXUS_VOCABULARY.to_lowercase().contains(term),
                "vocabulary missing: {term}"
            );
        }
    }

    #[test]
    fn captured_commands_use_neutral_decoder_even_after_owner_verification() {
        use crate::voice_profile::TurnOwnership::*;
        for ownership in [Verified, Unenrolled, Uncertain, Rejected] {
            assert_eq!(decoder_bias_prompt(decoder_bias_for_turn(ownership, false)), NEUTRAL_DECODER_PROMPT,
                "speaker identity must not seed repeated executable command phrases");
        }
    }

    #[test]
    fn decoder_bias_follows_ownership() {
        use crate::voice_profile::TurnOwnership::*;
        // Speaker authority and decoder bias are separate; all live captures are neutral.
        assert_eq!(decoder_bias_for_turn(Unenrolled, false), DecoderBias::Neutral);
        assert_eq!(decoder_bias_for_turn(Verified, false), DecoderBias::Neutral);
        // A voice that is (or may be) someone else never gets command biasing.
        assert_eq!(decoder_bias_for_turn(Uncertain, false), DecoderBias::Neutral);
        assert_eq!(decoder_bias_for_turn(Rejected, false), DecoderBias::Neutral);
    }

    /// Live miss 2026-10-19 18:21: Groq answered 400 "prompt length must be 896 characters or fewer, but provided
    /// prompt contains 928 characters" for EVERY turn, so all speech silently fell back to the slow local model.
    /// The word-count cap above is not the real limit; this is. Keep a margin for the next phrase someone adds.
    #[test]
    fn vocabulary_fits_groqs_prompt_character_limit() {
        let chars = NEXUS_VOCABULARY.chars().count();
        assert!(chars <= 860, "NEXUS_VOCABULARY is {chars} characters; Groq rejects the whole request above 896");
        assert!(NEUTRAL_DECODER_PROMPT.chars().count() <= 860);
    }

    /// Live miss 2026-10-19: Whisper heard "practice unattended shutdown" as "practice unobtended
    /// shutdown" and "Raktis unattended shutdown" with no hint in the prompt.
    #[test]
    fn vocabulary_teaches_the_unattended_shutdown_phrasing() {
        let v = NEXUS_VOCABULARY.to_lowercase();
        for phrase in ["practice unattended shutdown", "allow unattended shutdown", "turn off unattended shutdown", "hook up antigravity", "hook up opencode"] {
            assert!(v.contains(phrase), "vocabulary missing: {phrase}");
        }
    }

    #[test]
    fn vocabulary_teaches_the_chat_command_phrasing() {
        let v = NEXUS_VOCABULARY.to_lowercase();
        assert!(v.contains("chat with"), "STT heard \"chat with\" as \"chart with\"/\"start with\"");
        assert!(v.contains("in whatsapp"), "STT heard \"in WhatsApp\" as \"and what so\"");
    }

    #[test]
    fn test_english_transcription_contract_is_locked() {
        for verbose in [false, true] {
            let fields = transcription_fields("audio.wav", "audio/wav", None, verbose);
            assert_eq!(fields.model, GROQ_MODEL);
            assert_eq!(fields.language, "en");
            assert_eq!(fields.temperature, "0");
            assert_eq!(fields.prompt, NEUTRAL_DECODER_PROMPT);
            assert_eq!(
                fields.response_format,
                if verbose {
                    STT_VERBOSE_RESPONSE_FORMAT
                } else {
                    STT_RESPONSE_FORMAT
                }
            );
        }

        let owner = transcription_fields("audio.wav", "audio/wav", Some(NEXUS_VOCABULARY), false);
        assert_eq!(owner.prompt, NEXUS_VOCABULARY);
        assert_eq!(decoder_bias_prompt(DecoderBias::OwnerCommand), NEXUS_VOCABULARY);
        assert_eq!(decoder_bias_prompt(DecoderBias::Neutral), NEUTRAL_DECODER_PROMPT);
    }

    #[test]
    fn test_groq_segment_defaults_on_missing_fields() {
        // Groq may add/drop segment fields over time — parsing must never fail.
        let v: serde_json::Value = serde_json::json!({ "text": "Nexus." });
        let seg: GroqSegment = serde_json::from_value(v).unwrap();
        assert_eq!(seg.text, "Nexus.");
        assert_eq!(seg.avg_logprob, 0.0);
        assert_eq!(seg.no_speech_prob, 0.0);
    }

    #[test]
    fn test_groq_segment_full_parse() {
        let v: serde_json::Value = serde_json::json!({
            "text": " Lots of children.",
            "avg_logprob": -0.85,
            "no_speech_prob": 0.91
        });
        let seg: GroqSegment = serde_json::from_value(v).unwrap();
        assert_eq!(seg.text, " Lots of children.");
        assert!((seg.no_speech_prob - 0.91).abs() < 1e-6);
    }

    /// Task ST: Vocabulary biasing contract for NEXUS_VOCABULARY:
    /// - Under provider character limit (<= 860 < 896 cap)
    /// - Contains the wake word ("NEXUS" / "nexus")
    /// - Contains the ghost-mode phrases ("Ghost mode", "Ghost control", "Exit ghost mode")
    /// - Contains no duplicate phrase entries
    #[test]
    fn test_nexus_vocabulary_prompt_contract() {
        let chars = NEXUS_VOCABULARY.chars().count();
        assert!(chars <= 860, "NEXUS_VOCABULARY exceeds character ceiling: {chars}");

        let lower = NEXUS_VOCABULARY.to_lowercase();
        // Wake word
        assert!(lower.contains("nexus"), "NEXUS_VOCABULARY must contain the wake word");

        // Ghost mode phrases
        assert!(lower.contains("ghost mode"), "missing 'ghost mode'");
        assert!(lower.contains("ghost control"), "missing 'ghost control'");
        assert!(lower.contains("exit ghost mode"), "missing 'exit ghost mode'");

        // No duplicate entries (sentence-level command phrases)
        let phrases: Vec<&str> = NEXUS_VOCABULARY
            .split('.')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut seen = std::collections::HashSet::new();
        for phrase in phrases {
            let lower_phrase = phrase.to_lowercase();
            assert!(
                seen.insert(lower_phrase.clone()),
                "NEXUS_VOCABULARY contains duplicate entry: '{phrase}'"
            );
        }
    }

    /// Task ST: Upload encoding refuses an empty audio buffer (0 samples) to prevent wasted API calls.
    #[test]
    fn test_groq_refuses_empty_samples_buffer() {
        let res = crate::stt::encode_wav_for_upload(&[], 16000);
        assert!(res.is_err(), "Empty audio buffer must be refused before upload");
    }

    fn seg(logprob: f32, ratio: f32) -> GroqSegment {
        GroqSegment { text: "x".into(), avg_logprob: logprob, no_speech_prob: 0.0, compression_ratio: ratio }
    }

    #[test]
    fn test_confidence_rejects() {
        // Confident, normal speech passes.
        assert!(!confidence_rejects(&[seg(-0.3, 1.1)]));
        assert!(!confidence_rejects(&[seg(-0.9, 1.6), seg(-1.1, 1.4)]));
        // No metadata → no information → never reject (local fallback, parse gaps).
        assert!(!confidence_rejects(&[]));
        // Every segment below the floor → reject.
        assert!(confidence_rejects(&[seg(-1.5, 1.2)]));
        assert!(confidence_rejects(&[seg(-1.3, 1.0), seg(-1.9, 1.0)]));
        // One confident segment saves a mixed transcript.
        assert!(!confidence_rejects(&[seg(-1.5, 1.0), seg(-0.4, 1.0)]));
        // A single looping segment rejects regardless of logprob.
        assert!(confidence_rejects(&[seg(-0.2, 3.1)]));
        // Missing compression_ratio parses as 0 (never trips).
        let v: GroqSegment = serde_json::from_value(serde_json::json!({"text":"hi","avg_logprob":-0.5})).unwrap();
        assert_eq!(v.compression_ratio, 0.0);
    }
    #[test]
    fn test_decoder_bias_for_turn() {
        use crate::voice_profile::TurnOwnership as T;
        // Default (hints off) = exactly the previous behaviour.
        assert_eq!(decoder_bias_for_turn(T::Verified, false), DecoderBias::Neutral);
        assert_eq!(decoder_bias_for_turn(T::Unenrolled, false), DecoderBias::Neutral);
        // Hints on: only unenrolled turns change; the owner keeps the owner prompt.
        assert_eq!(decoder_bias_for_turn(T::Unenrolled, true), DecoderBias::Commands);
        assert_eq!(decoder_bias_for_turn(T::Verified, true), DecoderBias::Neutral);
        // Someone else's voice never receives a bias, setting or not.
        for t in [T::Uncertain, T::Rejected] {
            assert_eq!(decoder_bias_for_turn(t, true), DecoderBias::Neutral);
            assert_eq!(decoder_bias_for_turn(t, false), DecoderBias::Neutral);
        }
        assert_eq!(decoder_bias_label(DecoderBias::Commands), "commands");
        assert_eq!(decoder_bias_prompt(DecoderBias::Commands), COMMAND_VOCABULARY);
    }

    #[test]
    fn test_command_vocabulary_is_generic_and_small() {
        // No private names from the owner prompt.
        for private in ["Servx", "Zync", "Eesha", "Prem", "Lakshya", "Congi", "Shopkart", "Supabase"] {
            assert!(!COMMAND_VOCABULARY.contains(private), "{private}");
        }
        // Whisper's prompt window is 224 tokens; stay far below (≈ 1.5 tokens/word).
        assert!(COMMAND_VOCABULARY.split_whitespace().count() * 2 < 224);
        // ~1.5 tokens/word (the comment above); NEXUS_VOCABULARY's own budget (<= 140 words,
        // <= 860 chars) is pinned by test_nexus_vocabulary_budget_and_coverage.
        assert!(NEXUS_VOCABULARY.split_whitespace().count() * 3 / 2 < 224);
        assert!(COMMAND_VOCABULARY.contains("NEXUS"));
    }
}
