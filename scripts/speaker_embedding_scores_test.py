"""Synthetic enrollment/scoring invariants, not biometric accuracy evidence."""
import copy
import json
import math
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from speaker_embedding_scores import normalize, report, score_experiment


def fixture():
    rows = []
    for trial, kind, partition, speaker, vector in [
        ("enroll-1", "owner", "enrollment", "owner-1", [1, 0]),
        ("enroll-2", "owner", "enrollment", "owner-1", [.6, .8]),
        ("enroll-3", "owner", "enrollment", "owner-1", [1, 0]),
        ("cal-owner", "owner", "calibration", "owner-1", [.8, .6]),
        ("cal-other", "live_non_owner", "calibration", "stranger-cal", [-1, 0]),
        ("held-owner", "owner", "acceptance", "owner-1", [1, 0]),
        ("held-other", "live_non_owner", "acceptance", "stranger-held", [0, -1]),
        ("held-replay", "owner_replay", "acceptance", "owner-1", [0, 1]),
    ]:
        rows.append(dict(trial_id=trial, model_id="baseline", sample_id=trial,
                         recording_group="enrollment-session" if partition == "enrollment" else trial,
                         speaker_id=speaker, speaker_kind=kind, condition="quiet", partition=partition,
                         latency_ms=10, peak_memory_mb=30, embedding=vector))
    return dict(schema_version=1, experiment_id="synthetic-embedding-tests", source_revision="a" * 40,
                models=[dict(id="baseline", license="TEST-ONLY", artifact_sha256="b" * 64,
                             score_direction="higher_is_owner", embedding_dimension=2,
                             preprocessing_sha256="c" * 64)], rows=rows)


class EmbeddingScoreTests(unittest.TestCase):
    def test_known_cosines_and_enrollment_excluded(self):
        scores, provenance = score_experiment(fixture())
        by_trial = {row["trial_id"]: row for row in scores["rows"]}
        self.assertEqual(len(by_trial), 5)
        self.assertAlmostEqual(by_trial["cal-owner"]["score"], .96)
        self.assertAlmostEqual(by_trial["cal-other"]["score"], -.6)
        self.assertEqual(by_trial["held-owner"]["score"], 1)
        self.assertEqual(by_trial["held-other"]["score"], 0)
        self.assertAlmostEqual(by_trial["held-replay"]["score"], .8)
        self.assertEqual(provenance["models"][0]["enrollment_samples"], 3)
        self.assertEqual(provenance["models"][0]["enrollment_recording_groups"], 1)

    def test_uses_max_not_average_template(self):
        data = fixture()
        data["rows"][0]["embedding"] = [0, 1]
        data["rows"][1]["embedding"] = [1, 0]
        data["rows"][2]["embedding"] = [-1, 0]
        scores, _ = score_experiment(data)
        self.assertEqual(next(r for r in scores["rows"] if r["trial_id"] == "held-owner")["score"], 1)

    def test_acceptance_cannot_change_template_or_threshold(self):
        data = fixture()
        original = report(data)
        scores, _ = score_experiment(data)
        for row in data["rows"]:
            if row["partition"] == "acceptance":
                row["embedding"] = [0, -1]
        changed = report(data)
        after, _ = score_experiment(data)
        self.assertEqual(original["models"][0]["threshold"], changed["models"][0]["threshold"])
        self.assertEqual([r for r in scores["rows"] if r["partition"] == "calibration"],
                         [r for r in after["rows"] if r["partition"] == "calibration"])
        # If the held-out vector entered the template its similarity would be 1.
        self.assertEqual(next(r for r in after["rows"] if r["trial_id"] == "held-owner")["score"], 0)
        self.assertNotEqual(original["embedding_provenance"]["input_sha256"], changed["embedding_provenance"]["input_sha256"])

    def test_calibration_cannot_enter_template(self):
        data = fixture()
        data["rows"][3]["embedding"] = [0, -1]
        data["rows"][5]["embedding"] = [0, -1]
        scores, _ = score_experiment(data)
        self.assertEqual(next(r for r in scores["rows"] if r["trial_id"] == "held-owner")["score"], 0)

    def test_enrollment_sample_and_group_leakage(self):
        for partition in ("calibration", "acceptance"):
            for field in ("sample_id", "recording_group"):
                data = fixture()
                row = next(r for r in data["rows"] if r["partition"] == partition)
                row[field] = data["rows"][0][field]
                with self.subTest(partition=partition, field=field), self.assertRaises(ValueError):
                    score_experiment(data)

    def test_only_owner_can_enroll(self):
        data = fixture()
        data["rows"][0]["speaker_kind"] = "live_non_owner"
        with self.assertRaisesRegex(ValueError, "owner only"):
            score_experiment(data)

    def test_three_distinct_enrollment_samples_required(self):
        data = fixture()
        data["rows"].pop(0)
        with self.assertRaises(ValueError):
            score_experiment(data)
        data = fixture()
        data["rows"][1]["sample_id"] = data["rows"][0]["sample_id"]
        with self.assertRaisesRegex(ValueError, "duplicate sample"):
            score_experiment(data)

    def test_single_owner_and_replay_identity(self):
        for trial in ("enroll-2", "cal-owner", "held-owner", "held-replay"):
            data = fixture()
            next(r for r in data["rows"] if r["trial_id"] == trial)["speaker_id"] = "different-owner"
            with self.subTest(trial=trial), self.assertRaisesRegex(ValueError, "owner identity"):
                score_experiment(data)

    def test_missing_classes_and_impostor_identity_leakage(self):
        for trial in ("cal-owner", "cal-other", "held-owner", "held-other"):
            data = fixture()
            data["rows"] = [r for r in data["rows"] if r["trial_id"] != trial]
            with self.assertRaises(ValueError):
                score_experiment(data)
        data = fixture()
        data["rows"][6]["speaker_id"] = data["rows"][4]["speaker_id"]
        with self.assertRaisesRegex(ValueError, "split leakage"):
            score_experiment(data)

    def test_robust_normalization(self):
        for vector in ([1e308, 1e308], [1.7e308, 1.7e308], [1e-308, 1e-308]):
            result = normalize(vector, 2)
            self.assertTrue(all(math.isfinite(value) for value in result))
            self.assertAlmostEqual(result[0], math.sqrt(.5))
            self.assertAlmostEqual(math.hypot(*result), 1)

    def test_invalid_vectors_and_dimensions(self):
        for vector in ([], [1], [1, 0, 0], [0, 0], [True, 1], ["1", 0],
                       [math.nan, 1], [math.inf, 1], [-math.inf, 0], [10**400, 0], None):
            data = fixture()
            data["rows"][0]["embedding"] = vector
            with self.subTest(vector=str(vector)), self.assertRaises(ValueError):
                score_experiment(data)
        for dimension in (True, 1, 4097, 2.0, "2"):
            data = fixture()
            data["models"][0]["embedding_dimension"] = dimension
            with self.assertRaises(ValueError):
                score_experiment(data)

    def test_exact_schema_hashes_types_and_duplicates(self):
        for mutate in (
            lambda d: d.update(schema_version=True),
            lambda d: d.update(source_revision="missing"),
            lambda d: d.update(models=[]),
            lambda d: d.update(rows=[]),
            lambda d: d["models"][0].update(artifact_sha256="x"),
            lambda d: d["models"][0].update(preprocessing_sha256="x"),
            lambda d: d["models"][0].update(license=""),
            lambda d: d["rows"][0].update(latency_ms=True),
            lambda d: d["rows"][0].update(peak_memory_mb=-1),
            lambda d: d["rows"][0].update(model_id="unknown"),
            lambda d: d["rows"][0].update(transcript="private"),
            lambda d: d["rows"][0].update(sample_id="C:/private.wav"),
            lambda d: d["rows"].append(copy.deepcopy(d["rows"][0])),
            lambda d: d["models"].append(copy.deepcopy(d["models"][0])),
        ):
            data = fixture()
            mutate(data)
            with self.assertRaises(ValueError):
                score_experiment(data)

    def test_models_require_complete_paired_enrollment_and_trials(self):
        data = fixture()
        data["models"].append({**data["models"][0], "id": "candidate"})
        data["rows"] += [{**copy.deepcopy(row), "model_id": "candidate"} for row in data["rows"]]
        self.assertEqual(len(report(data)["models"]), 2)
        for trial in ("enroll-1", "held-owner"):
            incomplete = copy.deepcopy(data)
            incomplete["rows"] = [r for r in incomplete["rows"] if not (r["model_id"] == "candidate" and r["trial_id"] == trial)]
            with self.assertRaises(ValueError):
                score_experiment(incomplete)
        data["rows"][-1]["condition"] = "noise"
        with self.assertRaises(ValueError):
            score_experiment(data)

    def test_reports_exclude_vectors_and_preserve_resource_declarations(self):
        data = fixture()
        saved = copy.deepcopy(data)
        output = report(data)
        self.assertEqual(data, saved)
        encoded = json.dumps(output, allow_nan=False)
        self.assertNotIn('"embedding":', encoded)
        self.assertNotIn('"rows":', encoded)
        self.assertNotIn("enroll-1", encoded)
        self.assertEqual(output["models"][0]["acceptance"]["owner"]["latency_ms_median"], 10)
        self.assertEqual(output["models"][0]["acceptance"]["owner"]["peak_memory_mb_max"], 30)
        self.assertEqual(output["embedding_provenance"]["scoring_policy"], "max_enrollment_cosine_v1")
        self.assertEqual(output["embedding_provenance"]["models"][0]["preprocessing_sha256"], "c" * 64)
        self.assertEqual(output["action_success"], "unmeasured")

    def test_clean_cli_and_duplicate_json_keys(self):
        script = Path(__file__).with_name("speaker_embedding_scores.py")
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "synthetic.json"
            path.write_text(json.dumps(fixture()), encoding="utf-8")
            run = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertIn("embedding_provenance", json.loads(run.stdout))
            for invalid in ('{"private":"do not echo me"}', '{"schema_version":1,"schema_version":2}'):
                path.write_text(invalid, encoding="utf-8")
                run = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
                self.assertEqual(run.returncode, 2)
                self.assertEqual(run.stdout, "")
                self.assertNotIn("private", run.stderr)
                self.assertNotIn("Traceback", run.stderr)
            path.unlink()
            run = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
            self.assertEqual(run.returncode, 2)
            self.assertNotIn(str(path), run.stderr)


if __name__ == "__main__":
    unittest.main()
