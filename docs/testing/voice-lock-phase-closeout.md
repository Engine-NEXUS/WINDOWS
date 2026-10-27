# Voice Lock phase closeout and owner handoff

2026-10-26. This is a source-work checkpoint, not release acceptance. The owner controls NEXUS builds, runtime and settings manually. No current process, lock setting or installed profile is inferred from historical observations.

[Current source assessment](runs/windows1-20261026-phase-closeout-assessment.json) and [change170](../changes/170-voice-trial-accounting-and-phase-closeout.md): Python90+server31, frontend692/Worker145 with type checks, source guards and 79 mutations pass. Complete current Rust/dead-code/release/device gates remain pending.

## Phase status

| Phase | Source work / automated coverage | Remaining completion gate |
|---|---|---|
| P1 capture accounting | Unique sessions, lifecycle reconciliation, stale-result safeguards and isolated mutations | Exact-release ten-start batch must reconcile all ten outcomes, including failures; separately confirm action results |
| P2 native settings | Effective state, verified Save, cross-window drafts and accessible status; shipping settings device proof exists for an older exact candidate | Recheck the candidate being accepted; real profile failure/recovery and separate legacy prototype remain open |
| P3 verifier comparison | Scalar benchmark and enrollment-isolated embedding score adapter; corpus leakage, template isolation and report privacy tests | Consented independent corpus, pinned actual models/preprocessing, export/license audit, measured OWW-versus-speaker-model comparison; no model selected |
| P4 command fidelity | Neutral decoding/native executor repair; capture/intent/target/action outcomes must be measured separately | Held-out real commands, silence, ambient speech, replay and TTS-overlap checks on a fresh exact release; repaired owner action not yet proven |
| P5 acceptance/recovery | Legacy CSV results no longer qualify acceptance; [richer evidence accounting](voice-trial-evidence-protocol.md) supports independently reviewed trial metadata | At least95% owner acceptance and zero observed locked non-owner executable actions across at least300 qualified negatives, plus unfamiliar live voices/replay, restart, missing/corrupt/deleted-profile/model/capture/confirmation/keyboard recovery |
| P6 final acceptance | Source guards, selected suites and mutations are recorded in the current change record | Freeze one source; complete Rust library/integration and every shared gate on that source; owner-built fresh release provenance; real-device evidence; independent second review and rubric thresholds |

Every row above remains incomplete until its final gate is supported. Automated checks do not grant voice authority or promote the queue. Score remains41% (2.05/5).

## Current test boundary

Frontend/Worker tests and no-output TypeScript checks can run without rebuilding or operating the app. Python evidence tools use synthetic fixtures. Native mutation checks compile and run isolated test harnesses with temporary profiles/stubs; they do not rebuild the NEXUS application, open a WebView or operate its installed profile. These checks are narrower than the complete application Rust/integration suite and current dead-code build gate. Historical frozen-release results remain tied to their original revision.

## Exact-release evidence packet

Before live acceptance, retain the owner-produced build identity, revision, source-input hash and executable hash; verify provenance without silently replacing the running app. Keep automated check outputs for the same snapshot, including ignored tests and failures. New source does not inherit an older candidate's live proof.

For each requested voice attempt, record an anonymous trial ID and match one native run/session to it using capture-audit logs. Keep lock readback, verification result, valid executable-command status and independent observed action outcome distinct. No-speech, lock-off, missing capture, unavailable verification and calibration/enrollment rows cannot fill the300-negative denominator. A locked unsafe action still counts as a failure even if its trial is otherwise excluded.

Use separately labelled owner, unfamiliar live non-owner, owner replay, other-speaker playback and synthetic playback observations. Report sample/session/speaker grouping and uncertainty. Replaying the same recording300times cannot establish unfamiliar live-speaker performance. Solo owner tests cannot close that gate. Do not acquire more diagnostic recordings or reuse the retained one-clip failure diagnostic as held-out acceptance evidence without the appropriate independent consent/data protocol.

## Real-device cases still to execute manually

1. Repaired owner action: Voice Lock's native state confirmed, one hotkey capture, intended harmless app action observed, transcript/capture/dispatch and independent action result reconciled. A verified speaker with an incorrect or absent action fails command fidelity, not necessarily speaker verification.
2. Exact-candidate enrollment isolation: ten displayed phrase clips, normal wake recovery for all three phrases, cancellation during countdown/recording/pending permission. Enrollment clips are not acceptance trials.
3. Owner acceptance across distances/noise, with a fresh held-out set after any model/threshold/pipeline adjustment. Do not tune on acceptance results.
4. Separate unfamiliar live-speaker, owner replay, other-speaker and synthetic playback cases. Include owner wake followed by non-owner command and non-owner confirmation answers; rejected/unavailable voices must cause no executable action.
5. Restart and profile/model/capture/recovery checks in a controlled environment. Preserve the real enrollment/profile; do not automatically corrupt/delete installed user data. Keyboard/hotkey recovery must remain available without granting a voice bypass.
6. Stage heartbeat/blackout check throughout the real interaction. Keep failures, exceptions and unmatched captures in the ledger. Then second-pass compare code, logs, observed device behavior, documentation and the existing weighted rubric.

The feature can advance only with raw score>=90%, B>=90/T>=70/L>=70/S>=80/I>=60, every shared check, fresh provenance and no known contradictory failure. A blocked or failed live check leaves the queue here.
