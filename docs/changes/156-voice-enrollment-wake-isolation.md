# Voice enrollment: suppress normal waking and record all phrase variants

Date: 2026-10-25. Status: shared checks and replacement release complete; owner recheck pending.

## Observed failure

The owner completed enrollment on release `812a5b3333669e02`, then reported that saying the prompted "Nexus" also woke the assistant. The old component recorded with browser getUserMedia without pausing the native listener. The owner also requested "Hey Nexus" and "OK Nexus" in enrollment. The failed interaction is retained in the live-proof ledger. Enrollment success is not an accuracy result.

## Changes

- The entire ten-clip session owns the existing microphone baton: await native pause before any countdown or browser recording, stop browser tracks before native resume, and restore listening on success, error or component navigation. A pause/resume device error is shown instead of silently succeeding.
- Native pause invalidates existing STT capture. Audio callbacks, queued wake candidates, delayed wake emission and new native capture are blocked while the frontend owns the microphone. Resume resets wake-model state and the startup grace window before accepting more input.
- Both five-clip halves now prompt Nexus, Hey Nexus and OK Nexus; the stored phrase metadata includes all three. The five held-out clips remain excluded from the stored profile.
- Eight frontend lifecycle regressions cover pause ordering, all ten clips, recording/permission failure, cancellation, cleanup failure, resume failure and prompt coverage. Two Rust regressions check that native capture cannot open during enrollment and that pause waits for an in-flight wake dispatch. A shared dispatch barrier also serializes pause acknowledgment with the final wake check before any cue or turn. Two new frontend safety mutations remove the awaited pause or resume.

## Source and validation

Assessed snapshot: `ade0bfdddd99d52efe14da089d33ee5b7493d8a0` in the existing managed worktree. Build-input SHA-256: `328ab8cd23bf39fde595a16ee15dbd18ecbaf81f2c5c7ec4c76203a2c604cf11`. Main D: checkout and concurrent edits are preserved. Full Rust library/integration passes (2,524 + 19, zero failures, 38 ignored), as do frontend TypeScript/build + 655 tests, Worker TypeScript + 145 tests, Python 48, NLU validation, 14/14 safety mutations and repository guards. Dead-code passes (148 known, none new), and isolated latency passes at 742 ms against its retained 5,000 ms budget. The optimized release is building; replacement provenance and owner re-enrollment remain pending.

Speaker verification remains 41% (2.05/5), in progress. Do not advance the queue or reuse earlier enrollment as live acceptance proof. After the replacement candidate launches, verify that enrollment causes no wake/voice turn for all three phrases, then verify ordinary wake recovery after completion and cancellation. Run the existing held-out owner/non-owner/TV protocol only after that interaction passes.

Earlier compile attempts were stopped as superseded after the second pass found the pending-wake timing race. Their outputs are retained as `*-superseded.log`; they do not count as completed tests. Final native checks and release run sequentially with one Cargo job. IPC now passes with 60 known unused commands and six one-sided events after removing the newly used pause command from the baseline.

The pause/resume IPC operations run in blocking workers; waiting for an in-flight wake dispatch cannot block Command Hub's UI event loop. The final native run was restarted after this change; the earlier UI-thread compile output is retained separately.

## Pending-permission cancellation follow-up

A new lifecycle regression failed on the earlier candidate: navigating away while `getUserMedia` permission was pending did not reach the native resume. The release attempt was stopped and never launched. The task now races cancellation, while late stream acquisition still stops its tracks and refuses recording. A ninth lifecycle regression covers that case, and a fifteenth safety mutation removes its cancellation listener. Current snapshot `50dcda219018d3bbfff8be388b77610bee135749`, hash `086e534ae829a7abbdf1e793f584297e81b8f2c8c16625bcfeee03ea505572c4`; [current assessment](../testing/runs/windows1-20261025-enrollment-v2-assessment.md). Full gates and the replacement release are rerunning.

## Verified replacement

Replacement release 031caf5848621e18, source 50dcda219018d3bbfff8be388b77610bee135749, is provenance-verified before and after launch as PID 28248 at 2026-10-25T11:19:07Z. Optimized build completed in 49m 15s. All shared gates pass: 2,524 Rust library + 19 integration (38 ignored), frontend 656 and TypeScript/build, Worker 145 and TypeScript, Python 48, NLU, repository guards, 15/15 safety mutations. DirectML hash checked. Startup reached stage ready with microphone/wake models detected. Missing optional Tier 3 command models, Groq menu-model, registry and startup wake-suppression warnings are retained. Owner re-enrollment and accuracy remain pending; score stays 41% (2.05/5).

Evidence: [release identity](../testing/runs/windows1-20261025-enrollment-v2-release.json), session and filtered startup logs in the same run directory.

## Owner enrollment interaction result

Owner live report on candidate 031caf5848621e18: all ten enrollment clips completed without waking NEXUS. Enrollment wake isolation passes this interaction check. Normal wake recovery, cancellation recovery, owner/non-owner accuracy and profile recovery remain pending. These ten calibration/enrollment clips count as zero acceptance trials. Score remains 41% (2.05/5); queue stays on speaker verification.

### Post-enrollment wake recovery

Owner confirmed normal waking with Nexus, Hey Nexus and OK Nexus after the ten-clip enrollment on release 031caf5848621e18, with Voice Lock off. Enrollment isolation and post-completion wake recovery have passed owner interaction checks. Cancellation recovery and locked owner/non-owner/TV accuracy remain pending. No accuracy trials counted; score stays 41%.

### Cancellation interaction result

Owner reports both enrollment cancellation checks passed on 031caf5848621e18: navigation during countdown and during recording stopped recording and restored normal waking, with Voice Lock off. Pending-permission navigation is not live-verified. Locked owner/non-owner/TV accuracy, restart and profile recovery remain pending. Score stays 41%; no accuracy trials counted.

## Live build attribution correction

Build attribution correction: the owner supplied console logs showing nexus start replaced the assessed candidate with D: release 6f4e1063a38683a3 at 16:54, followed by 3d1932b68b2c0f15 at 17:36. Earlier owner enrollment/wake/cancellation reports remain observations, but exact assessed-build attribution is unconfirmed and must be repeated. The Notepad turn at 17:37:41 was owner=unenrolled with Voice Lock off; it is excluded from locked accuracy counts. Current device setting was confirmed false; a profile file exists. Candidate 031caf5848621e18 was restored with unique logs, PID 28952. No score increase or queue advancement.
