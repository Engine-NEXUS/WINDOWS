# Voice Lock evidence and reliability implementation plan

Date: 2026-10-25. Scope: Windows 1.0 speaker-verification work card, queue position 1. Starting score remains 41% (2.05/5). This plan does not authorize promotion to the next feature before the existing live gates pass.

## Research and observed gaps

- [Architect-guided local review](../research/elevenlabs-architect-guided-nexus-review.md): applied installed ElevenLabs guidance to NEXUS source and measured failures. No remote Architect/MCP consultation occurred.
- [External review brief](../research/elevenlabs-architect-review-brief.md): prepared questions; not a received expert opinion.
- Ten reported owner attempts produced seven identifiable completed captures. Existing native session IDs advance on abort only, so ordinary successive captures reuse a label.
- The effective native Voice Lock setting disagreed with the owner's earlier UI/save report. P2 later passed shipping settings device checks on its exact candidate; this does not establish speaker accuracy or profile recovery.
- Several command transcripts duplicated words; one differed from the requested command. A single consented same-clip experiment later reproduced owner-vocabulary repetition; neutral decoding and native local execution were repaired. Acoustic echo and general held-out fidelity are still unproven.
- Current owner-only calibration and OWW feature comparison have no measured unfamiliar-speaker distribution. Seven verified captures are not broad identity/replay proof.

## Ordered work cards

| Phase | Implementation | Regression and measurable acceptance | Status |
|---|---|---|---|
| P1 | Unique native capture sessions; structured lifecycle records for each request; terminal outcomes for blocked, duplicate, cancellation, supersession, unavailable receiver, stale result, empty/phantom, owner hold, STT error/filter, and transcript delivery. Link dispatch entry using the session. Record run/executable provenance without speech content. Add a log reconciliation tool. | Ten starts reconcile to ten outcomes, including failures. New capture invalidates older in-flight results. Channel failure and unwinding cannot silently lose an owned lifecycle. Duplicate starts preserve the active buffer and have their own outcome. Audit neither grants authority nor changes the lock. | Automated gates pass; release/live cross-check pending |
| P2 | Effective native lock status distinct from unsaved draft; native readback after Save and on reopen; visible error/mismatch. | Tests for failed/slow save, stale window, refused enable and profile failure. Real on/off/save/reopen walkthrough matches native state. | Repaired release 0b10fd3a1416a540 passes shipping settings device checks and confirmed controlled faults; legacy prototype and real profile recovery remain pending |
| P3 | Frozen comparative experiment: existing OWW verifier versus dedicated speaker embeddings. Independent owner/impostor calibration and held-out acceptance partitions. | Per-condition FAR/FRR, owner acceptance, actions, sample sizes, dependence and uncertainty; RAM/latency/license/export audit. Select only from measured results. No threshold tuning on acceptance data. | Offline score benchmark and enrollment-isolated embedding scorer implemented (changes 168–169); preliminary primary-source candidate audit recorded; actual corpus, artifact/export/license verification and measured comparison pending |
| P4 | Diagnose command fidelity with consented failed audio: capture boundaries/TTS overlap and neutral decoding versus domain hints. Narrow fix chosen from reproduction. | Held-out commands, silence, ambient speech and replay; measure unintended executable intent separately from transcription quality. Keep correction and hallucination guards; do not silently repair ambiguous actions. | Same-clip prompt duplication reproduced; neutral decoding and native local execution repaired in source (change 167); held-out/live fidelity remains pending |
| P5 | Separate owner, unfamiliar live speaker, owner replay, other-speaker playback and synthetic playback trials; restart/corrupt/missing profile/deletion/model failure/confirmation handoff/recovery. | At least 95% observed owner acceptance with sample size and interval; zero observed non-owner executable actions in at least 300 qualified negatives with uncertainty and grouping. Solo testing does not establish live unfamiliar-speaker performance. | Qualified declared-metadata reporter and stricter legacy accounting implemented (change170); actual live/recovery proof pending |
| P6 | Freeze assessed source, run full shared gates, verify fresh release provenance, repeat owner interaction checks, second review against code/logs/docs, then update rubric. | Raw score at least 90%, B≥90/T≥70/L≥70/S≥80/I≥60; all automated and real-device gates satisfied; no known contradicting failure. | Current permitted suites/guards/mutations recorded (change170); full exact-source Rust/dead-code, owner-built provenance and live gates pending; not ready |

## P1 implementation contract

Use existing native capture and routing ownership. Keep diagnostic attempt identity distinct from cancellation generation and from a successful action. A delivered transcript means the event was emitted; it does not mean an action executed or the user saw it. A dispatch-entry marker means routing received the session, not action success.

Audit records contain run ID, attempt ID, native session, origin, stage/outcome, monotonic elapsed time, UTC timestamp and executable-manifest linkage. They contain no transcripts, recordings, profile vectors, speaker scores, keys, file paths or contact names. Existing log retention remains unchanged; crashes leave explicitly unfinished attempts in the reconciliation report rather than invented successful terminals.

The capture callback must snapshot its session/lifecycle with its buffer before another start or abort can change them. New capture sessions must be distinct even if no cancellation occurred. Cancellation and duplicate-start safeguards remain in force. Late buffers cannot mint proofs or reach transcript delivery after cancellation/supersession.

## Verification and release procedure

P1's concrete owner protocol is [the native capture accounting checklist](../testing/voice-capture-accounting-checklist.md). All rows are pending until a real run supplies evidence.

1. Add lifecycle/reconciliation regressions and see them fail before their narrow fixes.
2. Run focused Rust and Python checks; mutation-check terminal and stale-session rules. Run relevant native wiring tests and frontend type checks if wire behavior changes.
3. Run complete Rust library/integration, frontend, Worker, Python, NLU and repository gates on a recorded source before calling any phase production ready.
4. The owner builds and controls the runtime manually (2026-10-26 instruction). Source editing and offline tests may proceed; do not automatically build, start, stop, restart NEXUS or change its settings. A later owner-produced release still requires provenance verification before live evidence can be attributed to it.
5. Owner performs one capture per trial ID. Reconcile every request, including blocked/cancelled attempts. Record unmatched evidence and failures in the live ledger.
6. The owner explicitly authorized P2 implementation on 2026-10-25 while P1 release/live cross-check remains pending. This changes implementation order only: neither phase is live-proven, and feature 2 remains gated on the complete speaker-verification card.

## Solo testing boundary

The owner can test their own acceptance, cancellation, settings, restart and profile recovery. Independently played audio can measure playback rejection when it actually reaches verification. Own-voice imitation, muted NEXUS TTS, and repeated identical clips do not substitute for independent live non-owner evidence. If participants are unavailable, retain that gate as pending.

## P2 implementation record

[Effective Voice Lock state and verified Save](../changes/164-voice-lock-native-state-ui.md) records the narrow UI fix, test evidence and pending real-window checklist. The P1 frozen release remains separate; P2 edits do not invalidate or inherit that release identity.

[Live verification failures and repairs](../changes/165-voice-lock-live-verification-fixes.md) supersede any implication that P2 is complete.

## P3 infrastructure record

[Change 168](../changes/168-offline-speaker-verifier-benchmark.md) and [the versioned benchmark protocol](../testing/speaker-verifier-benchmark.md) document the offline comparison tooling. The requested OpenCode GLM 5.3 Flash Free session returned no edits; authorized local fallback implemented the tools. Synthetic checks are not corpus or live accuracy evidence. No candidate, threshold installation or queue promotion follows.

[Enrollment-isolated exported embedding scoring](../changes/169-enrollment-isolated-embedding-scores.md) extends P3 without executing a model or accessing installed profiles. [Candidate research](../research/speaker-embedding-candidate-audit-2026-10-26.md) is preliminary; no candidate is selected.

## P4/P5 evidence and P6 checkpoint

[Change170](../changes/170-voice-trial-accounting-and-phase-closeout.md) records the remaining source-only evidence edits, failed-before-fix review tests, broad permitted suite results and 79 mutation checks. [Phase closeout matrix](../testing/voice-lock-phase-closeout.md) retains each unmet gate; [trial schema](../testing/voice-trial-evidence-protocol.md) never certifies readiness from declared metadata. No complete application build, fresh executable or new corpus/device proof was produced.
