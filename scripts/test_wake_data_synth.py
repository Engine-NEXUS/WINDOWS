"""Tests for the synthetic-data planner (no Piper, no network)."""
import json
import sys
import tempfile
import unittest
import wave
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_data_synth as S  # noqa: E402


class Licence(unittest.TestCase):
    def test_allowed(self):
        for t in ("* License: CC BY 4.0", "License: CC0", "licence: Public Domain", "License: MIT", "Licensed under Apache-2.0",
                  "License: CC-BY-4.0"):
            self.assertTrue(S.license_ok(t)[0], t)

    def test_refused(self):
        for t in ("License: CC BY-NC 4.0", "License: CC BY-SA 4.0", "License: CC BY-NC-SA", "License: CC BY-ND 4.0",
                  "License: for research use only", "License: all rights reserved", "License: Blizzard 2013"):
            self.assertFalse(S.license_ok(t)[0], t)

    def test_missing_licence_is_refused_unless_forced(self):
        self.assertFalse(S.license_ok("# Model card\nno such line")[0])
        self.assertTrue(S.license_ok("# Model card\nno such line", allow_unknown=True)[0])

    def test_restriction_beats_permission(self):
        self.assertFalse(S.license_ok("License: CC BY 4.0\nLicense: non-commercial only")[0])


class Effective(unittest.TestCase):
    def test_see_url_card_falls_back_to_the_vetted_dataset_licence(self):
        ok, text, src = S.effective_license("en_US-libritts_r-medium", "* License: See URL")
        self.assertEqual((ok, src), (True, "dataset"))
        self.assertIn("CC BY 4.0", text)

    def test_unvetted_voice_with_see_url_is_skipped(self):
        self.assertFalse(S.effective_license("en_US-amy-medium", "* License: See URL")[0])

    def test_restricted_card_is_never_overridden_by_the_table(self):
        self.assertFalse(S.effective_license("en_US-libritts_r-medium", "License: CC BY-NC 4.0")[0])

    def test_clear_card_wins(self):
        self.assertEqual(S.effective_license("en_US-amy-medium", "License: CC0")[2], "card")


class Plan(unittest.TestCase):
    def test_deterministic_and_covers_voices_and_speakers(self):
        a = S.plan_jobs({"v1": 10, "v2": 1}, S.POSITIVE_PHRASES, 100, seed=5)
        b = S.plan_jobs({"v1": 10, "v2": 1}, S.POSITIVE_PHRASES, 100, seed=5)
        self.assertEqual(a, b)
        self.assertNotEqual(a, S.plan_jobs({"v1": 10, "v2": 1}, S.POSITIVE_PHRASES, 100, seed=6))
        self.assertEqual({j["voice"] for j in a}, {"v1", "v2"})
        self.assertGreater(len({j["speaker"] for j in a if j["voice"] == "v1"}), 5)
        self.assertEqual({j["speaker"] for j in a if j["voice"] == "v2"}, {0})

    def test_weights_shift_the_mix(self):
        jobs = S.plan_jobs({"v": 1}, S.POSITIVE_PHRASES, 100, seed=1)
        n = {p: sum(j["phrase"] == p for j in jobs) for p in S.POSITIVE_PHRASES}
        self.assertGreater(n["hey nexus"], n["hello nexus"])
        self.assertAlmostEqual(len(jobs), 100 * len(S.POSITIVE_PHRASES), delta=len(S.POSITIVE_PHRASES))

    def test_list_phrases_get_per_phrase_each(self):
        jobs = S.plan_jobs({"v": 1}, ["next", "texas"], 7, seed=1)
        self.assertEqual(len(jobs), 14)

    def test_parameters_within_ranges(self):
        for j in S.plan_jobs({"v": 3}, ["x"], 200, seed=2):
            self.assertTrue(0.8 <= j["length_scale"] <= 1.35)
            self.assertTrue(-14 <= j["gain_db"] <= -2)
            self.assertTrue(0.05 <= j["lead_s"] <= 0.5 and 0.2 <= j["tail_s"] <= 0.9)

    def test_no_voices_no_jobs(self):
        self.assertEqual(S.plan_jobs({}, ["x"], 5, 1), [])


class Lists(unittest.TestCase):
    def test_no_positive_phrase_is_also_an_adversarial_or_borderline_text(self):
        pos = set(S.POSITIVE_PHRASES)
        self.assertFalse(pos & set(S.ADVERSARIAL))
        self.assertFalse(pos & set(S.BORDERLINE))

    def test_adversarial_never_contains_the_wake_word_itself(self):
        for p in S.ADVERSARIAL:
            self.assertNotIn("nexus", p.replace("plexus", ""), p)

    def test_slug(self):
        self.assertEqual(S.slug("Hey, NEXUS!"), "hey-nexus")


class Audio(unittest.TestCase):
    def test_postprocess_pads_and_normalises(self):
        job = {"gain_db": -6.0, "lead_s": 0.2, "tail_s": 0.5}
        tone = (0.9 * np.sin(2 * np.pi * 440 * np.arange(22050) / 22050)).astype(np.float32)
        y = S.postprocess(tone, job, 22050)
        self.assertEqual(y.dtype, np.float32)
        self.assertAlmostEqual(len(y) / 16000, 1.0 + 0.2 + 0.5, delta=0.01)
        self.assertAlmostEqual(float(np.abs(y).max()), 10 ** (-6 / 20), delta=0.03)
        self.assertEqual(float(np.abs(y[:3000]).max()), 0.0)

    def test_write_wav_roundtrip(self):
        with tempfile.TemporaryDirectory() as d:
            S.write_wav(Path(d) / "a.wav", np.zeros(1600, dtype=np.float32))
            with wave.open(str(Path(d) / "a.wav")) as w:
                self.assertEqual((w.getframerate(), w.getnchannels(), w.getnframes()), (16000, 1, 1600))

    def test_manifest_row_groups_by_speaker(self):
        r = S.manifest_row("positive/x.wav", {"voice": "v", "speaker": 3, "phrase": "nexus"}, "positive")
        self.assertEqual(r["source_group_id"], "v#3")
        json.dumps(r)


if __name__ == "__main__":
    unittest.main()
