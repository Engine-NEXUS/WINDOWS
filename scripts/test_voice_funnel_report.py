import unittest

from voice_funnel_report import analyse, render

LOG = """\
2026-11-13T10:00:00.000000Z DEBUG audio: mic .......... SILENT rms=0.0000 (cb 1)
2026-11-13T10:00:01.000000Z DEBUG audio: mic .......... SILENT rms=0.0000 (cb 2)
2026-11-13T10:00:02.000000Z DEBUG audio: mic ##........ LIVE   rms=0.0400 (cb 3)
2026-11-13T10:00:03.000000Z  WARN audio: mic has been silent for ~76s (callbacks 1-2)
2026-11-13T10:01:00.000000Z  INFO wake-word: stage-1 candidate (prob 0.719) -> verifying with STT...
2026-11-13T10:01:03.000000Z  INFO verify: transcript 'That's so' rejected (stage-1 prob 0.719) -> wake SUPPRESSED
2026-11-13T10:02:00.000000Z  INFO wake-word: stage-1 candidate (prob 0.800) -> verifying with STT...
2026-11-13T10:02:01.000000Z  INFO verify: transcript '' rejected (stage-1 prob 0.800) -> wake SUPPRESSED
2026-11-13T10:03:00.000000Z  INFO wake-word: stage-1 candidate (prob 0.871) -> verifying with STT...
2026-11-13T10:03:01.000000Z  INFO verify: STT confirmed 'Nexus.' (stage-1 prob 0.871, 1 segs) -> firing wake
2026-11-13T10:03:02.000000Z  INFO wake: firing orb (source=neural)
2026-11-13T10:03:02.100000Z  INFO stt-capture: started (session=12)
2026-11-13T10:03:04.000000Z  INFO stt-capture: aborted (was_capturing=true, had_speech=false, elapsed=2240ms)
2026-11-13T10:03:04.100000Z  INFO [CAPTURE] {"event":"terminal","outcome":"cancelled"}
2026-11-13T10:03:05.000000Z  INFO stt: groq transcript: 'open notepad'
2026-11-13T10:03:06.000000Z  INFO stt: filtered hallucination 'Thanks.' -> ''
2026-11-12T09:00:00.000000Z  INFO wake-word: stage-1 candidate (prob 0.900) -> verifying with STT...
"""


from voice_funnel_report import near_misses


class NearMiss(unittest.TestCase):
    def test_a_burst_is_one_moment_and_only_mid_scores_count(self):
        scored = [(10.0, 0.20), (10.1, 0.55), (10.2, 0.40),      # one moment, peak 0.55: near miss
                  (20.0, 0.30), (20.1, 0.31),                    # peak 0.31: noise, not counted
                  (30.0, 0.30), (30.1, 0.77),                    # fired (>= 0.68): not a miss
                  (40.0, 0.60), (42.0, 0.62)]                    # two moments 2 s apart
        self.assertEqual(sorted(near_misses(scored)), [0.55, 0.60, 0.62])

    def test_nothing_scored_means_no_near_misses(self):
        self.assertEqual(near_misses([]), [])


class Funnel(unittest.TestCase):
    def test_counts_every_stage(self):
        a = analyse(LOG.splitlines(), since="2026-11-13T00:00:00")
        self.assertEqual((a["mic_live"], a["mic_silent"], a["mic_silent_warn"]), (1, 2, 1))
        self.assertEqual((a["candidates"], a["rejected"], a["confirmed"], a["reject_empty"]), (3, 2, 1, 1))
        self.assertEqual(a["fires"]["neural"], 1)
        self.assertEqual((a["capture_started"], a["capture_no_speech"]), (1, 1))
        self.assertEqual(a["capture_outcomes"]["cancelled"], 1)
        self.assertEqual((a["transcripts"], a["hallucinations"]), (1, 1))

    def test_since_drops_older_lines(self):
        self.assertEqual(analyse(LOG.splitlines())["candidates"], 4)
        self.assertEqual(analyse(LOG.splitlines(), since="2026-11-13T00:00:00")["candidates"], 3)

    def test_empty_log_shows_zero_and_na_never_a_guess(self):
        text = render(analyse([]))
        self.assertIn("n/a", text)
        self.assertNotIn("100.0%", text)

    def test_render_names_what_fooled_the_detector(self):
        text = render(analyse(LOG.splitlines(), since="2026-11-13T00:00:00"))
        self.assertIn("That's so", text)
        self.assertIn("(2/3)", text)


if __name__ == "__main__":
    unittest.main()
