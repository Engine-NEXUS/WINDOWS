import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reverse_har_bench as B  # noqa: E402


class Helpers(unittest.TestCase):
    def test_norm_collapses_param_names(self):
        self.assertEqual(B.norm("/orgs/{org}/repos/{repo}"), "/orgs/{}/repos/{}")
        self.assertEqual(B.norm("/a/b"), "/a/b")

    def test_template_regex_matches_one_segment_per_param(self):
        rx = B.tmpl_regex("/users/{id}/orders")
        self.assertTrue(rx.match("/users/42/orders"))
        self.assertFalse(rx.match("/users/42/x/orders"))
        self.assertFalse(rx.match("/users/orders"))

    def test_flat_paths(self):
        self.assertEqual(B.flat({"a": {"b": 1}, "c": [{"d": 1}]}), {"a", "a.b", "c", "c[].d"})
        self.assertEqual(B.flat([{"id": 1}]), {"[].id"})

    def test_truth_tables_meet_the_scorecard_floor(self):
        n = len(B.SHOP) + len(B.FORGE) + len(B.GQL_OPS) + len(B.GQL_REST) - 1  # /auth/token is bootstrap only
        self.assertGreaterEqual(n, 40)

    def test_held_out_seeds_are_disjoint_from_dev_seeds(self):
        self.assertTrue(all(s >= 100 for s in B.HELD_OUT_SEEDS))


if __name__ == "__main__":
    unittest.main()
