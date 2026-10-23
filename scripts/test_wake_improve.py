import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_improve as W  # noqa: E402


def m(key, label, point, n, kind="rate", direction="high"):
    return {"key": key, "label": label, "kind": kind, "point": point, "n": n, "direction": direction, "target": 0.95}


class Verdict(unittest.TestCase):
    def status(self, before, after):
        return W.compare([before], [after])[0]["status"]

    def test_rate_rows_use_one_clip_of_slack(self):
        self.assertEqual(self.status(m("a", "x", 0.90, 100), m("a", "x", 0.95, 100)), "improved")
        self.assertEqual(self.status(m("a", "x", 0.90, 100), m("a", "x", 0.905, 100)), "same")      # < 1 clip
        self.assertEqual(self.status(m("a", "x", 0.95, 100), m("a", "x", 0.90, 100)), "regressed")

    def test_lower_is_better_rows(self):
        a = dict(direction="low", kind="perhour")
        self.assertEqual(self.status(m("fa", "fa", 2.0, 3, **a), m("fa", "fa", 0.5, 3, **a)), "improved")
        self.assertEqual(self.status(m("fa", "fa", 0.5, 3, **a), m("fa", "fa", 2.0, 3, **a)), "regressed")

    def test_false_alarms_per_hour_need_an_hour_of_audio(self):
        a = dict(direction="low", kind="perhour")
        self.assertEqual(self.status(m("fa", "fa", 0.0, 0.04, **a), m("fa", "fa", 692.0, 0.04, **a)), "inconclusive")
        self.assertEqual(self.status(m("fa", "fa", 0.0, 5.0, **a), m("fa", "fa", 3.0, 5.0, **a)), "regressed")

    def test_too_few_clips_is_not_evidence(self):
        self.assertEqual(self.status(m("a", "x", 0.5, 5), m("a", "x", 1.0, 5)), "inconclusive")

    def test_unmeasured_and_optimistic_rows(self):
        self.assertEqual(self.status(m("a", "x", None, 0), m("a", "x", 0.9, 100)), "new")
        self.assertEqual(self.status(m("a", "x", 0.9, 100), m("a", "x", None, 0)), "inconclusive")
        self.assertEqual(self.status(m("wake_owner_sealed", "s", 0.99, 100), m("wake_owner_sealed", "s", 0.5, 100)), "optimistic")

    def test_verdicts(self):
        r = lambda *s: [{"status": x} for x in s]  # noqa: E731
        self.assertEqual(W.verdict(r()), "NO_DATA")
        self.assertEqual(W.verdict(r("inconclusive", "optimistic", "new")), "NO_DATA")
        self.assertEqual(W.verdict(r("same", "same")), "SAME")
        self.assertEqual(W.verdict(r("improved", "same")), "IMPROVED")
        self.assertEqual(W.verdict(r("improved", "regressed")), "REGRESSED")
        self.assertEqual(W.verdict(r("optimistic", "improved")), "IMPROVED")

    def test_optimistic_row_can_never_cause_or_hide_a_verdict(self):
        rows = W.compare([m("wake_owner_sealed", "s", 0.99, 100), m("a", "x", 0.9, 100)],
                         [m("wake_owner_sealed", "s", 0.10, 100), m("a", "x", 0.9, 100)])
        self.assertEqual(W.verdict(rows), "SAME")

    def test_report_text(self):
        rows = W.compare([m("a", "Recall", 0.80, 100)], [m("a", "Recall", 0.95, 100)])
        txt = W.render_report(rows, "IMPROVED", {"missing": ["x"], "notes": ["y"]})
        self.assertIn("Verdict: IMPROVED", txt)
        self.assertIn("80.0% (n=100) | 95.0% (n=100) | better", txt)
        self.assertIn("## Missing", txt)
        self.assertIn("What this does NOT tell you", txt)


class Flow(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.data, self.oww, self.report = t / "data", t / "oww", t / "docs" / "improvement-report.md"
        (self.data / "positive").mkdir(parents=True)
        (self.data / "positive" / "nexus_0001.wav").write_bytes(b"RIFF")
        self.oww.mkdir()
        (self.oww / "nexus.onnx").write_bytes(b"old")
        self.calls = []
        self.review_pending = True
        self.after_points = (0.80, 0.95)

    def tearDown(self):
        self.tmp.cleanup()

    def args(self, **kw):
        base = dict(skip_fetch=True, skip_synth=True, max_gb="1", max_hours="1", per_phrase="1", epochs=None,
                    allow_unreviewed=False, promote=False, redo=False)
        base.update(kw)
        return SimpleNamespace(**base)

    def fake(self, argv):
        self.calls.append(argv)
        name = Path(argv[1]).name
        if name == "wake_label_audit.py" and argv[2] == "status":
            return 1 if self.review_pending else 0
        if name == "train_local_wakeword.py" and "--dry-run" not in argv:
            (self.oww / "nexus.candidate.onnx").write_bytes(b"new")
        if name == "wake_score.py":
            out = Path(argv[argv.index("--json") + 1])
            pt = self.after_points[1] if "--model" in argv else self.after_points[0]
            out.write_text(json.dumps({"metrics": [m("wake_recall", '"NEXUS" recall', pt, 100)]}))
        return 0

    def run_flow(self, **kw):
        return W.run_all(self.args(**kw), run=self.fake, data=self.data, report_path=self.report, oww=self.oww)

    def names(self):
        return [(Path(c[1]).name, c[2] if len(c) > 2 else "") for c in self.calls]

    def test_stops_for_the_by_ear_review_and_does_not_train(self):
        self.assertEqual(self.run_flow(), W.EXIT_NEEDS_REVIEW)
        self.assertFalse(any(n == "train_local_wakeword.py" and "--dry-run" not in c for (n, _), c in zip(self.names(), self.calls)))
        self.assertFalse(self.report.exists())

    def test_full_flow_after_review_reports_improvement_and_promotes_only_then(self):
        self.review_pending = False
        self.assertEqual(self.run_flow(promote=True), 0)
        txt = self.report.read_text()
        self.assertIn("Verdict: IMPROVED", txt)
        self.assertEqual((self.oww / "nexus.onnx").read_bytes(), b"new")
        self.assertEqual((self.oww / "nexus.previous.onnx").read_bytes(), b"old")
        order = [n for n, _ in self.names()]
        # the shipped model is scored BEFORE the candidate is trained
        first_score = order.index("wake_score.py")
        last_train = max(i for i, c in enumerate(self.calls) if Path(c[1]).name == "train_local_wakeword.py" and "--dry-run" not in c)
        self.assertLess(first_score, last_train)

    def test_regression_is_reported_and_never_promoted(self):
        self.review_pending = False
        self.after_points = (0.95, 0.80)
        self.assertEqual(self.run_flow(promote=True), 2)
        self.assertIn("Verdict: REGRESSED", self.report.read_text())
        self.assertEqual((self.oww / "nexus.onnx").read_bytes(), b"old")

    def test_successful_unpersisted_stage_does_not_print_a_failure(self):
        import contextlib
        import io
        self.review_pending = False
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            self.run_flow()
        self.assertNotIn("failed", buf.getvalue())

    def test_allow_unreviewed_trains_but_says_so(self):
        self.assertEqual(self.run_flow(allow_unreviewed=True), 0)
        self.assertIn("UNREVIEWED", self.report.read_text())
        self.assertTrue(any("--skip-audit-gate" in c for c in self.calls if "--dry-run" not in c))

    def test_no_recordings_and_no_test_audio_gives_no_data_not_a_number(self):
        for f in (self.data / "positive").glob("*"):
            f.unlink()
        self.review_pending = False
        failing = lambda argv: 1 if Path(argv[1]).name == "wake_score.py" else self.fake(argv)  # noqa: E731
        self.assertEqual(W.run_all(self.args(), run=failing, data=self.data, report_path=self.report, oww=self.oww), 0)
        txt = self.report.read_text()
        self.assertIn("Verdict: NO_DATA", txt)
        self.assertIn("No positive recordings", txt)


if __name__ == "__main__":
    unittest.main()
