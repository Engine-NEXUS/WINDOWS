# Change 82 — A verified wake can no longer be eaten by the cooldown (Phase B1)

**Defect (audit #2):** in the wake receiver loop (`wakeword_oww.rs`) the 1.5 s cooldown clock (`LAST_NEURAL_FIRE_MS`) is stamped when a stage-1 candidate *starts* stage-2 verification. When the STT check accepted it (typically 0.25–2 s later), `fire()` set a shared `VERIFIED_BYPASS` flag and sent a second candidate to the same loop — which hit the cooldown check first and was dropped whenever verification took < 1.5 s. Two consequences:
1. the user said "NEXUS", it was verified, and nothing happened (a "dead" wake);
2. `VERIFIED_BYPASS` stayed `true`, so the *next unrelated* stage-1 candidate (≥ 1.5 s later) consumed it and fired **without verification**, defeating the false-alarm gate.

## Fix

- `WakeCandidate` gains `verified: bool`; the verifier's self-send sets it, all stage-1 sends set `false`.
- New pure `candidate_gate(verified, now_ms, last_fire_ms)` → `FireVerified` (always, ignoring the cooldown) / `Suppress` (unverified inside 1.5 s) / `Proceed`. The receiver loop uses it; a verified candidate skips speaker re-verification and the instant/verify branches and goes straight to the fire path.
- The shared `VERIFIED_BYPASS` static is deleted, so no stale token can exist.
- Unchanged on purpose: a candidate that *starts* verification still stamps the cooldown (it rate-limits repeat STT calls on one utterance), and a rejected candidate still burns its 1.5 s window.

## Tests (+2)

`test_candidate_gate_verified_wake_bypasses_cooldown` (verified inside the window fires; unverified inside suppressed; edge at 1500 ms; backwards clock), `test_unverified_candidate_never_inherits_a_verified_token`. Rust lib 943/943 serial.

## Limits

The race itself (thread timing around the live audio loop) isn't reproducible offline; the decision rule is pinned by unit tests, and the end-to-end effect shows up in the wake-latency/recall rows of the scorecard run with `--stage2` plus live use (say "NEXUS" right after a quiet period and again within 2 s; both should wake).
