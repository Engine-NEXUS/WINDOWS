import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from speaker_trial_summary import summarize

SCRIPT = Path(__file__).with_name("speaker_trial_summary.py")


class SpeakerTrialSummaryTests(unittest.TestCase):
    def summarize_csv(self, text: str) -> dict:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "trials.csv"
            path.write_text(text, encoding="utf-8")
            return summarize(path)

    def summarize_rows(self, rows: list[str]) -> dict:
        return self.summarize_csv(
            "trial_id,speaker,condition,verification,executable_action\n"
            + "\n".join(rows)
            + "\n"
        )

    def test_legacy_evidence_cannot_establish_target_even_with_300_negatives(self) -> None:
        rows = [f"o{i},owner,quiet,accepted,yes" for i in range(20)]
        rows += [f"n{i},non_owner,quiet,rejected,no" for i in range(200)]
        rows += [f"t{i},tv_replay,television,rejected,no" for i in range(100)]
        result = self.summarize_rows(rows)
        self.assertFalse(result["numeric_targets_met"])
        self.assertEqual(result["evidence_scope"], "legacy_unqualified_csv")
        self.assertEqual(len(result["warnings"]), 2)
        self.assertTrue(result["warnings"][0])
        self.assertFalse(any("300" in str(reason) for reason in result["numeric_targets_blocked_reasons"]))
        self.assertEqual(result["negative_actions"], 0)
        self.assertAlmostEqual(result["zero_negative_action_one_sided_95_upper"], 0.009936, places=5)

    def test_all_tv_replay_rows_cannot_establish_target(self) -> None:
        rows = [f"t{i},tv_replay,television,rejected,no" for i in range(300)]
        result = self.summarize_rows(rows)
        self.assertFalse(result["numeric_targets_met"])
        self.assertEqual(result["evidence_scope"], "legacy_unqualified_csv")
        self.assertEqual(result["negative_class"], "tv_replay")
        self.assertEqual(result["by_speaker"]["tv_replay"]["trials"], 300)
        self.assertNotIn("non_owner", result["by_speaker"])

    def test_mixed_class_negative_rows_are_labelled_and_fail_target(self) -> None:
        rows = [f"t{i},tv_replay,television,rejected,no" for i in range(250)]
        rows += [f"n{i},non_owner,quiet,rejected,no" for i in range(100)]
        result = self.summarize_rows(rows)
        self.assertFalse(result["numeric_targets_met"])
        self.assertEqual(result["negative_class"], "mixed")
        self.assertEqual(result["by_speaker"]["tv_replay"]["trials"], 250)
        self.assertEqual(result["by_speaker"]["non_owner"]["trials"], 100)

    def test_independent_per_kind_and_condition_counts(self) -> None:
        rows = ["o0,owner,quiet,accepted,yes", "o1,owner,quiet,uncertain,yes"]
        rows += [f"n{i},non_owner,quiet,rejected,no" for i in range(150)]
        rows += [f"t{i},tv_replay,television,rejected,no" for i in range(150)]
        result = self.summarize_rows(rows)
        owner = result["by_speaker"]["owner"]
        non_owner = result["by_speaker"]["non_owner"]
        tv_replay = result["by_speaker"]["tv_replay"]
        self.assertEqual(owner["trials"], 2)
        self.assertEqual(owner["accepted"], 1)
        self.assertEqual(owner["rejected_or_uncertain"], 1)
        self.assertEqual(owner["actions"], 2)
        self.assertEqual(non_owner["trials"], 150)
        self.assertEqual(non_owner["accepted"], 0)
        self.assertEqual(non_owner["actions"], 0)
        self.assertEqual(tv_replay["trials"], 150)
        for kind in (owner, non_owner, tv_replay):
            self.assertLessEqual(kind["acceptance_wilson_95"][0], kind["acceptance_rate"])
            self.assertLessEqual(kind["acceptance_rate"], kind["acceptance_wilson_95"][1])
        self.assertEqual(result["by_condition"]["quiet"]["trials"], 152)
        self.assertEqual(result["by_condition"]["television"]["trials"], 150)
        self.assertEqual(result["by_condition"]["television"]["accepted"], 0)
        self.assertNotIn("television", result["by_speaker"])
        self.assertEqual(
            result["by_speaker"]["owner"]["trials"] + result["by_speaker"]["non_owner"]["trials"]
            + result["by_speaker"]["tv_replay"]["trials"],
            result["owner_trials"] + result["negative_trials"],
        )

    def test_action_without_acceptance_fails_target(self) -> None:
        rows = [f"o{i},owner,quiet,accepted,yes" for i in range(19)]
        rows.append("o19,owner,quiet,uncertain,yes")
        rows += [f"n{i},non_owner,quiet,rejected,no" for i in range(300)]
        result = self.summarize_rows(rows)
        self.assertFalse(result["numeric_targets_met"])
        self.assertEqual(result["owner_actions_without_acceptance"], 1)

    def test_duplicate_trial_is_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate trial_id"):
            self.summarize_rows([
                "same,owner,quiet,accepted,no",
                "same,non_owner,quiet,rejected,no",
            ])

    def test_malformed_rows_raise_value_error(self) -> None:
        header = "trial_id,speaker,condition,verification,executable_action\n"
        cases = [
            "extra,o0,owner,quiet,accepted,yes\n",
            "o0\n",
            "o0,owner,quiet,accepted\n",
            "o0,owner,quiet,accepted,yes,,\n",
            "out1,owner,quiet,accepted,yes\no1,non_owner,quiet,rejected,no\nout1,tv_replay,television,rejected,no\n",
            " ,owner,quiet,accepted,no\n",
            "\t,owner,quiet,accepted,no\n",
            "o0,owner,quiet,accepted,no\no1,non_owner,quiet ,rejected,no\n",
            "o0,owner,quiet,accepted,no\n o1 ,non_owner,quiet,rejected,no\n",
            "o0,owner,quiet,accepted,no\no1,owner, ,rejected,no\n",
            "o0,owner,quiet,accepted,no\no1,owner,quiet,accepted,maybe\n",
            "o0,owner,quiet,accepted,no\no1,owner,quiet,accepted,\n",
            "o0,owner,quiet,accepted,no\no1,impersonator,quiet,accepted,no\n",
            header[:3] + "speaker,condition,verification,executable_action\n",
        ]
        for case in cases:
            with self.subTest(case=case[:40]):
                with self.assertRaises(ValueError):
                    self.summarize_csv(case)

    def test_cli_failure_is_clean_without_traceback_or_content_echo(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "bad.csv"
            path.write_text(
                "trial_id,speaker,condition,verification,executable_action\n"
                "SECRET_TOKEN,non_owner,quiet,accepted\n",
                encoding="utf-8",
            )
            proc = subprocess.run(
                [sys.executable, str(SCRIPT), str(path)],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(proc.returncode, 2)
            self.assertNotIn("Traceback", proc.stderr)
            self.assertNotIn("SECRET_TOKEN", proc.stderr)
            self.assertIn("error", proc.stderr)

    def test_cli_success_prints_json(self) -> None:
        rows = "trial_id,speaker,condition,verification,executable_action\no0,owner,quiet,accepted,yes\n"
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ok.csv"
            path.write_text(rows, encoding="utf-8")
            proc = subprocess.run(
                [sys.executable, str(SCRIPT), str(path)],
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(proc.returncode, 0)
        self.assertTrue(proc.stdout.strip().startswith("{"))
        self.assertFalse(proc.stderr)

    def test_header_is_exact_without_padding(self) -> None:
        with self.assertRaises(ValueError):
            self.summarize_csv(" trial_id,speaker,condition,verification,executable_action\n")

    def test_delimited_blank_record_is_not_silently_skipped(self) -> None:
        with self.assertRaises(ValueError):
            self.summarize_rows([",,,,"])

    def test_condition_results_stay_separate_inside_each_speaker_kind(self) -> None:
        result = self.summarize_rows([
            "o,owner,quiet,accepted,yes", "n,non_owner,quiet,rejected,no",
            "t,tv_replay,noise,rejected,no",
        ])
        self.assertEqual(result["by_speaker"]["owner"]["by_condition"]["quiet"]["trials"], 1)
        self.assertEqual(result["by_speaker"]["non_owner"]["by_condition"]["quiet"]["accepted"], 0)
        self.assertNotIn("noise", result["by_speaker"]["owner"]["by_condition"])


if __name__ == "__main__":
    unittest.main()
