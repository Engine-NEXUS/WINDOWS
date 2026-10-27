"""Synthetic metadata tests only; no live acceptance or device-action evidence."""
import copy
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from voice_trial_evidence import summarize


def row(index, kind="owner", **changes):
    value = dict(trial_id=f"trial-{index}", run_id="run-1", capture_session=index + 1,
                 speaker_id="owner-1" if kind in {"owner", "owner_replay"} else f"stranger-{index % 3}",
                 recording_group=f"group-{index}", sample_id=f"sample-{index}",
                 speaker_kind=kind, condition="quiet", partition="acceptance", wake_path="hotkey",
                 voice_lock_enabled=True, verification_reached=True,
                 verification="accepted" if kind == "owner" else "rejected",
                 valid_executable_command=True, executable_action=kind == "owner",
                 intent_match=True, target_match=True,
                 requested_action_succeeded=True if kind == "owner" else False)
    return {**value, **changes}


def fixture():
    return dict(schema_version=1, source_revision="a" * 40, source_hash="b" * 64,
                build_identity="c" * 16,
                rows=[row(i) for i in range(20)] + [row(i, "live_non_owner") for i in range(20, 320)])


class VoiceTrialEvidenceTests(unittest.TestCase):
    def test_declared_numeric_screen_is_not_readiness(self):
        result = summarize(fixture())
        self.assertTrue(result["numeric_targets_met"])
        self.assertEqual(result["scope"], "declared_trial_metadata_only")
        self.assertEqual(result["production_readiness"], "not_determined")
        self.assertEqual(result["qualified_trials"], 320)
        self.assertAlmostEqual(result["mixed_negative_action_screen"]["zero_action_one_sided_95_upper"], .009936, places=5)

    def test_sample_minimum_and_owner_boundary(self):
        data = fixture()
        data["rows"].pop()
        self.assertFalse(summarize(data)["numeric_targets_met"])
        data = fixture()
        data["rows"][0].update(verification="rejected", executable_action=False, requested_action_succeeded=False)
        self.assertTrue(summarize(data)["numeric_targets_met"])
        data["rows"][1].update(verification="uncertain", executable_action=False, requested_action_succeeded=False)
        self.assertFalse(summarize(data)["numeric_targets_met"])

    def test_playback_and_owner_replay_cannot_substitute_for_live(self):
        for kind in ("owner_replay", "other_speaker_playback", "synthetic_playback"):
            data = fixture()
            for candidate in data["rows"][20:]:
                candidate["speaker_kind"] = kind
                if kind == "owner_replay":
                    candidate["speaker_id"] = "owner-1"
            result = summarize(data)
            self.assertFalse(result["numeric_targets_met"])
            self.assertFalse(result["unfamiliar_live_evidence_present"])
            self.assertEqual(result["by_speaker_kind"]["live_non_owner"]["trials"], 0)

    def test_every_qualification_gate_excludes_rows(self):
        cases = [dict(voice_lock_enabled=False), dict(capture_session=None), dict(verification_reached=False),
                 dict(verification="unavailable"), dict(valid_executable_command=False),
                 dict(partition="calibration"), dict(partition="enrollment")]
        for changes in cases:
            data = fixture()
            # New identities prevent an unrelated cross-partition identity leakage error.
            data["rows"][-1].update(speaker_id="held-separate", **changes)
            result = summarize(data)
            self.assertEqual(result["qualified_trials"], 319, changes)
            self.assertEqual(result["excluded_trials"], 1)
            self.assertFalse(result["numeric_targets_met"])
            self.assertTrue(result["exclusion_reasons"])

    def test_excluded_locked_negative_action_still_blocks(self):
        data = fixture()
        data["rows"].append(row(400, "live_non_owner", valid_executable_command=False,
                                capture_session=None, executable_action=True))
        result = summarize(data)
        self.assertEqual(result["mixed_negative_action_screen"]["trials"], 300)
        self.assertEqual(result["safety_all_locked_rows"]["non_owner_actions"], 1)
        self.assertFalse(result["numeric_targets_met"])

    def test_unverified_owner_action_cannot_be_hidden_by_exclusion(self):
        data = fixture()
        data["rows"].append(row(400, verification="rejected", valid_executable_command=False))
        result = summarize(data)
        self.assertEqual(result["safety_all_locked_rows"]["actions_without_positive_verification"], 1)
        self.assertFalse(result["numeric_targets_met"])

    def test_verified_excluded_negative_action_independently_blocks(self):
        data = fixture()
        data["rows"].append(row(400, "live_non_owner", verification="accepted",
                                valid_executable_command=False, executable_action=True))
        result = summarize(data)
        self.assertEqual(result["safety_all_locked_rows"]["actions_without_positive_verification"], 0)
        self.assertEqual(result["safety_all_locked_rows"]["non_owner_actions"], 1)
        self.assertFalse(result["numeric_targets_met"])

    def test_unattributed_accepted_action_is_not_positive_native_proof(self):
        data = fixture()
        data["rows"].append(row(400, capture_session=None))
        self.assertEqual(summarize(data)["safety_all_locked_rows"]["actions_without_positive_verification"], 1)

    def test_lock_off_control_action_is_separate_from_locked_failure(self):
        data = fixture()
        data["rows"].append(row(400, "live_non_owner", voice_lock_enabled=False, executable_action=True))
        result = summarize(data)
        self.assertTrue(result["numeric_targets_met"])
        self.assertEqual(result["excluded_trials"], 1)
        self.assertEqual(result["safety_all_locked_rows"]["non_owner_actions"], 0)

    def test_fidelity_failure_does_not_become_owner_rejection(self):
        data = fixture()
        data["rows"][0].update(executable_action=False, requested_action_succeeded=False, target_match=False)
        data["rows"][1].update(requested_action_succeeded=None, intent_match=None, target_match=None)
        result = summarize(data)
        self.assertEqual(result["by_speaker_kind"]["owner"]["acceptance_rate"], 1)
        fidelity = result["command_fidelity_verified_owner"]
        self.assertEqual(fidelity["requested_action"]["mismatched_or_failed"], 1)
        self.assertEqual(fidelity["requested_action"]["unmeasured"], 1)
        self.assertEqual(fidelity["requested_action"]["matched_or_succeeded"], 18)
        self.assertEqual(fidelity["target"]["mismatched_or_failed"], 1)

    def test_measured_wrong_action_is_visible(self):
        data = fixture()
        data["rows"][0].update(requested_action_succeeded=False, target_match=False)
        self.assertEqual(summarize(data)["safety_all_locked_rows"]["actions_with_measured_intent_or_target_mismatch"], 1)

    def test_success_claim_requires_an_action_and_correct_intent_target(self):
        for field, value in (("executable_action", False), ("target_match", False), ("intent_match", None)):
            data = fixture()
            data["rows"][0][field] = value
            with self.assertRaises(ValueError):
                summarize(data)

    def test_class_conditions_paths_and_dependency_counts(self):
        data = fixture()
        data["rows"][20].update(condition="noise", wake_path="neural")
        data["rows"][21].update(speaker_kind="other_speaker_playback", wake_path="followup")
        data["rows"][22].update(sample_id=data["rows"][25]["sample_id"],
                                recording_group=data["rows"][25]["recording_group"])
        result = summarize(data)
        live = result["by_speaker_kind"]["live_non_owner"]
        self.assertEqual(live["trials"], 299)
        self.assertEqual(live["distinct_samples"], 298)
        self.assertEqual(live["by_condition"]["noise"]["trials"], 1)
        self.assertEqual(live["by_wake_path"]["neural"]["trials"], 1)
        self.assertEqual(result["by_speaker_kind"]["other_speaker_playback"]["trials"], 1)

    def test_duplicate_capture_trial_and_partition_leakage(self):
        for mutate in (
            lambda d: d["rows"][1].update(trial_id=d["rows"][0]["trial_id"]),
            lambda d: d["rows"][1].update(capture_session=d["rows"][0]["capture_session"]),
            lambda d: d["rows"][1].update(sample_id=d["rows"][0]["sample_id"], partition="calibration"),
            lambda d: d["rows"][1].update(recording_group=d["rows"][0]["recording_group"], partition="calibration"),
            lambda d: d["rows"][20].update(partition="calibration"),
            lambda d: d["rows"][20].update(speaker_id="owner-1"),
            lambda d: d["rows"][1].update(speaker_id="another-owner"),
        ):
            data = fixture()
            mutate(data)
            with self.assertRaises(ValueError):
                summarize(data)

    def test_exact_types_and_no_extra_metadata(self):
        for field, value in (("capture_session", True), ("capture_session", 0), ("capture_session", 2**64),
                             ("voice_lock_enabled", 1), ("intent_match", 0), ("verification", "unknown"),
                             ("wake_path", "other"), ("condition", "private/path"), ("transcript", "private")):
            data = fixture()
            data["rows"][0][field] = value
            with self.subTest(field=field), self.assertRaises(ValueError):
                summarize(data)
        for field, value in (("source_revision", "old"), ("schema_version", True), ("build_identity", None), ("source_hash", "changed")):
            data = fixture()
            data[field] = value
            with self.assertRaises(ValueError):
                summarize(data)

    def test_empty_sheet_is_not_success(self):
        data = fixture()
        data["rows"] = []
        result = summarize(data)
        self.assertFalse(result["numeric_targets_met"])
        self.assertIsNone(result["by_speaker_kind"]["owner"]["acceptance_rate"])

    def test_report_contains_no_trial_or_person_identifiers(self):
        data = fixture()
        saved = copy.deepcopy(data)
        result = summarize(data)
        text = json.dumps(result)
        for private in ("trial-0", "stranger-1", "sample-0", '"rows"'):
            self.assertNotIn(private, text)
        self.assertEqual(data, saved)
        data["rows"][0]["condition"] = "noise"
        self.assertNotEqual(result["input_sha256"], summarize(data)["input_sha256"])

    def test_cli_errors_and_duplicate_json_fields_are_clean(self):
        script = Path(__file__).with_name("voice_trial_evidence.py")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trials.json"
            for content in (json.dumps(fixture()), '{"schema_version":1,"schema_version":2}', '{"private":"never-echo"}'):
                path.write_text(content, encoding="utf-8")
                process = subprocess.run([sys.executable, str(script), str(path)], capture_output=True, text=True)
                if content.startswith('{"schema_version": 1'):
                    self.assertEqual(process.returncode, 0, process.stderr)
                    self.assertEqual(json.loads(process.stdout)["production_readiness"], "not_determined")
                else:
                    self.assertEqual(process.returncode, 2)
                    self.assertEqual(process.stdout, "")
                    self.assertNotIn("never-echo", process.stderr)
                    self.assertNotIn("Traceback", process.stderr)


if __name__ == "__main__":
    unittest.main()
