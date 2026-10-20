#!/usr/bin/env python3
"""Cross-language check: the trainer's AGC equals the Rust runtime's, via the golden tables in
dsp_spec.json (the Rust side asserts the same tables in wake_dsp::tests).

    python -m unittest scripts/test_wake_dsp.py -v
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import wake_dsp  # noqa: E402


class WakeDspTests(unittest.TestCase):
    def test_aligned_golden_table(self):
        spec = wake_dsp.load_spec()
        p = wake_dsp.aligned_params(spec)
        for rms, want in spec["golden_agc_aligned"]:
            self.assertAlmostEqual(wake_dsp.agc_gain(rms, p), want, places=3, msg=f"rms {rms}")

    def test_legacy_golden_table_with_pre_gain(self):
        spec = wake_dsp.load_spec()
        p = {"target_rms": spec["legacy"]["target_rms"], "max_gain": spec["legacy"]["max_gain"],
             "floor_gain": 2.5}
        for rms, want in spec["golden_agc_legacy_pre_gain_2_5"]:
            self.assertAlmostEqual(wake_dsp.agc_gain(rms, p), want, places=3, msg=f"rms {rms}")

    def test_spec_has_what_the_trainer_needs(self):
        p = wake_dsp.aligned_params()
        for k in ("highpass_hz", "silence_rms", "target_rms", "max_gain", "floor_gain"):
            self.assertIn(k, p)
        self.assertEqual(p["floor_gain"], 1.0, "trainer never boosts speech that is already at target")


if __name__ == "__main__":
    unittest.main()
