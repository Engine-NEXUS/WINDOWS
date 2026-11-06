//! Echo-aware onset gate for speech over TTS (barge-in without wake word).
//!
//! Tracks the ratio `r = mic_rms / (ref_level * REF_SCALE + EPS)` with a slow EMA
//! as the learned speaker-to-mic echo gain. A chunk is a candidate onset if
//! `mic_rms > max(BARGE_RMS_FLOOR, 2.5 * ema * (ref_level * REF_SCALE + EPS))`.
//! Returns true after `BARGE_ONSET_CHUNKS = 3` consecutive onset chunks (240 ms).
//! Resets on any non-onset chunk. When `ref_level` is None, falls back to
//! `mic_rms > 2 * BARGE_RMS_FLOOR`.

pub const BARGE_RMS_FLOOR: f32 = 0.01;
pub const BARGE_ONSET_CHUNKS: u32 = 3;
pub const REF_SCALE: f32 = 1.0;
pub const EPS: f32 = 1e-4;
pub const DEFAULT_RATIO_EMA: f32 = 0.1;
pub const EMA_ALPHA: f32 = 0.03;
/// Chunks (80 ms each) at the start of a TTS session during which the gate only LEARNS the echo gain
/// and never reports an onset, so the default gain guess cannot duck every reply at its start.
pub const WARMUP_CHUNKS: u32 = 10;

#[derive(Debug, Clone)]
pub struct EchoGate {
    pub ratio_ema: f32,
    pub run: u32,
    pub seen: u32,
}

impl EchoGate {
    pub const fn new() -> Self {
        Self {
            ratio_ema: DEFAULT_RATIO_EMA,
            run: 0,
            seen: 0,
        }
    }

    pub fn reset(&mut self) {
        self.ratio_ema = DEFAULT_RATIO_EMA;
        self.run = 0;
        self.seen = 0;
    }

    /// Is this chunk the owner's voice rather than NEXUS's own (possibly ducked) echo? Used by the
    /// capture endpointer while a barge capture runs over TTS: the Silero/energy VAD would call the
    /// echo "speech" forever and the turn would never end until the 10 s cap.
    pub fn is_voice_chunk(&self, mic_rms: f32, ref_level: Option<f32>, duck_gain: f32) -> bool {
        match ref_level {
            Some(r) => {
                let expected = self.ratio_ema * (r * REF_SCALE + EPS) * duck_gain;
                mic_rms > (BARGE_RMS_FLOOR + 0.002).max(1.5 * expected)
            }
            None => mic_rms > 2.0 * BARGE_RMS_FLOOR * duck_gain.max(0.5),
        }
    }

    pub fn update(&mut self, mic_rms: f32, ref_level: Option<f32>) -> bool {
        match ref_level {
            Some(ref_lvl) => {
                let denom = ref_lvl * REF_SCALE + EPS;
                let r = mic_rms / denom;
                let threshold = BARGE_RMS_FLOOR.max(2.5 * self.ratio_ema * denom);
                self.seen = self.seen.saturating_add(1);
                let is_onset = self.seen > WARMUP_CHUNKS && mic_rms > threshold;

                if is_onset {
                    self.run = self.run.saturating_add(1);
                    if self.run >= BARGE_ONSET_CHUNKS {
                        return true;
                    }
                } else {
                    self.run = 0;
                    // Only update ratio_ema when reference level indicates active speech
                    if ref_lvl >= 0.01 {
                        let r_clamped = r.clamp(0.0, 1.0);
                        self.ratio_ema = (self.ratio_ema * (1.0 - EMA_ALPHA) + r_clamped * EMA_ALPHA).clamp(0.01, 1.0);
                    }
                }
                false
            }
            None => {
                let is_onset = mic_rms > 2.0 * BARGE_RMS_FLOOR;
                if is_onset {
                    self.run = self.run.saturating_add(1);
                    if self.run >= BARGE_ONSET_CHUNKS {
                        return true;
                    }
                } else {
                    self.run = 0;
                }
                false
            }
        }
    }
}

impl Default for EchoGate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn warm(g: &mut EchoGate) {
        for _ in 0..WARMUP_CHUNKS {
            assert!(!g.update(0.05, Some(0.5)));
        }
    }

    #[test]
    fn test_steady_echo_never_fires() {
        let mut gate = EchoGate::new();
        // Steady echo: reference level is 0.5, mic hears echo at 10% gain (0.05).
        for _ in 0..100 {
            assert!(!gate.update(0.05, Some(0.5)));
            assert_eq!(gate.run, 0);
        }
    }

    #[test]
    fn test_echo_plus_loud_voice_fires_in_3_chunks() {
        let mut gate = EchoGate::new();
        // Adapt on steady echo
        for _ in 0..20 {
            assert!(!gate.update(0.05, Some(0.5)));
        }

        // Owner speaks over TTS: mic_rms jumps to 0.25 (well above 2.5 * 0.1 * 0.5 = 0.125)
        assert!(!gate.update(0.25, Some(0.5)), "chunk 1 should not fire");
        assert_eq!(gate.run, 1);
        assert!(!gate.update(0.25, Some(0.5)), "chunk 2 should not fire");
        assert_eq!(gate.run, 2);
        assert!(gate.update(0.25, Some(0.5)), "chunk 3 must fire");
        assert_eq!(gate.run, 3);
    }

    #[test]
    fn test_echo_gain_change_adapts() {
        let mut gate = EchoGate::new();
        // Volume rises slowly: echo ratio increases from 0.05 to 0.08 over 60 chunks
        for i in 0..60 {
            let gain = 0.05 + 0.03 * (i as f32 / 60.0);
            let mic_rms = 0.5 * gain;
            assert!(!gate.update(mic_rms, Some(0.5)), "slow echo transition must not fire");
            assert_eq!(gate.run, 0);
        }
        // EMA has adapted towards ~0.08
        assert!(gate.ratio_ema < 0.12);
    }

    #[test]
    fn test_silence_never_fires() {
        let mut gate = EchoGate::new();
        // Digital silence and quiet room tone
        for _ in 0..50 {
            assert!(!gate.update(0.001, Some(0.0)));
            assert!(!gate.update(0.001, Some(0.5)));
            assert_eq!(gate.run, 0);
        }
    }

    #[test]
    fn test_none_reference_fallback() {
        let mut gate = EchoGate::new();
        // Below 2 * BARGE_RMS_FLOOR (0.02)
        for _ in 0..10 {
            assert!(!gate.update(0.015, None));
            assert_eq!(gate.run, 0);
        }

        // Voice above fallback threshold: 0.030 > 0.020
        assert!(!gate.update(0.030, None), "chunk 1 should not fire");
        assert_eq!(gate.run, 1);
        assert!(!gate.update(0.030, None), "chunk 2 should not fire");
        assert_eq!(gate.run, 2);
        assert!(gate.update(0.030, None), "chunk 3 must fire");
        assert_eq!(gate.run, 3);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut gate = EchoGate::new();
        warm(&mut gate);
        let _ = gate.update(0.25, Some(0.5));
        let _ = gate.update(0.25, Some(0.5));
        assert_eq!(gate.run, 2);

        gate.reset();
        assert_eq!(gate.run, 0);
        assert_eq!(gate.seen, 0);
        assert!((gate.ratio_ema - DEFAULT_RATIO_EMA).abs() < 1e-6);
    }

    #[test]
    fn test_non_onset_chunk_resets_run() {
        let mut gate = EchoGate::new();
        warm(&mut gate);
        assert!(!gate.update(0.25, Some(0.5)));
        assert_eq!(gate.run, 1);
        assert!(!gate.update(0.25, Some(0.5)));
        assert_eq!(gate.run, 2);

        // A single non-onset chunk resets run to 0
        assert!(!gate.update(0.02, Some(0.5)));
        assert_eq!(gate.run, 0);

        // Next chunk starts from 1 again
        assert!(!gate.update(0.25, Some(0.5)));
        assert_eq!(gate.run, 1);
    }

    #[test]
    fn test_warmup_never_fires_even_on_loud_voice() {
        let mut gate = EchoGate::new();
        for _ in 0..WARMUP_CHUNKS {
            assert!(!gate.update(0.5, Some(0.5)));
        }
    }

    #[test]
    fn test_is_voice_chunk_ignores_ducked_echo_but_hears_voice() {
        let gate = EchoGate::new();
        // echo 0.05 at reference 0.5 ducked to 25%: expected 0.0125; voice 0.2 is far above
        assert!(!gate.is_voice_chunk(0.012, Some(0.5), 0.25));
        assert!(gate.is_voice_chunk(0.2, Some(0.5), 0.25));
        assert!(!gate.is_voice_chunk(0.005, None, 1.0));
        assert!(gate.is_voice_chunk(0.05, None, 1.0));
    }
}
