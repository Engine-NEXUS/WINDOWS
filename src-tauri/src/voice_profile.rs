// ─── Voice Profile: Speaker Verification ───────────────────────────
//
// Phase D: Owner-only wake word activation.
//
// Uses the existing openWakeWord embedding_model.onnx to extract speaker
// embeddings from detected audio. Compares to enrolled profile using
// cosine similarity. If similarity > threshold, the wake is accepted.
// If similarity < threshold, the wake is rejected silently.
//
// This is the same approach Apple Siri uses:
//   1. User says "NEXUS" 5 times during enrollment
//   2. Embeddings are extracted and stored as a voice profile
//   3. On wake detection, the detected audio's embedding is compared
//   4. If cosine similarity > threshold, accept; otherwise reject
//   5. Accepted utterances grow the profile (implicit enrollment, up to 40)
//
// The embedding model is the same one used for wake word detection
// (embedding_model.onnx), so no additional model files are needed.

use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

/// Default cosine similarity threshold for speaker verification.
/// OVOS (Open Voice OS) uses 0.45 as default. Apple Siri uses a similar
/// threshold tuned for their proprietary embeddings.
///
/// Lower = more permissive (accepts more speakers)
/// Higher = more strict (only accepts the enrolled speaker)
pub const DEFAULT_THRESHOLD: f32 = 0.45;

/// Maximum number of embedding vectors to store in the profile.
/// Apple Siri grows the profile to 40 vectors via implicit enrollment.
/// We use the same limit.
pub const MAX_ENROLLMENT_VECTORS: usize = 40;

/// Minimum number of enrollment clips required.
pub const MIN_ENROLLMENT_CLIPS: usize = 3;

/// A memory wipe rotates the memory key. Keep the voice profile on its own
/// persisted OS key so that wiping conversation memory cannot strand Voice Lock.
fn profile_key() -> Option<crate::memcore::crypto::Key> {
    #[cfg(test)]
    { Some([9u8; crate::memcore::crypto::KEY_LEN]) }
    #[cfg(not(test))]
    {
        static KEY: once_cell::sync::Lazy<Option<crate::memcore::crypto::Key>> = once_cell::sync::Lazy::new(|| {
            let entry = keyring::Entry::new("com.nexus.assistant", "voice_profile_data_key_v1").ok()?;
            let decode = |encoded: &str| -> Option<crate::memcore::crypto::Key> {
                if encoded.len() != crate::memcore::crypto::KEY_LEN * 2 { return None; }
                let mut key = [0u8; crate::memcore::crypto::KEY_LEN];
                for (index, value) in key.iter_mut().enumerate() {
                    *value = u8::from_str_radix(encoded.get(index * 2..index * 2 + 2)?, 16).ok()?;
                }
                Some(key)
            };
            match entry.get_password() {
                Ok(encoded) => return decode(&encoded),
                Err(keyring::Error::NoEntry) => {},
                Err(_) => return None,
            }
            let key = crate::memcore::crypto::generate_key()?;
            let encoded: String = key.iter().map(|value| format!("{value:02x}")).collect();
            entry.set_password(&encoded).ok()?;
            // Read through a fresh handle to reject a non-persistent backend.
            let persisted = keyring::Entry::new("com.nexus.assistant", "voice_profile_data_key_v1").ok()?
                .get_password().ok()?;
            (persisted == encoded).then_some(key)
        });
        *KEY
    }
}

/// Authorship decision for one captured turn. This is separate from wake-word
/// detection: a loud television can pass energy gates and produce fluent
/// English, but it must never be treated as the enrolled owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnOwnership {
    Verified,
    Uncertain,
    Rejected,
    #[default]
    Unenrolled,
}

impl TurnOwnership {
    pub fn as_str(self) -> &'static str {
        match self {
            TurnOwnership::Verified => "verified",
            TurnOwnership::Uncertain => "uncertain",
            TurnOwnership::Rejected => "rejected",
            TurnOwnership::Unenrolled => "unenrolled",
        }
    }
}

/// Keyboard and direct UI actions remain available for recovery. A captured
/// voice turn may execute only after positive owner verification when locked.
pub fn voice_lock_allows_turn(enabled: bool, captured_voice: bool, owner: TurnOwnership) -> bool {
    !enabled || !captured_voice || owner == TurnOwnership::Verified
}

/// A missing flag is the off-by-default installation case. Malformed settings
/// or a malformed flag hold the lock until an explicit UI recovery change.
pub fn voice_lock_from_settings(content: &str) -> bool {
    let Ok(serde_json::Value::Object(settings)) = serde_json::from_str(content) else { return true; };
    match settings.get("speakerVerification").or_else(|| settings.get("speaker_verification")) {
        None => false,
        Some(serde_json::Value::Bool(enabled)) => *enabled,
        Some(_) => true,
    }
}

/// A stale full-settings form cannot change this security setting. Only a
/// patch explicitly naming Voice Lock is allowed to change its stored value.
pub fn voice_lock_after_settings_save(stored: bool, explicit_change: Option<bool>) -> bool {
    explicit_change.unwrap_or(stored)
}

/// Similarity band below the acceptance threshold that still permits neutral
/// parsing. Anything farther below the owner profile is ambient audio.
pub const TURN_REVIEW_MARGIN: f32 = 0.10;

/// Sound-alike phrases that should NOT trigger wake word.
/// Used for negative training and display in the UI.
pub const SOUND_ALIKES: &[&str] = &[
    "next us", "texas", "lexus", "plex us", "alex us", "nexus 6p",
];

/// Voice profile metadata and embeddings. The disk representation is sealed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfile {
    /// Enrolled embedding vectors (96-dimensional each)
    pub embeddings: Vec<Vec<f32>>,
    /// Cosine similarity threshold for acceptance
    pub threshold: f32,
    /// Number of enrollment clips used
    pub num_clips: usize,
    /// Wake word variants captured during enrollment (e.g., "nexus", "hey nexus")
    pub wake_variants: Vec<String>,
    /// Profile creation timestamp (Unix epoch seconds)
    pub created_at: i64,
    /// Profile last updated timestamp (Unix epoch seconds)
    pub updated_at: i64,
}

impl VoiceProfile {
    /// Create a new empty voice profile.
    pub fn new(threshold: f32) -> Self {
        VoiceProfile {
            embeddings: Vec::new(),
            threshold,
            num_clips: 0,
            wake_variants: vec!["nexus".to_string()],
            created_at: chrono::Utc::now().timestamp(),
            updated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Load a sealed profile. A valid legacy JSON profile is migrated in place.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read(path)
            .map_err(|e| anyhow::anyhow!("Failed to read voice profile: {e}"))?;
        let key = profile_key()
            .ok_or_else(|| anyhow::anyhow!("Voice profile key is unavailable"))?;
        let sealed = crate::memcore::crypto::is_sealed(&data);
        let mut migrate = !sealed;
        let plain = if sealed {
            match crate::memcore::crypto::open(&key, &data) {
                Some(plain) => plain,
                None => {
                    // Migrate profiles from the first shared-memory-key draft.
                    let plain = crate::memcore::crypto::process_key()
                        .and_then(|legacy| crate::memcore::crypto::open(&legacy, &data))
                        .ok_or_else(|| anyhow::anyhow!("Voice profile authentication failed"))?;
                    migrate = true;
                    plain
                }
            }
        } else {
            data
        };
        let profile: VoiceProfile = serde_json::from_slice(&plain)
            .map_err(|e| anyhow::anyhow!("Failed to parse voice profile: {e}"))?;
        if !profile.is_valid() {
            anyhow::bail!("Voice profile is invalid");
        }
        if migrate {
            profile.save(path)?;
        }
        Ok(profile)
    }

    /// Save a sealed profile with the Windows user key.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if !self.is_valid() {
            anyhow::bail!("Voice profile is invalid");
        }
        let key = profile_key()
            .ok_or_else(|| anyhow::anyhow!("Voice profile key is unavailable"))?;
        let data = serde_json::to_vec(self)
            .map_err(|e| anyhow::anyhow!("Failed to serialize voice profile: {e}"))?;
        let sealed = crate::memcore::crypto::seal(&key, &data)
            .ok_or_else(|| anyhow::anyhow!("Failed to encrypt voice profile"))?;
        let pending = path.with_extension("pending");
        std::fs::write(&pending, sealed)
            .map_err(|e| anyhow::anyhow!("Failed to write voice profile: {e}"))?;
        if path.exists() {
            crate::memcore::crypto::shred_file(path);
        }
        std::fs::rename(&pending, path)
            .map_err(|e| anyhow::anyhow!("Failed to replace voice profile: {e}"))?;
        Ok(())
    }

    /// Add an embedding vector to the profile (implicit enrollment).
    /// If the profile is full (MAX_ENROLLMENT_VECTORS), the oldest vector
    /// is removed first (FIFO).
    pub fn add_embedding(&mut self, embedding: Vec<f32>) {
        if self.embeddings.len() >= MAX_ENROLLMENT_VECTORS {
            self.embeddings.remove(0); // Remove oldest (FIFO)
        }
        self.embeddings.push(embedding);
        self.updated_at = chrono::Utc::now().timestamp();
    }

    /// Check if the profile is enrolled (has at least one embedding).
    pub fn is_enrolled(&self) -> bool {
        !self.embeddings.is_empty()
    }

    pub fn is_valid(&self) -> bool {
        self.num_clips >= MIN_ENROLLMENT_CLIPS
            && self.embeddings.len() >= MIN_ENROLLMENT_CLIPS
            && self.embeddings.len() <= MAX_ENROLLMENT_VECTORS
            && self.threshold.is_finite()
            && (DEFAULT_THRESHOLD..=0.95).contains(&self.threshold)
            && self.embeddings.iter().all(|v| {
                v.len() == 96
                    && v.iter().all(|x| x.is_finite())
                    && v.iter().any(|x| *x != 0.0)
            })
    }

    /// Acceptance threshold configured by enrollment.
    #[allow(dead_code)]
    pub fn acceptance_threshold(&self) -> f32 {
        self.threshold
    }

    /// Classify one turn embedding against this profile.
    pub fn classify_turn(&self, similarity: Option<f32>) -> TurnOwnership {
        let Some(similarity) = similarity else {
            return TurnOwnership::Uncertain;
        };
        if similarity >= self.threshold {
            TurnOwnership::Verified
        } else if similarity >= self.threshold - TURN_REVIEW_MARGIN {
            TurnOwnership::Uncertain
        } else {
            TurnOwnership::Rejected
        }
    }

    /// Get the number of enrolled embeddings.
    #[allow(dead_code)]
    pub fn num_embeddings(&self) -> usize {
        self.embeddings.len()
    }
}

/// Status of the voice profile (returned to frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceProfileStatus {
    pub enrolled: bool,
    pub num_clips: usize,
    pub threshold: f32,
    pub created_at: i64,
    pub updated_at: i64,
    pub wake_variants: Vec<String>,
    pub sound_alikes: Vec<String>,
}

/// Compute cosine similarity between two vectors.
/// Returns a value between -1.0 and 1.0.
/// 1.0 = identical, 0.0 = orthogonal, -1.0 = opposite.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Classify turn authorship for an optional profile. Absence of enrollment is
/// explicitly `Unenrolled`; it must not be conflated with a verified owner.
pub fn classify_turn_owner(
    profile: Option<&VoiceProfile>,
    similarity: Option<f32>,
) -> TurnOwnership {
    match profile {
        Some(profile) => profile.classify_turn(similarity),
        None => TurnOwnership::Unenrolled,
    }
}

/// Verify a speaker embedding against an enrolled profile.
/// Returns the maximum cosine similarity across all enrolled vectors.
/// If similarity > threshold, the speaker is verified.
pub fn verify_speaker(profile: &VoiceProfile, embedding: &[f32]) -> f32 {
    if !profile.is_enrolled() {
        return f32::NEG_INFINITY;
    }
    // Compute max similarity across all enrolled vectors
    // Use NEG_INFINITY as initial value so negative similarities are preserved
    let max_sim = profile
        .embeddings
        .iter()
        .map(|v| cosine_similarity(v, embedding))
        .fold(f32::NEG_INFINITY, f32::max);
    max_sim
}

/// Pick a provisional threshold from recordings withheld from enrollment.
/// A poor owner match fails enrollment instead of producing a permissive lock.
pub fn calibrate_owner_threshold(enrolled: &[Vec<f32>], held_out: &[Vec<f32>]) -> anyhow::Result<f32> {
    if enrolled.len() < MIN_ENROLLMENT_CLIPS || held_out.len() < 2 {
        anyhow::bail!("Need at least three enrollment and two held-out recordings");
    }
    let mut profile = VoiceProfile::new(DEFAULT_THRESHOLD);
    profile.embeddings = enrolled.to_vec();
    profile.num_clips = enrolled.len();
    if !profile.is_valid() || held_out.iter().any(|v| v.len() != 96 || v.iter().any(|x| !x.is_finite())) {
        anyhow::bail!("Invalid calibration recordings");
    }
    let min_owner = held_out.iter().map(|v| verify_speaker(&profile, v)).fold(f32::INFINITY, f32::min);
    if !min_owner.is_finite() || min_owner < DEFAULT_THRESHOLD + 0.05 {
        anyhow::bail!("Held-out owner recordings are inconsistent; record them again");
    }
    Ok((min_owner - 0.05).clamp(DEFAULT_THRESHOLD, 0.95))
}

/// Resolve the voice profile file path from the app data directory.
pub fn resolve_profile_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("voice_profile.json")
}

/// Use the most energetic 1.28-second window, rather than a recording's
/// trailing silence. Enrollment and live verification must select alike.
pub fn speaker_audio_window(audio: &[f32]) -> Option<&[f32]> {
    const WINDOW: usize = 16 * 1280;
    if audio.is_empty() || audio.iter().any(|sample| !sample.is_finite() || sample.abs() > 1.0) {
        return None;
    }
    let width = audio.len().min(WINDOW);
    let square = |sample: f32| f64::from(sample).powi(2);
    let mut energy: f64 = audio[..width].iter().map(|&sample| square(sample)).sum();
    let mut best_energy = energy;
    let mut best_start = 0;
    for end in width..audio.len() {
        energy += square(audio[end]) - square(audio[end - width]);
        if energy > best_energy {
            best_energy = energy;
            best_start = end + 1 - width;
        }
    }
    (best_energy > 0.0).then_some(&audio[best_start..best_start + width])
}

/// Pool exactly the live pipeline's sixteen 96-dimensional feature frames.
/// Malformed model output must not be padded into an apparently valid profile.
pub fn pool_speaker_frames(frames: &[f32]) -> Option<Vec<f32>> {
    if frames.len() != 16 * 96 || frames.iter().any(|value| !value.is_finite()) {
        return None;
    }
    let embedding: Vec<f32> = (0..96)
        .map(|column| (0..16).map(|row| frames[row * 96 + column]).sum::<f32>() / 16.0)
        .collect();
    (embedding.iter().all(|value| value.is_finite())
        && embedding.iter().any(|value| *value != 0.0)).then_some(embedding)
}

/// Delete a profile and report an OS refusal instead of claiming success.
/// Voice Lock remains enabled; a missing profile therefore holds voice turns.
pub fn delete_profile_file(path: &Path) -> anyhow::Result<()> {
    if path.try_exists()? {
        crate::memcore::crypto::shred_file(path);
        if path.try_exists()? {
            anyhow::bail!("Could not delete voice profile; check file permissions and try again");
        }
    }
    Ok(())
}

/// Speaker verifier using the OWW embedding model.
/// Extracts embeddings from audio and compares to enrolled profile.
pub struct SpeakerVerifier {
    /// Path to the voice profile JSON file
    profile_path: PathBuf,
    /// Loaded voice profile (if enrolled)
    profile: Option<VoiceProfile>,
}

impl SpeakerVerifier {
    /// Explicit re-enrollment replaces the old profile only after valid clips
    /// are processed. A corrupt old file must not prevent this recovery path.
    pub fn for_enrollment(profile_path: PathBuf) -> Self {
        Self { profile_path, profile: None }
    }

    /// Create a new speaker verifier.
    /// The profile is loaded from disk if it exists.
    pub fn new(profile_path: PathBuf) -> anyhow::Result<Self> {
        let profile = if profile_path.exists() {
            Some(VoiceProfile::load(&profile_path)?)
        } else {
            None
        };
        Ok(SpeakerVerifier {
            profile_path,
            profile,
        })
    }

    /// Get the loaded voice profile (if any).
    pub fn profile(&self) -> Option<&VoiceProfile> {
        self.profile.as_ref()
    }

    pub fn reload(&mut self) -> anyhow::Result<()> {
        self.profile = None;
        if self.profile_path.exists() {
            self.profile = Some(VoiceProfile::load(&self.profile_path)?);
        }
        Ok(())
    }

    /// Check if a voice profile is enrolled.
    pub fn is_enrolled(&self) -> bool {
        self.profile.as_ref().map(|p| p.is_valid()).unwrap_or(false)
    }

    /// Enroll a speaker using the provided embedding vectors.
    /// Each embedding is extracted from an enrollment clip.
    pub fn enroll(
        &mut self,
        embeddings: Vec<Vec<f32>>,
        threshold: f32,
        wake_variants: Vec<String>,
    ) -> anyhow::Result<()> {
        if embeddings.len() < MIN_ENROLLMENT_CLIPS {
            return Err(anyhow::anyhow!(
                "Need at least {} enrollment clips, got {}",
                MIN_ENROLLMENT_CLIPS,
                embeddings.len()
            ));
        }

        if !threshold.is_finite() || !(DEFAULT_THRESHOLD..=0.95).contains(&threshold) {
            return Err(anyhow::anyhow!("Invalid speaker threshold"));
        }
        if embeddings.iter().any(|v| v.len() != 96 || v.iter().any(|x| !x.is_finite()) || v.iter().all(|x| *x == 0.0)) {
            return Err(anyhow::anyhow!("Invalid speaker embedding"));
        }

        let mut profile = VoiceProfile::new(threshold);
        for embedding in embeddings {
            profile.add_embedding(embedding);
        }
        profile.num_clips = profile.embeddings.len();
        if !wake_variants.is_empty() {
            profile.wake_variants = wake_variants;
        }
        profile.save(&self.profile_path)?;
        self.profile = Some(profile);
        Ok(())
    }

    /// Verify a speaker embedding against the enrolled profile.
    /// Returns (similarity, is_verified).
    /// If no valid profile is enrolled, verification fails closed.
    pub fn verify(&self, embedding: &[f32]) -> (f32, bool) {
        match &self.profile {
            None => (f32::NEG_INFINITY, false),
            Some(profile) => {
                if !profile.is_valid() {
                    return (f32::NEG_INFINITY, false);
                }
                let sim = verify_speaker(profile, embedding);
                (sim, sim.is_finite() && sim >= profile.threshold)
            }
        }
    }

    /// Add an accepted embedding to the profile (implicit enrollment).
    /// This grows the profile up to MAX_ENROLLMENT_VECTORS.
    #[allow(dead_code)]
    pub fn implicit_enroll(&mut self, embedding: Vec<f32>) -> anyhow::Result<()> {
        if let Some(profile) = &mut self.profile {
            profile.add_embedding(embedding);
            profile.save(&self.profile_path)?;
        }
        Ok(())
    }

    /// Delete the voice profile.
    #[allow(dead_code)]
    pub fn delete(&mut self) -> anyhow::Result<()> {
        self.profile = None;
        delete_profile_file(&self.profile_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001, "Identical vectors should have sim=1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001, "Orthogonal vectors should have sim=0.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0];
        let b = vec![-1.0, -2.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.001, "Opposite vectors should have sim=-1.0, got {sim}");
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let a: Vec<f32> = vec![];
        let b: Vec<f32> = vec![];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0, "Empty vectors should return 0.0");
    }

    #[test]
    fn test_voice_profile_new() {
        let profile = VoiceProfile::new(DEFAULT_THRESHOLD);
        assert!(!profile.is_enrolled());
        assert_eq!(profile.num_embeddings(), 0);
        assert_eq!(profile.threshold, DEFAULT_THRESHOLD);
        assert_eq!(profile.wake_variants, vec!["nexus".to_string()]);
    }

    #[test]
    fn test_voice_profile_add_embedding() {
        let mut profile = VoiceProfile::new(DEFAULT_THRESHOLD);
        let embedding = vec![0.1; 96];
        profile.add_embedding(embedding);
        assert!(profile.is_enrolled());
        assert_eq!(profile.num_embeddings(), 1);
    }

    #[test]
    fn test_voice_profile_max_embeddings() {
        let mut profile = VoiceProfile::new(DEFAULT_THRESHOLD);
        for i in 0..MAX_ENROLLMENT_VECTORS + 5 {
            profile.add_embedding(vec![i as f32; 96]);
        }
        assert_eq!(profile.num_embeddings(), MAX_ENROLLMENT_VECTORS);
    }

    #[test]
    fn test_turn_ownership_zones() {
        let profile = VoiceProfile::new(0.45);
        assert_eq!(
            classify_turn_owner(Some(&profile), Some(0.72)),
            TurnOwnership::Verified
        );
        assert_eq!(
            classify_turn_owner(Some(&profile), Some(0.40)),
            TurnOwnership::Uncertain
        );
        assert_eq!(
            classify_turn_owner(Some(&profile), Some(0.12)),
            TurnOwnership::Rejected
        );
        assert_eq!(
            classify_turn_owner(Some(&profile), None),
            TurnOwnership::Uncertain
        );
        assert_eq!(
            classify_turn_owner(None, Some(0.99)),
            TurnOwnership::Unenrolled
        );
        assert_eq!(TurnOwnership::Rejected.as_str(), "rejected");
    }

    #[test]
    fn test_verify_speaker_no_profile() {
        let profile = VoiceProfile::new(DEFAULT_THRESHOLD);
        let embedding = vec![0.5; 96];
        let sim = verify_speaker(&profile, &embedding);
        assert_eq!(sim, f32::NEG_INFINITY, "No profile must fail closed");
    }

    #[test]
    fn test_verify_speaker_enrolled() {
        let mut profile = VoiceProfile::new(DEFAULT_THRESHOLD);
        let enrolled_embedding = vec![0.5; 96];
        profile.add_embedding(enrolled_embedding);

        // Same embedding → high similarity
        let test_embedding = vec![0.5; 96];
        let sim = verify_speaker(&profile, &test_embedding);
        assert!(sim > 0.99, "Same embedding should have sim > 0.99, got {sim}");

        // Different embedding → lower similarity
        let test_embedding = vec![-0.5; 96];
        let sim = verify_speaker(&profile, &test_embedding);
        assert!(sim < 0.0, "Opposite embedding should have sim < 0.0, got {sim}");
    }

    #[test]
    fn test_speaker_verifier_enroll_and_verify() {
        let temp_dir = std::env::temp_dir().join("nexus_voice_profile_test");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let profile_path = temp_dir.join("voice_profile.json");

        // Clean up any existing profile
        let _ = std::fs::remove_file(&profile_path);

        let mut verifier = SpeakerVerifier::new(profile_path.clone()).unwrap();
        assert!(!verifier.is_enrolled());

        // Enroll with 3 embeddings
        let embeddings = vec![
            vec![0.5; 96],
            vec![0.51; 96],
            vec![0.49; 96],
        ];
        verifier
            .enroll(embeddings, DEFAULT_THRESHOLD, vec!["nexus".to_string()])
            .unwrap();
        assert!(verifier.is_enrolled());

        // Verify with similar embedding → should pass
        let test_embedding = vec![0.5; 96];
        let (sim, verified) = verifier.verify(&test_embedding);
        assert!(verified, "Similar embedding should be verified (sim={sim})");
        assert!(sim > DEFAULT_THRESHOLD, "Similar embedding should have sim > threshold");

        // Verify with different embedding → should fail
        let test_embedding = vec![-0.5; 96];
        let (sim, verified) = verifier.verify(&test_embedding);
        assert!(!verified, "Different embedding should not be verified (sim={sim})");
        assert!(sim < DEFAULT_THRESHOLD, "Different embedding should have sim < threshold");

        // Clean up
        let _ = std::fs::remove_file(&profile_path);
    }

    #[test]
    fn test_speaker_verifier_implicit_enroll() {
        let temp_dir = std::env::temp_dir().join("nexus_voice_profile_test2");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let profile_path = temp_dir.join("voice_profile.json");
        let _ = std::fs::remove_file(&profile_path);

        let mut verifier = SpeakerVerifier::new(profile_path.clone()).unwrap();

        // Enroll with 3 embeddings
        verifier
            .enroll(
                vec![vec![0.5; 96], vec![0.51; 96], vec![0.49; 96]],
                DEFAULT_THRESHOLD,
                vec!["nexus".to_string()],
            )
            .unwrap();
        assert_eq!(verifier.profile().unwrap().num_embeddings(), 3);

        // Implicit enroll adds one more
        verifier.implicit_enroll(vec![0.52; 96]).unwrap();
        assert_eq!(verifier.profile().unwrap().num_embeddings(), 4);

        // Clean up
        let _ = std::fs::remove_file(&profile_path);
    }

    #[test]
    fn test_speaker_verifier_delete() {
        let temp_dir = std::env::temp_dir().join("nexus_voice_profile_test3");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let profile_path = temp_dir.join("voice_profile.json");
        let _ = std::fs::remove_file(&profile_path);

        let mut verifier = SpeakerVerifier::new(profile_path.clone()).unwrap();
        verifier
            .enroll(
                vec![vec![0.5; 96], vec![0.51; 96], vec![0.49; 96]],
                DEFAULT_THRESHOLD,
                vec!["nexus".to_string()],
            )
            .unwrap();
        assert!(verifier.is_enrolled());
        assert!(profile_path.exists());

        verifier.delete().unwrap();
        assert!(!verifier.is_enrolled());
        assert!(!profile_path.exists());
    }

    #[test]
    fn test_voice_profile_save_load() {
        let temp_dir = std::env::temp_dir().join("nexus_voice_profile_test4");
        std::fs::create_dir_all(&temp_dir).unwrap();
        let profile_path = temp_dir.join("voice_profile.json");
        let _ = std::fs::remove_file(&profile_path);

        let mut profile = VoiceProfile::new(0.5);
        profile.add_embedding(vec![0.1; 96]);
        profile.add_embedding(vec![0.2; 96]);
        profile.add_embedding(vec![0.3; 96]);
        profile.num_clips = 3;
        profile.wake_variants = vec!["nexus".to_string(), "hey nexus".to_string()];
        profile.save(&profile_path).unwrap();

        let loaded = VoiceProfile::load(&profile_path).unwrap();
        assert_eq!(loaded.num_embeddings(), 3);
        assert_eq!(loaded.threshold, 0.5);
        assert_eq!(loaded.wake_variants, vec!["nexus".to_string(), "hey nexus".to_string()]);

        let _ = std::fs::remove_file(&profile_path);
    }

    #[test]
    fn voice_lock_requires_positive_owner_proof_for_captured_voice() {
        for owner in [TurnOwnership::Unenrolled, TurnOwnership::Uncertain, TurnOwnership::Rejected] {
            assert!(!voice_lock_allows_turn(true, true, owner));
            assert!(voice_lock_allows_turn(false, true, owner));
            assert!(voice_lock_allows_turn(true, false, owner));
        }
        assert!(voice_lock_allows_turn(true, true, TurnOwnership::Verified));
    }

    #[test]
    fn corrupt_settings_hold_the_lock_and_unrelated_saves_preserve_it() {
        assert!(!voice_lock_from_settings("{}"));
        assert!(!voice_lock_from_settings(r#"{"speakerVerification":false}"#));
        assert!(voice_lock_from_settings(r#"{"speakerVerification":true}"#));
        assert!(voice_lock_from_settings(r#"{"speaker_verification":true}"#));
        for content in ["broken", "null", "[]", r#"{"speakerVerification":"false"}"#] {
            assert!(voice_lock_from_settings(content));
        }
        assert!(voice_lock_after_settings_save(true, None));
        assert!(!voice_lock_after_settings_save(false, None));
        assert!(!voice_lock_after_settings_save(true, Some(false)));
        assert!(voice_lock_after_settings_save(false, Some(true)));
    }

    #[test]
    fn held_out_owner_recordings_set_threshold_without_entering_profile() {
        let mut base = vec![0.0; 96];
        base[0] = 1.0;
        let mut held_out = vec![0.0; 96];
        held_out[0] = 0.8;
        held_out[1] = 0.6;
        let threshold = calibrate_owner_threshold(&vec![base; 5], &vec![held_out; 5]).unwrap();
        assert!((threshold - 0.75).abs() < 0.001);
        let bad = vec![0.0; 96];
        assert!(calibrate_owner_threshold(&vec![vec![1.0; 96]; 5], &vec![bad; 5]).is_err());
    }

    #[test]
    fn missing_invalid_and_corrupt_profiles_do_not_verify() {
        let path = std::env::temp_dir().join("nexus_voice_profile_fail_closed_test.json");
        let _ = std::fs::remove_file(&path);
        let mut verifier = SpeakerVerifier::new(path.clone()).unwrap();
        assert!(!verifier.verify(&vec![1.0; 96]).1);
        assert!(verifier.enroll(vec![vec![0.0; 96]; 3], 0.5, vec![]).is_err());
        assert!(verifier.enroll(vec![vec![1.0; 96]; 3], f32::NAN, vec![]).is_err());
        assert!(verifier.enroll(vec![vec![1.0; 96]; 3], 0.0, vec![]).is_err());
        std::fs::write(&path, b"broken profile").unwrap();
        assert!(verifier.reload().is_err());
        assert!(!verifier.verify(&vec![1.0; 96]).1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn explicit_enrollment_recovers_a_corrupt_profile() {
        let path = std::env::temp_dir().join(format!("nexus_profile_reenroll_{}.json", std::process::id()));
        std::fs::write(&path, b"corrupt profile").unwrap();
        assert!(SpeakerVerifier::new(path.clone()).is_err());
        let mut verifier = SpeakerVerifier::for_enrollment(path.clone());
        assert!(!verifier.verify(&vec![0.2; 96]).1);
        verifier.enroll(vec![vec![0.2; 96]; 5], 0.5, vec![]).unwrap();
        assert!(SpeakerVerifier::new(path.clone()).unwrap().verify(&vec![0.2; 96]).1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn profile_is_sealed_and_legacy_profile_migrates() {
        let path = std::env::temp_dir().join("nexus_voice_profile_crypto_test.json");
        let _ = std::fs::remove_file(&path);
        let mut profile = VoiceProfile::new(0.5);
        for _ in 0..3 { profile.add_embedding(vec![0.2; 96]); }
        profile.num_clips = 3;
        std::fs::write(&path, serde_json::to_vec(&profile).unwrap()).unwrap();
        let loaded = VoiceProfile::load(&path).unwrap();
        assert!(loaded.is_valid());
        let bytes = std::fs::read(&path).unwrap();
        assert!(crate::memcore::crypto::is_sealed(&bytes));
        assert!(!String::from_utf8_lossy(&bytes).contains("embeddings"));
        let mut tampered = bytes;
        *tampered.last_mut().unwrap() ^= 1;
        std::fs::write(&path, tampered).unwrap();
        assert!(VoiceProfile::load(&path).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn shared_memory_key_profile_migrates_to_independent_voice_key() {
        let path = std::env::temp_dir().join(format!("nexus_profile_key_migration_{}.json", std::process::id()));
        let mut profile = VoiceProfile::new(0.5);
        for _ in 0..3 { profile.add_embedding(vec![0.2; 96]); }
        profile.num_clips = 3;
        let legacy = crate::memcore::crypto::process_key().unwrap();
        let dedicated = profile_key().unwrap();
        assert_ne!(legacy, dedicated);
        let bytes = crate::memcore::crypto::seal(&legacy, &serde_json::to_vec(&profile).unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        assert!(VoiceProfile::load(&path).unwrap().is_valid());
        let migrated = std::fs::read(&path).unwrap();
        assert!(crate::memcore::crypto::open(&dedicated, &migrated).is_some());
        assert!(crate::memcore::crypto::open(&legacy, &migrated).is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn failed_profile_deletion_is_reported() {
        let path = std::env::temp_dir().join(format!("nexus_profile_delete_test_{}", std::process::id()));
        std::fs::create_dir(&path).unwrap();
        // A directory at the profile path cannot be removed by the file shredder.
        let result = delete_profile_file(&path);
        let still_exists = path.try_exists().unwrap();
        std::fs::remove_dir(&path).unwrap();
        assert!(result.is_err());
        assert!(still_exists);
        assert!(delete_profile_file(&path).is_ok());
    }

    #[test]
    fn speaker_window_preserves_signal_instead_of_silent_tail() {
        let mut audio = vec![0.0; 48_000];
        audio[4_000..10_000].fill(0.5);
        let window = speaker_audio_window(&audio).unwrap();
        assert_eq!(window.len(), 20_480);
        assert_eq!(window.iter().filter(|&&sample| sample == 0.5).count(), 6_000);
        assert!(speaker_audio_window(&vec![0.0; 48_000]).is_none());
        assert!(speaker_audio_window(&[f32::NAN]).is_none());
        assert!(speaker_audio_window(&[1.1]).is_none());
    }

    #[test]
    fn speaker_pool_rejects_malformed_model_output() {
        assert_eq!(pool_speaker_frames(&vec![0.5; 16 * 96]).unwrap(), vec![0.5; 96]);
        assert!(pool_speaker_frames(&vec![0.5; 16 * 96 - 1]).is_none());
        assert!(pool_speaker_frames(&vec![0.0; 16 * 96]).is_none());
        assert!(pool_speaker_frames(&vec![f32::MAX; 16 * 96]).is_none());
        let mut frames = vec![0.5; 16 * 96];
        frames[0] = f32::NAN;
        assert!(pool_speaker_frames(&frames).is_none());
    }
}
