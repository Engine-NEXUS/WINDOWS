#!/usr/bin/env python3
"""Tests for scripts/voice_eval_set.py and scripts/voice_scorecard.py (stdlib only).

    python -m unittest scripts/test_voice_eval_tools.py -v
"""

import json
import math
import struct
import sys
import tempfile
import unittest
import wave
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import voice_eval_set as vs  # noqa: E402
import voice_scorecard as sc  # noqa: E402
import wake_calibrate as wc  # noqa: E402
import wake_audition as wa  # noqa: E402


def write_wav(path, seconds=1.0, amp=3000, sr=16000):
    path.parent.mkdir(parents=True, exist_ok=True)
    n = int(seconds * sr)
    frames = b"".join(struct.pack("<h", int(amp * math.sin(i / 8))) for i in range(n))
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(sr)
        w.writeframes(frames)


class PlanTests(unittest.TestCase):
    def test_wake_plan_is_30_takes_per_phrase_with_unique_ids(self):
        plan = vs.build_plan("wake", "owner", "s1", "mic")
        self.assertEqual(len(plan), 90)
        self.assertEqual(len({i["id"] for i in plan}), 90)
        for ph in vs.WAKE_PHRASES:
            self.assertEqual(sum(1 for i in plan if i["phrase"] == ph), 30)
        self.assertTrue(all(i["split"] == "test" and i["consent"] is True for i in plan))
        self.assertTrue(any(i["environment"] != "quiet" for i in plan))
        self.assertTrue(any(i["distance_cm"] >= 100 for i in plan))
        self.assertTrue(any(i["volume"] == "whisper" for i in plan))

    def test_command_plan_has_one_breath_rows_with_nexus_prefix(self):
        plan = vs.build_plan("command", "owner", "s1", "mic")
        breath = [i for i in plan if i["one_breath"]]
        self.assertEqual(len(breath), len(vs.ONE_BREATH) * vs.COMMAND_TAKES)
        self.assertTrue(all(i["expected_text"].startswith("nexus ") for i in breath))
        self.assertTrue(all(i["expected_intent"] for i in plan))

    def test_silence_section_has_silence_and_noise(self):
        plan = vs.build_plan("silence", "owner", "s1", "mic")
        self.assertEqual({i["kind"] for i in plan}, {"silence", "noise"})
        self.assertTrue(all(i["expected_intent"] == "none" for i in plan))

    def test_unknown_section_rejected(self):
        with self.assertRaises(ValueError):
            vs.build_plan("bogus", "a", "b", "c")

    def test_speakers_sessions_do_not_collide(self):
        a = {i["id"] for i in vs.build_plan("wake", "owner", "s1", "mic")}
        b = {i["id"] for i in vs.build_plan("wake", "friend1", "s1", "mic")}
        c = {i["id"] for i in vs.build_plan("wake", "owner", "s2", "mic")}
        self.assertFalse(a & b or a & c)

    def test_negative_plan_covers_soundalikes(self):
        phrases = {i["phrase"] for i in vs.build_plan("negative", "o", "s", "m")}
        for must in ("lexus", "next us", "nixes"):
            self.assertIn(must, phrases)


class TrainPurposeTests(unittest.TestCase):
    def test_train_plan_is_marked_and_limited(self):
        plan = vs.build_plan("wake", "friend1", "s9", "mic", purpose="train")
        self.assertEqual(len(plan), 90)
        self.assertTrue(all(i["id"].startswith("trn-") and i["split"] == "train" for i in plan))
        ev = {i["id"] for i in vs.build_plan("wake", "friend1", "s9", "mic")}
        self.assertFalse(ev & {i["id"] for i in plan}, "ids never collide with the eval set")
        with self.assertRaises(ValueError):
            vs.build_plan("command", "a", "b", "c", purpose="train")
        with self.assertRaises(ValueError):
            vs.build_plan("wake", "a", "b", "c", purpose="bogus")

    def test_train_audio_lands_where_the_trainer_reads_it(self):
        root = Path("/x")
        wake = vs.build_plan("wake", "f", "s", "m", purpose="train")[0]
        neg = vs.build_plan("negative", "f", "s", "m", purpose="train")[0]
        self.assertEqual(vs.audio_path(root, "train", wake, "f", "s"), root / "positive" / f"{wake['id']}.wav")
        self.assertEqual(vs.audio_path(root, "train", neg, "f", "s"), root / "negative" / f"{neg['id']}.wav")
        self.assertIn("audio", str(vs.audio_path(root, "eval", wake, "f", "s")))

    def test_validate_keeps_the_two_sets_apart(self):
        with tempfile.TemporaryDirectory() as tmp:
            row = vs.build_plan("wake", "owner", "s1", "mic", purpose="train")[0]
            row["audio"] = "positive/a.wav"
            write_wav(Path(tmp) / row["audio"])
            vs.append_manifest(Path(tmp) / "manifest.jsonl", row)
            errors, _ = vs.validate(tmp, expected_split="train")
            self.assertEqual(errors, [])
            errors, _ = vs.validate(tmp)   # judged as the held-out set: must be refused
            self.assertTrue(any("split='test'" in e for e in errors))
            self.assertTrue(any("TRAINING recording" in e for e in errors))


class ValidateTests(unittest.TestCase):
    def _set(self, tmp, **over):
        row = vs.build_plan("wake", "owner", "s1", "mic")[0]
        row["audio"] = "audio/owner/s1/a.wav"
        row.update(over)
        write_wav(Path(tmp) / row["audio"])
        vs.append_manifest(Path(tmp) / "manifest.jsonl", row)
        return row

    def test_valid_row_passes_with_speaker_warning(self):
        with tempfile.TemporaryDirectory() as tmp:
            self._set(tmp)
            errors, warnings = vs.validate(tmp)
            self.assertEqual(errors, [])
            self.assertTrue(any("speaker" in w for w in warnings))

    def test_missing_audio_and_train_split_are_errors(self):
        with tempfile.TemporaryDirectory() as tmp:
            row = self._set(tmp, split="train")
            (Path(tmp) / row["audio"]).unlink()
            errors, _ = vs.validate(tmp)
            joined = " ".join(errors)
            self.assertIn("audio file missing", joined)
            self.assertIn("split='test'", joined)

    def test_duplicate_ids_and_missing_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            self._set(tmp)
            rows = vs.read_manifest(Path(tmp) / "manifest.jsonl")
            rows.append(dict(rows[0]))
            rows.append({"id": "x"})
            errors, _ = vs.validate(tmp, rows)
            self.assertTrue(any("duplicate" in e for e in errors))
            self.assertTrue(any("missing fields" in e for e in errors))

    def test_near_silent_wake_clip_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            row = self._set(tmp)
            write_wav(Path(tmp) / row["audio"], amp=3)
            _, warnings = vs.validate(tmp)
            self.assertTrue(any("almost silent" in w for w in warnings))

    def test_wav_stats_and_summary(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "a.wav"
            write_wav(p, seconds=0.5, amp=16384)
            sr, ch, secs, rms = vs.wav_stats(p)
            self.assertEqual((sr, ch), (16000, 1))
            self.assertAlmostEqual(secs, 0.5, places=2)
            self.assertAlmostEqual(rms, 0.5 / math.sqrt(2), delta=0.02)
        s = vs.summarize([{"kind": "wake", "phrase": "nexus", "speaker_id": "o"},
                          {"kind": "room", "seconds": 600}])
        self.assertEqual(s["wake_by_phrase"]["nexus"], 1)
        self.assertEqual(s["room_seconds"], 600)


class StatsTests(unittest.TestCase):
    def test_wilson_known_values(self):
        p, lo, hi = sc.wilson(95, 100)
        self.assertAlmostEqual(p, 0.95)
        self.assertAlmostEqual(lo, 0.8883, places=3)
        self.assertAlmostEqual(hi, 0.9785, places=3)
        self.assertTrue(math.isnan(sc.wilson(0, 0)[0]))
        self.assertEqual(sc.wilson(0, 10)[1], 0.0)

    def test_poisson_upper_bound(self):
        self.assertAlmostEqual(sc.poisson_upper(0), 3.69, delta=0.05)   # exact: 3.689
        self.assertAlmostEqual(sc.poisson_upper(5), 11.67, delta=0.1)   # exact: 11.668

    def test_percentile(self):
        self.assertEqual(sc.percentile([], 0.5), None)
        self.assertEqual(sc.percentile([10], 0.95), 10)
        self.assertAlmostEqual(sc.percentile(list(range(101)), 0.95), 95)

    def test_stars_high_is_driven_by_lower_bound(self):
        # 30/30 -> lower bound ~0.886 < 0.95: strong point estimate, but not five stars.
        p, lo, hi = sc.wilson(30, 30)
        self.assertEqual(sc.stars(p, lo, hi, 0.95, "high"), 4)
        # 300/300 -> lower bound ~0.987: five stars.
        p, lo, hi = sc.wilson(300, 300)
        self.assertEqual(sc.stars(p, lo, hi, 0.95, "high"), 5)
        p, lo, hi = sc.wilson(70, 100)
        self.assertEqual(sc.stars(p, lo, hi, 0.95, "high"), 1)
        p, lo, hi = sc.wilson(92, 100)
        self.assertEqual(sc.stars(p, lo, hi, 0.95, "high"), 3)
        self.assertEqual(sc.stars(float("nan"), 0, 0, 0.95, "high"), 0)

    def test_stars_low_direction(self):
        self.assertEqual(sc.stars(0.0, 0.0, 0.004, 0.005, "low"), 5)
        self.assertEqual(sc.stars(0.0, 0.0, 0.03, 0.005, "low"), 4)
        self.assertEqual(sc.stars(0.008, 0.0, 0.03, 0.005, "low"), 3)
        self.assertEqual(sc.stars(0.2, 0.0, 0.3, 0.005, "low"), 1)


def wake_row(phrase, fired, speaker="owner", env="quiet", dist=30, vol="normal", fa=None, stage2=None, ms=None):
    r = {"kind": "wake", "phrase": phrase, "speaker_id": speaker, "environment": env,
         "distance_cm": dist, "volume": vol, "fired": fired, "fire_after_onset_ms": ms}
    if stage2 is not None:
        r["stage2_pass"] = stage2
        r["stage2_ms"] = 500
    return r


class MetricTests(unittest.TestCase):
    def test_wake_metrics_recall_groups_and_false_alarms(self):
        rows = [wake_row("nexus", True, ms=100)] * 9 + [wake_row("nexus", False)]
        rows += [wake_row("hey nexus", True, speaker="friend", ms=200)] * 4
        rows += [wake_row("okay nexus", False, env="tv")] * 2
        rows += [{"kind": "room", "duration_ms": 3600 * 1000 * 2, "n_fires": 1}]
        rows += [{"kind": "negative", "fired": False}] * 5 + [{"kind": "negative", "fired": True}]
        m = {x["label"]: x for x in sc.wake_metrics(rows)}
        nx = m['"NEXUS" recall (neural only)']
        self.assertEqual((nx["k"], nx["n"]), (9, 10))
        self.assertEqual(m['"OKAY NEXUS" recall (neural only)']["point"], 0.0)
        self.assertEqual(m["Other people (non-owner speakers) (neural only)"]["n"], 4)
        self.assertEqual(m["Noisy room / far away / whisper (neural only)"]["n"], 2)
        fa = m["False alarms per hour (room audio, neural only)"]
        self.assertAlmostEqual(fa["point"], 0.5)
        self.assertEqual(m["Soundalike / everyday-word false alarms (neural only)"]["k"], 1)
        lat = m["Wake speed p95 (ms after you start speaking) (neural only)"]
        self.assertTrue(100 <= lat["point"] <= 200)

    def test_full_gate_requires_stage2_and_adds_latency(self):
        rows = [wake_row("nexus", True, stage2=True, ms=100), wake_row("nexus", True, stage2=False, ms=100)]
        m = {x["label"]: x for x in sc.wake_metrics(rows, full_gate=True)}
        self.assertEqual(m['"NEXUS" recall (with speech check)']["k"], 1)
        self.assertEqual(m["Wake speed p95 (ms after you start speaking) (with speech check)"]["point"], 600)

    def test_nothing_measured_means_zero_stars_not_a_guess(self):
        m = sc.wake_metrics([])
        self.assertTrue(all(x["stars"] == 0 and x["point"] is None for x in m))
        report, allm = sc.build_report([], [])
        self.assertIn("nothing measured yet", report)

    def test_stt_metrics(self):
        rows = [
            {"kind": "command", "expected_text": "open chrome", "raw": "open chrome", "ref_words": 2,
             "edits_raw": 0, "edits_filtered": 0, "intent_ok": True, "dropped_by_filter": False},
            {"kind": "command", "expected_text": "list pull requests", "raw": "list pull requests", "ref_words": 3,
             "edits_raw": 0, "edits_filtered": 3, "intent_ok": False, "dropped_by_filter": True},
            {"kind": "command", "one_breath": True, "expected_text": "nexus open chrome", "raw": "lexus open chrome",
             "ref_words": 3, "edits_raw": 1, "edits_filtered": 1, "intent_ok": False, "dropped_by_filter": False},
            {"kind": "silence", "raw": "", "leaked": False},
            {"kind": "noise", "raw": "thanks for watching", "leaked": True},
        ]
        m = {x["key"]: x for x in sc.stt_metrics(rows)}
        self.assertAlmostEqual(m["stt_word_accuracy"]["point"], 1 - 1 / 8)
        self.assertEqual((m["stt_command_match"]["k"], m["stt_command_match"]["n"]), (1, 2))
        self.assertEqual(m["stt_wrongly_dropped"]["k"], 1)
        self.assertEqual((m["stt_hallucination_leak"]["k"], m["stt_hallucination_leak"]["n"]), (1, 2))
        self.assertEqual(m["stt_one_breath"]["k"], 0)

    def test_one_breath_label_reflects_capture_simulation(self):
        base = {"kind": "command", "one_breath": True, "expected_text": "nexus open chrome",
                "raw": "open chrome", "ref_words": 3, "edits_raw": 1, "edits_filtered": 1}
        plain = [dict(base, intent_ok=True) for _ in range(4)]
        m = {x["key"]: x for x in sc.stt_metrics(plain)}["stt_one_breath"]
        self.assertIn("STT + parser only", m["label"])
        self.assertIn("NOT included", m["note"])
        sim = [dict(base, intent_ok=True, capture_offset_ms=-160) for _ in range(3)]
        sim.append(dict(base, intent_ok=False, capture_offset_ms=-160, wake_missed=True, raw=""))
        m = {x["key"]: x for x in sc.stt_metrics(sim)}["stt_one_breath"]
        self.assertIn("capture simulated, offset -160 ms", m["label"])
        self.assertEqual((m["k"], m["n"]), (3, 4))
        self.assertIn("1 wake miss", m["note"])

    def test_endpoint_cutoff_row_only_with_simulation(self):
        base = {"kind": "command", "expected_text": "open chrome", "raw": "open chrome", "ref_words": 2,
                "edits_raw": 0, "edits_filtered": 0, "intent_ok": True}
        keys = {m["key"] for m in sc.stt_metrics([dict(base)])}
        self.assertNotIn("stt_endpoint_cutoff", keys)
        rows = [dict(base, cut_early=False) for _ in range(99)] + [dict(base, cut_early=True)]
        m = {x["key"]: x for x in sc.stt_metrics(rows)}["stt_endpoint_cutoff"]
        self.assertEqual((m["k"], m["n"]), (1, 100))

    def test_report_renders_table_and_baseline_delta(self):
        wake = [wake_row("nexus", True, ms=100)] * 10
        report, metrics = sc.build_report(wake, [])
        self.assertIn("| What | Measured |", report)
        self.assertIn("★", report)
        base = {m["label"]: m for m in metrics}
        report2, _ = sc.build_report(wake, [], base)
        self.assertIn("(was 100.0% (10/10))", report2)


class ConfidenceSweepTests(unittest.TestCase):
    def rows(self):
        cmd = lambda lp, cr=1.2, **k: {"kind": "command", "raw": "open chrome", "max_avg_logprob": lp,  # noqa: E731
                                       "max_compression_ratio": cr, **k}
        rows = [cmd(-0.3) for _ in range(300)] + [cmd(-0.5) for _ in range(100)]
        # hallucinations that survived the text filter: low confidence / looping
        rows += [{"kind": "noise", "raw": "so we begin", "leaked": True, "max_avg_logprob": -1.4,
                  "max_compression_ratio": 1.1},
                 {"kind": "silence", "raw": "uh huh", "leaked": True, "max_avg_logprob": -1.8,
                  "max_compression_ratio": 1.0},
                 {"kind": "noise", "raw": "ha ha ha ha", "leaked": True, "max_avg_logprob": -0.4,
                  "max_compression_ratio": 3.4}]
        return rows

    def test_sweep_counts_and_recommendation(self):
        sweep = sc.confidence_sweep(self.rows())
        by = {(r["signal"], r["threshold"]): r for r in sweep}
        r = by[("avg_logprob", -1.2)]
        self.assertEqual((r["caught"], r["n_leaks"], r["wrong"]), (2, 3, 0))
        r = by[("compression_ratio", 3.0)]
        self.assertEqual((r["caught"], r["wrong"]), (1, 0))
        rec = sc.recommend_gate(sweep)
        self.assertEqual(rec["avg_logprob"]["caught"], 2)
        self.assertEqual(rec["compression_ratio"]["caught"], 1)

    def test_no_recommendation_when_it_would_reject_real_commands(self):
        rows = [{"kind": "command", "raw": "x", "max_avg_logprob": -1.6, "max_compression_ratio": 1.0}
                for _ in range(10)]
        rows.append({"kind": "noise", "raw": "y", "leaked": True, "max_avg_logprob": -1.7,
                     "max_compression_ratio": 1.0})
        rec = sc.recommend_gate(sc.confidence_sweep(rows))
        self.assertNotIn("avg_logprob", rec)
        self.assertIn("Do not enable", sc.render_sweep(sc.confidence_sweep(rows)))

    def test_no_stats_means_no_section(self):
        self.assertIsNone(sc.confidence_sweep([{"kind": "command", "raw": "open chrome"}]))
        report, _ = sc.build_report([], [{"kind": "command", "raw": "open chrome", "ref_words": 2,
                                          "edits_raw": 0, "edits_filtered": 0, "intent_ok": True,
                                          "expected_text": "open chrome"}])
        self.assertNotIn("Confidence gate calibration", report)

    def test_report_contains_sweep_when_stats_present(self):
        report, _ = sc.build_report([], self.rows())
        self.assertIn("Confidence gate calibration", report)
        self.assertIn("Safe to enable", report)


def sweep_rows(neg_max=0.60):
    """30 wake clips whose scores spread 0.5..0.95, 20 soundalikes up to neg_max, 2 h of room audio."""
    ths = ["0.50", "0.60", "0.68", "0.76", "0.84"]
    rows = []
    for i in range(30):
        score = 0.5 + 0.45 * i / 29
        rows.append({"kind": "wake", "sweep": {t: {"fired": score >= float(t), "n_fires": 1} for t in ths}})
    for i in range(20):
        score = neg_max * i / 19
        rows.append({"kind": "negative", "sweep": {t: {"fired": score >= float(t), "n_fires": 0} for t in ths}})
    # room audio: noise bursts at 0.55 (x3) and 0.65 (x1)
    fires = {"0.50": 4, "0.60": 1, "0.68": 0, "0.76": 0, "0.84": 0}
    rows.append({"kind": "room", "duration_ms": 2 * 3600 * 1000,
                 "sweep": {t: {"n_fires": fires[t]} for t in ths}})
    return rows


class CalibrateTests(unittest.TestCase):
    def test_table_and_recommendation_pick_highest_recall_inside_budget(self):
        tab = wc.table(sweep_rows())
        by = {r["threshold"]: r for r in tab}
        self.assertEqual(len(tab), 5)
        self.assertEqual(by[0.50]["fires"], 4)
        self.assertAlmostEqual(by[0.50]["fa_per_hour"], 2.0)
        self.assertAlmostEqual(by[0.68]["hours"], 2.0)
        self.assertGreater(by[0.50]["recall"], by[0.84]["recall"])
        rec = wc.recommend(tab)
        # 0.50/0.60 break the false-alarm budget; 0.68 is the highest-recall acceptable point.
        self.assertEqual(rec["threshold"], 0.68)

    def test_soundalike_budget_can_veto_a_threshold(self):
        tab = wc.table(sweep_rows(neg_max=0.74))
        rec = wc.recommend(tab, max_fa_per_hour=100.0, max_soundalike_fa=0.0)
        self.assertEqual(rec["threshold"], 0.76)   # 0.68 lets a soundalike (0.74) through

    def test_no_acceptable_threshold_and_missing_room_audio(self):
        tab = wc.table(sweep_rows())
        self.assertIsNone(wc.recommend([dict(r, fa_per_hour=9.0) for r in tab]))
        no_room = [r for r in sweep_rows() if r["kind"] != "room"]
        rec = wc.recommend(wc.table(no_room))
        self.assertIsNotNone(rec, "without room audio the soundalike budget still decides")

    def test_report_flags_uncertified_recommendations(self):
        tab = wc.table(sweep_rows())
        text = wc.render(tab, wc.recommend(tab), 0.125, 0.01)
        self.assertIn("Recommended threshold: **0.68**", text)
        self.assertIn("Not certified", text)   # 2 h of audio cannot prove 1 per 8 h


class AuditionTests(unittest.TestCase):
    def _tone(self, ms, amp=6000):
        n = 16000 * ms // 1000
        return [int(amp * math.sin(i / 7)) for i in range(n)]

    def _silence(self, ms):
        return [0] * (16000 * ms // 1000)

    def test_split_utterances_finds_words_drops_clicks_and_caps_length(self):
        a = (self._silence(500) + self._tone(600) + self._silence(1200) + self._tone(60)
             + self._silence(1200) + self._tone(800) + self._silence(600))
        segs = wa.split_utterances(a)
        self.assertEqual(len(segs), 2, "two real words; the 60 ms click is dropped")
        for s, e in segs:
            self.assertGreater(e - s, 16000 * 600 // 1000)   # includes the padding
        # a 9 s monologue is cut into pieces of <= 4 s (+ nothing lost)
        long_segs = wa.split_utterances(self._silence(300) + self._tone(9000) + self._silence(600))
        self.assertGreaterEqual(len(long_segs), 3)
        self.assertTrue(all((e - s) <= 16000 * 4000 // 1000 for s, e in long_segs))
        self.assertEqual(wa.split_utterances(self._silence(3000)), [])

    def test_decisions_move_files_and_log_and_never_collide(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            (base / "positive").mkdir()
            wa.write_wav(base / "positive" / "audition_0007.wav", self._tone(300))
            src1, src2, src3 = base / "a.wav", base / "b.wav", base / "c.wav"
            for p in (src1, src2, src3):
                wa.write_wav(p, self._tone(300))
            d1 = wa.apply_decision(src1, "p", base, now=1)
            d2 = wa.apply_decision(src2, "n", base, now=2)
            self.assertEqual(d1.name, "audition_0008.wav", "continues after the highest existing index")
            self.assertEqual(d2.parent.name, "negative")
            self.assertFalse(src1.exists() and src2.exists())
            # skip / replay / garbage do nothing and never touch the file
            for ans in ("s", "r", "x", ""):
                self.assertIsNone(wa.apply_decision(src3, ans, base))
            self.assertTrue(src3.exists())
            log = [json.loads(l) for l in (base / "audition_log.jsonl").read_text().splitlines()]
            self.assertEqual([r["decision"] for r in log], ["positive", "negative"])
            counts, audited = wa.stats(base)
            self.assertEqual((counts["positive"], counts["negative"], audited), (2, 1, 2))

    def test_wav_roundtrip(self):
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "x.wav"
            wa.write_wav(p, self._tone(100))
            sr, a = wa.read_wav(p)
            self.assertEqual((sr, len(a)), (16000, 1600))


if __name__ == "__main__":
    unittest.main()
