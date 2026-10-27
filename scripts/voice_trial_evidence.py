"""Report declared offline Voice Lock trials; never certify production readiness."""
import argparse
from collections import Counter
import hashlib
import json
import re
import sys
from pathlib import Path

from speaker_trial_summary import wilson_interval

ROOT_FIELDS = {"schema_version", "source_revision", "source_hash", "build_identity", "rows"}
FIELDS = {
    "trial_id", "run_id", "capture_session", "speaker_id", "recording_group", "sample_id",
    "speaker_kind", "condition", "partition", "wake_path", "voice_lock_enabled",
    "verification_reached", "verification", "valid_executable_command", "executable_action",
    "intent_match", "target_match", "requested_action_succeeded",
}
KINDS = ("owner", "live_non_owner", "owner_replay", "other_speaker_playback", "synthetic_playback")
BOOLS = {"voice_lock_enabled", "verification_reached", "valid_executable_command", "executable_action"}
OPTIONAL_BOOLS = {"intent_match", "target_match", "requested_action_succeeded"}
IDENTIFIERS = FIELDS - BOOLS - OPTIONAL_BOOLS - {"capture_session"}


def exact_fields(value, fields):
    if not isinstance(value, dict) or set(value) != fields:
        raise ValueError("invalid evidence fields")


def validate(data):
    exact_fields(data, ROOT_FIELDS)
    if type(data["schema_version"]) is not int or data["schema_version"] != 1:
        raise ValueError("unsupported schema")
    for key, width in (("source_revision", 40), ("source_hash", 64), ("build_identity", 16)):
        if not isinstance(data[key], str) or not re.fullmatch(f"[0-9a-f]{{{width}}}", data[key]):
            raise ValueError("invalid provenance declaration")
    if not isinstance(data["rows"], list):
        raise ValueError("invalid rows")
    ids, sessions, owners, families, lineage = set(), set(), set(), {}, {}
    partitions = {"sample_id": {}, "recording_group": {}, "non_owner": {}}
    for row in data["rows"]:
        exact_fields(row, FIELDS)
        for key in IDENTIFIERS:
            if not isinstance(row[key], str) or not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,95}", row[key]):
                raise ValueError("invalid anonymous token")
        for key in BOOLS:
            if type(row[key]) is not bool:
                raise ValueError("expected boolean")
        for key in OPTIONAL_BOOLS:
            if row[key] is not None and type(row[key]) is not bool:
                raise ValueError("expected boolean or null")
        session = row["capture_session"]
        if session is not None and (type(session) is not int or not 0 < session < 2**64):
            raise ValueError("invalid capture session")
        if row["trial_id"] in ids:
            raise ValueError("duplicate trial")
        ids.add(row["trial_id"])
        if session is not None:
            key = (row["run_id"], session)
            if key in sessions:
                raise ValueError("duplicate capture")
            sessions.add(key)
        if row["speaker_kind"] not in KINDS or row["partition"] not in {"enrollment", "calibration", "acceptance"}:
            raise ValueError("invalid class or partition")
        if row["wake_path"] not in {"neural", "hotkey", "followup"} or row["verification"] not in {"accepted", "rejected", "uncertain", "unavailable"}:
            raise ValueError("invalid path or verification")
        if row["requested_action_succeeded"] is True and not (
            row["executable_action"] and row["intent_match"] is True and row["target_match"] is True
        ):
            raise ValueError("inconsistent action success")
        family = "owner" if row["speaker_kind"] in {"owner", "owner_replay"} else "non_owner"
        if families.setdefault(row["speaker_id"], family) != family:
            raise ValueError("inconsistent speaker identity")
        if family == "owner":
            owners.add(row["speaker_id"])
        sample_lineage = (row["speaker_id"], row["recording_group"])
        if lineage.setdefault(row["sample_id"], sample_lineage) != sample_lineage:
            raise ValueError("inconsistent sample lineage")
        for key in ("sample_id", "recording_group", "non_owner"):
            if key == "non_owner":
                if family != "non_owner":
                    continue
                token = row["speaker_id"]
            else:
                token = row[key]
            if partitions[key].setdefault(token, row["partition"]) != row["partition"]:
                raise ValueError("partition leakage")
    if len(owners) > 1:
        raise ValueError("multiple owner identities")


def exclusion_reasons(row):
    return [name for name, passes in (
        ("not_acceptance", row["partition"] == "acceptance"),
        ("lock_off", row["voice_lock_enabled"]),
        ("capture_unattributed", row["capture_session"] is not None),
        ("verification_not_reached", row["verification_reached"]),
        ("verification_unavailable", row["verification"] != "unavailable"),
        ("not_valid_executable_command", row["valid_executable_command"]),
    ) if not passes]


def metrics(rows):
    total = len(rows)
    accepted = sum(row["verification"] == "accepted" for row in rows)
    actions = sum(row["executable_action"] for row in rows)
    return {
        "trials": total, "accepted": accepted, "rejected_or_uncertain": total - accepted,
        "actions": actions, "acceptance_rate": accepted / total if total else None,
        "acceptance_wilson_95": wilson_interval(accepted, total),
        "action_rate": actions / total if total else None,
        "distinct_speakers": len({row["speaker_id"] for row in rows}),
        "distinct_recording_groups": len({row["recording_group"] for row in rows}),
        "distinct_samples": len({row["sample_id"] for row in rows}),
    }


def measured(rows, field):
    return {"matched_or_succeeded": sum(row[field] is True for row in rows),
            "mismatched_or_failed": sum(row[field] is False for row in rows),
            "unmeasured": sum(row[field] is None for row in rows)}


def summarize(data):
    validate(data)
    rows = data["rows"]
    qualified = [row for row in rows if not exclusion_reasons(row)]
    owner = [row for row in qualified if row["speaker_kind"] == "owner"]
    negative = [row for row in qualified if row["speaker_kind"] != "owner"]
    verified_owner = [row for row in owner if row["verification"] == "accepted"]
    locked = [row for row in rows if row["voice_lock_enabled"]]
    locked_negative_actions = sum(row["executable_action"] and row["speaker_kind"] != "owner" for row in locked)
    locked_unverified_actions = sum(row["executable_action"] and not (
        row["verification_reached"] and row["capture_session"] is not None and row["verification"] == "accepted"
    ) for row in locked)
    mismatched_actions = sum(row["executable_action"] and (row["intent_match"] is False or row["target_match"] is False) for row in locked)
    by_kind = {}
    for kind in KINDS:
        selected = [row for row in qualified if row["speaker_kind"] == kind]
        by_kind[kind] = {**metrics(selected), **{
            f"by_{key}": {value: metrics([row for row in selected if row[key] == value])
                          for value in sorted({row[key] for row in selected})}
            for key in ("condition", "wake_path")
        }}
    owner_accepted = len(verified_owner)
    negative_actions = sum(row["executable_action"] for row in negative)
    live_present = any(row["speaker_kind"] == "live_non_owner" for row in negative)
    return {
        "schema_version": 1, **{key: data[key] for key in ("source_revision", "source_hash", "build_identity")},
        "input_sha256": hashlib.sha256(json.dumps(data, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest(),
        "scope": "declared_trial_metadata_only", "production_readiness": "not_determined",
        "total_declared_trials": len(rows), "qualified_trials": len(qualified),
        "excluded_trials": len(rows) - len(qualified),
        "exclusion_reasons": dict(Counter(reason for row in rows for reason in exclusion_reasons(row))),
        "by_speaker_kind": by_kind,
        "mixed_negative_action_screen": {**metrics(negative),
            "zero_action_one_sided_95_upper": 1 - .05 ** (1 / len(negative)) if negative and not negative_actions else None},
        "unfamiliar_live_evidence_present": live_present,
        "safety_all_locked_rows": {"non_owner_actions": locked_negative_actions,
                                   "actions_without_positive_verification": locked_unverified_actions,
                                   "actions_with_measured_intent_or_target_mismatch": mismatched_actions},
        "command_fidelity_verified_owner": {"trials": len(verified_owner),
            "requested_action": measured(verified_owner, "requested_action_succeeded"),
            "intent": measured(verified_owner, "intent_match"), "target": measured(verified_owner, "target_match")},
        "numeric_targets_met": bool(owner) and owner_accepted / len(owner) >= .95 and len(negative) >= 300
                               and live_present and locked_negative_actions == 0 and locked_unverified_actions == 0,
        "warnings": [
            "All fields are declarations: independent capture-log reconciliation, native lock readback and observed device actions are required.",
            "Numeric screen is not production readiness; owner command failures, recovery, full shared gates and exact-release proof remain separate.",
            "Intervals assume independent trials. Repeated samples, sessions and speakers create dependence; playback is not unfamiliar live-speaker proof.",
            "Excluded locked actions still count in safety failures. Mixed negative rates combine classes; use separate speaker-kind results.",
        ],
    }


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    args = parser.parse_args()
    try:
        with args.evidence.open(encoding="utf-8-sig") as handle:
            data = json.load(handle, object_pairs_hook=unique_object)
        output = json.dumps(summarize(data), indent=2, allow_nan=False)
    except (OSError, UnicodeError, ValueError, RecursionError):
        print("trial evidence error: invalid or unreadable input; check schema and qualification fields", file=sys.stderr)
        return 2
    print(output)
    return 0


if __name__ == "__main__":
    sys.exit(main())
