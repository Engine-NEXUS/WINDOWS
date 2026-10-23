# Change 88 — Endpointing that follows the room, and a way to see cut-offs (Phase B9)

**Defect (audit #7):** the command capture decided "speech started / speech ended" with fixed RMS thresholds (0.010 / 0.006) that assume a quiet room. Next to a fan or an open window the room itself sits around 0.008, never falls below the "silent" level, and every turn ran to the 10 s cap (long waits, and long noisy buffers for the STT to hallucinate on).

## Change (`wakeword_oww.rs`)

- `track_noise_floor`: a minimum-tracking estimate of the room's RMS, updated for every audio chunk — falls fast toward quieter chunks, rises over ~50 s, so a few seconds of speech do not move it. Frozen at capture start (`CAPTURE_FLOOR_BITS`) so your own voice cannot raise the bar mid-turn.
- `endpoint_thresholds(floor)`: speech-start ≈ 3.3 × floor, keep-going ≈ 2 × floor — and **never below today's 0.010 / 0.006**. Quiet rooms behave exactly as before; only noisy rooms change (the old behaviour there was "never ends").
- `silence_limit_for` shares the 400 ms / patient-1 s rule between the live path and the simulator.
- `EndpointSim`: an offline copy of the live endpointer. `test_endpoint_sim_matches_live_path` drives it and the real atomics with the same chunk sequence at two noise floors and requires identical stop decisions, so the evaluator cannot drift from production.

## Measure it (`stt_eval --simulate-endpoint`)

Command rows are cut where the endpointer would stop and the cut audio is what gets transcribed; the row records `endpoint_cut_ms`, `speech_end_ms`, `cut_early`. The scorecard gains **"Commands cut off mid-sentence by the endpointer"** (target ≤ 1%). Sweep the one number I did *not* change blindly — the 400 ms silence limit — with `--endpoint-silence-chunks 5|6|8|10`, and `--noise-floor` to emulate a noisy room. Pick a longer limit only if the cut-off rate falls without hurting command match.

## Not changed

- The 400 ms first-pause limit (no data yet; the sweep decides).
- The thresholds are raise-only. A very quiet mic whose speech is below 0.010 is a gain/AGC issue; `stt.rs` also drops buffers with mean RMS < 0.005.

## Tests (+5 Rust, +1 Python)

noise-floor tracker, threshold scaling/bounds, sim-vs-live equivalence, noisy-room end-to-end, `simulate_endpoint` (stop point, early-cut flag, limit sweep). Rust lib 965/965 serial; Python 27/27.
