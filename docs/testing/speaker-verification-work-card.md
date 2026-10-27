# Windows 1.0 work card 1 — speaker verification

## Current P4/P5/P6 source checkpoint (2026-10-26)

[Change170](../changes/170-voice-trial-accounting-and-phase-closeout.md) repairs legacy qualification and adds [declared trial accounting](voice-trial-evidence-protocol.md), with identity acceptance and command fidelity separate. Python90+server31, frontend692/Worker145 and type checks, source guards and79 mutations pass. [P1–P6 matrix](voice-lock-phase-closeout.md) preserves all unmet gates: no current complete Rust/dead-code/release/corpus/live proof and no runtime operation. Score41%, no queue advancement. Historical process/launch notes below are not current instructions.

## P3 exported embedding scoring (2026-10-26)

[Change169](../changes/169-enrollment-isolated-embedding-scores.md) adds a reproducible scorer for explicitly supplied offline embeddings, isolated enrollment and vector-free provenance, plus a narrow resource-summary overflow repair. Python63 and 18 offline mutations pass. Candidate documentation is audited preliminarily; actual artifacts, extraction, corpus, comparison and all live gates remain pending. No runtime/settings/build operations. Score41%, queue unchanged.

## P3 source-only follow-up (2026-10-26)

The [offline verifier benchmark](speaker-verifier-benchmark.md) now validates paired model scores, disjoint partitions and calibration-only thresholds. See [change 168](../changes/168-offline-speaker-verifier-benchmark.md): 47 Python script tests and eight mutations pass. No corpus comparison, model selection, runtime operation or build occurred. Historical runtime labels below do not describe the current process; the owner handles runtime/builds manually. Score stays 41%, and all existing live gates remain open.

## Current implementation follow-up

[Voice Lock evidence/reliability plan](../features/152-voice-lock-evidence-and-reliability-plan.md) now orders the work from the [Architect-guided local research](../research/elevenlabs-architect-guided-nexus-review.md). P1 implements unique native sessions, per-request terminal accounting and log reconciliation. See [change 163](../changes/163-voice-capture-accounting.md). No score or queue advancement follows from these diagnostics; a fresh verified release and owner ten-trial reconciliation remain required before the phase is complete.

**Live failure follow-up:** the owner reported that enrollment also wakes NEXUS on candidate `812a5b3333669e02`. The corrected replacement 031caf5848621e18 is verified and running; see [change 156](../changes/156-voice-enrollment-wake-isolation.md). Re-enrollment on the replacement candidate is required. Speaker verification stays at 41%.

Status: **in progress**. Queue position 1 of 65. The scorecard remains provisionally 41% while owner live proof is pending.

The current [65-feature Windows queue](windows1-production-queue.md) uses the programme's 90% requirement; older 80% report verdicts are historical.

## Baseline

- Starting commit: `fd89438fef88dc8270998048d00d1d73baab8e27` on `feat/action-kernel`, with extensive pre-existing working-tree edits.
- Assessed local source snapshot: `954c57471be0564f69763f73b43e0868f50c74ca`, isolated in `C:\Users\Chitkul Lakshya\.codex\worktrees\windows-readiness\ULTRON`; frozen build-input fingerprint `3087f61760442b846c71baaba710b625cf5ebe961e81fa5d4677c381e150ce5e`. Concurrent mail edits were preserved in the original checkout. Branch and primary staging index were preserved. The frozen tree was clean and its fingerprint matched after all tests. See [the run record](runs/windows1-20261025-assessment.md).
- The frozen complete Rust run passes 2,522 library and 19 integration tests, with zero failures and 38 explicitly ignored tests. This includes 24 voice-profile tests, the real-model enrollment/reset regression, and settings recovery tests. Twelve safety mutations are killed. The original five-second Smart Turn budget passes in isolation at 721 ms best of three.
- NLU validation failed on seven training rows overlapping validation, calibration, or the frozen test by normalized text or phrase family. Those rows were quarantined and the split lock regenerated after validation passed. The frozen test rows were not changed.
- `git diff --check` passes. IPC passes with a reduced baseline of 61 known unused commands and 6 known one-sided events. The frozen dead-code audit passes with 148 known findings and none new.
- Frozen frontend type checking/build and all 647 tests, Worker type checking and 145 tests, Python 48 tests, NLU validation, turn-end, hygiene, and window opacity pass. The Windows keyring backend persisted a reserved test entry across two processes and deleted it; this is storage-prerequisite evidence, not owner accuracy.
- Earlier failures and superseded source associations are retained in the run record. Use one Cargo job after a parallel compile hit Windows paging-file error 1455. Fresh release `812a5b3333669e02` passes provenance verification before and after startup. The actual owner/profile walkthrough remains pending. Startup reached stage ready with microphone and wake models detected; optional-model and registry warnings are retained in the run record.

## Gap and implementation

- Enrollment existed only in Setup and used a fixed threshold. Command Hub Audio now embeds it. Five enrollment recordings and five withheld owner recordings set a provisional threshold. Non-owner data and real-device accuracy are still needed.
- A missing/corrupt profile, failed model, corrupt settings, or uncertain turn could previously proceed when Voice Lock was enabled. Wake, capture, frontend pending confirmations, and command dispatch now require positive verification. Keyboard and direct UI controls remain available for recovery.
- A stored profile was plaintext JSON. Valid profiles are now sealed with the Windows user key; legacy plaintext is migrated on load. Missing key or corrupted ciphertext fails closed.
- The profile now has its own Windows key, so wiping conversation memory cannot rotate it. Profiles from the earlier shared-key draft migrate after successful authentication. A failed file deletion is reported as an error rather than success.
- The second pass found that enrollment skipped the live pipeline's mel normalization and reshape. Enrollment now reuses that pipeline off the UI thread; both enrollment and live checks reset between clips, select the peak-energy 1.28-second window, and reject malformed feature output. A real-model regression checks execution and clip isolation; this synthetic tone test does not establish speaker accuracy.
- Explicit re-enrollment can replace a corrupt old profile after valid clips are processed. Corrupt/malformed settings retain the lock in both the UI and dispatch; unrelated or stale full-settings saves preserve it. Only a patch explicitly naming Voice Lock can change it. Twelve isolated safety mutations are killed.
- The old engine-level speaker check was loaded independently of the setting and could reject wakes even when Voice Lock was off. The receiver now owns the single wake check.

## Acceptance criteria

1. All shared library, integration, frontend, Worker, Python, NLU, IPC, dead-code, turn-end, hygiene, opacity, whitespace, and provenance checks pass on one recorded source snapshot.
2. Safety regression tests fail when any locked non-verified path is allowed. Mutation checks cover the voice gate, profile validation, encryption/authentication failure, and calibrated threshold rules.
3. A fresh release is used for owner enrollment, held-out speech at different distances and noise levels, non-owner speech, TV replay, restart, missing/corrupt profile recovery, deletion, and keyboard/hotkey recovery. `Ctrl+Shift+S` opens Command Hub without a voice turn; use Audio to re-enroll or explicitly turn Voice Lock off. `Ctrl+Space` does not bypass owner verification.
4. Count owner rejection and non-owner acceptance separately. Target at least 95% owner acceptance and zero executable non-owner actions across at least 300 negative trials. Record trial counts and uncertainty in the live-proof ledger.
5. Recheck the implementation, logs, ledger, scorecard, and user-facing claims. Raise the score only if raw score is at least 90%, with B≥90, T≥70, L≥70, S≥80, and I≥60. Do not advance to installer work before owner live proof.

## Live trial protocol

- The five calibration recordings are not accuracy trials. Use new owner recordings for the acceptance measurement, across quiet/noisy conditions and different distances, and report the sample size and confidence interval.
- Do not tune the threshold on the acceptance trials. If a failure changes the model, threshold, or extraction pipeline, preserve the failed results and use a new held-out trial set for the new release.
- Test both the wake-word path and `Ctrl+Space` capture. The latter ensures that wake-word misses do not conceal a failure in the owner gate; it must still require owner verification.
- Each negative accuracy trial must contain a valid captured command that would otherwise run, such as opening a harmless app. Include real non-owner speech and TV/speaker playback. Confirm the verification result in the log and independently check whether an executable action occurred. Map trial IDs to ordered timestamps/log excerpts.
- Include an owner wake followed by a non-owner command, and non-owner answers to pending confirmations. Speech must not inherit owner status from an earlier wake or turn.
- Vary speakers, utterances, distance and noise; record repeated clips and counts by condition. The reported binomial intervals assume independent trials, so 300 replays of one recording cannot support a broad claim about unfamiliar voices.
- Record missing/corrupt profile, model/capture failure, restart, deletion, and hotkey recovery separately. Keep failures in the evidence; do not count a test that never reached voice verification toward the 300 measured negative trials.
- Enable Voice Lock explicitly for the controlled trials, then leave it off until the full acceptance decision is supported. No production-ready claim follows from enrollment or a few successful attempts.

Use `docs/testing/speaker-trials-template.csv` as the anonymized trial sheet. Record one row per real attempt with a unique ID, speaker class (`owner`, `non_owner`, or `tv_replay`), condition, verification result, and whether any executable action occurred. Run `python scripts/speaker_trial_summary.py <completed.csv>` for descriptive legacy rates and uncertainty only. The old columns cannot establish capture/lock/provenance qualification; `numeric_targets_met` is now always false for this format. Separate speaker-class and condition counts remain available. Keep the completed trial sheet and matching application logs as evidence; the template alone is not live proof.

## Historical controlled restart recipe — superseded

**Historical record only:** the recipe below refers to an older frozen candidate and must not be used as current launch instructions. The owner controls builds/runtime manually, and no old PID or setting is assumed current.

The earlier release `812a5b3333669e02` failed the enrollment interaction and is superseded. The revised source is `50dcda219018d3bbfff8be388b77610bee135749`; its optimized release 031caf5848621e18 is verified and running as PID 28248. See [the current assessment](runs/windows1-20261025-enrollment-v2-assessment.md). Re-enroll on this replacement now.

For a later restart trial, quit the running NEXUS through its tray first. This recipe refuses a failed or different assessment and writes unique stdout/stderr logs. It does not clear previous session evidence.

```powershell
Set-Location -LiteralPath "C:\Users\Chitkul Lakshya\.codex\worktrees\windows-readiness\ULTRON"
$candidateRelease = node scripts/build-provenance.mjs verify | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or $candidateRelease.source -ne '086e534ae829a7abbdf1e793f584297e81b8f2c8c16625bcfeee03ea505572c4' -or $candidateRelease.identity -notmatch '^[0-9a-f]{16}$') {
    throw 'The verified release does not match this assessed source; stop here.'
}
if (Get-Process -Name nexus -ErrorAction SilentlyContinue) {
    throw 'Quit the running NEXUS before this controlled restart.'
}
$trialLogDir = Join-Path ([Environment]::GetFolderPath('ApplicationData')) 'com.nexus.assistant'
New-Item -ItemType Directory -Path $trialLogDir -Force | Out-Null
$trialStamp = [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')
$trialLogPrefix = Join-Path $trialLogDir ("nexus_readiness_" + $candidateRelease.identity + "_" + $trialStamp)
$candidateProcess = Start-Process -FilePath $candidateRelease.executablePath -WorkingDirectory ([IO.Path]::GetDirectoryName($candidateRelease.executablePath)) -WindowStyle Hidden -RedirectStandardOutput ($trialLogPrefix + '.log') -RedirectStandardError ($trialLogPrefix + '_err.log') -PassThru
[pscustomobject]@{
    sourceRevision = '50dcda219018d3bbfff8be388b77610bee135749'
    sourceHash = $candidateRelease.source
    buildIdentity = $candidateRelease.identity
    processId = $candidateProcess.Id
    launchedAtUtc = [DateTime]::UtcNow.ToString('o')
    stdoutLog = $trialLogPrefix + '.log'
    stderrLog = $trialLogPrefix + '_err.log'
} | ConvertTo-Json | Tee-Object -FilePath ($trialLogPrefix + '_session.json')
```

After startup, use Ctrl+Shift+S, then Audio. Keep Voice Lock off for fresh ten-clip enrollment. The three phrases must not wake the assistant during recording. Wait two seconds after completion before checking ordinary waking; use the current assessment's cancellation checklist too. Only then begin the separate owner/non-owner accuracy protocol with Voice Lock explicitly enabled for controlled trials.

## Owner enrollment interaction result

Owner live report on candidate 031caf5848621e18: all ten enrollment clips completed without waking NEXUS. Enrollment wake isolation passes this interaction check. Normal wake recovery, cancellation recovery, owner/non-owner accuracy and profile recovery remain pending. These ten calibration/enrollment clips count as zero acceptance trials. Score remains 41% (2.05/5); queue stays on speaker verification.

### Post-enrollment wake recovery

Owner confirmed normal waking with Nexus, Hey Nexus and OK Nexus after the ten-clip enrollment on release 031caf5848621e18, with Voice Lock off. Enrollment isolation and post-completion wake recovery have passed owner interaction checks. Cancellation recovery and locked owner/non-owner/TV accuracy remain pending. No accuracy trials counted; score stays 41%.

### Cancellation interaction result

Owner reports both enrollment cancellation checks passed on 031caf5848621e18: navigation during countdown and during recording stopped recording and restored normal waking, with Voice Lock off. Pending-permission navigation is not live-verified. Locked owner/non-owner/TV accuracy, restart and profile recovery remain pending. Score stays 41%; no accuracy trials counted.

## Live build attribution correction

Build attribution correction: the owner supplied console logs showing nexus start replaced the assessed candidate with D: release 6f4e1063a38683a3 at 16:54, followed by 3d1932b68b2c0f15 at 17:36. Earlier owner enrollment/wake/cancellation reports remain observations, but exact assessed-build attribution is unconfirmed and must be repeated. The Notepad turn at 17:37:41 was owner=unenrolled with Voice Lock off; it is excluded from locked accuracy counts. Current device setting was confirmed false; a profile file exists. Candidate 031caf5848621e18 was restored with unique logs, PID 28952. No score increase or queue advancement.

### Voice Lock draft/save observation

Owner reported toggle on/no executable result. Readback found speakerVerification=false and the running process was D: build 3d1932b68b2c0f15 (PID 17928). Command Hub toggles are drafts until Save; the assistant's test instructions omitted that step. Exclude this attempt from locked accuracy counts. Save and verify the native setting before another trial; assessed-build attribution must also be resolved. No owner rejection or acceptance rate established.

### Native lock enable readback

Owner reported Saved successfully, but live Tauri IPC voice_lock_enabled and get_settings both returned false; a valid five-clip profile with all three phrases loaded. With existing authorization to enable for controlled trials, settings_patch({speakerVerification:true}) succeeded and both native readbacks returned true. This proves current lock configuration, not speaker accuracy. Current D: release 3d1932b68b2c0f15 / PID17928 is being used for diagnosis only; it does not inherit frozen-source suite proof. The UI/save discrepancy remains unresolved; no user misclick or UI defect is assumed.

### Diagnostic owner acceptance

Diagnostic D: build 3d1932b68b2c0f15 recorded an owner-verified captured turn at 2026-10-25T12:44:35Z (similarity 0.944), after native Voice Lock enable. Intended Open Notepad was transcribed as open over open load pad; requested app-open success is not established. This is one owner-verification acceptance, not broad accuracy or frozen-source proof. Owner is alone; owner trials can continue, other-speaker playback must be labelled separately, and live non-owner evidence remains unavailable. Score stays 41%.

### Solo owner batch

Owner reported ten solo attempts (five normal, five about one metre). In 20:07:00–20:09:00 IST, seven completed captures are identifiable, all owner=verified; three reported attempts remain unmatched and condition-by-condition mapping is unavailable. Do not claim 10/10 or a formal owner acceptance rate. One capture was repeat-loop filtered, several app targets were duplicated, and one transcript became hook up opencode. Requested app-opening success is not established for the batch. D: build3d1932 diagnosis only; zero negative trials; score stays41%.

## P2 native settings readback

**2026-10-26 owner follow-up:** requested Notepad action failed/was absent after shipping settings checks. Two logged speech captures passed owner verification, but transcript duplication/repeat filtering and a later stage blackout remain unresolved. Inherited warn filtering hid lifecycle audit, so no clean one-trial accounting/rate. Same assessed binary restarted as PID4564 with explicit audit logging; a fresh exactly-once attempt is pending. [Evidence](runs/windows1-20261026-owner-001-result.json). The settings subtest pass does not close this card.

Owner authorized implementing P2 while P1 release/live reconciliation remains pending. [Change record and live settings checklist](../changes/164-voice-lock-native-state-ui.md). Both windows distinguish drafts from effective native state and verify Save. This does not establish speaker accuracy or complete either phase; score remains 41%.

### Repaired shipping settings device result — 2026-10-26 IST

Candidate `0b10fd3a1416a540` / revision `3e5b4a8`, PID12064, passes shared gates, provenance and real shipping settings checks: draft/native separation, Save Off/On/reopen, accessible acknowledgement, bounded standalone open, cross-window refresh and explicit-draft preservation. Confirmed simulations cover false acknowledgement, refusal, delay and profile-status error/recovery. The initial ineffective invoke wrapper is excluded; actual profile recovery and the separate legacy prototype remain untested live. Original lock On and five-clip profile restored, interceptors removed, wake resumed. [Assessment](runs/windows1-20261025-voice-lock-ui-v2-assessment.md). No new owner/negative trials counted; score stays41%, queue stays on this card. One fresh owner Ctrl+Space attempt requested on this exact instance.

## Measured decoder failure and native execution repair —2026-10-26

One consented diagnostic clip reproduces owner-vocabulary repetition under same-audio comparison; neutral decoding returns one command in both replays. The native microphone path also skipped the legacy frontend executor while Rust only emitted Done. Fixes are frozen atf0988d9/inputd7b49eeb; regression failed before fix,7 new mutations are killed, full library2,537 passes. Integration, dead-code and optimized release gates subsequently passed on that frozen source; repaired owner action proof remains pending. The old candidate was stopped with diagnostics Off/profile intact. Stage heartbeat and all remaining owner/non-owner/replay/recovery/enrollment/capture gates stay open. [Change167](../changes/167-neutral-decoding-and-native-local-execution.md), [current assessment](runs/windows1-20261026-neutral-local-assessment.json). Score41%; no queue advance.
