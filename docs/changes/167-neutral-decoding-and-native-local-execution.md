# Neutral command decoding and native local execution

Date: 2026-10-26 IST. Speaker verification remains **41% (2.05/5), in progress**. Follow-up to [settings verification](165-voice-lock-live-verification-fixes.md).

## Measured failure

The owner reported that Notepad did not open on assessed release `0b10fd3a1416a540`. Owner verification succeeded on the speech captures, while transcription duplicated words or whole commands. A later stage watchdog blackout is retained as a separate unresolved failure. The first launch inherited `RUST_LOG=warn`, suppressing INFO lifecycle records; the assistant corrected the launch protocol and restarted the same executable with explicit audit logging. No formal rate can be inferred from the ambiguous first attempt. [First result](../testing/runs/windows1-20261026-owner-001-result.json).

The subsequent explicit hotkey capture, run `4564-1792953451045269100`, attempt2/session3, was owner-verified at 0.960, transcribed `open open open notebook.`, delivered and received by routing. An earlier automatic capture transcribed briefing-like text; neither its source nor acoustic echo is established by text logs. Extra captures must not be counted as the requested one-start trial.

## Consented one-clip experiment

The owner explicitly authorized **one** encrypted local diagnostic clip. A one-shot listener disabled diagnostics after the first completed capture, with a 90-second timeout fallback. Native readback confirmed diagnostics Off before stopping the candidate. Session6 produced exactly one sealed diagnostic file under the app's existing 24-hour retention policy. No plaintext recording or encryption/API key was printed or written by the analysis helper.

Same PCM bytes, same Groq Whisper turbo model, English and temperature 0; only the prompt varied:

| Condition | Replay1 | Replay2 |
|---|---|---|
| Neutral prompt | Open note pad. | Open note pad. |
| Existing owner vocabulary | Open note pad. Open note pad. | Open note pad. Open note pad. |

The original live capture was also repeat-loop filtered, with owner score 0.954. Duration 2.88s, no clipped samples by the stated threshold. This is diagnostic reproduction on **one clip**, not held-out accuracy, proof of the acoustic source, or speaker calibration. Two initial replay requests received HTTP403 with urllib's default user agent; a named NEXUS diagnostic user agent succeeded. [Analysis and sample hashes](../testing/runs/windows1-20261026-one-clip-analysis.json), [diagnostics Off](../testing/runs/windows1-20261026-one-clip-disabled.json), [final status](../testing/runs/windows1-20261026-one-clip-status.json).

## Narrow fixes

- Captured owner commands now select neutral decoding. The legacy vocabulary is retained for controlled comparison but disabled by source policy pending independent corpus evidence. The frontend Groq path also uses the shared neutral default, including verbose responses. Native ownership checks and repetition filtering remain unchanged.
- Source review found an independent execution gap: `recorder.ts` returns native sessions directly to Rust, but the ordinary Rust LocalCommand branch still claimed the frontend would execute them and emitted Done. Native turns now call the existing `command_center::to_local_intent` and `command_executor::execute_command` after the existing native authorization/action gates, returning the real result/error through normal speech handling. Sessionless legacy turns do not execute in this adapter, preventing double execution. Cancellation before dispatch prevents execution; cancellation after completion suppresses its late reply. Unsupported adapters keep their existing behavior.
- Tests inject the executor at the native dispatch boundary; they run no OS actions and verify one invocation, target propagation, resolver refusal/error propagation, and no invocation for legacy, cancelled or unsupported turns.

## Frozen verification

Revision `f0988d95afbfac06176c923c2227e7e79e841013`, source hash `d7b49eebf14f49913b36ec7a125e59195915e41d4950a1ac703190b84c1e475a`, isolated readiness worktree. Primary concurrent work is preserved.

The new decoder test failed on the old policy before the fix. The isolated harness runs five tests using the actual decoder/dispatch bodies plus the actual local intent mapper. Seven mutations are killed: restore biased decoding, replace neutral prompt, remove legacy/cancellation guards, remove native execution, disconnect its call, restore frontend vocabulary. The two call-path mutations are source wiring checks, distinguished from runtime tests.

Frontend687 + TypeScript/build, Worker145 + TypeScript, Python29+31, NLU512/8 sources, IPC/turn-end/hygiene/opacity/provenance/whitespace checks pass. Existing UI 11/window 1/audit 6/speaker 15 mutations plus the new 7 total 40 killed. Full Rust passes 2,537 library + 19 integration tests (38 ignored); dead-code passes 148 known findings, none new. All shared gates pass; release `d846dd346c470b60` finished in46m14s and passed provenance verification. A source change invalidates the earlier release's full-feature proof; the repaired code has **not** been live-verified yet.

After all shared gates and fresh provenance pass, repeat the real native owner turn and match its lifecycle, neutral decoding, app target, execution result and observed window. Keep stage heartbeat, enrollment attribution repeat, ten-start reconciliation, profile recovery and non-owner/replay gates open. Do not advance the queue or raise the score.

## Owner-only runtime handoff

The owner now requests **edits only** and will manually control NEXUS stopping, starting and rebuilding. No automated runtime or settings changes should follow without new authorization.

Before that instruction, the assessed release launched as PID8460. Startup reached stage ready; native readback found a valid five-clip profile and diagnostics Off. Voice Lock initially read Off, then was explicitly enabled for the previously authorized controlled trial; both routing and persisted flags read On. No repaired owner action trial completed. These are historical startup/configuration observations, not current-state or action-success claims. The pre-trial setting was Off; runtime/settings restoration is left to the owner under the new instruction. [Launch](../testing/runs/windows1-20261026-neutral-local-live-launch.json), [controlled native readback](../testing/runs/windows1-20261026-neutral-local-live-lock-on.json).

The ordinary D: instance replaced before this instruction had a manifest declaring3d1932b68b2c0f15; its runtime provenance was not verified, so that declaration is not assessed-build evidence. Score remains41% and all outstanding live gates remain open.
