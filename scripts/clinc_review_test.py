#!/usr/bin/env python3

import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MODULE_PATH = ROOT / "server" / "nlu" / "review_clinc150.py"
spec = importlib.util.spec_from_file_location("review_clinc150", MODULE_PATH)
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


class ClincReviewTests(unittest.TestCase):
    def test_sensitive_requests_are_unknown(self):
        decision, intent, _ = review.review_text("can you fill in my credit card number on the screen")
        self.assertEqual((decision, intent), ("approved_unknown", "unknown"))

    def test_explicit_web_search_is_supported(self):
        decision, intent, _ = review.review_text("search the web for rust ownership")
        self.assertEqual((decision, intent), ("mapped_supported", "browser_search"))

    def test_capability_adjacent_language_is_excluded(self):
        decision, intent, _ = review.review_text("what time does the museum open")
        self.assertEqual((decision, intent), ("excluded_ambiguous", None))

    def test_general_oos_is_unknown(self):
        decision, intent, _ = review.review_text("how many sides are in a hexagon")
        self.assertEqual((decision, intent), ("approved_unknown", "unknown"))

    def test_review_outputs_are_complete_and_locked(self):
        data = ROOT / "server" / "nlu" / "data"
        report = json.loads((data / "clinc150_review_report.json").read_text())
        lock = json.loads((data / "external_evaluation_lock.json").read_text())
        benchmarks = {item["id"]: item for item in lock["benchmarks"]}
        ids = set(benchmarks)
        self.assertIn("clinc150_oos_reviewed_test", ids)
        self.assertIn("clinc150_supported_mappings", ids)
        self.assertEqual(report["input"]["test"], benchmarks["clinc150_oos_test"]["rows"])
        self.assertEqual(report["output"]["approved_unknown_test"], benchmarks["clinc150_oos_reviewed_test"]["rows"])
        self.assertEqual(report["output"]["mapped_supported_test"], benchmarks["clinc150_supported_mappings"]["rows"])
        self.assertEqual(sum(report["counts"].values()), sum(report["input"].values()))


if __name__ == "__main__":
    unittest.main()
