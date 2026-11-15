import json
from pathlib import Path
import struct
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import wake_cohort as cohort


def person(speaker, split, device):
    return {"speaker_id": speaker, "device_id": device, "split": split,
            "device_class": "array", "laptop_model": "fixture " + device, "accent": "fixture"}


def write_clip(path, level):
    cohort.voice._write_wav(path, struct.pack("<h", level) * 16000)


class WakeCohortTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        self.people = {"alice": person("alice", "train", "lap1"),
                       "bob": person("bob", "validation", "lap2"),
                       "cam": person("cam", "test", "lap3")}
        self.roster = self.root / "roster.json"
        self.write_roster()

    def write_roster(self):
        self.roster.write_text(json.dumps({"participants": list(self.people.values())}), encoding="utf-8")

    def sample(self, who, level):
        row = next(cohort.slots(self.people[who]))
        cohort.save_sample(self.root, row, lambda path: write_clip(path, level))
        return row

    def test_balanced_sittings(self):
        rows = list(cohort.slots(self.people["alice"]))
        self.assertEqual(len({row["id"] for row in rows}), 150)
        for phrase in cohort.voice.WAKE_PHRASES:
            for condition, *_ in cohort.CONDITIONS:
                matched = [row for row in rows if row["phrase"] == phrase and row["condition"] == condition]
                self.assertEqual(len(matched), 10)
                self.assertEqual({row["session_id"] for row in matched}, {"s1", "s2", "s3"})

    def test_roster_rejects_shared_physical_laptop_between_splits(self):
        self.people["bob"]["device_id"] = "lap1"
        self.people["bob"]["laptop_model"] = self.people["alice"]["laptop_model"]
        self.write_roster()
        with self.assertRaisesRegex(ValueError, "conflicting"):
            cohort.load_roster(self.roster)

    def test_audit_rejects_duplicate_audio_and_tampered_split(self):
        self.sample("alice", 1200)
        row = self.sample("bob", 1200)
        _, report = cohort.audit(self.root, self.people)
        self.assertIn("Duplicate PCM", report["errors"][0])
        meta = self.root / "samples" / row["id"] / "row.json"
        data = json.loads(meta.read_text())
        data["split"] = "test"
        meta.write_text(json.dumps(data))
        _, report = cohort.audit(self.root, self.people)
        self.assertIn("disagrees", report["errors"][0])

    def test_failed_sample_can_be_retried_without_overwriting(self):
        row = next(cohort.slots(self.people["alice"]))
        with self.assertRaisesRegex(ValueError, "digital silence"):
            cohort.save_sample(self.root, row, lambda path: write_clip(path, 0))
        self.sample("alice", 1200)
        with self.assertRaisesRegex(ValueError, "already exists"):
            self.sample("alice", 1201)
        self.assertEqual(list(self.root.glob(".pending-*")), [])

    def test_cli_exports_only_calibration_and_holds_incomplete_acceptance(self):
        self.sample("alice", 1200)
        self.sample("bob", 1300)
        self.sample("cam", 1400)
        out = self.root / "manifest.jsonl"
        argv = ["wake_cohort", "--roster", str(self.roster), "--root", str(self.root),
                "export", "--split", "validation", "--out", str(out)]
        with patch("sys.argv", argv):
            self.assertEqual(cohort.main(), 0)
        self.assertEqual([row["speaker_id"] for row in cohort.voice.read_manifest(out)], ["bob"])
        argv[argv.index("validation")] = "test"
        with patch("sys.argv", argv), self.assertRaises(SystemExit) as err:
            cohort.main()
        self.assertEqual(err.exception.code, 1)

    def test_replay_rejects_missing_rows_and_wrong_metadata(self):
        self.sample("bob", 1200)
        rows, _ = cohort.audit(self.root, self.people)
        result = {**rows[0], "threshold": .68, "dsp": "legacy", "fired": True, "cohort_replay": True}
        self.assertEqual(cohort.replay_metrics(rows, [result], .68, "legacy")["overall"]["recall"], 1)
        with self.assertRaisesRegex(ValueError, "omitted"):
            cohort.replay_metrics(rows, [], .68, "legacy")
        with self.assertRaisesRegex(ValueError, "metadata mismatch"):
            cohort.replay_metrics(rows, [{**result, "device_id": "lap1"}], .68, "legacy")
        with self.assertRaisesRegex(ValueError, "lacks explicit cohort mode"):
            cohort.replay_metrics(rows, [{**result, "cohort_replay": False}], .68, "legacy")

    def test_replay_writes_fingerprinted_report_with_room_rate(self):
        self.sample("bob", 1200)
        room = cohort.expected_room(self.people["bob"], {"session_id": "r1", "room_id": "fan1",
                                                        "environment": "fan"})
        cohort.save_sample(self.root, room, lambda path: write_clip(path, 1400))
        rows, report = cohort.audit(self.root, self.people)
        assets = self.root / "resources" / "oww"
        assets.mkdir(parents=True)
        for name in ("nexus.onnx", "melspectrogram.onnx", "embedding_model.onnx",
                     "acoustic_profile.json", "dsp_spec.json"):
            (assets / name).write_text("fixture")
        evaluator = self.root / "wake_eval.exe"
        evaluator.write_text("fixture")
        args = SimpleNamespace(evaluator=evaluator, resources=assets.parent, root=self.root,
                               out=self.root / "run", split="validation", threshold=.68, dsp="legacy")

        def fake_eval(command, *, check):
            self.assertIn("--cohort-replay", command)
            self.assertNotIn("--stage2", command)
            results = [{**row, "cohort_replay": True, "threshold": .68, "dsp": "legacy",
                        "fired": True, "n_fires": 1} for row in rows]
            cohort.write_manifest(Path(command[command.index("--out") + 1]), results)

        with patch.object(cohort.subprocess, "run", side_effect=fake_eval):
            cohort.replay(args, rows, report)
        evidence = json.loads((args.out / "report.json").read_text())
        self.assertEqual(evidence["measurement"], "neural_replay")
        self.assertEqual(evidence["groups"]["device_id=lap2"]["raw_detections_per_hour"], 3600)
        self.assertEqual(evidence["input_file_sha256"][str(evaluator.resolve())], cohort.file_sha256(evaluator))


if __name__ == "__main__":
    unittest.main()
