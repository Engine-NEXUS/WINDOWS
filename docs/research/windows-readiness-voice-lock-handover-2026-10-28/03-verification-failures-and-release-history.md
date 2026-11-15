# 03 — Verification, failures, source identities and releases

[Handover index](README.md) · [Research/plan](01-research-decisions-and-plan.md) · [Implementation](02-implementation-and-file-inventory.md) · [Remaining work](04-pending-work-and-maintenance.md)

Prepared 2026-10-28. All functional results below are historical observations from their cited runs. This documentation task did not repeat those tests, operate NEXUS or verify a newly built executable.

## 1. Why source attribution was necessary

The initial primary checkout was on `feat/action-kernel`, HEAD `fd89438fef88dc8270998048d00d1d73baab8e27`, with substantial uncommitted work. A clean HEAD label could not identify the actual tested inputs.

Concurrent mail edits changed the source fingerprint during the first assessment. They were preserved, not reverted. Final shared checks were moved to a retained snapshot in the managed readiness worktree. Later P1/P2 combinations and decoder repairs received their own snapshot/hash/build identities. The main branch, primary index and unrelated work were preserved.

A verified manifest must link source inputs, frontend bundle and executable. An executable's own declared manifest identity alone is weaker than independent provenance verification. A later source change does not inherit an earlier release's live proof.

## 2. Assessment and release timeline

| Stage | Source identity | Release / outcome | Associated final test scope |
|---|---|---|---|
| Baseline/profile safety | `954c57471be0564f69763f73b43e0868f50c74ca` | `812a5b3333669e02`; provenance passed, enrollment later failed live | Rust 2522 library+19 integration; frontend 647; Worker 145; Python 48; shared gates and12 mutations |
| Enrollment race follow-up | `ade0bfdddd99d52efe14da089d33ee5b7493d8a0` | Superseded during cancellation/wake-race review; not the final live candidate | Intermediate frozen checks; do not substitute for the final revised source |
| Enrollment v2 | `50dcda219018d3bbfff8be388b77610bee135749` | `031caf5848621e18`; built49m15s, provenance passed | Rust 2524+19; frontend 656; Worker 145; Python 48;15 mutations and shared guards |
| P1 accounting | `c430a21c9000e00be131478a7a718f53b5f23baf` | P1-only optimized build later superseded by combined work | Rust 2532+19; frontend 656; Worker 145; Python 60;23 audit/speaker/evidence-reader mutations |
| First combined P1/P2 candidate | `9c3f0320a5c7df359c6a9769e8492ed48baf8fcd` | `021cba2bac4e4bed`; built42m51s; failed live settings verification | Rust 2532+19; frozen frontend 678; Worker 145; Python 60; full guards |
| Repaired P2 candidate | `3e5b4a803b2a22bc28986189377942f127234fa7` | `0b10fd3a1416a540`; built61m37s; shipping settings device checks passed; requested owner action failed | Rust 2533+19; frontend 687; Worker 145; Python 60;33 mutations |
| Neutral decoding/native execution repair | `f0988d95afbfac06176c923c2227e7e79e841013` | `d846dd346c470b60`; built46m14s; provenance passed; repaired owner action untested | Rust 2537+19; frontend 687; Worker 145; Python 60;40 mutations;148 known dead-code findings, none new |
| P3 scalar tooling | D: source-only files; hashes in its assessment | No application build or live operation |47 script tests;8 offline mutations |
| P3 embedding tooling | D: source-only files; hashes in its assessment | No model inference/application build/live operation |63 script tests;18 offline mutations |
| P4/P5/P6 source checkpoint | Dirty D: fingerprint `6def3963dc8380f45294c0c1ab9b373727090def2f00eb842278ae5da78077f4` | No new release; all required live/full-exact-source gates remain open |90 script+31 server tests; frontend 692; Worker 145;79 combined mutations; no current full application Rust/dead-code build gate rerun |

The full Rust snapshots above retained 38 explicitly ignored tests, rather than pretending ignored tests passed. The final decoder repair had2537 passing library tests with 31 ignored, plus 19 integration passes with 7 ignored. Distinguish passing and ignored counts.

Primary-tree frontend counts sometimes exceeded frozen counts because concurrent feature tests were excluded from the frozen assessment. For example, a primary 682 count did not change a frozen 678 result. Counts across different snapshots must not be summed or used interchangeably.

### Key input fingerprints

| Snapshot | Build-input SHA256 |
|---|---|
| Baseline954c574 | `3087f61760442b846c71baaba710b625cf5ebe961e81fa5d4677c381e150ce5e` |
| Enrollment50dcda2 | `086e534ae829a7abbdf1e793f584297e81b8f2c8c16625bcfeee03ea505572c4` |
| P1 accountingc430a21 | `ca2daa4bff07a9b6f7dd6b3b797cc09dc5bee188e573fd74d769b2e2ffbe9cba` |
| Repaired P23e5b4a8 | `342c6f390461e0c69d0c6aa6b66536d84d9cd2c16423f0f8e35604d206dc9124` |
| Neutral/localf0988d9 | `d7b49eebf14f49913b36ec7a125e59195915e41d4950a1ac703190b84c1e475a` |

Source records: [baseline](../../testing/runs/windows1-20261025-assessment.md), [enrollment v2](../../testing/runs/windows1-20261025-enrollment-v2-assessment.md), [P2 frozen](../../testing/runs/windows1-20261025-voice-lock-ui-frozen-assessment.json), [P2 replacement](../../testing/runs/windows1-20261025-voice-lock-ui-v2-assessment.json), [neutral/local](../../testing/runs/windows1-20261026-neutral-local-assessment.json), [source-only checkpoint](../../testing/runs/windows1-20261026-phase-closeout-assessment.json).

## 3. Enrollment reports and attribution correction

The first release's enrollment-success UI did not settle the interaction: the owner reported that saying the displayed phrase also woke NEXUS. That was a real failed interaction, even though enrollment produced a profile.

After the isolation changes, the owner reported:

- All ten clips completed without normal wake cues/actions.
- Nexus, Hey Nexus and OK Nexus woke normally after enrollment.
- Navigation during countdown and during recording stopped enrollment and restored waking.

These reports remain valid observations. Later supplied console logs showed that `nexus start` had switched the running app to D: releases `6f4e1063a38683a3` and then `3d1932b68b2c0f15`. Their precise attribution to the previously assessed candidate was therefore not secure. Exact-candidate interaction checks must be repeated. Pending browser-permission cancellation was covered by regression but not independently live-proven.

The logged Notepad turn at 17:37:41 on the D: diagnostic build was owner=unenrolled with Voice Lock off. It is excluded from locked accuracy counts. No formal rejection/acceptance percentage was inferred.

## 4. Native state versus owner reports

The assistant originally omitted Save from the toggle instructions. That attempt was not a qualified locked trial. A later owner report said Saved successfully, but native routing and saved-setting queries both returnedfalse. The cause was not assumed to be user error or a proved UI defect.

Under then-existing authorization for controlled trials, an explicit native patch enabled the lock and both readbacks returnedtrue. This demonstrated configuration at that time, not speaker accuracy. The later effective-state/verified-Save implementation addressed the ambiguity systematically.

## 5. Owner trial observations before accounting was complete

On the D: diagnostic build, one intended Notepad command was owner-verified around 0.944 but transcribed incorrectly. Requested app-opening success was not established.

The later owner batch reported ten attempts, five near and five approximately one metre away. Seven completed captures were identifiable, all verified, but three attempts could not be mapped. Condition-by-condition attribution was unavailable. Repetition/filtering and incorrect command targets remained. The work explicitly did not claim10/10 or a formal owner acceptance rate.

This failure motivated unique request/session accounting. A future batch must reconcile physical starts, native requests, accepted captures, terminal outcomes, routing receipt and independently observed actions.

## 6. P2 live failure and replacement settings proof

### First combined candidate: failed

Real UI Automation/native checks on release 021cba2bac4e4bed confirmed draft isolation, native Save Off/On and keyboard reopen. However:

1. The accessibility tree exposed Saved before any Save because visual opacity alone hid it.
2. A synchronous settings-window open did not complete; the process became unresponsive and a stage blackout was logged.

The lock/profile was preserved. No owner accuracy trial was counted. The failed executable was stopped at that historical point after confirming its path; that operation preceded the later owner-only runtime restriction.

Native capture-exclusion policy meant screen captures did not reliably show the target window. UIA/native evidence was therefore separated from browser-rendered visual proof. The work did not disable window capture exclusion merely to obtain a screenshot.

### Replacement: scoped shipping settings pass

Release 0b10fd3a1416a540 passed provenance before/after its real Windows settings checks:

- Draft changes did not apply before Save.
- Save Off/On/reopen matched native and saved state.
- No premature accessible Save acknowledgement.
- Cross-window untouched-draft synchronization and preservation of explicit edits.
- Standalone shipping settings opened responsively in 640 ms for that test.
- Confirmed fetch-intercept simulations exercised false acknowledgement, refused/delayed write and profile-status error/recovery.

The first interception approach tried replacing immutable Tauri invoke and did not intercept the calls. Those observations are excluded from fault-injection proof. The later fetch-level interceptions were confirmed before being counted.

The standalone native settings window defaults to Command Hub. The separate `?view=legacy` prototype was not independently device-tested. Simulated status errors do not prove actual corrupt/deleted-profile recovery. Settings proof did not establish a successful voice command.

## 7. Owner action failure after P2 settings proof

The owner reported rejected/nothing for the requested Notepad action. The first run inherited `RUST_LOG=warn`, suppressing INFO capture-audit records. This was a launch/protocol mistake, not a microphone diagnosis. The ambiguous multi-capture report was excluded from a formal lifecycle/rate calculation.

Logs showed owner verification around 0.942, command repetition, repeat-loop filtering and a later stage blackout. Because the same report could not be mapped cleanly to exactly one captured attempt, the correct conclusion was “requested action failed or unconfirmed,” not “owner verifier rejected the owner.”

The same verified binary was historically restarted with explicit audit logging. One subsequent hotkey turn was mapped to run `4564-1792953451045269100`, attempt 2/session 3. Owner verification succeeded at 0.960, but the transcript became a repeated/wrong Notepad target. Transcript delivery and dispatch receipt were established; successful requested action was not.

A selected one-attempt log slice reconciled that attempt only. It was not a full-process one-start proof or the ten-start batch. Extra automatic captures and later commands stayed outside the controlled-trial denominator.

See [owner result001](../../testing/runs/windows1-20261026-owner-001-result.json), [owner result002](../../testing/runs/windows1-20261026-owner-002-result.json) and [selected audit](../../testing/runs/windows1-20261026-owner-002-audit.json).

## 8. One consented diagnostic clip

The owner authorized exactly one encrypted local diagnostic clip. A one-shot listener disabled recording after the first completed capture and had a 90-second timeout fallback. Native readback confirmed diagnostics Off. Consent was not generalized to later recordings.

Recorded sample metadata:

| Property | Historical value |
|---|---|
| Capture session |6|
| Duration / samples |2880ms /46080 samples|
| Owner similarity |approximately 0.954|
| Original live filter |repeat_loop|
| Clipping fraction under recorded threshold |0.0|
| Sealed-file SHA256 |`8bf8bc28cfca965481e1d2ff6d6ea7408ad0b9ca017359fc81deca0f05ecabb0`|
| PCM SHA256 |`b32fa90dad6518c4d86c482f1b4ea009b7beee2a5f9615e2c81aa8c309ee1c8a`|

The controlled replay kept PCM, model, English language and temperature 0 fixed; only prompt condition changed:

| Prompt | Replay1 | Replay2 |
|---|---|---|
| Neutral |Open note pad.|Open note pad.|
| Owner command vocabulary |Open note pad. Open note pad.|Open note pad. Open note pad.|

Two initial replay HTTP requests failed with403 using urllib's default user agent. A named diagnostic user agent succeeded. No plaintext audio or secret key was printed or persisted by the helper.

This reproduces prompt contribution for one clip. It is not held-out accuracy, an acoustic-echo diagnosis, calibration, a replay-resistance result or a population identity measurement. The sample was subject to the app's then-existing 24-hour retention. Its existence/retention state was not checked for this handover; do not assume it is still present or reopen it automatically.

See [analysis](../../testing/runs/windows1-20261026-one-clip-analysis.json), [one-shot disable record](../../testing/runs/windows1-20261026-one-clip-disabled.json) and [final diagnostic status](../../testing/runs/windows1-20261026-one-clip-status.json).

## 9. Last repaired release and runtime handoff

The repaired frozen release `d846dd346c470b60` completed in46m14s from source f0988d9. Its recorded executable SHA256 is `e7210e43bdbaa737b7b7786ed2a16e3c5427272740a921be7f72c3491dc46007`. Provenance passed.

Before the owner-only restriction, startup reached stage ready. Native readback showed a valid five-clip profile and diagnostics Off. Voice Lock initially read Off, then was enabled under prior controlled-trial authorization. No repaired owner action trial completed before interruption.

The owner then required edits only and manual build/runtime control. The last observed state is historical; no current PID, protection state, diagnostic retention or restored setting is inferred. No later source-only tool phase rebuilt or operated the app.

## 10. Mutation coverage catalogue

The final source checkpoint recorded 79 mutations:40 native/UI,18 offline benchmark/embedding and21 trial-accounting mutations. These are safeguard checks on specific source copies, not79 live security trials.

### Native/UI groups:40

| Group | Count | Representative safeguards |
|---|---:|---|
| Speaker/enrollment |15|Locked owner gate, corrupt settings, unrelated Save preservation, missing/stale/corrupt profile, validation, dedicated key, deletion failure, sealed storage/authentication, calibration, confirmation gate, pause/resume and pending-permission cancellation|
| Effective-state UI |11|Verified requested state, profile requirement, overlapping saves, stale refresh, native disagreement, in-flight drafts, external refresh, old acknowledgement and accessible Saved|
| Decoder/native dispatch |7|Owner vocabulary policy, neutral prompt, cancellation/legacy guard, actual execution and source call-path wiring|
| Capture audit |6|Unique attempts/sessions, terminal deduplication, abandoned packets, executable linkage and stale work|
| Async opener |1|A synchronous window opener violates the Future contract|

The later native decoder API has evolved; see document04's current-source maintenance warning before assuming the saved harness still runs unchanged.

### Offline benchmark/embedding:18

- Remove split isolation; tune using acceptance; admit impostor calibration ties; allow incomplete classes or unpaired models.
- Relabel owner/non-owner identity; change the acceptance boundary; remove the full-input fingerprint.
- Restore overflowing median arithmetic.
- Allow enrollment-group leakage, impostor enrollment, fewer required samples, multiple owners or duplicate enrollment samples.
- Put calibration/acceptance vectors in the enrollment template; replace maximum cosine; remove robust normalization; leak vectors into reports.

### Trial accounting:21

- Let legacy evidence qualify; accept padded headers; silently drop blank records; merge negative classes incorrectly.
- Qualify lock-off, captureless, unreached/unavailable verification, invalid-command or non-acceptance rows.
- Treat playback as live unfamiliar proof; accept299 negatives or90% owner acceptance.
- Hide excluded locked actions; remove negative/unverified action gates.
- Allow duplicate captures or partition leakage; accept inconsistent action-success claims.
- Promote declared metadata to ready; leak private trial rows into reports.

Each accepted mutation run required a healthy baseline and an assertion failure for the mutated source, not a syntax/import failure. The trial mutation helper first rejected a stale anchor count; that rejected run was corrected and the complete 21-mutation pass then recorded.

## 11. OpenCode GLM delegation and actual credit

The owner chose `opencode/glm-5.3-flash-free` for coding, with the supervisor planning/testing and allowed to edit if the delegated implementation failed.

| Task | Observed delegation result | Actual implementation credit |
|---|---|---|
| Scalar benchmark |Model listed/started; no response/files after about five minutes|Authorized supervisor fallback|
| Embedding scorer, main repo |Repeated context compaction while loading roughly 405 KB of instructions; no edits|Request interrupted; moved to isolated directory|
| Embedding scorer, isolated retry |Read brief/reference and exited0 without requested files|Authorized supervisor fallback|
| Legacy CSV repair |Wrote both files; nine tests passed|GLM patch accepted after review|
| Legacy review |Three additional regressions failed: header padding, blank records, per-kind conditions|Supervisor narrow fixes;12 legacy tests passed|
| Richer trial reporter |Read larger brief; stalled without files|Only that request interrupted; authorized supervisor fallback and18 tests|

The successful legacy session was `ses_e86e34b13ffeLmbFH7bfxPgKnp`. Earlier failed sessions and brief locations are in changes168–170. Temporary editor directories are implementation workspaces, not durable release evidence. No unrelated OpenCode task or NEXUS process was stopped by these source-only delegations.

## 12. Failures and qualifications that must stay visible

| Event | Disposition |
|---|---|
| Windows pagefile error1455 during parallel native compilation |Retained failure; later native work used one Cargo job|
| Smart Turn timing failure under contention |Retained; original 5 s budget separately passed in isolation, not relaxed to hide it|
| Concurrent primary-tree compile/source changes |Preserved; frozen worktree assessments used for attribution|
| First enrollment waking NEXUS |Failed interaction; replacement required|
| Pending permission blocking enrollment cancellation |New failing regression, then fixed; live case remains separately open|
| Early Saved accessibility text |Live failure, then repaired and scoped device proof|
| Synchronous settings-window hang/blackout |Live failure, async repair; broader stage health still open|
| Ineffective Tauri invoke-wrapper fault injection |Excluded; confirmed fetch interception used later|
| Wrong running build / unsaved or native-Off lock |Excluded from locked accuracy counts|
| Suppressed INFO audit due to inherited logging |No formal capture reconciliation from that run|
| Verified owner but wrong/unexecuted command |Retained as fidelity/action failure, not automatically biometric rejection|
| Failed default-user-agent replay requests |Recorded before successful controlled comparison|
| Benchmark median overflow |New failing test before narrow repair|
| Wrong server pytest working directory |No tests ran; corrected server run31 passed|
| Mutation anchor expected2 occurrences instead of3 |Rejected helper run; corrected complete pass|
| Empty/stalled GLM outputs |No code credit or fabricated review; fallback documented|

## 13. Current versus later reported project totals

The 2026-10-26 source checkpoint recorded 90 script+31 server tests, frontend 692 and Worker 145. Separate2026-10-27 project notes and the owner live checklist report Rust 2635 library+20 integration, frontend 697, Worker 145, Python 90 and a dead-code baseline147. Those are later project reports, not fresh tests performed for this documentation request and not retroactive proof for the older snapshots.

The updated project notes also contain intermediate deploy-work totals with different counts. Preserve run/source attribution rather than selecting the largest number. The generic testing index's older522/20/49 matrix is historical; it must not be confused with current acceptance evidence.

The feature remains41%, with no qualified new negative corpus or successful repaired exact-release owner action added by this handover.
