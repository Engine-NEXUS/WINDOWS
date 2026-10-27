"""Reconcile native capture evidence without inferring action success."""
import json
import unittest
from capture_audit_summary import summarize_lines


def event(attempt, stage, *, session=None, outcome=None, run="run-a", elapsed=0):
    return "[CAPTURE] " + json.dumps({
        "run_id": run, "attempt_id": attempt, "session": session,
        "origin": "direct", "event": stage, "outcome": outcome,
        "utc_ms": 1234567890, "elapsed_ms": elapsed,
        "build_identity": None, "source_hash": None,
        "executable_matches_manifest": False,
    })


class CaptureAuditSummaryTests(unittest.TestCase):
    def test_ten_requests_including_blocked_cancelled_and_filtered(self):
        lines = []
        outcomes = ["mic_held", "duplicate", "cancelled", "superseded", "receiver_unavailable",
                    "stale", "phantom", "owner_hold", "filtered", "delivered"]
        for ident, outcome in enumerate(outcomes, 1):
            lines.append(event(ident, "requested"))
            session = None if outcome in {"mic_held", "duplicate"} else ident
            if session is not None:
                lines.append(event(ident, "started", session=session))
            lines.append(event(ident, "terminal", session=session, outcome=outcome, elapsed=3))
        report = summarize_lines(lines)
        self.assertTrue(report["complete"])
        self.assertEqual(report["requests"], 10)
        self.assertEqual(report["terminals"], 10)
        self.assertEqual(report["captures_started"], 8)
        self.assertEqual(report["outcomes"]["owner_hold"], 1)
        self.assertEqual(report["outcomes"]["filtered"], 1)
        self.assertEqual(report["action_successes_confirmed"], 0)

    def test_missing_attempt_stays_unfinished(self):
        report = summarize_lines([event(1, "requested")])
        self.assertFalse(report["complete"])
        self.assertEqual(report["unfinished"], [{"run_id": "run-a", "attempt_id": 1}])

    def test_seven_complete_captures_cannot_pass_ten_reported_starts(self):
        lines = []
        for ident in range(1, 8):
            lines.extend([event(ident, "requested"), event(ident, "started", session=ident),
                          event(ident, "terminal", session=ident, outcome="delivered")])
        report = summarize_lines(lines, expected_starts=10)
        self.assertEqual(report["captures_started"], 7)
        self.assertFalse(report["expected_count_match"])
        self.assertFalse(report["complete"])
        self.assertTrue(summarize_lines(lines, expected_starts=7)["complete"])

    def test_duplicate_terminal_is_an_error_not_two_successes(self):
        lines = [event(1, "requested"), event(1, "started", session=1), event(1, "terminal", session=1, outcome="delivered")]
        report = summarize_lines(lines + [lines[-1]])
        self.assertFalse(report["complete"])
        self.assertTrue(any("duplicate terminal" in e for e in report["errors"]))

    def test_terminal_without_start_does_not_fill_missing_evidence(self):
        report = summarize_lines([event(1, "terminal", outcome="delivered")])
        self.assertFalse(report["complete"])
        self.assertTrue(any("without request" in e for e in report["errors"]))

    def test_restart_separates_reused_attempt_numbers(self):
        lines = [event(1, stage, run=run, session=None if stage == "requested" else 1,
                       outcome="cancelled" if stage == "terminal" else None)
                 for run in ["run-a", "run-b"] for stage in ["requested", "started", "terminal"]]
        report = summarize_lines(lines)
        self.assertTrue(report["complete"])
        self.assertEqual(report["requests"], 2)

    def test_dispatch_entry_does_not_prove_execution(self):
        lines = [event(1, "requested"), event(1, "started", session=9),
                 event(1, "terminal", session=9, outcome="delivered"),
                 '[CAPTURE-DISPATCH] {"run_id":"run-a","session":9,"event":"dispatch_received","utc_ms":1234567891}']
        report = summarize_lines(lines)
        self.assertTrue(report["complete"])
        self.assertEqual(report["dispatch_entries"], 1)
        self.assertEqual(report["action_successes_confirmed"], 0)

    def test_empty_or_legacy_log_is_not_a_passing_batch(self):
        self.assertFalse(summarize_lines(["[TRACE] owner=verified"])["complete"])

    def test_quoted_stt_markers_do_not_manufacture_trials(self):
        request = event(1, "requested")
        self.assertEqual(summarize_lines(["2026-10-25T14:00:00Z INFO stt transcript: '" + request + "'"])["requests"], 0)
        real = "2026-10-25T14:00:00Z INFO capture_audit: "
        report = summarize_lines([real + request, real + event(1, "terminal", outcome="mic_held")])
        self.assertTrue(report["complete"])

    def test_unknown_stage_or_malformed_record_is_reported(self):
        report = summarize_lines([event(1, "requested"), event(1, "execute_anything"), '[CAPTURE] {bad'])
        self.assertFalse(report["complete"])
        self.assertEqual(len(report["errors"]), 2)

    def test_late_stage_and_session_reuse_are_rejected(self):
        report = summarize_lines([
            event(1, "requested"), event(1, "started", session=3),
            event(1, "terminal", session=3, outcome="stale"), event(1, "received", session=3),
            event(2, "requested"), event(2, "started", session=3),
            event(2, "terminal", session=3, outcome="delivered"),
        ])
        self.assertFalse(report["complete"])
        self.assertTrue(any("after terminal" in e for e in report["errors"]))
        self.assertTrue(any("session reused" in e for e in report["errors"]))

    def test_missing_capture_start_and_dispatch_of_held_voice_cannot_pass(self):
        report = summarize_lines([event(1, "requested"), event(1, "terminal", session=3, outcome="delivered")])
        self.assertFalse(report["complete"])
        report = summarize_lines([
            event(1, "requested"), event(1, "started", session=3),
            event(1, "terminal", session=3, outcome="owner_hold"),
            '[CAPTURE-DISPATCH] {"run_id":"run-a","session":3,"event":"dispatch_received","utc_ms":1234567891}',
        ])
        self.assertFalse(report["complete"])
        self.assertEqual(len(report["unexpected_dispatches"]), 1)


if __name__ == "__main__":
    unittest.main()
