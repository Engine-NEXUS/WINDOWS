//! Deepgram Nova-2 cloud STT engine (optional streaming & batch provider).
//!
//! When a Deepgram API key is configured, this provides ultra-low latency
//! (~160ms–190ms) transcription via Deepgram's hosted Nova-2 model with
//! zero local model RAM usage. If no Deepgram key is configured, NEXUS uses
//! the primary Groq LPU pipeline (100% free, Whisper Large v3 Turbo).

use serde::Deserialize;

const DEEPGRAM_LISTEN_URL: &str = "https://api.deepgram.com/v1/listen?model=nova-2&smart_format=true&punctuate=true";

/// Domain terms to bias Nova-2 toward NEXUS vocabulary: proper nouns and command verbs that otherwise collapse into
/// homophones ("ghost mode" -> "post mode"). Sent as `keywords=`: Deepgram documents `keyterm=` for Nova-3 and Flux only
/// and `keywords=` for Nova-2 (2026-11-24 audit). The "+30-40 %" gain once claimed here was never measured. Whether this
/// helps on the owner's voice needs the owner's key and a before/after run.
const DEEPGRAM_KEYTERMS: &[&str] = &[
    "NEXUS",
    "ghost mode",
    "ghost control",
    "servx",
    "zync",
    "whatsapp",
    "architecture mapper",
    "memory audit",
    "pull request",
    "ghostwriter",
    "deepgram",
    "groq",
    "shopkart",
];

#[derive(Debug, Default, Deserialize)]
struct DeepgramAlternative {
    #[serde(default)]
    transcript: String,
    #[allow(dead_code)]
    #[serde(default)]
    confidence: f64,
}

#[derive(Debug, Default, Deserialize)]
struct DeepgramChannel {
    #[serde(default)]
    alternatives: Vec<DeepgramAlternative>,
}

#[derive(Debug, Default, Deserialize)]
struct DeepgramResults {
    #[serde(default)]
    channels: Vec<DeepgramChannel>,
}

#[derive(Debug, Default, Deserialize)]
struct DeepgramResponse {
    #[serde(default)]
    results: DeepgramResults,
}

/// Transcribe PCM16 audio samples using Deepgram Nova-2 REST API.
pub async fn transcribe_with_deepgram(
    samples: &[i16],
    api_key: &str,
    client: &reqwest::Client,
) -> Result<String, String> {
    let wav_bytes = crate::stt::encode_wav_for_upload(samples, 16000)?;

    let url = deepgram_listen_url();

    let resp = client
        .post(url)
        .header("Authorization", format!("Token {api_key}"))
        .header("Content-Type", "audio/wav")
        .body(wav_bytes)
        .send()
        .await
        .map_err(|e| format!("Deepgram request failed: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let err_body = resp.text().await.unwrap_or_default();
        return Err(format!("Deepgram API error {status}: {err_body}"));
    }

    let parsed: DeepgramResponse = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse Deepgram response: {e}"))?;

    let text = parsed
        .results
        .channels
        .first()
        .and_then(|ch| ch.alternatives.first())
        .map(|alt| alt.transcript.trim().to_string())
        .unwrap_or_default();

    Ok(text)
}

/// Build the listen URL with the vocabulary terms appended (as `keywords=`, the Nova-2 parameter).
/// Pure + unit-tested.
pub(crate) fn deepgram_listen_url() -> String {
    let mut url = DEEPGRAM_LISTEN_URL.to_string();
    for term in DEEPGRAM_KEYTERMS {
        let encoded = term.replace(' ', "+");
        url.push_str(&format!("&keywords={encoded}"));
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listen_url_uses_the_nova2_keywords_parameter_not_the_nova3_keyterm_one() {
        let url = deepgram_listen_url();
        assert!(url.starts_with("https://api.deepgram.com/v1/listen?model=nova-2"));
        assert!(url.contains("smart_format=true"));
        for term in ["NEXUS", "ghost+mode", "servx", "zync", "whatsapp", "memory+audit"] {
            assert!(
                url.contains(&format!("keywords={term}")),
                "url missing keywords={term}: {url}"
            );
        }
        assert_eq!(url.matches("keywords=").count(), DEEPGRAM_KEYTERMS.len());
        assert!(!url.contains("keyterm="), "keyterm is a Nova-3 parameter; this URL asks for nova-2");
        assert!(url.contains("model=nova-2"), "if the model is ever changed to nova-3 the parameter must change back to keyterm");
    }

    #[test]
    fn parse_deepgram_response_json() {
        let raw = r#"{
            "metadata": { "transaction_key": "xyz" },
            "results": {
                "channels": [
                    {
                        "alternatives": [
                            {
                                "transcript": "open whatsapp",
                                "confidence": 0.994
                            }
                        ]
                    }
                ]
            }
        }"#;
        let parsed: DeepgramResponse = serde_json::from_str(raw).expect("valid deepgram json");
        let alt = parsed.results.channels.first().unwrap().alternatives.first().unwrap();
        assert_eq!(alt.transcript, "open whatsapp");
        assert!(alt.confidence > 0.9);
    }

    #[test]
    fn parse_deepgram_empty_alternatives() {
        let raw = r#"{ "results": { "channels": [] } }"#;
        let parsed: DeepgramResponse = serde_json::from_str(raw).expect("empty channels json");
        assert!(parsed.results.channels.is_empty());
    }

    /// Task ST: Deepgram upload path refuses empty sample buffers.
    #[test]
    fn test_deepgram_refuses_empty_audio_buffer() {
        let res = crate::stt::encode_wav_for_upload(&[], 16000);
        assert!(res.is_err(), "Empty audio buffer must be refused");
    }
}
