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
/// While a reference level exists, an onset also needs the mic above this absolute level. Between words and in the
/// tail of a reply the reference is near zero, so the relative threshold fell to the 0.01 floor and the room's reverb of
/// the voice itself counted as someone talking over it. A person speaking over a reply is far louder (0.05 to 0.2).
pub const ONSET_FLOOR: f32 = 0.03;
/// With no reference level (the first 0.2 to 0.5 s of a reply, before playback is "playing") the mic must exceed this
/// level, and only after the warm-up. Before, 2 x `BARGE_RMS_FLOOR` (0.02) with no warm-up let room noise start a
/// barge-in right at the start of most replies; each one ducked the voice to 25% until the capture was classified.
pub const NO_REF_ONSET: f32 = 0.04;
/// The learned echo gain never goes below this: measured against a near-silent reference it collapsed to 0.01.
pub const MIN_RATIO: f32 = 0.05;
/// An onset needs the mic to exceed this multiple of the learned echo (was 2.5).
pub const ONSET_MULT: f32 = 3.0;
/// The mic hears the speaker a little after the reference level says it played, so a new word makes the mic briefly
/// louder than the CURRENT reference. The threshold uses the loudest reference of the last `REF_WINDOW` chunks (400 ms).
pub const REF_WINDOW: usize = 5;
/// While learning (the first chunks of a reply) the echo gain moves this fast, from every chunk: the owner is not
/// talking over the first 0.8 s of a reply, and learning only from "quiet" chunks left a too-low gain that then
/// flagged the echo itself as an onset forever.
pub const LEARN_ALPHA: f32 = 0.2;

#[derive(Debug, Clone)]
pub struct EchoGate {
    pub ratio_ema: f32,
    pub run: u32,
    pub seen: u32,
    recent: [f32; REF_WINDOW],
}

impl EchoGate {
    pub const fn new() -> Self {
        Self {
            ratio_ema: DEFAULT_RATIO_EMA,
            run: 0,
            seen: 0,
            recent: [0.0; REF_WINDOW],
        }
    }

    pub fn reset(&mut self) {
        self.ratio_ema = DEFAULT_RATIO_EMA;
        self.run = 0;
        self.seen = 0;
        self.recent = [0.0; REF_WINDOW];
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
                let slot = (self.seen as usize) % REF_WINDOW;
                self.recent[slot] = ref_lvl;
                let peak = self.recent.iter().copied().fold(0.0_f32, f32::max);
                let denom = peak * REF_SCALE + EPS;
                let r = mic_rms / denom;
                let threshold = ONSET_FLOOR.max(ONSET_MULT * self.ratio_ema * denom);
                self.seen = self.seen.saturating_add(1);
                if self.seen <= WARMUP_CHUNKS {
                    // learning phase: every chunk teaches the echo gain, and nothing can fire
                    if peak >= 0.01 {
                        self.ratio_ema = (self.ratio_ema * (1.0 - LEARN_ALPHA) + r.clamp(0.0, 1.0) * LEARN_ALPHA).clamp(MIN_RATIO, 1.0);
                    }
                    return false;
                }
                let is_onset = mic_rms > threshold;

                if is_onset {
                    self.run = self.run.saturating_add(1);
                    if self.run >= BARGE_ONSET_CHUNKS {
                        return true;
                    }
                } else {
                    self.run = 0;
                    // Only update ratio_ema when reference level indicates active speech
                    if peak >= 0.01 {
                        let r_clamped = r.clamp(0.0, 1.0);
                        self.ratio_ema = (self.ratio_ema * (1.0 - EMA_ALPHA) + r_clamped * EMA_ALPHA).clamp(MIN_RATIO, 1.0);
                    }
                }
                false
            }
            None => {
                self.seen = self.seen.saturating_add(1);
                let is_onset = self.seen > WARMUP_CHUNKS && mic_rms > NO_REF_ONSET;
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
    fn a_loud_echo_is_learned_in_the_first_chunks_and_never_fires() {
        // Mic next to the speaker: the echo is 30% of the reference, three times the default gain guess.
        let mut gate = EchoGate::new();
        for i in 0..200 {
            assert!(!gate.update(0.15, Some(0.5)), "chunk {i}: steady loud echo is not the owner");
        }
        assert!(gate.ratio_ema > 0.25, "learned {}", gate.ratio_ema);
    }

    #[test]
    fn a_word_onset_in_the_echo_that_arrives_late_does_not_fire() {
        // The mic hears each reference level three chunks (240 ms) later, so in the pause after a word it is still
        // louder than the reference of that moment.
        let reference = [0.1_f32, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8, 0.1, 0.1, 0.1, 0.1, 0.1, 0.1, 0.8, 0.8, 0.8, 0.8, 0.8, 0.8];
        let mut gate = EchoGate::new();
        for (i, _) in reference.iter().enumerate() {
            let heard = reference[i.saturating_sub(3)] * 0.3;
            assert!(!gate.update(heard, Some(reference[i])), "chunk {i}");
        }
    }

    #[test]
    fn reverb_in_the_tail_of_a_reply_is_not_someone_talking_over_it() {
        let mut gate = EchoGate::new();
        for _ in 0..30 {
            assert!(!gate.update(0.15, Some(0.5)));
        }
        // the reply ends: the reference falls to nothing, the room still rings at 0.015 for a second
        for i in 0..20 {
            assert!(!gate.update(0.015, Some(0.003)), "tail chunk {i}");
        }
        assert!(gate.ratio_ema >= MIN_RATIO, "the echo gain must not collapse: {}", gate.ratio_ema);
        // a real voice over a quiet moment still gets through
        for _ in 0..2 {
            assert!(!gate.update(0.12, Some(0.003)));
        }
        assert!(gate.update(0.12, Some(0.003)));
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
        // Quiet room during the warm-up
        for _ in 0..WARMUP_CHUNKS {
            assert!(!gate.update(0.015, None));
            assert_eq!(gate.run, 0);
        }

        // A real voice well above the no-reference threshold fires after three chunks
        assert!(!gate.update(0.08, None), "chunk 1 should not fire");
        assert_eq!(gate.run, 1);
        assert!(!gate.update(0.08, None), "chunk 2 should not fire");
        assert_eq!(gate.run, 2);
        assert!(gate.update(0.08, None), "chunk 3 must fire");
        assert_eq!(gate.run, 3);
    }

    #[test]
    fn room_noise_at_the_start_of_a_reply_does_not_duck_the_voice() {
        // No reference yet (playback not "playing"): a 0.03 burst for the whole first second used to fire at once
        // (it exceeded 0.02) and duck the reply to 25%.
        let mut gate = EchoGate::new();
        for i in 0..WARMUP_CHUNKS {
            assert!(!gate.update(0.5, None), "warm-up chunk {i}: nothing can fire, whatever the level");
        }
        for i in 0..30 {
            assert!(!gate.update(0.03, None), "chunk {i}: ordinary room level is not someone talking over the reply");
        }
        assert_eq!(gate.run, 0);
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
