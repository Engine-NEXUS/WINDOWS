"""Score explicit offline embedding exports with isolated owner enrollment.

No model inference, audio capture, profile access or runtime changes.
"""
import argparse
import hashlib
import json
import math
import re
import sys
from pathlib import Path

import speaker_benchmark as benchmark

MODEL_FIELDS = benchmark.MODEL_FIELDS | {"embedding_dimension", "preprocessing_sha256"}
ROW_FIELDS = (benchmark.ROW_FIELDS - {"score"}) | {"embedding"}
POLICY = "max_enrollment_cosine_v1"


def normalize(vector, dimension):
    if not isinstance(vector, list) or len(vector) != dimension:
        raise ValueError("embedding dimension mismatch")
    for value in vector:
        benchmark.number(value, "embedding component", -sys.float_info.max, sys.float_info.max)
    scale = max(abs(value) for value in vector)
    if scale == 0:
        raise ValueError("zero embedding")
    scaled = [value / scale for value in vector]
    norm = math.hypot(*scaled)
    return [value / norm for value in scaled]


def score_experiment(data):
    """Return benchmark input and vector-free extraction/enrollment provenance."""
    benchmark.fields(data, benchmark.ROOT_FIELDS, "embedding experiment")
    if type(data["schema_version"]) is not int or data["schema_version"] != 1:
        raise ValueError("schema_version must be integer 1")
    if not isinstance(data["models"], list) or not data["models"]:
        raise ValueError("models must be a nonempty list")
    models, definitions = {}, {}
    for model in data["models"]:
        benchmark.fields(model, MODEL_FIELDS, "embedding model")
        benchmark.identifier(model["id"], "model id")
        if model["id"] in models:
            raise ValueError("duplicate model id")
        dimension = model["embedding_dimension"]
        if type(dimension) is not int or not 2 <= dimension <= 4096:
            raise ValueError("embedding_dimension must be integer 2..4096")
        if not isinstance(model["preprocessing_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", model["preprocessing_sha256"]):
            raise ValueError("invalid preprocessing_sha256")
        models[model["id"]] = {}
        definitions[model["id"]] = model
    if not isinstance(data["rows"], list) or not data["rows"]:
        raise ValueError("rows must be a nonempty list")
    samples, partitions, vectors = {}, {"sample_id": {}, "recording_group": {}}, {}
    for row in data["rows"]:
        benchmark.fields(row, ROW_FIELDS, "embedding row")
        for key in ROW_FIELDS - {"embedding", "latency_ms", "peak_memory_mb"}:
            benchmark.identifier(row[key], f"row {key}")
        model_id = row["model_id"]
        if model_id not in models:
            raise ValueError("unknown model_id")
        if row["speaker_kind"] not in benchmark.KINDS or row["partition"] not in {"enrollment", "calibration", "acceptance"}:
            raise ValueError("invalid speaker_kind or partition")
        if row["partition"] == "enrollment" and row["speaker_kind"] != "owner":
            raise ValueError("enrollment must be owner only")
        benchmark.number(row["latency_ms"], "latency_ms", 0)
        benchmark.number(row["peak_memory_mb"], "peak_memory_mb", 0)
        model_rows = models[model_id]
        if row["trial_id"] in model_rows:
            raise ValueError("duplicate trial_id within model")
        sample_key = (model_id, row["sample_id"])
        if sample_key in samples:
            raise ValueError("duplicate sample_id within model")
        samples[sample_key] = True
        for key in partitions:
            previous = partitions[key].setdefault(row[key], row["partition"])
            if previous != row["partition"]:
                raise ValueError(f"split leakage: {key}")
        model_rows[row["trial_id"]] = row
        vectors[(model_id, row["trial_id"])] = normalize(row["embedding"], definitions[model_id]["embedding_dimension"])
    baseline = next(iter(models.values()))
    metadata = ROW_FIELDS - {"model_id", "embedding", "latency_ms", "peak_memory_mb"}
    for model_rows in models.values():
        if set(model_rows) != set(baseline):
            raise ValueError("models must have identical complete trial sets")
        if any(row[key] != baseline[trial][key] for trial, row in model_rows.items() for key in metadata):
            raise ValueError("models must have identical complete metadata")
    enrollment = [row for row in baseline.values() if row["partition"] == "enrollment"]
    if len(enrollment) < 3:
        raise ValueError("at least three distinct owner enrollment samples required")
    owners = {row["speaker_id"] for row in baseline.values() if row["speaker_kind"] in {"owner", "owner_replay"}}
    if len(owners) != 1:
        raise ValueError("one enrolled owner identity required")
    scored_rows, enrollment_reports = [], []
    for model_id, model_rows in models.items():
        template_rows = [row for row in model_rows.values() if row["partition"] == "enrollment"]
        template = [vectors[(model_id, row["trial_id"])] for row in template_rows]
        for row in model_rows.values():
            if row["partition"] == "enrollment":
                continue
            vector = vectors[(model_id, row["trial_id"])]
            score = max(math.fsum(a * b for a, b in zip(vector, reference)) for reference in template)
            scored_rows.append({**{key: value for key, value in row.items() if key != "embedding"},
                                "score": min(1.0, max(-1.0, score))})
        enrollment_reports.append({
            "model_id": model_id,
            "preprocessing_sha256": definitions[model_id]["preprocessing_sha256"],
            "embedding_dimension": definitions[model_id]["embedding_dimension"],
            "enrollment_samples": len(template_rows),
            "enrollment_recording_groups": len({row["recording_group"] for row in template_rows}),
        })
    score_input = {
        "schema_version": 1, "experiment_id": data["experiment_id"], "source_revision": data["source_revision"],
        "models": [{key: model[key] for key in benchmark.MODEL_FIELDS} for model in data["models"]],
        "rows": scored_rows,
    }
    benchmark.validate(score_input)
    provenance = {
        "input_sha256": hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")).hexdigest(),
        "scoring_policy": POLICY, "models": enrollment_reports,
        "scope": "offline_exported_embeddings_only",
        "native_inference_and_float_parity": "unmeasured",
        "resource_measurements": "declared_upstream_extraction_only; scoring_overhead_unmeasured",
    }
    return score_input, provenance


def report(data):
    score_input, provenance = score_experiment(data)
    result = benchmark.summarize(score_input)
    result["embedding_provenance"] = provenance
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("experiment", type=Path)
    args = parser.parse_args()
    try:
        with args.experiment.open(encoding="utf-8-sig") as handle:
            data = json.load(handle, object_pairs_hook=benchmark.unique_object)
        output = json.dumps(report(data), indent=2, allow_nan=False)
    except (OSError, UnicodeError, ValueError, RecursionError):
        print("embedding score error: invalid or unreadable experiment; check schema and enrollment splits", file=sys.stderr)
        return 2
    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
