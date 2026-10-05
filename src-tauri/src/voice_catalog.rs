//! Feature 83 — Top-10 iconic voice catalog (single source of truth).
//!
//! Each persona pairs a cloud Edge-TTS neural voice (instant switch,
//! 0 MB RAM) with a local Piper VITS twin (offline fallback, single
//! disk slot). The hub grid, the background swap worker, and the TTS
//! dispatcher all read THIS table — never a duplicated list.
//!
//! Source: docs/features/83-top-10-iconic-voices-and-single-slot-
//! dynamic-offline-swapping.md §2 + docs/research/tts/01-cloud-primary-
//! single-slot-offline-voice-swapping-architecture-2026-10-01.md §4.

use serde::{Deserialize, Serialize};

/// One iconic voice persona: cloud neural ID + local Piper twin +
/// signature preview phrase. `sha256` is the expected checksum of the
/// local `.onnx` twin, populated from the voice CDN manifest at download
/// time; `None` means "verify against the manifest checksum instead of
/// a baked-in value" (no invented hashes — verification is enforced
/// whenever a checksum is available).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoicePersona {
    /// Stable key (e.g. "jarvis") — stored in settings + memory.
    pub key: &'static str,
    /// Display name (e.g. "JARVIS").
    pub name: &'static str,
    /// One-line tone summary for the card.
    pub persona: &'static str,
    /// Region · gender tag (e.g. "UK · Male").
    pub accent_tag: &'static str,
    /// Avatar badge for the card (e.g. "🤖").
    pub avatar: &'static str,
    /// Cloud Edge-TTS voice id (e.g. "en-GB-RyanNeural").
    pub cloud_id: &'static str,
    /// Local Piper model stem (e.g. "en_GB-alan-medium").
    pub local_model: &'static str,
    /// 2-second signature preview line (non-destructive demo).
    pub preview_phrase: &'static str,
    /// Expected SHA-256 of the local twin, when known.
    pub sha256: Option<&'static str>,
}

/// The 10-voice lineup, in hub display order. Append-only: never reorder
/// (hub grid, manifests, and memory profiles key off position stability —
/// use `key` for lookups, never the index).
pub const VOICE_CATALOG: &[VoicePersona] = &[
    VoicePersona {
        key: "jarvis",
        name: "JARVIS",
        persona: "Calm, intelligent, British baritone with witty refinement",
        accent_tag: "UK · Male",
        avatar: "🤖",
        cloud_id: "en-GB-RyanNeural",
        local_model: "en_GB-alan-medium",
        preview_phrase: "At your service, sir. All systems operational.",
        sha256: None,
    },
    VoicePersona {
        key: "friday",
        name: "FRIDAY",
        persona: "Sharp tactical companion, fast and precise",
        accent_tag: "Ireland · Female",
        avatar: "🛡️",
        cloud_id: "en-IE-EmilyNeural",
        local_model: "en_GB-southern_english_female-low",
        preview_phrase: "Boss, tactical links and neural feeds are live.",
        sha256: None,
    },
    VoicePersona {
        key: "nexus",
        name: "NEXUS",
        persona: "Expressive flagship assistant, warm and clear",
        accent_tag: "US · Female",
        avatar: "🌟",
        cloud_id: "en-US-AvaNeural",
        local_model: "en_US-amy-medium",
        preview_phrase: "Hello, I'm NEXUS. What are we building today?",
        sha256: None,
    },
    VoicePersona {
        key: "siri",
        name: "SIRI",
        persona: "Modern, crisp assistant delivery",
        accent_tag: "US · Female",
        avatar: "📱",
        cloud_id: "en-US-JennyNeural",
        local_model: "en_US-lessac-medium",
        preview_phrase: "Here is what I found for you.",
        sha256: None,
    },
    VoicePersona {
        key: "alexa",
        name: "ALEXA",
        persona: "Confident smart-home lead, steady and direct",
        accent_tag: "US · Female",
        avatar: "🔵",
        cloud_id: "en-US-AriaNeural",
        local_model: "en_US-kristin-medium",
        preview_phrase: "Ready. Standing by for your instructions.",
        sha256: None,
    },
    VoicePersona {
        key: "google",
        name: "GOOGLE",
        persona: "Analytical tech specialist, measured and exact",
        accent_tag: "US · Male",
        avatar: "🎙️",
        cloud_id: "en-US-BrianNeural",
        local_model: "en_US-ryan-medium",
        preview_phrase: "Good day. Let me know what you need analyzed.",
        sha256: None,
    },
    VoicePersona {
        key: "cortana",
        name: "CORTANA",
        persona: "Heroic sci-fi companion, loyal and bold",
        accent_tag: "US · Female",
        avatar: "💠",
        cloud_id: "en-US-MichelleNeural",
        local_model: "en_US-libritts-high",
        preview_phrase: "Chief, telemetry is locked. I'm with you.",
        sha256: None,
    },
    VoicePersona {
        key: "samantha",
        name: "SAMANTHA",
        persona: "Warm, intimate conversationalist",
        accent_tag: "US · Female",
        avatar: "💫",
        cloud_id: "en-US-SaraNeural",
        local_model: "en_US-hfc_female-medium",
        preview_phrase: "I'm here. It's really good to hear your voice.",
        sha256: None,
    },
    VoicePersona {
        key: "alfred",
        name: "ALFRED",
        persona: "Master butler, formal and reassuring",
        accent_tag: "UK · Male",
        avatar: "🎩",
        cloud_id: "en-GB-OliverNeural",
        local_model: "en_GB-northern_english_male-medium",
        preview_phrase: "Very good, sir. I have prepared your workspace.",
        sha256: None,
    },
    VoicePersona {
        key: "offline_safe",
        name: "EMERGENCY",
        persona: "Offline-safe fallback, always available",
        accent_tag: "US · Female",
        avatar: "🆘",
        cloud_id: "en-US-AvaNeural",
        local_model: "en_US-amy-medium",
        preview_phrase: "Local speech synthesizer operational.",
        sha256: None,
    },
];

/// Default persona key (ships as the active voice).
pub const DEFAULT_VOICE_KEY: &str = "nexus";

/// Managed single-slot directory: %APPDATA%/com.nexus.assistant/voices/.
/// Holds AT MOST one model pair (active_offline.onnx + .json) plus the
/// manifest — the disk invariant from the Feature 83 spec.
pub fn managed_voices_dir() -> Option<std::path::PathBuf> {
    dirs_next::data_dir().map(|d| d.join("com.nexus.assistant").join("voices"))
}

/// The single-slot model file pair (both must exist to count).
pub fn managed_model_paths() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let dir = managed_voices_dir()?;
    let onnx = dir.join("active_offline.onnx");
    let json = dir.join("active_offline.onnx.json");
    if onnx.is_file() && json.is_file() {
        Some((onnx, json))
    } else {
        None
    }
}

/// Slot manifest shape: {voiceKey, modelName, sha256, version}.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoiceManifest {
    #[serde(default)]
    pub voice_key: String,
    #[serde(default)]
    pub model_name: String,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub version: u32,
}

/// Read the slot manifest from an explicit dir (pure core for tests).
pub fn read_manifest_at(dir: &std::path::Path) -> Option<VoiceManifest> {
    let content = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write the slot manifest to an explicit dir. Pure core for tests.
pub fn write_manifest_at(dir: &std::path::Path, m: &VoiceManifest) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let json = serde_json::to_string_pretty(m)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(dir.join("manifest.json"), json)
}

/// Read the slot manifest. None = empty slot (nothing downloaded yet).
pub fn read_manifest() -> Option<VoiceManifest> {
    read_manifest_at(&managed_voices_dir()?)
}

/// True when the managed slot holds THIS stem (manifest agrees AND both
/// files exist). Pure over injected paths (unit-testable).
pub fn twin_ready_for_paths(
    stem: &str,
    manifest: Option<&VoiceManifest>,
    files_present: bool,
) -> bool {
    manifest.map(|m| m.model_name == stem).unwrap_or(false) && files_present
}

/// True when the managed slot holds THIS stem (reads the live slot).
pub fn twin_ready_for(stem: &str) -> bool {
    twin_ready_for_paths(stem, read_manifest().as_ref(), managed_model_paths().is_some())
}

/// Sync state of a persona's offline twin: ready now, or cloud-only
/// (download queued — the P3 worker fills it in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceSyncState {
    Ready,
    CloudOnly,
}

/// Sync state for a persona key (unknown key → CloudOnly, never an
/// error — the hub still shows the card, just without offline).
pub fn sync_state_for(key: &str) -> VoiceSyncState {
    match find_by_key(key) {
        Some(p) if twin_ready_for(p.local_model) => VoiceSyncState::Ready,
        _ => VoiceSyncState::CloudOnly,
    }
}

/// Look up a persona by key. Pure.
pub fn find_by_key(key: &str) -> Option<&'static VoicePersona> {
    VOICE_CATALOG.iter().find(|p| p.key == key)
}

/// Look up a persona by cloud Edge-TTS id. First match wins — twins can
/// share an id (e.g. nexus + offline_safe both use AvaNeural). Pure.
pub fn find_by_cloud_id(cloud_id: &str) -> Option<&'static VoicePersona> {
    VOICE_CATALOG.iter().find(|p| p.cloud_id == cloud_id)
}

/// Look up a persona by local Piper model stem. First match wins. Pure.
pub fn find_by_local_model(local_model: &str) -> Option<&'static VoicePersona> {
    VOICE_CATALOG
        .iter()
        .find(|p| p.local_model == local_model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_catalog_has_ten_personas() {
        assert_eq!(VOICE_CATALOG.len(), 10);
    }

    #[test]
    fn test_catalog_keys_unique_and_nonempty() {
        let mut seen = HashSet::new();
        for p in VOICE_CATALOG {
            assert!(!p.key.is_empty(), "empty key");
            assert!(!p.name.is_empty(), "empty name for {}", p.key);
            assert!(seen.insert(p.key), "duplicate key {}", p.key);
        }
    }

    #[test]
    fn test_catalog_cloud_ids_wellformed() {
        for p in VOICE_CATALOG {
            assert!(
                p.cloud_id.ends_with("Neural") && p.cloud_id.contains('-'),
                "malformed cloud id {} for {}",
                p.cloud_id,
                p.key
            );
        }
    }

    #[test]
    fn test_catalog_local_twins_present() {
        for p in VOICE_CATALOG {
            assert!(!p.local_model.is_empty(), "missing twin for {}", p.key);
        }
    }

    #[test]
    fn test_catalog_preview_phrases_present() {
        for p in VOICE_CATALOG {
            assert!(!p.preview_phrase.is_empty(), "missing preview for {}", p.key);
        }
    }

    #[test]
    fn test_catalog_lookup_roundtrip() {
        for p in VOICE_CATALOG {
            assert_eq!(find_by_key(p.key).map(|q| q.key), Some(p.key));
        }
        // Shared twins resolve first-match-wins (nexus precedes offline_safe).
        assert_eq!(
            find_by_cloud_id("en-US-AvaNeural").map(|q| q.key),
            Some("nexus")
        );
        assert_eq!(
            find_by_local_model("en_US-amy-medium").map(|q| q.key),
            Some("nexus")
        );
        // Unique ids resolve to their owner.
        assert_eq!(
            find_by_cloud_id("en-GB-RyanNeural").map(|q| q.key),
            Some("jarvis")
        );
        assert_eq!(
            find_by_local_model("en_GB-alan-medium").map(|q| q.key),
            Some("jarvis")
        );
        assert!(find_by_key("nope").is_none());
        assert!(find_by_cloud_id("xx-YY-NopeNeural").is_none());
        assert!(find_by_local_model("xx_nope-medium").is_none());
    }

    #[test]
    fn test_default_voice_exists() {
        assert!(find_by_key(DEFAULT_VOICE_KEY).is_some());
    }

    #[test]
    fn test_offline_safe_twin_is_bundled_amy() {
        // The emergency persona must point at the bundled fallback model.
        let p = find_by_key("offline_safe").unwrap();
        assert_eq!(p.local_model, "en_US-amy-medium");
    }

    #[test]
    fn test_managed_dir_points_at_voices_slot() {
        let dir = managed_voices_dir().unwrap();
        assert!(dir.ends_with("voices"));
    }

    #[test]
    fn test_twin_ready_for_paths_matrix() {
        let m = VoiceManifest {
            voice_key: "jarvis".into(),
            model_name: "en_GB-alan-medium".into(),
            sha256: String::new(),
            version: 1,
        };
        // Match + files → ready.
        assert!(twin_ready_for_paths("en_GB-alan-medium", Some(&m), true));
        // Wrong stem → not ready (a different twin occupies the slot).
        assert!(!twin_ready_for_paths("en_US-amy-medium", Some(&m), true));
        // Match but files missing (half-download) → not ready.
        assert!(!twin_ready_for_paths("en_GB-alan-medium", Some(&m), false));
        // No manifest (empty slot) → not ready.
        assert!(!twin_ready_for_paths("en_GB-alan-medium", None, false));
    }

    #[test]
    fn test_manifest_roundtrip() {
        let m = VoiceManifest {
            voice_key: "siri".into(),
            model_name: "en_US-lessac-medium".into(),
            sha256: "abc123".into(),
            version: 2,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: VoiceManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn test_manifest_file_roundtrip_in_temp_dir() {
        let dir = std::env::temp_dir().join(format!(
            "nexus_voice_manifest_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let m = VoiceManifest {
            voice_key: "jarvis".into(),
            model_name: "en_GB-alan-medium".into(),
            sha256: "deadbeef".into(),
            version: 1,
        };
        write_manifest_at(&dir, &m).unwrap();
        assert_eq!(read_manifest_at(&dir), Some(m));
        assert!(dir.join("manifest.json").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_manifest_missing_is_none() {
        let dir = std::env::temp_dir().join(format!(
            "nexus_voice_manifest_missing_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(read_manifest_at(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_corrected_twins_match_repo_layout() {
        // Verified against rhasspy/piper-voices VOICES.md (2026-10-02):
        // southern_english_female exists ONLY in low; libritts ONLY in
        // high; northern voice is northern_english_male.
        assert_eq!(
            find_by_key("friday").map(|p| p.local_model),
            Some("en_GB-southern_english_female-low")
        );
        assert_eq!(
            find_by_key("cortana").map(|p| p.local_model),
            Some("en_US-libritts-high")
        );
        assert_eq!(
            find_by_key("alfred").map(|p| p.local_model),
            Some("en_GB-northern_english_male-medium")
        );
    }

    #[test]
    fn test_sync_state_unknown_key_is_cloud_only() {
        assert_eq!(sync_state_for("nope"), VoiceSyncState::CloudOnly);
    }
}
