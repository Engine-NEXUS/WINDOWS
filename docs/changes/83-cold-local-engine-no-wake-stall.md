# Change 83 — A cold local speech engine no longer stalls (or fakes) a wake (Phase B3)

**Defects (audit #3, plus one found while fixing it):**
1. The local STT sidecar is killed after 60 s idle (`lazy_stt.rs`, deliberate: RAM budget). When the stage-2 wake verifier then had to use it (no Groq key, local-only mode, or any candidate that goes through the STT gate), `transcribe_local` waited up to 20 s for the model to load while the verifier's own timeout was 10 s — the user saw a dead orb for 10 s before the wake fired anyway.
2. `verify_candidate` failed **open** on every error path (too short, runtime error, STT error, timeout). That is right for a real stage-1 detection, but barge-in candidates (speech while TTS plays) carry `prob = 0.0` — no neural evidence — and fired a phantom wake whenever the STT call failed.
3. A Ghost Mode session that paused > 60 s between commands lost the sidecar, so the next command paid the cold load.

## Fix

- `verify_plan(local_path, local_ready, prob)` (pure): if the verifier would use the local sidecar and it is not answering, **skip the cross-check at once** — fire on the neural evidence (and start the sidecar so the command that follows is not cold), or drop the candidate when there is no evidence. Groq users are unaffected.
- `fail_open_allowed(prob)` (pure, threshold `STAGE1_EVIDENCE_FLOOR = 0.3`, same as the instant-fire path): every fail-open path in the verifier now goes through one helper; evidence-free candidates are suppressed instead of fired.
- `lazy_stt::idle_kill_due`: the idle kill is deferred while a Ghost session is active (RAM is reclaimed on the next check after it ends). The 600–700 MB budget is otherwise unchanged; keep-alive stays off.
- `stt::stt_uses_local_path` / `local_path_for` expose the "which engine will this use" decision.

## Trade-off

For local-STT users with `verifyWake` on, the first wake after > 60 s idle is neural-only (not STT-verified); that wake also warms the engine, so the next one is verified. `verifyWake` is ON by default in the code (`commands.rs` `default_verify_wake` / `read_verify_wake` return true when the setting is missing or unreadable); an earlier version of this note said "off by default", which was wrong (corrected in change 91).

## Tests (+4)

`test_verify_plan_cold_local_engine`, `test_fail_open_needs_neural_evidence`, `test_local_path_for`, `test_idle_kill_waits_for_ghost_session_to_end`. Rust lib 947/947 serial.

## Not changed / measure

The 10 s verifier timeout itself is unchanged (it now only bounds a slow Groq call or an engine that dies mid-request). Use `wake_eval --stage2 --engine`-side timings (`stage2_ms` in the scorecard's "Wake speed … with speech check" row) for real latency.
