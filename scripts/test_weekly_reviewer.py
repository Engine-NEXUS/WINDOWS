import json
import os
import tempfile
import time
import unittest
from unittest import mock

import weekly_reviewer as w


class Collection(unittest.TestCase):
    def test_only_short_command_shaped_lines_are_kept(self):
        payload, dropped = w.build_payload([
            "open load pad", "Nexus, open brief", "She's a little bit of a ricotta.", "What time is this?",
            "open " + "word " * 12, "send down my laptop", "open load pad",
        ])
        texts = list(payload.values())
        self.assertEqual(texts, ["open load pad", "Nexus, open brief", "send down my laptop"])
        self.assertEqual(dropped["duplicate"], 1)
        self.assertGreaterEqual(dropped["not_command_shaped"], 3)

    def test_talk_after_a_command_means_room_conversation(self):
        payload, _ = w.build_payload(["hook up open. open. thank you, steven.", "Open. Puri yunai. Andang on the east. Good.",
                                      "hook up anti-pilot. hook up anti-gravity."])
        self.assertEqual(list(payload.values()), ["hook up anti-pilot. hook up anti-gravity."])

    def test_sensitive_lines_never_leave(self):
        payload, dropped = w.build_payload(["open my password manager", "type my otp 123456", "open notepad"])
        self.assertEqual(list(payload.values()), ["open notepad"])
        self.assertEqual(dropped["sensitive"], 2)

    def test_redaction_removes_contact_details(self):
        self.assertEqual(w.redact("send mail to a.b@x.com now"), "send mail to [email] now")
        self.assertIn("[number]", w.redact("call 98765 43210 please"))
        self.assertIn("[url]", w.redact("open https://x.com/a?b=1"))

    def test_item_cap(self):
        lines = [f"open app{chr(97 + i % 26)}{chr(97 + i // 26)}" for i in range(200)]
        payload, _ = w.build_payload(lines, max_items=60)
        self.assertEqual(len(payload), 60)


GOOD = {"proposals": [
    {"kind": "homophone_group", "words": ["note pad", "load pad"], "canonical": "notepad", "evidence": ["t1"], "why": "heard as two words"},
    {"kind": "confusing_pair", "words": ["send"], "canonical": "shut", "evidence": ["t2"], "why": "verb sound-alike"},
]}


class Validation(unittest.TestCase):
    def test_good_proposals_pass(self):
        out, rej = w.validate(json.dumps(GOOD), {"t1", "t2"})
        self.assertEqual(len(out), 2)
        self.assertEqual(rej, [])

    def test_evidence_must_exist(self):
        out, rej = w.validate(json.dumps(GOOD), {"t9"})
        self.assertEqual(out, [])
        self.assertTrue(rej)

    def test_unsafe_or_malformed_proposals_are_rejected(self):
        bad = {"proposals": [
            {"kind": "vocabulary_entry", "words": ["visit http://x.co"], "canonical": "x", "evidence": ["t1"], "why": ""},
            {"kind": "vocabulary_entry", "words": ["prem"], "canonical": "delete all my files now please", "evidence": ["t1"], "why": ""},
            {"kind": "run_shell", "words": ["a"], "canonical": "b", "evidence": ["t1"], "why": ""},
            {"kind": "command_phrasing", "words": ["x y"], "canonical": "banana split", "evidence": ["t1"], "why": ""},
            {"kind": "vocabulary_entry", "words": ["ok one"], "canonical": "okay", "evidence": ["t1"], "why": ""},
        ]}
        out, rej = w.validate(json.dumps(bad), {"t1"})
        self.assertEqual([p["canonical"] for p in out], ["okay"])
        self.assertEqual(len(rej), 4)

    def test_prose_around_json_and_garbage(self):
        out, _ = w.validate("Here you go: " + json.dumps(GOOD) + " thanks", {"t1", "t2"})
        self.assertEqual(len(out), 2)
        self.assertEqual(w.validate("no json here", {"t1"})[0], [])
        self.assertEqual(w.validate("{not json}", {"t1"})[0], [])

    def test_capped(self):
        many = {"proposals": [{"kind": "vocabulary_entry", "words": [f"w{chr(97 + i)}"], "canonical": f"c{chr(97 + i)}", "evidence": ["t1"], "why": ""} for i in range(26)]}
        out, _ = w.validate(json.dumps(many), {"t1"})
        self.assertLessEqual(len(out), w.MAX_PROPOSALS)


class Flow(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.mkdtemp()
        self.missed = os.path.join(self.dir, "missed.jsonl")
        now = int(time.time() * 1000)
        with open(self.missed, "w", encoding="utf-8") as f:
            for t, ts in (("open load pad", now), ("send down my laptop", now), ("open ancient", now - 40 * 86400000)):
                f.write(json.dumps({"timestamp_ms": ts, "transcript": t}) + "\n")
        self.out = os.path.join(self.dir, "review")

    def run_cli(self, *extra, env=None):
        with mock.patch.dict(os.environ, env or {}, clear=False):
            return w.main(["run", "--input", self.missed, "--out", self.out, *extra])

    def test_dry_run_sends_nothing_and_saves_payload(self):
        with mock.patch.object(w, "call_advisor") as call:
            self.assertEqual(self.run_cli(), 0)
            call.assert_not_called()
        payload = json.load(open(os.path.join(self.out, "last_payload.json"), encoding="utf-8"))
        self.assertEqual(sorted(payload.values()), ["open load pad", "send down my laptop"])  # the 40-day-old line is outside the window

    def test_send_refuses_without_a_key(self):
        with mock.patch.object(w, "call_advisor") as call, mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": ""}):
            self.assertEqual(self.run_cli("--send", "--yes"), 2)
            call.assert_not_called()

    def test_send_then_weekly_guard_then_review(self):
        answer = json.dumps({"proposals": [{"kind": "homophone_group", "words": ["load pad"], "canonical": "notepad", "evidence": ["t1"], "why": "x"}]})
        with mock.patch.object(w, "call_advisor", return_value=answer) as call, mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": "k"}):
            self.assertEqual(self.run_cli("--send", "--yes"), 0)
            call.assert_called_once()
            self.assertEqual(self.run_cli("--send", "--yes"), 2)  # second send inside six days is refused
            self.assertEqual(call.call_count, 1)
        state = json.load(open(os.path.join(self.out, "proposals.json"), encoding="utf-8"))
        self.assertEqual(state["items"][0]["status"], "pending")
        self.assertEqual(state["items"][0]["evidence_text"], ["open load pad"])
        w.main(["approve", "--out", self.out, "p1"])
        approved = json.load(open(os.path.join(self.out, "approved.json"), encoding="utf-8"))
        self.assertEqual(approved[0]["canonical"], "notepad")

    def test_confirmation_prompt_no_means_no_call(self):
        with mock.patch.object(w, "call_advisor") as call, mock.patch.dict(os.environ, {"ANTHROPIC_API_KEY": "k"}), mock.patch("builtins.input", return_value="no"):
            self.assertEqual(self.run_cli("--send"), 1)
            call.assert_not_called()

    def test_api_call_shape_uses_opus_5_5_and_caps_output(self):
        seen = {}

        def fake_post(body):
            seen.update(json.loads(body))
            return {"content": [{"type": "text", "text": "{}"}]}
        w.call_advisor({"t1": "open x"}, "k", post=fake_post)
        self.assertEqual(seen["model"], "claude-opus-5-5")
        self.assertEqual(seen["max_tokens"], 2048)
        self.assertNotIn("tools", seen)  # the advisor has no hands


if __name__ == "__main__":
    unittest.main()
