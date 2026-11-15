"""Offline structural regressions for the cross-platform Python runtime lock."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ContributorLockTests(unittest.TestCase):
    def test_every_runtime_distribution_is_exactly_pinned(self):
        lines = (ROOT / "scripts/requirements-contributor-lock.txt").read_text().splitlines()
        packages = {}
        for line in lines:
            if not line.strip() or line.startswith("#"):
                continue
            match = re.fullmatch(r'([a-z0-9-]+)==([0-9][^;\s]*)(?:; .+)?', line)
            self.assertIsNotNone(match, line)
            self.assertNotIn(match[1], packages, line)
            packages[match[1]] = match[2]
        self.assertGreaterEqual(len(packages), 40)
        self.assertEqual(packages["moonshine-voice"], "0.1.5")

    def test_direct_runtime_requirements_agree_with_the_transitive_lock(self):
        lock = (ROOT / "scripts/requirements-contributor-lock.txt").read_text()
        direct = (ROOT / "scripts/requirements-contributor.txt").read_text().splitlines()
        self.assertIn("-r requirements-contributor-lock.txt", direct)
        for line in direct:
            if "==" in line:
                name, version = line.split("==")
                name = name.split("[")[0]
                self.assertRegex(lock, rf"(?m)^{re.escape(name)}=={re.escape(version)}(?:;|$)")
        self.assertRegex(lock, r'(?m)^colorama==.+platform_system == "Windows"$')
        self.assertRegex(lock, r'(?m)^uvloop==.+platform_system == "Darwin" or platform_system == "Linux"$')


if __name__ == "__main__":
    unittest.main()
