"""Reconcile content-free [CAPTURE] records in a saved native NEXUS log.

An emitted transcript or dispatch entry is not an independently confirmed action.
Old logs cannot supply missing attempt identities retroactively.
"""
import argparse
from collections import Counter
import json
from pathlib import Path
import re

STAGES = {"requested", "started", "buffered", "queued", "received", "owner_verified",
          "owner_rejected", "owner_uncertain", "owner_unenrolled", "transcribing", "terminal"}
OUTCOMES = {"mic_held", "duplicate", "cancelled", "superseded", "receiver_unavailable",
            "stale", "empty_buffer", "phantom", "owner_hold", "stt_error", "filtered",
            "empty_transcript", "delivery_failed", "delivered", "abandoned"}
ANSI = re.compile(chr(27) + r"\[[0-9;]*m")


def positive_int(value):
    return type(value) is int and value > 0


def summarize_lines(lines, run_id=None, expected_starts=None):
    attempts = {}
    sessions = {}
    errors = []
    dispatches = set()
    outcomes = Counter()
    for number, line in enumerate(lines, 1):
        line = ANSI.sub("", line)
        dispatch = "[CAPTURE-DISPATCH] " in line
        marker = "[CAPTURE-DISPATCH] " if dispatch else "[CAPTURE] "
        if marker not in line:
            continue
        prefix, payload = line.split(marker, 1)
        # Existing logs can contain quoted user/STT text. A marker inside that
        # text is not a native audit record and must not manufacture a trial.
        if prefix.strip() and not re.fullmatch(
                r"[0-9]{4}-[0-9]{2}-[0-9]{2}T\S+\s+INFO\s+(?:capture_audit:\s+)?", prefix):
            continue
        try:
            event, _ = json.JSONDecoder().raw_decode(payload)
            if not isinstance(event, dict):
                raise ValueError()
            run = event.get("run_id")
            if not isinstance(run, str) or not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", run):
                raise ValueError()
            if run_id is not None and run != run_id:
                continue
            session = event.get("session")
            if session is not None and not positive_int(session):
                raise ValueError()
            if dispatch:
                if not positive_int(session) or event.get("event") != "dispatch_received":
                    raise ValueError()
                dispatches.add((run, session))
                continue
            ident = event.get("attempt_id")
            stage = event.get("event")
            if not positive_int(ident) or stage not in STAGES:
                raise ValueError()
            elapsed = event.get("elapsed_ms")
            if type(elapsed) is not int or elapsed < 0:
                raise ValueError()
            outcome = event.get("outcome")
            if (stage == "terminal" and outcome not in OUTCOMES) or (stage != "terminal" and outcome is not None):
                raise ValueError()
        except (ValueError, TypeError, json.JSONDecodeError):
            errors.append(f"line {number}: malformed or unknown audit record")
            continue
        key = (run, ident)
        if stage == "requested":
            if key in attempts:
                errors.append(f"line {number}: duplicate request {run}/{ident}")
                continue
            attempts[key] = {"terminal": False, "elapsed": elapsed, "session": None, "started": False, "outcome": None}
        elif key not in attempts:
            errors.append(f"line {number}: record without request {run}/{ident}")
            continue
        attempt = attempts[key]
        if attempt["terminal"]:
            reason = "duplicate terminal" if stage == "terminal" else "stage after terminal"
            errors.append(f"line {number}: {reason} {run}/{ident}")
            continue
        if elapsed < attempt["elapsed"]:
            errors.append(f"line {number}: decreasing elapsed time {run}/{ident}")
        attempt["elapsed"] = elapsed
        if stage == "started":
            if attempt["started"] or session is None:
                errors.append(f"line {number}: invalid or repeated capture start {run}/{ident}")
            attempt["started"] = True
        elif stage not in {"requested", "terminal"} and not attempt["started"]:
            errors.append(f"line {number}: stage before capture start {run}/{ident}")
        if session is not None:
            if attempt["session"] is not None and attempt["session"] != session:
                errors.append(f"line {number}: session changed {run}/{ident}")
            attempt["session"] = session
            previous = sessions.setdefault((run, session), key)
            if previous != key:
                errors.append(f"line {number}: session reused {run}/{session}")
        if stage == "terminal":
            if outcome not in {"mic_held", "duplicate", "abandoned"} and not attempt["started"]:
                errors.append(f"line {number}: terminal without capture start {run}/{ident}")
            attempt["terminal"] = True
            attempt["outcome"] = outcome
            outcomes[outcome] += 1
    unfinished = [{"run_id": run, "attempt_id": ident} for (run, ident), value in attempts.items()
                  if not value["terminal"]]
    uncorrelated = sorted(dispatches - sessions.keys())
    unexpected_dispatches = [key for key in dispatches if key in sessions
                             and attempts[sessions[key]]["outcome"] != "delivered"]
    started = sum(attempt["started"] for attempt in attempts.values())
    count_matches = None if expected_starts is None else started == expected_starts
    return {
        "requests": len(attempts), "terminals": sum(outcomes.values()),
        "captures_started": started,
        "expected_starts": expected_starts, "expected_count_match": count_matches,
        "outcomes": dict(sorted(outcomes.items())), "unfinished": unfinished, "errors": errors,
        "dispatch_entries": len(dispatches),
        "uncorrelated_dispatches": [{"run_id": run, "session": session} for run, session in uncorrelated],
        "unexpected_dispatches": [{"run_id": run, "session": session} for run, session in sorted(unexpected_dispatches)],
        "action_successes_confirmed": 0,
        "complete": bool(attempts) and not unfinished and not errors and not uncorrelated and not unexpected_dispatches and count_matches is not False,
        "note": "Complete means lifecycle accounting only; no speaker-accuracy or executed-action claim.",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--run-id", help="Restrict to one exact process run ID")
    parser.add_argument("--expected-starts", type=int, help="Physical starts expected in this controlled batch")
    args = parser.parse_args()
    if args.expected_starts is not None and args.expected_starts < 1:
        parser.error("--expected-starts must be positive")
    with args.log.open(encoding="utf-8-sig", errors="replace") as handle:
        report = summarize_lines(handle, args.run_id, args.expected_starts)
    print(json.dumps(report, indent=2))
    return 0 if report["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
