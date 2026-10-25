import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import wake_score as S  # noqa: E402


class Score(unittest.TestCase):
    def test_sources_only_when_present_and_nonempty(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            self.assertEqual(S.find_sources(d), [])
            (d / "wake_word_data" / "sealed").mkdir(parents=True)
            (d / "wake_word_data" / "sealed" / "manifest.jsonl").write_text("")
            self.assertEqual(S.find_sources(d), [])
            (d / "wake_word_data" / "sealed" / "manifest.jsonl").write_text('{"audio":"a"}\n')
            (d / "eval_set").mkdir()
            (d / "eval_set" / "manifest.jsonl").write_text('{"audio":"b"}\n')
            self.assertEqual([s[0] for s in S.find_sources(d)], ["held-out eval set", "sealed owner takes"])

    def test_candidate_resources_swap_only_the_model(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            oww = d / "oww_src"
            oww.mkdir()
            (oww / "nexus.onnx").write_bytes(b"old")
            (oww / "melspectrogram.onnx").write_bytes(b"mel")
            cand = d / "cand.onnx"
            cand.write_bytes(b"new")
            res = S.resources_with_model(cand, d / "res", oww)
            self.assertEqual((res / "oww" / "nexus.onnx").read_bytes(), b"new")
            self.assertEqual((res / "oww" / "melspectrogram.onnx").read_bytes(), b"mel")
            self.assertEqual((oww / "nexus.onnx").read_bytes(), b"old")

    def test_merge_and_command(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "a").write_text('{"x":1}\n\n')
            (d / "b").write_text('{"x":2}\n')
            self.assertEqual(S.merge_jsonl([d / "a", d / "b"], d / "o"), 2)
            cmd = S.eval_cmd("m", "r", "o", resources="res", threshold=0.7, dsp="aligned", stage2=True)
            for tok in ("--example", "wake_eval", "--resources", "--threshold", "0.7", "--dsp", "aligned", "--stage2"):
                self.assertIn(tok, cmd)


if __name__ == "__main__":
    unittest.main()
