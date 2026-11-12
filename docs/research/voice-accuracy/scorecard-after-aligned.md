# NEXUS voice accuracy scorecard

**Overall: ★☆☆☆☆** - weakest measured row is 1 star(s); 6 row(s) not measured yet.

Stars follow the lower confidence bound against the target, not just the average. 'not measured' means no held-out data was supplied, never a guess.


### Wake word - neural model only

| What | Measured | 95% range | Target | Stars |
|---|---|---|---|---|
| "NEXUS" recall (neural only) | not measured | - | >= 95% | ☆☆☆☆☆ |
| "HEY NEXUS" recall (neural only) | not measured | - | >= 95% | ☆☆☆☆☆ |
| "OKAY NEXUS" recall (neural only) | not measured | - | >= 95% | ☆☆☆☆☆ |
| Other people (non-owner speakers) (neural only) | not measured | - | >= 90% | ☆☆☆☆☆ |
| Owner's own takes withheld from training (all phrases) (neural only) | 77.7% (108/139) (was 86.2% (138/160)) | 70.1-83.8% | >= 95% | ★☆☆☆☆ |
| Noisy room / far away / whisper (neural only) | not measured | - | >= 90% | ☆☆☆☆☆ |
| False alarms per hour (room audio, neural only) - 10.78 h of audio | 30.90/h (333 in 10.78 h) (was 14.20/h (153 in 10.78 h)) | <= 34.40/h | <= 1 per 8 h | ★☆☆☆☆ |
| Soundalike / everyday-word false alarms (neural only) | not measured | - | <= 1% | ☆☆☆☆☆ |
| Wake speed p95 (ms after you start speaking) (neural only) - median 440 ms | 680 ms (was 600 ms) | - | <= 400 ms | ★★★☆☆ |
