# 203 — Plan 06 step 2: Whisper confidence does not tell a right command from a wrong one (finding, no code)

Plan 06 step 2 proposed using Groq's `verbose_json` confidence to decide "execute or ask". Checked against the 168 transcriptions from
`scripts/prompt_ab.py` (synthetic clips, two Windows voices, four prompts):

| clips | count | `avg_logprob` min / median / max |
|---|---|---|
| command, right words | 106 | -0.77 / -0.48 / -0.16 |
| command, wrong words | 14 | -0.59 / -0.45 / -0.24 |
| ordinary talk, right | 24 | -0.42 / -0.28 / -0.11 |
| silence/noise, text invented | 23 | -1.27 / -0.59 / -0.19 |

Short commands score low whether or not they are right (right median -0.48, wrong -0.45), so a logprob threshold would ask about many correct commands and
miss the wrong ones. Groq's Whisper gives per-segment `avg_logprob`, not per-word probabilities, so the "low-confidence word" idea cannot be built on it.
The existing `sttConfidenceGate` (drop a transcript below -1.0 / compression > 2.4) stays as the garbage filter it already is.

Decision: do not build a confidence-gated ask. Decide asks from **sound-match margin** instead (plan step 4), which is what separated right from wrong in
`sound_match` tests. Caveat: clean synthetic audio; real-mic data (`sttLogConfidence`) could be collected later to re-check.
