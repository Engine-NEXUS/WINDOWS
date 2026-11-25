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
| Owner's own takes withheld from training (all phrases) (neural only) | 77.0% (107/139) (was 86.2% (138/160)) | 69.3-83.2% | >= 95% | ★☆☆☆☆ |
| Noisy room / far away / whisper (neural only) | not measured | - | >= 90% | ☆☆☆☆☆ |
| False alarms per hour (room audio, neural only) - 10.78 h of audio | 23.75/h (256 in 10.78 h) (was 14.20/h (153 in 10.78 h)) | <= 26.85/h | <= 1 per 8 h | ★☆☆☆☆ |
| Soundalike / everyday-word false alarms (neural only) | not measured | - | <= 1% | ☆☆☆☆☆ |
| Wake speed p95 (ms after you start speaking) (neural only) - median 440 ms | 600 ms (was 600 ms) | - | <= 400 ms | ★★★☆☆ |
