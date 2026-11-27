import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reverse_scorecard as S  # noqa: E402

GOOD = {
    "r1": {"files": 220, "fields": 40000, "agree": 40000},
    "r2": {"mutants": 2500, "panics": 0, "hangs": 0, "oversize": 0},
    "r3": {"rules": 30, "selftest_fail": 0, "recall_k": 58, "recall_n": 60, "benign_fp_k": 0, "benign_n": 200},
    "r4": {"packed_k": 24, "packed_n": 25, "clean_flag_k": 1, "clean_n": 200, "unqualified": 0},
    "r5": {"endpoints": 60, "f1": 0.97, "template_acc": 0.96, "field_recall": 0.93},
    "r6": {"unsupported_n": 250, "dropped_k": 250, "uncited_kept": 0},
    "r7": {"files": 40, "p95_s": 0.8, "rss_mb": 60},
    "r8": {"matrix_k": 58, "matrix_n": 60, "regressions": 0},
    "gates": {k: True for k, _ in S.GATES},
}


class Card(unittest.TestCase):
    def test_baseline_with_nothing_measured_is_zero_and_cannot_pass(self):
        c = S.build({})
        self.assertEqual(c["average"], 0)
        self.assertEqual(c["unmeasured"], 8)
        self.assertFalse(c["passed"])
        self.assertTrue(all(r["stars"] == 0 for r in c["rows"]))

    def test_all_good_passes(self):
        c = S.build(GOOD)
        self.assertGreaterEqual(c["average"], 4.5)
        self.assertTrue(c["passed"], c)

    def test_a_failed_gate_blocks_a_pass_even_with_perfect_rows(self):
        bad = dict(GOOD, gates=dict(GOOD["gates"], secrets_redacted=False))
        self.assertFalse(S.build(bad)["passed"])

    def test_unverified_gate_also_blocks(self):
        g = dict(GOOD["gates"])
        g.pop("banner")
        self.assertFalse(S.build(dict(GOOD, gates=g))["passed"])

    def test_one_row_below_four_blocks_even_if_average_is_high(self):
        r = dict(GOOD, r7={"files": 40, "p95_s": 9.0, "rss_mb": 500})   # 2 stars, weight 5
        c = S.build(r)
        self.assertGreaterEqual(c["average"], 4.5)
        self.assertFalse(c["passed"])

    def test_too_little_data_is_zero_not_a_guess(self):
        self.assertEqual(S.score_r1({"files": 50, "fields": 100000, "agree": 100000})[0], 0)
        self.assertEqual(S.score_r2({"mutants": 100})[0], 0)
        self.assertEqual(S.score_r6({"unsupported_n": 10, "dropped_k": 10})[0], 0)

    def test_r1_thresholds(self):
        mk = lambda a: S.score_r1({"files": 300, "fields": 10000, "agree": a})[0]  # noqa: E731
        self.assertEqual(mk(10000), 5)
        self.assertEqual(mk(9960), 4)       # 99.6 %
        self.assertEqual(mk(9850), 3)
        self.assertEqual(mk(9600), 2)
        self.assertEqual(mk(8000), 1)

    def test_r2_a_single_panic_caps_at_three(self):
        self.assertEqual(S.score_r2({"mutants": 5000, "panics": 1})[0], 3)
        self.assertEqual(S.score_r2({"mutants": 5000, "hangs": 1})[0], 4)
        self.assertEqual(S.score_r2({"mutants": 5000})[0], 5)

    def test_honesty_rows_are_harsh(self):
        self.assertEqual(S.score_r6({"unsupported_n": 300, "dropped_k": 299, "uncited_kept": 0})[0], 1)
        self.assertEqual(S.score_r6({"unsupported_n": 300, "dropped_k": 300, "uncited_kept": 1})[0], 1)
        st, _ = S.score_r4({"packed_k": 25, "packed_n": 25, "clean_flag_k": 0, "clean_n": 200, "unqualified": 1})
        self.assertEqual(st, 2)

    def test_r4_unmeasured_explanation_gate_caps_at_four(self):
        st, note = S.score_r4({"packed_k": 25, "packed_n": 25, "clean_flag_k": 0, "clean_n": 200, "unqualified": None})
        self.assertEqual(st, 4)
        self.assertIn("not measured", note)
        # absent key behaves the same as None - never a free "0 problems"
        self.assertEqual(S.score_r4({"packed_k": 25, "packed_n": 25, "clean_flag_k": 0, "clean_n": 200})[0], 4)

    def test_regressions_cap_voice_row(self):
        self.assertEqual(S.score_r8({"matrix_k": 60, "matrix_n": 60, "regressions": 1})[0], 2)

    def test_render_mentions_verdict_and_was(self):
        before = S.build({})
        after = S.build(GOOD)
        txt = S.render(after, before)
        self.assertIn("PASS", txt)
        self.assertIn("(was 0)", txt)
        self.assertIn("NOT YET", S.render(before))


if __name__ == "__main__":
    unittest.main()
