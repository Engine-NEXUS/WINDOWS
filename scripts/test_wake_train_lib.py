#!/usr/bin/env python3
"""Tests for wake_train_lib.py and the trainer's --dry-run path (numpy/scipy/onnxruntime; no torch).

    python -m unittest scripts/test_wake_train_lib.py -v
"""

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))

import wake_train_lib as L  # noqa: E402
from scipy.io import wavfile  # noqa: E402

SCRIPTS = Path(__file__).resolve().parent


def burst(seconds=1.0, amp=0.1, f0=180.0, sr=16000, lead=0.2, tail=0.3):
    """Voiced-looking test signal: a harmonic stack with a syllable-rate envelope, padded with silence."""
    n = int(seconds * sr)
    t = np.arange(n) / sr
    env = 0.5 * (1 + np.sin(2 * np.pi * 4 * t))
    sig = sum(np.sin(2 * np.pi * f0 * k * t) / k for k in range(1, 8)) * env
    sig = (amp * sig / np.max(np.abs(sig))).astype(np.float32)
    return np.concatenate([np.zeros(int(lead * sr), np.float32), sig, np.zeros(int(tail * sr), np.float32)])


class SplitTests(unittest.TestCase):
    def test_splits_are_deterministic_stable_and_roughly_70_15_15(self):
        groups = [f"g{i}" for i in range(3000)]
        a = L.assign_splits(groups, seed=1)
        self.assertEqual(a, L.assign_splits(reversed(groups), seed=1))
        share = {s: sum(v == s for v in a.values()) / len(a) for s in ("train", "val", "test")}
        self.assertAlmostEqual(share["train"], 0.70, delta=0.03)
        self.assertAlmostEqual(share["val"], 0.15, delta=0.03)
        self.assertAlmostEqual(share["test"], 0.15, delta=0.03)
        # Adding groups never moves existing ones.
        b = L.assign_splits(groups + ["new1", "new2"], seed=1)
        self.assertTrue(all(b[g] == a[g] for g in groups))
        # A different seed gives a different assignment.
        self.assertNotEqual(a, L.assign_splits(groups, seed=2))

    def test_group_key_uses_manifest_source_group_then_speaker_session(self):
        rows = {"a.wav": {"source_group_id": "owner-s1"}, "b.wav": {"speaker_id": "x", "session_id": "s9"}}
        self.assertEqual(L.group_key("/d/a.wav", rows), "owner-s1")
        self.assertEqual(L.group_key("/d/b.wav", rows), "x|s9")
        self.assertEqual(L.group_key("/d/c.wav", rows), "c.wav")
        self.assertEqual(L.group_key("/d/c.wav", None), "c.wav")

    def test_load_manifest_rows(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "m.jsonl"
            p.write_text(json.dumps({"audio": "positive/a.wav", "source_group_id": "g"}) + "\n", encoding="utf-8")
            rows = L.load_manifest_rows(p)
            self.assertEqual(rows["a.wav"]["source_group_id"], "g")
            self.assertEqual(L.load_manifest_rows(Path(tmp) / "missing.jsonl"), {})


class AudioTests(unittest.TestCase):
    def test_resample_length_and_antialias(self):
        sr = 44100
        t = np.arange(sr) / sr
        # 1 kHz survives, 10 kHz (above the 8 kHz Nyquist of 16 kHz) is removed, not aliased.
        low = L.to_mono_16k((np.sin(2 * np.pi * 1000 * t) * 16000).astype(np.int16), sr)
        high = L.to_mono_16k((np.sin(2 * np.pi * 10000 * t) * 16000).astype(np.int16), sr)
        self.assertAlmostEqual(len(low), 16000, delta=2)
        self.assertGreater(np.sqrt(np.mean(low ** 2)), 0.3)
        self.assertLess(np.sqrt(np.mean(high[1000:-1000] ** 2)), 0.02)

    def test_stereo_is_averaged(self):
        x = np.stack([np.full(1600, 8000, np.int16), np.full(1600, -8000, np.int16)], axis=1)
        self.assertLess(np.max(np.abs(L.to_mono_16k(x, 16000))), 1e-6)


class AugmentTests(unittest.TestCase):
    def setUp(self):
        self.x = burst()

    def test_seeded_augmentations_repeat_and_vary(self):
        for fn in (L.aug_gain, L.aug_shift, L.aug_speed, L.aug_reverb):
            a = fn(self.x, np.random.default_rng(5))
            b = fn(self.x, np.random.default_rng(5))
            c = fn(self.x, np.random.default_rng(6))
            self.assertTrue(np.array_equal(a, b), fn.__name__)
            self.assertTrue(np.all(np.isfinite(a)) and np.max(np.abs(a)) <= 1.0 + 1e-6, fn.__name__)
            self.assertFalse(len(a) == len(c) and np.array_equal(a, c), f"{fn.__name__} should vary with the seed")

    def test_speed_changes_length_within_8_percent(self):
        for s in range(10):
            y = L.aug_speed(self.x, np.random.default_rng(s))
            self.assertLess(abs(len(y) / len(self.x) - 1.0), 0.09)

    def test_mix_noise_hits_the_requested_snr(self):
        rng = np.random.default_rng(0)
        noise = rng.standard_normal(48000).astype(np.float32) * 0.05
        x = burst(amp=0.05)
        for snr in (0.0, 10.0, 20.0):
            y = L.mix_noise(x, noise, snr, np.random.default_rng(1))
            resid = y - x
            got = 10 * np.log10(np.mean(x ** 2) / np.mean(resid ** 2))
            self.assertAlmostEqual(got, snr, delta=0.6)

    def test_rates_at(self):
        r, fa = L.rates_at([0.9, 0.7, 0.4, 0.8], [0.1, 0.7, 0.2, 0.69], 0.68)
        self.assertEqual((r, fa), (0.75, 0.5))
        r, fa = L.rates_at([], [0.1], 0.5)
        self.assertTrue(np.isnan(r) and fa == 0.0)


class FrontendTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import onnxruntime as ort
        cls.fe = L.Frontend(ort.InferenceSession(str(L.OWW_DIR / "melspectrogram.onnx")),
                            ort.InferenceSession(str(L.OWW_DIR / "embedding_model.onnx")))

    def test_silence_yields_no_windows_and_speech_does(self):
        self.assertEqual(self.fe.windows(np.zeros(16000, np.float32), include_transitions=True), [])
        w = self.fe.windows(burst(), include_transitions=True)
        self.assertGreater(len(w), 3)
        self.assertEqual(w[0].shape, (16, 96))
        self.assertTrue(all(np.all(np.isfinite(x)) for x in w))

    def test_transitions_add_at_most_two_windows(self):
        sig = burst()
        a = self.fe.windows(sig, include_transitions=False)
        b = self.fe.windows(sig, include_transitions=True)
        self.assertTrue(0 <= len(b) - len(a) <= 2)

    def test_agc_makes_quiet_and_normal_speech_comparable(self):
        # Quiet speech is lifted toward the target RMS, so its features resemble normal-level speech.
        loud = self.fe.windows(burst(amp=0.06), True)[-1].reshape(-1)
        quiet = self.fe.windows(burst(amp=0.012), True)[-1].reshape(-1)
        corr = float(np.corrcoef(loud, quiet)[0, 1])
        self.assertGreater(corr, 0.8, f"AGC should align levels (corr {corr:.2f})")

    def test_highpass_matches_the_one_pole_recurrence(self):
        x = np.random.default_rng(0).standard_normal(2000).astype(np.float32) * 0.1
        y = self.fe.highpass(x, 80.0)
        rc = 1 / (2 * np.pi * 80.0)
        alpha = rc / (rc + 1 / 16000)
        py = px = 0.0
        for i in (0, 1, 2, 3):
            py = alpha * (py + x[i] - px)
            px = x[i]
            self.assertAlmostEqual(float(y[i]), py, places=5)


class TrainerDryRunTests(unittest.TestCase):
    def test_dry_run_splits_by_recording_and_writes_report(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            for sub in ("positive", "negative", "background"):
                (d / sub).mkdir()
            rng = np.random.default_rng(0)
            for i in range(60):
                wavfile.write(str(d / "positive" / f"nexus_{i:04d}.wav"), 16000,
                              (burst(0.5, f0=150 + 3 * i) * 32767).astype(np.int16))
            for i in range(20):
                wavfile.write(str(d / "negative" / f"neg_{i:04d}.wav"), 16000,
                              (burst(0.5, f0=300 + 5 * i) * 32767).astype(np.int16))
            for i in range(6):
                wavfile.write(str(d / "background" / f"bg_{i:04d}.wav"), 16000,
                              (rng.standard_normal(16000) * 0.01 * 32767).astype(np.int16))
            out = d / "cand.onnx"
            res = subprocess.run([sys.executable, str(SCRIPTS / "train_local_wakeword.py"), "--dry-run",
                                  "--data-dir", str(d), "--out", str(out), "--seed", "7",
                                  "--seal-owner", "0"],
                                 capture_output=True, text=True, timeout=900)
            self.assertEqual(res.returncode, 0, res.stdout[-800:] + res.stderr[-800:])
            rep = json.loads((d / "train_report.dry.json").read_text())
            rec = rep["recordings"]["positive"]
            self.assertEqual(sum(rec.values()), 60, "every recording lands in exactly one split")
            self.assertGreater(rec["train"], rec["val"])
            for sp in ("train", "val", "test"):
                self.assertGreater(rep["windows"][sp]["pos"] + rep["windows"][sp]["owner_pos"], 0, sp)
            # Val/test are clean recordings only; train is augmented (many more windows per file).
            per_file_train = (rep["windows"]["train"]["pos"] + rep["windows"]["train"]["owner_pos"]) / rec["train"]
            per_file_val = (rep["windows"]["val"]["pos"] + rep["windows"]["val"]["owner_pos"]) / max(rec["val"], 1)
            self.assertGreater(per_file_train, 2 * per_file_val)
            self.assertFalse(out.exists(), "a dry run must not write a model")


class TrainerPhaseETests(unittest.TestCase):
    def make_data(self, d, n_pos=60):
        for sub in ("positive", "negative", "background"):
            (d / sub).mkdir(exist_ok=True)
        rng = np.random.default_rng(0)
        for i in range(n_pos):
            wavfile.write(str(d / "positive" / f"nexus_{i:04d}.wav"), 16000, (burst(0.5, f0=150 + 3 * i) * 32767).astype(np.int16))
        for i in range(20):
            wavfile.write(str(d / "negative" / f"neg_{i:04d}.wav"), 16000, (burst(0.5, f0=300 + 5 * i) * 32767).astype(np.int16))
        for i in range(6):
            wavfile.write(str(d / "background" / f"bg_{i:04d}.wav"), 16000, (rng.standard_normal(16000) * 0.01 * 32767).astype(np.int16))

    def run_trainer(self, d, *extra):
        return subprocess.run([sys.executable, str(SCRIPTS / "train_local_wakeword.py"), "--dry-run", "--data-dir", str(d),
                               "--out", str(d / "cand.onnx"), "--seed", "7", *extra],
                              capture_output=True, text=True, timeout=900)

    def test_sealed_owner_clips_are_withheld_and_listed(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            self.make_data(d)
            res = self.run_trainer(d, "--seal-owner", "0.3")
            self.assertEqual(res.returncode, 0, res.stdout[-600:] + res.stderr[-600:])
            rep = json.loads((d / "train_report.dry.json").read_text())
            sealed = [json.loads(l) for l in (d / "sealed" / "manifest.jsonl").read_text().splitlines()]
            self.assertEqual(len(sealed), rep["sealed_owner_recordings"])
            self.assertGreater(len(sealed), 5)
            self.assertEqual(sum(rep["recordings"]["positive"].values()) + len(sealed), 60)
            self.assertTrue(all(r["kind"] == "wake" and r["audio"].startswith("positive/") for r in sealed))
            # sealing is stable: the same files are sealed on the next run
            first = [r["audio"] for r in sealed]
            self.run_trainer(d, "--seal-owner", "0.3")
            again = [json.loads(l)["audio"] for l in (d / "sealed" / "manifest.jsonl").read_text().splitlines()]
            self.assertEqual(first, again)

    def test_audit_gate_blocks_a_real_run_but_only_warns_on_dry_run(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = Path(tmp)
            self.make_data(d)
            (d / "audit_flags.jsonl").write_text(json.dumps(
                {"sha": "x", "path": "negative/neg_0000.wav", "label": "negative", "provenance": "owner_mic",
                 "oof": 0.9, "severity": 0.9, "reasons": ["negative_sounds_like_nexus"]}) + "\n")
            dry = self.run_trainer(d, "--seal-owner", "0")
            self.assertEqual(dry.returncode, 0)
            self.assertIn("AUDIT GATE FAILED", dry.stdout)
            rep = json.loads((d / "train_report.dry.json").read_text())
            self.assertFalse(rep["audit_gate"]["ok"])
            real = subprocess.run([sys.executable, str(SCRIPTS / "train_local_wakeword.py"), "--data-dir", str(d),
                                   "--out", str(d / "cand.onnx")], capture_output=True, text=True, timeout=900)
            self.assertNotEqual(real.returncode, 0)
            self.assertIn("Refusing to train on unaudited labels", real.stderr + real.stdout)

    def test_synth_and_public_clips_use_lighter_augmentation_than_owner_clips(self):
        import onnxruntime as ort
        mel = ort.InferenceSession(str(L.OWW_DIR / "melspectrogram.onnx"))
        emb = ort.InferenceSession(str(L.OWW_DIR / "embedding_model.onnx"))
        fe = L.Frontend(mel, emb)
        sys.path.insert(0, str(SCRIPTS))
        import train_local_wakeword as T
        a = burst(0.6)
        full = T.positives_from(fe, a, np.random.default_rng(1), [], train=True, level="full")
        lite = T.positives_from(fe, a, np.random.default_rng(1), [], train=True, level="lite")
        clean = T.positives_from(fe, a, np.random.default_rng(1), [], train=False)
        self.assertGreater(len(full), len(lite))
        self.assertGreater(len(lite), len(clean))


class LibHelpers(unittest.TestCase):
    def test_sealed_fraction_is_stable_and_roughly_right(self):
        gs = [f"g{i}" for i in range(2000)]
        s = [L.is_sealed(g, 0.3) for g in gs]
        self.assertEqual(s, [L.is_sealed(g, 0.3) for g in gs])
        self.assertAlmostEqual(sum(s) / len(s), 0.3, delta=0.04)
        self.assertFalse(any(L.is_sealed(g, 0.0) for g in gs))

    def test_sealing_is_independent_of_the_split_hash(self):
        gs = [f"g{i}" for i in range(3000)]
        sp = L.assign_splits(gs)
        sealed = [g for g in gs if L.is_sealed(g, 0.3)]
        frac_test = sum(sp[g] == "test" for g in sealed) / len(sealed)
        self.assertAlmostEqual(frac_test, 0.15, delta=0.05)

    def test_cap_list(self):
        rng = np.random.default_rng(0)
        self.assertEqual(L.cap_list([1, 2, 3], 5, rng), [1, 2, 3])
        out = L.cap_list(list(range(100)), 10, rng)
        self.assertEqual(len(out), 10)
        self.assertEqual(out, sorted(out))

    def test_hard_negative_indices(self):
        p = [0.1, 0.9, 0.25, 0.5, 0.19]
        self.assertEqual(list(L.hard_negative_indices(p, 0.2, 10)), [1, 3, 2])
        self.assertEqual(list(L.hard_negative_indices(p, 0.2, 2)), [1, 3])
        self.assertEqual(len(L.hard_negative_indices([0.0, 0.1], 0.2, 5)), 0)


if __name__ == "__main__":
    unittest.main()
