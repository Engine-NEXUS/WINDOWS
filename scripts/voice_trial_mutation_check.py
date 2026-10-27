"""Assert that temporary evidence-tool mutations fail synthetic regressions."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    root = Path(__file__).resolve().parent
    files = ("speaker_trial_summary.py", "speaker_trial_summary_test.py",
             "voice_trial_evidence.py", "voice_trial_evidence_test.py")
    sources = {name: (root / name).read_text(encoding="utf-8") for name in files}
    legacy = {
        "legacy_evidence_qualifies": ('"numeric_targets_met": False,', '"numeric_targets_met": True,'),
        "padded_header_accepted": ('if tuple(header) != FIELDS:', 'if tuple(value.strip() for value in header) != FIELDS:'),
        "blank_record_dropped": ('if not values:', 'if not values or all(not value.strip() for value in values):'),
        "negative_classes_merged": ('if len(non_owner) == 1 else "mixed"', 'if len(non_owner) == 1 else "tv_replay"'),
    }
    trial = {
        "lock_off_qualifies": ('("lock_off", row["voice_lock_enabled"])', '("lock_off", True)'),
        "missing_capture_qualifies": ('("capture_unattributed", row["capture_session"] is not None)', '("capture_unattributed", True)'),
        "unverified_capture_qualifies": ('("verification_not_reached", row["verification_reached"])', '("verification_not_reached", True)'),
        "unavailable_verifier_qualifies": ('("verification_unavailable", row["verification"] != "unavailable")', '("verification_unavailable", True)'),
        "invalid_command_qualifies": ('("not_valid_executable_command", row["valid_executable_command"])', '("not_valid_executable_command", True)'),
        "enrollment_or_calibration_qualifies": ('("not_acceptance", row["partition"] == "acceptance")', '("not_acceptance", True)'),
        "playback_becomes_live_proof": ('live_present = any(row["speaker_kind"] == "live_non_owner" for row in negative)', 'live_present = True'),
        "299_negatives_pass": ('len(negative) >= 300', 'len(negative) >= 299'),
        "90_percent_owner_passes": ('owner_accepted / len(owner) >= .95', 'owner_accepted / len(owner) >= .90'),
        "excluded_locked_actions_hidden": ('for row in locked)', 'for row in qualified)'),
        "negative_action_gate_removed": ('and locked_negative_actions == 0', 'and True'),
        "unverified_action_gate_removed": ('and locked_unverified_actions == 0', 'and True'),
        "duplicate_capture_accepted": ('if key in sessions:', 'if False:'),
        "partition_leakage_accepted": ('if partitions[key].setdefault(token, row["partition"]) != row["partition"]:', 'if False:'),
        "inconsistent_success_accepted": ('if row["requested_action_succeeded"] is True and not (', 'if False and not ('),
        "declared_report_promotes_readiness": ('"production_readiness": "not_determined"', '"production_readiness": "ready"'),
        "report_leaks_trial_rows": ('"total_declared_trials": len(rows),', '"total_declared_trials": len(rows), "rows": rows,'),
    }

    def run(copies):
        with tempfile.TemporaryDirectory(prefix="nexus-trial-mutations-") as directory:
            for name, text in copies.items():
                (Path(directory) / name).write_text(text, encoding="utf-8")
            return subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", directory,
                                   "-p", "*_test.py"], capture_output=True, text=True, timeout=60)

    if run(sources).returncode:
        print("trial mutation baseline failed", file=sys.stderr)
        return 1
    results = []
    for target, mutations in (("speaker_trial_summary.py", legacy), ("voice_trial_evidence.py", trial)):
        for name, (before, after) in mutations.items():
            # The safety-wide denominator mutation intentionally changes all three safety counters.
            occurrences = sources[target].count(before)
            if occurrences != (3 if name == "excluded_locked_actions_hidden" else 1):
                print(f"stale mutation anchor: {name} ({occurrences})", file=sys.stderr)
                return 1
            result = run({**sources, target: sources[target].replace(before, after)})
            results.append({"name": name, "killed_by_assertion": result.returncode != 0 and "FAIL:" in result.stderr})
    print(json.dumps({"scope": "synthetic metadata in temporary copies; no device actions",
                      "baseline": "pass", "mutations": results,
                      "sha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in files}}, indent=2))
    return 0 if all(result["killed_by_assertion"] for result in results) else 1


if __name__ == "__main__":
    sys.exit(main())
