"""Summarize anonymized Voice Lock live trials from a legacy CSV file.

Each row of the legacy CSV is expected to have exactly the fixed columns
``trial_id`` in column 1, ``speaker`` in column 2 (owner, non_owner, or
tv_replay), ``condition`` in column 3, ``verification`` in column 4
(accepted, rejected, or uncertain), and ``executable_action`` in column 5
(yes or no). Rows with extra or fewer values, blank fields, or
whitespace-padded identifiers are refused. Do not record transcripts or
names.

This summarizer reads legacy CSV evidence only; it does not see capture,
lock, or provenance records. Its output is therefore descriptive and can
never establish that numeric qualification targets were met, that the
system is ready for deployment, or that live-speaker and replay
performance were measured on distinct, independently verified inputs.
"""

import argparse
import csv
import json
import math
import sys
from pathlib import Path

FIELDS = ("trial_id", "speaker", "condition", "verification", "executable_action")
SPEAKERS = ("owner", "non_owner", "tv_replay")
SPEAKER_SET = frozenset(SPEAKERS)
VERDICTS = {"accepted", "rejected", "uncertain"}
ACTIONS = {"yes", "no"}
EVIDENCE_SCOPE = "legacy_unqualified_csv"


def wilson_interval(successes: int, total: int) -> tuple[float, float] | None:
    if total == 0:
        return None
    z = 1.959963984540054
    p = successes / total
    denominator = 1 + z * z / total
    centre = (p + z * z / (2 * total)) / denominator
    margin = z * math.sqrt((p * (1 - p) + z * z / (4 * total)) / total) / denominator
    return (max(0.0, centre - margin), min(1.0, centre + margin))


def _kind_metrics(rows: list[dict[str, str]]) -> dict:
    total = len(rows)
    accepted = sum(row["verification"] == "accepted" for row in rows)
    actions = sum(row["executable_action"] == "yes" for row in rows)
    return {
        "trials": total,
        "accepted": accepted,
        "rejected_or_uncertain": total - accepted,
        "actions": actions,
        "acceptance_rate": accepted / total if total else None,
        "acceptance_wilson_95": wilson_interval(accepted, total),
    }


def _group_metrics(rows: list[dict[str, str]], key: str) -> dict[str, dict]:
    groups: dict[str, list[dict[str, str]]] = {}
    for row in rows:
        groups.setdefault(row[key], []).append(row)
    return {name: _kind_metrics(members) for name, members in sorted(groups.items())}


def _negative_class(speakers: set[str]) -> str:
    non_owner = speakers - {"owner"}
    if not non_owner:
        return "none"
    return non_owner.pop() if len(non_owner) == 1 else "mixed"


def summarize(path: Path) -> dict:
    with path.open(newline="", encoding="utf-8-sig") as handle:
        reader = csv.reader(handle)
        try:
            header = next(reader)
        except StopIteration as exc:
            raise ValueError(f"CSV header must be exactly: {','.join(FIELDS)}") from exc
        if tuple(header) != FIELDS:
            raise ValueError(f"CSV header must be exactly: {','.join(FIELDS)}")
        rows: list[dict[str, str]] = []
        seen: set[str] = set()
        for line_number, values in enumerate(reader, start=2):
            if not values:
                continue
            if len(values) != len(FIELDS):
                raise ValueError(
                    f"line {line_number}: expected exactly {len(FIELDS)} columns, got {len(values)}"
                )
            row = dict(zip(FIELDS, values))
            trial_id = row["trial_id"].strip()
            if not trial_id or trial_id != row["trial_id"]:
                raise ValueError(f"line {line_number}: missing, padded, or duplicate trial_id")
            if trial_id in seen:
                raise ValueError(f"line {line_number}: missing or duplicate trial_id")
            seen.add(trial_id)
            if row["speaker"] not in SPEAKER_SET or row["verification"] not in VERDICTS:
                raise ValueError(
                    f"line {line_number}: invalid speaker or verification"
                )
            condition = row["condition"]
            clean_condition = condition.strip()
            if not clean_condition or clean_condition != condition:
                raise ValueError(f"line {line_number}: missing or padded condition")
            if row["executable_action"] not in ACTIONS:
                raise ValueError(f"line {line_number}: invalid executable_action")
            rows.append(row)

    owner = [row for row in rows if row["speaker"] == "owner"]
    negative = [row for row in rows if row["speaker"] != "owner"]
    owner_accepted = sum(row["verification"] == "accepted" for row in owner)
    negative_accepted = sum(row["verification"] == "accepted" for row in negative)
    negative_actions = sum(row["executable_action"] == "yes" for row in negative)
    owner_actions_without_acceptance = sum(
        row["executable_action"] == "yes" and row["verification"] != "accepted" for row in owner
    )
    zero_action_upper = 1 - 0.05 ** (1 / len(negative)) if negative and negative_actions == 0 else None

    warnings = [
        "Legacy CSV evidence has no capture, lock, or provenance records, so it cannot "
        "establish that numeric qualification targets were met or that the system is ready.",
        "Legacy columns cannot prove inputs were distinct or independently verified, so "
        "distinct live-speaker versus replay performance cannot be established from this file.",
    ]

    return {
        "evidence_scope": EVIDENCE_SCOPE,
        "owner_trials": len(owner),
        "owner_accepted": owner_accepted,
        "owner_rejected_or_uncertain": len(owner) - owner_accepted,
        "owner_acceptance_rate": owner_accepted / len(owner) if owner else None,
        "owner_acceptance_wilson_95": wilson_interval(owner_accepted, len(owner)),
        "negative_trials": len(negative),
        "negative_accepted": negative_accepted,
        "negative_acceptance_rate": negative_accepted / len(negative) if negative else None,
        "negative_actions": negative_actions,
        "negative_action_rate": negative_actions / len(negative) if negative else None,
        "negative_acceptance_wilson_95": wilson_interval(negative_accepted, len(negative)),
        "negative_class": _negative_class({row["speaker"] for row in negative}) if negative else "none",
        "zero_negative_action_one_sided_95_upper": zero_action_upper,
        "owner_actions_without_acceptance": owner_actions_without_acceptance,
        "by_speaker": {
            speaker: {**_kind_metrics(members), "by_condition": _group_metrics(members, "condition")}
            for speaker in SPEAKERS
            if (members := [row for row in rows if row["speaker"] == speaker])
        },
        "by_condition": _group_metrics(rows, "condition"),
        "numeric_targets_met": False,
        "numeric_targets_blocked_reasons": [
            "legacy CSV evidence lacks capture, lock, and provenance records"
        ],
        "warnings": warnings,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("csv_path", type=Path)
    args = parser.parse_args()
    try:
        summary = summarize(args.csv_path)
    except (ValueError, csv.Error, UnicodeDecodeError, OSError) as exc:
        print(f"error: could not summarize trials CSV: {type(exc).__name__}", file=sys.stderr)
        raise SystemExit(2) from None
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
