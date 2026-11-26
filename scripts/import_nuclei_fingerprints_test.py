"""Tests for scripts/import_nuclei_fingerprints.py (run: python -m unittest scripts/import_nuclei_fingerprints_test.py)."""
import importlib.util
import os
import unittest

HERE = os.path.dirname(__file__)
spec = importlib.util.spec_from_file_location("imp", os.path.join(HERE, "import_nuclei_fingerprints.py"))
imp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(imp)


def tpl(matchers, cond="or", paths=("{{BaseURL}}",), name="Acme CMS - Detect", tags="cms,detect"):
    return {
        "info": {"name": name, "tags": tags},
        "http": [{"method": "GET", "path": list(paths), "matchers-condition": cond, "matchers": matchers}],
    }


class Convert(unittest.TestCase):
    def test_or_words_are_detected_by_one_hit(self):
        t = imp.convert(tpl([{"type": "word", "part": "body", "words": ["powered by acmecms"], "condition": "or"}]), "acme")
        self.assertEqual(t["name"], "Acme CMS")
        self.assertEqual(t["category"], "cms")
        self.assertEqual(t["signals"][0]["weight"], imp.OR_WEIGHT)
        self.assertGreaterEqual(imp.OR_WEIGHT, 0.80, "one hit must reach 'detected'")

    def test_and_words_need_two_hits(self):
        t = imp.convert(tpl([{"type": "word", "part": "body", "words": ["acmecms-core", "acmecms-theme"], "condition": "and"}]), "acme")
        self.assertEqual({s["weight"] for s in t["signals"]}, {imp.AND_WEIGHT})
        self.assertLess(imp.AND_WEIGHT, 0.5, "one hit alone stays below 'likely'")
        self.assertGreaterEqual(1 - (1 - imp.AND_WEIGHT) ** 2, 0.5, "two hits reach 'likely'")

    def test_short_words_are_never_signals(self):
        self.assertIsNone(imp.convert(tpl([{"type": "word", "part": "body", "words": ["acme", "login"]}]), "acme"))

    def test_other_urls_dsl_and_binary_are_skipped(self):
        w = [{"type": "word", "part": "body", "words": ["powered by acmecms"]}]
        self.assertIsNone(imp.convert(tpl(w, paths=("{{BaseURL}}/admin",)), "x"))
        self.assertIsNone(imp.convert(tpl(w + [{"type": "dsl", "dsl": ["status_code==200"]}]), "x"))
        self.assertIsNone(imp.convert(tpl([{"type": "binary", "binary": ["ff"]}]), "x"))

    def test_status_matchers_are_ignored_not_fatal(self):
        t = imp.convert(tpl([{"type": "word", "part": "body", "words": ["powered by acmecms"]}, {"type": "status", "status": [200]}], cond="and"), "x")
        self.assertIsNotNone(t)

    def test_header_names_and_values(self):
        t = imp.convert(tpl([{"type": "word", "part": "header", "words": ["X-Acmeware-Version", "Server: AcmewareHTTPd"]}], name="Acmeware CMS - Detect"), "x")
        kinds = {(s["kind"], s["name"], s.get("contains")) for s in t["signals"]}
        self.assertIn(("header", "x-acmeware-version", None), kinds)
        self.assertIn(("header", "server", "acmewarehttpd"), kinds)

    def test_unsupported_regex_features_are_dropped(self):
        t = imp.convert(tpl([{"type": "regex", "part": "body", "regex": ["(?=lookahead)acmecms", "acmecms-[0-9]+"]}]), "x")
        self.assertEqual(len(t["signals"]), 1)
        self.assertTrue(t["signals"][0]["regex"].startswith("(?i)"))

    def test_a_signal_that_does_not_name_the_product_is_dropped(self):
        # the first import turned 'Documentation' into "Symfony Default Page" on 20+ real sites
        self.assertIsNone(imp.convert(tpl([{"type": "word", "part": "body", "words": ["Documentation page"]}], name="Symfony Default Page", tags="symfony"), "x"))
        self.assertIsNone(imp.convert(tpl([{"type": "word", "part": "body", "words": ["/themes/default"]}], name="Winter CMS"), "x"))
        self.assertIsNotNone(imp.convert(tpl([{"type": "word", "part": "body", "words": ["powered by winter cms"]}], name="Winter CMS"), "x"))

    def test_names_that_describe_a_kind_of_page_are_skipped(self):
        for name in ("Coming Soon Page", "Symfony Default Page", "Welcome To Nginx", "Site Under Maintenance"):
            self.assertIsNone(imp.convert(tpl([{"type": "word", "part": "body", "words": [name.lower()]}], name=name), "x"), name)

    def test_every_signal_is_tagged_for_clean_removal(self):
        t = imp.convert(tpl([{"type": "word", "part": "body", "words": ["powered by acmecms"]}]), "x")
        self.assertTrue(all(s["evidence"].endswith(imp.TAG) for s in t["signals"]))
        self.assertEqual(t["source"], imp.SOURCE)

    def test_rerun_removes_its_own_output_and_nothing_else(self):
        table = {"technologies": [
            {"id": "react", "name": "React", "category": "x", "signals": [{"kind": "html", "contains": "data-reactroot", "weight": 0.8, "evidence": "hand written"}]},
            {"id": "acme-cms", "name": "Acme CMS", "category": "cms", "source": imp.SOURCE, "signals": []},
        ]}
        removed = imp.strip_previous(table)
        self.assertEqual(removed, 1)
        self.assertEqual([t["id"] for t in table["technologies"]], ["react"])
        self.assertEqual(table["technologies"][0]["signals"][0]["evidence"], "hand written")

    def test_clean_name(self):
        self.assertEqual(imp.clean_name("Shopify Detection"), "Shopify")
        self.assertEqual(imp.clean_name("Detect Abyss Web Server"), "Abyss Web Server")
        self.assertEqual(imp.clean_name("Foo - Detect"), "Foo")


if __name__ == "__main__":
    unittest.main()
