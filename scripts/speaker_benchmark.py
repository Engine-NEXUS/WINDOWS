"""Validate and summarize frozen, offline speaker-verifier score experiments.

No audio, model execution, threshold installation, or production verdicts.
See docs/testing/speaker-verifier-benchmark.md for the versioned input contract.
"""

import argparse
import hashlib
import json
import math
import re
import statistics
import sys
from pathlib import Path

ROOT_FIELDS = {"schema_version", "experiment_id", "source_revision", "models", "rows"}
MODEL_FIELDS = {"id", "license", "artifact_sha256", "score_direction"}
ROW_FIELDS = {
    "trial_id", "model_id", "sample_id", "recording_group", "speaker_id",
    "speaker_kind", "condition", "partition", "score", "latency_ms", "peak_memory_mb",
}
KINDS = ("owner", "live_non_owner", "owner_replay", "other_speaker_playback", "synthetic_playback")
METRICS = {"score", "latency_ms", "peak_memory_mb"}


def fields(value, expected, label):
    if not isinstance(value, dict) or set(value) != expected:
        raise ValueError(f"{label}: fields must be {', '.join(sorted(expected))}")


def text(value, label):
    if not isinstance(value, str) or not value.strip() or value != value.strip():
        raise ValueError(f"{label}: expected nonblank trimmed text")


def identifier(value, label):
    if not isinstance(value, str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,95}", value):
        raise ValueError(f"{label}: expected anonymous token, not a path or free text")


def number(value, label, low, high=math.inf):
    if type(value) not in (int, float):
        raise ValueError(f"{label}: expected finite number in range")
    try:
        valid = math.isfinite(value) and low <= value <= high
    except OverflowError:
        valid = False
    if not valid:
        raise ValueError(f"{label}: expected finite number in range")


def validate(data):
    fields(data, ROOT_FIELDS, "experiment")
    if type(data["schema_version"]) is not int or data["schema_version"] != 1:
        raise ValueError("schema_version must be integer 1")
    identifier(data["experiment_id"], "experiment_id")
    if not isinstance(data["source_revision"], str) or not re.fullmatch(r"[0-9a-f]{40}", data["source_revision"]):
        raise ValueError("source_revision must be a lowercase 40-character Git hash")
    if not isinstance(data["models"], list) or not data["models"]:
        raise ValueError("models must be a nonempty list")
    models = {}
    for model in data["models"]:
        fields(model, MODEL_FIELDS, "model")
        identifier(model["id"], "model id")
        text(model["license"], "model license")
        if not re.fullmatch(r"[A-Za-z0-9(). +_-]{1,160}", model["license"]):
            raise ValueError("license must be a short declared license expression, not a path")
        if model["id"] in models:
            raise ValueError("duplicate model id")
        if not isinstance(model["artifact_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", model["artifact_sha256"]):
            raise ValueError("artifact_sha256 must be a lowercase SHA256")
        if model["score_direction"] != "higher_is_owner":
            raise ValueError("only higher_is_owner scores are supported")
        models[model["id"]] = {}
    if not isinstance(data["rows"], list) or not data["rows"]:
        raise ValueError("rows must be a nonempty list")
    splits = {"sample_id": {}, "recording_group": {}, "non_owner_speaker": {}}
    samples = {}
    speaker_families = {}
    for row in data["rows"]:
        fields(row, ROW_FIELDS, "row")
        for key in ROW_FIELDS - METRICS:
            identifier(row[key], f"row {key}")
        if row["model_id"] not in models:
            raise ValueError("unknown model_id")
        if row["speaker_kind"] not in KINDS or row["partition"] not in {"calibration", "acceptance"}:
            raise ValueError("invalid speaker_kind or partition")
        family = "owner" if row["speaker_kind"] in {"owner", "owner_replay"} else "non_owner"
        if speaker_families.setdefault(row["speaker_id"], family) != family:
            raise ValueError("inconsistent speaker identity: owner and non-owner labels overlap")
        number(row["score"], "score", -1, 1)
        number(row["latency_ms"], "latency_ms", 0)
        number(row["peak_memory_mb"], "peak_memory_mb", 0)
        model_rows = models[row["model_id"]]
        if row["trial_id"] in model_rows:
            raise ValueError("duplicate trial_id within model")
        model_rows[row["trial_id"]] = row
        for key in ("sample_id", "recording_group", "non_owner_speaker"):
            if key == "non_owner_speaker":
                if row["speaker_kind"] not in {"live_non_owner", "other_speaker_playback", "synthetic_playback"}:
                    continue
                value = row["speaker_id"]
            else:
                value = row[key]
            previous = splits[key].setdefault(value, row["partition"])
            if previous != row["partition"]:
                raise ValueError(f"split leakage: {key}")
        # Re-scoring a recording cannot silently relabel its speaker or condition.
        metadata = tuple(row[k] for k in ("recording_group", "speaker_id", "speaker_kind", "condition", "partition"))
        if samples.setdefault(row["sample_id"], metadata) != metadata:
            raise ValueError("inconsistent metadata for sample_id")
    baseline = next(iter(models.values()))
    for model_rows in models.values():
        if set(model_rows) != set(baseline):
            raise ValueError("models must have identical trial sets")
        for trial, row in model_rows.items():
            if any(row[key] != baseline[trial][key] for key in ROW_FIELDS - METRICS - {"model_id"}):
                raise ValueError("models must have identical trial metadata")
        for partition in ("calibration", "acceptance"):
            kinds = {r["speaker_kind"] for r in model_rows.values() if r["partition"] == partition}
            if not {"owner", "live_non_owner"} <= kinds:
                raise ValueError(f"{partition} requires owner and live_non_owner trials")
    return models


def wilson(successes, total):
    if not total:
        return None
    z = 1.959963984540054
    p = successes / total
    denominator = 1 + z * z / total
    centre = (p + z * z / (2 * total)) / denominator
    margin = z * math.sqrt((p * (1 - p) + z * z / (4 * total)) / total) / denominator
    return [max(0.0, centre - margin), min(1.0, centre + margin)]


def choose_threshold(calibration):
    """Lowest observed threshold above every live impostor calibration score."""
    negatives = [r["score"] for r in calibration if r["speaker_kind"] == "live_non_owner"]
    floor = max(negatives)
    candidates = sorted({r["score"] for r in calibration if r["score"] > floor})
    return candidates[0] if candidates else math.nextafter(max(r["score"] for r in calibration), math.inf)


def metrics(rows, threshold, kind):
    count = len(rows)
    accepted = sum(r["score"] >= threshold for r in rows)
    latency = sorted(r["latency_ms"] for r in rows)
    result = {
        "trials": count, "accepted": accepted, "rejected": count - accepted,
        "acceptance_rate": accepted / count if count else None,
        "acceptance_wilson_95": wilson(accepted, count),
        "distinct_speakers": len({r["speaker_id"] for r in rows}),
        "distinct_recording_groups": len({r["recording_group"] for r in rows}),
        "distinct_samples": len({r["sample_id"] for r in rows}),
        # Nonnegative finite inputs can overflow (low + high) / 2.
        "latency_ms_median": (statistics.median_low(latency)
                              + (statistics.median_high(latency) - statistics.median_low(latency)) / 2)
                             if count else None,
        "latency_ms_p95_nearest_rank": latency[math.ceil(.95 * count) - 1] if count else None,
        "peak_memory_mb_max": max((r["peak_memory_mb"] for r in rows), default=None),
    }
    if kind == "owner":
        result.update(frr=(count - accepted) / count if count else None,
                      frr_wilson_95=wilson(count - accepted, count))
    if kind == "live_non_owner":
        result.update(far=accepted / count if count else None,
                      far_wilson_95=wilson(accepted, count),
                      zero_acceptance_one_sided_95_upper=1 - .05 ** (1 / count) if count and not accepted else None)
    return result


def summarize(data):
    models = validate(data)
    reports = []
    for model in data["models"]:
        rows = list(models[model["id"]].values())
        calibration = [r for r in rows if r["partition"] == "calibration"]
        acceptance = [r for r in rows if r["partition"] == "acceptance"]
        threshold = choose_threshold(calibration)
        by_kind = {}
        for kind in KINDS:
            selected = [r for r in acceptance if r["speaker_kind"] == kind]
            by_kind[kind] = metrics(selected, threshold, kind)
            by_kind[kind]["by_condition"] = {
                condition: metrics([r for r in selected if r["condition"] == condition], threshold, kind)
                for condition in sorted({r["condition"] for r in selected})
            }
        reports.append({
            **model, "threshold": threshold, "frozen_threshold_source": "calibration_only",
            "calibration": {kind: metrics([r for r in calibration if r["speaker_kind"] == kind], threshold, kind)
                            for kind in KINDS},
            "acceptance": by_kind,
        })
    return {
        "schema_version": 1, "experiment_id": data["experiment_id"], "source_revision": data["source_revision"],
        "input_sha256": hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")).hexdigest(),
        "evidence_scope": "offline_scores_only", "action_success": "unmeasured",
        "warnings": [
            "Intervals assume independent rows; repeated recordings, speakers and sessions create dependence.",
            "Distinct group counts do not establish independence. No live device or executable-action proof.",
            "Hashes and license labels are declarations, not artifact or license audits. No automatic winner.",
        ],
        "models": reports,
    }


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON object field")
        result[key] = value
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("experiment", type=Path)
    args = parser.parse_args()
    try:
        with args.experiment.open(encoding="utf-8-sig") as handle:
            data = json.load(handle, object_pairs_hook=unique_object)
        report = summarize(data)
    except (OSError, UnicodeError):
        print("benchmark error: cannot read experiment file", file=sys.stderr)
        return 2
    except (ValueError, RecursionError):
        # Avoid echoing JSON source, identifiers, paths or speech accidentally supplied as input.
        print("benchmark error: invalid experiment; check the documented schema and split rules", file=sys.stderr)
        return 2
    print(json.dumps(report, indent=2, allow_nan=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
