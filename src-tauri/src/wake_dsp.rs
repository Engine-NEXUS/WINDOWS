//! Wake-word front-end parameters shared by the runtime and the trainer.
//!
//! The classifier is trained on audio shaped by the trainer's high-pass /
//! gate / AGC (`scripts/train_local_wakeword.py`) but used to be served audio
//! shaped by a *different* chain (different AGC target and cap, a minimum 2.5×
//! gain on everything, loud speech multiplied and clipped). The numbers now
//! live in `resources/oww/dsp_spec.json`, read by both sides. `legacy` is the
//! previous runtime behaviour (still the default — no change until measured);
//! `aligned` is the trainer's chain. Compare with `wake_eval --dsp`.

use serde::Deserialize;

const SPEC_JSON: &str = include_str!("../resources/oww/dsp_spec.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DspProfile {
    Legacy,
    Aligned,
}

impl DspProfile {
    /// "aligned" (any case) → Aligned; anything else → Legacy.
    pub fn from_label(label: &str) -> Self {
        if label.trim().eq_ignore_ascii_case("aligned") {
            DspProfile::Aligned
        } else {
            DspProfile::Legacy
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DspProfile::Legacy => "legacy",
            DspProfile::Aligned => "aligned",
        }
    }
}

/// Resolved front-end parameters for one stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DspParams {
    pub highpass_hz: f32,
    /// Chunks below this RMS (after the high-pass) never reach the classifier.
    pub silence_rms: f32,
    pub target_rms: f32,
    pub max_gain: f32,
    /// Minimum gain applied to every chunk that reaches the classifier.
    pub floor_gain: f32,
}

#[derive(Deserialize)]
struct Aligned {
    highpass_hz: f32,
    silence_rms: f32,
    target_rms: f32,
    max_gain: f32,
    floor_gain: f32,
}
#[derive(Deserialize)]
struct Legacy {
    target_rms: f32,
    max_gain: f32,
}
#[derive(Deserialize)]
struct Spec {
    aligned: Aligned,
    legacy: Legacy,
    #[serde(default)]
    golden_agc_aligned: Vec<[f32; 2]>,
    #[serde(default)]
    golden_agc_legacy_pre_gain_2_5: Vec<[f32; 2]>,
}

fn spec() -> Spec {
    // The file is compiled in and covered by tests; a malformed edit fails the build's tests.
    serde_json::from_str(SPEC_JSON).expect("resources/oww/dsp_spec.json is valid")
}

impl DspParams {
    /// Parameters for `profile`. `legacy` borrows the per-device calibration
    /// (high-pass cutoff, silence gate, pre-gain as the minimum gain) exactly as
    /// the runtime always did.
    pub fn resolve(profile: DspProfile, acoustic: &crate::acoustic_profile::AcousticProfile) -> Self {
        let s = spec();
        match profile {
            DspProfile::Aligned => DspParams {
                highpass_hz: s.aligned.highpass_hz,
                silence_rms: s.aligned.silence_rms,
                target_rms: s.aligned.target_rms,
                max_gain: s.aligned.max_gain,
                floor_gain: s.aligned.floor_gain,
            },
            DspProfile::Legacy => DspParams {
                highpass_hz: acoustic.highpass_cutoff_hz,
                silence_rms: acoustic.silence_rms_threshold,
                target_rms: s.legacy.target_rms,
                max_gain: s.legacy.max_gain,
                floor_gain: acoustic.pre_gain.max(1.0),
            },
        }
    }
}

/// Gain applied to a chunk of RMS `rms`: quiet speech is lifted toward
/// `target_rms` (capped at `max_gain`), nothing gets less than `floor_gain`.
/// Pure; the Python trainer implements the same formula.
pub fn agc_gain(rms: f32, p: &DspParams) -> f32 {
    if rms < p.target_rms {
        (p.target_rms / rms.max(1e-9)).min(p.max_gain).max(p.floor_gain)
    } else {
        p.floor_gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustic_profile::AcousticProfile;

    fn profile_with(pre_gain: f32) -> AcousticProfile {
        AcousticProfile { pre_gain, ..AcousticProfile::default() }
    }

    #[test]
    fn test_spec_parses_and_labels() {
        let _ = spec();
        assert_eq!(DspProfile::from_label("Aligned "), DspProfile::Aligned);
        assert_eq!(DspProfile::from_label("legacy"), DspProfile::Legacy);
        assert_eq!(DspProfile::from_label("nonsense"), DspProfile::Legacy);
        assert_eq!(DspProfile::from_label("aligned").label(), "aligned");
    }

    /// The same golden table is checked by scripts/test_wake_dsp.py against the
    /// Python implementation the trainer uses.
    #[test]
    fn test_agc_matches_golden_tables() {
        let s = spec();
        let a = DspParams::resolve(DspProfile::Aligned, &AcousticProfile::default());
        for [rms, want] in s.golden_agc_aligned {
            assert!((agc_gain(rms, &a) - want).abs() < 1e-3, "aligned rms {rms}");
        }
        let l = DspParams::resolve(DspProfile::Legacy, &profile_with(2.5));
        for [rms, want] in s.golden_agc_legacy_pre_gain_2_5 {
            assert!((agc_gain(rms, &l) - want).abs() < 1e-3, "legacy rms {rms}");
        }
    }

    /// `legacy` must reproduce the formula that was inline in detect_chunk.
    #[test]
    fn test_legacy_is_the_old_inline_formula() {
        for pre_gain in [1.0f32, 1.5, 2.5] {
            let p = DspParams::resolve(DspProfile::Legacy, &profile_with(pre_gain));
            for rms in [0.0006f32, 0.003, 0.01, 0.03, 0.0349, 0.035, 0.1, 0.4] {
                let pg = pre_gain.max(1.0);
                let old = if rms < 0.035 {
                    ((0.035 / rms).min(30.0)).max(pg)
                } else if pg > 1.0 {
                    pg
                } else {
                    1.0
                };
                assert!((agc_gain(rms, &p) - old).abs() < 1e-5, "pre_gain {pre_gain}, rms {rms}");
            }
        }
    }

    /// Aligned never amplifies speech that is already at target, unlike legacy.
    #[test]
    fn test_aligned_leaves_loud_speech_alone() {
        let a = DspParams::resolve(DspProfile::Aligned, &profile_with(2.5));
        assert_eq!(agc_gain(0.2, &a), 1.0);
        assert_eq!(a.highpass_hz, 80.0);
        let l = DspParams::resolve(DspProfile::Legacy, &profile_with(2.5));
        assert_eq!(agc_gain(0.2, &l), 2.5);
    }
}
