# Wake-word improvement report - 2026-11-09 00:48

**Verdict: REGRESSED**

At least one conclusive row got WORSE. Do not promote this candidate.

| Metric | Before (shipped) | After (candidate) | Result |
|---|---|---|---|
| "NEXUS" recall (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| "HEY NEXUS" recall (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| "OKAY NEXUS" recall (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| Other people (non-owner speakers) (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| Owner's own takes withheld from training (all phrases) (neural only) | 86.2% (n=160) | 77.0% (n=139) | optimistic* |
| Noisy room / far away / whisper (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| False alarms per hour (room audio, neural only) | 14.20/h (n=10.78) | 23.75/h (n=10.78) | WORSE |
| Soundalike / everyday-word false alarms (neural only) | not measured (n=0) | not measured (n=0) | too little data |
| Wake speed p95 (ms after you start speaking) (neural only) | 600 ms (n=138) | 600 ms (n=107) | same |

\* *optimistic*: the shipped model was trained before these takes were withheld, so it has probably heard them. These rows are informational and never count towards the verdict.
Rows with fewer than 20 clips (or less than 1 h of held-out audio for false alarms per hour) are 'too little data', not evidence. One flipped clip is treated as noise.

## Run notes

- trained with UNREVIEWED flagged clips (--allow-unreviewed) - treat the result as unreliable

## What this does NOT tell you

- Accuracy for people whose voices are not in the held-out set, and for microphones you did not test.
- Command-recognition (STT) accuracy: run `nexus wake score --stage2` and the stt_eval step of the runbook.
- False alarms per hour are only certified when the room/held-out audio is long enough (about 30 h for 1 per 8 h); the scorecard prints the exact bound.
