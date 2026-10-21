"""Tests for the wake label audit (stdlib unittest + numpy/scipy; no onnx, no audio)."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_label_audit as A  # noqa: E402
import wake_audition as W  # noqa: E402


def planted(seed=3, n=60, d=24, n_bad=5):
    """Two well separated classes + n_bad negatives that really are positives (and vice versa)."""
    rng = np.random.default_rng(seed)
    pos = rng.normal(2.0, 0.6, (n, d))
    neg = rng.normal(-2.0, 0.6, (n, d))
    X = np.vstack([pos, neg])
    y = np.array([1] * n + [0] * n)
    bad = list(range(n, n + n_bad)) + list(range(0, n_bad))        # first n_bad negatives, first n_bad positives
    # the "negatives" at indices n..n+n_bad are actually NEXUS-like; the "positives" 0..n_bad are noise-like
    X[n:n + n_bad] = rng.normal(2.0, 0.6, (n_bad, d))
    X[:n_bad] = rng.normal(-2.0, 0.6, (n_bad, d))
    return X, y, bad


class Provenance(unittest.TestCase):
    def test_known_prefixes(self):
        self.assertEqual(A.provenance("soundalike_from_pos_0007.wav"), "relabelled_from_positive")
        self.assertEqual(A.provenance("soundalike_from_rec_0001.wav"), "relabelled_from_recording")
        self.assertEqual(A.provenance("nexus_0820.wav"), "owner_mic")
        self.assertEqual(A.provenance("audition_0003.wav"), "by_ear")
        self.assertEqual(A.provenance("audit_0003.wav"), "by_ear")
        self.assertEqual(A.provenance("piper_libritts_nexus_0001.wav"), "tts_piper")
        self.assertEqual(A.provenance("gen_bg_0042.wav"), "dsp_or_tts_negative")
        self.assertEqual(A.provenance("who_knows.wav"), "unknown")


class OutOfFold(unittest.TestCase):
    def test_planted_mislabels_are_found_and_clean_clips_are_not(self):
        X, y, bad = planted()
        groups = np.arange(len(y))
        oof = A.oof_scores(X, y, groups, k=5, seed=1)
        rows = [{"sha": str(i), "path": f"c{i}.wav", "label": "positive" if y[i] else "negative",
                 "provenance": "owner_mic", "oof": float(oof[i])} for i in range(len(y))]
        flagged = {int(f["sha"]) for f in A.make_flags(rows)}
        self.assertTrue(set(bad) <= flagged, f"missed planted errors: {set(bad) - flagged}")
        self.assertLessEqual(len(flagged - set(bad)), 3, "too many false flags on clean clips")

    def test_a_clip_is_scored_without_its_duplicates_in_training(self):
        # 20 identical copies of one mislabeled clip: as ONE group they are held out together, so they flag
        rng = np.random.default_rng(0)
        X = np.vstack([rng.normal(2, .5, (40, 8)), rng.normal(-2, .5, (40, 8)), np.tile(rng.normal(2, .5, (1, 8)), (20, 1))])
        y = np.array([1] * 40 + [0] * 40 + [0] * 20)               # the 20 copies are labelled negative but sound positive
        groups = A.cluster_near_duplicates(X + rng.normal(0, 1e-4, X.shape))
        self.assertEqual(len(set(groups[80:].tolist())), 1)
        oof = A.oof_scores(X, y, groups, k=5, seed=2)
        self.assertGreater(float(np.nanmin(oof[80:])), 0.5)

    def test_ungrouped_duplicates_hide_the_error(self):
        # the same data with one group per clip: copies vote for each other - this is why groups matter
        rng = np.random.default_rng(0)
        X = np.vstack([rng.normal(2, .5, (40, 8)), rng.normal(-2, .5, (40, 8)), np.tile(rng.normal(2, .5, (1, 8)), (20, 1))])
        y = np.array([1] * 40 + [0] * 40 + [0] * 20)
        oof = A.oof_scores(X, y, np.arange(len(y)), k=5, seed=2)
        self.assertLess(float(np.nanmedian(oof[80:])), 0.5)

    def test_fold_without_both_classes_gives_nan_not_a_crash(self):
        X = np.random.default_rng(0).normal(size=(6, 4))
        out = A.oof_scores(X, np.zeros(6), np.arange(6), k=3)
        self.assertTrue(np.isnan(out).all())


class Flags(unittest.TestCase):
    def row(self, **kw):
        base = {"sha": "a", "path": "x.wav", "label": "negative", "provenance": "owner_mic", "oof": 0.1}
        base.update(kw)
        return base

    def test_suspect_origin_is_always_flagged_even_with_a_low_score(self):
        f = A.make_flags([self.row(provenance="relabelled_from_positive", oof=0.01)])
        self.assertEqual(f[0]["reasons"], ["suspect_origin"])

    def test_by_ear_clips_are_never_flagged(self):
        self.assertEqual(A.make_flags([self.row(provenance="by_ear", oof=0.99)]), [])

    def test_thresholds_are_inclusive_and_ordered_worst_first(self):
        rows = [self.row(sha="n1", oof=0.5), self.row(sha="n2", oof=0.9),
                self.row(sha="p1", label="positive", oof=0.2), self.row(sha="ok", oof=0.49)]
        ids = [f["sha"] for f in A.make_flags(rows)]
        self.assertEqual(ids, ["n2", "p1", "n1"])

    def test_short_or_silent_positive_is_flagged(self):
        f = A.make_flags([self.row(label="positive", oof=0.9, duration_s=0.2),
                          self.row(sha="b", label="positive", oof=0.9, rms=0.0001)])
        self.assertEqual({tuple(x["reasons"]) for x in f}, {("too_short",), ("near_silent",)})

    def test_missing_score_does_not_flag_by_score(self):
        self.assertEqual(A.make_flags([self.row(oof=None)]), [])


class LedgerAndGate(unittest.TestCase):
    def test_gate_blocks_until_every_flag_has_a_decision(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            ok, msg = A.gate(base)
            self.assertFalse(ok)
            self.assertIn("audit", msg)
            flags = [{"sha": "s1", "path": "negative/a.wav", "label": "negative", "provenance": "owner_mic",
                      "oof": 0.9, "severity": 0.9, "reasons": ["negative_sounds_like_nexus"]},
                     {"sha": "s2", "path": "negative/b.wav", "label": "negative", "provenance": "owner_mic",
                      "oof": 0.8, "severity": 0.8, "reasons": ["negative_sounds_like_nexus"]}]
            (base / A.FLAGS_NAME).write_text("".join(json.dumps(f) + "\n" for f in flags))
            self.assertFalse(A.gate(base)[0])
            A.record_decision(base / A.LEDGER_NAME, "s1", "negative")
            self.assertFalse(A.gate(base)[0])
            A.record_decision(base / A.LEDGER_NAME, "s2", "exclude")
            self.assertTrue(A.gate(base)[0])

    def test_last_decision_wins(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "l.jsonl"
            A.record_decision(p, "s", "negative")
            A.record_decision(p, "s", "positive")
            self.assertEqual(A.load_ledger(p), {"s": "positive"})

    def test_bad_decision_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(AssertionError):
                A.record_decision(Path(d) / "l", "s", "maybe")


class QueueMoves(unittest.TestCase):
    def test_destinations(self):
        b = Path("/b")
        self.assertIsNone(W.queue_decision_dest("negative", "keep", b))
        self.assertIsNone(W.queue_decision_dest("negative", "negative", b))
        self.assertEqual(W.queue_decision_dest("negative", "positive", b), b / "positive")
        self.assertEqual(W.queue_decision_dest("positive", "exclude", b), b / "quarantine")


class QueueReview(unittest.TestCase):
    def test_queue_review_moves_files_records_decisions_and_clears_the_gate(self):
        import builtins
        from unittest import mock
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            tone = [int(8000 * np.sin(2 * np.pi * (200 + 40 * i) * j / 16000)) for i in range(4) for j in range(1)]
            names = {"negative/soundalike_from_pos_0001.wav": "positive",     # really NEXUS -> becomes a positive
                     "negative/soundalike_from_pos_0002.wav": "negative",     # real soundalike -> stays (hard negative)
                     "positive/nexus_0003.wav": "exclude",                    # garbage take -> quarantine
                     "negative/neg_0004.wav": "keep"}
            flags = []
            for i, rel in enumerate(names):
                W.write_wav(base / rel, [(i + 1) * 100] * 8000)
                label = rel.split("/")[0]
                flags.append({"sha": A.file_sha(base / rel), "path": rel, "label": label, "provenance": A.provenance(rel),
                              "oof": 0.9 - i * 0.1, "severity": 0.9 - i * 0.1, "reasons": ["x"]})
            (base / A.FLAGS_NAME).write_text("".join(json.dumps(f) + "\n" for f in flags))
            answers = iter(["p", "n", "x", "k"])
            args = type("A", (), {"queue": str(base / A.FLAGS_NAME)})()
            with mock.patch.object(W, "_play", lambda p: None), mock.patch.object(builtins, "input", lambda _="": next(answers)):
                self.assertEqual(W.cmd_review_queue(args), 0)
            self.assertTrue(list((base / "positive").glob("audit_*.wav")), "relabelled clip lands in positive/")
            self.assertTrue((base / "negative/soundalike_from_pos_0002.wav").exists(), "same label: stays put")
            self.assertTrue(list((base / "quarantine").glob("audit_*.wav")))
            self.assertTrue((base / "negative/neg_0004.wav").exists())
            self.assertFalse((base / "negative/soundalike_from_pos_0001.wav").exists())
            ledger = A.load_ledger(base / A.LEDGER_NAME)
            self.assertEqual(sorted(ledger.values()), ["exclude", "keep", "negative", "positive"])
            self.assertTrue(A.gate(base)[0])


class Inventory(unittest.TestCase):
    def test_inventory_counts_duplicates_and_suspects(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            sr = 16000
            tone = [int(8000 * np.sin(2 * np.pi * 300 * i / sr)) for i in range(sr)]
            for rel in ("positive/nexus_0001.wav", "positive/nexus_0002.wav", "negative/soundalike_from_pos_0001.wav"):
                W.write_wav(base / rel, tone, sr)
            W.write_wav(base / "positive/nexus_0003.wav", [0] * 1000, sr)      # short + silent
            A.main(["--base", str(base), "inventory"])
            rep = json.loads((base / A.INVENTORY_NAME).read_text())
            self.assertEqual(rep["counts"]["positive"]["owner_mic"], 3)
            self.assertEqual(len(rep["exact_duplicates"]), 1)                      # 3 identical tones = 1 group
            self.assertEqual(rep["suspect_negatives"], ["negative/soundalike_from_pos_0001.wav"])
            self.assertEqual(rep["too_short_positive"], ["positive/nexus_0003.wav"])
            self.assertEqual(rep["near_silent_positive"], ["positive/nexus_0003.wav"])


if __name__ == "__main__":
    unittest.main()
