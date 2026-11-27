import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import reverse_eval as E  # noqa: E402


def facts(**kw):
    base = {"machine": 0x8664, "timestamp": 5, "subsystem": 3, "entry_point": 0x1000, "image_base": 0x140000000,
            "is_dll": False, "is_64": True, "dll_characteristics": 0x8160, "n_sections": 1, "has_clr": False,
            "has_tls": False, "has_debug": False, "has_signature": False,
            "sections": [{"name": ".text", "virtual_size": 1, "virtual_address": 0x1000, "raw_size": 0x200,
                          "raw_offset": 0x200, "characteristics": 0x60000020}],
            "imports": {("kernel32.dll", "CreateFileW", None)}, "exports": set(), "version": {k: None for k in E.VERSION_KEYS}}
    base.update(kw)
    return base


class Compare(unittest.TestCase):
    def test_identical_facts_fully_agree(self):
        s, a, n, d = E.compare_file(facts(), {}, facts())
        self.assertEqual((s, a, n, d), (s, s, 0, []))
        self.assertGreater(s, 15)

    def test_a_wrong_field_and_a_missing_import_are_disagreements(self):
        rust = facts(entry_point=0x2000, imports=set())
        s, a, n, d = E.compare_file(facts(), {}, rust)
        self.assertEqual(s - a, 2)
        self.assertIn("entry_point", d)
        self.assertTrue(any(x.startswith("imports:") for x in d))

    def test_parse_failure_counts_every_field_as_disagreement(self):
        s, a, n, d = E.compare_file(facts(), {}, None)
        self.assertEqual(a, 0)
        self.assertGreater(s, 15)

    def test_oracle_noise_is_excluded_from_the_denominator(self):
        lief = {"has_tls": True}                       # LIEF disagrees with pefile on has_tls
        s0, a0, _, _ = E.compare_file(facts(), {}, facts())
        s1, a1, n1, _ = E.compare_file(facts(), lief, facts(has_tls=True))
        self.assertEqual(n1, 1)
        self.assertEqual(s1, s0 - 1)
        self.assertEqual(a1, s1)                       # and Rust is not blamed for it

    def test_extra_import_in_rust_only_is_a_disagreement(self):
        rust = facts(imports={("kernel32.dll", "CreateFileW", None), ("user32.dll", "MessageBoxW", None)})
        s, a, _, d = E.compare_file(facts(), {}, rust)
        self.assertEqual(s - a, 1)

    def test_rust_view_maps_ordinals_and_unnamed_exports(self):
        rf = {"machine": 1, "timestamp": 2, "subsystem": 3, "entry_point": 4, "image_base": 5, "is_dll": True, "is_64": False,
              "dll_characteristics": 0, "sections": [], "has_clr": False, "has_tls": False, "has_debug": False,
              "has_signature": False, "version": {},
              "imports": [{"dll": "KERNEL32.dll", "name": None, "ordinal": 7}, {"dll": "A.dll", "name": "F", "ordinal": None}],
              "exports": [{"name": None, "rva": 16}, {"name": "Foo", "rva": 32}]}
        v = E.rust_view(rf)
        self.assertEqual(v["imports"], {("kernel32.dll", None, 7), ("a.dll", "F", None)})
        self.assertEqual(v["exports"], {"#16", "Foo"})


if __name__ == "__main__":
    unittest.main()
