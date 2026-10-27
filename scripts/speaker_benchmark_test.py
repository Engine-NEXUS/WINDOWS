"""Synthetic fixture tests, never speaker-accuracy evidence."""
import copy
import json
import math
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from speaker_benchmark import summarize, unique_object, validate, wilson


def fixture():
    rows = []
    specifications = [
        ("cal-o", "owner", "calibration", .8, "owner-1"),
        ("cal-n", "live_non_owner", "calibration", .5, "stranger-cal"),
        ("held-o", "owner", "acceptance", .9, "owner-1"),
        ("held-n", "live_non_owner", "acceptance", .4, "stranger-held"),
        ("held-r", "owner_replay", "acceptance", .95, "owner-1"),
        ("held-p", "other_speaker_playback", "acceptance", .7, "playback-1"),
        ("held-s", "synthetic_playback", "acceptance", .85, "synthetic-1"),
    ]
    for trial, kind, partition, score, speaker in specifications:
        rows.append(dict(trial_id=trial, model_id="oww", sample_id=trial,
                         recording_group=trial, speaker_id=speaker, speaker_kind=kind,
                         condition="quiet", partition=partition, score=score,
                         latency_ms=12, peak_memory_mb=20))
    return dict(schema_version=1, experiment_id="synthetic-test-only", source_revision="a" * 40,
                models=[dict(id="oww", license="TEST-ONLY", artifact_sha256="b" * 64,
                             score_direction="higher_is_owner")], rows=rows)


class SpeakerBenchmarkTests(unittest.TestCase):
    def test_separate_replay_synthetic_and_live_metrics(self):
        report = summarize(fixture())
        model = report["models"][0]
        self.assertEqual(model["threshold"], .8)
        self.assertEqual(model["frozen_threshold_source"], "calibration_only")
        held = model["acceptance"]
        self.assertEqual(held["owner"]["frr"], 0)
        self.assertEqual(held["live_non_owner"]["far"], 0)
        self.assertEqual(held["owner_replay"]["accepted"], 1)
        self.assertEqual(held["synthetic_playback"]["accepted"], 1)
        self.assertEqual(held["other_speaker_playback"]["accepted"], 0)
        self.assertEqual(held["live_non_owner"]["trials"], 1)
        self.assertEqual(report["action_success"], "unmeasured")
        self.assertNotIn("production_ready", report)

    def test_acceptance_cannot_tune_threshold(self):
        data = fixture()
        before = summarize(data)["models"][0]["threshold"]
        for row in data["rows"]:
            if row["partition"] == "acceptance":
                row["score"] = 1 if row["speaker_kind"] == "live_non_owner" else -1
        model = summarize(data)["models"][0]
        self.assertEqual(model["threshold"], before)
        self.assertEqual(model["acceptance"]["owner"]["frr"], 1)
        self.assertEqual(model["acceptance"]["live_non_owner"]["far"], 1)

    def test_tie_rejects_impostor_and_can_reject_all(self):
        data = fixture()
        data["rows"][0]["score"] = 1
        data["rows"][1]["score"] = 1
        report = summarize(data)["models"][0]
        self.assertEqual(report["threshold"], math.nextafter(1, math.inf))
        self.assertEqual(report["calibration"]["owner"]["accepted"], 0)
        self.assertEqual(report["calibration"]["live_non_owner"]["accepted"], 0)

    def test_lowest_observed_safe_threshold(self):
        data = fixture()
        extra = copy.deepcopy(data["rows"][0])
        extra.update(trial_id="cal-o2", sample_id="cal-o2", recording_group="cal-o2", score=.6)
        data["rows"].append(extra)
        model = summarize(data)["models"][0]
        self.assertEqual(model["threshold"], .6)
        self.assertEqual(model["calibration"]["owner"]["accepted"], 2)

    def test_split_leakage(self):
        for key in ("sample_id", "recording_group"):
            with self.subTest(key=key):
                data = fixture()
                data["rows"][2][key] = data["rows"][0][key]
                with self.assertRaisesRegex(ValueError, "split leakage"):
                    validate(data)
        data = fixture()
        data["rows"][3]["speaker_id"] = data["rows"][1]["speaker_id"]
        with self.assertRaisesRegex(ValueError, "split leakage"):
            validate(data)

    def test_playback_cannot_reuse_calibration_impostor(self):
        data = fixture()
        data["rows"][5]["speaker_id"] = data["rows"][1]["speaker_id"]
        with self.assertRaisesRegex(ValueError, "split leakage"):
            validate(data)

    def test_missing_partition_or_class_refused(self):
        for removed in ("cal-o", "cal-n", "held-o", "held-n"):
            data = fixture()
            data["rows"] = [r for r in data["rows"] if r["trial_id"] != removed]
            with self.subTest(removed=removed), self.assertRaises(ValueError):
                validate(data)

    def test_malformed_types_and_nonfinite(self):
        for field in ("score", "latency_ms", "peak_memory_mb"):
            for value in (True, "0.5", None, float("nan"), math.inf, -math.inf, 10**400):
                data = fixture()
                data["rows"][0][field] = value
                with self.subTest(field=field, value=str(value)), self.assertRaises(ValueError):
                    validate(data)
        for field, value in (("score", 1.1), ("score", -1.1), ("latency_ms", -1), ("peak_memory_mb", -1)):
            data = fixture()
            data["rows"][0][field] = value
            with self.assertRaises(ValueError):
                validate(data)

    def test_metadata_and_exact_schema(self):
        for mutate in (
            lambda d: d.update(schema_version=True),
            lambda d: d.update(source_revision="unknown"),
            lambda d: d.update(models=[]),
            lambda d: d.update(rows=[]),
            lambda d: d["models"][0].update(artifact_sha256="not-a-hash"),
            lambda d: d["models"][0].update(license=""),
            lambda d: d["models"][0].update(score_direction="lower_is_owner"),
            lambda d: d["rows"][0].update(trial_id=" "),
            lambda d: d["rows"][0].update(model_id="unknown"),
            lambda d: d["rows"][0].update(transcript="must-not-be-reported"),
            lambda d: d["rows"][0].pop("sample_id"),
            lambda d: d["rows"].append(copy.deepcopy(d["rows"][0])),
            lambda d: d["models"].append(copy.deepcopy(d["models"][0])),
        ):
            data = fixture()
            mutate(data)
            with self.assertRaises(ValueError):
                validate(data)

    def test_comparison_requires_same_trials_and_metadata(self):
        data = fixture()
        second = copy.deepcopy(data["models"][0])
        second["id"] = "embedding"
        data["models"].append(second)
        for row in list(data["rows"]):
            other = copy.deepcopy(row)
            other.update(model_id="embedding", score=row["score"] - .1)
            data["rows"].append(other)
        self.assertEqual(len(summarize(data)["models"]), 2)
        missing = copy.deepcopy(data)
        missing["rows"].pop()
        with self.assertRaisesRegex(ValueError, "identical trial sets"):
            validate(missing)
        data["rows"][-1]["trial_id"] = "different-trial"
        with self.assertRaises(ValueError):
            validate(data)

    def test_model_metadata_mismatch(self):
        data = fixture()
        data["models"].append({**data["models"][0], "id": "second"})
        data["rows"] += [{**r, "model_id": "second"} for r in data["rows"]]
        data["rows"][-1]["condition"] = "noise"
        with self.assertRaises(ValueError):
            validate(data)

    def test_condition_resources_and_dependence_counts(self):
        data = fixture()
        extra = {**data["rows"][2], "trial_id": "held-o2", "sample_id": "held-o2",
                 "score": .2, "condition": "noise", "latency_ms": 100, "peak_memory_mb": 42}
        data["rows"].append(extra)  # same group: repeated session is disclosed
        held = summarize(data)["models"][0]["acceptance"]["owner"]
        self.assertEqual(held["trials"], 2)
        self.assertEqual(held["distinct_recording_groups"], 1)
        self.assertEqual(held["distinct_speakers"], 1)
        self.assertEqual(held["distinct_samples"], 2)
        self.assertEqual(held["frr"], .5)
        self.assertEqual(held["latency_ms_median"], 56)
        self.assertEqual(held["latency_ms_p95_nearest_rank"], 100)
        self.assertEqual(held["peak_memory_mb_max"], 42)
        self.assertEqual(held["by_condition"]["noise"]["frr"], 1)
        self.assertIsNone(summarize(data)["models"][0]["calibration"]["owner_replay"]["acceptance_rate"])

    def test_uncertainty_is_not_zero_with_zero_errors(self):
        lo, hi = wilson(0, 300)
        self.assertAlmostEqual(lo, 0)
        self.assertGreater(hi, .01)
        held = summarize(fixture())["models"][0]["acceptance"]["live_non_owner"]
        self.assertAlmostEqual(held["zero_acceptance_one_sided_95_upper"], .95)

    def test_duplicate_json_keys_refused(self):
        with self.assertRaises(ValueError):
            json.loads('{"schema_version":1,"schema_version":2}', object_pairs_hook=unique_object)

    def test_cannot_disguise_impostor_as_owner_replay(self):
        data = fixture()
        data["rows"][4]["speaker_id"] = data["rows"][1]["speaker_id"]
        with self.assertRaisesRegex(ValueError, "speaker identity"):
            validate(data)

    def test_identifiers_cannot_be_file_paths(self):
        data = fixture()
        data["rows"][0]["sample_id"] = "C:/private/recording.wav"
        with self.assertRaises(ValueError):
            validate(data)

    def test_input_fingerprint_tracks_acceptance_data(self):
        data = fixture()
        report = summarize(data)
        data["rows"][2]["score"] = .3
        changed = summarize(data)
        self.assertNotEqual(report["input_sha256"], changed["input_sha256"])
        self.assertEqual(report["models"][0]["threshold"], changed["models"][0]["threshold"])

    def test_finite_resource_measurements_cannot_overflow_report(self):
        data = fixture()
        extra = {**data["rows"][2], "trial_id": "held-o2", "sample_id": "held-o2"}
        data["rows"].append(extra)
        for row in data["rows"]:
            row["latency_ms"] = 1e308
        report = summarize(data)
        self.assertEqual(report["models"][0]["acceptance"]["owner"]["latency_ms_median"], 1e308)
        json.dumps(report, allow_nan=False)

    def test_cli_no_traceback_or_input_echo(self):
        script = Path(__file__).with_name("speaker_benchmark.py")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.json"
            path.write_text(json.dumps(fixture()), encoding="utf-8")
            result = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["evidence_scope"], "offline_scores_only")
            path.write_text('{"private-text":"a deliberately incomplete file"}', encoding="utf-8")
            result = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 2)
            self.assertEqual(result.stdout, "")
            self.assertNotIn("private-text", result.stderr)
            self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
