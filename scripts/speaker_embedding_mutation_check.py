"""Mutation-check offline score tools on temporary synthetic copies only."""
import hashlib
import json
import subprocess
import sys
import tempfile
from pathlib import Path


def main():
    root = Path(__file__).resolve().parent
    names = ("speaker_benchmark.py", "speaker_benchmark_test.py",
             "speaker_embedding_scores.py", "speaker_embedding_scores_test.py")
    sources = {name: (root / name).read_text(encoding="utf-8") for name in names}
    benchmark_mutants = {
        "allow_cross_split": ('if previous != row["partition"]:', 'if False:'),
        "tune_on_acceptance": ('calibration = [r for r in rows if r["partition"] == "calibration"]', 'calibration = rows'),
        "accept_impostor_tie": ('if r["score"] > floor', 'if r["score"] >= floor'),
        "allow_incomplete_classes": ('if not {"owner", "live_non_owner"} <= kinds:', 'if False:'),
        "allow_unpaired_models": ('if set(model_rows) != set(baseline):', 'if False:'),
        "allow_identity_relabelling": ('if speaker_families.setdefault(row["speaker_id"], family) != family:', 'if False:'),
        "change_acceptance_boundary": ('sum(r["score"] >= threshold for r in rows)', 'sum(r["score"] > threshold for r in rows)'),
        "disable_input_fingerprint": ('hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")).hexdigest()', '"constant"'),
        "restore_overflowing_median": ('(statistics.median_low(latency)\n                              + (statistics.median_high(latency) - statistics.median_low(latency)) / 2)', 'statistics.median(latency)'),
    }
    embedding_mutants = {
        "allow_enrollment_group_leak": ('if previous != row["partition"]:', 'if False:'),
        "allow_impostor_enrollment": ('if row["partition"] == "enrollment" and row["speaker_kind"] != "owner":', 'if False:'),
        "allow_two_enrollment_clips": ('if len(enrollment) < 3:', 'if len(enrollment) < 1:'),
        "allow_multiple_owners": ('if len(owners) != 1:', 'if False:'),
        "allow_duplicate_enrollment_sample": ('if sample_key in samples:', 'if False:'),
        "use_all_trials_in_template": ('template_rows = [row for row in model_rows.values() if row["partition"] == "enrollment"]', 'template_rows = list(model_rows.values())'),
        "use_min_instead_of_max_cosine": ('score = max(math.fsum(a * b for a, b in zip(vector, reference)) for reference in template)', 'score = min(math.fsum(a * b for a, b in zip(vector, reference)) for reference in template)'),
        "remove_scaled_normalization": ('scaled = [value / scale for value in vector]', 'scaled = list(vector)'),
        "leak_embeddings_in_report": ('"scope": "offline_exported_embeddings_only",', '"scope": "offline_exported_embeddings_only", "rows": data["rows"],'),
    }

    def run(copies):
        with tempfile.TemporaryDirectory(prefix="nexus-embedding-mutations-") as directory:
            for name, source in copies.items():
                (Path(directory) / name).write_text(source, encoding="utf-8")
            return subprocess.run([sys.executable, "-m", "unittest", "discover", "-s", directory,
                                   "-p", "*_test.py"], capture_output=True, text=True, timeout=60)

    baseline = run(sources)
    if baseline.returncode:
        print("mutation check: baseline tests fail", file=sys.stderr)
        return 1
    outcomes = []
    for target, mutants in (("speaker_benchmark.py", benchmark_mutants),
                            ("speaker_embedding_scores.py", embedding_mutants)):
        for name, (before, after) in mutants.items():
            if sources[target].count(before) != 1:
                print(f"mutation check: stale anchor for {name}", file=sys.stderr)
                return 1
            copies = {**sources, target: sources[target].replace(before, after)}
            result = run(copies)
            outcomes.append({"name": name, "target": target,
                             "killed_by_assertion": result.returncode != 0 and "FAIL:" in result.stderr})
    print(json.dumps({
        "scope": "temporary synthetic copies; no runtime, recordings or models",
        "baseline": "pass", "mutations": outcomes,
        "sha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in names},
    }, indent=2))
    return 0 if all(result["killed_by_assertion"] for result in outcomes) else 1


if __name__ == "__main__":
    sys.exit(main())
